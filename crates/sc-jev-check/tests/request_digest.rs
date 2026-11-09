//! `request_sha256` is computed before sending and equals what a send records
//! (REQ-SAG-0621, ADR-SAG-0031).
//!
//! Needs `--features test-util` (mock backends).

use std::fs;
use std::path::Path;
use std::sync::Arc;

use sc_agent_types::observability::v2::{NoopSessionLog, SharedSessionLog};
use sc_agent_types::{
    BackendId, BackendSet, ModelAliases, RepoRelativePath, RepoRoot, RequestSha256,
};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_jev_check::config::CheckSpec;
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::{CheckSource, RunConfig, RunResponse, prepare, run};
use serde_json::json;

fn questions(instructions: &str) -> String {
    json!({
        "schema": "plan-anchoring/questions/1",
        "version": 1,
        "questions": {"relevant": {
            "type": "choice",
            "instructions": instructions,
            "criteria": {"no": "no", "yes": "yes"}
        }}
    })
    .to_string()
}

fn state(choice: &str) -> String {
    json!({
        "schema": "plan-anchoring/state/1",
        "answers": [{"question_id": "relevant", "choice": choice}]
    })
    .to_string()
}

/// A repo with questions, a state and a system prompt.
fn repo(instructions: &str, state_choice: &str, prompt: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp");
    fs::write(dir.path().join("questions.json"), questions(instructions)).expect("questions");
    fs::write(dir.path().join("state.json"), state(state_choice)).expect("state");
    fs::write(dir.path().join("prompt.md"), prompt).expect("prompt");
    dir
}

fn spec() -> CheckSpec {
    let path = |name: &str| RepoRelativePath::try_from(name).expect("path");
    CheckSpec::new(path("questions.json"))
        .with_state_path(path("state.json"))
        .with_system_prompt_file(path("prompt.md"))
}

fn backends() -> BackendSet {
    BackendSet::try_from(vec![BackendId::Jev, BackendId::Haiku]).expect("set")
}

fn config(dir: &Path) -> RunConfig {
    let log: SharedSessionLog = Arc::new(NoopSessionLog);
    RunConfig::new(RepoRoot::new(dir), log).backends(backends())
}

fn dry(dir: &Path, models: Option<ModelAliases>) -> RunResponse {
    let mut config = config(dir).dry_run(true);
    if let Some(models) = models {
        config = config.models(models);
    }
    run(prepare(CheckSource::Spec(spec()), config).expect("prepare")).expect("dry run")
}

fn digest(response: &RunResponse, id: BackendId) -> RequestSha256 {
    response
        .request_sha256()
        .iter()
        .find(|(backend, _)| *backend == id)
        .map(|(_, digest)| digest.clone())
        .unwrap_or_else(|| panic!("no request_sha256 for {id}"))
}

#[test]
fn test_live_run_reports_the_digests_a_dry_run_does() {
    // The per-backend proofs that the digest equals what a send records (the
    // trace behind the session-log `request_sha256`) are in `sc-jev`
    // (`catalog.rs`) and `sc-jev-backends` (`llm.rs`): this crate cannot
    // depend on the HTTP crate (boundaries). Here the engine's live path,
    // driven by mock backends, must report what the dry run reports.
    let dir = repo("Does it apply?", "yes", "judge carefully");
    let dry_run = dry(dir.path(), None);
    assert_eq!(dry_run.request_sha256().len(), 2);

    let answers = |choice: &str| {
        serde_json::from_value(json!({"relevant": {"choice": choice}})).expect("answers")
    };
    let mut registry = BackendRegistry::new();
    for id in [BackendId::Jev, BackendId::Haiku] {
        registry
            .register(Arc::new(MockJudgeBackend::new(id, answers("no"))))
            .expect("register");
    }
    let sessions = tempfile::tempdir().expect("sessions");
    let live = run(prepare(
        CheckSource::Spec(spec()),
        config(dir.path())
            .sessions_dir(sessions.path())
            .registry(Arc::new(registry)),
    )
    .expect("prepare"))
    .expect("live run");
    assert_eq!(live.request_sha256(), dry_run.request_sha256());
    assert_eq!(live.questions_sha256(), dry_run.questions_sha256());
}

#[test]
fn test_request_digest_changes_with_question_state_model_and_system_prompt() {
    let base_dir = repo("Does it apply?", "yes", "judge carefully");
    let base = dry(base_dir.path(), None);
    let (jev, haiku) = (
        digest(&base, BackendId::Jev),
        digest(&base, BackendId::Haiku),
    );
    // Stable for the same inputs.
    assert_eq!(
        dry(base_dir.path(), None).request_sha256(),
        base.request_sha256()
    );

    // A question: both backends change.
    let other = dry(
        repo("Does it really apply?", "yes", "judge carefully").path(),
        None,
    );
    assert_ne!(digest(&other, BackendId::Jev), jev);
    assert_ne!(digest(&other, BackendId::Haiku), haiku);

    // The state: both change.
    let other = dry(repo("Does it apply?", "no", "judge carefully").path(), None);
    assert_ne!(digest(&other, BackendId::Jev), jev);
    assert_ne!(digest(&other, BackendId::Haiku), haiku);

    // The LLM system prompt: only the LLM backend changes (Jev takes none).
    let other = dry(repo("Does it apply?", "yes", "judge harshly").path(), None);
    assert_eq!(digest(&other, BackendId::Jev), jev);
    assert_ne!(digest(&other, BackendId::Haiku), haiku);
    // `payload_sha256` (the payload digest) does not see the system prompt.
    assert_eq!(other.payload_sha256(), base.payload_sha256());

    // The model: each backend changes with its own model only.
    let aliases = ModelAliases::builtin()
        .with_override("jev", "jev-9.9.9".parse().expect("model"))
        .expect("alias");
    let other = dry(base_dir.path(), Some(aliases));
    assert_ne!(digest(&other, BackendId::Jev), jev);
    assert_eq!(digest(&other, BackendId::Haiku), haiku);
    assert_eq!(other.payload_sha256(), base.payload_sha256());

    // The question set alone does not see the state.
    let state_only = dry(repo("Does it apply?", "no", "judge carefully").path(), None);
    assert_eq!(state_only.questions_sha256(), base.questions_sha256());
}

/// A question reorder changes the payload and request digests (the payload
/// array is ordered, and so is the body Jev reads) but not the digest of the
/// question set.
#[test]
fn test_question_reorder_changes_payload_and_request_digests_only() {
    use sc_jev_check::run::CallerRequest;
    let dir = tempfile::tempdir().expect("temp");
    let dry = |questions: &str| {
        let document = format!(r#"{{"questions":{questions},"state":[],"backends":["jev"]}}"#);
        let request = CallerRequest::from_json(document.as_bytes()).expect("request");
        run(prepare(
            CheckSource::CallerRequest(request),
            config(dir.path()).dry_run(true),
        )
        .expect("prepare"))
        .expect("dry run")
    };
    let q = |id: &str| format!(r#"{{"id":"{id}","prompt":"{id}?","choices":["no","yes"]}}"#);
    let ab = dry(&format!("[{},{}]", q("a"), q("b")));
    let ba = dry(&format!("[{},{}]", q("b"), q("a")));
    assert_eq!(ab.questions_sha256(), ba.questions_sha256());
    assert_ne!(ab.payload_sha256(), ba.payload_sha256());
    assert_ne!(
        digest(&ab, BackendId::Jev),
        digest(&ba, BackendId::Jev),
        "the request body keeps question order"
    );
}
