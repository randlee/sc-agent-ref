//! The engine entry point: [`prepare`] then [`run`] (ADR-SAG-0024,
//! REQ-SAG-0620, REQ-SAG-0601).
//!
//! This is the only public way to run a check. [`prepare`] turns a
//! [`CheckSource`] and a [`RunConfig`] into a [`PreparedRun`]: it loads the
//! check, assembles the one canonical payload (ADR-SAG-0005), selects the
//! backends, and resolves the LLM system prompt when an LLM backend is
//! selected (ADR-SAG-0019). It reads only static `BackendDescriptor` facts
//! and constructs no backend. [`run`] consumes the [`PreparedRun`], so a run
//! always sees the config it was prepared with, and returns one
//! [`RunResponse`] whose serialization every consumer prints unchanged.
//! [`discover_repo_root`] is the one repo-root rule for every consumer
//! (ADR-SAG-0003 r-2 amendment). [`ask`] sends a follow-up to a saved
//! session; it and `run` share the engine's one send step (validate, judge,
//! log), so `FileSessionStore` never sends.
//!
//! # Backend selection
//!
//! [`RunConfig::backends`] when set (CLI `--backend`, Python `backends=`),
//! else the source document's `backends` (`check.toml` or the caller
//! request), else `[jev]`.
//!
//! # Live runs
//!
//! `run` uses [`RunConfig::registry`] when set, else
//! [`BackendRegistry::from_env`]. It calls `validate_for_send` on every
//! selected backend before the first `judge` (ADR-SAG-0004): each backend
//! owns its request byte cap (REQ-SAG-0314). Backends are judged in
//! `BackendSet` order and each is judged exactly once (ADR-SAG-0025).
//! [`RunConfig::mode`] decides what a later backend sees: `Independent`
//! (the default) gives no backend another's answers; `ChainContext` appends
//! the prior `Ok` answers to a later LLM backend's system prompt, never to
//! the canonical bytes, and that augmented request is re-validated before
//! its own send. A refusal there, or a failed `judge`, makes that backend
//! `Failed` without discarding the others' answers; `leak` is true when any
//! backend failed, and `run` returns `Err` (the first error) only when every
//! backend failed. Each backend reports the digest of the canonical blob it
//! recovered from its own wire body; the engine records `Matched`,
//! `Mismatched`, `Unattested`, `Failed` or `NotSent` per backend and `equal`
//! is true only when every entry is `Matched` (REQ-SAG-0606). Every
//! call that reaches `judge` emits exactly one `LogEvent` whose `fields` is one
//! session-log row (ADR-SAG-0031), whatever its result: `Ok`, `Failed`, and a
//! call cancelled before it reached the transport (REQ-SAG-0610,
//! REQ-SAG-0617, ADR-SAG-0013). The row's `repo` and `project` come from
//! [`RunConfig::log_context`]; a rejected row never fails the run. Only a backend refused by
//! `validate_for_send` (`NotSent`) emits none.
//!
//! The session is written once, at the end, only when a backend whose
//! descriptor has `supports_session_ask` returned `Ok`; that needs
//! [`RunConfig::sessions_dir`], checked before any send. If the write fails
//! after paid answers, `run` still returns `Ok` with `session_saved = false`
//! and one `session not saved: <code>` warning.
//!
//! # Worst-case wall time
//!
//! Backends are judged one after another. One LLM backend (Haiku, Luna) makes
//! two exchanges per judge call (token count, then the message); each exchange
//! is bounded by the 90 s busy-retry budget plus one 120 s in-flight request,
//! about 210 s, so one LLM backend can take about 420 s. Jev makes one
//! exchange, and its client applies the same 90 s busy-retry budget plus one
//! in-flight request (the packaged `timeout_seconds`, at most 120 s), so its
//! term is also about 210 s, not 120 s. A run is the sum over the selected
//! backends: both LLM backends and Jev can take 2 x 420 + 210 = 1050 s (about
//! 17.5 minutes) in the worst case. The
//! [`RunConfig::cancel`] token ends a busy-retry wait within 100 ms but not an
//! in-flight request; there is no run-level deadline.
//!
//! # Dry runs
//!
//! [`RunConfig::dry_run`] sends nothing and constructs no backend. The
//! response carries the payload, `payload_bytes`, and per selected backend
//! its `request_limit_bytes` and `over_limit`; an over-limit payload is
//! reported, not refused. Every backend is `NotSent`, so `equal` is false;
//! the record carries [`RunConfig::mode`].
//!
//! # Examples
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! use sc_agent_types::observability::v2::NoopSessionLog;
//! use sc_jev_check::run::{CheckSource, RunConfig, discover_repo_root, prepare, run};
//!
//! let repo = discover_repo_root(std::path::Path::new("."))?;
//! let config = RunConfig::new(repo.clone(), Arc::new(NoopSessionLog))
//!     .backends("jev,anthropic_haiku".parse()?)
//!     .sessions_dir(".sc-ai/sessions");
//! let source = CheckSource::check_file(&repo, std::path::Path::new("check.toml"))?;
//! let prepared = prepare(source, config)?;
//! println!("payload digest {}", prepared.payload_sha256());
//! let response = run(prepared)?;
//! assert!(!response.is_partial());
//! # Ok::<(), sc_agent_types::JevError>(())
//! ```

#![warn(missing_docs)]

#[cfg(any(test, feature = "test-util"))]
mod builder;
mod log;
pub(crate) mod prompt;
mod response;

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use sc_agent_types::observability::v2::SharedSessionLog;
use sc_agent_types::schema::subjects::SubjectsProducer;
use sc_agent_types::{
    BackendId, BackendSet, CancelToken, JevError, ModelAliases, ModelId, Operation, PayloadSha256,
    QuestionsSha256, RepoRelativePath, RepoRoot, RequestSha256, Rev, SessionId,
};
use sc_decision_schema::{
    Attestation, BackendAttestation, EqualInputRecord, JudgeBackend, JudgeOutcome, JudgeRequest,
    LlmWireContext, MultiBackendMode, PayloadBundle,
};

use crate::config::{CheckSpec, load_check_at};
use crate::equal_input::{RunAttestation, producer_from_extractions, record};
use crate::payload::build_payload;
use crate::payload::caller::parse_caller_request;
use crate::registry::{BackendRegistry, model_for};
use crate::session::{FileSessionStore, StoredSession, check_follow_up, contextual_prompt};
use log::{SendLog, send};

#[cfg(any(test, feature = "test-util"))]
#[doc(inline)]
pub use builder::RunResponseBuilder;
#[doc(inline)]
pub use log::LogContext;
use response::ResponseParts;
#[doc(inline)]
pub use response::{
    AskResponse, BackendFailure, BackendLimit, BackendResult, BackendStatus, ENVELOPE_VERSION,
    Envelope, OP_ASK, OP_DRY_RUN, OP_RUN, RunResponse,
};

/// Where a run's check comes from.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum CheckSource {
    /// A repo-relative `check.toml`, read at [`RunConfig::rev`]. Build it
    /// from a user path with [`CheckSource::check_file`].
    CheckFile(RepoRelativePath),
    /// An already parsed check.
    Spec(CheckSpec),
    /// A caller-built questions document (`sc-ai jev --request-json`).
    CallerRequest(CallerRequest),
}

impl CheckSource {
    /// A `check.toml` source from a user-supplied `path` (CLI `--check`,
    /// Python `check_path`): the one normalisation of a check path.
    ///
    /// A relative `path` is taken as relative to `repo`. An absolute one must
    /// sit under `repo` as given or canonicalized; that prefix is stripped.
    /// `\` becomes `/` and `.` segments are dropped. The filesystem is not
    /// read, so a check file that exists only at a revision still loads.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when `path` is outside `repo`, contains `..`, is not
    /// UTF-8, or names the repo root itself.
    pub fn check_file(repo: &RepoRoot, path: &Path) -> Result<Self, JevError> {
        normalize_check_path(repo, path).map(Self::CheckFile)
    }
}

fn normalize_check_path(repo: &RepoRoot, path: &Path) -> Result<RepoRelativePath, JevError> {
    let relative = if path.is_absolute() {
        strip_repo_prefix(repo, path)?
    } else {
        path.to_path_buf()
    };
    let text = relative.to_str().ok_or_else(|| {
        JevError::input_invalid(format!("check path {} is not UTF-8", path.display()))
    })?;
    let mut parts = Vec::new();
    for part in text.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                return Err(JevError::input_invalid(format!(
                    "check path {} must not contain '..'; pass a path inside the repo root",
                    path.display()
                )));
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return Err(JevError::input_invalid(
            "check path must be a file inside the repo, not the repo root itself",
        ));
    }
    RepoRelativePath::try_from(parts.join("/"))
}

fn strip_repo_prefix(repo: &RepoRoot, path: &Path) -> Result<PathBuf, JevError> {
    if let Ok(rest) = path.strip_prefix(repo.as_path()) {
        return Ok(rest.to_path_buf());
    }
    if let Ok(root) = std::fs::canonicalize(repo.as_path())
        && let Ok(rest) = path.strip_prefix(&root)
    {
        return Ok(rest.to_path_buf());
    }
    Err(JevError::input_invalid(format!(
        "check path {} is outside the repo root {}; pass a path inside it",
        path.display(),
        repo.as_path().display()
    )))
}

/// A validated caller-built questions document.
///
/// The document is `{"questions": [...], "state"?: [...], "backends"?: [...]}`;
/// it has no system-prompt field. Its payload is built here (ADR-SAG-0005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerRequest {
    bundle: PayloadBundle,
    backends: Option<BackendSet>,
}

impl CallerRequest {
    /// Parses and validates `bytes` and builds its canonical payload.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when the document is empty, not the questions shape,
    /// names an unknown or repeated backend, or includes `system_prompt`.
    pub fn from_json(bytes: &[u8]) -> Result<Self, JevError> {
        let parsed = parse_caller_request(bytes)?;
        let backends = if parsed.backends.is_empty() {
            None
        } else {
            Some(BackendSet::try_from(parsed.backends).map_err(|_| {
                JevError::input_invalid("request JSON backends must not repeat a backend")
            })?)
        };
        Ok(Self {
            bundle: parsed.bundle,
            backends,
        })
    }

    /// The canonical payload built from the document.
    #[must_use]
    pub fn bundle(&self) -> &PayloadBundle {
        &self.bundle
    }

    /// The document's `backends`, when it names any.
    #[must_use]
    pub fn backends(&self) -> Option<&BackendSet> {
        self.backends.as_ref()
    }
}

/// How to run a check. Built with [`RunConfig::new`] and the setters.
///
/// Fields are private: [`prepare`] takes the config by value and
/// [`PreparedRun`] owns it, so [`run`] cannot see a different one.
#[derive(Clone)]
pub struct RunConfig {
    repo_root: RepoRoot,
    session_log: SharedSessionLog,
    backends: Option<BackendSet>,
    rev: Rev,
    dry_run: bool,
    system_prompt_override: Option<PathBuf>,
    sessions_dir: Option<PathBuf>,
    registry: Option<Arc<BackendRegistry>>,
    cancel: CancelToken,
    mode: MultiBackendMode,
    models: Option<ModelAliases>,
    log_context: Option<LogContext>,
}

impl RunConfig {
    /// A live run at `WORKTREE` under `repo_root`, logging judge sends to
    /// `session_log`, with the backend selection left to the source.
    #[must_use]
    pub fn new(repo_root: RepoRoot, session_log: SharedSessionLog) -> Self {
        Self {
            repo_root,
            session_log,
            backends: None,
            rev: Rev::worktree(),
            dry_run: false,
            system_prompt_override: None,
            sessions_dir: None,
            registry: None,
            cancel: CancelToken::new(),
            mode: MultiBackendMode::Independent,
            models: None,
            log_context: None,
        }
    }

    /// Sets the `repo` and `project` columns of every log row this config
    /// writes (REQ-SAG-0614, REQ-SAG-0615).
    ///
    /// Unset, `repo` is [`discover_main_repo_root`] of the repo root given to
    /// [`RunConfig::new`] and `project` is `null`. Composition roots set it
    /// through `sc-jev-host`, so neither consumer builds log context.
    #[must_use]
    pub fn log_context(mut self, context: LogContext) -> Self {
        self.log_context = Some(context);
        self
    }

    /// How later backends see earlier answers (ADR-SAG-0008, ADR-SAG-0025).
    ///
    /// The default is [`MultiBackendMode::Independent`]: no backend receives
    /// another's answers. [`MultiBackendMode::ChainContext`] appends the
    /// prior `Ok` answers to later LLM backends' system prompts. The mode
    /// is recorded on `EqualInputRecord.mode`.
    #[must_use]
    pub fn mode(mut self, mode: MultiBackendMode) -> Self {
        self.mode = mode;
        self
    }

    /// Selects `backends`; wins over the source document's `backends`.
    #[must_use]
    pub fn backends(mut self, backends: BackendSet) -> Self {
        self.backends = Some(backends);
        self
    }

    /// Reads check inputs and the `check.toml` prompt file at `rev`.
    #[must_use]
    pub fn rev(mut self, rev: Rev) -> Self {
        self.rev = rev;
        self
    }

    /// When `on`, [`run`] sends nothing and reports limits instead.
    #[must_use]
    pub fn dry_run(mut self, on: bool) -> Self {
        self.dry_run = on;
        self
    }

    /// A working-tree prompt file that wins over `check.toml` and the
    /// packaged asset (ADR-SAG-0019). A relative path is under the repo root.
    #[must_use]
    pub fn system_prompt_override(mut self, path: impl Into<PathBuf>) -> Self {
        self.system_prompt_override = Some(path.into());
        self
    }

    /// Directory for session files. Required for a live run that selects an
    /// `ask`-capable backend.
    #[must_use]
    pub fn sessions_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.sessions_dir = Some(dir.into());
        self
    }

    /// Uses `registry` instead of [`BackendRegistry::from_env`]: the
    /// no-secrets path for embedders and tests (EXT-009, ADR-SAG-0022).
    #[must_use]
    pub fn registry(mut self, registry: Arc<BackendRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Decides each production backend's model from `models` (REQ-SAG-0148).
    ///
    /// Wins over the check's `[models]` table; without either, the catalog
    /// defaults apply. A registry set with [`Self::registry`] is already
    /// built, so it ignores this.
    #[must_use]
    pub fn models(mut self, models: ModelAliases) -> Self {
        self.models = Some(models);
        self
    }

    /// Cancellation token carried on every judge request.
    #[must_use]
    pub fn cancel(mut self, token: CancelToken) -> Self {
        self.cancel = token;
        self
    }
}

impl std::fmt::Debug for RunConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RunConfig")
            .field("repo_root", &self.repo_root)
            .field("backends", &self.backends)
            .field("rev", &self.rev)
            .field("dry_run", &self.dry_run)
            .field("system_prompt_override", &self.system_prompt_override)
            .field("sessions_dir", &self.sessions_dir)
            .field("registry", &self.registry)
            .field("mode", &self.mode)
            .field("models", &self.models)
            .field("log_context", &self.log_context)
            .finish_non_exhaustive()
    }
}

/// A check ready to run: its config, payload, backends, and resolved prompt.
///
/// Produced by [`prepare`] and consumed by [`run`].
pub struct PreparedRun {
    config: RunConfig,
    request: JudgeRequest,
    backends: BackendSet,
    producer: Option<SubjectsProducer>,
    models: ModelAliases,
}

impl PreparedRun {
    /// SHA-256 of the canonical payload.
    #[must_use]
    pub fn payload_sha256(&self) -> &PayloadSha256 {
        self.request.payload().sha256()
    }

    /// SHA-256 of the question set alone (`questions_sha256`).
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when a question breaks its criteria count.
    pub fn questions_sha256(&self) -> Result<QuestionsSha256, JevError> {
        self.request.payload().questions_sha256()
    }

    /// Per selected backend, the SHA-256 of the first body `send` would post
    /// for it, computed now: no key, no send (`request_sha256`).
    ///
    /// It equals the session-log `request_sha256` of the same live run. The
    /// model is the one the backend runs with. A backend is left out when its
    /// bytes cannot be known before a send or nothing would be sent: an LLM
    /// backend after another in `chain_context` mode (its system prompt
    /// carries the earlier answers), or one that refuses the questions (such
    /// as `noul`).
    #[must_use]
    pub fn request_sha256(&self) -> Vec<(BackendId, RequestSha256)> {
        presend_request_sha256(&self.request, &self.backends, self.config.mode, |id| {
            model_for(id, &self.models)
        })
    }

    /// Size of the canonical payload in bytes.
    #[must_use]
    pub fn payload_bytes(&self) -> u64 {
        payload_len(self.request.payload())
    }

    /// Subjects producer from the first `subjects` or `command` input.
    #[must_use]
    pub fn producer(&self) -> Option<&SubjectsProducer> {
        self.producer.as_ref()
    }

    /// The base judge request: the shared payload and, when an LLM backend
    /// is selected, the resolved system prompt. A backend that takes no LLM
    /// context (Jev) is sent this payload with `llm: None`.
    #[must_use]
    pub fn request(&self) -> &JudgeRequest {
        &self.request
    }

    /// The selected backends, in judge order.
    #[must_use]
    pub fn backends(&self) -> &BackendSet {
        &self.backends
    }

    /// The model aliases production backends are built with:
    /// [`RunConfig::models`], else the check's `[models]` table, else the
    /// catalog.
    #[must_use]
    pub fn models(&self) -> &ModelAliases {
        &self.models
    }
}

impl std::fmt::Debug for PreparedRun {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedRun")
            .field("config", &self.config)
            .field("payload_sha256", self.payload_sha256())
            .field("backends", &self.backends)
            .finish_non_exhaustive()
    }
}

/// Loads the check, builds its payload, selects backends, and resolves the
/// LLM system prompt. Sends nothing and constructs no backend.
///
/// # Errors
///
/// Returns [`JevError`] when the check file or a document is missing or
/// invalid, an input fails, or an LLM backend is selected and its prompt
/// file is missing (`PROMPT_FILE_MISSING`) or empty.
pub fn prepare(source: CheckSource, config: RunConfig) -> Result<PreparedRun, JevError> {
    let (spec, bundle, producer, source_backends) = match source {
        CheckSource::CheckFile(path) => {
            let spec = load_check_at(&config.repo_root, &config.rev, &path)?;
            assemble(spec, &config)?
        }
        CheckSource::Spec(spec) => assemble(spec, &config)?,
        CheckSource::CallerRequest(request) => (None, request.bundle, None, request.backends),
    };
    let backends = match (&config.backends, source_backends) {
        (Some(backends), _) => backends.clone(),
        (None, Some(backends)) => backends,
        (None, None) => BackendSet::default(),
    };
    let needs_llm = backends
        .iter()
        .any(|id| id.descriptor().requires_llm_context());
    let override_path = config.system_prompt_override.as_deref();
    let system_prompt = match &spec {
        Some(spec) => prompt::load_system_prompt(
            spec,
            &config.repo_root,
            &config.rev,
            override_path,
            needs_llm,
        )?,
        None => prompt::load_request_system_prompt(&config.repo_root, override_path, needs_llm)?,
    };
    let request = JudgeRequest::new(Arc::new(bundle), system_prompt.map(LlmWireContext::new))
        .with_cancel(config.cancel.clone());
    let models = config
        .models
        .clone()
        .or_else(|| spec.as_ref().map(|spec| spec.models().clone()))
        .unwrap_or_else(ModelAliases::builtin);
    Ok(PreparedRun {
        config,
        request,
        backends,
        producer,
        models,
    })
}

type Assembled = (
    Option<CheckSpec>,
    PayloadBundle,
    Option<SubjectsProducer>,
    Option<BackendSet>,
);

fn assemble(spec: CheckSpec, config: &RunConfig) -> Result<Assembled, JevError> {
    let built = build_payload(&spec, &config.repo_root, &config.rev)?;
    let producer = producer_from_extractions(&built.extractions)?;
    let backends = spec.backends().cloned();
    Ok((Some(spec), built.bundle, producer, backends))
}

fn payload_len(bundle: &PayloadBundle) -> u64 {
    u64::try_from(bundle.bytes().len()).unwrap_or(u64::MAX)
}

/// Runs `prepared` and returns its one [`RunResponse`].
///
/// See the module documentation for live and dry-run behaviour.
///
/// # Errors
///
/// A dry run fails only if the payload cannot be re-read as JSON (a bug).
/// A live run returns `CONFIG_INVALID` when an `ask`-capable backend is
/// selected and no sessions directory is set, or the session directory
/// cannot be created; the [`BackendRegistry::from_env`] errors (no
/// registry injected); `CONFIG_INVALID` when a selected backend is not in
/// the registry; the first base-request `validate_for_send` refusal, before
/// any send; and, when every backend failed, the first backend's error. A
/// session write error is a warning, not an error. Every error before the
/// first send leaves no log line and no session file.
pub fn run(prepared: PreparedRun) -> Result<RunResponse, JevError> {
    if prepared.config.dry_run {
        dry_run_response(&prepared)
    } else {
        live_run(prepared)
    }
}

fn dry_run_response(prepared: &PreparedRun) -> Result<RunResponse, JevError> {
    let bundle = prepared.request.payload();
    let payload_bytes = payload_len(bundle);
    let payload = serde_json::from_slice(bundle.bytes())
        .map_err(|error| JevError::encode_failed("canonical payload", &error))?;
    let limits = prepared
        .backends
        .iter()
        .map(|id| BackendLimit::for_payload(id, payload_bytes))
        .collect();
    let equal_input = EqualInputRecord::new(
        bundle.sha256().clone(),
        prepared.config.mode,
        prepared
            .backends
            .iter()
            .map(|id| BackendAttestation::new(id, Attestation::NotSent))
            .collect(),
        false,
        prepared.producer.clone(),
    )?;
    Ok(RunResponse::from_parts(ResponseParts {
        questions_sha256: bundle.questions_sha256()?,
        request_sha256: prepared.request_sha256(),
        session_id: None,
        payload_bytes,
        equal_input,
        results: Vec::new(),
        payload: Some(payload),
        limits,
    }))
}

fn live_run(prepared: PreparedRun) -> Result<RunResponse, JevError> {
    let PreparedRun {
        config,
        request,
        backends,
        producer,
        models,
    } = prepared;
    let store = open_session_store(&config, &backends)?;
    let registry = match &config.registry {
        Some(registry) => Arc::clone(registry),
        None => Arc::new(BackendRegistry::from_env_with_models(
            &config.repo_root,
            &backends,
            &models,
        )?),
    };
    let judges = backends
        .iter()
        .map(|id| registry.get(id).map(|judge| (id, judge)))
        .collect::<Result<Vec<(BackendId, Arc<dyn JudgeBackend>)>, JevError>>()?;
    // ADR-SAG-0004: every backend validates its base request (its own byte
    // cap, REQ-SAG-0314) before the first send. Any refusal is `Err`.
    for (id, judge) in &judges {
        judge.validate_for_send(&request_for(*id, &request, &[])?)?;
    }

    // Computed before the first send, from the models the backends run with.
    let request_digests = presend_request_sha256(&request, &backends, config.mode, |id| {
        judges
            .iter()
            .find(|(judge_id, _)| *judge_id == id)
            .map_or_else(
                || id.descriptor().default_model(),
                |(_, judge)| judge.model_id(),
            )
    });

    let session_id = SessionId::new();
    // One creation time per session: the rows' `session_created_at` and the
    // saved document's `created_at` are the same instant.
    let session_created_at = store
        .as_ref()
        .map_or_else(SystemTime::now, FileSessionStore::now);
    let log = SendLog::new(&config, session_id, session_created_at, Operation::Judge);
    let mut results: Vec<BackendResult> = Vec::with_capacity(judges.len());
    let mut attestations: Vec<BackendAttestation> = Vec::with_capacity(judges.len());
    // Prior `Ok` outcomes, in `BackendSet` order; chained only in
    // `ChainContext` mode (ADR-SAG-0008 r-2b).
    let mut answered: Vec<JudgeOutcome> = Vec::new();
    for (id, judge) in &judges {
        let prior: &[JudgeOutcome] = match config.mode {
            MultiBackendMode::ChainContext => &answered,
            _ => &[],
        };
        let step = judge_one(judge.as_ref(), *id, &request, prior, &log);
        let (result, attestation) = match step {
            Step::Answered(outcome) => {
                let attestation = attest(request.payload().sha256(), &outcome);
                answered.push(outcome.clone());
                (BackendResult::ok(outcome), attestation)
            }
            Step::Failed(error) => (BackendResult::failed(*id, error), Attestation::Failed),
            Step::NotSent(error) => (BackendResult::failed(*id, error), Attestation::NotSent),
        };
        results.push(result);
        attestations.push(BackendAttestation::new(*id, attestation));
    }

    // ADR-SAG-0025: a run in which every backend failed is an error.
    if answered.is_empty() {
        let first = results.iter().find_map(|result| match result.status() {
            BackendStatus::Failed(failure) => Some(failure.error().clone()),
            BackendStatus::Ok(_) => None,
        });
        return Err(first.unwrap_or_else(|| JevError::input_invalid("the run selected no backend")));
    }
    let leak = results
        .iter()
        .any(|result| matches!(result.status(), BackendStatus::Failed(_)));

    // A failed save keeps the answers (ADR-SAG-0025): `session_saved = false`
    // and one warning that carries the error code only, never a path.
    let (saved, save_warning) = match &store {
        Some(store) => {
            match save_session(store, session_id, session_created_at, &request, &answered) {
                Ok(saved) => (saved, None),
                Err(error) => (false, Some(format!("session not saved: {}", error.code()))),
            }
        }
        None => (false, None),
    };
    let equal_input = record(
        request.payload(),
        RunAttestation {
            mode: config.mode,
            backends: attestations,
            leak,
        },
        producer,
    )?;
    let response = RunResponse::from_parts(ResponseParts {
        questions_sha256: request.payload().questions_sha256()?,
        request_sha256: request_digests,
        session_id: saved.then_some(session_id),
        payload_bytes: payload_len(request.payload()),
        equal_input,
        results,
        payload: None,
        limits: Vec::new(),
    });
    Ok(match save_warning {
        Some(warning) => response.with_warning(warning),
        None => response,
    })
}

/// What one backend's turn produced.
enum Step {
    /// The backend answered.
    Answered(JudgeOutcome),
    /// The backend was sent and failed.
    Failed(JevError),
    /// Nothing was sent: the request could not be built or was refused.
    NotSent(JevError),
}

/// One backend's turn: build its request, re-validate it when it gained
/// session context, then send once (ADR-SAG-0004, ADR-SAG-0025).
fn judge_one(
    judge: &dyn JudgeBackend,
    id: BackendId,
    base: &JudgeRequest,
    prior: &[JudgeOutcome],
    log: &SendLog<'_>,
) -> Step {
    let judge_request = match request_for(id, base, prior) {
        Ok(request) => request,
        Err(error) => return Step::NotSent(error),
    };
    // Only an LLM request that gained session context differs from the
    // validated base request; re-validate it before its own send.
    if id.descriptor().requires_llm_context()
        && !prior.is_empty()
        && let Err(error) = judge.validate_for_send(&judge_request)
    {
        return Step::NotSent(error);
    }
    match send(judge, &judge_request, log) {
        Ok(outcome) => Step::Answered(outcome),
        Err(error) => Step::Failed(error),
    }
}

/// The attestation of an `Ok` outcome: its reported digest against the
/// run's `payload_sha256` (REQ-SAG-0606). The engine trusts the reported
/// digest; a backend that reports none is `Unattested`.
fn attest(expected: &PayloadSha256, outcome: &JudgeOutcome) -> Attestation {
    match outcome.input_sha256() {
        Some(reported) if reported == expected => Attestation::Matched,
        Some(reported) => Attestation::Mismatched {
            reported: reported.clone(),
        },
        None => Attestation::Unattested,
    }
}

/// The session store a live run may write, opened before any send.
fn open_session_store(
    config: &RunConfig,
    backends: &BackendSet,
) -> Result<Option<FileSessionStore>, JevError> {
    if !backends
        .iter()
        .any(|id| id.descriptor().supports_session_ask())
    {
        return Ok(None);
    }
    let Some(dir) = &config.sessions_dir else {
        return Err(JevError::operational_config(
            "A live run with an ask-capable backend saves a session. Set RunConfig::sessions_dir (sc-ai --sessions-dir), or use dry_run.",
        ));
    };
    FileSessionStore::new(dir).map(Some)
}

/// Writes the session once, when an `ask`-capable backend returned `Ok`.
fn save_session(
    store: &FileSessionStore,
    session_id: SessionId,
    created_at: SystemTime,
    request: &JudgeRequest,
    outcomes: &[JudgeOutcome],
) -> Result<bool, JevError> {
    let Some(backend) = outcomes
        .iter()
        .map(JudgeOutcome::backend)
        .find(|id| id.descriptor().supports_session_ask())
    else {
        return Ok(false);
    };
    store.write_session(&StoredSession {
        session_id,
        backend,
        created_at,
        payload: request.payload().clone(),
        system_prompt: request.llm().map(|llm| llm.system_prompt.clone()),
        outcomes: outcomes.to_vec(),
        follow_ups: Vec::new(),
    })?;
    Ok(true)
}

/// The pre-send `request_sha256` of each backend in `backends` that has one
/// (see [`PreparedRun::request_sha256`]).
fn presend_request_sha256(
    base: &JudgeRequest,
    backends: &BackendSet,
    mode: MultiBackendMode,
    model: impl Fn(BackendId) -> ModelId,
) -> Vec<(BackendId, RequestSha256)> {
    let mut digests = Vec::new();
    for (index, id) in backends.iter().enumerate() {
        let llm = id.descriptor().requires_llm_context();
        if llm && index > 0 && mode == MultiBackendMode::ChainContext {
            continue;
        }
        let Ok(request) = request_for(id, base, &[]) else {
            continue;
        };
        let model = model(id);
        let digest = match id {
            BackendId::Jev => sc_jev::request_sha256(&model, request.payload()),
            BackendId::AnthropicHaiku | BackendId::OpenAiLuna => {
                sc_jev_backends::request_sha256(id, &model, &request)
            }
            _ => continue,
        };
        if let Ok(digest) = digest {
            digests.push((id, digest));
        }
    }
    digests
}

/// The request `id` receives: the shared payload with no LLM context for a
/// backend that takes none (Jev), else the base system prompt with prior
/// answers appended as session context.
fn request_for(
    id: BackendId,
    base: &JudgeRequest,
    prior: &[JudgeOutcome],
) -> Result<JudgeRequest, JevError> {
    if !id.descriptor().requires_llm_context() {
        return Ok(JudgeRequest::new(Arc::clone(base.payload_arc()), None)
            .with_cancel(base.cancel().clone()));
    }
    if prior.is_empty() {
        return Ok(base.clone());
    }
    let prompt = contextual_prompt(base.llm().map(|llm| llm.system_prompt.as_str()), prior, &[])?;
    Ok(JudgeRequest::new(
        Arc::clone(base.payload_arc()),
        Some(LlmWireContext::new(prompt)),
    )
    .with_cancel(base.cancel().clone()))
}

/// Sends `follow_up` to the saved session `session_id` in `store`.
///
/// The session's Haiku or Luna backend comes from [`RunConfig::registry`]
/// when set, else [`BackendRegistry::from_env`] at the repo root given to [`RunConfig::new`]
/// for that one backend. The request is the stored payload with the stored
/// system prompt, the prior answers as session context, the stored
/// follow-ups and then `follow_up`; it is validated, then sent through the
/// same send step as `run`, with one [`Operation::Ask`] log row on
/// [`RunConfig`]'s session log. The session file is read and never
/// rewritten. Only `repo_root`, `session_log`, `log_context`, `registry`,
/// `cancel` and `models` of `config` are used.
///
/// # Errors
///
/// `SESSION_NOT_FOUND` / `SESSION_EXPIRED` / `INPUT_INVALID` from loading the
/// session; `INPUT_INVALID` for an empty follow-up or a session with no
/// `ask` (Jev); the registry errors; and the backend's `validate_for_send`
/// or `judge` error.
pub fn ask(
    store: &FileSessionStore,
    session_id: SessionId,
    follow_up: &str,
    config: RunConfig,
) -> Result<AskResponse, JevError> {
    let stored = store.read_session(&session_id)?;
    check_follow_up(&stored, follow_up)?;
    let registry = match &config.registry {
        Some(registry) => Arc::clone(registry),
        None => Arc::new(BackendRegistry::from_env_with_models(
            &config.repo_root,
            &BackendSet::from(stored.backend),
            &config.models.clone().unwrap_or_else(ModelAliases::builtin),
        )?),
    };
    let judge = registry.get(stored.backend)?;
    let mut follow_ups: Vec<&str> = stored.follow_ups.iter().map(String::as_str).collect();
    follow_ups.push(follow_up);
    let prompt = contextual_prompt(
        stored.system_prompt.as_deref(),
        &stored.outcomes,
        &follow_ups,
    )?;
    let request = JudgeRequest::new(
        Arc::new(stored.payload.clone()),
        Some(LlmWireContext::new(prompt)),
    )
    .with_cancel(config.cancel.clone());
    judge.validate_for_send(&request)?;
    let log = SendLog::new(&config, session_id, stored.created_at, Operation::Ask);
    let outcome = send(judge.as_ref(), &request, &log)?;
    Ok(AskResponse::new(
        outcome,
        session_id,
        stored.payload.sha256().clone(),
    ))
}

/// The repo root for `start`: its git toplevel, or `start` made absolute.
///
/// Uses `sc_agent_types::git::discover_toplevel`. `start` itself (made
/// absolute) is the root only when it is outside a git work tree (git ran
/// and found no repository, and no ancestor of `start` holds a `.git`
/// entry) or when git is not installed. Both consumers call this; the
/// `.env` used for keys is the one at this root (ADR-SAG-0003 r-2
/// amendment).
///
/// # Errors
///
/// `INPUT_INVALID` when `start` cannot be made absolute; when git is
/// installed but cannot be run for another reason; and when git finds no
/// work tree although an ancestor of `start` holds `.git` (for example a
/// repository git refuses as unsafe), so a subdirectory is never silently
/// taken for the root.
pub fn discover_repo_root(start: &Path) -> Result<RepoRoot, JevError> {
    let absolute =
        std::path::absolute(start).map_err(|error| JevError::io_failed("repo root", &error))?;
    let discovered = sc_agent_types::git::discover_toplevel(&absolute);
    let enclosing = match &discovered {
        Ok(None) => enclosing_git_entry(&absolute),
        _ => None,
    };
    classify_discovery(absolute, discovered, enclosing)
}

/// The main repository root for `start`, for the log row's `repo` column.
///
/// A linked worktree resolves to the repository it was added from, so every
/// worktree of one repository logs the same `repo`. Uses
/// `sc_agent_types::git::discover_common_dir`: when the common dir is a
/// `.git` directory, its parent is the root. A separate git dir or a bare
/// common dir has no such parent, so the current toplevel
/// ([`discover_repo_root`]) is used; outside git the result is `start` made
/// absolute. Session and log paths do not use this root (ADR-SAG-0021).
///
/// # Errors
///
/// The errors of [`discover_repo_root`], when the fallback runs.
///
/// # Examples
///
/// ```no_run
/// use sc_jev_check::run::discover_main_repo_root;
///
/// let main = discover_main_repo_root(std::path::Path::new("."))?;
/// println!("{}", main.as_path().display());
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
pub fn discover_main_repo_root(start: &Path) -> Result<RepoRoot, JevError> {
    let absolute =
        std::path::absolute(start).map_err(|error| JevError::io_failed("repo root", &error))?;
    if let Ok(Some(common)) = sc_agent_types::git::discover_common_dir(&absolute)
        && common.file_name().is_some_and(|name| name == ".git")
        && let Some(parent) = common.parent()
    {
        return Ok(RepoRoot::new(parent));
    }
    discover_repo_root(&absolute)
}

/// The decision of [`discover_repo_root`], separated so every branch is
/// tested without depending on the test machine's directories.
fn classify_discovery(
    absolute: PathBuf,
    discovered: Result<Option<RepoRoot>, JevError>,
    enclosing: Option<PathBuf>,
) -> Result<RepoRoot, JevError> {
    match discovered {
        Ok(Some(root)) => Ok(root),
        Ok(None) => match enclosing {
            None => Ok(RepoRoot::new(absolute)),
            Some(git_entry) => Err(JevError::input_invalid(format!(
                "git found no work tree at {}, but {} exists; fix the repository (for example git's safe.directory) or pass an explicit repo root",
                absolute.display(),
                git_entry.display()
            ))),
        },
        Err(JevError::IoFailed {
            kind: std::io::ErrorKind::NotFound,
            ..
        }) => Ok(RepoRoot::new(absolute)),
        Err(error) => Err(error),
    }
}

/// The nearest `.git` entry at or above `start`, honouring
/// `GIT_CEILING_DIRECTORIES` as git does (the walk stops below a listed
/// directory).
fn enclosing_git_entry(start: &Path) -> Option<PathBuf> {
    let ceilings: Vec<PathBuf> = std::env::var_os("GIT_CEILING_DIRECTORIES")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    let mut dir = Some(start);
    while let Some(current) = dir {
        if ceilings.iter().any(|ceiling| ceiling == current) {
            return None;
        }
        let candidate = current.join(".git");
        if candidate.symlink_metadata().is_ok() {
            return Some(candidate);
        }
        dir = current
            .parent()
            .filter(|parent| parent.components().next() != Some(Component::CurDir));
    }
    None
}

#[cfg(test)]
mod tests;
