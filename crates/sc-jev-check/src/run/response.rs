//! The one run result type and its one serialization (ADR-SAG-0024,
//! ADR-SAG-0014 r-2 amendment, REQ-SAG-0620, REQ-SAG-0710).
//!
//! [`RunResponse`]'s hand-written `Serialize` **is** the `data` body every
//! consumer prints. [`Envelope`] wraps it as `{version, operation, data}`.

#![warn(missing_docs)]

use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};
use serde_json::Value;

use sc_agent_types::{
    BackendFamily, BackendId, JevError, ScAgentErrorResponse, SessionId, Sha256Digest,
};
use sc_decision_schema::{EqualInputRecord, JudgeOutcome};

/// Envelope `version` field.
pub const ENVELOPE_VERSION: &str = "1";

/// Operation name of a live run (`sc-ai jev run`).
pub const OP_RUN: &str = "jev.run";

/// Operation name of a dry run (`sc-ai jev dry-run`).
pub const OP_DRY_RUN: &str = "jev.dry-run";

/// Operation name of a session follow-up (`sc-ai jev ask`).
pub const OP_ASK: &str = "jev.ask";

/// Keys the response body writes itself.
///
/// A Jev primary's `provider_meta` field with one of these names is not
/// copied to the top level, so no key is written twice; the full
/// `provider_meta` stays under `results[].outcome`. `readback` is the one
/// key [`Envelope::with_readback`] adds.
const BODY_KEYS: &[&str] = &[
    "answers",
    "primary_backend",
    "session_id",
    "session_saved",
    "prompt_sha256",
    "questions_sha256",
    "payload_bytes",
    "results",
    "equal_input",
    "payload",
    "limits",
    "warnings",
    "readback",
];

/// Result of one run: per-backend results plus run-level facts.
///
/// Fields are private; read them through the accessors. The `Serialize`
/// impl is the ADR-SAG-0014 `data` body, in this key order:
///
/// 1. the **primary backend**'s answer fields: the first [`BackendStatus::Ok`]
///    result in `BackendSet` order. For Jev, every `provider_meta` field
///    except `answers` (and except the body's own keys) is copied verbatim,
///    then `answers` once from the typed answers. For an LLM backend,
///    `answers`, then `model` and `usage` when its `provider_meta` has them;
/// 2. `primary_backend` (omitted when no backend returned `Ok`, as on a dry
///    run);
/// 3. `session_id` (null unless `session_saved`), `session_saved`,
///    `prompt_sha256`, `questions_sha256` (the sc-sha digest of the question
///    set alone, for a consumer's cache lookup; bytes in
///    `tests/fixtures/README.md`), `payload_bytes`, `results`,
///    `equal_input`;
/// 4. on a dry run only, `payload` (the canonical payload object) and
///    `limits`;
/// 5. `warnings`, omitted when empty.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
///
/// use sc_agent_types::RepoRoot;
/// use sc_agent_types::observability::v2::NoopSessionLog;
/// use sc_jev_check::run::{CheckSource, Envelope, RunConfig, prepare, run};
///
/// let repo = RepoRoot::new(".");
/// let source = CheckSource::check_file(&repo, std::path::Path::new("check.toml"))?;
/// let config = RunConfig::new(repo, Arc::new(NoopSessionLog)).dry_run(true);
/// let response = run(prepare(source, config)?)?;
/// let body = serde_json::to_string(&Envelope::run(&response))?;
/// assert!(body.contains("\"prompt_sha256\""));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct RunResponse {
    session_id: Option<SessionId>,
    session_saved: bool,
    prompt_sha256: Sha256Digest,
    questions_sha256: Sha256Digest,
    payload_bytes: u64,
    dry_run: bool,
    equal_input: EqualInputRecord,
    results: Vec<BackendResult>,
    warnings: Vec<String>,
    payload: Option<Value>,
    limits: Vec<BackendLimit>,
}

/// Run-level facts shared by a live and a dry response.
pub(crate) struct ResponseParts {
    pub(crate) session_id: Option<SessionId>,
    pub(crate) questions_sha256: Sha256Digest,
    pub(crate) payload_bytes: u64,
    pub(crate) equal_input: EqualInputRecord,
    pub(crate) results: Vec<BackendResult>,
    pub(crate) payload: Option<Value>,
    pub(crate) limits: Vec<BackendLimit>,
}

impl RunResponse {
    /// Builds a response; `session_saved` is `session_id.is_some()` and
    /// `dry_run` is `payload.is_some()`.
    pub(crate) fn from_parts(parts: ResponseParts) -> Self {
        Self {
            session_saved: parts.session_id.is_some(),
            session_id: parts.session_id,
            prompt_sha256: parts.equal_input.prompt_sha256().clone(),
            questions_sha256: parts.questions_sha256,
            payload_bytes: parts.payload_bytes,
            dry_run: parts.payload.is_some(),
            equal_input: parts.equal_input,
            results: parts.results,
            warnings: Vec::new(),
            payload: parts.payload,
            limits: parts.limits,
        }
    }

    /// Saved session id. `None` when [`Self::session_saved`] is false.
    #[must_use]
    pub fn session_id(&self) -> Option<SessionId> {
        self.session_id
    }

    /// True when the run wrote a session file (an `ask`-capable backend
    /// returned `Ok`).
    #[must_use]
    pub fn session_saved(&self) -> bool {
        self.session_saved
    }

    /// SHA-256 of the canonical payload every backend received.
    #[must_use]
    pub fn prompt_sha256(&self) -> &Sha256Digest {
        &self.prompt_sha256
    }

    /// SHA-256 (sc-sha) of the question set alone, for a consumer's cache
    /// lookup. Unlike [`Self::prompt_sha256`] it does not cover the state, and
    /// it covers the criteria descriptions. sc-agent builds no cache.
    #[must_use]
    pub fn questions_sha256(&self) -> &Sha256Digest {
        &self.questions_sha256
    }

    /// Size of the canonical payload in bytes.
    #[must_use]
    pub fn payload_bytes(&self) -> u64 {
        self.payload_bytes
    }

    /// True for a dry run: nothing was sent.
    #[must_use]
    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    /// Equal-input record of this run.
    #[must_use]
    pub fn equal_input(&self) -> &EqualInputRecord {
        &self.equal_input
    }

    /// One result per backend that was judged, in `BackendSet` order.
    /// Empty on a dry run.
    #[must_use]
    pub fn results(&self) -> &[BackendResult] {
        &self.results
    }

    /// Warnings, in the order they were added.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Canonical payload object. `Some` only on a dry run.
    #[must_use]
    pub fn payload(&self) -> Option<&Value> {
        self.payload.as_ref()
    }

    /// Per-backend request limits. Non-empty only on a dry run.
    #[must_use]
    pub fn limits(&self) -> &[BackendLimit] {
        &self.limits
    }

    /// The primary result: the first `Ok` result in `BackendSet` order.
    #[must_use]
    pub fn primary(&self) -> Option<&JudgeOutcome> {
        self.results
            .iter()
            .find_map(|result| result.status.outcome())
    }

    /// Outcomes of every `Ok` result, in `BackendSet` order.
    pub fn outcomes(&self) -> impl Iterator<Item = &JudgeOutcome> + '_ {
        self.results
            .iter()
            .filter_map(|result| result.status.outcome())
    }

    /// The first failed backend, in `BackendSet` order.
    ///
    /// Consumers print its `response()` on stderr when [`Self::is_partial`]
    /// is true.
    #[must_use]
    pub fn first_failure(&self) -> Option<&BackendFailure> {
        self.results.iter().find_map(|result| match &result.status {
            BackendStatus::Failed(failure) => Some(failure),
            BackendStatus::Ok(_) => None,
        })
    }

    /// True when at least one backend failed.
    ///
    /// The one exit / exception decision for every consumer: print the
    /// envelope, report [`Self::first_failure`], and exit 2 (CLI) or raise
    /// (Python). `run` produces a partial response when some, not all,
    /// backends failed (ADR-SAG-0025).
    #[must_use]
    pub fn is_partial(&self) -> bool {
        self.first_failure().is_some()
    }

    /// Appends `warning`. This is the one way a consumer adds data to the
    /// body (ADR-SAG-0024).
    #[must_use]
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}

impl Serialize for RunResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        self.write_entries(&mut map)?;
        map.end()
    }
}

impl RunResponse {
    /// Writes the body's entries, in the documented order, into `map`.
    fn write_entries<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        if let Some(primary) = self.primary() {
            write_answer_fields(map, primary)?;
            map.serialize_entry("primary_backend", &primary.backend())?;
        }
        map.serialize_entry("session_id", &self.session_id)?;
        map.serialize_entry("session_saved", &self.session_saved)?;
        map.serialize_entry("prompt_sha256", &self.prompt_sha256)?;
        map.serialize_entry("questions_sha256", &self.questions_sha256)?;
        map.serialize_entry("payload_bytes", &self.payload_bytes)?;
        map.serialize_entry("results", &self.results)?;
        map.serialize_entry("equal_input", &self.equal_input)?;
        if self.dry_run {
            map.serialize_entry("payload", &self.payload)?;
            map.serialize_entry("limits", &self.limits)?;
        }
        if !self.warnings.is_empty() {
            map.serialize_entry("warnings", &self.warnings)?;
        }
        Ok(())
    }
}

/// The result of [`crate::run::ask`]: the follow-up's outcome and the
/// session facts the ask envelope prints.
///
/// Serialized as the `ask` `data` body: the outcome's answer fields by the
/// run body's primary-backend rule, then `session_id`, `prompt_sha256` (the
/// session's payload digest), and `warnings` (omitted when empty).
#[derive(Debug, Clone, PartialEq)]
pub struct AskResponse {
    outcome: JudgeOutcome,
    session_id: SessionId,
    prompt_sha256: Sha256Digest,
    warnings: Vec<String>,
}

impl AskResponse {
    pub(crate) fn new(
        outcome: JudgeOutcome,
        session_id: SessionId,
        prompt_sha256: Sha256Digest,
    ) -> Self {
        Self {
            outcome,
            session_id,
            prompt_sha256,
            warnings: Vec::new(),
        }
    }

    /// The follow-up's outcome.
    #[must_use]
    pub fn outcome(&self) -> &JudgeOutcome {
        &self.outcome
    }

    /// The session that was asked.
    #[must_use]
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// SHA-256 of the session's canonical payload.
    #[must_use]
    pub fn prompt_sha256(&self) -> &Sha256Digest {
        &self.prompt_sha256
    }

    /// Warnings, in the order they were added.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Appends `warning` (for example a log-flush failure, CR-014); the one
    /// way a consumer adds data to the `ask` body.
    #[must_use]
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }

    fn write_entries<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        write_answer_fields(map, &self.outcome)?;
        map.serialize_entry("session_id", &self.session_id)?;
        map.serialize_entry("prompt_sha256", &self.prompt_sha256)?;
        if !self.warnings.is_empty() {
            map.serialize_entry("warnings", &self.warnings)?;
        }
        Ok(())
    }
}

impl Serialize for AskResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        self.write_entries(&mut map)?;
        map.end()
    }
}

/// Writes one outcome's answer fields at the top level (ADR-SAG-0014).
///
/// Jev: `provider_meta` fields except the body's own keys, verbatim, then
/// `answers` once from the typed answers. LLM: `answers`, then `model` and
/// `usage` when present in `provider_meta`.
fn write_answer_fields<M: SerializeMap>(
    map: &mut M,
    outcome: &JudgeOutcome,
) -> Result<(), M::Error> {
    let meta = outcome.provider_meta().and_then(Value::as_object);
    if outcome.backend().family() == BackendFamily::TypeSafeJev {
        for (key, value) in meta.into_iter().flatten() {
            if !BODY_KEYS.contains(&key.as_str()) {
                map.serialize_entry(key, value)?;
            }
        }
        return map.serialize_entry("answers", outcome.answers());
    }
    map.serialize_entry("answers", outcome.answers())?;
    for key in ["model", "usage"] {
        if let Some(value) = meta.and_then(|meta| meta.get(key)) {
            map.serialize_entry(key, value)?;
        }
    }
    Ok(())
}

/// One backend's result.
///
/// Serialized as `{"backend", "status": "ok", "outcome"}` or
/// `{"backend", "status": "failed", "error"}`.
#[derive(Debug, Clone, PartialEq)]
pub struct BackendResult {
    backend: BackendId,
    status: BackendStatus,
}

impl BackendResult {
    /// An `Ok` result carrying `outcome`; the backend is the outcome's.
    #[must_use]
    pub fn ok(outcome: JudgeOutcome) -> Self {
        Self {
            backend: outcome.backend(),
            status: BackendStatus::Ok(outcome),
        }
    }

    /// A `Failed` result of `backend` carrying `error`.
    #[must_use]
    pub fn failed(backend: BackendId, error: JevError) -> Self {
        Self {
            backend,
            status: BackendStatus::Failed(BackendFailure::new(error)),
        }
    }

    /// The backend this result belongs to.
    #[must_use]
    pub fn backend(&self) -> BackendId {
        self.backend
    }

    /// Its status.
    #[must_use]
    pub fn status(&self) -> &BackendStatus {
        &self.status
    }
}

impl Serialize for BackendResult {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("backend", &self.backend)?;
        match &self.status {
            BackendStatus::Ok(outcome) => {
                map.serialize_entry("status", "ok")?;
                map.serialize_entry("outcome", outcome)?;
            }
            BackendStatus::Failed(failure) => {
                map.serialize_entry("status", "failed")?;
                map.serialize_entry("error", &failure.response())?;
            }
        }
        map.end()
    }
}

/// Whether one backend answered.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BackendStatus {
    /// The backend answered.
    Ok(JudgeOutcome),
    /// The backend failed; the other backends' answers are kept.
    Failed(BackendFailure),
}

impl BackendStatus {
    /// The outcome when the status is `Ok`.
    #[must_use]
    pub fn outcome(&self) -> Option<&JudgeOutcome> {
        match self {
            Self::Ok(outcome) => Some(outcome),
            Self::Failed(_) => None,
        }
    }
}

/// One backend's failure.
///
/// Stores **only** the [`JevError`]. Its serialized `error` object is
/// derived when it is read or serialized, as
/// [`ScAgentErrorResponse::from_jev_error`], so the two can never diverge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendFailure {
    error: JevError,
}

impl BackendFailure {
    /// Wraps `error`.
    #[must_use]
    pub fn new(error: JevError) -> Self {
        Self { error }
    }

    /// The error.
    #[must_use]
    pub fn error(&self) -> &JevError {
        &self.error
    }

    /// The error response derived from [`Self::error`].
    #[must_use]
    pub fn response(&self) -> ScAgentErrorResponse {
        ScAgentErrorResponse::from_jev_error(&self.error)
    }
}

/// One selected backend's request limit, reported by a dry run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BackendLimit {
    backend: BackendId,
    request_limit_bytes: u64,
    over_limit: bool,
}

impl BackendLimit {
    /// The limit of `backend` (its descriptor's `request_limit_bytes`) for a
    /// payload of `payload_bytes`.
    #[must_use]
    pub fn for_payload(backend: BackendId, payload_bytes: u64) -> Self {
        let request_limit_bytes = backend.descriptor().request_limit_bytes();
        Self {
            backend,
            request_limit_bytes,
            over_limit: payload_bytes > request_limit_bytes,
        }
    }

    /// The backend.
    #[must_use]
    pub fn backend(&self) -> BackendId {
        self.backend
    }

    /// The backend's request byte limit.
    #[must_use]
    pub fn request_limit_bytes(&self) -> u64 {
        self.request_limit_bytes
    }

    /// True when the payload is larger than the limit; a live run would be
    /// refused by this backend before any send.
    #[must_use]
    pub fn over_limit(&self) -> bool {
        self.over_limit
    }
}

/// The versioned stdout envelope `{version, operation, data}` (ADR-SAG-0014).
///
/// Built from a [`RunResponse`] ([`Self::run`]) or an [`AskResponse`]
/// ([`Self::ask`]); [`Self::with_readback`] is the only key a consumer adds.
/// Construction cannot fail: the body is kept as typed data and written
/// when the envelope is serialized, so `data`'s keys come out in the body's
/// documented order, followed by `readback`.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    operation: &'static str,
    body: Body,
    readback: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
enum Body {
    Run(Box<RunResponse>),
    Ask(Box<AskResponse>),
}

impl Envelope {
    /// The envelope of a run: `data` is `response`'s serialization and
    /// `operation` is [`OP_RUN`] or, for a dry run, [`OP_DRY_RUN`].
    #[must_use]
    pub fn run(response: &RunResponse) -> Self {
        let operation = if response.dry_run() {
            OP_DRY_RUN
        } else {
            OP_RUN
        };
        Self {
            operation,
            body: Body::Run(Box::new(response.clone())),
            readback: None,
        }
    }

    /// The envelope of a session follow-up ([`OP_ASK`]): `data` is
    /// `response`'s serialization.
    #[must_use]
    pub fn ask(response: &AskResponse) -> Self {
        Self {
            operation: OP_ASK,
            body: Body::Ask(Box::new(response.clone())),
            readback: None,
        }
    }

    /// Adds `readback` as `data.readback`, the one consumer-added key.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when `readback` cannot be encoded as JSON.
    pub fn with_readback(mut self, readback: impl Serialize) -> Result<Self, JevError> {
        let value = serde_json::to_value(readback)
            .map_err(|error| JevError::encode_failed("readback", &error))?;
        self.readback = Some(value);
        Ok(self)
    }

    /// Envelope format version ([`ENVELOPE_VERSION`]).
    #[must_use]
    pub fn version(&self) -> &str {
        ENVELOPE_VERSION
    }

    /// Operation name.
    #[must_use]
    pub fn operation(&self) -> &str {
        self.operation
    }
}

impl Serialize for Envelope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Data<'a>(&'a Envelope);
        impl Serialize for Data<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(None)?;
                match &self.0.body {
                    Body::Run(response) => response.write_entries(&mut map)?,
                    Body::Ask(response) => response.write_entries(&mut map)?,
                }
                if let Some(readback) = &self.0.readback {
                    map.serialize_entry("readback", readback)?;
                }
                map.end()
            }
        }
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("version", ENVELOPE_VERSION)?;
        map.serialize_entry("operation", self.operation)?;
        map.serialize_entry("data", &Data(self))?;
        map.end()
    }
}
