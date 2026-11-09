//! Multi-backend semantics of `run` (ADR-SAG-0025, REQ-SAG-0602/0603/0604
//! engine part/0605 engine part/0606). Every test drives `prepare` then `run`
//! over `MockJudgeBackend`s; the wire-level digest tests live in the backend
//! crates. Needs `--features test-util`.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use sc_agent_types::observability::v2::{LogEvent, LogRejected, SessionLog, SharedSessionLog};
use sc_agent_types::{
    BackendId, BackendSet, BusyCause, JevError, NetworkErrorKind, RepoRoot, SendTrace, Sha256Digest,
};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_decision_schema::{Attestation, JudgeBackend, JudgeOutcome, JudgeRequest, MultiBackendMode};
use sc_jev_check::config::PACKAGED_SYSTEM_PROMPT;
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::{BackendStatus, CheckSource, RunConfig, RunResponse, prepare, run};
use tempfile::TempDir;

/// Header the engine puts before prior answers in an LLM system prompt.
const SESSION_CONTEXT_HEADER: &str = "Session context:";

const QUESTIONS: &str = r#"{
  "schema": "plan-anchoring/questions/1",
  "version": 1,
  "questions": {
    "relevant": {
      "type": "choice",
      "instructions": "Does it apply?",
      "criteria": {"no": "No.", "yes": "Yes."}
    }
  }
}"#;

fn bare_repo() -> TempDir {
    let dir = tempfile::tempdir().expect("temp");
    fs::write(dir.path().join("questions.json"), QUESTIONS).expect("questions");
    fs::write(
        dir.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n",
    )
    .expect("check");
    dir
}

fn answers(
    choice: &str,
) -> std::collections::BTreeMap<sc_decision_schema::QuestionId, sc_decision_schema::QuestionAnswer>
{
    serde_json::from_value(serde_json::json!({"relevant": {"choice": choice}})).expect("answers")
}

fn mock(id: BackendId, choice: &str) -> MockJudgeBackend {
    MockJudgeBackend::new(id, answers(choice))
}

fn busy() -> JevError {
    JevError::provider_busy("scripted busy", 429, 4, BusyCause::Sends, None)
}

fn set(ids: &[BackendId]) -> BackendSet {
    BackendSet::try_from(ids.to_vec()).expect("backend set")
}

fn registry(judges: Vec<Arc<dyn JudgeBackend>>) -> Arc<BackendRegistry> {
    let mut registry = BackendRegistry::new();
    for judge in judges {
        registry.register(judge).expect("register");
    }
    Arc::new(registry)
}

/// Collects every log event.
#[derive(Default)]
struct Sink(Mutex<Vec<(String, String, String)>>);

impl SessionLog for Sink {
    fn emit(&self, event: LogEvent) -> Result<(), LogRejected> {
        self.0.lock().expect("lock").push((
            event.fields()["backend"]
                .as_str()
                .expect("backend")
                .to_owned(),
            event.operation().as_str().to_owned(),
            event.fields()["prompt_sha256"]
                .as_str()
                .expect("prompt_sha256")
                .to_owned(),
        ));
        Ok(())
    }
}

impl Sink {
    fn events(&self) -> Vec<(String, String, String)> {
        self.0.lock().expect("lock").clone()
    }
}

/// A judge that wraps a mock to count `judge` calls, run a hook inside
/// `judge`, and refuse any request carrying session context.
struct Probe {
    inner: MockJudgeBackend,
    judged: AtomicUsize,
    refuse_context: bool,
    during_judge: Option<Box<dyn Fn() + Send + Sync>>,
}

impl Probe {
    fn new(inner: MockJudgeBackend) -> Self {
        Self {
            inner,
            judged: AtomicUsize::new(0),
            refuse_context: false,
            during_judge: None,
        }
    }
}

impl JudgeBackend for Probe {
    fn id(&self) -> BackendId {
        self.inner.id()
    }

    fn validate_for_send(&self, request: &JudgeRequest) -> Result<(), JevError> {
        let chained = request
            .llm()
            .is_some_and(|llm| llm.system_prompt.contains(SESSION_CONTEXT_HEADER));
        if self.refuse_context && chained {
            return Err(JevError::input_too_large(10, 20));
        }
        self.inner.validate_for_send(request)
    }

    fn judge(&self, request: &JudgeRequest, _trace: &SendTrace) -> Result<JudgeOutcome, JevError> {
        self.judged.fetch_add(1, Ordering::SeqCst);
        if let Some(hook) = &self.during_judge {
            hook();
        }
        self.inner.judge(request, &SendTrace::new())
    }
}

fn config(repo: &Path, sessions: &Path) -> RunConfig {
    RunConfig::new(
        RepoRoot::new(repo),
        Arc::new(sc_agent_types::observability::v2::NoopSessionLog),
    )
    .sessions_dir(sessions)
}

fn run_with(
    config: RunConfig,
    ids: &[BackendId],
    judges: Vec<Arc<dyn JudgeBackend>>,
) -> Result<RunResponse, JevError> {
    let source = CheckSource::CheckFile(
        sc_agent_types::RepoRelativePath::try_from("check.toml").expect("path"),
    );
    run(prepare(
        source,
        config.backends(set(ids)).registry(registry(judges)),
    )?)
}

fn dirs() -> (TempDir, TempDir) {
    (bare_repo(), tempfile::tempdir().expect("sessions"))
}

fn dyn_judge(judge: &Arc<MockJudgeBackend>) -> Arc<dyn JudgeBackend> {
    Arc::clone(judge) as Arc<dyn JudgeBackend>
}

const JEV_HAIKU: [BackendId; 2] = [BackendId::Jev, BackendId::AnthropicHaiku];

#[test]
fn test_partial_failure_keeps_answers() {
    let (repo, sessions) = dirs();
    let jev = Arc::new(mock(BackendId::Jev, "yes").with_failure(busy()));
    let haiku = Arc::new(mock(BackendId::AnthropicHaiku, "no"));
    let response = run_with(
        config(repo.path(), sessions.path()),
        &JEV_HAIKU,
        vec![dyn_judge(&jev), dyn_judge(&haiku)],
    )
    .expect("a partial run is Ok");
    assert!(response.is_partial());
    assert_eq!(response.results().len(), 2);
    match response.results()[0].status() {
        BackendStatus::Failed(failure) => assert_eq!(failure.error().code(), "PROVIDER_BUSY"),
        _ => panic!("jev should have failed"),
    }
    assert!(matches!(
        response.results()[1].status(),
        BackendStatus::Ok(_)
    ));
    assert_eq!(
        response.primary().expect("primary").backend(),
        BackendId::AnthropicHaiku
    );
    let attestations: Vec<&Attestation> = response
        .equal_input()
        .backends()
        .iter()
        .map(|entry| entry.attestation())
        .collect();
    assert_eq!(attestations, [&Attestation::Failed, &Attestation::Matched]);
    assert!(!response.equal_input().equal());
}

/// ADR-SAG-0025 as amended (agt-2a3): an all-failed run returns the response,
/// so every backend's own typed error survives, each naming its backend.
#[test]
fn test_all_failed_keeps_every_backends_error() {
    let (repo, sessions) = dirs();
    let jev = Arc::new(mock(BackendId::Jev, "yes").with_failure(busy()));
    let haiku = Arc::new(
        mock(BackendId::AnthropicHaiku, "no").with_failure(JevError::response_invalid("later")),
    );
    let response = run_with(
        config(repo.path(), sessions.path()),
        &JEV_HAIKU,
        vec![dyn_judge(&jev), dyn_judge(&haiku)],
    )
    .expect("an all-failed run still returns its response");
    assert!(response.is_partial());
    assert!(response.primary().is_none());
    assert!(!response.session_saved());
    assert_eq!(haiku.requests().len(), 1, "a later backend is still judged");
    let codes: Vec<(String, String)> = response
        .results()
        .iter()
        .map(|result| match result.status() {
            BackendStatus::Failed(failure) => (
                failure.error().backend().unwrap_or_default().to_owned(),
                failure.error().code().to_string(),
            ),
            _ => panic!("{result:?} should have failed"),
        })
        .collect();
    assert_eq!(
        codes,
        [
            ("jev".to_owned(), "PROVIDER_BUSY".to_owned()),
            ("anthropic_haiku".to_owned(), "RESPONSE_INVALID".to_owned())
        ]
    );
    assert_eq!(
        response.first_failure().expect("first").error().code(),
        "PROVIDER_BUSY"
    );
}

#[test]
fn test_leak_true_iff_any_failed() {
    let (repo, sessions) = dirs();
    let ok = run_with(
        config(repo.path(), sessions.path()),
        &JEV_HAIKU,
        vec![
            dyn_judge(&Arc::new(mock(BackendId::Jev, "yes"))),
            dyn_judge(&Arc::new(mock(BackendId::AnthropicHaiku, "no"))),
        ],
    )
    .expect("run");
    assert!(!ok.equal_input().leak());
    assert!(!ok.is_partial());
    let partial = run_with(
        config(repo.path(), sessions.path()),
        &JEV_HAIKU,
        vec![
            dyn_judge(&Arc::new(mock(BackendId::Jev, "yes"))),
            dyn_judge(&Arc::new(
                mock(BackendId::AnthropicHaiku, "no").with_failure(busy()),
            )),
        ],
    )
    .expect("run");
    assert!(partial.equal_input().leak());
}

#[test]
fn test_independent_default_no_prior_answers_in_prompt() {
    let (repo, sessions) = dirs();
    let jev = Arc::new(mock(BackendId::Jev, "yes"));
    let haiku = Arc::new(mock(BackendId::AnthropicHaiku, "no"));
    let response = run_with(
        config(repo.path(), sessions.path()),
        &JEV_HAIKU,
        vec![dyn_judge(&jev), dyn_judge(&haiku)],
    )
    .expect("run");
    let seen = haiku.requests();
    let prompt = &seen[0].llm().expect("prompt").system_prompt;
    assert_eq!(prompt, PACKAGED_SYSTEM_PROMPT);
    assert!(!prompt.contains(SESSION_CONTEXT_HEADER));
    assert!(jev.requests()[0].llm().is_none());
    assert_eq!(response.equal_input().mode(), MultiBackendMode::Independent);
    assert_eq!(haiku.validations(), 1, "no re-validation without context");
}

#[test]
fn test_chain_context_recorded_and_applied() {
    let (repo, sessions) = dirs();
    let jev = Arc::new(mock(BackendId::Jev, "yes"));
    let haiku = Arc::new(mock(BackendId::AnthropicHaiku, "no"));
    let response = run_with(
        config(repo.path(), sessions.path()).mode(MultiBackendMode::ChainContext),
        &JEV_HAIKU,
        vec![dyn_judge(&jev), dyn_judge(&haiku)],
    )
    .expect("run");
    let seen = haiku.requests();
    let prompt = &seen[0].llm().expect("prompt").system_prompt;
    assert!(prompt.starts_with(PACKAGED_SYSTEM_PROMPT));
    assert!(prompt.contains(SESSION_CONTEXT_HEADER));
    assert!(prompt.contains("\"choice\":\"yes\""));
    assert!(
        jev.requests()[0].llm().is_none(),
        "Jev never receives context"
    );
    assert_eq!(
        seen[0].payload().bytes(),
        jev.requests()[0].payload().bytes()
    );
    assert_eq!(
        response.equal_input().mode(),
        MultiBackendMode::ChainContext
    );
}

#[test]
fn test_chain_context_augmented_request_revalidated() {
    let (repo, sessions) = dirs();
    let sink = Arc::new(Sink::default());
    let jev = Arc::new(mock(BackendId::Jev, "yes"));
    let mut probe = Probe::new(mock(BackendId::AnthropicHaiku, "no"));
    probe.refuse_context = true;
    let haiku = Arc::new(probe);
    let log: SharedSessionLog = sink.clone();
    let source = CheckSource::CheckFile(
        sc_agent_types::RepoRelativePath::try_from("check.toml").expect("path"),
    );
    let config = RunConfig::new(RepoRoot::new(repo.path()), log)
        .sessions_dir(sessions.path())
        .mode(MultiBackendMode::ChainContext)
        .backends(set(&JEV_HAIKU))
        .registry(registry(vec![
            dyn_judge(&jev),
            Arc::clone(&haiku) as Arc<dyn JudgeBackend>,
        ]));
    let response = run(prepare(source, config).expect("prepare")).expect("run");
    assert_eq!(haiku.judged.load(Ordering::SeqCst), 0, "nothing is sent");
    match response.results()[1].status() {
        BackendStatus::Failed(failure) => assert_eq!(failure.error().code(), "INPUT_TOO_LARGE"),
        _ => panic!("the over-cap chained request must be refused"),
    }
    assert!(response.equal_input().leak());
    assert_eq!(
        response.equal_input().backends()[1].attestation(),
        &Attestation::NotSent
    );
    assert_eq!(
        sink.events().len(),
        1,
        "a refused backend emits no log event"
    );
}

#[test]
fn test_no_resend_on_timeout() {
    let (repo, sessions) = dirs();
    let jev = Arc::new(
        mock(BackendId::Jev, "yes")
            .with_failure(JevError::network(NetworkErrorKind::Timeout, "slow")),
    );
    let haiku = Arc::new(mock(BackendId::AnthropicHaiku, "no"));
    let response = run_with(
        config(repo.path(), sessions.path()),
        &JEV_HAIKU,
        vec![dyn_judge(&jev), dyn_judge(&haiku)],
    )
    .expect("run");
    assert_eq!(jev.requests().len(), 1, "judge is called once per backend");
    assert!(response.is_partial());
}

/// Removes `dir` while a backend judges, so the end-of-run save fails.
fn removing_sessions(dir: &Path) -> Arc<Probe> {
    let dir = dir.to_path_buf();
    let mut probe = Probe::new(mock(BackendId::AnthropicHaiku, "no"));
    probe.during_judge = Some(Box::new(move || {
        let _ = fs::remove_dir_all(&dir);
    }));
    Arc::new(probe)
}

#[test]
fn test_session_save_failure_keeps_answers() {
    let (repo, sessions) = dirs();
    let dir = sessions.path().join("sessions");
    let haiku = removing_sessions(&dir);
    let response = run_with(
        config(repo.path(), &dir),
        &[BackendId::AnthropicHaiku],
        vec![Arc::clone(&haiku) as Arc<dyn JudgeBackend>],
    )
    .expect("a save failure keeps the answers");
    assert!(!response.session_saved());
    assert!(response.session_id().is_none());
    assert_eq!(response.primary().expect("answer kept").answers().len(), 1);
    let pinned: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/run-response/session_save_failed.json"),
        )
        .expect("fixture"),
    )
    .expect("fixture json");
    assert_eq!(
        response.warnings(),
        [pinned["warnings"][0].as_str().expect("warning")]
    );
    let body = serde_json::to_value(&response).expect("json");
    assert_eq!(body["session_id"], serde_json::Value::Null);
    assert_eq!(body["session_saved"], false);
    let warning = &response.warnings()[0];
    assert!(
        !warning.contains(dir.to_str().expect("utf-8")),
        "no path: {warning}"
    );
}

#[test]
fn test_equal_false_when_backend_digest_differs() {
    let (repo, sessions) = dirs();
    let other = Sha256Digest::of_text("another payload");
    let jev = Arc::new(mock(BackendId::Jev, "yes"));
    let haiku = Arc::new(mock(BackendId::AnthropicHaiku, "no").with_reported_digest(other.clone()));
    let response = run_with(
        config(repo.path(), sessions.path()),
        &JEV_HAIKU,
        vec![dyn_judge(&jev), dyn_judge(&haiku)],
    )
    .expect("run");
    let record = response.equal_input();
    assert!(!record.equal());
    assert_eq!(record.backends()[0].attestation(), &Attestation::Matched);
    assert_eq!(
        record.backends()[1].attestation(),
        &Attestation::Mismatched { reported: other }
    );
}

#[test]
fn test_equal_true_when_all_digests_match() {
    let (repo, sessions) = dirs();
    let response = run_with(
        config(repo.path(), sessions.path()),
        &JEV_HAIKU,
        vec![
            dyn_judge(&Arc::new(mock(BackendId::Jev, "yes"))),
            dyn_judge(&Arc::new(mock(BackendId::AnthropicHaiku, "no"))),
        ],
    )
    .expect("run");
    let record = response.equal_input();
    assert!(record.equal());
    assert!(
        record
            .backends()
            .iter()
            .all(|entry| entry.attestation() == &Attestation::Matched)
    );
    assert_eq!(response.prompt_sha256(), record.prompt_sha256());
}

#[test]
fn test_failed_backend_emits_one_log_event() {
    let (repo, sessions) = dirs();
    let sink = Arc::new(Sink::default());
    let log: SharedSessionLog = sink.clone();
    let source = CheckSource::CheckFile(
        sc_agent_types::RepoRelativePath::try_from("check.toml").expect("path"),
    );
    let jev = Arc::new(mock(BackendId::Jev, "yes").with_failure(busy()));
    let haiku = Arc::new(mock(BackendId::AnthropicHaiku, "no"));
    let config = RunConfig::new(RepoRoot::new(repo.path()), log)
        .sessions_dir(sessions.path())
        .backends(set(&JEV_HAIKU))
        .registry(registry(vec![dyn_judge(&jev), dyn_judge(&haiku)]));
    let prepared = prepare(source, config).expect("prepare");
    let digest = prepared.prompt_sha256().as_str().to_owned();
    run(prepared).expect("run");
    let events = sink.events();
    assert_eq!(events.len(), 2, "the failed backend logs too");
    assert_eq!(
        events[0],
        ("jev".to_owned(), "judge".to_owned(), digest.clone())
    );
    assert_eq!(
        events[1],
        ("anthropic_haiku".to_owned(), "judge".to_owned(), digest)
    );
}

#[test]
fn test_session_save_failure_log_events_unchanged() {
    let (repo, sessions) = dirs();
    let dir = sessions.path().join("sessions");
    let sink = Arc::new(Sink::default());
    let log: SharedSessionLog = sink.clone();
    let source = CheckSource::CheckFile(
        sc_agent_types::RepoRelativePath::try_from("check.toml").expect("path"),
    );
    let haiku = removing_sessions(&dir);
    let config = RunConfig::new(RepoRoot::new(repo.path()), log)
        .sessions_dir(&dir)
        .backends(set(&[BackendId::AnthropicHaiku]))
        .registry(registry(vec![haiku as Arc<dyn JudgeBackend>]));
    let response = run(prepare(source, config).expect("prepare")).expect("run");
    assert!(!response.session_saved());
    let events = sink.events();
    assert_eq!(events.len(), 1, "the save failure adds no event of its own");
    let rendered = format!("{events:?}");
    assert!(
        !rendered.contains(dir.to_str().expect("utf-8")),
        "{rendered}"
    );
}
