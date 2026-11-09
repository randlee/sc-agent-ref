//! The model catalog: [`ModelId`], [`MODELS`] and [`ModelAliases`]
//! (REQ-SAG-0147, REQ-SAG-0148, ADR-SAG-0030).
//!
//! [`BackendId`](crate::BackendId) says how we call (Jev API, Anthropic API,
//! OpenAI API); [`ModelId`] says which model. Private module, reached through
//! the crate-root re-exports.
//!
//! A [`ModelId`] is open: a model released after this build parses as
//! [`ModelId::Other`], is sent verbatim and is logged.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::JevError;

/// A model string this build does not know.
///
/// The field is private and there is no public constructor. The only ways to
/// get one are [`ModelId::from_str`] (which returns the known variant for a
/// known string, so [`ModelId::Other`] never holds a known string and the
/// derived `PartialEq` and `Hash` stay correct) and `Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnknownModel(Arc<str>);

impl UnknownModel {
    /// The model string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which model: the string a backend sends and the session log records.
///
/// # Examples
///
/// ```
/// use sc_agent_types::ModelId;
///
/// let haiku: ModelId = "claude-haiku-5-5".parse()?;
/// assert_eq!(haiku, ModelId::ClaudeHaiku5_5);
/// let later: ModelId = "claude-haiku-9".parse()?;
/// assert_eq!(later.as_str(), "claude-haiku-9");
/// assert!("".parse::<ModelId>().is_err());
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModelId {
    /// `jev`: the latest TypeSafe Jev model, whichever version that is.
    ///
    /// sc-agent accepts `jev` or any supported version such as
    /// `jev-1.14.0` (the newest row; `jev-1.13.0` is older). `jev` follows the vendor's latest release, so an API
    /// change this build does not know can break it; that is expected, and
    /// the remedy is upgrading sc-agent. Pin a version (`jev-1.14.0`) to
    /// avoid the exposure. The wire value `jev` is sent as is decided in one
    /// place, `sc_decision_schema::jev::v1::wire_model`.
    Jev,
    /// `jev-1.13.0`.
    Jev1_13_0,
    /// `jev-1.14.0`: the version live replies answered.
    Jev1_14_0,
    /// `claude-haiku-5-5`.
    ClaudeHaiku5_5,
    /// `claude-sonnet-5-5`. A definition for future agent work; not a
    /// `--backend` today.
    ClaudeSonnet5_5,
    /// `claude-opus-5-5`. A definition; not a `--backend` today.
    ClaudeOpus5_5,
    /// `claude-fable-5-1`. A definition; not a `--backend` today.
    ClaudeFable5_1,
    /// `gpt-6-luna`.
    Gpt6Luna,
    /// `gpt-6.1-terra`. A definition; not a `--backend` today.
    Gpt6_1Terra,
    /// `gpt-6.1-sol`. A definition; not a `--backend` today.
    Gpt6_1Sol,
    /// `gpt-6-astra`. A definition; not a `--backend` today.
    Gpt6Astra,
    /// Any other valid model string; never a known string.
    Other(UnknownModel),
}

impl ModelId {
    /// The known variant for `text`, if the catalog has one.
    pub(crate) fn known(text: &str) -> Option<Self> {
        match text {
            "jev" => Some(Self::Jev),
            "jev-1.13.0" => Some(Self::Jev1_13_0),
            "jev-1.14.0" => Some(Self::Jev1_14_0),
            "claude-haiku-5-5" => Some(Self::ClaudeHaiku5_5),
            "claude-sonnet-5-5" => Some(Self::ClaudeSonnet5_5),
            "claude-opus-5-5" => Some(Self::ClaudeOpus5_5),
            "claude-fable-5-1" => Some(Self::ClaudeFable5_1),
            "gpt-6-luna" => Some(Self::Gpt6Luna),
            "gpt-6.1-terra" => Some(Self::Gpt6_1Terra),
            "gpt-6.1-sol" => Some(Self::Gpt6_1Sol),
            "gpt-6-astra" => Some(Self::Gpt6Astra),
            _ => None,
        }
    }

    /// The model string (`Display`, serde, the wire).
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Jev => "jev",
            Self::Jev1_13_0 => "jev-1.13.0",
            Self::Jev1_14_0 => "jev-1.14.0",
            Self::ClaudeHaiku5_5 => "claude-haiku-5-5",
            Self::ClaudeSonnet5_5 => "claude-sonnet-5-5",
            Self::ClaudeOpus5_5 => "claude-opus-5-5",
            Self::ClaudeFable5_1 => "claude-fable-5-1",
            Self::Gpt6Luna => "gpt-6-luna",
            Self::Gpt6_1Terra => "gpt-6.1-terra",
            Self::Gpt6_1Sol => "gpt-6.1-sol",
            Self::Gpt6Astra => "gpt-6-astra",
            Self::Other(unknown) => unknown.as_str(),
        }
    }

    /// The model family; [`ModelFamily::Unknown`] for [`Self::Other`].
    ///
    /// This is the only family `schema_for` in `sc-decision-schema`
    /// consumes. It is never derived from `BackendFamily`.
    #[must_use]
    pub fn family(&self) -> ModelFamily {
        // `jev` is not a `MODELS` row: it names no one release, so it has no
        // release date and must not become the `jev` alias default.
        if matches!(self, Self::Jev) {
            return ModelFamily::TypeSafeJev;
        }
        MODELS
            .iter()
            .find(|row| row.model == self.as_str())
            .map_or(ModelFamily::Unknown, |row| row.family)
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ModelId {
    type Err = JevError;

    /// Parses a model string: the known variant when the catalog has it,
    /// else [`ModelId::Other`].
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when the text is empty or has a control character.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty() || text.chars().any(char::is_control) {
            return Err(JevError::input_invalid(format!(
                "model id {text:?} must be non-empty with no control characters; use a non-empty model string or an alias such as haiku"
            )));
        }
        Ok(Self::known(text).unwrap_or_else(|| Self::Other(UnknownModel(Arc::from(text)))))
    }
}

impl Serialize for ModelId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ModelId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The family of a model, the key `schema_for` maps to a decision schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModelFamily {
    /// The TypeSafe Jev models.
    TypeSafeJev,
    /// Anthropic Claude models.
    AnthropicClaude,
    /// OpenAI GPT models.
    OpenAiGpt,
    /// A local decision model.
    Local,
    /// A model string this build does not know.
    Unknown,
}

/// One row of [`MODELS`].
#[derive(Debug, Clone, Copy)]
pub struct ModelDescriptor {
    model: &'static str,
    family: ModelFamily,
    alias: &'static str,
    released: &'static str,
}

impl ModelDescriptor {
    /// The model string of this row, without parsing it.
    #[must_use]
    pub const fn model_str(&self) -> &'static str {
        self.model
    }

    /// The model of this row.
    ///
    /// # Panics
    ///
    /// Never for a `MODELS` row: `test_known_models_roundtrip` proves every
    /// row string is a valid model string (M-PANIC-ON-BUG).
    #[must_use]
    pub fn model(&self) -> ModelId {
        self.model
            .parse()
            .expect("MODELS rows are valid model strings (test_known_models_roundtrip)")
    }

    /// The model family.
    #[must_use]
    pub const fn family(&self) -> ModelFamily {
        self.family
    }

    /// The alias this row answers to (`"jev"`, `"haiku"`, `"luna"`,
    /// `"sonnet"`, `"opus"`, `"fable"`, `"terra"`, `"sol"`, `"astra"`).
    #[must_use]
    pub const fn alias(&self) -> &'static str {
        self.alias
    }

    /// Release date, `YYYY-MM-DD`.
    #[must_use]
    pub const fn released(&self) -> &'static str {
        self.released
    }
}

/// The known models. Per alias, the newest `released` row is the default.
///
/// Only `jev`, `haiku` and `luna` are selectable as `--backend`; the other
/// aliases are definitions for future agent work (agt-akp). Models added
/// after this build go in `~/.sc-agent/models.toml` ([`ModelAliases::with_file_model`]);
/// a local freshness test tells maintainers when this table is behind
/// (randlee/sc-agent#48).
pub static MODELS: &[ModelDescriptor] = &[
    row("jev-1.13.0", ModelFamily::TypeSafeJev, "jev", "2026-06-01"),
    row("jev-1.14.0", ModelFamily::TypeSafeJev, "jev", "2026-10-22"),
    row(
        "claude-haiku-5-5",
        ModelFamily::AnthropicClaude,
        "haiku",
        "2026-09-01",
    ),
    row(
        "claude-sonnet-5-5",
        ModelFamily::AnthropicClaude,
        "sonnet",
        "2026-09-01",
    ),
    row(
        "claude-opus-5-5",
        ModelFamily::AnthropicClaude,
        "opus",
        "2026-09-01",
    ),
    row(
        "claude-fable-5-1",
        ModelFamily::AnthropicClaude,
        "fable",
        "2026-10-01",
    ),
    row("gpt-6-luna", ModelFamily::OpenAiGpt, "luna", "2026-08-01"),
    row(
        "gpt-6.1-terra",
        ModelFamily::OpenAiGpt,
        "terra",
        "2026-10-01",
    ),
    row("gpt-6.1-sol", ModelFamily::OpenAiGpt, "sol", "2026-10-01"),
    row("gpt-6-astra", ModelFamily::OpenAiGpt, "astra", "2026-08-01"),
];

const fn row(
    model: &'static str,
    family: ModelFamily,
    alias: &'static str,
    released: &'static str,
) -> ModelDescriptor {
    ModelDescriptor {
        model,
        family,
        alias,
        released,
    }
}

/// Where a resolved model came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ModelSource {
    /// The [`MODELS`] table of this build.
    Crate,
    /// A `[[model]]` row of `~/.sc-agent/models.toml`.
    File,
    /// A `check.toml` `[models]` entry, a `--backend` model pin, or a caller
    /// override ([`ModelAliases::with_override`]).
    Override,
}

impl ModelSource {
    /// `crate`, `file` or `override`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Crate => "crate",
            Self::File => "file",
            Self::Override => "override",
        }
    }
}

impl fmt::Display for ModelSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One alias with the model it resolves to and where that came from.
///
/// Serializes as `{"alias","model","source"}` plus `released` (absent for an
/// override).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedModel {
    alias: &'static str,
    model: ModelId,
    source: ModelSource,
    #[serde(skip_serializing_if = "String::is_empty")]
    released: String,
}

impl ResolvedModel {
    /// The alias (`haiku`, `luna`, ...).
    #[must_use]
    pub const fn alias(&self) -> &'static str {
        self.alias
    }

    /// The model the alias resolves to.
    #[must_use]
    pub const fn model(&self) -> &ModelId {
        &self.model
    }

    /// Where the model came from.
    #[must_use]
    pub const fn source(&self) -> ModelSource {
        self.source
    }

    /// Release date `YYYY-MM-DD`; empty for an override.
    #[must_use]
    pub fn released(&self) -> &str {
        &self.released
    }
}

/// Alias to model lookup: the newest known model per alias, with the global
/// file's additions and overrides.
///
/// Resolution order, last wins: the newest `released` row across [`MODELS`]
/// and the models file ([`Self::with_file_model`]); then overrides
/// ([`Self::with_override`], which `check.toml` `[models]` and a
/// `--backend` model pin use).
///
/// # Examples
///
/// ```
/// use sc_agent_types::{ModelAliases, ModelId, ModelSource};
///
/// let aliases = ModelAliases::builtin();
/// assert_eq!(aliases.latest("haiku"), Some(ModelId::ClaudeHaiku5_5));
/// assert_eq!(aliases.source("haiku"), Some(ModelSource::Crate));
/// let newer = aliases.with_file_model("claude-haiku-9", "haiku", "2027-01-01")?;
/// assert_eq!(newer.latest("haiku").unwrap().as_str(), "claude-haiku-9");
/// assert_eq!(newer.source("haiku"), Some(ModelSource::File));
/// let pinned = newer.with_override("haiku", "claude-haiku-5-5".parse()?)?;
/// assert_eq!(pinned.latest("haiku"), Some(ModelId::ClaudeHaiku5_5));
/// assert_eq!(pinned.source("haiku"), Some(ModelSource::Override));
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAliases(
    BTreeMap<&'static str, Entry>,
    // Every model id the catalog knows (crate rows and models-file rows,
    // not overrides) and the alias it was declared under.
    BTreeMap<String, &'static str>,
);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    model: ModelId,
    source: ModelSource,
    released: String,
}

impl ModelAliases {
    /// The newest `MODELS` row per alias.
    #[must_use]
    pub fn builtin() -> Self {
        let mut newest: BTreeMap<&'static str, &ModelDescriptor> = BTreeMap::new();
        for row in MODELS {
            let replace = newest
                .get(row.alias)
                .is_none_or(|current| row.released >= current.released);
            if replace {
                newest.insert(row.alias, row);
            }
        }
        let known = MODELS
            .iter()
            .map(|row| (row.model.to_owned(), row.alias))
            .collect();
        Self(
            newest
                .into_iter()
                .map(|(alias, row)| {
                    let entry = Entry {
                        model: row.model(),
                        source: ModelSource::Crate,
                        released: row.released.to_owned(),
                    };
                    (alias, entry)
                })
                .collect(),
            known,
        )
    }

    /// The alias a model id of the catalog was declared under (`jev` for
    /// `jev-1.15.0`), or `None` for an id the catalog does not list.
    ///
    /// The catalog is the [`MODELS`] table plus the rows added with
    /// [`Self::with_file_model`], whether or not a row is the newest of its
    /// alias. Overrides ([`Self::with_override`]) do not add to it.
    #[must_use]
    pub fn alias_of(&self, id: &str) -> Option<&'static str> {
        self.1.get(id).copied()
    }

    /// Every model id of the catalog with its alias, sorted by id.
    #[must_use]
    pub fn known_models(&self) -> Vec<(&str, &'static str)> {
        self.1
            .iter()
            .map(|(id, alias)| (id.as_str(), *alias))
            .collect()
    }

    /// The model `alias` currently names, or `None` for an unknown alias.
    #[must_use]
    pub fn latest(&self, alias: &str) -> Option<ModelId> {
        self.0.get(alias).map(|entry| entry.model.clone())
    }

    /// Where the model `alias` names came from, or `None` for an unknown
    /// alias.
    #[must_use]
    pub fn source(&self, alias: &str) -> Option<ModelSource> {
        self.0.get(alias).map(|entry| entry.source)
    }

    /// Every alias with its model and source, sorted by alias.
    #[must_use]
    pub fn resolved(&self) -> Vec<ResolvedModel> {
        self.0
            .iter()
            .map(|(alias, entry)| ResolvedModel {
                alias,
                model: entry.model.clone(),
                source: entry.source,
                released: entry.released.clone(),
            })
            .collect()
    }

    /// Adds one `[[model]]` row of `~/.sc-agent/models.toml`: `alias` names
    /// `id` when `released` is not older than what the alias has now.
    ///
    /// A row only extends an existing alias; it cannot create one, so it
    /// cannot create a backend. A tie goes to the file. An alias already
    /// overridden stays overridden.
    ///
    /// # Errors
    ///
    /// `CONFIG_INVALID` for an unknown alias (the message lists the valid
    /// ones), an invalid model string, or a `released` that is not a
    /// `YYYY-MM-DD` date.
    pub fn with_file_model(
        mut self,
        id: &str,
        alias: &str,
        released: &str,
    ) -> Result<Self, JevError> {
        let model: ModelId = id.parse().map_err(|error: JevError| {
            JevError::config_invalid(format!("models file: model id {id:?}: {error}"))
        })?;
        check_date(released)?;
        self.check_family(alias, &model)
            .map_err(|error| error.map_detail(|detail| format!("models file: {detail}")))?;
        let valid = self.aliases().join(", ");
        let Some((&name, entry)) = self.0.iter_mut().find(|(name, _)| **name == alias) else {
            return Err(JevError::config_invalid(format!(
                "models file: unknown model alias {alias:?}; valid aliases: {valid}"
            )));
        };
        let newer = entry.source != ModelSource::Override && released >= entry.released.as_str();
        self.1.insert(model.as_str().to_owned(), name);
        if newer {
            *entry = Entry {
                model,
                source: ModelSource::File,
                released: released.to_owned(),
            };
        }
        Ok(self)
    }

    /// Makes `alias` name `model`.
    ///
    /// # Errors
    ///
    /// `CONFIG_INVALID` for an unknown alias (the fix hint lists the valid
    /// aliases) or a model of another family (`haiku` takes `claude-...`).
    pub fn with_override(mut self, alias: &str, model: ModelId) -> Result<Self, JevError> {
        if self.0.contains_key(alias) {
            self.check_family(alias, &model)?;
        }
        match self.0.get_mut(alias) {
            Some(slot) => {
                *slot = Entry {
                    model,
                    source: ModelSource::Override,
                    released: String::new(),
                };
                Ok(self)
            }
            None => Err(JevError::config_invalid(format!(
                "unknown model alias {alias:?}; valid aliases: {}",
                self.aliases().join(", ")
            ))),
        }
    }

    /// Applies every override of `top` (an entry whose source is
    /// [`ModelSource::Override`]) over `self`. `top` is the `check.toml`
    /// `[models]` layer, built on the catalog; its other entries are
    /// ignored so the global file's rows are not undone by the catalog.
    #[must_use]
    pub fn overlay_overrides(mut self, top: &Self) -> Self {
        for (alias, entry) in &top.0 {
            if entry.source == ModelSource::Override {
                self.0.insert(alias, entry.clone());
            }
        }
        self
    }

    /// `CONFIG_INVALID` unless `model` belongs to the family of `alias`: by
    /// prefix, `jev` takes `jev` or `jev-...`, the Claude lines take
    /// `claude-...`, the GPT lines `gpt-...`. An unknown alias passes (the
    /// caller reports it).
    fn check_family(&self, alias: &str, model: &ModelId) -> Result<(), JevError> {
        let Some(entry) = self.0.get(alias) else {
            return Ok(());
        };
        let prefix = match entry.model.family() {
            ModelFamily::TypeSafeJev => "jev",
            ModelFamily::AnthropicClaude => "claude-",
            ModelFamily::OpenAiGpt => "gpt-",
            _ => return Ok(()),
        };
        let text = model.as_str();
        let matches = text.starts_with(prefix)
            && (prefix != "jev" || text == "jev" || text.starts_with("jev-"));
        if matches {
            Ok(())
        } else {
            Err(JevError::config_invalid(format!(
                "model {text:?} does not belong to alias {alias:?}; its ids start with {prefix:?}"
            )))
        }
    }

    /// The valid aliases, sorted.
    #[must_use]
    pub fn aliases(&self) -> Vec<&'static str> {
        self.0.keys().copied().collect()
    }
}

/// `YYYY-MM-DD` with a month 1..=12 and a day 1..=31.
fn check_date(text: &str) -> Result<(), JevError> {
    let bytes = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        bytes
            .get(range)
            .filter(|part| part.iter().all(u8::is_ascii_digit))
            .and_then(|part| std::str::from_utf8(part).ok())
            .and_then(|part| part.parse::<u32>().ok())
    };
    let ok = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && digits(0..4).is_some()
        && digits(5..7).is_some_and(|month| (1..=12).contains(&month))
        && digits(8..10).is_some_and(|day| (1..=31).contains(&day));
    if ok {
        Ok(())
    } else {
        Err(JevError::config_invalid(format!(
            "models file: released {text:?} is not a YYYY-MM-DD date"
        )))
    }
}
