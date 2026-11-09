//! `DecisionStream` and `run_many`: many requests through one process
//! (ADR-SAG-0032, REQ-SAG-0726).
//!
//! A thin adapter over `sc_jev_host::stream`. Every call that can block
//! (waiting for queue room, waiting for an answer, running a batch) runs
//! under `py.detach`, so the GIL is released while workers run and other
//! Python threads keep going. Waits are taken in 50 ms slices with a signal
//! check between them, so `Ctrl-C` raises `KeyboardInterrupt` instead of
//! hanging. The Python surface is at least the CLI's: the same wire lines
//! (`{"id", "ok", "envelope"?, "error"?}` as dicts), the same concurrency
//! setting and default, plus per-request `submit`, timeouts, `cancel`,
//! `finish` and a context manager.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pyo3::exceptions::PyTimeoutError;
use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict};
use sc_agent_types::JevError;
use sc_jev_host::stream::{
    Batch, Concurrency, DecisionStream as HostStream, Poll, StreamRequest, StreamSummary,
    SubmitError,
};
use serde_json::Value;

use crate::error::{map_err, to_py_err};
use crate::logging::{self, HostGeneralLog, Injected};
use crate::run::{base_inputs, non_empty, parse_backends, parse_project, to_py};
use sc_jev_host::paths::PathPolicy;

/// How long one blocking wait lasts before Python signals are checked.
const SLICE: Duration = Duration::from_millis(50);

/// The keyword arguments shared by `DecisionStream` and `run_many`.
struct StreamSpec {
    concurrency: Concurrency,
    backends: Option<Vec<String>>,
    repo_root: Option<PathBuf>,
    log_dir: Option<PathBuf>,
    session_dir: Option<PathBuf>,
    models_file: Option<PathBuf>,
    system_prompt_file: Option<PathBuf>,
    chain_context: bool,
    project: Option<String>,
    loggers: Injected,
    #[cfg(any(test, feature = "test-util"))]
    scripted_answers: Option<Value>,
}

impl StreamSpec {
    #[expect(clippy::too_many_arguments, reason = "Python keyword API")]
    fn new(
        concurrency: usize,
        backends: Option<Vec<String>>,
        repo_root: Option<PathBuf>,
        log_dir: Option<PathBuf>,
        session_dir: Option<PathBuf>,
        models_file: Option<PathBuf>,
        system_prompt_file: Option<PathBuf>,
        chain_context: bool,
        project: Option<String>,
        loggers: Injected,
    ) -> Result<Self, JevError> {
        Ok(Self {
            concurrency: Concurrency::new(concurrency)?,
            backends,
            repo_root: non_empty(repo_root),
            log_dir: non_empty(log_dir),
            session_dir: non_empty(session_dir),
            models_file: non_empty(models_file),
            system_prompt_file: non_empty(system_prompt_file),
            chain_context,
            project,
            loggers,
            #[cfg(any(test, feature = "test-util"))]
            scripted_answers: None,
        })
    }

    /// Starts the stream. The general log is the caller's, else the shared
    /// host logger at this log root (the one `run_check` and
    /// `sc_observability.get_host_logger()` use, so one file has one writer);
    /// the host logger is returned so the caller flushes it on every exit.
    fn start(self) -> Result<(HostStream, Option<Arc<HostGeneralLog>>), JevError> {
        let mut inputs = base_inputs(self.repo_root.as_deref(), self.log_dir, self.session_dir)?
            .mode(if self.chain_context {
                sc_decision_schema::MultiBackendMode::ChainContext
            } else {
                sc_decision_schema::MultiBackendMode::Independent
            });
        if let Some(path) = self.models_file {
            inputs = inputs.models_file(path);
        }
        if let Some(backends) = parse_backends(self.backends.as_deref())? {
            inputs = inputs.backends(backends);
        }
        if let Some(path) = self.system_prompt_file {
            inputs = inputs.system_prompt_override(path);
        }
        if let Some(project) = parse_project(self.project)? {
            inputs = inputs.project(project);
        }
        #[cfg(any(test, feature = "test-util"))]
        if let Some(answers) = self.scripted_answers {
            inputs = inputs.registry(crate::run::scripted_registry(answers)?);
        }
        if let Some(log) = self.loggers.session.clone() {
            inputs = inputs.session_log(log);
        }
        let resolved = inputs.resolve()?;
        let mut host = None;
        match self.loggers.general.clone() {
            Some(log) => inputs = inputs.general_log(log),
            None => {
                // A default that cannot start leaves the sc-jev-host default.
                if let Ok(opened) = HostGeneralLog::open(&PathPolicy::python(), &resolved.log_root)
                {
                    inputs = inputs.general_log(opened.shared());
                    host = Some(Arc::new(opened));
                }
            }
        }
        let stream = HostStream::start(inputs.repo_root(resolved.repo_root), self.concurrency)?;
        Ok((stream, host))
    }
}

/// A request given as a `dict` (or any JSON-serializable object) or as JSON
/// text.
fn request_text(request: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(text) = request.extract::<String>() {
        return Ok(text);
    }
    request
        .py()
        .import("json")?
        .call_method1("dumps", (request,))?
        .extract()
}

fn closed() -> JevError {
    JevError::input_invalid_field(
        "submit",
        "the stream input is closed or the stream was cancelled",
    )
}

/// One answer line as a Python dict.
fn answer_dict(py: Python<'_>, answer: &sc_jev_host::stream::StreamAnswer) -> PyResult<Py<PyAny>> {
    let line = map_err(py, answer.to_line())?;
    to_py(py, "stream answer", serde_json::from_slice::<Value>(&line))
}

/// A stream of decision requests served by N worker threads.
///
/// Submit requests with `submit(id, request)` while reading answers with
/// `next()` or by iterating; answers come back in completion order, each
/// carrying the `id` it was submitted with. Per-request failures are
/// answers with `ok == False` and an `error` dict; they never raise and
/// never stop the stream. The stream is finished when `close_input()` was
/// called, the queue is empty and the workers are idle; iteration then ends.
#[pyclass(module = "sc_jev_py", name = "DecisionStream", frozen)]
pub(crate) struct DecisionStream {
    inner: Arc<HostStream>,
    /// The shared host logger, flushed on every way out of the stream.
    host: Option<Arc<HostGeneralLog>>,
}

#[pymethods]
impl DecisionStream {
    /// Starts the workers.
    ///
    /// `concurrency` is the number of worker threads, 1 to 64 (default 8; the
    /// default was exercised against scripted and loopback backends only,
    /// never a live provider). Other arguments are as `run_check`; `backends`,
    /// when given, wins over a request's own `backends`. `session_log=` and
    /// `general_log=` are callables taking one dict, as in `run_check`; unset,
    /// the general log is the shared host logger (so
    /// `sc_observability.get_host_logger()` shares its writer) and the session
    /// log is the default under the log root.
    #[new]
    #[pyo3(signature = (
        *,
        concurrency=8,
        backends=None,
        repo_root=None,
        log_dir=None,
        session_dir=None,
        models_file=None,
        system_prompt_file=None,
        chain_context=false,
        project=None,
        session_log=None,
        general_log=None,
    ))]
    #[expect(clippy::too_many_arguments, reason = "Python keyword API")]
    fn new(
        py: Python<'_>,
        concurrency: usize,
        backends: Option<Vec<String>>,
        repo_root: Option<PathBuf>,
        log_dir: Option<PathBuf>,
        session_dir: Option<PathBuf>,
        models_file: Option<PathBuf>,
        system_prompt_file: Option<PathBuf>,
        chain_context: bool,
        project: Option<String>,
        session_log: Option<Py<PyAny>>,
        general_log: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let loggers = map_err(py, logging::injected(session_log, general_log, py))?;
        let spec = map_err(
            py,
            StreamSpec::new(
                concurrency,
                backends,
                repo_root,
                log_dir,
                session_dir,
                models_file,
                system_prompt_file,
                chain_context,
                project,
                loggers,
            ),
        )?;
        Self::from_spec(py, spec)
    }

    /// Queues one request under `id`; returns `True` when accepted.
    ///
    /// Blocks while the bounded queue is full (the GIL is released). With
    /// `timeout` (seconds) it returns `False` if there was still no room
    /// then. `timeout` must be zero or more (NaN or negative is
    /// `INPUT_INVALID`); `inf` or a value too large to represent means no
    /// deadline. Raises `JevError` (`INPUT_INVALID`) for a bad `id` (1 to 256
    /// bytes, no control characters), text that is not JSON, or a closed or
    /// cancelled stream. `request` is the `--request-json` document as a dict
    /// or JSON text. Ids are not checked for uniqueness.
    ///
    /// **An answer you have not read holds a place in the stream.** Past
    /// `capacity` unread answers, `submit` blocks: a single thread that
    /// submits more than `capacity` requests before reading any answer waits
    /// forever. Read answers from another thread while submitting, or pass
    /// `timeout=` and read when it returns `False`.
    #[pyo3(signature = (id, request, *, timeout=None))]
    fn submit(
        &self,
        py: Python<'_>,
        id: &str,
        request: &Bound<'_, PyAny>,
        timeout: Option<f64>,
    ) -> PyResult<bool> {
        let request = map_err(py, StreamRequest::new(id, &request_text(request)?))?;
        self.queue(py, timeout, |stream, slice| {
            stream.submit_request(request.clone(), Some(slice))
        })
    }

    /// Queues one JSONL input line, `{"id": ..., "request": ...}`, exactly as
    /// `sc-ai jev stream` reads it from a file or stdin. A line that is not a
    /// valid request is not an exception: it comes back as an answer with
    /// `ok == False`. Blocking and `timeout` are as for `submit`.
    #[pyo3(signature = (line, *, timeout=None))]
    fn submit_line(&self, py: Python<'_>, line: &str, timeout: Option<f64>) -> PyResult<bool> {
        self.queue(py, timeout, |stream, slice| {
            stream.submit_line(line.to_owned(), Some(slice))
        })
    }

    /// The next answer as a dict, in completion order.
    ///
    /// Returns `None` once the stream is finished (input closed, queue empty,
    /// workers idle). Raises `TimeoutError` if `timeout` seconds pass first.
    /// The GIL is released while waiting.
    #[pyo3(signature = (timeout=None))]
    fn next(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<Option<Py<PyAny>>> {
        let deadline = deadline_of(py, timeout)?;
        loop {
            let stream = Arc::clone(&self.inner);
            let slice = remaining(deadline, SLICE);
            match py.detach(move || stream.next_answer_within(slice)) {
                Poll::Answer(answer) => return answer_dict(py, &answer).map(Some),
                Poll::Finished => return Ok(None),
                Poll::Pending => {
                    py.check_signals()?;
                    if expired(deadline) {
                        return Err(PyTimeoutError::new_err("no answer within the timeout"));
                    }
                }
            }
        }
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.next(py, None)
    }

    /// No more requests will be submitted; the stream finishes once the
    /// queue drains.
    fn close_input(&self) {
        self.inner.close_input();
    }

    /// Stops new work and cancels the engine's token. It also stops waiting for
    /// room in a full general-log queue: a row that finds it full is counted
    /// as dropped, so a stuck log file cannot hold `finish()`. Requests still queued
    /// are answered with a `NETWORK_ERROR` ("cancelled") line; a request
    /// already sent is not interrupted. Further `submit` calls raise.
    fn cancel(&self) {
        self.inner.cancel();
        if let Some(host) = &self.host {
            host.cancel();
        }
    }

    /// Requests accepted whose answer was not yet returned.
    #[getter]
    fn in_flight(&self) -> usize {
        self.inner.in_flight()
    }

    /// The most requests the stream holds at once (its memory bound).
    #[getter]
    fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Closes the input, drops answers not yet read, waits for the workers,
    /// flushes the log, and returns the summary: `{"answered", "failed",
    /// "exit_code", "log_flush_warning"}` where `exit_code` is the CLI's
    /// (0, or 3 if every failure is retryable, else 2). It never hangs on
    /// unread answers; it waits for requests already sent (the wait can be
    /// interrupted with `Ctrl-C`).
    fn finish(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.inner.close_input();
        loop {
            let stream = Arc::clone(&self.inner);
            match py.detach(move || stream.next_answer_within(SLICE)) {
                Poll::Finished => break,
                Poll::Answer(_) | Poll::Pending => py.check_signals()?,
            }
        }
        let stream = Arc::clone(&self.inner);
        let host = self.host.clone();
        let (summary, dropped) = py.detach(move || {
            let summary = stream.finish();
            let dropped = flush_host(host.as_deref());
            (summary, dropped)
        });
        summary_dict(py, &summary, dropped)
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Leaving the block cancels the stream and drops unread answers.
    ///
    /// Waits at most 2 s for the workers (`ABANDON_WAIT`), then writes
    /// `stream.end` and flushes both logs, so the rows of every request that
    /// finished are on disk when the process exits at once.
    #[pyo3(signature = (*_exc))]
    fn __exit__(&self, py: Python<'_>, _exc: &Bound<'_, pyo3::types::PyTuple>) {
        self.leave(py);
    }
}

impl DecisionStream {
    /// Cancels, abandons (bounded) and flushes, with the GIL released so a
    /// worker that needs it for a Python logger can finish.
    fn leave(&self, py: Python<'_>) {
        if let Some(host) = &self.host {
            host.cancel();
        }
        let stream = Arc::clone(&self.inner);
        let host = self.host.clone();
        py.detach(move || {
            stream.abandon();
            let _ = flush_host(host.as_deref());
        });
    }

    /// Retries `try_queue` in slices until it is accepted, the stream is
    /// closed, or `timeout` passes, checking for Python signals between.
    fn queue(
        &self,
        py: Python<'_>,
        timeout: Option<f64>,
        try_queue: impl Fn(&HostStream, Duration) -> Result<(), SubmitError> + Send + Sync,
    ) -> PyResult<bool> {
        let deadline = deadline_of(py, timeout)?;
        loop {
            let slice = remaining(deadline, SLICE);
            let stream = &self.inner;
            match py.detach(|| try_queue(stream, slice)) {
                Ok(()) => return Ok(true),
                Err(SubmitError::Full) => {
                    py.check_signals()?;
                    if expired(deadline) {
                        return Ok(false);
                    }
                }
                Err(_) => return Err(to_py_err(py, &closed())),
            }
        }
    }

    fn from_spec(py: Python<'_>, spec: StreamSpec) -> PyResult<Self> {
        let started = py.detach(move || spec.start());
        let (stream, host) = map_err(py, started)?;
        Ok(Self {
            inner: Arc::new(stream),
            host,
        })
    }
}

impl Drop for DecisionStream {
    fn drop(&mut self) {
        Python::attach(|py| self.leave(py));
    }
}

/// Flushes the shared host logger; returns the rows it had to give up on.
fn flush_host(host: Option<&HostGeneralLog>) -> u64 {
    host.map_or(0, HostGeneralLog::flush)
}

fn summary_dict(py: Python<'_>, summary: &StreamSummary, dropped: u64) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("answered", summary.answered())?;
    dict.set_item("failed", summary.failed())?;
    dict.set_item("exit_code", summary.exit_code())?;
    dict.set_item("log_flush_warning", summary.log_flush_warning())?;
    // General-log rows given up on after the bounded wait (0 when none); the
    // same count is a Warn row `general_log.rows_dropped`.
    dict.set_item("general_log_rows_dropped", dropped)?;
    Ok(dict.into_any().unbind())
}

/// The one conversion of a Python `timeout` (seconds) at the boundary.
///
/// `None`, `inf` and any finite value too large for a `Duration` or an
/// `Instant` mean no deadline. NaN and negative values are `INPUT_INVALID`
/// naming `timeout`.
fn deadline_of(py: Python<'_>, timeout: Option<f64>) -> PyResult<Option<Instant>> {
    let Some(seconds) = timeout else {
        return Ok(None);
    };
    if seconds.is_nan() || seconds < 0.0 {
        return Err(to_py_err(
            py,
            &JevError::input_invalid_field(
                "timeout",
                "timeout must be a number of seconds, zero or more",
            ),
        ));
    }
    Ok(Duration::try_from_secs_f64(seconds)
        .ok()
        .and_then(|wait| Instant::now().checked_add(wait)))
}

fn expired(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|at| Instant::now() >= at)
}

/// `slice`, or what is left until `deadline` if that is shorter.
fn remaining(deadline: Option<Instant>, slice: Duration) -> Duration {
    deadline.map_or(slice, |at| {
        at.saturating_duration_since(Instant::now()).min(slice)
    })
}

/// Runs a batch: every `(id, request)` pair in `requests` in parallel on
/// `concurrency` workers. Returns `(answers, summary)`: every answer dict in
/// completion order, and the CLI's summary (`answered`, `failed`,
/// `exit_code`, `log_flush_warning`).
///
/// `requests` is a dict of id to request, or an iterable of `(id, request)`
/// pairs; a request is a dict or JSON text. Every id and body is validated
/// before anything starts, so a bad one raises `JevError` with nothing sent.
/// A failed request is an answer with `ok == False`, not an exception, so one
/// bad request cannot lose the others. Waiting is interruptible (`Ctrl-C`
/// cancels the batch and raises `KeyboardInterrupt`). The GIL is released
/// while the workers run. Other arguments are as `DecisionStream`.
#[pyfunction]
#[pyo3(signature = (
    requests,
    *,
    concurrency=8,
    backends=None,
    repo_root=None,
    log_dir=None,
    session_dir=None,
    models_file=None,
    system_prompt_file=None,
    chain_context=false,
    project=None,
    session_log=None,
    general_log=None,
))]
#[expect(clippy::too_many_arguments, reason = "Python keyword API")]
pub(crate) fn run_many(
    py: Python<'_>,
    requests: &Bound<'_, PyAny>,
    concurrency: usize,
    backends: Option<Vec<String>>,
    repo_root: Option<PathBuf>,
    log_dir: Option<PathBuf>,
    session_dir: Option<PathBuf>,
    models_file: Option<PathBuf>,
    system_prompt_file: Option<PathBuf>,
    chain_context: bool,
    project: Option<String>,
    session_log: Option<Py<PyAny>>,
    general_log: Option<Py<PyAny>>,
) -> PyResult<(Vec<Py<PyAny>>, Py<PyAny>)> {
    let loggers = map_err(py, logging::injected(session_log, general_log, py))?;
    let spec = map_err(
        py,
        StreamSpec::new(
            concurrency,
            backends,
            repo_root,
            log_dir,
            session_dir,
            models_file,
            system_prompt_file,
            chain_context,
            project,
            loggers,
        ),
    )?;
    run_batch(py, spec, requests)
}

/// Every pair of `requests`, validated.
fn validated(py: Python<'_>, requests: &Bound<'_, PyAny>) -> PyResult<Vec<StreamRequest>> {
    let items: Vec<Bound<'_, PyAny>> = match requests.cast::<PyDict>() {
        Ok(dict) => dict.items().iter().collect(),
        Err(_) => requests.try_iter()?.collect::<PyResult<_>>()?,
    };
    items
        .iter()
        .map(|item| {
            let (id, request): (String, Bound<'_, PyAny>) = item.extract()?;
            map_err(py, StreamRequest::new(&id, &request_text(&request)?))
        })
        .collect()
}

fn run_batch(
    py: Python<'_>,
    spec: StreamSpec,
    requests: &Bound<'_, PyAny>,
) -> PyResult<(Vec<Py<PyAny>>, Py<PyAny>)> {
    // Validate everything before a stream, a log or a send exists.
    let batch = validated(py, requests)?;
    let started = py.detach(move || spec.start());
    let (stream, host) = map_err(py, started)?;
    let batch = Batch::start(Arc::new(stream), batch);
    let mut answers = Vec::new();
    let polled = (|| -> PyResult<()> {
        loop {
            match py.detach(|| batch.next_within(SLICE)) {
                Poll::Answer(answer) => answers.push(answer_dict(py, &answer)?),
                Poll::Pending => py.check_signals()?,
                Poll::Finished => return Ok(()),
            }
        }
    })();
    if let Err(error) = polled {
        if let Some(host) = &host {
            host.cancel();
        }
        // Ctrl-C or a failure: cancel and abandon (bounded to 2 s, GIL
        // released), which writes `stream.end` and flushes the session log;
        // then flush the shared host logger.
        py.detach(move || {
            drop(batch);
            let _ = flush_host(host.as_deref());
        });
        return Err(error);
    }
    let (summary, dropped) = py.detach(move || {
        let summary = batch.finish();
        let dropped = flush_host(host.as_deref());
        (summary, dropped)
    });
    Ok((answers, summary_dict(py, &summary, dropped)?))
}

/// Test-only entry points, registered in `sc_jev_py._testing`.
#[cfg(feature = "test-util")]
pub(crate) mod testing {
    use super::*;
    use crate::run::py_to_value;

    /// `DecisionStream(...)` with scripted judge answers instead of HTTP
    /// backends (the same r-2b JSON as `run_check_scripted`).
    #[pyfunction]
    #[pyo3(signature = (
        *,
        scripted_answers,
        concurrency=8,
        backends=None,
        repo_root=None,
        log_dir=None,
        session_dir=None,
        session_log=None,
        general_log=None,
    ))]
    #[expect(clippy::too_many_arguments, reason = "Python keyword API")]
    pub(crate) fn decision_stream_scripted(
        py: Python<'_>,
        scripted_answers: Bound<'_, PyAny>,
        concurrency: usize,
        backends: Option<Vec<String>>,
        repo_root: Option<PathBuf>,
        log_dir: Option<PathBuf>,
        session_dir: Option<PathBuf>,
        session_log: Option<Py<PyAny>>,
        general_log: Option<Py<PyAny>>,
    ) -> PyResult<DecisionStream> {
        let loggers = map_err(py, logging::injected(session_log, general_log, py))?;
        let answers = map_err(py, py_to_value(&scripted_answers))?;
        let mut spec = map_err(
            py,
            StreamSpec::new(
                concurrency,
                backends,
                repo_root,
                log_dir,
                session_dir,
                None,
                None,
                false,
                None,
                loggers,
            ),
        )?;
        spec.scripted_answers = Some(answers);
        DecisionStream::from_spec(py, spec)
    }

    /// `run_many(...)` with scripted judge answers.
    #[pyfunction]
    #[pyo3(signature = (
        requests,
        *,
        scripted_answers,
        concurrency=8,
        backends=None,
        repo_root=None,
        log_dir=None,
        session_dir=None,
        session_log=None,
        general_log=None,
    ))]
    #[expect(clippy::too_many_arguments, reason = "Python keyword API")]
    pub(crate) fn run_many_scripted(
        py: Python<'_>,
        requests: &Bound<'_, PyAny>,
        scripted_answers: Bound<'_, PyAny>,
        concurrency: usize,
        backends: Option<Vec<String>>,
        repo_root: Option<PathBuf>,
        log_dir: Option<PathBuf>,
        session_dir: Option<PathBuf>,
        session_log: Option<Py<PyAny>>,
        general_log: Option<Py<PyAny>>,
    ) -> PyResult<(Vec<Py<PyAny>>, Py<PyAny>)> {
        let loggers = map_err(py, logging::injected(session_log, general_log, py))?;
        let answers = map_err(py, py_to_value(&scripted_answers))?;
        let mut spec = map_err(
            py,
            StreamSpec::new(
                concurrency,
                backends,
                repo_root,
                log_dir,
                session_dir,
                None,
                None,
                false,
                None,
                loggers,
            ),
        )?;
        spec.scripted_answers = Some(answers);
        run_batch(py, spec, requests)
    }
}
