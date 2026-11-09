//! `run_from` / `ask_from` wiring (REQ-SAG-0605, ARCHE-019, ADR-SAG-0009).

use std::path::Path;
use std::sync::Arc;

use sc_agent_types::{BackendId, BackendSet, JevError, RepoRoot, SessionId};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::CheckSource;
use sc_jev_host::compose::{ComposeInputs, ask_from, explicit_repo_root, run_from};
use sc_jev_host::paths::PathPolicy;
use tempfile::TempDir;

fn repo() -> TempDir {
    let dir = tempfile::tempdir().expect("temp");
    std::fs::write(
        dir.path().join("questions.json"),
        r#"{"schema":"plan-anchoring/questions/1","version":1,"questions":{"relevant":{"type":"choice","instructions":"Does it apply?","criteria":{"no":"No.","yes":"Yes."}}}}"#,
    )
    .expect("questions");
    std::fs::write(
        dir.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n",
    )
    .expect("check");
    dir
}

fn source(repo: &Path) -> CheckSource {
    CheckSource::check_file(&RepoRoot::new(repo), Path::new("check.toml")).expect("source")
}

fn mock_registry(id: BackendId) -> Arc<BackendRegistry> {
    let answers = serde_json::from_value(serde_json::json!({"relevant": {"choice": "yes"}}))
        .expect("answers");
    let mut registry = BackendRegistry::new();
    registry
        .register(Arc::new(MockJudgeBackend::new(id, answers)))
        .expect("register");
    Arc::new(registry)
}

fn inputs(repo: &Path) -> ComposeInputs {
    ComposeInputs::new(repo.to_path_buf(), PathPolicy::cli()).repo_root(RepoRoot::new(repo))
}

#[test]
fn test_dry_run_writes_nothing_and_defaults_to_independent() {
    let repo = repo();
    let response = run_from(source(repo.path()), inputs(repo.path()).dry_run(true)).expect("dry");
    assert!(response.dry_run());
    assert!(response.results().is_empty());
    let record = serde_json::to_value(response.equal_input()).expect("record");
    assert_eq!(record["mode"], "independent");
    assert!(
        !repo.path().join(".sc-ai").exists(),
        "no log root, no sessions"
    );
}

#[test]
fn test_live_run_logs_saves_a_session_and_ask_follows_up() {
    let repo = repo();
    let registry = mock_registry(BackendId::Haiku);
    let configured = || {
        inputs(repo.path())
            .backends(BackendSet::try_from(vec![BackendId::Haiku]).expect("set"))
            .registry(Arc::clone(&registry))
    };
    let response = run_from(source(repo.path()), configured()).expect("run");
    let id: SessionId = response.session_id().expect("session saved");
    assert!(response.warnings().is_empty());
    let log = repo.path().join(".sc-ai/logs/sc-ai.log.jsonl");
    let text = std::fs::read_to_string(&log).expect("jsonl");
    assert_eq!(
        text.lines().filter(|line| !line.is_empty()).count(),
        1,
        "{text}"
    );
    assert!(repo.path().join(".sc-ai/sessions").is_dir());

    let asked = ask_from(id, "why?", configured()).expect("ask");
    assert_eq!(asked.outcome().backend(), BackendId::Haiku);
    let text = std::fs::read_to_string(&log).expect("jsonl");
    assert_eq!(
        text.lines().filter(|line| !line.is_empty()).count(),
        2,
        "{text}"
    );
}

#[test]
fn test_explicit_repo_root_is_a_directory_made_absolute() {
    let repo = repo();
    let root = explicit_repo_root(repo.path()).expect("directory");
    assert!(root.as_path().is_absolute());
    let error: JevError =
        explicit_repo_root(&repo.path().join("absent")).expect_err("absent directory");
    assert_eq!(error.code(), "INPUT_MISSING");
    let error = explicit_repo_root(&repo.path().join("check.toml")).expect_err("a file");
    assert_eq!(error.code(), "INPUT_MISSING");
}

/// Audit rows lost silently: a run into an unwritable `logs/` directory still
/// returns its answer, with a warning in `data.warnings` naming the loss.
#[cfg(unix)]
#[test]
fn test_unwritable_log_dir_is_a_warning_not_a_silent_loss() {
    use std::os::unix::fs::PermissionsExt;
    let repo = repo();
    let logs = repo.path().join(".sc-ai").join("logs");
    std::fs::create_dir_all(&logs).expect("logs");
    std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o555)).expect("chmod");
    if std::fs::write(logs.join(".probe"), b"x").is_ok() {
        return; // root can write anywhere
    }
    let response = run_from(
        source(repo.path()),
        inputs(repo.path())
            .backends(BackendSet::try_from(vec![BackendId::Haiku]).expect("set"))
            .registry(mock_registry(BackendId::Haiku)),
    )
    .expect("the answer is kept");
    assert!(!response.is_partial());
    let warnings = response.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with("session log rows lost: 1 session-log row(s) could not be written"),
        "{warnings:?}"
    );
    let envelope = serde_json::to_value(&response).expect("json");
    assert_eq!(envelope["warnings"], serde_json::json!(warnings));
    std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o700)).expect("restore");
}

/// `.sc-ai/` shows in `git status`, so `git add -A` would commit session
/// files holding prompts: sc-agent ignores the directory it creates.
#[test]
fn test_a_created_agent_directory_ignores_itself_and_an_existing_one_is_left() {
    let repo = repo();
    let configured = |repo: &Path| {
        inputs(repo)
            .backends(BackendSet::try_from(vec![BackendId::Haiku]).expect("set"))
            .registry(mock_registry(BackendId::Haiku))
    };
    run_from(source(repo.path()), configured(repo.path())).expect("run");
    assert_eq!(
        std::fs::read_to_string(repo.path().join(".sc-ai/.gitignore")).expect("gitignore"),
        "*\n"
    );

    // The user's own `.sc-ai/` (and its `.gitignore`) is never rewritten.
    let other = self::repo();
    std::fs::create_dir(other.path().join(".sc-ai")).expect("dir");
    std::fs::write(other.path().join(".sc-ai/.gitignore"), "keep\n").expect("own");
    run_from(source(other.path()), configured(other.path())).expect("run");
    assert_eq!(
        std::fs::read_to_string(other.path().join(".sc-ai/.gitignore")).expect("own"),
        "keep\n"
    );
}
