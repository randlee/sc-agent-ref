//! `build_request`, `run_check`, and session `ask`.
//!
//! Both Python entry points keep their Python names and call
//! `sc_jev_host::compose` (which calls `sc_jev_check::run::prepare` and
//! `run`); this module only maps the [`RunResponse`] into Python
//! (ADR-SAG-0024, ADR-SAG-0026). Every pyfunction here is `pub(crate)`: the
//! Python module registered in `lib.rs` is the only surface. Work that may
//! block (git, extractors, judge sends) runs under `py.detach`, so other
//! Python threads keep running.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyBytes};
use sc_agent_types::schema::subjects::SubjectsProducer;
use sc_agent_types::{
    BackendId, BackendSet, JevError, PayloadSha256, ProjectLabel, QuestionsSha256, RequestSha256,
    Rev, SessionId, Sha256Digest,
};
use sc_decision_schema::{MultiBackendMode, canonical_json_bytes};
use sc_jev_check::run::{CallerRequest, CheckSource, RunResponse};
use sc_jev_host::compose::{ComposeInputs, ask_from, explicit_repo_root, prepare_from};
use sc_jev_host::paths::PathPolicy;
use serde_json::Value;

use crate::error::{self, map_err};
use crate::logging::{self, FlushOnDrop, HostGeneralLog, Injected};

/// One Python `run_check` invocation, with owned arguments.
struct CheckRun {
    input: Input,
    /// `backends=`; `None` leaves the choice to `check.toml`, then `[jev]`.
    backends: Option<Vec<String>>,
    dry_run: bool,
    repo_root: Option<PathBuf>,
    rev: String,
    log_dir: Option<PathBuf>,
    session_dir: Option<PathBuf>,
    system_prompt_file: Option<PathBuf>,
    /// `chain_context=True` selects `MultiBackendMode::ChainContext`.
    chain_context: bool,
    /// `project=`: label for this run's log rows (REQ-SAG-0615).
    project: Option<ProjectLabel>,
    /// Loggers the caller injected; unset ones use the sc-agent defaults.
    loggers: Injected,
    /// Scripted judge answers; only `cargo test` and the `test-util` feature
    /// can set this (ADR-SAG-0022).
    #[cfg(any(test, feature = "test-util"))]
    scripted_answers: Option<Value>,
}

/// Where a run's questions come from: a `check.toml` path, or the bytes of a
/// caller-built questions document (the CLI's `--request-json`).
#[derive(Clone)]
enum Input {
    Check(PathBuf),
    Request(Vec<u8>),
}

impl Input {
    /// The one input of a Python call.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when both or neither of `check_path` and
    /// `request_json` is given, or `request_json` is neither `str` nor
    /// `bytes`.
    fn from_python(
        check_path: Option<PathBuf>,
        request_json: Option<&Bound<'_, PyAny>>,
    ) -> Result<Self, JevError> {
        match (check_path, request_json) {
            (Some(path), None) => Ok(Self::Check(path)),
            (None, Some(document)) => {
                let bytes = if let Ok(text) = document.extract::<String>() {
                    text.into_bytes()
                } else if let Ok(bytes) = document.cast::<PyBytes>() {
                    bytes.as_bytes().to_vec()
                } else {
                    return Err(JevError::input_invalid(
                        "request_json must be str or bytes holding the questions document; pass JSON text",
                    ));
                };
                Ok(Self::Request(bytes))
            }
            (Some(_), Some(_)) => Err(JevError::input_invalid(
                "pass check_path or request_json, not both; they are alternative sources of questions",
            )),
            (None, None) => Err(JevError::input_invalid(
                "pass check_path (a check.toml) or request_json (a questions document)",
            )),
        }
    }

    /// The engine source, resolving a check path against `repo_root`.
    fn source(&self, repo_root: &sc_agent_types::RepoRoot) -> Result<CheckSource, JevError> {
        match self {
            Self::Check(path) => CheckSource::check_file(repo_root, path),
            Self::Request(bytes) => {
                Ok(CheckSource::CallerRequest(CallerRequest::from_json(bytes)?))
            }
        }
    }
}

/// What `ask` needs to continue a live run: the host inputs resolved by the
/// run (repo root, log root, session directory, injected registry).
#[derive(Clone)]
struct LiveSession {
    inputs: ComposeInputs,
    /// The default general log, flushed after every follow-up.
    general: Option<Arc<HostGeneralLog>>,
}

/// Engine result for one run. `live` is `None` on dry-run.
struct FinishedRun {
    response: RunResponse,
    payload: String,
    system_prompt: Option<String>,
    live: Option<LiveSession>,
}

impl FinishedRun {
    fn session_id(&self) -> Option<SessionId> {
        self.response.session_id()
    }

    fn payload_sha256(&self) -> &PayloadSha256 {
        self.response.payload_sha256()
    }

    fn questions_sha256(&self) -> &QuestionsSha256 {
        self.response.questions_sha256()
    }
}

struct PreparedRequest {
    payload: String,
    payload_sha256: PayloadSha256,
    questions_sha256: QuestionsSha256,
    request_sha256: Vec<(BackendId, RequestSha256)>,
    producer: Option<SubjectsProducer>,
}

/// Canonical JSON bytes for `json_text` (ADR-SAG-0002).
///
/// # Errors
///
/// Returns [`JevError::InputInvalid`] when `json_text` is not JSON or cannot
/// be encoded.
pub(crate) fn canonical_of(json_text: &str) -> Result<Vec<u8>, JevError> {
    let value: Value = serde_json::from_str(json_text)
        .map_err(|error| JevError::encode_failed("JSON text", &error))?;
    canonical_json_bytes(&value)
}

/// The host inputs shared by every Python entry point.
pub(crate) fn base_inputs(
    repo_root: Option<&Path>,
    log_dir: Option<PathBuf>,
    session_dir: Option<PathBuf>,
) -> Result<ComposeInputs, JevError> {
    let explicit = repo_root.map(explicit_repo_root).transpose()?;
    let cwd = std::env::current_dir().or_else(|error| {
        if explicit.is_some() {
            // An explicit repo root never needs the working directory.
            Ok(PathBuf::from("."))
        } else {
            Err(JevError::io_failed("current directory", &error))
        }
    })?;
    let mut inputs = ComposeInputs::new(cwd, PathPolicy::python());
    if let Some(root) = explicit {
        inputs = inputs.repo_root(root);
    }
    if let Some(dir) = log_dir {
        inputs = inputs.log_dir(dir);
    }
    if let Some(dir) = session_dir {
        inputs = inputs.sessions_dir(dir);
    }
    Ok(inputs)
}

fn execute(spec: CheckRun) -> Result<FinishedRun, JevError> {
    let backends = parse_backends(spec.backends.as_deref())?;
    let rev = Rev::parse(&spec.rev)?;
    let mut inputs = base_inputs(spec.repo_root.as_deref(), spec.log_dir, spec.session_dir)?
        .dry_run(spec.dry_run)
        .rev(rev)
        .mode(if spec.chain_context {
            MultiBackendMode::ChainContext
        } else {
            MultiBackendMode::Independent
        });
    if let Some(backends) = backends {
        inputs = inputs.backends(backends);
    }
    if let Some(path) = spec.system_prompt_file {
        inputs = inputs.system_prompt_override(path);
    }
    #[cfg(any(test, feature = "test-util"))]
    if let Some(answers) = spec.scripted_answers {
        inputs = inputs.registry(scripted_registry(answers)?);
    }
    if let Some(log) = spec.loggers.session.clone() {
        inputs = inputs.session_log(log);
    }
    let resolved = inputs.resolve()?;
    // The general log: the caller's, else the shared host logger at this
    // call's log root. A dry run opens no default log, and a default that
    // cannot start leaves the sc-jev-host default in place.
    let mut general = None;
    match spec.loggers.general.clone() {
        Some(log) => inputs = inputs.general_log(log),
        None if !spec.dry_run => {
            if let Ok(host) = HostGeneralLog::open(&PathPolicy::python(), &resolved.log_root) {
                inputs = inputs.general_log(host.shared());
                general = Some(Arc::new(host));
            }
        }
        None => {}
    }
    let _flush = FlushOnDrop(general.clone());
    let source = spec.input.source(&resolved.repo_root)?;
    // The root is resolved once; the host does not discover it again.
    let inputs = inputs.repo_root(resolved.repo_root.clone());
    // The project belongs to this call only: a later `ask` names its own, so
    // the live session's inputs are cloned before it is set.
    let call_inputs = match spec.project {
        Some(project) => inputs.clone().project(project),
        None => inputs.clone(),
    };
    let prepared = prepare_from(source, call_inputs)?;
    let request = prepared.prepared().request();
    let payload = request.payload().as_str().to_owned();
    let system_prompt = request.llm().map(|llm| llm.system_prompt.clone());
    let response = prepared.run()?;
    // A live run's follow-ups use the paths this run resolved.
    let live = (!spec.dry_run).then(|| LiveSession {
        inputs: inputs
            .log_dir(resolved.log_root)
            .sessions_dir(resolved.sessions_dir),
        general,
    });
    Ok(FinishedRun {
        response,
        payload,
        system_prompt,
        live,
    })
}

/// The scripted registry for `answers` (ADR-SAG-0022, r-2b D5): passed
/// unchanged to `register_scripted_backends`. The per-backend form names the
/// backends it scripts; the shared form scripts every built-in backend.
#[cfg(any(test, feature = "test-util"))]
pub(crate) fn scripted_registry(
    answers: Value,
) -> Result<Arc<sc_jev_check::registry::BackendRegistry>, JevError> {
    let ids: Vec<sc_agent_types::BackendId> = match answers
        .as_object()
        .filter(|map| map.len() == 1)
        .and_then(|map| map.get("backends"))
        .and_then(Value::as_object)
    {
        Some(by_backend) => by_backend
            .keys()
            .map(|name| name.parse())
            .collect::<Result<_, _>>()?,
        None => sc_agent_types::BACKENDS
            .iter()
            .map(sc_agent_types::BackendDescriptor::id)
            .collect(),
    };
    sc_jev_check::registry::register_scripted_backends(&ids, answers).map(Arc::new)
}

fn build(
    input: &Input,
    repo_root: Option<&Path>,
    rev_text: &str,
) -> Result<PreparedRequest, JevError> {
    let rev = Rev::parse(rev_text)?;
    let inputs = base_inputs(repo_root, None, None)?.dry_run(true).rev(rev);
    let resolved = inputs.resolve()?;
    let source = input.source(&resolved.repo_root)?;
    let host = prepare_from(source, inputs.repo_root(resolved.repo_root))?;
    let prepared = host.prepared();
    Ok(PreparedRequest {
        payload: prepared.request().payload().as_str().to_owned(),
        payload_sha256: prepared.payload_sha256().clone(),
        questions_sha256: prepared.request().payload().questions_sha256()?,
        request_sha256: prepared.request_sha256(),
        producer: prepared.producer().cloned(),
    })
}

/// Parses `backends=` into a [`BackendSet`] with `BackendSet::from_str`.
///
/// Only `jev`, `haiku`, and `luna` are accepted, matching
/// `sc-ai` and the engine. Kebab-case aliases (`anthropic-haiku`) are not
/// accepted. An unknown id is `INPUT_INVALID`; an empty or repeated list is
/// `CONFIG_INVALID`. `None` leaves the selection to the engine.
pub(crate) fn parse_backends(names: Option<&[String]>) -> Result<Option<BackendSet>, JevError> {
    names.map(|names| names.join(",").parse()).transpose()
}

/// Drops an empty path, which Python callers use to mean "unset".
pub(crate) fn non_empty(path: Option<PathBuf>) -> Option<PathBuf> {
    path.filter(|path| !path.as_os_str().is_empty())
}

#[cfg(any(test, feature = "test-util"))]
pub(crate) fn py_to_value(obj: &Bound<'_, PyAny>) -> Result<Value, JevError> {
    let text = python_json(obj).map_err(|error| {
        JevError::input_invalid(format!("scripted_answers is not JSON: {error}"))
    })?;
    serde_json::from_str(&text).map_err(|error| JevError::encode_failed("scripted_answers", &error))
}

#[cfg(any(test, feature = "test-util"))]
pub(crate) fn python_json(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    let json = obj.py().import("json")?;
    json.call_method1("dumps", (obj,))?.extract()
}

fn encode_py_err(py: Python<'_>, label: &str, error: &serde_json::Error) -> PyErr {
    error::to_py_err(py, &JevError::encode_failed(label, error))
}

/// Converts `value` (from `serde_json::to_value`) to a Python object via JSON.
pub(crate) fn to_py(
    py: Python<'_>,
    label: &str,
    value: serde_json::Result<Value>,
) -> PyResult<Py<PyAny>> {
    let text = value
        .and_then(|value| serde_json::to_string(&value))
        .map_err(|error| encode_py_err(py, label, &error))?;
    Ok(py.import("json")?.call_method1("loads", (text,))?.unbind())
}

/// Validates the `project=` text of one call (`INPUT_INVALID` when it is over
/// 256 bytes or has a control character). `None` is "no label".
pub(crate) fn parse_project(project: Option<String>) -> Result<Option<ProjectLabel>, JevError> {
    project.map(ProjectLabel::new).transpose()
}

fn ask_with(
    py: Python<'_>,
    live: &LiveSession,
    id: SessionId,
    follow_up: &str,
    project: Option<String>,
) -> PyResult<Py<PyAny>> {
    let follow_up = follow_up.to_owned();
    let project = map_err(py, parse_project(project))?;
    let mut inputs = live.inputs.clone();
    if let Some(project) = project {
        inputs = inputs.project(project);
    }
    let general = live.general.clone();
    let outcome = py.detach(move || {
        let _flush = FlushOnDrop(general);
        ask_from(id, &follow_up, inputs)
    });
    let response = map_err(py, outcome)?;
    to_py(py, "outcome JSON", serde_json::to_value(response.outcome()))
}

/// Result of `run_check`, built from the engine's `RunResponse`.
///
/// Immutable. `outcomes` (the `ok` results, in backend order), `equal_input`
/// and `data` are converted to Python objects once, when the result is
/// built. `data` is the response body every consumer prints as the
/// envelope's `data` (without `readback`). `ask` is kept beside `Session.ask`
/// because the Python harness relies on it; both send through the same
/// session. `session` is empty on a dry run and when no backend that
/// supports `ask` answered.
#[pyclass(module = "sc_jev_py", name = "CheckResult", frozen)]
pub(crate) struct CheckResult {
    inner: FinishedRun,
    outcomes: Py<PyAny>,
    equal_input: Py<PyAny>,
    request_sha256: Py<PyAny>,
    data: Py<PyAny>,
}

/// `request_sha256` as a backend-id to digest mapping.
fn request_digest_map(digests: &[(BackendId, RequestSha256)]) -> serde_json::Value {
    serde_json::Value::Object(
        digests
            .iter()
            .map(|(backend, digest)| {
                (
                    backend.to_string(),
                    serde_json::Value::String(digest.as_str().to_owned()),
                )
            })
            .collect(),
    )
}

impl CheckResult {
    fn new(py: Python<'_>, inner: FinishedRun) -> PyResult<Self> {
        let outcomes: Vec<_> = inner.response.outcomes().collect();
        let outcomes = to_py(py, "outcomes JSON", serde_json::to_value(&outcomes))?;
        let equal_input = to_py(
            py,
            "equal-input JSON",
            serde_json::to_value(inner.response.equal_input()),
        )?;
        let request_sha256 = to_py(
            py,
            "request digests JSON",
            Ok(request_digest_map(inner.response.request_sha256())),
        )?;
        let data = to_py(py, "run data JSON", serde_json::to_value(&inner.response))?;
        Ok(Self {
            inner,
            outcomes,
            equal_input,
            request_sha256,
            data,
        })
    }
}

#[pymethods]
impl CheckResult {
    /// Saved session id. `None` on a dry run and when no backend that
    /// supports `ask` answered (a Jev-only run saves no session).
    #[getter]
    fn session_id(&self) -> Option<String> {
        self.inner.session_id().map(|id| id.to_string())
    }

    /// SHA-256 of the canonical payload. This is `payload_sha256`.
    #[getter]
    fn payload_sha256(&self) -> &str {
        self.inner.payload_sha256().as_str()
    }

    /// SHA-256 of the question set alone (ids, kinds, instructions and
    /// criteria descriptions), for a consumer's cache lookup. The exact bytes
    /// are in `tests/fixtures/README.md`.
    #[getter]
    fn questions_sha256(&self) -> &str {
        self.inner.questions_sha256().as_str()
    }

    /// Per backend id, the SHA-256 of the exact first request body that
    /// backend is sent, computed before sending (no key, no send). It
    /// identifies an identical request, so a consumer can substitute a cached
    /// response. Equals the session-log `request_sha256` of the same send.
    /// A backend is absent when its bytes depend on an earlier answer
    /// (`chain_context`) or it refuses the questions. `jev` is sent as
    /// `jev-latest`: cache on pinned versions.
    #[getter]
    fn request_sha256(&self, py: Python<'_>) -> Py<PyAny> {
        self.request_sha256.clone_ref(py)
    }

    /// Canonical payload JSON.
    #[getter]
    fn payload(&self) -> &str {
        &self.inner.payload
    }

    /// Outcomes of the backends that answered, in backend order. Empty on
    /// dry-run.
    #[getter]
    fn outcomes(&self, py: Python<'_>) -> Py<PyAny> {
        self.outcomes.clone_ref(py)
    }

    /// Equal-input record. On a dry run every backend is `not_sent`.
    #[getter]
    fn equal_input(&self, py: Python<'_>) -> Py<PyAny> {
        self.equal_input.clone_ref(py)
    }

    /// The run response body: the same `data` the `sc-ai` envelope carries,
    /// minus its `readback` key. Includes `warnings` when the session log
    /// was not flushed.
    #[getter]
    fn data(&self, py: Python<'_>) -> Py<PyAny> {
        self.data.clone_ref(py)
    }

    /// LLM system prompt injected for this run. Not part of `payload_sha256`.
    #[getter]
    fn system_prompt(&self) -> Option<&str> {
        self.inner.system_prompt.as_deref()
    }

    /// Session handle for `Session.ask`. `None` on dry-run.
    #[getter]
    fn session(&self) -> Option<Session> {
        let live = self.inner.live.as_ref()?;
        self.inner.session_id().map(|id| Session {
            id,
            live: live.clone(),
        })
    }

    /// Sends `follow_up` to the live session's Haiku or Luna backend.
    ///
    /// Same as `self.session.ask(follow_up, project=project)`.
    ///
    /// # Errors
    ///
    /// Raises `JevError`. Dry-run uses `CONFIG_INVALID`. A run with no
    /// session (Jev only) uses `INPUT_INVALID`, as does a `project` over 256
    /// bytes or with a control character. The exception carries the CLI error
    /// body's fields (`code`, `kind`, `message`, `suggested_action`, ...).
    #[pyo3(signature = (follow_up, *, project=None))]
    fn ask(&self, py: Python<'_>, follow_up: &str, project: Option<String>) -> PyResult<Py<PyAny>> {
        let Some(live) = self.inner.live.as_ref() else {
            return Err(error::to_py_err(
                py,
                &JevError::config_invalid("dry-run does not create a session; ask a live run"),
            ));
        };
        let Some(id) = self.inner.session_id() else {
            return Err(error::to_py_err(
                py,
                &JevError::input_invalid_field(
                    "backends",
                    "this run saved no session: only Haiku and Luna support ask, and Jev has no rationale field; run with backends=[\"haiku\"] or [\"luna\"] to ask",
                ),
            ));
        };
        ask_with(py, live, id, follow_up, project)
    }

    fn __repr__(&self) -> String {
        format!(
            "CheckResult(session_id={:?}, payload_sha256={})",
            self.session_id(),
            self.inner.payload_sha256()
        )
    }
}

/// Live judge session returned by `CheckResult.session`.
#[pyclass(module = "sc_jev_py", name = "Session", frozen)]
pub(crate) struct Session {
    id: SessionId,
    live: LiveSession,
}

#[pymethods]
impl Session {
    /// Session id.
    #[getter]
    fn session_id(&self) -> String {
        self.id.to_string()
    }

    /// Sends `follow_up` through `sc-jev-check` and appends one JSONL line.
    ///
    /// `project` labels this call's log row only; it is per call, caller
    /// text, stored as given and not scrubbed.
    ///
    /// # Errors
    ///
    /// Raises `JevError`. A Jev session uses `INPUT_INVALID` because Jev has
    /// no rationale field, and so does a `project` over 256 bytes or with a
    /// control character. `code` is the stable id and `message` is the fix
    /// hint.
    #[pyo3(signature = (follow_up, *, project=None))]
    fn ask(&self, py: Python<'_>, follow_up: &str, project: Option<String>) -> PyResult<Py<PyAny>> {
        ask_with(py, &self.live, self.id, follow_up, project)
    }

    fn __repr__(&self) -> String {
        format!("Session(session_id={})", self.id)
    }
}

/// Builds the canonical payload for repo-relative `check_path`, or for a
/// caller-built questions document `request_json` (`str` or `bytes`, the CLI's
/// `--request-json` document), without sending. Exactly one of the two is
/// given; both or neither is `INPUT_INVALID`.
///
/// Assembly is `sc_jev_host::compose::prepare_from` over
/// `sc_jev_check::run::prepare` (ADR-SAG-0005, ADR-SAG-0024). This function
/// maps the prepared run into Python. It does not run extractors or build the
/// payload itself. `repo_root` defaults to the git toplevel of the process
/// current directory (the directory itself outside git).
/// `rev` defaults to `WORKTREE`. The GIL is released while the request is
/// assembled (git reads and command extractors may take seconds). The result
/// mapping contains `payload`, `payload_sha256`, `questions_sha256`, `request_sha256` (a backend-id to digest mapping), and `producer`.
///
/// # Errors
///
/// Raises `JevError` when the check file or payload cannot be built. It
/// carries the CLI error body's fields (`code`, `kind`, `message`,
/// `suggested_action`, ...).
///
/// # Examples
///
/// ```text
/// built = jev.build_request("check.toml", repo_root=pathlib.Path("/path/to/repo"))
/// ```
#[pyfunction]
#[pyo3(signature = (check_path=None, *, request_json=None, repo_root=None, rev="WORKTREE"))]
pub(crate) fn build_request(
    py: Python<'_>,
    check_path: Option<PathBuf>,
    request_json: Option<Bound<'_, PyAny>>,
    repo_root: Option<PathBuf>,
    rev: &str,
) -> PyResult<Py<PyAny>> {
    let input = map_err(py, Input::from_python(check_path, request_json.as_ref()))?;
    let repo_root = non_empty(repo_root);
    let rev = rev.to_owned();
    let built = py.detach(move || build(&input, repo_root.as_deref(), &rev));
    let built = map_err(py, built)?;
    let body = serde_json::json!({
        "payload": built.payload,
        "payload_sha256": built.payload_sha256,
        "questions_sha256": built.questions_sha256,
        "request_sha256": request_digest_map(&built.request_sha256),
        "producer": built.producer,
    });
    to_py(py, "build_request JSON", Ok(body))
}

/// Builds the owned [`CheckRun`] shared by both Python entry points.
#[expect(clippy::too_many_arguments, reason = "mirrors the Python keyword API")]
fn check_run(
    input: Input,
    backends: Option<Vec<String>>,
    dry_run: bool,
    repo_root: Option<PathBuf>,
    rev: String,
    log_dir: Option<PathBuf>,
    session_dir: Option<PathBuf>,
    system_prompt_file: Option<PathBuf>,
    chain_context: bool,
    project: Option<String>,
    loggers: Injected,
) -> Result<CheckRun, JevError> {
    Ok(CheckRun {
        input,
        backends,
        dry_run,
        repo_root: non_empty(repo_root),
        rev,
        log_dir: non_empty(log_dir),
        session_dir: non_empty(session_dir),
        system_prompt_file: non_empty(system_prompt_file),
        chain_context,
        project: parse_project(project)?,
        loggers,
        #[cfg(any(test, feature = "test-util"))]
        scripted_answers: None,
    })
}

fn finish_check(py: Python<'_>, spec: CheckRun) -> PyResult<CheckResult> {
    let finished = py.detach(move || execute(spec));
    let finished = map_err(py, finished)?;
    // REQ-SAG-0604: a partial response raises, carrying the first failure's
    // code and the whole body as `.response` (the answers that succeeded).
    if let Some(failure) = finished
        .response
        .is_partial()
        .then(|| finished.response.first_failure())
        .flatten()
    {
        let data = to_py(
            py,
            "run data JSON",
            serde_json::to_value(&finished.response),
        )?;
        return Err(error::to_py_err_with_response(py, failure.error(), data));
    }
    CheckResult::new(py, finished)
}

/// Runs `check_path` (or the caller-built questions document `request_json`,
/// as `str` or `bytes`; exactly one of the two) through `sc-jev-check`.
///
/// `backends` wins over `check.toml`'s `backends`; with neither the run uses
/// `["jev"]`. Ids are `jev`, `haiku`, and `luna`. `dry_run`
/// returns the payload digest and does not resolve keys, send, or create a
/// session store or log. `chain_context=True` lets later backends see earlier
/// answers; the default is independent (ADR-SAG-0008). Path arguments accept
/// `str` or `os.PathLike`. This function does not register a judge. Live runs
/// use the HTTP backends inside `sc-jev-check` (ADR-SAG-0009). The GIL is
/// released while the check runs.
///
/// Log and session paths follow ADR-SAG-0021. See the module documentation
/// for `SC_AGENT_LOG_DIR`, `SC_LOG_ROOT`, and the JSONL path. `project`
/// labels this run's log rows (per call; caller text, stored as given and
/// not scrubbed, so put no secret in it).
///
/// # Errors
///
/// Raises `JevError` with `code` and `message` set from the facade error. An
/// unknown backend id, a malformed `rev`, or a `project` over 256 bytes or
/// with a control character is `INPUT_INVALID`. When any
/// backend fails, the `JevError` carries the first failure's fields and a
/// `response` attribute holding the response body (with the successful
/// answers if any, and every failed backend's typed error) as a dict.
///
/// # Examples
///
/// ```text
/// import sc_jev_py as jev
/// result = jev.run_check("check.toml", backends=["jev"], dry_run=False)
/// ```
#[pyfunction]
#[pyo3(signature = (
    check_path=None,
    *,
    request_json=None,
    backends=None,
    dry_run=false,
    repo_root=None,
    rev="WORKTREE",
    log_dir=None,
    session_dir=None,
    system_prompt_file=None,
    chain_context=false,
    project=None,
    session_log=None,
    general_log=None,
))]
#[expect(clippy::too_many_arguments, reason = "Python keyword API")]
pub(crate) fn run_check(
    py: Python<'_>,
    check_path: Option<PathBuf>,
    request_json: Option<Bound<'_, PyAny>>,
    backends: Option<Vec<String>>,
    dry_run: bool,
    repo_root: Option<PathBuf>,
    rev: &str,
    log_dir: Option<PathBuf>,
    session_dir: Option<PathBuf>,
    system_prompt_file: Option<PathBuf>,
    chain_context: bool,
    project: Option<String>,
    session_log: Option<Py<PyAny>>,
    general_log: Option<Py<PyAny>>,
) -> PyResult<CheckResult> {
    let loggers = map_err(py, logging::injected(session_log, general_log, py))?;
    let input = map_err(py, Input::from_python(check_path, request_json.as_ref()))?;
    let spec = map_err(
        py,
        check_run(
            input,
            backends,
            dry_run,
            repo_root,
            rev.to_owned(),
            log_dir,
            session_dir,
            system_prompt_file,
            chain_context,
            project,
            loggers,
        ),
    )?;
    finish_check(py, spec)
}

/// Test-only entry points, registered as `sc_jev_py._testing` (ADR-SAG-0022).
#[cfg(any(test, feature = "test-util"))]
pub(crate) mod testing {
    use super::*;

    /// `run_check` with scripted judge answers instead of HTTP backends.
    ///
    /// `scripted_answers` is the r-2b D5 JSON (shared or per-backend form) as
    /// a Python mapping. It is forwarded unchanged to
    /// `sc_jev_check::registry::register_scripted_backends`, which owns the
    /// shape rules. This crate implements no judge (ADR-SAG-0009).
    ///
    /// # Errors
    ///
    /// Raises `JevError` like `run_check`. `scripted_answers` that is not JSON
    /// or not a mapping of question ids is `INPUT_INVALID`.
    #[pyfunction]
    #[pyo3(signature = (
        check_path=None,
        *,
        request_json=None,
        scripted_answers,
        backends=None,
        dry_run=false,
        repo_root=None,
        rev="WORKTREE",
        log_dir=None,
        session_dir=None,
        system_prompt_file=None,
        chain_context=false,
        project=None,
        session_log=None,
        general_log=None,
    ))]
    #[expect(clippy::too_many_arguments, reason = "Python keyword API")]
    pub(crate) fn run_check_scripted(
        py: Python<'_>,
        check_path: Option<PathBuf>,
        request_json: Option<Bound<'_, PyAny>>,
        scripted_answers: Bound<'_, PyAny>,
        backends: Option<Vec<String>>,
        dry_run: bool,
        repo_root: Option<PathBuf>,
        rev: &str,
        log_dir: Option<PathBuf>,
        session_dir: Option<PathBuf>,
        system_prompt_file: Option<PathBuf>,
        chain_context: bool,
        project: Option<String>,
        session_log: Option<Py<PyAny>>,
        general_log: Option<Py<PyAny>>,
    ) -> PyResult<CheckResult> {
        let loggers = map_err(py, logging::injected(session_log, general_log, py))?;
        let answers = map_err(py, py_to_value(&scripted_answers))?;
        let input = map_err(py, Input::from_python(check_path, request_json.as_ref()))?;
        let mut spec = map_err(
            py,
            check_run(
                input,
                backends,
                dry_run,
                repo_root,
                rev.to_owned(),
                log_dir,
                session_dir,
                system_prompt_file,
                chain_context,
                project,
                loggers,
            ),
        )?;
        spec.scripted_answers = Some(answers);
        finish_check(py, spec)
    }
}

/// Returns canonical payload bytes for `json_text`.
///
/// Python must pass the JSON text through. This function parses it in Rust
/// and encodes ADR-SAG-0002 bytes. It does not accept a pre-canonicalized
/// digest from Python.
///
/// # Errors
///
/// Raises `JevError` with code `INPUT_INVALID` when `json_text` is not JSON.
#[pyfunction]
pub(crate) fn canonical_bytes(py: Python<'_>, json_text: &str) -> PyResult<Py<PyBytes>> {
    let bytes = map_err(py, canonical_of(json_text))?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

/// Returns the SHA-256 hex digest of `canonical_bytes(json_text)`.
///
/// # Errors
///
/// Raises `JevError` with code `INPUT_INVALID` when `json_text` is not JSON.
#[pyfunction]
pub(crate) fn canonical_sha256(py: Python<'_>, json_text: &str) -> PyResult<String> {
    let bytes = map_err(py, canonical_of(json_text))?;
    // `canonical_json_bytes` is `serde_json` output, so UTF-8 by construction.
    let text = String::from_utf8(bytes)
        .map_err(|_| JevError::input_invalid("canonical payload is not UTF-8"));
    let text = map_err(py, text)?;
    Ok(Sha256Digest::of_text(&text).to_string())
}

/// The Python path policy; the repo root does not enter its defaults.
fn policy_root() -> sc_agent_types::RepoRoot {
    sc_agent_types::RepoRoot::new(".")
}

/// Returns the default log root (`~/.sc-agent/log`).
#[pyfunction]
#[must_use]
pub(crate) fn default_log_root() -> PathBuf {
    PathPolicy::python().default_log_root(&policy_root())
}

/// Resolves the log root (ADR-SAG-0021).
///
/// First match wins: `log_dir`, then `SC_AGENT_LOG_DIR`, then `SC_LOG_ROOT`,
/// then `~/.sc-agent/log` (`%USERPROFILE%\.sc-agent\log` when `HOME` is unset).
/// When both home variables are unset, the path is the process temp directory
/// plus `sc-agent/log`. An empty argument or environment value is absent.
#[pyfunction]
#[pyo3(signature = (log_dir=None))]
#[must_use]
pub(crate) fn resolve_log_root(log_dir: Option<PathBuf>) -> PathBuf {
    PathPolicy::python().resolve_log_root(log_dir.as_deref(), &policy_root())
}

/// Resolves the session directory (ADR-SAG-0021).
///
/// `log_dir` is the log root from `resolve_log_root`. First match wins:
/// `session_dir`, then `SC_AGENT_SESSION_DIR`, then `{log_dir}/sessions`.
/// An empty `session_dir` or environment value is absent.
#[pyfunction]
#[pyo3(signature = (log_dir, *, session_dir=None))]
#[must_use]
pub(crate) fn resolve_session_dir(log_dir: PathBuf, session_dir: Option<PathBuf>) -> PathBuf {
    PathPolicy::python().resolve_session_dir(session_dir.as_deref(), &log_dir, &policy_root())
}

/// Returns `{log_dir}/logs/sc-jev-py.log.jsonl`.
#[pyfunction]
#[must_use]
pub(crate) fn log_file(log_dir: PathBuf) -> PathBuf {
    PathPolicy::python().jsonl_path(&log_dir)
}

/// Returns `{log_dir}/logs/sc-jev-py-general.log.jsonl`, the default
/// general log (separate from the session log, `log_file`).
#[pyfunction]
#[must_use]
pub(crate) fn general_log_file(log_dir: PathBuf) -> PathBuf {
    let policy = PathPolicy::python();
    log_dir.join("logs").join(format!(
        "{}.log.jsonl",
        sc_jev_host::obs::GeneralLogAdapter::service_name(&policy)
    ))
}

/// Starts the shared host logger now and returns its log file.
///
/// The logger is the `sc-observability` core logger this wheel links, and
/// Rust writes its general log through it with no GIL. After this call,
/// `sc_observability.get_host_logger()` returns a logger over the same queue
/// and file (import `sc_jev_py` first). `log_dir` follows `resolve_log_root`.
/// One process has one host logger; a second call with another root returns
/// the first installed logger's file, not the requested root's. The return
/// value is always the file Python's logger writes to.
///
/// # Errors
///
/// Raises `JevError` (`CONFIG_INVALID`) when the `sc_observability` runtime
/// was already loaded by another wheel, or the logger cannot start;
/// (`IO_FAILED`) when the root cannot be created.
#[pyfunction]
#[pyo3(signature = (log_dir=None))]
pub(crate) fn install_host_logger(py: Python<'_>, log_dir: Option<PathBuf>) -> PyResult<PathBuf> {
    let root = PathPolicy::python().resolve_log_root(log_dir.as_deref(), &policy_root());
    map_err(py, py.detach(move || logging::install_at(&root)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sc_agent_types::BackendId;
    use tempfile::TempDir;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn temp_dir(label: &str) -> TempDir {
        tempfile::Builder::new()
            .prefix(&format!("sc-jev-py-{label}-"))
            .tempdir()
            .expect("temp dir")
    }

    fn write_questions(dir: &Path, instructions: &str) {
        let questions = format!(
            r#"{{
              "schema": "plan-anchoring/questions/1",
              "version": 1,
              "questions": {{
                "relevant": {{
                  "type": "choice",
                  "instructions": {instructions:?},
                  "criteria": {{"no": "No.", "yes": "Yes."}}
                }}
              }}
            }}"#
        );
        std::fs::write(dir.join("questions.json"), questions).expect("questions");
        std::fs::write(
            dir.join("check.toml"),
            "[questions]\npath = \"questions.json\"\n",
        )
        .expect("check");
    }

    fn run_spec(
        repo: &Path,
        logs: &Path,
        sessions: &Path,
        backend: &str,
        dry_run: bool,
    ) -> CheckRun {
        CheckRun {
            input: Input::Check(PathBuf::from("check.toml")),
            backends: Some(vec![backend.to_owned()]),
            dry_run,
            repo_root: Some(repo.to_path_buf()),
            rev: "WORKTREE".to_owned(),
            log_dir: Some(logs.to_path_buf()),
            session_dir: Some(sessions.to_path_buf()),
            system_prompt_file: None,
            chain_context: false,
            project: None,
            loggers: Injected::default(),
            scripted_answers: Some(serde_json::json!({"relevant": "yes"})),
        }
    }

    fn live(finished: &FinishedRun) -> &LiveSession {
        finished.live.as_ref().expect("live run")
    }

    /// `ask` through the run's own (scripted) registry and logger.
    fn ask_live(
        finished: &FinishedRun,
        follow_up: &str,
    ) -> Result<sc_decision_schema::JudgeOutcome, JevError> {
        ask_from(
            finished.session_id().expect("session"),
            follow_up,
            live(finished).inputs.clone(),
        )
        .map(|response| response.outcome().clone())
    }

    fn jsonl_lines(logs: &Path) -> Vec<String> {
        let path = PathPolicy::python().jsonl_path(logs);
        if !path.exists() {
            return Vec::new();
        }
        std::fs::read_to_string(path)
            .expect("jsonl")
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn branches_fixture_payload_includes_subjects() {
        let root = temp_dir("branches");
        let repo = root.path();
        write_questions(repo, "Branches:\n\n{branches}");
        let subjects = workspace_root().join("tests/fixtures/cross-impl/branches/subjects.json");
        std::fs::copy(subjects, repo.join("subjects.json")).expect("copy subjects");
        std::fs::write(
            repo.join("check.toml"),
            "[questions]\npath = \"questions.json\"\n\n[inputs.branches]\nkind = \"subjects\"\npath = \"subjects.json\"\n",
        )
        .expect("check");
        let built = build(
            &Input::Check(PathBuf::from("check.toml")),
            Some(repo),
            "WORKTREE",
        )
        .expect("build");
        assert!(built.payload.contains("Transition.GIVE_BACK"));
        let producer = built.producer.expect("producer");
        assert_eq!(producer.name(), "sc-treesitter");
        assert_eq!(producer.language(), "python");
        assert_eq!(producer.mode().as_str(), "matches");
        let again = build(
            &Input::Check(PathBuf::from("check.toml")),
            Some(repo),
            "WORKTREE",
        )
        .expect("rebuild");
        assert_eq!(built.payload_sha256, again.payload_sha256);
    }

    const REQUEST: &str = r#"{"questions":[{"id":"relevant","prompt":"Does it apply?","choices":["no","yes"]}],"state":[]}"#;

    #[test]
    fn request_json_builds_the_same_payload_as_the_cli_document() {
        let repo = tempfile::tempdir().expect("repo");
        let input = Input::Request(REQUEST.as_bytes().to_vec());
        let built = build(&input, Some(repo.path()), "WORKTREE").expect("build");
        assert_eq!(
            built.payload,
            r#"{"questions":[{"choices":["no","yes"],"id":"relevant","prompt":"Does it apply?"}],"state":[]}"#
        );
        assert_eq!(built.request_sha256.len(), 1, "jev is the default backend");
    }

    #[test]
    fn git_repo_dry_run_creates_no_session_store_or_log() {
        let root = temp_dir("dry");
        let logs = root.path().join("log");
        let sessions = root.path().join("sessions");
        let repo = workspace_root().join("tests/fixtures/jev/git-repo");
        let finished = execute(run_spec(&repo, &logs, &sessions, "jev", true)).expect("dry-run");
        assert!(finished.session_id().is_none());
        assert!(finished.live.is_none());
        assert!(finished.payload.contains("alpha note"));
        assert!(finished.response.dry_run());
        assert!(finished.response.results().is_empty());
        assert!(
            finished
                .response
                .equal_input()
                .backends()
                .iter()
                .all(|entry| entry.attestation() == &sc_decision_schema::Attestation::NotSent)
        );
        assert!(!logs.exists(), "dry-run must not create the log root");
        assert!(
            !sessions.exists(),
            "dry-run must not create the session dir"
        );
    }

    #[test]
    fn live_run_appends_one_jsonl_line_per_send_without_the_prompt() {
        let root = temp_dir("live");
        let repo = root.path().join("repo");
        let logs = root.path().join("log");
        let sessions = root.path().join("sessions");
        std::fs::create_dir_all(&repo).expect("repo");
        let sentinel = "sentinel-question-text-not-for-logs";
        write_questions(&repo, sentinel);
        let finished = execute(run_spec(&repo, &logs, &sessions, "haiku", false)).expect("run");
        assert!(finished.session_id().is_some());
        assert!(finished.payload.contains(sentinel));
        assert!(!finished.system_prompt.as_deref().unwrap_or("").is_empty());
        let lines = jsonl_lines(&logs);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("judge"));
        assert!(lines[0].contains(finished.payload_sha256().as_str()));
        assert!(lines[0].contains("haiku"));
        assert!(!lines[0].contains(sentinel));
        assert!(!lines[0].to_ascii_lowercase().contains("api_key"));
        let follow = ask_live(&finished, "why?");
        let outcome = follow.expect("ask");
        assert_eq!(outcome.backend(), BackendId::Haiku);
        let lines = jsonl_lines(&logs);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[1].contains("ask"));
        assert!(!lines[1].contains(sentinel));
        assert!(!lines[1].contains("why?"));
    }

    #[test]
    fn jev_ask_is_input_invalid_and_missing_check_is_input_missing() {
        let root = temp_dir("errors");
        let repo = root.path().join("repo");
        let logs = root.path().join("log");
        let sessions = root.path().join("sessions");
        std::fs::create_dir_all(&repo).expect("repo");
        write_questions(&repo, "Does it apply?");
        let finished = execute(run_spec(&repo, &logs, &sessions, "jev", false)).expect("run");
        // Jev has no `ask`, so a Jev-only run saves no session file.
        assert!(finished.session_id().is_none());
        assert_eq!(std::fs::read_dir(&sessions).map_or(0, Iterator::count), 0);
        assert_eq!(jsonl_lines(&logs).len(), 1);
        let mut spec = run_spec(&repo, &logs, &sessions, "jev", false);
        spec.input = Input::Check(PathBuf::from("missing.toml"));
        spec.scripted_answers = None;
        let Err(missing) = execute(spec) else {
            panic!("missing check should fail");
        };
        assert_eq!(missing.code(), "INPUT_MISSING");
        assert!(!missing.message().to_ascii_lowercase().contains("api_key"));
    }

    #[test]
    fn backend_ids_use_from_str_and_reject_kebab_aliases() {
        assert_eq!(
            parse_backends(Some(&["jev".into(), "haiku".into(), "luna".into()]))
                .expect("documented ids")
                .expect("set")
                .as_slice(),
            [BackendId::Jev, BackendId::Haiku, BackendId::Luna]
        );
        assert!(parse_backends(None).expect("unset").is_none());
        for alias in ["anthropic-haiku", "openai-luna", "not-a-backend"] {
            let error = parse_backends(Some(&[alias.to_owned()])).expect_err(alias);
            assert_eq!(error.code(), "INPUT_INVALID", "{alias}");
            assert!(error.to_string().contains(alias), "{error}");
        }
        assert_eq!(
            parse_backends(Some(&[])).expect_err("empty").code(),
            "CONFIG_INVALID"
        );
    }

    #[test]
    fn malformed_rev_and_missing_repo_are_typed() {
        let root = temp_dir("rev");
        write_questions(root.path(), "Rev?");
        for rev in ["", "--output=x", "a b"] {
            let error = build(
                &Input::Check(PathBuf::from("check.toml")),
                Some(root.path()),
                rev,
            )
            .err()
            .expect("malformed rev");
            assert_eq!(error.code(), "INPUT_INVALID", "{rev:?}");
        }
        let absent = root.path().join("absent");
        let error = explicit_repo_root(&absent).expect_err("absent repo");
        assert_eq!(error.code(), "INPUT_MISSING");
        let file = root.path().join("check.toml");
        assert_eq!(
            explicit_repo_root(&file).expect_err("file repo").code(),
            "INPUT_MISSING"
        );
        let relative = explicit_repo_root(Path::new(".")).expect("relative repo");
        assert!(relative.as_path().is_absolute());
    }

    #[test]
    fn serde_encode_failure_is_jev_error() {
        struct FailWriter;
        impl std::io::Write for FailWriter {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("encode refused"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let failure = serde_json::to_writer(FailWriter, &serde_json::json!({"ok": true}))
            .expect_err("writer");
        Python::initialize();
        Python::attach(|py| {
            let py_err = encode_py_err(py, "outcomes JSON", &failure);
            assert!(py_err.is_instance_of::<crate::error::JevError>(py));
            assert!(!py_err.is_instance_of::<pyo3::exceptions::PyValueError>(py));
            let code: String = py_err
                .value(py)
                .getattr("code")
                .expect("code")
                .extract()
                .expect("code str");
            assert_eq!(code, "INPUT_INVALID");
            let message: String = py_err
                .value(py)
                .getattr("message")
                .expect("message")
                .extract()
                .expect("message str");
            assert!(message.contains("outcomes JSON"), "{message}");
            assert!(!message.contains("encode refused"), "{message}");
        });
    }

    #[test]
    fn scripted_answers_that_are_not_json_are_input_invalid() {
        Python::initialize();
        Python::attach(|py| {
            let nan = py.eval(c"float('nan')", None, None).expect("nan");
            let error = py_to_value(&nan).expect_err("nan is not JSON");
            assert_eq!(error.code(), "INPUT_INVALID");
            assert!(error.message().contains("scripted_answers"));

            let opaque = py.eval(c"object()", None, None).expect("object");
            let error = py_to_value(&opaque).expect_err("object is not JSON");
            assert_eq!(error.code(), "INPUT_INVALID");
            assert!(error.message().contains("scripted_answers"));
        });
    }

    #[test]
    fn pyfunctions_accept_pathlike_and_return_paths() {
        let root = temp_dir("pyfn");
        let repo = root.path().join("repo");
        std::fs::create_dir_all(&repo).expect("repo");
        write_questions(&repo, "Path?");
        Python::initialize();
        Python::attach(|py| {
            let built = build_request(
                py,
                Some(PathBuf::from("check.toml")),
                None,
                Some(repo.clone()),
                "WORKTREE",
            )
            .expect("build_request");
            let digest: String = built
                .bind(py)
                .get_item("payload_sha256")
                .expect("digest")
                .extract()
                .expect("str");
            assert_eq!(digest.len(), 64);

            let result = run_check(
                py,
                Some(PathBuf::from("check.toml")),
                None,
                None,
                true,
                Some(repo.clone()),
                "WORKTREE",
                None,
                None,
                None,
                false,
                None,
                None,
                None,
            )
            .expect("dry run");
            assert_eq!(result.payload_sha256(), digest);
            assert!(result.session().is_none());
            let equal: bool = result
                .equal_input(py)
                .bind(py)
                .get_item("equal")
                .expect("equal")
                .extract()
                .expect("bool");
            assert!(!equal, "a dry run sends nothing, so equal is false");
            let error = result.ask(py, "why?", None).expect_err("dry-run ask");
            let code: String = error
                .value(py)
                .getattr("code")
                .expect("code")
                .extract()
                .expect("str");
            assert_eq!(code, "CONFIG_INVALID");

            let logs = root.path().join("log");
            let live = testing::run_check_scripted(
                py,
                Some(PathBuf::from("check.toml")),
                None,
                py.eval(c"{'relevant': 'yes'}", None, None)
                    .expect("answers"),
                Some(vec!["haiku".to_owned()]),
                false,
                Some(repo.clone()),
                "WORKTREE",
                Some(logs.clone()),
                Some(root.path().join("sessions")),
                None,
                false,
                None,
                None,
                None,
            )
            .expect("scripted run");
            let outcomes = live.outcomes(py);
            let backend: String = outcomes
                .bind(py)
                .get_item(0)
                .and_then(|outcome| outcome.get_item("backend"))
                .expect("backend")
                .extract()
                .expect("str");
            assert_eq!(backend, "haiku");
            assert!(outcomes.is(live.outcomes(py)), "converted once");
            let session = live.session().expect("session");
            assert_eq!(Some(session.session_id()), live.session_id());
            session.ask(py, "why?", None).expect("session ask");
            live.ask(py, "and?", None).expect("result ask");
            assert!(live.__repr__().contains("CheckResult"));
            assert!(session.__repr__().contains("Session"));
            assert!(!live.payload().is_empty());

            assert_eq!(
                log_file(logs.clone()),
                logs.join("logs").join("sc-jev-py.log.jsonl")
            );
            assert_eq!(resolve_log_root(Some(logs.clone())), logs);
            assert_eq!(
                resolve_session_dir(logs.clone(), Some(root.path().join("s"))),
                root.path().join("s")
            );
            assert!(default_log_root().ends_with("log"));
            let bytes = canonical_bytes(py, "{\"b\":1,\"a\":2}").expect("bytes");
            assert_eq!(bytes.bind(py).as_bytes(), b"{\"a\":2,\"b\":1}");
            assert_eq!(
                canonical_sha256(py, "{}").expect("sha"),
                Sha256Digest::of_text("{}").to_string()
            );
            assert!(canonical_sha256(py, "{").is_err());
        });
    }

    /// Stops and joins the Python worker even when an assertion panics.
    /// Only the Unix GIL test (which needs `sh`) uses it.
    #[cfg(unix)]
    struct WorkerGuard(Py<pyo3::types::PyDict>);

    #[cfg(unix)]
    impl Drop for WorkerGuard {
        fn drop(&mut self) {
            Python::attach(|py| {
                let _ = py.run(
                    c"stop.set()\nworker.join(30)\n",
                    Some(self.0.bind(py)),
                    None,
                );
            });
        }
    }

    /// `build_request` releases the GIL while a command extractor runs.
    ///
    /// Sentinel handshake (FTQ-007), no rate or tick count: the extractor
    /// writes `started`, then polls up to 20 s for `ack`. A Python worker
    /// thread waits for `started` and writes `ack`; it can only run if the
    /// GIL is free. With `ack` the extractor prints a non-subjects document
    /// (`EXTRACTION_INVALID`); without it the extractor exits 3
    /// (`EXTRACTION_FAILED`). The extractor timeout (60 s) is above both
    /// bounds.
    #[cfg(unix)]
    #[test]
    fn build_request_releases_the_gil_during_a_slow_extractor() {
        let root = temp_dir("gil");
        let repo = root.path();
        let started = repo.join("started");
        let ack = repo.join("ack");
        write_questions(repo, "Slow:\n\n{slow}");
        let script = "touch \"$1\"; i=0; while [ ! -e \"$2\" ]; do i=$((i+1)); [ $i -gt 400 ] && exit 3; sleep 0.05; done; echo not-subjects";
        let command = serde_json::to_string(&[
            "sh",
            "-c",
            script,
            "sh",
            started.to_str().expect("utf8 path"),
            ack.to_str().expect("utf8 path"),
        ])
        .expect("argv");
        std::fs::write(
            repo.join("check.toml"),
            format!(
                "[questions]\npath = \"questions.json\"\n\n[inputs.slow]\nkind = \"command\"\ncommand = {command}\ntimeout_seconds = 60\n"
            ),
        )
        .expect("check");
        Python::initialize();
        Python::attach(|py| {
            let scope = pyo3::types::PyDict::new(py);
            scope
                .set_item("started", started.to_str().expect("utf8 path"))
                .expect("started");
            scope
                .set_item("ack", ack.to_str().expect("utf8 path"))
                .expect("ack");
            py.run(
                c"import os, threading, time\nstop = threading.Event()\ndef answer():\n    deadline = time.monotonic() + 20\n    while not stop.is_set() and time.monotonic() < deadline:\n        if os.path.exists(started):\n            open(ack, 'w').close()\n            return\n        time.sleep(0.01)\nworker = threading.Thread(target=answer)\nworker.start()\n",
                Some(&scope),
                None,
            )
            .expect("start thread");
            let _guard = WorkerGuard(scope.clone().unbind());
            let error = build_request(
                py,
                Some(PathBuf::from("check.toml")),
                None,
                Some(repo.to_path_buf()),
                "WORKTREE",
            )
            .expect_err("extractor output is not a subjects document");
            let code: String = error
                .value(py)
                .getattr("code")
                .expect("code")
                .extract()
                .expect("str");
            assert_eq!(
                code, "EXTRACTION_INVALID",
                "EXTRACTION_FAILED means the worker never ran: the GIL was held"
            );
        });
    }
}
