//! The general log beside the session log: separate files, injectable, with
//! sc-agent defaults (ADR-SAG-0013 amendment 2026-11-09).

use std::path::Path;
use std::sync::Arc;

use sc_agent_types::observability::v2::{
    LogEvent, LogRejected, RecordingGeneralLog, RejectingGeneralLog, SessionLog,
};
use sc_agent_types::{BackendId, BackendSet, RepoRoot};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::CheckSource;
use sc_jev_host::compose::{ComposeInputs, run_from};
use sc_jev_host::paths::PathPolicy;
use tempfile::TempDir;

const QUESTION_TEXT: &str = "PROMPT-TEXT-SECRET-5521";

fn repo() -> TempDir {
    let dir = tempfile::tempdir().expect("temp");
    std::fs::write(
        dir.path().join("questions.json"),
        format!(
            r#"{{"schema":"plan-anchoring/questions/1","version":1,"questions":{{"relevant":{{"type":"choice","instructions":"{QUESTION_TEXT}","criteria":{{"no":"No.","yes":"Yes."}}}}}}}}"#
        ),
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

fn registry() -> Arc<BackendRegistry> {
    let answers = serde_json::from_value(serde_json::json!({"relevant": {"choice": "yes"}}))
        .expect("answers");
    let mut registry = BackendRegistry::new();
    registry
        .register(Arc::new(MockJudgeBackend::new(
            BackendId::AnthropicHaiku,
            answers,
        )))
        .expect("register");
    Arc::new(registry)
}

fn inputs(repo: &Path) -> ComposeInputs {
    ComposeInputs::new(repo.to_path_buf(), PathPolicy::cli())
        .repo_root(RepoRoot::new(repo))
        .backends(BackendSet::try_from(vec![BackendId::AnthropicHaiku]).expect("set"))
        .registry(registry())
}

fn lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .expect("jsonl")
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("json line"))
        .collect()
}

#[test]
fn defaults_write_two_separate_logs() {
    let repo = repo();
    run_from(source(repo.path()), inputs(repo.path())).expect("run");
    let logs = repo.path().join(".sc-ai/logs");
    let session = lines(&logs.join("sc-ai.log.jsonl"));
    let general = lines(&logs.join("sc-ai-general.log.jsonl"));
    assert_eq!(session.len(), 1, "one session row per send");
    assert!(session[0]["fields"]["answer_id"].is_string());
    let actions: Vec<&str> = general
        .iter()
        .filter_map(|line| line["action"].as_str())
        .collect();
    for expected in ["paths.resolved", "prepare.start", "run.start", "run.done"] {
        assert!(actions.contains(&expected), "{expected} in {actions:?}");
    }
    assert!(
        general
            .iter()
            .all(|line| line["fields"]["answer_id"].is_null()),
        "session rows stay out of the general log"
    );
    assert!(
        general
            .iter()
            .any(|line| line["level"] == "Debug" || line["level"] == "debug"),
        "debug rows are kept: {general:?}"
    );
}

#[test]
fn the_general_log_never_holds_prompt_text() {
    let repo = repo();
    run_from(source(repo.path()), inputs(repo.path())).expect("run");
    let general = std::fs::read_to_string(repo.path().join(".sc-ai/logs/sc-ai-general.log.jsonl"))
        .expect("general");
    assert!(!general.contains(QUESTION_TEXT), "{general}");
    assert!(!general.contains("Does it apply"), "{general}");
}

#[test]
fn a_dry_run_opens_no_default_log() {
    let repo = repo();
    run_from(source(repo.path()), inputs(repo.path()).dry_run(true)).expect("dry");
    assert!(!repo.path().join(".sc-ai").exists());
}

#[test]
fn an_injected_general_log_replaces_the_default_file() {
    let repo = repo();
    let recorder = RecordingGeneralLog::new();
    run_from(
        source(repo.path()),
        inputs(repo.path()).general_log(Arc::new(recorder.clone())),
    )
    .expect("run");
    let actions = recorder.actions();
    assert!(
        actions.contains(&"run.start") && actions.contains(&"run.done"),
        "{actions:?}"
    );
    let logs = repo.path().join(".sc-ai/logs");
    assert!(
        logs.join("sc-ai.log.jsonl").is_file(),
        "session default stays"
    );
    assert!(!logs.join("sc-ai-general.log.jsonl").exists());
    assert!(!recorder.rendered().contains(QUESTION_TEXT));
}

#[derive(Default)]
struct RecordingSession(std::sync::Mutex<Vec<LogEvent>>);

impl SessionLog for RecordingSession {
    fn emit(&self, event: LogEvent) -> Result<(), LogRejected> {
        self.0.lock().expect("lock").push(event);
        Ok(())
    }
}

#[test]
fn an_injected_session_log_replaces_the_default_file() {
    let repo = repo();
    let session = Arc::new(RecordingSession::default());
    run_from(
        source(repo.path()),
        inputs(repo.path()).session_log(Arc::clone(&session) as Arc<dyn SessionLog>),
    )
    .expect("run");
    assert_eq!(session.0.lock().expect("lock").len(), 1);
    let logs = repo.path().join(".sc-ai/logs");
    assert!(!logs.join("sc-ai.log.jsonl").exists());
    assert!(
        logs.join("sc-ai-general.log.jsonl").is_file(),
        "general default stays"
    );
}

#[test]
fn a_rejecting_general_log_changes_nothing() {
    let quiet = repo();
    let loud = repo();
    let first = run_from(
        source(quiet.path()),
        inputs(quiet.path()).general_log(Arc::new(RecordingGeneralLog::new())),
    )
    .expect("recording");
    let second = run_from(
        source(loud.path()),
        inputs(loud.path()).general_log(Arc::new(RejectingGeneralLog)),
    )
    .expect("rejecting");
    assert_eq!(first.is_partial(), second.is_partial());
    assert_eq!(first.results().len(), second.results().len());
    assert!(second.warnings().is_empty());
}
