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
use sc_jev_host::stream::{Concurrency, DecisionStream as HostStream, Poll, SubmitError};
use serde_json::Value;

use crate::error::{map_err, to_py_err};
use crate::run::{base_inputs, non_empty, parse_backends, parse_project, to_py};

/// How long one blocking wait lasts before Python signals are checked.
const SLICE: Duration = Duration::from_millis(50);

/// The keyword arguments shared by `DecisionStream` and `run_many`.
struct StreamSpec {
    concurrency: Concurrency,
    backends: Option<Vec<String>>,
    repo_root: Option<PathBuf>,
    log_dir: Option<PathBuf>,
    session_dir: Option<PathBuf>,
    system_prompt_file: Option<PathBuf>,
    chain_context: bool,
    project: Option<String>,
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
        system_prompt_file: Option<PathBuf>,
        chain_context: bool,
        project: Option<String>,
    ) -> Result<Self, JevError> {
        Ok(Self {
            concurrency: Concurrency::new(concurrency)?,
            backends,
            repo_root: non_empty(repo_root),
            log_dir: non_empty(log_dir),
            session_dir: non_empty(session_dir),
            system_prompt_file: non_empty(system_prompt_file),
            chain_context,
            project,
            #[cfg(any(test, feature = "test-util"))]
            scripted_answers: None,
        })
    }

    fn start(self) -> Result<HostStream, JevError> {
        let mut inputs = base_inputs(self.repo_root.as_deref(), self.log_dir, self.session_dir)?
            .mode(if self.chain_context {
                sc_decision_schema::MultiBackendMode::ChainContext
            } else {
                sc_decision_schema::MultiBackendMode::Independent
            });
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
        let resolved = inputs.resolve()?;
        HostStream::start(inputs.repo_root(resolved.repo_root), self.concurrency)
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
}

#[pymethods]
impl DecisionStream {
    /// Starts the workers.
    ///
    /// `concurrency` is the number of worker threads, 1 to 64 (default 8; the
    /// default was exercised against scripted and loopback backends only,
    /// never a live provider). Other arguments are as `run_check`; `backends`,
    /// when given, wins over a request's own `backends`.
    #[new]
    #[pyo3(signature = (
        *,
        concurrency=8,
        backends=None,
        repo_root=None,
        log_dir=None,
        session_dir=None,
        system_prompt_file=None,
        chain_context=false,
        project=None,
    ))]
    #[expect(clippy::too_many_arguments, reason = "Python keyword API")]
    fn new(
        py: Python<'_>,
        concurrency: usize,
        backends: Option<Vec<String>>,
        repo_root: Option<PathBuf>,
        log_dir: Option<PathBuf>,
        session_dir: Option<PathBuf>,
        system_prompt_file: Option<PathBuf>,
        chain_context: bool,
        project: Option<String>,
    ) -> PyResult<Self> {
        let spec = map_err(
            py,
            StreamSpec::new(
                concurrency,
                backends,
                repo_root,
                log_dir,
                session_dir,
                system_prompt_file,
                chain_context,
                project,
            ),
        )?;
        Self::from_spec(py, spec)
    }

    /// Queues one request under `id`; returns `True` when accepted.
    ///
    /// Blocks while the bounded queue is full (the GIL is released). With
    /// `timeout` (seconds) it returns `False` if there was still no room
    /// then. Raises `JevError` (`INPUT_INVALID`) for a bad `id` (1 to 256
    /// bytes, no control characters), text that is not JSON, or a closed or
    /// cancelled stream. `request` is the `--request-json` document as a dict
    /// or JSON text. Ids are not checked for uniqueness.
    #[pyo3(signature = (id, request, *, timeout=None))]
    fn submit(
        &self,
        py: Python<'_>,
        id: &str,
        request: &Bound<'_, PyAny>,
        timeout: Option<f64>,
    ) -> PyResult<bool> {
        let text = request_text(request)?;
        let deadline = deadline_of(timeout);
        loop {
            let slice = remaining(deadline, SLICE);
            let stream = Arc::clone(&self.inner);
            let id = id.to_owned();
            let text = text.clone();
            let queued = py.detach(move || stream.submit_request(&id, &text, Some(slice)));
            match queued {
                Ok(()) => return Ok(true),
                Err(SubmitError::Full) => {
                    py.check_signals()?;
                    if expired(deadline) {
                        return Ok(false);
                    }
                }
                Err(SubmitError::Invalid(error)) => return Err(to_py_err(py, &error)),
                Err(_) => return Err(to_py_err(py, &closed())),
            }
        }
    }

    /// The next answer as a dict, in completion order.
    ///
    /// Returns `None` once the stream is finished (input closed, queue empty,
    /// workers idle). Raises `TimeoutError` if `timeout` seconds pass first.
    /// The GIL is released while waiting.
    #[pyo3(signature = (timeout=None))]
    fn next(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<Option<Py<PyAny>>> {
        let deadline = deadline_of(timeout);
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

    /// Stops new work and cancels the engine's token. Requests still queued
    /// are answered with a `NETWORK_ERROR` ("cancelled") line; a request
    /// already sent is not interrupted. Further `submit` calls raise.
    fn cancel(&self) {
        self.inner.cancel();
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

    /// Waits for the workers, flushes the log, and returns the summary:
    /// `{"answered", "failed", "exit_code"}` where `exit_code` is the CLI's
    /// (0, or 3 if every failure is retryable, else 2). Call after `next()`
    /// returned `None`.
    fn finish(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let stream = Arc::clone(&self.inner);
        let summary = py.detach(move || stream.finish());
        let dict = PyDict::new(py);
        dict.set_item("answered", summary.answered())?;
        dict.set_item("failed", summary.failed())?;
        dict.set_item("exit_code", summary.exit_code())?;
        dict.set_item("log_flush_warning", summary.log_flush_warning())?;
        Ok(dict.into_any().unbind())
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Leaving the block cancels the stream and drops unread answers.
    #[pyo3(signature = (*_exc))]
    fn __exit__(&self, _exc: &Bound<'_, pyo3::types::PyTuple>) {
        self.inner.cancel();
        self.inner.abandon();
    }
}

impl DecisionStream {
    fn from_spec(py: Python<'_>, spec: StreamSpec) -> PyResult<Self> {
        let started = py.detach(move || spec.start());
        Ok(Self {
            inner: Arc::new(map_err(py, started)?),
        })
    }
}

impl Drop for DecisionStream {
    fn drop(&mut self) {
        self.inner.abandon();
    }
}

fn deadline_of(timeout: Option<f64>) -> Option<Instant> {
    timeout.map(|seconds| Instant::now() + Duration::from_secs_f64(seconds.max(0.0)))
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
/// `concurrency` workers, and returns every answer dict in completion order.
///
/// `requests` is a dict of id to request, or an iterable of `(id, request)`
/// pairs; a request is a dict or JSON text. A failed request is an answer
/// with `ok == False`, not an exception, so one bad request cannot lose the
/// others. Raises `JevError` only for a bad argument or a stream-level
/// failure (no repo root, logger start), or for a bad `id` before anything
/// runs. The GIL is released while the workers run. Other arguments are as
/// `DecisionStream`.
#[pyfunction]
#[pyo3(signature = (
    requests,
    *,
    concurrency=8,
    backends=None,
    repo_root=None,
    log_dir=None,
    session_dir=None,
    system_prompt_file=None,
    chain_context=false,
    project=None,
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
    system_prompt_file: Option<PathBuf>,
    chain_context: bool,
    project: Option<String>,
) -> PyResult<Vec<Py<PyAny>>> {
    let spec = map_err(
        py,
        StreamSpec::new(
            concurrency,
            backends,
            repo_root,
            log_dir,
            session_dir,
            system_prompt_file,
            chain_context,
            project,
        ),
    )?;
    run_batch(py, spec, requests)
}

fn pairs(requests: &Bound<'_, PyAny>) -> PyResult<Vec<(String, String)>> {
    let items: Vec<Bound<'_, PyAny>> = match requests.cast::<PyDict>() {
        Ok(dict) => dict.items().iter().collect(),
        Err(_) => requests.try_iter()?.collect::<PyResult<_>>()?,
    };
    items
        .iter()
        .map(|item| {
            let (id, request): (String, Bound<'_, PyAny>) = item.extract()?;
            Ok((id, request_text(&request)?))
        })
        .collect()
}

fn run_batch(
    py: Python<'_>,
    spec: StreamSpec,
    requests: &Bound<'_, PyAny>,
) -> PyResult<Vec<Py<PyAny>>> {
    let batch = pairs(requests)?;
    let lines = map_err(py, py.detach(move || drive(spec, batch)))?;
    lines
        .iter()
        .map(|line| to_py(py, "stream answer", serde_json::from_slice::<Value>(line)))
        .collect()
}

/// Feeds `batch` from a helper thread while this thread drains the answers,
/// so neither side can fill a bounded queue and block the other.
fn drive(spec: StreamSpec, batch: Vec<(String, String)>) -> Result<Vec<Vec<u8>>, JevError> {
    let stream = Arc::new(spec.start()?);
    // Validate every id and body before anything runs.
    for (id, text) in &batch {
        serde_json::from_str::<&serde_json::value::RawValue>(text).map_err(|error| {
            JevError::input_invalid_field("request", format!("request {id:?} is not JSON: {error}"))
        })?;
    }
    let feeder = {
        let stream = Arc::clone(&stream);
        std::thread::spawn(move || -> Result<(), JevError> {
            for (id, text) in batch {
                match stream.submit_request(&id, &text, None) {
                    Ok(()) => {}
                    Err(SubmitError::Invalid(error)) => {
                        stream.cancel();
                        return Err(error);
                    }
                    Err(_) => break,
                }
            }
            stream.close_input();
            Ok(())
        })
    };
    let mut lines = Vec::new();
    while let Some(answer) = stream.next_answer() {
        lines.push(answer.to_line()?);
    }
    let _ = stream.finish();
    feeder.join().map_err(|_| {
        JevError::io_failed_with("run_many feeder thread", std::io::ErrorKind::Other, None)
    })??;
    Ok(lines)
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
    ))]
    pub(crate) fn decision_stream_scripted(
        py: Python<'_>,
        scripted_answers: Bound<'_, PyAny>,
        concurrency: usize,
        backends: Option<Vec<String>>,
        repo_root: Option<PathBuf>,
        log_dir: Option<PathBuf>,
        session_dir: Option<PathBuf>,
    ) -> PyResult<DecisionStream> {
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
                false,
                None,
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
    ) -> PyResult<Vec<Py<PyAny>>> {
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
                false,
                None,
            ),
        )?;
        spec.scripted_answers = Some(answers);
        run_batch(py, spec, requests)
    }
}
