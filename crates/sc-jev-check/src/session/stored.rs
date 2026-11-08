//! The in-memory form of one session document.

use std::time::SystemTime;

use sc_agent_types::{BackendId, SessionId};
use sc_decision_schema::{JudgeOutcome, PayloadBundle};

/// One session document (public under the `test-util` feature).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSession {
    /// File id.
    pub session_id: SessionId,
    /// Backend `ask` will call.
    pub backend: BackendId,
    /// Creation instant. Expiry is measured from this value.
    pub created_at: SystemTime,
    /// Canonical payload bytes and their digest.
    pub payload: PayloadBundle,
    /// Base LLM system prompt. `None` for a session with no LLM backend.
    pub system_prompt: Option<String>,
    /// Prior judge outcomes, in call order.
    pub outcomes: Vec<JudgeOutcome>,
    /// Follow-up texts included before a new one when the session is asked.
    ///
    /// `ask` does not append to this list or rewrite the file; a caller
    /// that keeps a conversation saves a new document itself.
    pub follow_ups: Vec<String>,
}
