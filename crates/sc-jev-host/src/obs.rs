//! The `sc-observability` v2 logger adapted to the session-log and
//! general-log ports.
//!
//! Two adapters write two separate files under one log root:
//! [`ObservabilityAdapter`] is the session log
//! (`<log_root>/logs/<service>.log.jsonl`, one ADR-SAG-0031 row per send) and
//! [`GeneralLogAdapter`] is the general log
//! (`<log_root>/logs/<service>-general.log.jsonl`, operator diagnostics,
//! ADR-SAG-0013 amendment 2026-11-09). Both are the sc-agent defaults a root
//! uses when the caller injects no logger.
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

use serde_json::Value;

use crate::paths::PathPolicy;
use sc_agent_types::observability::v2::{
    GeneralEvent, GeneralLevel, GeneralLog, LogEvent as SessionEvent, LogRejected, SessionLog,
    SharedGeneralLog, SharedSessionLog,
};
use sc_agent_types::{JevError, Operation};
use sc_observability::v2::{Logger, LoggerBuilder};
use sc_observability::{
    ActionName, Level, LogEvent, LoggerConfig, OBSERVATION_ENVELOPE_VERSION, ProcessIdentity,
    SchemaVersion, ServiceName, TargetCategory, Timestamp,
};

/// Target category recorded on every session-log line.
const TARGET: &str = "jev.session";

/// Suffix that turns a policy's service into the general log's service.
const GENERAL_SUFFIX: &str = "-general";

/// An envelope id from ULID text. A ULID is always a valid id, so a `None`
/// here is a bug: it fails loudly in debug builds and drops the id in release.
/// A macro, not a generic function, because the id type is only nameable
/// through the `LogEvent` field it is assigned to (`serde` is not a direct
/// dependency, so there is no `DeserializeOwned` bound to write).
macro_rules! envelope_id {
    ($text:expr) => {{
        let text: &str = $text;
        let id = serde_json::from_value(Value::String(text.to_owned())).ok();
        debug_assert!(id.is_some(), "{text:?} is not a valid envelope id");
        id
    }};
}

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
    /// Returns `CONFIG_INVALID` when the service is not a valid service name
    /// or the logger refuses the configuration, and `IO_FAILED` when
    /// `log_root` cannot be created or resolved. The suggested action names
    /// the policy's log-root argument ([`PathPolicy::log_root_flag`]), and
    /// does not mention `check.toml` or secret material.
    pub fn open(policy: &PathPolicy, log_root: &Path) -> Result<Self, JevError> {
        let (inner, service) = open_shared(
            policy.service,
            policy.log_root_flag,
            log_root,
            Level::Info,
            "session log",
        )?;
        Ok(Self { inner, service })
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
    /// Returns `IO_FAILED` when the sink cannot flush. The action tells the
    /// caller to check the log root and does not mention `check.toml`.
    pub fn flush(&self) -> Result<(), JevError> {
        self.inner.0.flush().map_err(|error| {
            JevError::io_failed_with(
                format!("session log flush ({})", error.diagnostic().message),
                std::io::ErrorKind::Other,
                Some("Check that the log root is writable.".to_owned()),
            )
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
            level: level_of(event),
            service: self.service.clone(),
            target: TargetCategory::new(TARGET)
                .map_err(|_| LogRejected::new("session log target is not valid"))?,
            action: action_name(event.operation())?,
            message: None,
            identity: ProcessIdentity::default(),
            trace: None,
            // The session correlates its sends; the answer id names one send.
            // Both are ULIDs, valid ids (`sc-observability-types` is not a
            // direct dependency, so the ids are built through serde);
            // `fields` is unchanged.
            request_id: event
                .fields()
                .get("answer_id")
                .and_then(Value::as_str)
                .and_then(|id| envelope_id!(id)),
            correlation_id: envelope_id!(&event.session_id().to_string()),
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

/// `Warn` for a failed send, else `Info`, from the row's `send_outcome`.
fn level_of(event: &SessionEvent) -> Level {
    match event.fields().get("send_outcome").and_then(Value::as_str) {
        Some("failed") => Level::Warn,
        _ => Level::Info,
    }
}

/// The shared writer for `service` at `log_root`: reused when one is live,
/// else started. `label` names the log in error text.
fn open_shared(
    service: &str,
    flag: &str,
    log_root: &Path,
    level: Level,
    label: &str,
) -> Result<(Arc<LoggerInner>, ServiceName), JevError> {
    let service_name = ServiceName::new(service).map_err(|error| {
        JevError::config_invalid_with(
            format!("{label} service name {service:?} is not valid ({error})"),
            "Use a name of letters, digits, '-' and '_'.",
        )
    })?;
    std::fs::create_dir_all(log_root).map_err(|error| {
        JevError::io_failed_with(
            format!("log directory {} (create)", log_root.display()),
            error.kind(),
            Some(format!(
                "Pass a writable {flag} or fix the directory's permissions, then retry."
            )),
        )
    })?;
    let key = std::fs::canonicalize(log_root).map_err(|error| {
        JevError::io_failed_with(
            format!("log directory {} (resolve)", log_root.display()),
            error.kind(),
            Some(format!("Pass a readable {flag} and retry.")),
        )
    })?;
    let mut live = registry().lock().unwrap_or_else(PoisonError::into_inner);
    live.retain(|_, weak| weak.strong_count() > 0);
    let registry_key = (service.to_owned(), key.clone());
    if let Some(inner) = live.get(&registry_key).and_then(Weak::upgrade) {
        return Ok((inner, service_name));
    }
    let inner = Arc::new(LoggerInner(start_logger(
        service_name.clone(),
        &key,
        flag,
        level,
        label,
    )?));
    live.insert(registry_key, Arc::downgrade(&inner));
    Ok((inner, service_name))
}

/// The file-only logger configuration at `level` for `service`.
fn logger_config(service: ServiceName, log_root: &Path, level: Level) -> LoggerConfig {
    let mut config = LoggerConfig::default_for(service, log_root.to_path_buf());
    // `LevelFilter` is not re-exported by `sc-observability` and
    // `sc-observability-types` is not a direct dependency, so the filter is
    // built from the variant name through serde, like the envelope ids. A
    // miss keeps the logger's default (`Info`).
    config.level =
        serde_json::from_value(Value::String(format!("{level:?}"))).unwrap_or(config.level);
    config.enable_console_sink = false;
    config.enable_file_sink = true;
    config
}

fn start_logger(
    service: ServiceName,
    log_root: &Path,
    flag: &str,
    level: Level,
    label: &str,
) -> Result<Logger, JevError> {
    let config = logger_config(service, log_root, level);
    LoggerBuilder::new(config)
        .and_then(LoggerBuilder::build)
        .map_err(|error| {
            JevError::config_invalid_with(
                format!(
                    "the {label} could not start at {} ({})",
                    log_root.display(),
                    error.diagnostic().message
                ),
                format!("Pass a writable {flag} and retry."),
            )
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

/// `sc-observability` v2 logger used as a [`GeneralLog`].
///
/// The sc-agent default general log: a second file beside the session log,
/// `<log_root>/logs/<service>-general.log.jsonl`, at level `debug` and above.
/// Clones share one writer. Redaction of bearer tokens is on, but the ports
/// already forbid secrets, prompt text, state and provider bodies.
///
/// # Examples
///
/// ```no_run
/// use sc_jev_host::obs::GeneralLogAdapter;
/// use sc_jev_host::paths::PathPolicy;
///
/// let root = std::env::temp_dir().join("example-log");
/// let adapter = GeneralLogAdapter::open(&PathPolicy::cli(), &root)?;
/// let _port = adapter.shared();
/// adapter.flush()?;
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Clone)]
pub struct GeneralLogAdapter {
    inner: Arc<LoggerInner>,
    service: ServiceName,
}

impl std::fmt::Debug for GeneralLogAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GeneralLogAdapter")
            .field("service", &self.service.as_str())
            .finish_non_exhaustive()
    }
}

impl GeneralLogAdapter {
    /// The general log's service name for `policy`: `<service>-general`.
    #[must_use]
    pub fn service_name(policy: &PathPolicy) -> String {
        format!("{}{GENERAL_SUFFIX}", policy.service)
    }

    /// Returns the general-log file logger for `policy` rooted at
    /// `log_root`, reusing a live writer for the same service and root.
    ///
    /// # Errors
    ///
    /// As [`ObservabilityAdapter::open`]: `CONFIG_INVALID` for an invalid
    /// service or a logger that will not start, `IO_FAILED` when `log_root`
    /// cannot be created or resolved.
    pub fn open(policy: &PathPolicy, log_root: &Path) -> Result<Self, JevError> {
        let (inner, service) = open_shared(
            &Self::service_name(policy),
            policy.log_root_flag,
            log_root,
            Level::Debug,
            "general log",
        )?;
        Ok(Self { inner, service })
    }

    /// The `sc-observability` configuration of the default general log for
    /// `policy` at `log_root`: service `<service>-general`, file sink on,
    /// console sink off, `debug` and above.
    ///
    /// For a root that runs its own writer (the Python wheel hands this to
    /// the `sc-observability` Python runtime) so the file name, level and
    /// sinks match the default [`GeneralLogAdapter::open`] writes.
    ///
    /// # Errors
    ///
    /// `CONFIG_INVALID` when the service name is not valid.
    pub fn logger_config(policy: &PathPolicy, log_root: &Path) -> Result<LoggerConfig, JevError> {
        let name = Self::service_name(policy);
        let service = ServiceName::new(&name).map_err(|error| {
            JevError::config_invalid_with(
                format!("general log service name {name:?} is not valid ({error})"),
                "Use a name of letters, digits, '-' and '_'.",
            )
        })?;
        Ok(logger_config(service, log_root, Level::Debug))
    }

    /// This adapter as the port libraries take.
    #[must_use]
    pub fn shared(&self) -> SharedGeneralLog {
        Arc::new(self.clone())
    }

    /// Flushes admitted events to the JSONL sink.
    ///
    /// # Errors
    ///
    /// Returns `IO_FAILED` when the sink cannot flush.
    pub fn flush(&self) -> Result<(), JevError> {
        self.inner.0.flush().map_err(|error| {
            JevError::io_failed_with(
                format!("general log flush ({})", error.diagnostic().message),
                std::io::ErrorKind::Other,
                Some("Check that the log root is writable.".to_owned()),
            )
        })
    }

    /// Whether `self` and `other` write through the same logger.
    #[must_use]
    pub fn shares_writer_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// The `sc-observability` event that `event` becomes: the event's
    /// target, action, message and fields, unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`LogRejected`] when the target or action spelling is not a
    /// valid name; the event is rejected, never a panic.
    pub fn log_event(&self, event: &GeneralEvent) -> Result<LogEvent, LogRejected> {
        Ok(LogEvent {
            version: SchemaVersion::new(OBSERVATION_ENVELOPE_VERSION)
                .map_err(|_| LogRejected::new("observation envelope version is not valid"))?,
            timestamp: Timestamp::now_utc(),
            level: general_level(event.level()),
            service: self.service.clone(),
            target: TargetCategory::new(event.target()).map_err(|_| {
                LogRejected::new(format!(
                    "general log target {:?} is not valid",
                    event.target()
                ))
            })?,
            action: ActionName::new(event.action()).map_err(|_| {
                LogRejected::new(format!(
                    "general log action {:?} is not valid",
                    event.action()
                ))
            })?,
            message: Some(event.message().to_owned()),
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

impl GeneralLog for GeneralLogAdapter {
    fn log(&self, event: GeneralEvent) -> Result<(), LogRejected> {
        self.inner
            .0
            .log(self.log_event(&event)?)
            .map_err(|error| LogRejected::new(error.to_string()))
    }
}

fn general_level(level: GeneralLevel) -> Level {
    match level {
        GeneralLevel::Trace => Level::Trace,
        GeneralLevel::Debug => Level::Debug,
        GeneralLevel::Info => Level::Info,
        GeneralLevel::Warn => Level::Warn,
        // `GeneralLevel` is non-exhaustive: an unknown future level is
        // reported as an error rather than dropped.
        GeneralLevel::Error | _ => Level::Error,
    }
}
