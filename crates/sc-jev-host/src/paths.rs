//! Log-root and session-directory resolution (ADR-SAG-0021).
//!
//! [`PathPolicy`] is one plain data type. A composition root picks a preset
//! ([`PathPolicy::cli`], [`PathPolicy::python`]) or writes its own literal; a
//! new consumer never edits this crate (EXT-012). Each preset encodes the
//! precedence of its root unchanged: the explicit value wins, then the
//! environment variables in order, then the default.
//!
//! This module is the only place in the workspace that reads the log and
//! session path environment variables (ADR-SAG-0026 environment ownership).
//! Every resolver has a pure `_with` form over an injected environment, so a
//! test needs no process-wide environment change.
//!
//! An empty explicit path or environment value counts as absent.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use sc_agent_types::RepoRoot;

/// Where the log root lives when neither a flag nor an environment variable
/// sets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultRoot {
    /// `<repo-root>/<path>`.
    UnderRepoRoot(PathBuf),
    /// `<home>/<home_relative>`, where home is `HOME`, else `USERPROFILE`;
    /// when neither is set (or the one found is empty), `<temp dir>/<temp_fallback>`.
    UnderHome {
        /// Path below the home directory.
        home_relative: PathBuf,
        /// Path below the process temp directory when home is unknown.
        temp_fallback: PathBuf,
    },
    /// This exact path.
    Fixed(PathBuf),
}

/// Where the session directory lives when neither a flag nor an environment
/// variable sets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionDefault {
    /// `<log-root>/<path>`.
    UnderLogRoot(PathBuf),
    /// `<repo-root>/<path>`, whatever the log root is.
    UnderRepoRoot(PathBuf),
    /// This exact path.
    Fixed(PathBuf),
}

/// The log and session path rules of one composition root.
///
/// # Examples
///
/// A new consumer writes its own literal; flag, then environment, then the
/// default:
///
/// ```
/// use std::ffi::OsString;
/// use std::path::{Path, PathBuf};
///
/// use sc_jev_host::paths::{DefaultRoot, PathPolicy, SessionDefault};
///
/// let policy = PathPolicy {
///     service: "my-server",
///     log_root_flag: "--logs",
///     log_root_env: &["MY_SERVER_LOGS"],
///     log_root_default: DefaultRoot::Fixed(PathBuf::from("/var/log/my-server")),
///     session_dir_env: &[],
///     session_dir_default: SessionDefault::UnderLogRoot(PathBuf::from("sessions")),
/// };
/// let repo = sc_agent_types::RepoRoot::new("/repo");
/// let env = |name: &str| (name == "MY_SERVER_LOGS").then(|| OsString::from("/env"));
/// assert_eq!(
///     policy.resolve_log_root_with(None, &repo, &env),
///     Path::new("/env")
/// );
/// assert_eq!(
///     policy.resolve_log_root_with(Some(Path::new("/flag")), &repo, &env),
///     Path::new("/flag")
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPolicy {
    /// Service name; the JSONL file is `<log-root>/logs/<service>.log.jsonl`.
    pub service: &'static str,
    /// Name of the explicit log-root argument, for messages (`--log-dir`,
    /// `log_dir`).
    pub log_root_flag: &'static str,
    /// Environment variables that set the log root, first match wins.
    pub log_root_env: &'static [&'static str],
    /// The log root when nothing else sets it.
    pub log_root_default: DefaultRoot,
    /// Environment variables that set the session directory, first match
    /// wins.
    pub session_dir_env: &'static [&'static str],
    /// The session directory when nothing else sets it.
    pub session_dir_default: SessionDefault,
}

impl PathPolicy {
    /// The `sc-ai` command-line policy (ADR-SAG-0021): `--log-dir`, then
    /// `SC_LOG_ROOT`, then `<repo-root>/.sc-ai`; sessions in
    /// `<repo-root>/.sc-ai/sessions`.
    #[must_use]
    pub fn cli() -> Self {
        Self {
            service: "sc-ai",
            log_root_flag: "--log-dir",
            log_root_env: &["SC_LOG_ROOT"],
            log_root_default: DefaultRoot::UnderRepoRoot(PathBuf::from(".sc-ai")),
            session_dir_env: &[],
            session_dir_default: SessionDefault::UnderRepoRoot(
                PathBuf::from(".sc-ai").join("sessions"),
            ),
        }
    }

    /// The `sc-jev-py` Python policy (ADR-SAG-0021): `log_dir`, then
    /// `SC_AGENT_LOG_DIR`, then `SC_LOG_ROOT`, then `~/.sc-agent/log`
    /// (`USERPROFILE` when `HOME` is unset, the temp directory plus
    /// `sc-agent/log` when both are); sessions from `session_dir`, then
    /// `SC_AGENT_SESSION_DIR`, then `<log-root>/sessions`.
    #[must_use]
    pub fn python() -> Self {
        Self {
            service: "sc-jev-py",
            log_root_flag: "log_dir",
            log_root_env: &["SC_AGENT_LOG_DIR", "SC_LOG_ROOT"],
            log_root_default: DefaultRoot::UnderHome {
                home_relative: PathBuf::from(".sc-agent").join("log"),
                temp_fallback: PathBuf::from("sc-agent").join("log"),
            },
            session_dir_env: &["SC_AGENT_SESSION_DIR"],
            session_dir_default: SessionDefault::UnderLogRoot(PathBuf::from("sessions")),
        }
    }

    /// Resolves the log root from the process environment.
    #[must_use]
    pub fn resolve_log_root(&self, explicit: Option<&Path>, repo: &RepoRoot) -> PathBuf {
        self.resolve_log_root_with(explicit, repo, &|name| std::env::var_os(name))
    }

    /// [`Self::resolve_log_root`] over an injected environment lookup.
    #[must_use]
    pub fn resolve_log_root_with(
        &self,
        explicit: Option<&Path>,
        repo: &RepoRoot,
        env: &dyn Fn(&str) -> Option<OsString>,
    ) -> PathBuf {
        if let Some(path) = non_empty(explicit) {
            return path.to_path_buf();
        }
        if let Some(path) = first_env_path(self.log_root_env, env) {
            return path;
        }
        match &self.log_root_default {
            DefaultRoot::UnderRepoRoot(relative) => repo.as_path().join(relative),
            DefaultRoot::UnderHome {
                home_relative,
                temp_fallback,
            } => {
                let home = env("HOME").or_else(|| env("USERPROFILE"));
                match home.filter(|home| !home.is_empty()) {
                    Some(home) => PathBuf::from(home).join(home_relative),
                    None => std::env::temp_dir().join(temp_fallback),
                }
            }
            DefaultRoot::Fixed(path) => path.clone(),
        }
    }

    /// Resolves the session directory from the process environment.
    #[must_use]
    pub fn resolve_session_dir(
        &self,
        explicit: Option<&Path>,
        log_root: &Path,
        repo: &RepoRoot,
    ) -> PathBuf {
        self.resolve_session_dir_with(explicit, log_root, repo, &|name| std::env::var_os(name))
    }

    /// [`Self::resolve_session_dir`] over an injected environment lookup.
    #[must_use]
    pub fn resolve_session_dir_with(
        &self,
        explicit: Option<&Path>,
        log_root: &Path,
        repo: &RepoRoot,
        env: &dyn Fn(&str) -> Option<OsString>,
    ) -> PathBuf {
        if let Some(path) = non_empty(explicit) {
            return path.to_path_buf();
        }
        if let Some(path) = first_env_path(self.session_dir_env, env) {
            return path;
        }
        match &self.session_dir_default {
            SessionDefault::UnderLogRoot(relative) => log_root.join(relative),
            SessionDefault::UnderRepoRoot(relative) => repo.as_path().join(relative),
            SessionDefault::Fixed(path) => path.clone(),
        }
    }

    /// The JSONL file the file sink writes for this service under
    /// `log_root`: `<log-root>/logs/<service>.log.jsonl`.
    #[must_use]
    pub fn jsonl_path(&self, log_root: &Path) -> PathBuf {
        log_root
            .join("logs")
            .join(format!("{}.log.jsonl", self.service))
    }
}

fn non_empty(path: Option<&Path>) -> Option<&Path> {
    path.filter(|path| !path.as_os_str().is_empty())
}

fn first_env_path(names: &[&str], env: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    names
        .iter()
        .find_map(|name| env(name).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}
