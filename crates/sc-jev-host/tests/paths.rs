//! ADR-SAG-0021 precedence for both presets and a custom policy
//! (NFR-SAG-0003, EXT-031). Environment is injected, never set.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use sc_agent_types::RepoRoot;
use sc_jev_host::paths::{DefaultRoot, PathPolicy, SessionDefault};

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(value))
    }
}

fn repo() -> RepoRoot {
    RepoRoot::new("/repo")
}

#[test]
fn test_cli_path_policy_precedence() {
    let policy = PathPolicy::cli();
    let repo = repo();
    let none = env_of(&[]);
    let both = env_of(&[("SC_LOG_ROOT", "/env-root")]);
    // Default: <repo>/.sc-ai.
    assert_eq!(
        policy.resolve_log_root_with(None, &repo, &none),
        Path::new("/repo/.sc-ai")
    );
    // Environment beats the default.
    assert_eq!(
        policy.resolve_log_root_with(None, &repo, &both),
        Path::new("/env-root")
    );
    // The flag beats the environment.
    assert_eq!(
        policy.resolve_log_root_with(Some(Path::new("/flag")), &repo, &both),
        Path::new("/flag")
    );
    // An empty environment value and an empty flag count as absent.
    let empty = env_of(&[("SC_LOG_ROOT", "")]);
    assert_eq!(
        policy.resolve_log_root_with(Some(Path::new("")), &repo, &empty),
        Path::new("/repo/.sc-ai")
    );
    // Sessions default under the repo root, wherever the log root is, and
    // the CLI has no session environment variable.
    let log_root = policy.resolve_log_root_with(Some(Path::new("/flag")), &repo, &both);
    assert_eq!(
        policy.resolve_session_dir_with(None, &log_root, &repo, &both),
        Path::new("/repo/.sc-ai/sessions")
    );
    assert_eq!(
        policy.resolve_session_dir_with(Some(Path::new("/sessions")), &log_root, &repo, &both),
        Path::new("/sessions")
    );
    assert_eq!(
        policy.jsonl_path(Path::new("/log")),
        Path::new("/log/logs/sc-ai.log.jsonl")
    );
}

#[test]
fn test_python_path_policy_precedence() {
    let policy = PathPolicy::python();
    let repo = repo();
    let explicit = Path::new("/explicit");
    let env = env_of(&[
        ("SC_AGENT_LOG_DIR", "/agent-log"),
        ("SC_LOG_ROOT", "/obs-log"),
        ("HOME", "/home/me"),
    ]);
    assert_eq!(
        policy.resolve_log_root_with(Some(explicit), &repo, &env),
        explicit
    );
    assert_eq!(
        policy.resolve_log_root_with(None, &repo, &env),
        Path::new("/agent-log")
    );
    let obs_only = env_of(&[("SC_LOG_ROOT", "/obs-log"), ("HOME", "/home/me")]);
    assert_eq!(
        policy.resolve_log_root_with(None, &repo, &obs_only),
        Path::new("/obs-log")
    );
    let home = env_of(&[("HOME", "/home/me")]);
    assert_eq!(
        policy.resolve_log_root_with(None, &repo, &home),
        Path::new("/home/me").join(".sc-agent").join("log")
    );
    // HOME unset: USERPROFILE.
    let profile = env_of(&[("USERPROFILE", "C:/Users/me")]);
    assert_eq!(
        policy.resolve_log_root_with(None, &repo, &profile),
        Path::new("C:/Users/me").join(".sc-agent").join("log")
    );
    // Neither, or an empty HOME: the temp directory fallback.
    let temp = std::env::temp_dir().join("sc-agent").join("log");
    assert_eq!(
        policy.resolve_log_root_with(None, &repo, &env_of(&[])),
        temp
    );
    assert_eq!(
        policy.resolve_log_root_with(None, &repo, &env_of(&[("HOME", "")])),
        temp
    );

    // Sessions: explicit, SC_AGENT_SESSION_DIR, <log-root>/sessions.
    let log_root = Path::new("/agent-log");
    let sessions_env = env_of(&[("SC_AGENT_SESSION_DIR", "/env-sessions")]);
    assert_eq!(
        policy.resolve_session_dir_with(Some(Path::new("/s")), log_root, &repo, &sessions_env),
        Path::new("/s")
    );
    assert_eq!(
        policy.resolve_session_dir_with(None, log_root, &repo, &sessions_env),
        Path::new("/env-sessions")
    );
    assert_eq!(
        policy.resolve_session_dir_with(None, log_root, &repo, &env_of(&[])),
        Path::new("/agent-log/sessions")
    );
    assert_eq!(
        policy.jsonl_path(Path::new("/log")),
        Path::new("/log/logs/sc-jev-py.log.jsonl")
    );
}

/// EXT-031: a consumer outside the two presets writes its own literal.
#[test]
fn test_custom_path_policy_resolves() {
    let policy = PathPolicy {
        service: "my-server",
        log_root_flag: "--logs",
        log_root_env: &["MY_SERVER_LOGS"],
        log_root_default: DefaultRoot::Fixed(PathBuf::from("/var/log/my-server")),
        session_dir_env: &["MY_SERVER_SESSIONS"],
        session_dir_default: SessionDefault::Fixed(PathBuf::from("/var/lib/my-server")),
    };
    let repo = repo();
    let env = env_of(&[
        ("MY_SERVER_LOGS", "/env-logs"),
        ("MY_SERVER_SESSIONS", "/env-sessions"),
    ]);
    // flag > environment > default.
    assert_eq!(
        policy.resolve_log_root_with(Some(Path::new("/flag")), &repo, &env),
        Path::new("/flag")
    );
    assert_eq!(
        policy.resolve_log_root_with(None, &repo, &env),
        Path::new("/env-logs")
    );
    assert_eq!(
        policy.resolve_log_root_with(None, &repo, &env_of(&[])),
        Path::new("/var/log/my-server")
    );
    assert_eq!(
        policy.resolve_session_dir_with(Some(Path::new("/flag")), Path::new("/l"), &repo, &env),
        Path::new("/flag")
    );
    assert_eq!(
        policy.resolve_session_dir_with(None, Path::new("/l"), &repo, &env),
        Path::new("/env-sessions")
    );
    assert_eq!(
        policy.resolve_session_dir_with(None, Path::new("/l"), &repo, &env_of(&[])),
        Path::new("/var/lib/my-server")
    );
    assert_eq!(
        policy.jsonl_path(Path::new("/l")),
        Path::new("/l/logs/my-server.log.jsonl")
    );
}

/// NFR-SAG-0003: the default and a configured session and log root both
/// resolve under the repo or the configured directory, never elsewhere.
#[test]
fn test_default_and_configured_roots() {
    let policy = PathPolicy::cli();
    let repo = RepoRoot::new("/work/project");
    let env = env_of(&[]);
    let log_root = policy.resolve_log_root_with(None, &repo, &env);
    assert!(log_root.starts_with("/work/project"));
    let configured = policy.resolve_log_root_with(Some(Path::new("/elsewhere")), &repo, &env);
    assert_eq!(configured, Path::new("/elsewhere"));
    assert_eq!(
        policy.jsonl_path(&configured),
        Path::new("/elsewhere/logs/sc-ai.log.jsonl")
    );
}
