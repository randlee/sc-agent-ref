//! `RunResponse::test_builder` (`test-util` feature, plan "Test seams").
//!
//! Kept in its own module: sc-boundary 0.8 reports a builder and the type it
//! builds in one module as a type cycle (SCB-CYCLE-001).

use serde_json::Value;

use sc_agent_types::{BackendId, JevError, SessionId, Sha256Digest};
use sc_decision_schema::{EqualInputRecord, JudgeOutcome};

use super::response::{BackendLimit, BackendResult, ResponseParts, RunResponse};

/// Builds any [`RunResponse`] shape for tests.
///
/// Unlike `run`, the builder can produce `Failed` results, an unsaved
/// session next to an `ask`-capable `Ok` result, or warnings, so consumer
/// tests can pin output shapes that only later sprints produce at runtime.
#[derive(Debug, Clone, Default)]
pub struct RunResponseBuilder {
    session_id: Option<SessionId>,
    payload_bytes: u64,
    results: Vec<BackendResult>,
    warnings: Vec<String>,
    payload: Option<Value>,
    limits: Vec<BackendLimit>,
    questions_sha256: Option<Sha256Digest>,
    request_sha256: Vec<(BackendId, Sha256Digest)>,
}

impl RunResponse {
    /// Starts a [`RunResponseBuilder`] (`test-util` feature only).
    #[must_use]
    pub fn test_builder() -> RunResponseBuilder {
        RunResponseBuilder::default()
    }
}

impl RunResponseBuilder {
    /// Marks the session saved under `id`.
    #[must_use]
    pub fn session_saved(mut self, id: SessionId) -> Self {
        self.session_id = Some(id);
        self
    }

    /// Sets `payload_bytes`.
    #[must_use]
    pub fn payload_bytes(mut self, bytes: u64) -> Self {
        self.payload_bytes = bytes;
        self
    }

    /// Appends an `Ok` result.
    #[must_use]
    pub fn ok(mut self, outcome: JudgeOutcome) -> Self {
        self.results.push(BackendResult::ok(outcome));
        self
    }

    /// Appends a `Failed` result.
    #[must_use]
    pub fn failed(mut self, backend: BackendId, error: JevError) -> Self {
        self.results.push(BackendResult::failed(backend, error));
        self
    }

    /// Appends a warning.
    #[must_use]
    pub fn warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }

    /// Makes the response a dry run carrying `payload` and `limits`.
    #[must_use]
    pub fn dry_run(mut self, payload: Value, limits: Vec<BackendLimit>) -> Self {
        self.payload = Some(payload);
        self.limits = limits;
        self
    }

    /// Sets `questions_sha256`; the default is the digest of empty text.
    #[must_use]
    pub fn questions_sha256(mut self, digest: Sha256Digest) -> Self {
        self.questions_sha256 = Some(digest);
        self
    }

    /// Adds a backend's pre-send `request_sha256`.
    #[must_use]
    pub fn request_sha256(mut self, backend: BackendId, digest: Sha256Digest) -> Self {
        self.request_sha256.push((backend, digest));
        self
    }

    /// Finishes the response. `payload_sha256` is `equal_input`'s.
    #[must_use]
    pub fn build(self, equal_input: EqualInputRecord) -> RunResponse {
        let response = RunResponse::from_parts(ResponseParts {
            session_id: self.session_id,
            request_sha256: self.request_sha256,
            questions_sha256: self
                .questions_sha256
                .unwrap_or_else(|| Sha256Digest::of_text("")),
            payload_bytes: self.payload_bytes,
            equal_input,
            results: self.results,
            payload: self.payload,
            limits: self.limits,
        });
        self.warnings
            .into_iter()
            .fold(response, RunResponse::with_warning)
    }
}
