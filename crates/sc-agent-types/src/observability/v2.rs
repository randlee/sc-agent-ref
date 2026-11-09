//! Injectable session and general logs aligned with `sc-observability` v2.
//!
//! There are two logs, written to separate files. The session log
//! ([`SessionLog`]) records one row per judge send. The general log
//! ([`GeneralLog`]) records what an operator needs to diagnose a run: start
//! and end of operations, backend and model selection, path resolution,
//! retries, failures. Libraries take [`SharedSessionLog`] and
//! [`SharedGeneralLog`]. Composition roots adapt a real v2 logger to these
//! traits. The rejection type is the local [`LogRejected`].

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

/// Severity of a [`GeneralEvent`], lowest to highest.
///
/// # Examples
///
/// ```
/// use sc_agent_types::observability::v2::GeneralLevel;
///
/// assert!(GeneralLevel::Debug < GeneralLevel::Warn);
/// assert_eq!(GeneralLevel::Info.as_str(), "info");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum GeneralLevel {
    /// Fine-grained detail for deep diagnosis.
    Trace,
    /// Resolved values and step boundaries.
    Debug,
    /// Start and end of an operation.
    Info,
    /// Something degraded but the operation continued.
    Warn,
    /// An operation failed.
    Error,
}

impl GeneralLevel {
    /// The lower-case level name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for GeneralLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One general-log record: a level, a stable `target`, a message and
/// structured `fields`.
///
/// `target` names the emitting area (for example `"sc_jev_check::run"`) and
/// `action` is a short stable verb phrase (`"run.start"`). The message and
/// fields never carry secrets, API keys, prompt text, session state or
/// provider bodies (NFR-SAG-0002).
///
/// # Examples
///
/// ```
/// use sc_agent_types::observability::v2::{GeneralEvent, GeneralLevel};
///
/// let event = GeneralEvent::new(GeneralLevel::Info, "sc_jev_check::run", "run.start", "run started")
///     .with_field("backends", 2);
/// assert_eq!(event.action(), "run.start");
/// assert_eq!(event.fields()["backends"], 2);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct GeneralEvent {
    level: GeneralLevel,
    target: &'static str,
    action: &'static str,
    message: String,
    fields: serde_json::Map<String, serde_json::Value>,
}

impl GeneralEvent {
    /// An event with no fields.
    #[must_use]
    pub fn new(
        level: GeneralLevel,
        target: &'static str,
        action: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level,
            target,
            action,
            message: message.into(),
            fields: serde_json::Map::new(),
        }
    }

    /// Adds one structured field.
    #[must_use]
    pub fn with_field(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.fields.insert(key.to_owned(), value.into());
        self
    }

    /// Severity.
    #[must_use]
    pub fn level(&self) -> GeneralLevel {
        self.level
    }

    /// The emitting area.
    #[must_use]
    pub fn target(&self) -> &'static str {
        self.target
    }

    /// The stable action name.
    #[must_use]
    pub fn action(&self) -> &'static str {
        self.action
    }

    /// The human-readable message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The structured fields.
    #[must_use]
    pub fn fields(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.fields
    }
}

/// Best-effort general logger.
///
/// This is an **open port** (not sealed), like [`SessionLog`]: composition
/// roots adapt their own `sc-observability` logger to it and tests inject
/// doubles. Implementations return promptly and do not panic. Callers treat
/// every error as best effort: logging never changes a result.
pub trait GeneralLog: Send + Sync {
    /// Records `event`.
    ///
    /// # Errors
    ///
    /// Returns [`LogRejected`] when the receiver rejects the event.
    fn log(&self, event: GeneralEvent) -> Result<(), LogRejected>;
}

/// Shared general log injected into the library crates.
pub type SharedGeneralLog = Arc<dyn GeneralLog>;

/// Discards every event. The library default when no logger is injected.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopGeneralLog;

impl GeneralLog for NoopGeneralLog {
    fn log(&self, _event: GeneralEvent) -> Result<(), LogRejected> {
        Ok(())
    }
}
