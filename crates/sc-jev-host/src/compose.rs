//! `run`/`ask` wiring over `sc-jev-check`.
//!
//! [`run_from`] and [`ask_from`] are the whole composition of a root: they
//! resolve the repo root and the log and session paths ([`ComposeInputs`],
//! [`PathPolicy`]), start the session logger
//! ([`ObservabilityAdapter`]), call `sc_jev_check::run::{prepare, run}` or
//! `ask`, and flush. They never build a payload (ADR-SAG-0005) and add no
//! backend dispatch of their own (ADR-SAG-0009): the backends come from
//! `BackendRegistry::from_env` in `sc-jev-check` unless a registry is
//! injected with [`ComposeInputs::registry`].
//!
//! # Log context
//!
//! Every log row carries the main repository root (`repo`) and the caller's
//! optional `project` (REQ-SAG-0614, REQ-SAG-0615). Compose resolves both
//! here, once per call ([`ComposeInputs::project`]; `repo` through
//! `discover_main_repo_root`) and hands them to the engine as a `LogContext`,
//! so neither consumer builds log context. `repo` changes only the log field:
//! session and log paths keep using the worktree's own root (ADR-SAG-0021).
//!
//! # Two logs
//!
//! A call writes a session log (one row per judge send) and a general log
//! (operator diagnostics) to separate files under the log root. Each is the
//! sc-agent default unless injected with [`ComposeInputs::session_log`] or
//! [`ComposeInputs::general_log`]; an injected log's flush is the caller's.
//! A dry run opens no default log. Logging never changes a result.
//!
//! # Warnings
//!
//! When the logger reports a flush failure (CR-014), the failure is not an
//! error: [`run_from`] appends it with `RunResponse::with_warning`, so every
//! consumer emits it identically in `data.warnings`.
//!
//! # Types owned by other crates
//!
//! The signatures name `CheckSource`, `RunResponse` and `AskResponse`
//! (`sc-jev-check`) and `JevError` (`sc-agent-types`). A consumer depends on
//! those crates directly, at the versions this release pins.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sc_agent_types::observability::v2::{
    CorrelatedGeneralLog, GeneralEvent, GeneralLog, NoopGeneralLog, NoopSessionLog,
    SharedGeneralLog, SharedSessionLog, new_run_id,
};
use sc_agent_types::{BackendSet, CancelToken, JevError, ProjectLabel, RepoRoot, Rev, SessionId};
use sc_decision_schema::MultiBackendMode;
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::{
    AskResponse, CheckSource, LogContext, PreparedRun, RunConfig, RunResponse, ask,
    discover_main_repo_root, discover_repo_root, prepare, run,
};
use sc_jev_check::session::FileSessionStore;

use crate::obs::{GeneralLogAdapter, ObservabilityAdapter};

/// Logging target of this module.
const TARGET: &str = "host.compose";
use crate::paths::PathPolicy;

/// Everything a root decides about one run or follow-up.
///
/// Fields are private. [`ComposeInputs::new`] takes the two required values
/// and sets a default for the rest; each optional field has one by-value
/// setter. A new field is a new setter with a default, so adding one does not
/// break `sc-ai`, `sc-jev-py` or an external caller (EXT-021).
///
/// Defaults: no overrides, [`MultiBackendMode::Independent`] (ADR-SAG-0008),
/// a never-cancelled token, and the production registry
/// (`BackendRegistry::from_env`).
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use sc_jev_host::compose::ComposeInputs;
/// use sc_jev_host::paths::PathPolicy;
///
/// let inputs = ComposeInputs::new(PathBuf::from("."), PathPolicy::cli()).dry_run(true);
/// # let _ = inputs;
/// ```
#[derive(Clone)]
pub struct ComposeInputs {
    cwd: PathBuf,
    policy: PathPolicy,
    repo_root: Option<RepoRoot>,
    log_dir: Option<PathBuf>,
    sessions_dir: Option<PathBuf>,
    backends: Option<BackendSet>,
    rev: Option<Rev>,
    dry_run: bool,
    system_prompt_override: Option<PathBuf>,
    mode: MultiBackendMode,
    cancel: CancelToken,
    registry: Option<Arc<BackendRegistry>>,
    project: Option<ProjectLabel>,
    session_log: Option<SharedSessionLog>,
    general_log: Option<SharedGeneralLog>,
}

impl std::fmt::Debug for ComposeInputs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ComposeInputs")
            .field("cwd", &self.cwd)
            .field("policy", &self.policy)
            .field("repo_root", &self.repo_root)
            .field("log_dir", &self.log_dir)
            .field("sessions_dir", &self.sessions_dir)
            .field("backends", &self.backends)
            .field("rev", &self.rev)
            .field("dry_run", &self.dry_run)
            .field("system_prompt_override", &self.system_prompt_override)
            .field("mode", &self.mode)
            .field("registry", &self.registry.is_some())
            .field("project", &self.project)
            .field("session_log", &self.session_log.is_some())
            .field("general_log", &self.general_log.is_some())
            .finish_non_exhaustive()
    }
}

/// The paths a run or follow-up resolves to.
///
/// Returned by [`ComposeInputs::resolve`] so a root can build its
/// [`CheckSource`] and its readback from the same values [`run_from`] uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaths {
    /// The repo root (explicit, else discovered from the working directory).
    pub repo_root: RepoRoot,
    /// The directory that owns the JSONL log.
    pub log_root: PathBuf,
    /// The directory that holds session files.
    pub sessions_dir: PathBuf,
}

impl ComposeInputs {
    /// Inputs that start from `cwd` and resolve paths with `policy`.
    #[must_use]
    pub fn new(cwd: PathBuf, policy: PathPolicy) -> Self {
        Self {
            cwd,
            policy,
            repo_root: None,
            log_dir: None,
            sessions_dir: None,
            backends: None,
            rev: None,
            dry_run: false,
            system_prompt_override: None,
            mode: MultiBackendMode::Independent,
            cancel: CancelToken::new(),
            registry: None,
            project: None,
            session_log: None,
            general_log: None,
        }
    }

    /// Uses `root` as the repo root instead of discovering it from the
    /// working directory (the `--repo-root` flag; see [`explicit_repo_root`]).
    #[must_use]
    pub fn repo_root(mut self, root: RepoRoot) -> Self {
        self.repo_root = Some(root);
        self
    }

    /// The explicit log root (`--log-dir`, `log_dir`); wins over the
    /// environment and the policy default.
    #[must_use]
    pub fn log_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.log_dir = Some(dir.into());
        self
    }

    /// The explicit session directory; wins over the environment and the
    /// policy default.
    #[must_use]
    pub fn sessions_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.sessions_dir = Some(dir.into());
        self
    }

    /// Selects `backends`; wins over the check document's `backends`.
    #[must_use]
    pub fn backends(mut self, backends: BackendSet) -> Self {
        self.backends = Some(backends);
        self
    }

    /// Reads check inputs at `rev` instead of `WORKTREE`.
    #[must_use]
    pub fn rev(mut self, rev: Rev) -> Self {
        self.rev = Some(rev);
        self
    }

    /// When `on`, the run sends nothing, opens no log and writes no session.
    #[must_use]
    pub fn dry_run(mut self, on: bool) -> Self {
        self.dry_run = on;
        self
    }

    /// A prompt file that wins over `check.toml` and the packaged default
    /// (ADR-SAG-0019).
    #[must_use]
    pub fn system_prompt_override(mut self, path: impl Into<PathBuf>) -> Self {
        self.system_prompt_override = Some(path.into());
        self
    }

    /// How later backends see earlier answers (ADR-SAG-0008).
    #[must_use]
    pub fn mode(mut self, mode: MultiBackendMode) -> Self {
        self.mode = mode;
        self
    }

    /// The cancellation token carried on every judge request.
    #[must_use]
    pub fn cancel(mut self, token: CancelToken) -> Self {
        self.cancel = token;
        self
    }

    /// Uses `registry` instead of `BackendRegistry::from_env`: the
    /// no-secrets path for embedders and tests.
    #[must_use]
    pub fn registry(mut self, registry: Arc<BackendRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Labels this call's log rows with `project` (`--project`, `project=`).
    ///
    /// Per call: two sends of one session may differ. The text is the
    /// caller's, stored as given and not scrubbed (ADR-SAG-0031). Unset, the
    /// row's `project` is `null`.
    #[must_use]
    pub fn project(mut self, project: ProjectLabel) -> Self {
        self.project = Some(project);
        self
    }

    /// Injects the session log (one row per judge send).
    ///
    /// Unset, the run opens sc-agent's default session log under the
    /// resolved log root. An injected log replaces that default; the caller
    /// owns its flush.
    #[must_use]
    pub fn session_log(mut self, log: SharedSessionLog) -> Self {
        self.session_log = Some(log);
        self
    }

    /// Injects the general log (operator diagnostics, separate from the
    /// session log).
    ///
    /// Unset, the run falls back to sc-agent's default general log under the
    /// resolved log root. An injected log replaces that default.
    #[must_use]
    pub fn general_log(mut self, log: SharedGeneralLog) -> Self {
        self.general_log = Some(log);
        self
    }

    /// Resolves the repo root and the log and session paths.
    ///
    /// The repo root is the explicit one ([`Self::repo_root`]), else
    /// `discover_repo_root` of the working directory.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when root discovery fails (see `discover_repo_root`).
    pub fn resolve(&self) -> Result<ResolvedPaths, JevError> {
        let repo_root = match &self.repo_root {
            Some(root) => root.clone(),
            None => discover_repo_root(&self.cwd)?,
        };
        let log_root = self
            .policy
            .resolve_log_root(self.log_dir.as_deref(), &repo_root);
        let sessions_dir =
            self.policy
                .resolve_session_dir(self.sessions_dir.as_deref(), &log_root, &repo_root);
        Ok(ResolvedPaths {
            repo_root,
            log_root,
            sessions_dir,
        })
    }

    /// The `repo` and `project` of this call's log rows. `repo` is the main
    /// repository of the explicit repo root, else of the working directory; a
    /// discovery failure falls back to the resolved root, because the log is
    /// best effort.
    fn log_context(&self, resolved: &ResolvedPaths, general: &dyn GeneralLog) -> LogContext {
        if self.dry_run {
            // A dry run logs nothing, so it does not run git for `repo`.
            return LogContext::new(resolved.repo_root.clone(), self.project.clone());
        }
        let start = match &self.repo_root {
            Some(root) => root.as_path(),
            None => self.cwd.as_path(),
        };
        let repo = match discover_main_repo_root(start) {
            Ok(repo) => repo,
            Err(error) => {
                GeneralEvent::warn(
                    TARGET,
                    "git.main_repo_failed",
                    "main repository discovery failed; using the resolved repo root",
                )
                .with_error_code(error.code().as_str())
                .emit(general);
                resolved.repo_root.clone()
            }
        };
        GeneralEvent::debug(TARGET, "git.main_repo", "main repository resolved")
            .with_field("repo", repo.as_path().display().to_string())
            .emit(general);
        LogContext::new(repo, self.project.clone())
    }

    fn run_config(
        &self,
        resolved: &ResolvedPaths,
        log: SharedSessionLog,
        general: SharedGeneralLog,
    ) -> RunConfig {
        let mut config = RunConfig::new(resolved.repo_root.clone(), log)
            .general_log(Arc::clone(&general))
            .log_context(self.log_context(resolved, general.as_ref()))
            .dry_run(self.dry_run)
            .sessions_dir(resolved.sessions_dir.clone())
            .mode(self.mode)
            .cancel(self.cancel.clone());
        if let Some(rev) = &self.rev {
            config = config.rev(rev.clone());
        }
        if let Some(backends) = &self.backends {
            config = config.backends(backends.clone());
        }
        if let Some(path) = &self.system_prompt_override {
            config = config.system_prompt_override(path.clone());
        }
        if let Some(registry) = &self.registry {
            config = config.registry(Arc::clone(registry));
        }
        config
    }
}

/// An explicit repo root (`--repo-root`): it must be a directory and is made
/// absolute. It is used as given; no git discovery runs.
///
/// # Errors
///
/// `INPUT_MISSING` when `path` is not a directory; `IO_FAILED` when it
/// cannot be made absolute.
pub fn explicit_repo_root(path: &Path) -> Result<RepoRoot, JevError> {
    if !path.is_dir() {
        return Err(JevError::input_missing_with(
            path.display().to_string(),
            "Pass an existing directory as the repository root (--repo-root, or repo_root in Python).",
        ));
    }
    let absolute = std::path::absolute(path).map_err(|error| {
        JevError::io_failed_with(
            format!("repo root {} (make absolute)", path.display()),
            error.kind(),
            Some("Pass an absolute repo root.".to_owned()),
        )
    })?;
    Ok(RepoRoot::new(absolute))
}

/// The two logs of one call and the default writers behind them.
///
/// An injected log ([`ComposeInputs::session_log`],
/// [`ComposeInputs::general_log`]) replaces the default and its flush is the
/// caller's. A default log is the file adapter under the resolved log root.
struct Logs {
    session: SharedSessionLog,
    general: SharedGeneralLog,
    session_adapter: Option<ObservabilityAdapter>,
    general_adapter: Option<GeneralLogAdapter>,
}

impl std::fmt::Debug for Logs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Logs")
            .field("session_adapter", &self.session_adapter)
            .field("general_adapter", &self.general_adapter)
            .finish_non_exhaustive()
    }
}

impl Logs {
    /// Opens the defaults the caller did not inject.
    ///
    /// `live` is false for a dry run, which opens no default log
    /// (ARCHE-019). The general log is best effort: when its default cannot
    /// start the call carries on with no general log. The session log keeps
    /// its contract: a default that cannot start is the call's error.
    fn open(
        inputs: &ComposeInputs,
        resolved: &ResolvedPaths,
        live: bool,
    ) -> Result<Self, JevError> {
        let general_adapter = match (&inputs.general_log, live) {
            (None, true) => GeneralLogAdapter::open(&inputs.policy, &resolved.log_root).ok(),
            _ => None,
        };
        let base: SharedGeneralLog = match (&inputs.general_log, &general_adapter) {
            (Some(injected), _) => Arc::clone(injected),
            (None, Some(adapter)) => adapter.shared(),
            (None, None) => Arc::new(NoopGeneralLog),
        };
        // One correlation id per run: every layer that receives this log
        // stamps `run_id` on its events.
        let general: SharedGeneralLog = Arc::new(CorrelatedGeneralLog::new(base, new_run_id()));
        GeneralEvent::debug(TARGET, "paths.resolved", "paths resolved")
            .with_field(
                "repo_root",
                resolved.repo_root.as_path().display().to_string(),
            )
            .with_field(
                "repo_root_source",
                if inputs.repo_root.is_some() {
                    "explicit"
                } else {
                    "discovered"
                },
            )
            .with_field("log_root", resolved.log_root.display().to_string())
            .with_field("sessions_dir", resolved.sessions_dir.display().to_string())
            .with_field("service", inputs.policy.service)
            .with_field(
                "session_log",
                log_origin(inputs.session_log.is_some(), live),
            )
            .with_field(
                "general_log",
                log_origin(inputs.general_log.is_some(), live),
            )
            .emit(general.as_ref());
        let session_adapter = match (&inputs.session_log, live) {
            (None, true) => match ObservabilityAdapter::open(&inputs.policy, &resolved.log_root) {
                Ok(adapter) => Some(adapter),
                Err(error) => {
                    GeneralEvent::error(
                        TARGET,
                        "session_log.open_failed",
                        "the session log could not be opened",
                    )
                    .with_error_code(error.code().as_str())
                    .emit(general.as_ref());
                    flush_general(general_adapter.as_ref());
                    return Err(error);
                }
            },
            _ => None,
        };
        let session: SharedSessionLog = match (&inputs.session_log, &session_adapter) {
            (Some(injected), _) => Arc::clone(injected),
            (None, Some(adapter)) => adapter.shared(),
            (None, None) => Arc::new(NoopSessionLog),
        };
        Ok(Self {
            session,
            general,
            session_adapter,
            general_adapter,
        })
    }

    /// Flushes the default session log; a failure is logged to the general
    /// log and returned as the response warning text (CR-014).
    fn finish_session(&self) -> Option<String> {
        let warning = self.session_adapter.as_ref().and_then(finish_log);
        if self.session_adapter.is_some() {
            match &warning {
                Some(_) => GeneralEvent::warn(
                    TARGET,
                    "session_log.flush_failed",
                    "the session log was not flushed",
                )
                .emit(self.general.as_ref()),
                None => GeneralEvent::debug(TARGET, "session_log.flushed", "session log flushed")
                    .emit(self.general.as_ref()),
            }
        }
        warning
    }

    /// Flushes the default general log. A failure has nowhere to be
    /// reported and never changes the outcome.
    fn finish_general(&self) {
        flush_general(self.general_adapter.as_ref());
    }
}

fn log_origin(injected: bool, live: bool) -> &'static str {
    match (injected, live) {
        (true, _) => "injected",
        (false, true) => "default",
        (false, false) => "none",
    }
}

fn flush_general(adapter: Option<&GeneralLogAdapter>) {
    if let Some(adapter) = adapter {
        // Best effort by contract (NFR-SAG-0002).
        let _ = adapter.flush();
    }
}

/// A prepared run plus the loggers it will use.
///
/// Returned by [`prepare_from`] for a root that needs the request (payload,
/// resolved prompt) before the run; [`run_from`] is `prepare_from(..)?.run()`.
/// Preparing sends nothing and builds the payload once.
#[derive(Debug)]
pub struct HostRun {
    prepared: PreparedRun,
    logs: Logs,
}

impl HostRun {
    /// The prepared run (payload digest, request, resolved backends).
    #[must_use]
    pub fn prepared(&self) -> &PreparedRun {
        &self.prepared
    }

    /// Runs the prepared check, then flushes the logs.
    ///
    /// A flush failure is appended to the response as a warning (CR-014),
    /// never returned as an error. The caller decides what a partial
    /// response means (`RunResponse::is_partial`, `first_failure`).
    ///
    /// # Errors
    ///
    /// Any [`JevError`] from `run`. An all-backends-failed run is not an
    /// error: it is the response (`RunResponse::is_partial`).
    /// The log is flushed on this path too, so the rows of the sends that
    /// failed are durable; a flush failure there is appended to the error
    /// detail (`session log was not flushed: <hint>`) because no response
    /// exists to carry a warning.
    pub fn run(self) -> Result<RunResponse, JevError> {
        let Self { prepared, logs } = self;
        let result = match run(prepared) {
            Ok(mut response) => {
                if let Some(warning) = logs.finish_session() {
                    response = response.with_warning(warning);
                }
                Ok(response)
            }
            Err(error) => Err(note_flush(error, logs.finish_session())),
        };
        logs.finish_general();
        result
    }
}

/// Resolves paths, starts the loggers (not for a dry run) and prepares
/// `source`.
///
/// A dry run opens no default log and creates no session directory
/// (ARCHE-019).
///
/// # Errors
///
/// Any [`JevError`] from root discovery, session logger start or `prepare`.
pub fn prepare_from(source: CheckSource, inputs: ComposeInputs) -> Result<HostRun, JevError> {
    let resolved = inputs
        .resolve()
        .inspect_err(|error| log_failure(&inputs, None, "resolve", error))?;
    let logs = Logs::open(&inputs, &resolved, !inputs.dry_run)?;
    let config = inputs.run_config(
        &resolved,
        Arc::clone(&logs.session),
        Arc::clone(&logs.general),
    );
    match prepare(source, config) {
        Ok(prepared) => Ok(HostRun { prepared, logs }),
        Err(error) => {
            logs.finish_general();
            Err(error)
        }
    }
}

/// Runs `source` with `inputs`: `prepare`, `run`, then a log flush.
///
/// A dry run opens no default log and creates no session directory
/// (ARCHE-019). A live run starts the default session and general loggers at
/// the resolved log root, unless the caller injected them, and flushes the
/// defaults before returning. A flush failure is appended to the response as
/// a warning (CR-014), never returned as an error.
///
/// The caller decides what a partial response means
/// (`RunResponse::is_partial`, `first_failure`).
///
/// # Errors
///
/// Any [`JevError`] from root discovery, logger start, `prepare` or `run`.
/// An all-backends-failed run is not an error: it is the response.
pub fn run_from(source: CheckSource, inputs: ComposeInputs) -> Result<RunResponse, JevError> {
    prepare_from(source, inputs)?.run()
}

/// Sends `follow_up` to the saved session `session_id`.
///
/// Opens the session store at the resolved session directory and the default
/// loggers at the resolved log root (unless injected), builds the backend
/// registry unless one was injected, and flushes. A flush failure is
/// appended to the response as a warning.
///
/// # Errors
///
/// `SESSION_NOT_FOUND`, `SESSION_EXPIRED` or `INPUT_INVALID` from the
/// session, plus the registry and backend errors of `sc_jev_check::run::ask`.
pub fn ask_from(
    session_id: SessionId,
    follow_up: &str,
    inputs: ComposeInputs,
) -> Result<AskResponse, JevError> {
    let resolved = inputs
        .resolve()
        .inspect_err(|error| log_failure(&inputs, None, "resolve", error))?;
    let logs = Logs::open(&inputs, &resolved, true)?;
    let sessions = match FileSessionStore::new(&resolved.sessions_dir) {
        Ok(sessions) => sessions,
        Err(error) => {
            GeneralEvent::error(TARGET, "session.store_failed", "session store failed")
                .with_error_code(error.code().as_str())
                .emit(logs.general.as_ref());
            logs.finish_general();
            return Err(error);
        }
    };
    let config = inputs.run_config(
        &resolved,
        Arc::clone(&logs.session),
        Arc::clone(&logs.general),
    );
    let result = match ask(&sessions, session_id, follow_up, config) {
        Ok(mut response) => {
            if let Some(warning) = logs.finish_session() {
                response = response.with_warning(warning);
            }
            Ok(response)
        }
        // Flush before propagating: a failed send wrote its row too.
        Err(error) => Err(note_flush(error, logs.finish_session())),
    };
    logs.finish_general();
    result
}

/// Logs a failure that happens before a run starts (root discovery, a bad
/// request file, a check source) to the general log, by typed id.
///
/// The log is the injected one, else the default opened at the resolved log
/// root (`resolved`) or, with none, at a root known without a repository
/// ([`PathPolicy::log_root_if_known`]). When no root is known the failure is
/// not logged: it is a silent path (ADR-SAG-0013 amendment 2026-11-09). A dry
/// run opens no default log. Best effort; never changes the error.
pub fn log_failure(
    inputs: &ComposeInputs,
    resolved: Option<&ResolvedPaths>,
    stage: &'static str,
    error: &JevError,
) {
    let event = GeneralEvent::error(
        TARGET,
        "operation.failed",
        "the operation failed before the run",
    )
    .with_field("stage", stage)
    .with_error_code(error.code().as_str())
    .with_field("retryable", error.retryable());
    if let Some(injected) = &inputs.general_log {
        CorrelatedGeneralLog::new(Arc::clone(injected), new_run_id())
            .log(event)
            .ok();
        return;
    }
    if inputs.dry_run {
        return;
    }
    let root = match resolved {
        Some(resolved) => Some(resolved.log_root.clone()),
        None => inputs.policy.log_root_if_known(inputs.log_dir.as_deref()),
    };
    let Some(root) = root else { return };
    if let Ok(adapter) = GeneralLogAdapter::open(&inputs.policy, &root) {
        CorrelatedGeneralLog::new(adapter.shared(), new_run_id())
            .log(event)
            .ok();
        flush_general(Some(&adapter));
    }
}

/// Records a failed flush on `error` as its own field
/// (`details.log_flush_failure`), once, so it stays observable when an error
/// (not a response) leaves the call. Every variant carries it, and the error
/// id, message and action are unchanged. `None` means the flush succeeded.
fn note_flush(error: JevError, warning: Option<String>) -> JevError {
    match warning {
        Some(warning) => error.with_log_flush_failure(warning),
        None => error,
    }
}

/// Flushes the session log. The log is best effort: a failure becomes a
/// warning string and does not change the outcome (CR-014).
fn finish_log(adapter: &ObservabilityAdapter) -> Option<String> {
    flush_warning(adapter.flush())
}

fn flush_warning(flushed: Result<(), JevError>) -> Option<String> {
    flushed
        .err()
        .map(|error| format!("session log was not flushed: {}", error.message()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_failure_is_a_warning_not_an_error() {
        let warning = flush_warning(Err(JevError::io_failed_with(
            "session log flush (disk full)",
            std::io::ErrorKind::Other,
            None,
        )))
        .expect("warning");
        assert!(warning.contains("disk full"), "{warning}");
        assert!(flush_warning(Ok(())).is_none());
    }

    #[test]
    fn flush_failure_on_the_error_path_is_noted_in_the_detail_once() {
        let error = JevError::input_invalid("bad input");
        let code = error.code();
        let noted = note_flush(
            error.clone(),
            Some("session log was not flushed: disk full".into()),
        );
        assert_eq!(noted.code(), code);
        assert_eq!(noted.message(), error.message());
        assert_eq!(noted.suggested_action(), error.suggested_action());
        assert_eq!(
            noted.log_flush_failure(),
            Some("session log was not flushed: disk full")
        );
        let details = noted.details().expect("details");
        assert_eq!(
            details.log_flush_failure.as_deref(),
            Some("session log was not flushed: disk full")
        );
        assert_eq!(note_flush(error.clone(), None), error);
        // Variants with no text field carry it too.
        let empty = note_flush(JevError::InputEmpty, Some("disk full".into()));
        assert_eq!(empty.log_flush_failure(), Some("disk full"));
    }

    #[test]
    fn defaults_are_independent_and_unset() {
        let inputs = ComposeInputs::new(PathBuf::from("."), PathPolicy::cli());
        assert_eq!(inputs.mode, MultiBackendMode::Independent);
        assert!(!inputs.dry_run);
        assert!(inputs.registry.is_none());
        assert!(inputs.backends.is_none());
        assert!(inputs.project.is_none());
    }
}
