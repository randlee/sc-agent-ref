//! Domain newtypes and [`BackendId`].
//!
//! The backend catalog ([`BACKENDS`], one row of facts per id, REQ-SAG-0105)
//! lives in the private `catalog` module. [`BackendId::descriptor`] holds the
//! only exhaustive `match` over [`BackendId`]; every other read goes through
//! the descriptor.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ulid::Ulid;

use crate::catalog::{
    ANTHROPIC_HAIKU, BACKENDS, BackendDescriptor, BackendFamily, JEV, OPENAI_LUNA,
};
use crate::error::JevError;

/// Git toplevel used for path resolution.
///
/// Callers obtain this from `git rev-parse --show-toplevel` (or
/// `git::discover_toplevel` with the `git` feature).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRoot(PathBuf);

impl RepoRoot {
    /// Wraps a git toplevel path.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Returns the toplevel path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for RepoRoot {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

/// Git revision, or the literal `WORKTREE`, for `git show` inputs.
///
/// `Deserialize` goes through [`Rev::parse`], so a document cannot carry a
/// revision git would read as an option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rev(String);

impl TryFrom<String> for Rev {
    type Error = JevError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

impl TryFrom<&str> for Rev {
    type Error = JevError;

    /// # Errors
    ///
    /// `INPUT_INVALID` as for [`Rev::parse`].
    fn try_from(text: &str) -> Result<Self, Self::Error> {
        Self::parse(text)
    }
}

impl FromStr for Rev {
    type Err = JevError;

    /// # Errors
    ///
    /// `INPUT_INVALID` as for [`Rev::parse`].
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl AsRef<str> for Rev {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<Rev> for String {
    fn from(rev: Rev) -> Self {
        rev.0
    }
}

impl Rev {
    /// Revision string meaning the working tree rather than a commit.
    pub const WORKTREE: &'static str = "WORKTREE";

    /// Parses revision text, the one way to build a revision.
    ///
    /// A revision is passed to git as an argument, so there is no unchecked
    /// constructor: CLI flags, Python arguments, `check.toml`, and a SHA read
    /// back from git all go through this check.
    ///
    /// Accepts [`Self::WORKTREE`] or any text git may resolve. Rejects text
    /// that git would read as an option or that cannot be one argv element:
    /// empty text, a leading `-`, NUL, or any whitespace. This check does not
    /// run git, so a well-formed name can still fail to resolve later.
    ///
    /// # Errors
    ///
    /// Returns [`JevError::InputInvalid`] when the text is empty, starts with
    /// `-`, or contains NUL or whitespace.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::Rev;
    ///
    /// assert_eq!(Rev::parse("HEAD~1").unwrap().as_str(), "HEAD~1");
    /// assert_eq!(Rev::parse("WORKTREE").unwrap(), Rev::worktree());
    /// assert_eq!(Rev::parse("--output=x").unwrap_err().code(), "INPUT_INVALID");
    /// ```
    pub fn parse(text: &str) -> Result<Self, JevError> {
        let well_formed = !text.is_empty()
            && !text.starts_with('-')
            && !text.chars().any(|ch| ch == '\0' || ch.is_whitespace());
        if well_formed {
            Ok(Self(text.to_owned()))
        } else {
            Err(JevError::input_invalid(
                "revision must be WORKTREE or a git revision that is non-empty, does not start with '-', and has no whitespace or NUL",
            ))
        }
    }

    /// A revision that skips [`Self::parse`], to prove the guards that sit
    /// behind it. Compiled only for this crate's unit tests of the `git` feature.
    #[cfg(all(test, feature = "git"))]
    pub(crate) fn unchecked(rev: impl Into<String>) -> Self {
        Self(rev.into())
    }

    /// Returns the working-tree sentinel.
    #[must_use]
    pub fn worktree() -> Self {
        Self(Self::WORKTREE.to_owned())
    }

    /// Whether this is the [`Self::WORKTREE`] sentinel rather than a git
    /// revision.
    ///
    /// ```
    /// use sc_agent_types::Rev;
    ///
    /// assert!(Rev::worktree().is_worktree());
    /// assert!(!Rev::parse("HEAD").unwrap().is_worktree());
    /// ```
    #[must_use]
    pub fn is_worktree(&self) -> bool {
        self.0 == Self::WORKTREE
    }

    /// Returns the revision text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Rev {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Session correlation id: a ULID (26 Crockford base32 characters).
///
/// `Display`, `FromStr` and serde all use the 26-character text form. The id
/// carries a millisecond clock, so session file names sort by creation time.
///
/// # Examples
///
/// ```
/// use sc_agent_types::SessionId;
///
/// let id = SessionId::new();
/// assert_eq!(id.to_string().len(), 26);
/// assert_eq!(id.to_string().parse::<SessionId>().unwrap(), id);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(Ulid);

impl SessionId {
    /// Creates a new session id (`Ulid::new()`; no shared generator state).
    ///
    /// `SessionId` does not implement [`Default`]: a default value that is a
    /// fresh random id would surprise callers that expect `default()` to be
    /// deterministic.
    #[must_use]
    #[expect(
        clippy::new_without_default,
        reason = "a random id is not a sensible Default value"
    )]
    pub fn new() -> Self {
        Self(Ulid::new())
    }
}

impl FromStr for SessionId {
    type Err = JevError;

    /// Parses a 26-character ULID.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` for anything else, a UUID included; the message says
    /// session ids are ULIDs since Phase A.1.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::SessionId;
    ///
    /// let id: SessionId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
    /// assert_eq!(id.to_string(), "01ARZ3NDEKTSV4RRFFQ69G5FAV");
    /// let uuid = "00000000-0000-0000-0000-000000000000";
    /// assert_eq!(uuid.parse::<SessionId>().unwrap_err().code(), "INPUT_INVALID");
    /// ```
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_ulid(text).map(Self).ok_or_else(|| {
            JevError::input_invalid(format!(
                "session id {text:?} is not a ULID; session ids are ULIDs since Phase A.1"
            ))
        })
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

impl Serialize for SessionId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for SessionId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Backend credential id consumed by `sc-agent-secrets`.
///
/// The documented spelling is the [`Display`](fmt::Display) form
/// (`typesafe`, `anthropic`, `openai`). Serde writes that form and reads it
/// through [`FromStr`], which also accepts the one-release alias `open_ai`
/// (bead `agt-ho2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BackendKeyId {
    /// TypeSafe Jev API credential.
    TypeSafe,
    /// Anthropic API credential.
    Anthropic,
    /// OpenAI API credential.
    OpenAi,
}

impl BackendKeyId {
    /// Every credential id, in declaration order.
    pub const ALL: &'static [Self] = &[Self::TypeSafe, Self::Anthropic, Self::OpenAi];

    /// The documented spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypeSafe => "typesafe",
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
        }
    }

    /// Read aliases accepted beside the documented spelling.
    ///
    /// The match is exhaustive, so a new id must decide its aliases.
    const fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::TypeSafe | Self::Anthropic => &[],
            Self::OpenAi => &["open_ai"],
        }
    }
}

impl fmt::Display for BackendKeyId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for BackendKeyId {
    type Err = JevError;

    /// Parses the documented id or its read alias (`open_ai`).
    ///
    /// Kebab-case and other spellings are rejected.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .find(|id| id.as_str() == text || id.aliases().contains(&text))
            .copied()
            .ok_or_else(|| {
                JevError::input_invalid(format!(
                    "backend key {text} is not typesafe, anthropic, or openai"
                ))
            })
    }
}

impl Serialize for BackendKeyId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for BackendKeyId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Judge implementation id.
///
/// The documented spelling is [`BackendDescriptor::name`]. Serde writes it;
/// `Deserialize` goes through [`FromStr`], which also accepts the
/// descriptor's [`BackendDescriptor::aliases`] (one alias source; there is no
/// `#[serde(alias)]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BackendId {
    /// TypeSafe Jev.
    Jev,
    /// Anthropic Haiku, API-direct.
    AnthropicHaiku,
    /// OpenAI Luna, API-direct.
    OpenAiLuna,
}

impl BackendId {
    /// Returns this backend's row of [`BACKENDS`].
    ///
    /// This is the one exhaustive `match` over `BackendId` in the workspace.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::{BackendFamily, BackendId, BackendKeyId};
    ///
    /// let luna = BackendId::OpenAiLuna.descriptor();
    /// assert_eq!(luna.name(), "openai_luna");
    /// assert_eq!(luna.key_id(), Some(BackendKeyId::OpenAi));
    /// assert_eq!(luna.family(), BackendFamily::Llm);
    /// ```
    #[must_use]
    pub const fn descriptor(self) -> &'static BackendDescriptor {
        match self {
            Self::Jev => &JEV,
            Self::AnthropicHaiku => &ANTHROPIC_HAIKU,
            Self::OpenAiLuna => &OPENAI_LUNA,
        }
    }

    /// The documented spelling ([`BackendDescriptor::name`]).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.descriptor().name()
    }

    /// The credential this backend sends ([`BackendDescriptor::key_id`]).
    #[must_use]
    pub const fn key_id(self) -> Option<BackendKeyId> {
        self.descriptor().key_id()
    }

    /// The backend family ([`BackendDescriptor::family`]).
    #[must_use]
    pub const fn family(self) -> BackendFamily {
        self.descriptor().family()
    }
}

impl FromStr for BackendId {
    type Err = JevError;

    /// Parses a descriptor name or alias.
    ///
    /// `jev`, `anthropic_haiku`, and `openai_luna` are the documented ids;
    /// `open_ai_luna` is a one-release read alias. Kebab-case spellings such
    /// as `anthropic-haiku` are rejected.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::BackendId;
    ///
    /// assert_eq!("openai_luna".parse::<BackendId>().unwrap(), BackendId::OpenAiLuna);
    /// assert_eq!("open_ai_luna".parse::<BackendId>().unwrap(), BackendId::OpenAiLuna);
    /// assert_eq!("openai-luna".parse::<BackendId>().unwrap_err().code(), "INPUT_INVALID");
    /// ```
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        BACKENDS
            .iter()
            .find(|row| row.name() == text || row.aliases().contains(&text))
            .map(BackendDescriptor::id)
            .ok_or_else(|| {
                JevError::input_invalid(format!(
                    "backend {text} is not jev, anthropic_haiku, or openai_luna"
                ))
            })
    }
}

impl fmt::Display for BackendId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for BackendId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for BackendId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// A non-empty, duplicate-free list of backends, in request order.
///
/// # Examples
///
/// ```
/// use sc_agent_types::{BackendId, BackendSet};
///
/// let set: BackendSet = "jev,anthropic_haiku".parse().unwrap();
/// assert_eq!(set.as_slice(), [BackendId::Jev, BackendId::AnthropicHaiku]);
/// assert_eq!("jev,jev".parse::<BackendSet>().unwrap_err().code(), "CONFIG_INVALID");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendSet(Vec<BackendId>);

impl BackendSet {
    /// The backends, in request order.
    #[must_use]
    pub fn as_slice(&self) -> &[BackendId] {
        &self.0
    }

    /// Iterates the backends in request order.
    pub fn iter(&self) -> impl Iterator<Item = BackendId> + '_ {
        self.0.iter().copied()
    }

    /// Number of backends (never zero).
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always false: a `BackendSet` has at least one backend.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<BackendId> for BackendSet {
    /// A set of exactly `id`: one backend, so no validation can fail.
    ///
    /// ```
    /// use sc_agent_types::{BackendId, BackendSet};
    ///
    /// assert_eq!(BackendSet::from(BackendId::AnthropicHaiku).as_slice(), [BackendId::AnthropicHaiku]);
    /// ```
    fn from(id: BackendId) -> Self {
        Self(vec![id])
    }
}

impl Default for BackendSet {
    /// The default selection, `[jev]`: one built-in backend, so no
    /// validation can fail.
    ///
    /// ```
    /// use sc_agent_types::{BackendId, BackendSet};
    ///
    /// assert_eq!(BackendSet::default().as_slice(), [BackendId::Jev]);
    /// ```
    fn default() -> Self {
        Self(vec![BackendId::Jev])
    }
}

impl TryFrom<Vec<BackendId>> for BackendSet {
    type Error = JevError;

    /// # Errors
    ///
    /// `CONFIG_INVALID` when the list is empty or repeats a backend.
    fn try_from(ids: Vec<BackendId>) -> Result<Self, Self::Error> {
        if ids.is_empty() {
            return Err(JevError::config_invalid(
                "the backend list is empty; name at least one of jev, anthropic_haiku, or openai_luna",
            ));
        }
        let mut seen = BTreeSet::new();
        for id in &ids {
            if !seen.insert(*id) {
                return Err(JevError::config_invalid(format!(
                    "backend {id} is listed more than once"
                )));
            }
        }
        Ok(Self(ids))
    }
}

impl FromStr for BackendSet {
    type Err = JevError;

    /// Parses a comma-separated list (`jev,anthropic_haiku`).
    ///
    /// Surrounding whitespace on each entry is ignored.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let ids = text
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::parse)
            .collect::<Result<Vec<BackendId>, JevError>>()?;
        Self::try_from(ids)
    }
}

impl AsRef<[BackendId]> for BackendSet {
    fn as_ref(&self) -> &[BackendId] {
        self.as_slice()
    }
}

/// Lowercase hex SHA-256 (64 characters).
///
/// Used for canonical payload digests and equal-input `prompt_sha256`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256Digest(String);

impl Sha256Digest {
    /// Builds a digest from 32 raw SHA-256 bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(encode_hex(&bytes))
    }

    /// SHA-256 of `text` through `sc-sha`'s text identity.
    ///
    /// CRLF and bare CR are normalized to LF before hashing, so the digest
    /// equals SHA-256 of the bytes only for text without CR. Compact JSON
    /// (every request body and canonical payload) has no raw CR.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::Sha256Digest;
    ///
    /// assert_eq!(
    ///     Sha256Digest::of_text("").as_str(),
    ///     "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    /// );
    /// assert_eq!(Sha256Digest::of_text("a\r\nb"), Sha256Digest::of_text("a\nb"));
    /// ```
    #[must_use]
    pub fn of_text(text: &str) -> Self {
        let input = sc_sha::HashInput::TextFileBytes {
            utf8_file_bytes: text.as_bytes(),
        };
        match sc_sha::calculate_hash(input) {
            Ok(result) => Self(result.template().to_hex()),
            // A `&str` is valid UTF-8, the only error `calculate_hash` has.
            Err(error) => unreachable!("a &str is valid UTF-8: {error}"),
        }
    }

    /// Parses 64 lowercase hex characters.
    ///
    /// # Errors
    ///
    /// Returns [`JevError::InputInvalid`] when the text is not 64 lowercase
    /// hex characters.
    pub fn parse(text: impl AsRef<str>) -> Result<Self, JevError> {
        let text = text.as_ref();
        if text.len() == 64 && is_lower_hex(text) {
            Ok(Self(text.to_owned()))
        } else {
            Err(JevError::input_invalid(
                "SHA-256 digest must be 64 lowercase hex characters",
            ))
        }
    }

    /// Returns the lowercase hex text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for Sha256Digest {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<Sha256Digest> for String {
    fn from(value: Sha256Digest) -> Self {
        value.0
    }
}

impl TryFrom<String> for Sha256Digest {
    type Error = JevError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

/// Git object id of a blob: 40 (SHA-1) or 64 (SHA-256) lowercase hex digits.
///
/// # Examples
///
/// ```
/// use sc_agent_types::BlobSha;
///
/// assert!("a".repeat(40).parse::<BlobSha>().is_ok());
/// assert!("A".repeat(40).parse::<BlobSha>().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BlobSha(String);

impl BlobSha {
    /// Returns the lowercase hex text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for BlobSha {
    type Error = JevError;

    /// # Errors
    ///
    /// `INPUT_INVALID` unless the text is 40 or 64 lowercase hex digits.
    fn try_from(text: &str) -> Result<Self, Self::Error> {
        if matches!(text.len(), 40 | 64) && is_lower_hex(text) {
            Ok(Self(text.to_owned()))
        } else {
            Err(JevError::input_invalid(format!(
                "blob sha {text:?} is not 40 or 64 lowercase hex digits"
            )))
        }
    }
}

impl TryFrom<String> for BlobSha {
    type Error = JevError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::try_from(text.as_str())
    }
}

impl FromStr for BlobSha {
    type Err = JevError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::try_from(text)
    }
}

impl AsRef<str> for BlobSha {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<BlobSha> for String {
    fn from(value: BlobSha) -> Self {
        value.0
    }
}

impl fmt::Display for BlobSha {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A path relative to the repository root, with `/` separators.
///
/// Validated repo-relative path: non-empty, not absolute, at least one
/// normal component, no `..`, and no `\`, NUL, or `:`. The text is kept as
/// written. It guards `sc-jev-check`'s file reads and `check.toml` path
/// fields, subjects documents, the `git` helpers, and `sc-treesitter`'s CLI
/// path handling.
///
/// # Examples
///
/// ```
/// use sc_agent_types::RepoRelativePath;
///
/// assert_eq!(RepoRelativePath::try_from("src/a.py").unwrap().as_str(), "src/a.py");
/// for bad in ["", "/etc/passwd", "../x", "a\\b", "c:x"] {
///     assert_eq!(RepoRelativePath::try_from(bad).unwrap_err().code(), "INPUT_INVALID");
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepoRelativePath(String);

impl RepoRelativePath {
    /// Returns the path text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the path as a [`Path`].
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl TryFrom<&str> for RepoRelativePath {
    type Error = JevError;

    /// # Errors
    ///
    /// `INPUT_INVALID` for empty text, an absolute path, a `..` component, or
    /// a `\`, NUL, or `:` character.
    fn try_from(text: &str) -> Result<Self, Self::Error> {
        let path = Path::new(text);
        let well_formed = !text.is_empty()
            && !text.starts_with('/')
            && !text.contains(['\\', '\0', ':'])
            && !path.is_absolute()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
            && path
                .components()
                .any(|component| matches!(component, Component::Normal(_)));
        if well_formed {
            Ok(Self(text.to_owned()))
        } else {
            Err(JevError::input_invalid(format!(
                "path {text} must be a repo-relative path inside the repository (no leading /, .., \\, NUL, or :)"
            )))
        }
    }
}

impl TryFrom<String> for RepoRelativePath {
    type Error = JevError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::try_from(text.as_str())
    }
}

impl FromStr for RepoRelativePath {
    type Err = JevError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::try_from(text)
    }
}

impl From<RepoRelativePath> for String {
    fn from(value: RepoRelativePath) -> Self {
        value.0
    }
}

impl fmt::Display for RepoRelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl PartialEq<str> for RepoRelativePath {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for RepoRelativePath {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl AsRef<str> for RepoRelativePath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Caller text that labels a project in the session log.
///
/// UTF-8, at most [`Self::MAX_BYTES`] bytes, no control characters. The text
/// is caller text, not a secret, so `Debug` prints it. The CLI, Python
/// inputs and the session-log loader share this one validator.
///
/// # Examples
///
/// ```
/// use sc_agent_types::ProjectLabel;
///
/// let label: ProjectLabel = "billing-service".parse()?;
/// assert_eq!(label.as_str(), "billing-service");
/// assert_eq!(ProjectLabel::new("a\nb").unwrap_err().code(), "INPUT_INVALID");
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectLabel(Arc<str>);

impl ProjectLabel {
    /// Longest accepted label, in UTF-8 bytes.
    pub const MAX_BYTES: usize = 256;

    /// Validates `text`.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when the text is longer than 256 bytes or has a
    /// control character; the fix hint states both limits.
    pub fn new(text: impl Into<String>) -> Result<Self, JevError> {
        let text = text.into();
        if text.len() > Self::MAX_BYTES || text.chars().any(char::is_control) {
            return Err(JevError::input_invalid(format!(
                "project label must be at most {} bytes of UTF-8 with no control characters (got {} bytes)",
                Self::MAX_BYTES,
                text.len()
            )));
        }
        Ok(Self(Arc::from(text)))
    }

    /// The label text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ProjectLabel {
    type Err = JevError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::new(text)
    }
}

impl AsRef<str> for ProjectLabel {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Serialize for ProjectLabel {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ProjectLabel {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// One send's id: a ULID (26 Crockford base32 characters).
///
/// # Examples
///
/// ```
/// use sc_agent_types::AnswerId;
///
/// let id = AnswerId::new();
/// assert_eq!(id.to_string().len(), 26);
/// assert_eq!(id.to_string().parse::<AnswerId>()?, id);
/// assert_eq!("nope".parse::<AnswerId>().unwrap_err().code(), "INPUT_INVALID");
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnswerId(Ulid);

impl AnswerId {
    /// A fresh id.
    ///
    /// `AnswerId` does not implement [`Default`], like [`SessionId`]: a
    /// default that is a fresh random id would surprise callers.
    #[must_use]
    #[expect(
        clippy::new_without_default,
        reason = "a random id is not a sensible Default value"
    )]
    pub fn new() -> Self {
        Self(Ulid::new())
    }
}

impl FromStr for AnswerId {
    type Err = JevError;

    /// # Errors
    ///
    /// `INPUT_INVALID` unless the text is a ULID; the fix hint says to use
    /// the logged `answer_id`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_ulid(text).map(Self).ok_or_else(|| {
            JevError::input_invalid(format!(
                "answer id {text:?} is not a ULID (26 Crockford base32 characters); use the answer_id the session log recorded"
            ))
        })
    }
}

impl fmt::Display for AnswerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

impl Serialize for AnswerId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for AnswerId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The send a log event records: a judge send or a session `ask`.
///
/// Log adapters write [`Self::as_str`] (`judge` or `ask`), which is also a
/// valid `sc-observability` action name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Operation {
    /// A judge send of a check run.
    Judge,
    /// A session `ask` send.
    Ask,
}

impl Operation {
    /// Every operation, in declaration order.
    pub const ALL: &'static [Self] = &[Self::Judge, Self::Ask];

    /// The wire and action-name spelling: `judge` or `ask`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Judge => "judge",
            Self::Ask => "ask",
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Operation {
    type Err = JevError;

    /// Parses the [`Self::as_str`] spelling.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::Operation;
    ///
    /// assert_eq!("ask".parse::<Operation>().unwrap(), Operation::Ask);
    /// assert_eq!("Judge".parse::<Operation>().unwrap_err().code(), "INPUT_INVALID");
    /// ```
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|operation| operation.as_str() == text)
            .ok_or_else(|| {
                JevError::input_invalid(format!("log operation must be judge or ask, not {text:?}"))
            })
    }
}

/// Strict ULID text: the canonical 26-character uppercase Crockford form.
///
/// `Ulid::from_string` also accepts lowercase, the `I`/`L`/`O` aliases and
/// values above 2^128 (which wrap); comparing with the re-rendered text
/// rejects all three, so one id has one spelling (and one file name).
fn parse_ulid(text: &str) -> Option<Ulid> {
    let ulid = Ulid::from_string(text).ok()?;
    (ulid.to_string() == text).then_some(ulid)
}

fn is_lower_hex(text: &str) -> bool {
    text.bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_backend_key_id_round_trips_through_its_spellings() {
        assert!(!BackendKeyId::ALL.is_empty());
        for id in BackendKeyId::ALL {
            assert_eq!(id.as_str().parse::<BackendKeyId>().unwrap(), *id);
            assert_eq!(id.to_string().parse::<BackendKeyId>().unwrap(), *id);
            for alias in id.aliases() {
                assert_eq!(alias.parse::<BackendKeyId>().unwrap(), *id, "{alias}");
            }
        }
        assert_eq!(
            "open_ai".parse::<BackendKeyId>().unwrap(),
            BackendKeyId::OpenAi
        );
        assert!("open-ai".parse::<BackendKeyId>().is_err());
    }

    #[test]
    fn rev_and_blob_sha_convert_and_compare() {
        let rev: Rev = "HEAD".parse().unwrap();
        assert_eq!(Rev::try_from("HEAD").unwrap(), rev);
        assert_eq!(<Rev as AsRef<str>>::as_ref(&rev), "HEAD");
        assert_eq!("-x".parse::<Rev>().unwrap_err().code(), "INPUT_INVALID");
        assert_eq!(Rev::try_from("").unwrap_err().code(), "INPUT_INVALID");
        let low = BlobSha::try_from("a".repeat(40).as_str()).unwrap();
        let high = BlobSha::try_from("b".repeat(40).as_str()).unwrap();
        assert!(low < high);
        assert_eq!(<BlobSha as AsRef<str>>::as_ref(&low), "a".repeat(40));
    }

    #[test]
    fn digest_rejects_uppercase_and_short_text() {
        assert!(Sha256Digest::parse("ab").is_err());
        assert!(Sha256Digest::parse("A".repeat(64)).is_err());
        let ok = "ab".repeat(32);
        assert_eq!(Sha256Digest::parse(&ok).unwrap().as_str(), ok);
    }

    #[test]
    fn descriptor_lookup_matches_catalog_rows() {
        for row in BACKENDS {
            assert_eq!(row.id().descriptor(), row);
        }
    }

    #[test]
    fn rev_parse_rejects_option_like_and_unsafe_text() {
        for good in [
            "WORKTREE",
            "HEAD",
            "HEAD~1",
            "main",
            "a1b2c3",
            "refs/tags/v1.0",
        ] {
            assert_eq!(Rev::parse(good).unwrap().as_str(), good);
        }
        for bad in [
            "",
            "-",
            "--output=/tmp/x",
            "-n1",
            "HEAD 1",
            "a\tb",
            "a\nb",
            "a\0b",
            " HEAD",
        ] {
            let error = Rev::parse(bad).unwrap_err();
            assert_eq!(error.code(), "INPUT_INVALID", "{bad:?}");
        }
    }

    #[test]
    fn digest_of_empty_is_stable() {
        assert_eq!(
            Sha256Digest::of_text("").as_str(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
