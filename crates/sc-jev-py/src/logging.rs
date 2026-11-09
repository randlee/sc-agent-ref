//! Python-side logging: the shared host logger and injected Python loggers.
//!
//! # The two logs
//!
//! `run_check` writes a **session log** (one row per judge send) and a
//! **general log** (operator diagnostics) to separate files. Each has an
//! sc-agent default and each can be replaced from Python:
//!
//! - `session_log=` and `general_log=` take a callable. The session callable
//!   receives `{"session_id", "operation", "fields"}`; the general callable
//!   receives `{"level", "target", "action", "message", "fields"}`. A
//!   callable that raises is ignored: logging never changes a result. A
//!   callable needs the GIL once per row; the default does not.
//! - Left unset, the session log is `sc_jev_host`'s default
//!   (`<log_root>/logs/sc-jev-py.log.jsonl`) and the general log is the
//!   shared host logger below
//!   (`<log_root>/logs/sc-jev-py-general.log.jsonl`).
//!
//! # The shared host logger
//!
//! This wheel links the `sc-observability` Python runtime (`_native`, an
//! rlib) and registers it inside its own binary (sc-observability ADR-015).
//! At import it seeds `sys.modules["sc_observability._native"]` with that
//! module. The first default general log installs its core backend as the
//! process host logger (`install_host_logger`), and the backend `Arc` is
//! kept in a static for the life of the process. Rust then writes rows with
//! `try_log` inside `py.detach`, so **no row needs the GIL**. A Python caller
//! gets the same logger from `sc_observability.get_host_logger()` and its
//! rows land in the same queue and file as Rust's.
//!
//! One process has one host logger: the first log root that needs it. A
//! later call with another `log_dir` writes its general log to that root
//! through its own backend, which Python's `get_host_logger()` does not see.
//! `install_host_logger(log_dir=None)` installs the logger before the first
//! run so Python can choose the root.
//!
//! Import `sc_jev_py` before `sc_observability`: if the real
//! `sc_observability._native` is already loaded, this wheel cannot seed its
//! own, `install_host_logger` reports that, and Rust rows still reach the
//! default file.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use _native::_native;
use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyModule};
use sc_agent_types::JevError;
use sc_agent_types::observability::v2::{
    GeneralEvent, GeneralLog, LogEvent, LogRejected, SessionLog, SharedGeneralLog, SharedSessionLog,
};
use sc_jev_host::obs::GeneralLogAdapter;
use sc_jev_host::paths::PathPolicy;
use sc_observability_binding_runtime::{
    CoreLoggerBackend, CoreLoggerOwner, HostLoggingBackend, ProducerOrigin, create_core_backend,
};
use sc_observability_dto::{Failure, LogEventDto};
use serde_json::{Value, json};

/// The module name the `sc_observability` Python package imports.
const NATIVE_NAME: &str = "sc_observability._native";

/// How long a flush waits for the writer, per run.
const FLUSH_WAIT: Duration = Duration::from_secs(5);

/// Longest one row waits for room in the runtime's bounded queue. The queue
/// admits without blocking and refuses when full, and a stream at full rate
/// fills it; a full queue is waited out (the writer drains it in
/// microseconds on Linux and macOS, much slower on a slow Windows disk) instead of
/// dropping the row. A row still refused after this
/// is counted and reported, never lost silently.
const LOG_WAIT: Duration = Duration::from_secs(10);

/// Longest one row waits for room once its stream is cancelled. A healthy
/// writer still frees room in this time, so the tail rows (`stream.end`, the
/// session flush row) survive an interrupt; a stuck one costs each worker this
/// much once instead of [`LOG_WAIT`] per row.
const CANCELLED_WAIT: Duration = Duration::from_millis(250);

/// First pause between attempts while the queue is full; it doubles up to
/// [`LOG_RETRY_PAUSE_MAX`].
const LOG_RETRY_PAUSE_MIN: Duration = Duration::from_micros(200);

/// Longest pause between attempts.
const LOG_RETRY_PAUSE_MAX: Duration = Duration::from_millis(20);

/// The registered `_native` module of this binary.
static NATIVE: OnceLock<Py<PyModule>> = OnceLock::new();

/// Whether `NATIVE` is the module `sys.modules` serves under `NATIVE_NAME`.
static SEEDED: AtomicBool = AtomicBool::new(false);

/// Serializes check-and-install of the host logger.
static INSTALL_LOCK: Mutex<()> = Mutex::new(());

/// The entry installed as the host logger, once one is.
static INSTALLED: OnceLock<Arc<HostEntry>> = OnceLock::new();

/// One running core backend and the owner that keeps it alive.
pub(crate) struct HostEntry {
    backend: Arc<CoreLoggerBackend>,
    /// Held for the life of the process: dropping it would close the backend.
    _owner: CoreLoggerOwner,
    log_file: PathBuf,
    /// Serializes this process's flushes (see [`HostEntry::flush`]).
    flush_lock: Mutex<()>,
    /// Rows given up on after [`LOG_WAIT`] (see [`HostGeneralLog::log`]),
    /// counted until [`HostGeneralLog::flush`] reports them.
    dropped: AtomicU64,
}

type Key = (String, PathBuf);

/// Live backends by service and canonical log root. Entries are never
/// removed (process lifetime).
fn entries() -> &'static Mutex<HashMap<Key, Arc<HostEntry>>> {
    static ENTRIES: OnceLock<Mutex<HashMap<Key, Arc<HostEntry>>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers the `sc_observability` runtime inside this module's binary and
/// seeds `sys.modules` so the Python package attaches to it.
///
/// Called once from the module initializer.
pub(crate) fn register_native(py: Python<'_>) -> PyResult<()> {
    let native: Py<PyModule> = pyo3::wrap_pymodule!(_native)(py);
    let modules = py.import("sys")?.getattr("modules")?;
    let seeded = !modules.contains(NATIVE_NAME)?;
    if seeded {
        modules.set_item(NATIVE_NAME, native.bind(py))?;
    }
    if NATIVE.set(native).is_ok() {
        SEEDED.store(seeded, Ordering::SeqCst);
    }
    Ok(())
}

fn core_failure(error: impl std::fmt::Debug) -> JevError {
    JevError::config_invalid_with(
        format!("the shared general logger failed ({error:?})"),
        "Pass general_log= to use your own logger, or retry.",
    )
}

impl HostEntry {
    /// The entry for `log_root`, started on first use.
    fn open(policy: &PathPolicy, log_root: &Path) -> Result<Arc<Self>, JevError> {
        std::fs::create_dir_all(log_root).map_err(|error| {
            JevError::io_failed_with(
                format!("log directory {} (create)", log_root.display()),
                error.kind(),
                Some("Pass a writable log_dir and retry.".to_owned()),
            )
        })?;
        let canonical = std::fs::canonicalize(log_root).map_err(|error| {
            JevError::io_failed_with(
                format!("log directory {} (resolve)", log_root.display()),
                error.kind(),
                Some("Pass a readable log_dir and retry.".to_owned()),
            )
        })?;
        let service = GeneralLogAdapter::service_name(policy);
        let mut live = entries().lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = live.get(&(service.clone(), canonical.clone())) {
            return Ok(Arc::clone(entry));
        }
        let config = GeneralLogAdapter::logger_config(policy, &canonical)?;
        let (owner, backend) = create_core_backend(config).map_err(core_failure)?;
        let entry = Arc::new(Self {
            backend: Arc::new(backend),
            _owner: owner,
            log_file: canonical.join("logs").join(format!("{service}.log.jsonl")),
            flush_lock: Mutex::new(()),
            dropped: AtomicU64::new(0),
        });
        live.insert((service, canonical), Arc::clone(&entry));
        Ok(entry)
    }

    /// Installs this backend as the process host logger unless one is
    /// installed already.
    ///
    /// # Concurrency
    ///
    /// Installation is serialized by `INSTALL_LOCK`, so two Python threads
    /// that make their first `run_check` together cannot both pass the
    /// "not installed" check: the loser waits, sees the winner's entry, and
    /// gets `Ok(false)`, never the runtime's "already installed" error. The
    /// lock covers only this check-and-install; rows never take it. The
    /// runtime's own once-only guard remains the backstop for another
    /// embedder installing first.
    fn install_once(self: &Arc<Self>) -> Result<bool, JevError> {
        let _serialized = INSTALL_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        if INSTALLED.get().is_some() {
            return Ok(false);
        }
        if !SEEDED.load(Ordering::SeqCst) {
            return Err(JevError::config_invalid_with(
                "the sc_observability runtime was already loaded by another wheel",
                "Import sc_jev_py before sc_observability so both share one logger.",
            ));
        }
        let Some(native) = NATIVE.get() else {
            return Err(core_failure(
                "the sc_observability runtime is not registered",
            ));
        };
        let backend: Arc<dyn HostLoggingBackend> = self.backend.clone();
        let installed =
            Python::attach(|py| ::_native::install_host_logger(native.bind(py), backend));
        match installed {
            Ok(()) => {
                let _ = INSTALLED.set(Arc::clone(self));
                Ok(true)
            }
            Err(error) => Err(core_failure(error)),
        }
    }

    /// Waits until every row admitted before this call is on disk.
    ///
    /// The runtime allows one flush at a time and refuses a second with
    /// "flush in progress". Several runs of one process share this backend,
    /// so a refused flush used to be ignored and its run returned with rows
    /// still queued; a process that exited right after lost them. Flushes of
    /// this entry are now serialized by `flush_lock`, and a refusal (another
    /// embedder flushing the same backend, e.g. Python's own logger) is
    /// retried until `FLUSH_WAIT` runs out. Returns whether the flush
    /// completed; a caller reports `false` as a warning.
    pub(crate) fn flush(&self) -> bool {
        let _one_at_a_time = self
            .flush_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let deadline = std::time::Instant::now() + FLUSH_WAIT;
        loop {
            match self.backend.start_flush(FLUSH_WAIT) {
                Ok(operation) => return operation.wait(FLUSH_WAIT).is_ok(),
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return false,
            }
        }
    }

    /// The JSONL file of this entry.
    pub(crate) fn log_file(&self) -> &Path {
        &self.log_file
    }
}

/// The sc-agent default general log for Python: the shared host logger.
///
/// Holds the backend; `flush` makes its rows durable.
pub(crate) struct HostGeneralLog {
    entry: Arc<HostEntry>,
    /// Set when the owning stream is cancelled: a full queue is no longer
    /// waited out (see [`HostGeneralLog::cancel`]). Shared by this log and
    /// the ports made from it.
    cancelled: Arc<AtomicBool>,
}

impl HostGeneralLog {
    /// The default general log at `log_root`, installing the process host
    /// logger on first use.
    pub(crate) fn open(policy: &PathPolicy, log_root: &Path) -> Result<Self, JevError> {
        let entry = HostEntry::open(policy, log_root)?;
        // A host logger that cannot be installed (another wheel loaded the
        // runtime first) leaves Rust rows working; Python just cannot attach.
        let host = Self {
            entry,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        if let Err(error) = host.entry.install_once() {
            // Rust rows still reach the file; Python just cannot attach.
            // Say so in the log, by typed id.
            GeneralEvent::warn(
                "py.host_logger",
                "host_logger.install_skipped",
                "the shared host logger was not installed; Python cannot attach to it",
            )
            .with_error_code(error.code().as_str())
            .emit(&host);
        }
        Ok(host)
    }

    /// The port libraries take.
    pub(crate) fn shared(&self) -> SharedGeneralLog {
        Arc::new(Self {
            entry: Arc::clone(&self.entry),
            cancelled: Arc::clone(&self.cancelled),
        })
    }

    /// Shortens the wait for room to [`CANCELLED_WAIT`]: from now on a row that
    /// finds the queue full waits at most that long, then is counted as dropped. Called when the stream that
    /// owns this log is cancelled, so a stuck writer cannot hold `finish()` or
    /// a `with` exit for the row wait of every worker.
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Makes this log's admitted rows durable; a failure is a Warn row, never
    /// an error (logging never changes a result). Returns the number of rows
    /// given up on since the last call (0 when none), after writing a Warn
    /// row `general_log.rows_dropped` with that count, so a loss is never
    /// silent.
    pub(crate) fn flush(&self) -> u64 {
        let dropped = self.entry.dropped.swap(0, Ordering::AcqRel);
        if dropped > 0 {
            GeneralEvent::warn(
                "py.host_logger",
                "general_log.rows_dropped",
                "general-log rows were dropped because the queue stayed full",
            )
            .with_field("dropped", dropped)
            .emit(self);
        }
        if !self.entry.flush() {
            GeneralEvent::warn(
                "py.host_logger",
                "general_log.flush_failed",
                "the general log was not flushed; recent rows may be missing",
            )
            .emit(self);
        }
        dropped
    }
}

impl HostGeneralLog {
    /// How long a row may wait for room: [`LOG_WAIT`], or [`CANCELLED_WAIT`]
    /// once the stream is cancelled.
    fn wait_limit(&self) -> Duration {
        if self.cancelled.load(Ordering::Acquire) {
            CANCELLED_WAIT
        } else {
            LOG_WAIT
        }
    }
}

impl GeneralLog for HostGeneralLog {
    /// Admits the row. A full queue is waited out for at most [`LOG_WAIT`]
    /// (the caller holds no GIL: workers and detached calls), so a stream at
    /// full rate keeps every row; a row refused after the bound is counted in
    /// the entry and reported by [`HostGeneralLog::flush`].
    fn log(&self, event: GeneralEvent) -> Result<(), LogRejected> {
        let dto = event_dto(&event).map_err(LogRejected::new)?;
        let started = std::time::Instant::now();
        let mut pause = LOG_RETRY_PAUSE_MIN;
        loop {
            match self
                .entry
                .backend
                .try_log(dto.clone(), ProducerOrigin::RustHost)
            {
                Ok(_) => return Ok(()),
                Err(Failure::QueueFull { .. }) if started.elapsed() < self.wait_limit() => {
                    std::thread::sleep(pause);
                    // Back off: a healthy writer frees room in microseconds,
                    // a stuck one should not be polled by every worker.
                    pause = (pause * 2).min(LOG_RETRY_PAUSE_MAX);
                }
                Err(failure) => {
                    self.entry.dropped.fetch_add(1, Ordering::AcqRel);
                    return Err(LogRejected::new(format!("{failure:?}")));
                }
            }
        }
    }
}

/// `install_host_logger(log_dir=None)`: starts the shared logger now.
///
/// Returns the path of its log file. Call it before the first `run_check`
/// to choose the root Python's `get_host_logger()` attaches to.
pub(crate) fn install_at(log_root: &Path) -> Result<PathBuf, JevError> {
    let entry = HostEntry::open(&PathPolicy::python(), log_root)?;
    entry.install_once()?;
    // The installed logger, which is `entry` only for the first caller.
    Ok(INSTALLED
        .get()
        .map_or_else(|| entry.log_file(), |installed| installed.log_file())
        .to_path_buf())
}

/// The core DTO of `event`.
fn event_dto(event: &GeneralEvent) -> Result<LogEventDto, String> {
    let fields: BTreeMap<&String, Value> = event
        .fields()
        .iter()
        .map(|(key, value)| (key, value_dto(value)))
        .collect();
    serde_json::from_value(json!({
        "schema_version": 1,
        "level": event.level().as_str(),
        "target": event.target(),
        "action": event.action(),
        "message": event.message(),
        "trace": null,
        "request_id": null,
        "correlation_id": null,
        "outcome": null,
        "fields": fields,
    }))
    .map_err(|error| error.to_string())
}

/// The tagged wire form of one JSON value.
fn value_dto(value: &Value) -> Value {
    match value {
        Value::Null => json!({"kind": "null"}),
        Value::Bool(flag) => json!({"kind": "boolean", "value": flag}),
        Value::String(text) => json!({"kind": "string", "value": text}),
        Value::Number(number) => {
            if number.is_f64() {
                json!({"kind": "float", "value": number})
            } else {
                json!({"kind": "integer", "value": number.to_string()})
            }
        }
        Value::Array(items) => {
            json!({"kind": "array", "value": items.iter().map(value_dto).collect::<Vec<_>>()})
        }
        Value::Object(map) => {
            let inner: serde_json::Map<String, Value> = map
                .iter()
                .map(|(key, value)| (key.clone(), value_dto(value)))
                .collect();
            json!({"kind": "object", "value": inner})
        }
    }
}

/// Flushes a default general log when dropped, on every exit path.
pub(crate) struct FlushOnDrop(pub(crate) Option<Arc<HostGeneralLog>>);

impl Drop for FlushOnDrop {
    fn drop(&mut self) {
        if let Some(log) = &self.0 {
            log.flush();
        }
    }
}

/// A Python callable used as the general log.
struct CallableGeneralLog(Py<PyAny>);

/// A Python callable used as the session log.
struct CallableSessionLog(Py<PyAny>);

/// Calls `callable(payload)` with `payload` as a dict. Any Python error is a
/// rejection and never propagates.
fn call_with(callable: &Py<PyAny>, payload: &Value) -> Result<(), LogRejected> {
    Python::attach(|py| {
        let text =
            serde_json::to_string(payload).map_err(|error| LogRejected::new(error.to_string()))?;
        let dict = py
            .import("json")
            .and_then(|json| json.call_method1("loads", (text,)))
            .map_err(|_| LogRejected::new("the log row could not be converted"))?;
        callable.call1(py, (dict,)).map(|_| ()).map_err(|error| {
            // The error is dropped, not printed or raised: a logger that
            // raises must not change the run.
            drop(error);
            LogRejected::new("the injected logger raised")
        })
    })
}

impl GeneralLog for CallableGeneralLog {
    fn log(&self, event: GeneralEvent) -> Result<(), LogRejected> {
        call_with(
            &self.0,
            &json!({
                "level": event.level().as_str(),
                "target": event.target(),
                "action": event.action(),
                "message": event.message(),
                "fields": event.fields(),
            }),
        )
    }
}

impl SessionLog for CallableSessionLog {
    fn emit(&self, event: LogEvent) -> Result<(), LogRejected> {
        call_with(
            &self.0,
            &json!({
                "session_id": event.session_id().to_string(),
                "operation": event.operation().as_str(),
                "fields": event.fields(),
            }),
        )
    }
}

/// The injected loggers of one Python call.
#[derive(Default, Clone)]
pub(crate) struct Injected {
    pub(crate) session: Option<SharedSessionLog>,
    pub(crate) general: Option<SharedGeneralLog>,
}

/// Wraps the `session_log=` and `general_log=` arguments.
///
/// # Errors
///
/// `INPUT_INVALID` when an argument is not callable.
pub(crate) fn injected(
    session: Option<Py<PyAny>>,
    general: Option<Py<PyAny>>,
    py: Python<'_>,
) -> Result<Injected, JevError> {
    let check = |name: &str, value: &Py<PyAny>| -> Result<(), JevError> {
        if value.bind(py).is_callable() {
            Ok(())
        } else {
            Err(JevError::input_invalid(format!(
                "{name} must be a callable taking one dict"
            )))
        }
    };
    if let Some(value) = &session {
        check("session_log", value)?;
    }
    if let Some(value) = &general {
        check("general_log", value)?;
    }
    Ok(Injected {
        session: session.map(|value| Arc::new(CallableSessionLog(value)) as SharedSessionLog),
        general: general.map(|value| Arc::new(CallableGeneralLog(value)) as SharedGeneralLog),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn sample() -> GeneralEvent {
        GeneralEvent::info("py.test", "row.write", "hello")
            .with_field("count", 3)
            .with_field("ratio", 0.5)
            .with_field("flag", true)
            .with_field("name", "x")
            .with_field("nested", json!({"a": [1, null, "b"]}))
    }

    fn rows(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line).expect("json line"))
            .collect()
    }

    #[test]
    fn every_value_kind_converts_to_the_core_dto() {
        let dto = event_dto(&sample()).expect("dto");
        assert_eq!(dto.action, "row.write");
        assert_eq!(dto.fields.len(), 5);
    }

    #[test]
    fn a_row_is_written_by_a_thread_while_another_holds_the_gil() {
        let dir = tempfile::tempdir().expect("temp");
        let host = HostGeneralLog::open(&PathPolicy::python(), dir.path()).expect("host");
        let log = host.shared();
        Python::initialize();
        Python::attach(|_py| {
            // This thread holds the GIL and does not release it. If a row
            // needed the GIL, the writer thread would never answer.
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(log.log(sample()));
            });
            let sent = receiver
                .recv_timeout(Duration::from_secs(20))
                .expect("a row must not wait for the GIL");
            assert!(sent.is_ok(), "{sent:?}");
        });
        host.flush();
        let file = dir.path().join("logs/sc-jev-py-general.log.jsonl");
        let written = rows(&file);
        assert!(
            written.iter().any(|row| row["action"] == "row.write"),
            "{written:?}"
        );
    }

    #[test]
    fn a_callable_that_raises_is_a_rejection_not_a_panic() {
        Python::initialize();
        Python::attach(|py| {
            let raising = py
                .eval(c"(lambda event: 1 / 0)", None, None)
                .expect("lambda")
                .unbind();
            let log = CallableGeneralLog(raising);
            assert!(log.log(sample()).is_err());
            let recording = py.eval(c"[].append", None, None).expect("append").unbind();
            assert!(CallableGeneralLog(recording).log(sample()).is_ok());
        });
    }
}
