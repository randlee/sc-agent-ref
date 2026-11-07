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

use sc_agent_types::observability::v2::{NoopSessionLog, SharedSessionLog};
use sc_agent_types::{
    BackendSet, CancelToken, JevError, MultiBackendMode, RepoRoot, Rev, SessionId,
};
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::{
    AskResponse, CheckSource, RunConfig, RunResponse, ask, discover_repo_root, prepare, run,
};
use sc_jev_check::session::FileSessionStore;

use crate::obs::ObservabilityAdapter;
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

    fn run_config(&self, resolved: &ResolvedPaths, log: SharedSessionLog) -> RunConfig {
        let mut config = RunConfig::new(resolved.repo_root.clone(), log)
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
/// `INPUT_MISSING` when `path` is not a directory; `CONFIG_INVALID` when it
/// cannot be made absolute.
pub fn explicit_repo_root(path: &Path) -> Result<RepoRoot, JevError> {
    if !path.is_dir() {
        return Err(JevError::input_missing(path.display().to_string()));
    }
    let absolute = std::path::absolute(path).map_err(|error| {
        JevError::operational_config(format!(
            "repo root {} could not be made absolute (cause: {}). Pass an absolute repo root.",
            path.display(),
            error.kind()
        ))
    })?;
    Ok(RepoRoot::new(absolute))
}

/// Runs `source` with `inputs`: `prepare`, `run`, then a log flush.
///
/// A dry run opens no log and creates no session directory (ARCHE-019). A
/// live run starts the session logger at the resolved log root and flushes it
/// before returning. A flush failure is appended to the response as a warning
/// (CR-014), never returned as an error.
///
/// The caller decides what a partial response means
/// (`RunResponse::is_partial`, `first_failure`).
///
/// # Errors
///
/// Any [`JevError`] from root discovery, logger start, `prepare` or `run`,
/// including the all-backends-failed case.
pub fn run_from(source: CheckSource, inputs: ComposeInputs) -> Result<RunResponse, JevError> {
    let resolved = inputs.resolve()?;
    let adapter = if inputs.dry_run {
        None
    } else {
        Some(ObservabilityAdapter::open(
            &inputs.policy,
            &resolved.log_root,
        )?)
    };
    let log: SharedSessionLog = match &adapter {
        Some(adapter) => adapter.shared(),
        None => Arc::new(NoopSessionLog),
    };
    let config = inputs.run_config(&resolved, log);
    let mut response = run(prepare(source, config)?)?;
    if let Some(warning) = adapter.as_ref().and_then(finish_log) {
        response = response.with_warning(warning);
    }
    Ok(response)
}

/// Sends `follow_up` to the saved session `session_id`.
///
/// Opens the session store at the resolved session directory and the session
/// logger at the resolved log root, builds the backend registry unless one
/// was injected, and flushes. A flush failure is appended to the response as
/// a warning.
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
    let resolved = inputs.resolve()?;
    let adapter = ObservabilityAdapter::open(&inputs.policy, &resolved.log_root)?;
    let sessions = FileSessionStore::new(&resolved.sessions_dir)?;
    let config = inputs.run_config(&resolved, adapter.shared());
    let mut response = ask(&sessions, session_id, follow_up, config)?;
    if let Some(warning) = finish_log(&adapter) {
        response = response.with_warning(warning);
    }
    Ok(response)
}

/// Flushes the session log. The log is best effort: a failure becomes a
/// warning string and does not change the outcome (CR-014).
fn finish_log(adapter: &ObservabilityAdapter) -> Option<String> {
    flush_warning(adapter.flush())
}

fn flush_warning(flushed: Result<(), JevError>) -> Option<String> {
    flushed
        .err()
        .map(|error| format!("session log was not flushed: {}", error.fix_hint()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_failure_is_a_warning_not_an_error() {
        let warning = flush_warning(Err(JevError::operational_config(
            "session log flush failed (disk full). Check that the log root is writable.",
        )))
        .expect("warning");
        assert!(warning.contains("disk full"), "{warning}");
        assert!(flush_warning(Ok(())).is_none());
    }

    #[test]
    fn defaults_are_independent_and_unset() {
        let inputs = ComposeInputs::new(PathBuf::from("."), PathPolicy::cli());
        assert_eq!(inputs.mode, MultiBackendMode::Independent);
        assert!(!inputs.dry_run);
        assert!(inputs.registry.is_none());
        assert!(inputs.backends.is_none());
    }
}
