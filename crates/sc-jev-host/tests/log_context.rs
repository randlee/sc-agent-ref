//! Log context from compose: `repo` is the main repository, `project` is
//! per call, and paths stay on the worktree (REQ-SAG-0614, REQ-SAG-0615,
//! NFR-SAG-0003, ADR-SAG-0021).

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

#[path = "../../../tests/support/deadline.rs"]
mod deadline;

use sc_agent_types::{BackendId, BackendSet, ProjectLabel, RepoRoot, SessionId};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::CheckSource;
use sc_jev_host::compose::{ComposeInputs, ask_from, run_from};
use sc_jev_host::paths::PathPolicy;
use tempfile::TempDir;

fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: builds fixture repositories with isolated `git`"
)]
fn git(dir: &Path, args: &[&str]) {
    let hooks = format!("core.hooksPath={}", null_device());
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args(["-c", "commit.gpgsign=false", "-c", &hooks])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", null_device())
        .env("GIT_CONFIG_SYSTEM", null_device())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_PREFIX",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_DEFAULT_HASH",
    ] {
        command.env_remove(var);
    }
    let output = deadline::spawn_with_deadline(&mut command, None);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

const QUESTIONS: &str = r#"{"schema":"plan-anchoring/questions/1","version":1,"questions":{"relevant":{"type":"choice","instructions":"Does it apply?","criteria":{"no":"No.","yes":"Yes."}}}}"#;

/// A main repository with a committed check and one linked worktree.
fn main_and_linked() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("temp");
    git(dir.path(), &["init", "-q"]);
    std::fs::write(dir.path().join("questions.json"), QUESTIONS).expect("questions");
    std::fs::write(
        dir.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n",
    )
    .expect("check");
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    let linked = dir.path().join("linked");
    git(
        dir.path(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            linked.to_str().expect("utf8"),
        ],
    );
    (dir, linked)
}

fn registry() -> Arc<BackendRegistry> {
    let answers = serde_json::from_value(serde_json::json!({"relevant": {"choice": "yes"}}))
        .expect("answers");
    let mut registry = BackendRegistry::new();
    registry
        .register(Arc::new(
            MockJudgeBackend::new(BackendId::Haiku, answers).with_sent_body("{}"),
        ))
        .expect("register");
    Arc::new(registry)
}

fn inputs(worktree: &Path) -> ComposeInputs {
    // No explicit repo root: compose discovers it from the working directory.
    ComposeInputs::new(worktree.to_path_buf(), PathPolicy::cli())
        .backends(BackendSet::try_from(vec![BackendId::Haiku]).expect("set"))
        .registry(registry())
}

/// Every written line, whole (envelope plus `fields`).
fn lines(log: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(log)
        .expect("jsonl")
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("line"))
        .collect()
}

fn rows(log: &Path) -> Vec<serde_json::Value> {
    lines(log)
        .iter()
        .map(|line| line["fields"].clone())
        .collect()
}

/// The envelope carries the level and correlation ids derived from the row
/// (RSH-003, RSH-004); `fields` is unchanged.
fn assert_envelope(line: &serde_json::Value, level: &str) {
    assert_eq!(line["level"], level, "{line}");
    assert_eq!(
        line["correlation_id"], line["fields"]["session_id"],
        "{line}"
    );
    assert_eq!(line["request_id"], line["fields"]["answer_id"], "{line}");
    assert!(line["correlation_id"].is_string() && line["request_id"].is_string());
}

/// The row's `repo` canonicalized the same way, so the comparison holds on
/// Windows, where git prints `C:/...` and `canonicalize` gives `\\?\C:\...`.
fn logged_repo(row: &serde_json::Value) -> String {
    canonical(Path::new(row["repo"].as_str().expect("repo is a string")))
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .expect("canonical")
        .to_string_lossy()
        .into_owned()
}

/// From a linked worktree: the session file and the log stay under the
/// worktree, and the row's `repo` is the main repository.
#[test]
fn test_log_context_repo_is_main_repo_paths_stay_on_worktree() {
    let (main, linked) = main_and_linked();
    let source =
        CheckSource::check_file(&RepoRoot::new(&linked), Path::new("check.toml")).expect("source");
    let label = ProjectLabel::new("billing").expect("label");
    let response = run_from(source, inputs(&linked).project(label)).expect("run");
    assert!(response.warnings().is_empty(), "{:?}", response.warnings());
    let id: SessionId = response.session_id().expect("session saved");

    assert!(linked.join(format!(".sc-ai/sessions/{id}.json")).is_file());
    assert!(linked.join(".sc-ai/logs/sc-ai.log.jsonl").is_file());
    assert!(
        !main.path().join(".sc-ai").exists(),
        "nothing under the main repo"
    );

    let envelopes = lines(&linked.join(".sc-ai/logs/sc-ai.log.jsonl"));
    assert_eq!(envelopes.len(), 1);
    assert_envelope(&envelopes[0], "Info");
    assert_eq!(envelopes[0]["correlation_id"], id.to_string());
    let rows = rows(&linked.join(".sc-ai/logs/sc-ai.log.jsonl"));
    assert_eq!(rows.len(), 1);
    assert_eq!(logged_repo(&rows[0]), canonical(main.path()));
    assert_eq!(rows[0]["project"], "billing");
    assert_eq!(rows[0]["session_id"], id.to_string());

    // A follow-up is its own call: its own project (here none), same repo.
    let asked = ask_from(id, "why?", inputs(&linked)).expect("ask");
    assert!(asked.warnings().is_empty(), "{:?}", asked.warnings());
    let rows = self::rows(&linked.join(".sc-ai/logs/sc-ai.log.jsonl"));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["operation"], "ask");
    assert_eq!(logged_repo(&rows[1]), canonical(main.path()));
    assert!(rows[1]["project"].is_null());
    assert_eq!(rows[1]["session_created_at"], rows[0]["session_created_at"]);
}

/// ULID names change no directory: sessions under `.sc-ai/sessions`, logs
/// under `.sc-ai/logs` (NFR-SAG-0003, ADR-SAG-0021).
#[test]
fn test_log_and_session_paths_unchanged_with_ulid_names() {
    let dir = tempfile::tempdir().expect("temp");
    std::fs::write(dir.path().join("questions.json"), QUESTIONS).expect("questions");
    std::fs::write(
        dir.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n",
    )
    .expect("check");
    let source = CheckSource::check_file(&RepoRoot::new(dir.path()), Path::new("check.toml"))
        .expect("source");
    let response = run_from(source, inputs(dir.path())).expect("run");
    assert!(response.warnings().is_empty(), "{:?}", response.warnings());
    let id = response.session_id().expect("session saved");
    let text = id.to_string();
    assert_eq!(text.len(), 26);
    assert!(
        dir.path()
            .join(format!(".sc-ai/sessions/{text}.json"))
            .is_file()
    );
    assert!(dir.path().join(".sc-ai/logs/sc-ai.log.jsonl").is_file());
    let sessions: Vec<_> = std::fs::read_dir(dir.path().join(".sc-ai/sessions"))
        .expect("sessions")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(sessions, [std::ffi::OsString::from(format!("{text}.json"))]);
}

/// With an explicit repo root, discovery starts there, not at the working
/// directory (D5): the cwd is an unrelated folder outside git.
#[test]
fn test_log_context_explicit_repo_root_starts_discovery() {
    let (main, linked) = main_and_linked();
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    let explicit = sc_jev_host::compose::explicit_repo_root(&linked).expect("explicit root");
    let source = CheckSource::check_file(&explicit, Path::new("check.toml")).expect("source");
    let inputs = ComposeInputs::new(elsewhere.path().to_path_buf(), PathPolicy::cli())
        .repo_root(explicit)
        .backends(BackendSet::try_from(vec![BackendId::Haiku]).expect("set"))
        .registry(registry());
    let response = run_from(source, inputs).expect("run");
    assert!(response.warnings().is_empty(), "{:?}", response.warnings());
    let rows = rows(&linked.join(".sc-ai/logs/sc-ai.log.jsonl"));
    assert_eq!(rows.len(), 1);
    assert_eq!(logged_repo(&rows[0]), canonical(main.path()));
}

/// An all-backends-failed run is an `Err`, and its failed row is still in the
/// flushed log (RSH-002).
#[test]
fn test_error_path_flushes_the_failed_row() {
    let dir = tempfile::tempdir().expect("temp");
    std::fs::write(dir.path().join("questions.json"), QUESTIONS).expect("questions");
    std::fs::write(
        dir.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n",
    )
    .expect("check");
    let mut failing = BackendRegistry::new();
    failing
        .register(Arc::new(
            MockJudgeBackend::new(BackendId::Haiku, Default::default())
                .with_failure(sc_agent_types::JevError::response_invalid("scripted")),
        ))
        .expect("register");
    let source = CheckSource::check_file(&RepoRoot::new(dir.path()), Path::new("check.toml"))
        .expect("source");
    let inputs = ComposeInputs::new(dir.path().to_path_buf(), PathPolicy::cli())
        .backends(BackendSet::try_from(vec![BackendId::Haiku]).expect("set"))
        .registry(Arc::new(failing));
    let response = run_from(source, inputs).expect("an all-failed run returns its response");
    let failure = response.first_failure().expect("failed").error();
    assert_eq!(failure.code(), "RESPONSE_INVALID");
    assert_eq!(failure.backend(), Some("haiku"));
    let envelopes = lines(&dir.path().join(".sc-ai/logs/sc-ai.log.jsonl"));
    assert_eq!(envelopes.len(), 1);
    assert_envelope(&envelopes[0], "Warn");
    assert_eq!(envelopes[0]["fields"]["send_outcome"], "failed");
}
