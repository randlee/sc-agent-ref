//! Injectable session log aligned with `sc-observability` v2.
//!
//! Libraries take [`SharedSessionLog`]. Composition roots adapt a real v2
//! logger to this trait. The rejection type is the local [`LogRejected`].

use std::fmt;
use std::sync::Arc;

use crate::ids::{Operation, SessionId};

/// A [`SessionLog`] refused an event.
///
/// # Examples
///
/// ```
/// use sc_agent_types::observability::v2::LogRejected;
///
/// let rejected = LogRejected::new("queue full");
/// assert_eq!(rejected.to_string(), "session log rejected the event: queue full");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRejected {
    reason: String,
}

impl LogRejected {
    /// A rejection with a human-readable `reason` (no secrets).
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// Why the event was rejected.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for LogRejected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "session log rejected the event: {}", self.reason)
    }
}

impl std::error::Error for LogRejected {}

/// One prompt or judge send.
///
/// The event carries the session id, the [`Operation`], and the `fields{}`
/// object the log line records: one ADR-SAG-0031 session-log row. The port
/// does not name the row type; the composition crates build `fields` and the
/// adapter copies them into the log line verbatim. Secrets never appear in
/// `fields` (NFR-SAG-0002).
///
/// # Examples
///
/// ```
/// use sc_agent_types::observability::v2::LogEvent;
/// use sc_agent_types::{Operation, SessionId};
///
/// let mut fields = serde_json::Map::new();
/// fields.insert("backend".to_owned(), "jev".into());
/// let event = LogEvent::new(SessionId::new(), Operation::Judge, fields);
/// assert_eq!(event.fields()["backend"], "jev");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct LogEvent {
    session_id: SessionId,
    operation: Operation,
    fields: serde_json::Map<String, serde_json::Value>,
}

impl LogEvent {
    /// An event for one send.
    #[must_use]
    pub fn new(
        session_id: SessionId,
        operation: Operation,
        fields: serde_json::Map<String, serde_json::Value>,
    ) -> Self {
        Self {
            session_id,
            operation,
            fields,
        }
    }

    /// Session the send belongs to.
    #[must_use]
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Which send this is.
    #[must_use]
    pub fn operation(&self) -> Operation {
        self.operation
    }

    /// The row the log line records, as a JSON object.
    #[must_use]
    pub fn fields(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.fields
    }
}

/// Best-effort session logger.
///
/// This is an **open port** (not sealed): composition roots (`sc-ai`,
/// `sc-jev-py`) adapt their own `sc-observability` logger to it and tests
/// inject doubles, so third-party implementations are expected.
/// Implementations return promptly and do not panic.
pub trait SessionLog: Send + Sync {
    /// Records `event`.
    ///
    /// # Errors
    ///
    /// Returns [`LogRejected`] when the receiver rejects the event. Callers
    /// treat that as best effort.
    fn emit(&self, event: LogEvent) -> Result<(), LogRejected>;
}

/// Shared session log injected into `sc-jev-check`.
pub type SharedSessionLog = Arc<dyn SessionLog>;

/// Discards every event.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSessionLog;

impl SessionLog for NoopSessionLog {
    fn emit(&self, _event: LogEvent) -> Result<(), LogRejected> {
        Ok(())
    }
}
