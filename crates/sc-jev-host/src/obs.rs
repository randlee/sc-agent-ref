//! The `sc-observability` v2 logger adapted to the session-log port.
//!
//! [`ObservabilityAdapter`] is the one adapter both composition roots use
//! (QA-004): it maps a
//! [`LogEvent`](sc_agent_types::observability::v2::LogEvent) to one
//! `sc-observability` event with a fixed action and target (REQ-SAG-0610)
//! and maps `EventError` to [`LogRejected`]. The event's `fields` object is
//! one ADR-SAG-0031 session-log row; the adapter copies it into the v2
//! `fields{}` verbatim (REQ-SAG-0611, REQ-SAG-0617, ADR-SAG-0013: no bespoke
//! writer). The envelope `outcome` stays `null`; the row's own
//! `send_outcome` carries the result.
//!
//! # Writers
//!
//! One logger (one file writer) exists per `(service, canonical log root)`
//! in a process. [`ObservabilityAdapter::open`] reuses a live one from a
//! process-wide registry of weak handles, so repeated runs in one
//! interpreter share the writer. The last handle to drop flushes and shuts it
//! down. Console logging stays off so stderr remains the error channel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use crate::paths::PathPolicy;
use sc_agent_types::observability::v2::{
    LogEvent as SessionEvent, LogRejected, SessionLog, SharedSessionLog,
};
use sc_agent_types::{JevError, Operation};
use sc_observability::v2::{Logger, LoggerBuilder};
use sc_observability::{
    ActionName, Level, LogEvent, LoggerConfig, OBSERVATION_ENVELOPE_VERSION, ProcessIdentity,
    SchemaVersion, ServiceName, TargetCategory, Timestamp,
};

/// Target category recorded on every session-log line.
const TARGET: &str = "jev.session";

/// Owns one running logger. Dropping it flushes and shuts the writer down.
///
/// Shutdown runs exactly once, when the last [`Arc`] goes away, so there is
/// no reference-count race between concurrent drops.
struct LoggerInner(Logger);

impl Drop for LoggerInner {
    fn drop(&mut self) {
        // Best effort: the process is done with this writer and a failure
        // here has no caller left to report to.
        let _ = self.0.flush();
        let _ = self.0.shutdown();
    }
}

/// Live loggers keyed by service and canonical log root.
type Registry = Mutex<HashMap<(String, PathBuf), Weak<LoggerInner>>>;

fn registry() -> &'static Registry {
    static LOGGERS: OnceLock<Registry> = OnceLock::new();
    LOGGERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `sc-observability` v2 logger used as a [`SessionLog`].
///
/// Clones share one writer. Call [`Self::flush`] after a command so the JSONL
/// file is durable before the process prints stdout.
///
/// # Examples
///
/// ```no_run
/// use sc_jev_host::obs::ObservabilityAdapter;
/// use sc_jev_host::paths::PathPolicy;
///
/// let root = std::env::temp_dir().join("example-log");
/// let adapter = ObservabilityAdapter::open(&PathPolicy::cli(), &root)?;
/// let _port = adapter.shared();
/// adapter.flush()?;
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Clone)]
pub struct ObservabilityAdapter {
    inner: Arc<LoggerInner>,
    service: ServiceName,
}

impl std::fmt::Debug for ObservabilityAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ObservabilityAdapter")
            .field("service", &self.service.as_str())
            .finish_non_exhaustive()
    }
}

impl ObservabilityAdapter {
    /// Returns the file logger for `policy.service` rooted at `log_root`.
    ///
    /// A logger already running for the same service and canonical root is
    /// reused; otherwise one is started. The JSONL file is
    /// `<log_root>/logs/<service>.log.jsonl` ([`PathPolicy::jsonl_path`]).
    ///
    /// # Errors
    ///
    /// Returns `CONFIG_INVALID` ([`JevError::operational_config`]) when the
    /// service is not a valid service name, `log_root` cannot be created or
    /// resolved, or the logger refuses the configuration. The fix hint names
    /// the directory and the policy's log-root argument
    /// ([`PathPolicy::log_root_flag`]), and does not mention `check.toml` or
    /// secret material.
    pub fn open(policy: &PathPolicy, log_root: &Path) -> Result<Self, JevError> {
        let service = policy.service;
        let flag = policy.log_root_flag;
        let service_name = ServiceName::new(service).map_err(|error| {
            JevError::operational_config(format!(
                "session log service name {service:?} is not valid ({error}). Use a name of letters, digits, '-' and '_'."
            ))
        })?;
        std::fs::create_dir_all(log_root).map_err(|error| {
            JevError::operational_config(format!(
                "Create log directory {} or pass a writable {flag} ({error}).",
                log_root.display()
            ))
        })?;
        let key = std::fs::canonicalize(log_root).map_err(|error| {
            JevError::operational_config(format!(
                "Resolve log directory {} ({error}). Pass a readable {flag} and retry.",
                log_root.display()
            ))
        })?;
        let mut live = registry().lock().unwrap_or_else(PoisonError::into_inner);
        live.retain(|_, weak| weak.strong_count() > 0);
        let registry_key = (service.to_owned(), key.clone());
        if let Some(inner) = live.get(&registry_key).and_then(Weak::upgrade) {
            return Ok(Self {
                inner,
                service: service_name,
            });
        }
        let inner = Arc::new(LoggerInner(start_logger(service_name.clone(), &key, flag)?));
        live.insert(registry_key, Arc::downgrade(&inner));
        Ok(Self {
            inner,
            service: service_name,
        })
    }

    /// This adapter as the port libraries take.
    #[must_use]
    pub fn shared(&self) -> SharedSessionLog {
        Arc::new(self.clone())
    }

    /// Flushes admitted events to the JSONL sink.
    ///
    /// # Errors
    ///
    /// Returns `CONFIG_INVALID` when the sink cannot flush. The hint tells
    /// the caller to check the log root and does not mention `check.toml`.
    pub fn flush(&self) -> Result<(), JevError> {
        self.inner.0.flush().map_err(|error| {
            JevError::operational_config(format!(
                "session log flush failed ({}). Check that the log root is writable.",
                error.diagnostic().message
            ))
        })
    }

    /// Whether `self` and `other` write through the same logger.
    #[must_use]
    pub fn shares_writer_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// The `sc-observability` event that `event` becomes.
    ///
    /// The conversion is public so the golden test can pin the exact action,
    /// target and fields (REQ-SAG-0611). `fields` is the event's row, cloned
    /// unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`LogRejected`] when the operation spelling is not a valid
    /// action name; the event is rejected, never a panic.
    pub fn log_event(&self, event: &SessionEvent) -> Result<LogEvent, LogRejected> {
        Ok(LogEvent {
            version: SchemaVersion::new(OBSERVATION_ENVELOPE_VERSION)
                .map_err(|_| LogRejected::new("observation envelope version is not valid"))?,
            timestamp: Timestamp::now_utc(),
            level: Level::Info,
            service: self.service.clone(),
            target: TargetCategory::new(TARGET)
                .map_err(|_| LogRejected::new("session log target is not valid"))?,
            action: action_name(event.operation())?,
            message: None,
            identity: ProcessIdentity::default(),
            trace: None,
            request_id: None,
            correlation_id: None,
            outcome: None,
            diagnostic: None,
            state_transition: None,
            fields: event.fields().clone(),
        })
    }
}

impl SessionLog for ObservabilityAdapter {
    fn emit(&self, event: SessionEvent) -> Result<(), LogRejected> {
        self.inner
            .0
            .log(self.log_event(&event)?)
            .map_err(|error| LogRejected::new(error.to_string()))
    }
}

fn start_logger(service: ServiceName, log_root: &Path, flag: &str) -> Result<Logger, JevError> {
    let mut config = LoggerConfig::default_for(service, log_root.to_path_buf());
    config.enable_console_sink = false;
    config.enable_file_sink = true;
    LoggerBuilder::new(config)
        .and_then(LoggerBuilder::build)
        .map_err(|error| {
            JevError::operational_config(format!(
                "Start the session logger at {} ({}). Pass a writable {flag} and retry.",
                log_root.display(),
                error.diagnostic().message
            ))
        })
}

/// A spelling that is not a valid action name is a rejected event, never a
/// panic. The test `every_operation_is_an_action_name` proves every
/// [`Operation`] is valid.
fn action_name(operation: Operation) -> Result<ActionName, LogRejected> {
    ActionName::new(operation.as_str()).map_err(|_| {
        LogRejected::new(format!(
            "operation {operation} is not a valid sc-observability action name"
        ))
    })
}
