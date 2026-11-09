//! Session-log rows from the engine's one send step (REQ-SAG-0610,
//! REQ-SAG-0611, REQ-SAG-0613, REQ-SAG-0616, REQ-SAG-0617, ADR-SAG-0031).
//!
//! Every test drives `prepare` then `run` (or `ask`) over `MockJudgeBackend`s
//! and reads the rows a recording `SessionLog` received. Needs
//! `--features test-util`.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use sc_agent_types::observability::v2::{LogEvent, LogRejected, SessionLog, SharedSessionLog};
use sc_agent_types::{
    BackendId, BackendSet, BusyCause, JevError, ModelId, Operation, ProjectLabel, RepoRoot,
    SendTrace, SessionId, Sha256Digest,
};
use sc_decision_schema::session_log::{SendOutcome, SessionLogRow};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_decision_schema::{
    DecisionSchemaId, JudgeBackend, JudgeOutcome, JudgeRequest, MultiBackendMode,
};
use sc_jev_check::registry::{BackendRegistry, register_scripted_backends};
use sc_jev_check::run::{
    CheckSource, LogContext, RunConfig, RunResponse, ask, discover_main_repo_root, prepare, run,
};
use sc_jev_check::session::FileSessionStore;
use tempfile::TempDir;

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

/// A prompt body that must never reach a row.
const PROMPT_SENTINEL: &str = "SENTINEL-prompt-text-sk-test-DO_NOT_LOG_7c1e";

/// Records every event; optionally rejects each one after recording it.
#[derive(Default)]
struct Sink {
    events: Mutex<Vec<LogEvent>>,
    reject: bool,
}

impl SessionLog for Sink {
    fn emit(&self, event: LogEvent) -> Result<(), LogRejected> {
        self.events.lock().expect("lock").push(event);
        if self.reject {
            Err(LogRejected::new("queue full"))
        } else {
            Ok(())
        }
    }
}

impl Sink {
    fn events(&self) -> Vec<LogEvent> {
        self.events.lock().expect("lock").clone()
    }

    /// Every event's `fields`, parsed back as a session-log row.
    fn rows(&self) -> Vec<SessionLogRow> {
        self.events()
            .iter()
            .map(|event| SessionLogRow::try_from_fields(event.fields()).expect("a session log row"))
            .collect()
    }
}

fn repo() -> TempDir {
    let dir = tempfile::tempdir().expect("temp");
    fs::write(dir.path().join("questions.json"), QUESTIONS).expect("questions");
    fs::write(
        dir.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n",
    )
    .expect("check");
    dir
}

fn mock(id: BackendId, choice: &str) -> MockJudgeBackend {
    let answers = serde_json::from_value(serde_json::json!({"relevant": {"choice": choice}}))
        .expect("answers");
    MockJudgeBackend::new(id, answers)
}

fn busy() -> JevError {
    JevError::provider_busy("scripted busy", 429, 4, BusyCause::Sends, None)
}

fn registry(judges: Vec<Arc<dyn JudgeBackend>>) -> Arc<BackendRegistry> {
    let mut registry = BackendRegistry::new();
    for judge in judges {
        registry.register(judge).expect("register");
    }
    Arc::new(registry)
}

fn dyn_judge(judge: MockJudgeBackend) -> Arc<dyn JudgeBackend> {
    Arc::new(judge)
}

fn set(ids: &[BackendId]) -> BackendSet {
    BackendSet::try_from(ids.to_vec()).expect("set")
}

fn config(repo: &Path, sessions: &Path, sink: &Arc<Sink>) -> RunConfig {
    let log: SharedSessionLog = Arc::clone(sink) as SharedSessionLog;
    RunConfig::new(RepoRoot::new(repo), log).sessions_dir(sessions)
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

fn digest_of(body: &str) -> Sha256Digest {
    Sha256Digest::of_text(body)
}

/// The scripted registry's pretend body for `id` (see `scripted.rs`).
fn scripted_body(id: BackendId) -> String {
    format!("{{\"scripted\":\"{id}\"}}")
}

#[test]
fn test_ok_row_request_sha256_is_sent_body_digest() {
    let repo = repo();
    let sessions = tempfile::tempdir().expect("sessions");
    let sink = Arc::new(Sink::default());
    let body = "{\"wire\":\"body\"}";
    let judge = dyn_judge(mock(BackendId::Jev, "yes").with_sent_body(body));
    let response = run_with(
        config(repo.path(), sessions.path(), &sink),
        &[BackendId::Jev],
        vec![judge],
    )
    .expect("run");
    let rows = sink.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].send_outcome(), SendOutcome::Ok);
    assert_eq!(rows[0].context().request_sha256, Some(digest_of(body)));
    assert_eq!(&rows[0].context().prompt_sha256, response.prompt_sha256());
    assert_eq!(rows[0].context().decision_schema, DecisionSchemaId::CURRENT);
}

#[test]
fn test_failed_before_body_row_has_null_request_sha256() {
    let repo = repo();
    let sessions = tempfile::tempdir().expect("sessions");
    let sink = Arc::new(Sink::default());
    // No `with_sent_body`: the backend failed before it built a body.
    let judge = dyn_judge(
        mock(BackendId::Jev, "yes").with_failure(JevError::key_missing(
            sc_agent_types::BackendKeyId::TypeSafe,
        )),
    );
    run_with(
        config(repo.path(), sessions.path(), &sink),
        &[BackendId::Jev],
        vec![judge],
    )
    .expect("an all-failed run still returns its response");
    let rows = sink.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].send_outcome(), SendOutcome::Failed);
    assert_eq!(
        rows[0].error_code().map(|code| code.to_string()).as_deref(),
        Some("KEY_MISSING")
    );
    assert!(rows[0].context().request_sha256.is_none());
    assert!(sink.events()[0].fields()["request_sha256"].is_null());
}

#[test]
fn test_multi_backend_rows_carry_own_request_sha256() {
    let repo = repo();
    let sessions = tempfile::tempdir().expect("sessions");
    let sink = Arc::new(Sink::default());
    let ids = [BackendId::Jev, BackendId::AnthropicHaiku];
    let scripted =
        register_scripted_backends(&ids, serde_json::json!({"relevant": "yes"})).expect("scripted");
    let source = CheckSource::CheckFile(
        sc_agent_types::RepoRelativePath::try_from("check.toml").expect("path"),
    );
    run(prepare(
        source,
        config(repo.path(), sessions.path(), &sink)
            .backends(set(&ids))
            .registry(Arc::new(scripted)),
    )
    .expect("prepare"))
    .expect("run");
    let rows = sink.rows();
    assert_eq!(rows.len(), 2);
    let digests: Vec<_> = rows
        .iter()
        .map(|row| row.context().request_sha256.clone())
        .collect();
    assert_eq!(digests[0], Some(digest_of(&scripted_body(BackendId::Jev))));
    assert_eq!(
        digests[1],
        Some(digest_of(&scripted_body(BackendId::AnthropicHaiku)))
    );
    assert_ne!(digests[0], digests[1], "each backend has its own trace");
    assert_eq!(rows[0].context().backend, BackendId::Jev);
    assert_eq!(rows[1].context().backend, BackendId::AnthropicHaiku);
}

#[test]
fn test_failed_row_carries_model_id() {
    let repo = repo();
    let sessions = tempfile::tempdir().expect("sessions");
    let sink = Arc::new(Sink::default());
    let model: ModelId = "claude-haiku-5-5".parse().expect("model");
    let haiku = mock(BackendId::AnthropicHaiku, "yes")
        .with_model_id(model.clone())
        .with_failure(busy());
    let expected = haiku.model_id();
    let jev = mock(BackendId::Jev, "yes");
    let jev_model = jev.model_id();
    run_with(
        config(repo.path(), sessions.path(), &sink),
        &[BackendId::Jev, BackendId::AnthropicHaiku],
        vec![dyn_judge(jev), dyn_judge(haiku)],
    )
    .expect("partial run");
    let rows = sink.rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].context().model_id, jev_model);
    assert_eq!(rows[1].send_outcome(), SendOutcome::Failed);
    assert_eq!(rows[1].context().model_id, expected);
    assert_eq!(rows[1].context().model_id, model);
    assert_ne!(
        sink.events()[1].fields()["backend"],
        sink.events()[1].fields()["model_id"]
    );
}

/// A judge that refuses any request carrying chained session context, so the
/// engine's re-validation turns it into `NotSent`.
struct RefusesContext(MockJudgeBackend);

impl JudgeBackend for RefusesContext {
    fn id(&self) -> BackendId {
        self.0.id()
    }

    fn model_id(&self) -> ModelId {
        self.0.model_id()
    }

    fn validate_for_send(&self, request: &JudgeRequest) -> Result<(), JevError> {
        let chained = request
            .llm()
            .is_some_and(|llm| llm.system_prompt.contains("Session context:"));
        if chained {
            return Err(JevError::input_too_large(10, 20));
        }
        self.0.validate_for_send(request)
    }

    fn judge(&self, request: &JudgeRequest, trace: &SendTrace) -> Result<JudgeOutcome, JevError> {
        self.0.judge(request, trace)
    }
}

/// `Ok` and `Failed` sends write one row each; a backend refused before send
/// (`NotSent`) writes none (REQ-SAG-0610, REQ-SAG-0617).
#[test]
fn test_one_row_per_send_failed_included_not_sent_none() {
    let repo = repo();
    let sessions = tempfile::tempdir().expect("sessions");
    let sink = Arc::new(Sink::default());
    let ids = [
        BackendId::Jev,
        BackendId::AnthropicHaiku,
        BackendId::OpenAiLuna,
    ];
    let judges: Vec<Arc<dyn JudgeBackend>> = vec![
        dyn_judge(mock(BackendId::Jev, "yes").with_sent_body("{}")),
        Arc::new(RefusesContext(mock(BackendId::AnthropicHaiku, "no"))),
        dyn_judge(
            mock(BackendId::OpenAiLuna, "no")
                .with_sent_body("{}")
                .with_failure(busy()),
        ),
    ];
    let response = run_with(
        config(repo.path(), sessions.path(), &sink).mode(MultiBackendMode::ChainContext),
        &ids,
        judges,
    )
    .expect("partial run");
    assert!(response.is_partial());
    let rows = sink.rows();
    assert_eq!(
        rows.iter()
            .map(|row| (row.context().backend, row.send_outcome()))
            .collect::<Vec<_>>(),
        [
            (BackendId::Jev, SendOutcome::Ok),
            (BackendId::OpenAiLuna, SendOutcome::Failed),
        ],
        "Haiku was NotSent and has no row"
    );
    assert!(
        rows.iter()
            .all(|row| row.context().operation == Operation::Judge)
    );
    // One session id, one fresh answer id per row.
    assert_eq!(rows[0].context().session_id, rows[1].context().session_id);
    assert_ne!(rows[0].context().answer_id, rows[1].context().answer_id);
    assert_eq!(
        rows[1].error_code().map(|code| code.to_string()).as_deref(),
        Some("PROVIDER_BUSY")
    );

    // A base-request refusal fails the run before any send: no row at all.
    let sink = Arc::new(Sink::default());
    let error = run_with(
        config(repo.path(), sessions.path(), &sink),
        &[BackendId::Jev],
        vec![dyn_judge(MockJudgeBackend::refusing(
            BackendId::Jev,
            JevError::input_too_large(10, 20),
        ))],
    )
    .expect_err("refused before the first send");
    assert_eq!(error.code(), "INPUT_TOO_LARGE");
    assert!(sink.events().is_empty());
}

#[test]
fn test_ask_writes_one_row_in_the_sessions_context() {
    let repo = repo();
    let sessions = tempfile::tempdir().expect("sessions");
    let sink = Arc::new(Sink::default());
    let haiku = Arc::new(mock(BackendId::AnthropicHaiku, "yes").with_sent_body("{\"ask\":1}"));
    let judge: Arc<dyn JudgeBackend> = Arc::clone(&haiku) as _;
    let response = run_with(
        config(repo.path(), sessions.path(), &sink),
        &[BackendId::AnthropicHaiku],
        vec![Arc::clone(&judge)],
    )
    .expect("run");
    let id = response.session_id().expect("saved");
    let store = FileSessionStore::new(sessions.path()).expect("store");
    let project = ProjectLabel::new("later-call").expect("label");
    let asked = ask(
        &store,
        id,
        "why?",
        config(repo.path(), sessions.path(), &sink)
            .registry(registry(vec![judge]))
            .log_context(LogContext::new(RepoRoot::new(repo.path()), Some(project))),
    )
    .expect("ask");
    assert_eq!(asked.prompt_sha256(), response.prompt_sha256());
    let rows = sink.rows();
    assert_eq!(rows.len(), 2);
    let (judged, followed) = (rows[0].context(), rows[1].context());
    assert_eq!(judged.operation, Operation::Judge);
    assert_eq!(followed.operation, Operation::Ask);
    assert_eq!(judged.session_id, id);
    assert_eq!(followed.session_id, id);
    assert_eq!(judged.session_created_at, followed.session_created_at);
    assert_eq!(
        followed.project.as_ref().map(ProjectLabel::as_str),
        Some("later-call")
    );
    assert!(judged.project.is_none(), "project is per call");
    assert_eq!(followed.request_sha256, Some(digest_of("{\"ask\":1}")));
}

#[test]
fn test_every_row_carries_context() {
    let repo = repo();
    let sessions = tempfile::tempdir().expect("sessions");
    let sink = Arc::new(Sink::default());
    let context = LogContext::new(
        RepoRoot::new("/logged/main-repo"),
        Some(ProjectLabel::new("billing").expect("label")),
    );
    let haiku = Arc::new(mock(BackendId::AnthropicHaiku, "yes"));
    let jev = mock(BackendId::Jev, "no").with_failure(busy());
    let judge: Arc<dyn JudgeBackend> = Arc::clone(&haiku) as _;
    let response = run_with(
        config(repo.path(), sessions.path(), &sink).log_context(context.clone()),
        &[BackendId::Jev, BackendId::AnthropicHaiku],
        vec![dyn_judge(jev), Arc::clone(&judge)],
    )
    .expect("partial run");
    let id = response.session_id().expect("saved");
    let store = FileSessionStore::new(sessions.path()).expect("store");
    ask(
        &store,
        id,
        "why?",
        config(repo.path(), sessions.path(), &sink)
            .registry(registry(vec![judge]))
            .log_context(context),
    )
    .expect("ask");

    let rows = sink.rows();
    assert_eq!(rows.len(), 3);
    let kinds: Vec<_> = rows
        .iter()
        .map(|row| (row.context().operation, row.send_outcome()))
        .collect();
    assert_eq!(
        kinds,
        [
            (Operation::Judge, SendOutcome::Failed),
            (Operation::Judge, SendOutcome::Ok),
            (Operation::Ask, SendOutcome::Ok),
        ]
    );
    for (event, row) in sink.events().iter().zip(&rows) {
        let fields = event.fields();
        assert_eq!(fields["session_id"], id.to_string());
        assert_eq!(fields["repo"], "/logged/main-repo");
        assert_eq!(fields["project"], "billing");
        assert!(fields["model_id"].is_string());
        assert_eq!(fields["prompt_sha256"], response.prompt_sha256().as_str());
        assert_eq!(event.session_id(), &id);
        assert_eq!(event.operation(), row.context().operation);
    }
}

#[test]
fn test_unset_log_context_defaults_to_main_repo_and_null_project() {
    let repo = repo();
    let sessions = tempfile::tempdir().expect("sessions");
    let sink = Arc::new(Sink::default());
    run_with(
        config(repo.path(), sessions.path(), &sink),
        &[BackendId::Jev],
        vec![dyn_judge(mock(BackendId::Jev, "yes"))],
    )
    .expect("run");
    let rows = sink.rows();
    let expected = discover_main_repo_root(repo.path()).expect("root");
    assert_eq!(rows[0].context().repo, expected.as_path().to_string_lossy());
    assert!(rows[0].context().project.is_none());
}

#[test]
fn test_row_has_no_body_prompt_or_key_bytes() {
    let repo = repo();
    fs::write(repo.path().join("override.md"), PROMPT_SENTINEL).expect("prompt");
    let sessions = tempfile::tempdir().expect("sessions");
    let sink = Arc::new(Sink::default());
    let body = "{\"messages\":[\"BODY-SENTINEL\"],\"x-api-key\":\"sk-test-DO_NOT_LOG\"}";
    let response = run_with(
        config(repo.path(), sessions.path(), &sink)
            .system_prompt_override(repo.path().join("override.md")),
        &[BackendId::Jev, BackendId::AnthropicHaiku],
        vec![
            dyn_judge(mock(BackendId::Jev, "yes").with_sent_body(body)),
            dyn_judge(mock(BackendId::AnthropicHaiku, "no").with_sent_body(body)),
        ],
    )
    .expect("run");
    assert!(response.session_id().is_some());
    let text = sink
        .events()
        .iter()
        .map(|event| serde_json::Value::Object(event.fields().clone()).to_string())
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        PROMPT_SENTINEL,
        "BODY-SENTINEL",
        "sk-test-DO_NOT_LOG",
        "Does it apply?",
        "authorization",
        "Bearer",
    ] {
        assert!(!text.contains(forbidden), "{forbidden} in {text}");
    }
    // The digest of the body is there; the body is not.
    assert!(text.contains(digest_of(body).as_str()));
}

#[test]
fn test_rejected_log_row_keeps_run_ok() {
    let repo = repo();
    let sessions = tempfile::tempdir().expect("sessions");
    let sink = Arc::new(Sink {
        reject: true,
        ..Sink::default()
    });
    let response = run_with(
        config(repo.path(), sessions.path(), &sink),
        &[BackendId::AnthropicHaiku],
        vec![dyn_judge(mock(BackendId::AnthropicHaiku, "yes"))],
    )
    .expect("a rejected row never fails the run");
    assert!(response.session_id().is_some());
    assert!(response.warnings().is_empty());
    assert_eq!(sink.events().len(), 1, "the row was offered once");
    let store = FileSessionStore::new(sessions.path()).expect("store");
    let id: SessionId = response.session_id().expect("saved");
    let asked = ask(
        &store,
        id,
        "again?",
        config(repo.path(), sessions.path(), &sink).registry(registry(vec![dyn_judge(mock(
            BackendId::AnthropicHaiku,
            "no",
        ))])),
    );
    assert!(asked.is_ok(), "ask also survives a rejected row");
}
