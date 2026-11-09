//! File-backed judge sessions (REQ-SAG-0600, REQ-SAG-0607, ADR-SAG-0008,
//! ADR-SAG-0024).
//!
//! Each session is one `<id>.json` file with mode `0600` in a directory with
//! mode `0700` (when this store creates it). Expiry is `created_at` plus the
//! store TTL; touching the file does not extend the session. Haiku and Luna
//! `ask` calls reuse the stored payload, system prompt, and prior answers.
//! Jev has no rationale field, so `ask` refuses a Jev session.
//!
//! [`FileSessionStore`] is stateless: a directory, a TTL, and a clock `fn`
//! pointer. It holds no registry, logger, or lock, and it never sends:
//! [`crate::run::ask`] loads a session through it and sends the follow-up
//! on the engine's one send path (ADR-SAG-0024). [`crate::run::run`]
//! writes a session once, at the end of a run, and only when a backend whose
//! descriptor has `supports_session_ask` returned `Ok`. Reading and writing
//! session documents directly (`save`, `load`, `StoredSession`) is public
//! only under the `test-util` feature.
//!
//! # Concurrency model (RSH-004)
//!
//! There is no lock, in process or across processes.
//!
//! - `FileSessionStore::save` writes the document to
//!   `<id>.json.tmp.<pid>.<random>` opened with `create_new`, fsyncs it,
//!   renames it over `<id>.json`, then fsyncs the parent directory. Any
//!   failure removes the temporary file. A rename is atomic on one file
//!   system, so a reader sees either the old document or the new one, never
//!   a torn write.
//! - Two writers of the same id: the last rename wins. Neither corrupts the
//!   file. Session ids are random ULIDs and `run` saves each id once, so
//!   this only happens when a caller saves the same `StoredSession` twice.
//!   On Windows, replacing a file that another writer is replacing at the
//!   same instant can fail with a sharing error; that save returns
//!   `IO_FAILED` and removes its temporary file, and the other write
//!   stands.
//! - Readers re-verify every document they load: the stored id must match
//!   the file name's id and the payload must match its stored SHA-256, and
//!   every stored outcome is checked against the payload. A file that fails
//!   any check is `SessionDocumentInvalid`.
//! - `ask` reads the session and never rewrites it, so concurrent `ask`
//!   calls on one session cannot lose each other's data.
//! - The parent-directory fsync after the rename is best effort: the new
//!   file is already in place, so a failed directory sync is not reported
//!   as a failed save (it only weakens crash durability of the rename).
//! - A crashed writer can leave a temporary file behind.
//!   [`FileSessionStore::new`] removes `*.tmp.*` files older than one hour;
//!   younger ones may belong to a live writer.
//! - Retention: [`FileSessionStore::new`] also removes every expired
//!   `*.json` session document in the directory, measured exactly like
//!   `read_session` (`created_at` plus the default TTL, 24 h, against the
//!   system clock) plus one grace period of the same length, so a session that
//!   just expired still answers `SESSION_EXPIRED` ("start a new session")
//!   instead of `SESSION_NOT_FOUND` for another day. That includes version 1 and
//!   2 documents, which are refused
//!   on read and would otherwise stay forever. A file that is not a JSON
//!   object with a numeric `created_at`, or is larger than the read cap, is
//!   left alone. The sweep is best effort and runs once per `new`; a store
//!   opened with a shorter [`FileSessionStore::with_ttl`] still removes only
//!   documents past the default TTL.

#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::collections::hash_map::RandomState;
use std::fs::{self, File, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sc_agent_types::{BackendId, JevError, SessionId};
use sc_decision_schema::{
    DecisionSchemaId, JudgeOutcome, PayloadBundle, QuestionAnswer, QuestionId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod stored;

#[cfg(any(test, feature = "test-util"))]
#[doc(inline)]
pub use stored::StoredSession;
#[cfg(not(any(test, feature = "test-util")))]
pub(crate) use stored::StoredSession;

/// Default lifetime measured from `created_at`.
pub(crate) const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Age after which [`FileSessionStore::new`] removes a leftover temporary file.
///
/// A save finishes in well under a second, so a temporary file this old
/// belongs to a writer that crashed. One hour leaves a wide margin for a
/// slow disk without letting leftovers pile up.
pub(crate) const STALE_TMP_AGE: Duration = Duration::from_secs(60 * 60);

/// Header inserted before prior answers in an LLM system prompt.
pub(crate) const SESSION_CONTEXT_HEADER: &str = "Session context:";

/// Largest session file this store will read.
const MAX_SESSION_BYTES: u64 = 8 * 1024 * 1024;

/// Session document version written and accepted by this store.
///
/// Version 1 (Phase A) files are refused: their outcomes carry the removed
/// `probabilities` field and untyped answers (r-1 D7.6). Version 2 files are
/// refused too: they predate ULID session ids and the `decision_schema` field
/// (r-6, ADR-SAG-0030); they expire by TTL and are not migrated.
const SESSION_VERSION: u64 = 3;

/// Session file extension.
const SESSION_EXT: &str = "json";

/// Marker between a session file name and a temporary suffix.
const TMP_MARKER: &str = ".tmp.";

/// Sessions stored as `<id>.json` files under one directory.
///
/// Stateless and cheap to copy: `dir`, `ttl`, and a `clock` function
/// pointer. See the module documentation for the concurrency model.
#[derive(Debug, Clone)]
pub struct FileSessionStore {
    dir: PathBuf,
    ttl: Duration,
    clock: fn() -> SystemTime,
}

impl FileSessionStore {
    /// Creates a store rooted at `dir`, creating the directory when needed,
    /// and removes temporary files older than one hour.
    ///
    /// Directories created here are mode `0700` on Unix. An existing
    /// directory's mode is left unchanged. Session files are mode `0600`.
    /// The sweep is best effort: a file that cannot be inspected or removed
    /// is left in place.
    ///
    /// # Errors
    ///
    /// Returns [`JevError::io_failed_with`] (`IO_FAILED`) when `dir` cannot be created.
    /// The code is `IO_FAILED`; the message names `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self, JevError> {
        let dir = dir.into();
        create_private_dir(&dir).map_err(|error| {
            JevError::io_failed_with(
                format!("session directory {} (create)", dir.display()),
                error.kind(),
                Some("Choose a writable directory, then retry.".to_owned()),
            )
        })?;
        let now = SystemTime::now();
        sweep_stale_tmp(&dir, now);
        sweep_expired_sessions(&dir, now, DEFAULT_SESSION_TTL.saturating_mul(2));
        Ok(Self {
            dir,
            ttl: DEFAULT_SESSION_TTL,
            clock: SystemTime::now,
        })
    }

    /// Sets the lifetime measured from `created_at`.
    #[must_use]
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Replaces the clock used for expiry and by [`Self::now`].
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> SystemTime) -> Self {
        self.clock = clock;
        self
    }

    /// Directory that holds session files.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path of the `<id>.json` file for `id`.
    #[must_use]
    pub fn path(&self, id: &SessionId) -> PathBuf {
        self.dir.join(format!("{id}.{SESSION_EXT}"))
    }

    /// The store clock's current time, used for a new session's `created_at`.
    #[must_use]
    pub fn now(&self) -> SystemTime {
        (self.clock)()
    }

    /// Writes `session` to `<id>.json`, replacing any existing file
    /// (`test-util` feature).
    ///
    /// The write is atomic (temporary file, fsync, rename, parent fsync) and
    /// removes its temporary file on any error. Last writer wins.
    ///
    /// # Errors
    ///
    /// Returns [`JevError::IoFailed`] (`IO_FAILED`, naming the session file)
    /// when the file cannot be written or when `created_at` is before the
    /// Unix epoch.
    #[cfg(any(test, feature = "test-util"))]
    pub fn save(&self, session: &StoredSession) -> Result<(), JevError> {
        self.write_session(session)
    }

    /// Writes `session` (see `save`).
    pub(crate) fn write_session(&self, session: &StoredSession) -> Result<(), JevError> {
        let document = SessionDocument {
            version: SESSION_VERSION,
            session_id: session.session_id,
            backend: session.backend,
            created_at: unix_ms(session.created_at)?,
            payload_utf8: session.payload.as_str().to_owned(),
            prompt_sha256: session.payload.sha256().clone(),
            decision_schema: DecisionSchemaId::CURRENT,
            system_prompt: session.system_prompt.clone(),
            outcomes: session.outcomes.clone(),
            follow_ups: session.follow_ups.clone(),
        };
        let path = self.path(&session.session_id);
        let bytes = serde_json::to_vec(&document).map_err(|error| {
            session_parse(
                &path,
                &format!("could not be encoded ({})", json_cause(&error)),
            )
        })?;
        write_private(&path, &bytes)
            .map_err(|error| session_io(&path, &self.dir, "write", error.kind()))
    }

    /// Loads `id` when `created_at` plus the TTL is still in the future
    /// (`test-util` feature).
    ///
    /// File modification time is not consulted.
    ///
    /// # Errors
    ///
    /// Returns [`JevError::SessionNotFound`] when the file is absent,
    /// [`JevError::SessionExpired`] when `created_at` is past the TTL,
    /// [`JevError::io_failed_with`] when the file cannot be read, and
    /// [`JevError::session_document_invalid`] when the file is not a valid
    /// session document for `id`.
    #[cfg(any(test, feature = "test-util"))]
    pub fn load(&self, id: &SessionId) -> Result<StoredSession, JevError> {
        self.read_session(id)
    }

    /// Loads `id` (see `load`).
    pub(crate) fn read_session(&self, id: &SessionId) -> Result<StoredSession, JevError> {
        let document = self.read_document(id)?;
        if self.is_expired(document.created_at)? {
            return Err(JevError::session_expired(*id));
        }
        StoredSession::from_document(document, &self.path(id))
    }

    fn is_expired(&self, created_at: u64) -> Result<bool, JevError> {
        let now = unix_ms(self.now())?;
        let ttl_ms = u64::try_from(self.ttl.as_millis()).unwrap_or(u64::MAX);
        Ok(now >= created_at.saturating_add(ttl_ms))
    }

    fn read_document(&self, id: &SessionId) -> Result<SessionDocument, JevError> {
        let path = self.path(id);
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(JevError::session_not_found(*id));
            }
            Err(error) => {
                return Err(session_io(&path, &self.dir, "open", error.kind()));
            }
        };
        let mut bytes = Vec::new();
        file.take(MAX_SESSION_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| session_io(&path, &self.dir, "read", error.kind()))?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_SESSION_BYTES {
            return Err(session_parse(
                &path,
                &format!("exceeds {MAX_SESSION_BYTES} bytes"),
            ));
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
            session_parse(
                &path,
                &format!("is not a session document ({})", json_cause(&error)),
            )
        })?;
        check_version(&path, &value)?;
        let document: SessionDocument = serde_json::from_value(value).map_err(|error| {
            session_parse(
                &path,
                &format!("is not a session document ({})", json_cause(&error)),
            )
        })?;
        if document.session_id != *id {
            return Err(session_parse(&path, "does not match this session id"));
        }
        if sc_agent_types::Sha256Digest::of_text(&document.payload_utf8) != document.prompt_sha256 {
            return Err(session_parse(&path, "payload digest does not match"));
        }
        Ok(document)
    }
}

/// Renders prior answers for an LLM system prompt.
///
/// The canonical payload bytes are not modified. An empty `outcomes` slice
/// renders as an empty string.
///
/// # Errors
///
/// Returns [`JevError::InputInvalid`] when the answers cannot be encoded.
pub(crate) fn render_session_context(outcomes: &[JudgeOutcome]) -> Result<String, JevError> {
    if outcomes.is_empty() {
        return Ok(String::new());
    }
    let prior: Vec<Prior<'_>> = outcomes
        .iter()
        .map(|outcome| Prior {
            backend: outcome.backend(),
            answers: outcome.answers(),
        })
        .collect();
    let json = serde_json::to_string(&prior)
        .map_err(|error| JevError::encode_failed("session context", &error))?;
    Ok(format!("\n\n{SESSION_CONTEXT_HEADER}\n{json}"))
}

/// The LLM system prompt with session context: `base`, then the prior
/// answers ([`render_session_context`]), then each follow-up.
///
/// The one assembly for a chained backend in `run` (no follow-ups) and for a
/// session `ask` (stored follow-ups plus the new one).
pub(crate) fn contextual_prompt(
    base: Option<&str>,
    prior: &[JudgeOutcome],
    follow_ups: &[&str],
) -> Result<String, JevError> {
    let mut prompt = base.unwrap_or_default().to_owned();
    prompt.push_str(&render_session_context(prior)?);
    for follow_up in follow_ups {
        prompt.push_str("\n\nFollow-up:\n");
        prompt.push_str(follow_up);
    }
    Ok(prompt)
}

/// Refuses an `ask` that cannot be sent: an empty follow-up, or a session
/// whose backend has no `ask` (Jev has no rationale field).
pub(crate) fn check_follow_up(stored: &StoredSession, follow_up: &str) -> Result<(), JevError> {
    if follow_up.trim().is_empty() {
        return Err(follow_up_invalid(
            "follow_up",
            "the follow-up text is empty; pass non-empty text to ask",
        ));
    }
    if !stored.backend.descriptor().supports_session_ask() {
        return Err(follow_up_invalid(
            "session_id",
            "Jev sessions have no rationale field, so they cannot be asked; ask a Haiku or Luna session instead",
        ));
    }
    Ok(())
}

/// `serde_json` failure without its message, which can quote document values.
fn json_cause(error: &serde_json::Error) -> String {
    format!(
        "{:?} error at line {} column {}",
        error.classify(),
        error.line(),
        error.column()
    )
}

fn session_io(path: &Path, dir: &Path, op: &str, kind: std::io::ErrorKind) -> JevError {
    JevError::io_failed_with(
        format!("session file {} ({op})", path.display()),
        kind,
        Some(format!(
            "Check that session directory {} is accessible, then retry.",
            dir.display()
        )),
    )
}

fn session_parse(path: &Path, problem: &str) -> JevError {
    JevError::session_document_invalid(
        format!("Session file {} {problem}", path.display()),
        "Delete it and start a new session.",
    )
}

/// Refuses any session document whose `version` is not [`SESSION_VERSION`].
///
/// A missing or older version is a Phase A file; a newer one was written by
/// a newer sc-agent. Both are `SessionDocumentInvalid` (`INPUT_INVALID`).
fn check_version(path: &Path, value: &Value) -> Result<(), JevError> {
    match value.get("version").and_then(Value::as_u64) {
        Some(SESSION_VERSION) => Ok(()),
        Some(newer) if newer > SESSION_VERSION => Err(JevError::session_document_invalid(
            format!(
                "Session file {} is version {newer}; this sc-agent reads version {SESSION_VERSION}",
                path.display()
            ),
            "written by a newer sc-agent; upgrade or re-run the check",
        )),
        older => Err(JevError::session_document_invalid(
            format!(
                "Session file {} is version {}; this sc-agent reads version {SESSION_VERSION}",
                path.display(),
                older.map_or_else(|| "missing".to_owned(), |v| v.to_string())
            ),
            "re-run the check; session files from before Phase A.1 cannot be resumed",
        )),
    }
}

/// `INPUT_INVALID` for a refused `ask`: `field` names the argument and
/// `detail` says why it is refused.
fn follow_up_invalid(field: &str, detail: &str) -> JevError {
    JevError::input_invalid_field(field, detail)
}

fn unix_ms(time: SystemTime) -> Result<u64, JevError> {
    let elapsed = time.duration_since(UNIX_EPOCH).map_err(|_| {
        JevError::io_failed_with(
            "system clock (before the unix epoch, so the session timestamp cannot be written)",
            std::io::ErrorKind::Other,
            Some("Fix the system clock and retry.".to_owned()),
        )
    })?;
    u64::try_from(elapsed.as_millis()).map_err(|_| {
        JevError::io_failed_with(
            "system clock (out of range for a session timestamp)",
            std::io::ErrorKind::Other,
            Some("Fix the system clock and retry.".to_owned()),
        )
    })
}

fn system_time_from_unix_ms(millis: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(millis)
}

fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Unique temporary sibling of `path`: `<name>.tmp.<pid>.<random>`.
///
/// The random part is 64 bits from a fresh [`RandomState`] (seeded from the
/// OS), so two writers in one process or two processes never pick the same
/// name; `create_new` refuses the rare collision and the caller retries.
fn tmp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session");
    let random = RandomState::new().build_hasher().finish();
    path.with_file_name(format!(
        "{name}{TMP_MARKER}{}.{random:016x}",
        std::process::id()
    ))
}

/// Atomically replaces `path` with `bytes` (mode `0600`).
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let (tmp, file) = open_unique_tmp(path, &options)?;
    let written = finish_tmp(file, &tmp, bytes).and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
        return written;
    }
    // Best effort: the renamed file is in place; a failed directory sync only
    // weakens crash durability of the rename, so it is not a failed save.
    let _ = sync_parent(path);
    Ok(())
}

/// Attempts at a fresh temporary name before giving up.
///
/// Names carry 64 random bits, so a collision is already improbable; eight
/// tries make a persistent `AlreadyExists` (a directory that refuses new
/// names) an error instead of a loop.
const TMP_OPEN_ATTEMPTS: usize = 8;

fn open_unique_tmp(path: &Path, options: &OpenOptions) -> std::io::Result<(PathBuf, File)> {
    let mut last = None;
    for _ in 0..TMP_OPEN_ATTEMPTS {
        let tmp = tmp_path(path);
        match options.open(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => last = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::from(std::io::ErrorKind::AlreadyExists)))
}

/// Writes, fsyncs, and closes the temporary file.
fn finish_tmp(mut file: File, tmp: &Path, bytes: &[u8]) -> std::io::Result<()> {
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    tighten_0600(tmp)
}

/// Removes `<name>.tmp.*` files in `dir` last modified before `now - STALE_TMP_AGE`.
fn sweep_stale_tmp(dir: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let Some(cutoff) = now.checked_sub(STALE_TMP_AGE) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_str().is_some_and(|name| name.contains(TMP_MARKER)) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| modified < cutoff);
        if stale && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Removes `*.json` session documents in `dir` whose `created_at` plus
/// `retention` is not after `now` (the `is_expired` rule with a longer
/// lifetime: the TTL plus a grace period of one more TTL). Best effort: anything that
/// cannot be read or parsed as a document with a numeric `created_at` stays.
fn sweep_expired_sessions(dir: &Path, now: SystemTime, retention: Duration) {
    let Ok(now_ms) = unix_ms(now) else {
        return;
    };
    let retention_ms = u64::try_from(retention.as_millis()).unwrap_or(u64::MAX);
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_session = path.extension().and_then(|ext| ext.to_str()) == Some(SESSION_EXT)
            && !entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.contains(TMP_MARKER));
        if !is_session || !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let Some(created_at) = created_at_of(&path) else {
            continue;
        };
        if now_ms >= created_at.saturating_add(retention_ms) {
            let _ = fs::remove_file(&path);
        }
    }
}

/// The `created_at` of the session document at `path`, when it has one.
fn created_at_of(path: &Path) -> Option<u64> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(MAX_SESSION_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    if u64::try_from(bytes.len()).ok()? > MAX_SESSION_BYTES {
        return None;
    }
    serde_json::from_slice::<Value>(&bytes)
        .ok()?
        .get("created_at")?
        .as_u64()
}

/// Syncs the directory entry after a rename so the new file survives a crash.
fn sync_parent(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            File::open(parent)?.sync_all()?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn tighten_0600(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct Prior<'a> {
    backend: BackendId,
    answers: &'a BTreeMap<QuestionId, QuestionAnswer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionDocument {
    version: u64,
    session_id: SessionId,
    backend: BackendId,
    created_at: u64,
    payload_utf8: String,
    prompt_sha256: sc_agent_types::Sha256Digest,
    decision_schema: DecisionSchemaId,
    system_prompt: Option<String>,
    outcomes: Vec<JudgeOutcome>,
    follow_ups: Vec<String>,
}

impl StoredSession {
    fn from_document(document: SessionDocument, path: &Path) -> Result<Self, JevError> {
        let payload = PayloadBundle::decode(document.payload_utf8.as_bytes())
            .map_err(|_| session_parse(path, "holds a payload that is not canonical"))?;
        for outcome in &document.outcomes {
            outcome.check_against(&payload).map_err(|_| {
                session_parse(path, "holds an outcome that does not match its payload")
            })?;
        }
        Ok(Self {
            session_id: document.session_id,
            backend: document.backend,
            created_at: system_time_from_unix_ms(document.created_at),
            payload,
            system_prompt: document.system_prompt,
            outcomes: document.outcomes,
            follow_ups: document.follow_ups,
        })
    }
}

#[cfg(test)]
mod tests;
