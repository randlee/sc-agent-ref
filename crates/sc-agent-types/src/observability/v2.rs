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

    /// An event at [`GeneralLevel::Trace`].
    #[must_use]
    pub fn trace(target: &'static str, action: &'static str, message: impl Into<String>) -> Self {
        Self::new(GeneralLevel::Trace, target, action, message)
    }

    /// An event at [`GeneralLevel::Debug`].
    #[must_use]
    pub fn debug(target: &'static str, action: &'static str, message: impl Into<String>) -> Self {
        Self::new(GeneralLevel::Debug, target, action, message)
    }

    /// An event at [`GeneralLevel::Info`].
    #[must_use]
    pub fn info(target: &'static str, action: &'static str, message: impl Into<String>) -> Self {
        Self::new(GeneralLevel::Info, target, action, message)
    }

    /// An event at [`GeneralLevel::Warn`].
    #[must_use]
    pub fn warn(target: &'static str, action: &'static str, message: impl Into<String>) -> Self {
        Self::new(GeneralLevel::Warn, target, action, message)
    }

    /// An event at [`GeneralLevel::Error`].
    #[must_use]
    pub fn error(target: &'static str, action: &'static str, message: impl Into<String>) -> Self {
        Self::new(GeneralLevel::Error, target, action, message)
    }

    /// Adds the typed error id (`JevError::code`) as the `error_code` field.
    ///
    /// Only the id is recorded, never the error text, which can echo
    /// provider replies.
    #[must_use]
    pub fn with_error_code(self, code: &str) -> Self {
        self.with_field("error_code", code)
    }

    /// Sends the event to `log` and drops a rejection.
    ///
    /// Logging is best effort and never changes a result: a rejected event is
    /// ignored.
    pub fn emit(self, log: &dyn GeneralLog) {
        // Best effort by contract (NFR-SAG-0002): a rejection has no caller
        // that could act on it.
        let _ = log.log(self);
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

/// Keeps every event in memory so a test can assert on what was logged.
///
/// A test double for embedders and the workspace's own tests. Clones share
/// one buffer.
///
/// # Examples
///
/// ```
/// use sc_agent_types::observability::v2::{GeneralEvent, GeneralLog, RecordingGeneralLog};
///
/// let log = RecordingGeneralLog::new();
/// GeneralEvent::info("t", "a.b", "hello").emit(&log);
/// assert_eq!(log.events().len(), 1);
/// assert!(log.rendered().contains("hello"));
/// ```
#[derive(Debug, Default, Clone)]
pub struct RecordingGeneralLog {
    events: Arc<std::sync::Mutex<Vec<GeneralEvent>>>,
}

impl RecordingGeneralLog {
    /// An empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The events recorded so far, in order.
    #[must_use]
    pub fn events(&self) -> Vec<GeneralEvent> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Every recorded event as one debug string: for a leak check.
    #[must_use]
    pub fn rendered(&self) -> String {
        format!("{:?}", self.events())
    }

    /// The actions recorded so far, in order.
    #[must_use]
    pub fn actions(&self) -> Vec<&'static str> {
        self.events().iter().map(GeneralEvent::action).collect()
    }
}

impl GeneralLog for RecordingGeneralLog {
    fn log(&self, event: GeneralEvent) -> Result<(), LogRejected> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
        Ok(())
    }
}

/// Rejects every event: for proving that a failing log changes nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct RejectingGeneralLog;

impl GeneralLog for RejectingGeneralLog {
    fn log(&self, _event: GeneralEvent) -> Result<(), LogRejected> {
        Err(LogRejected::new("rejected by test double"))
    }
}

#[cfg(test)]
mod general_tests {
    use super::*;

    #[test]
    fn event_builder_keeps_level_target_action_message_and_fields() {
        let event = GeneralEvent::warn("t.x", "a.b", "msg")
            .with_field("n", 3)
            .with_error_code("INPUT_INVALID");
        assert_eq!(event.level(), GeneralLevel::Warn);
        assert_eq!(event.target(), "t.x");
        assert_eq!(event.action(), "a.b");
        assert_eq!(event.message(), "msg");
        assert_eq!(event.fields()["n"], 3);
        assert_eq!(event.fields()["error_code"], "INPUT_INVALID");
    }

    #[test]
    fn a_rejecting_log_is_ignored_by_emit() {
        GeneralEvent::info("t", "a", "m").emit(&RejectingGeneralLog);
    }

    #[test]
    fn levels_are_ordered_and_spelled() {
        let levels = [
            GeneralLevel::Trace,
            GeneralLevel::Debug,
            GeneralLevel::Info,
            GeneralLevel::Warn,
            GeneralLevel::Error,
        ];
        assert!(levels.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(GeneralLevel::Error.to_string(), "error");
    }
}

/// A fresh per-run correlation id: a ULID in text form.
///
/// Every event of one run carries it as the `run_id` field (see
/// [`CorrelatedGeneralLog`]), so runs that share a file can be told apart.
#[must_use]
pub fn new_run_id() -> String {
    ulid::Ulid::new().to_string()
}

/// Adds one `run_id` field to every event it forwards.
///
/// Composition roots wrap the general log once per run, so every layer that
/// receives the logger (registry, backends, HTTP) is correlated without
/// knowing the id. An event that already has a `run_id` keeps it.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use sc_agent_types::observability::v2::{
///     CorrelatedGeneralLog, GeneralEvent, GeneralLog, RecordingGeneralLog,
/// };
///
/// let inner = RecordingGeneralLog::new();
/// let log = CorrelatedGeneralLog::new(Arc::new(inner.clone()), "RUN1");
/// GeneralEvent::info("t", "a.b", "m").emit(&log);
/// assert_eq!(inner.events()[0].fields()["run_id"], "RUN1");
/// ```
#[derive(Clone)]
pub struct CorrelatedGeneralLog {
    inner: SharedGeneralLog,
    run_id: String,
}

impl fmt::Debug for CorrelatedGeneralLog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CorrelatedGeneralLog")
            .field("run_id", &self.run_id)
            .finish_non_exhaustive()
    }
}

impl CorrelatedGeneralLog {
    /// Wraps `inner`, stamping `run_id` on every event.
    #[must_use]
    pub fn new(inner: SharedGeneralLog, run_id: impl Into<String>) -> Self {
        Self {
            inner,
            run_id: run_id.into(),
        }
    }

    /// The correlation id.
    #[must_use]
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
}

impl GeneralLog for CorrelatedGeneralLog {
    fn log(&self, event: GeneralEvent) -> Result<(), LogRejected> {
        let event = if event.fields().contains_key("run_id") {
            event
        } else {
            event.with_field("run_id", self.run_id.as_str())
        };
        self.inner.log(event)
    }
}
