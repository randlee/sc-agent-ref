//! Engine unit tests (REQ-SAG-0601, REQ-SAG-0610). Multi-backend payload
//! sharing, equal-input, and the packaged prompt default live in
//! `tests/sessions_exact.rs`; the response contract in
//! `tests/run_response.rs`.

use super::*;
use crate::config::PACKAGED_SYSTEM_PROMPT;
use crate::extract::test_support::{self, copy_fixture};
use sc_agent_types::NetworkErrorKind;
use sc_agent_types::observability::v2::{LogEvent, LogRejected, NoopSessionLog, SessionLog};
use sc_decision_schema::test_util::MockJudgeBackend;
use std::path::PathBuf;

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

struct JsonlSink {
    path: PathBuf,
}

impl SessionLog for JsonlSink {
    fn emit(&self, event: LogEvent) -> Result<(), LogRejected> {
        use std::io::Write;
        let line = serde_json::json!({
            "session_id": event.session_id().to_string(),
            "backend": event.fields()["backend"],
            "payload_sha256": event.fields()["payload_sha256"],
            "operation": event.operation().as_str(),
        });
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .expect("jsonl");
        writeln!(file, "{line}").expect("write jsonl");
        Ok(())
    }
}

fn bare_repo() -> tempfile::TempDir {
    let dir = test_support::temp_dir("run");
    std::fs::write(dir.path().join("questions.json"), QUESTIONS).expect("questions");
    std::fs::write(
        dir.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n",
    )
    .expect("check");
    dir
}

fn judge(id: BackendId, choice: &str) -> Arc<MockJudgeBackend> {
    let answers = serde_json::from_value(serde_json::json!({"relevant": {"choice": choice}}))
        .expect("answers");
    Arc::new(MockJudgeBackend::new(id, answers))
}

fn registry_of(judges: &[&Arc<MockJudgeBackend>]) -> Arc<BackendRegistry> {
    let mut registry = BackendRegistry::new();
    for judge in judges {
        registry
            .register(Arc::clone(*judge) as Arc<dyn JudgeBackend>)
            .expect("register");
    }
    Arc::new(registry)
}

fn set(ids: &[BackendId]) -> BackendSet {
    BackendSet::try_from(ids.to_vec()).expect("backend set")
}

fn config(repo: &Path, sessions: &Path) -> RunConfig {
    RunConfig::new(RepoRoot::new(repo), Arc::new(NoopSessionLog)).sessions_dir(sessions)
}

fn check_file() -> CheckSource {
    CheckSource::CheckFile(test_support::rel("check.toml"))
}

fn system_prompt(request: &JudgeRequest) -> &str {
    &request.llm().expect("llm prompt").system_prompt
}

#[test]
fn dry_run_sends_nothing_and_needs_no_registry_or_store() {
    let repo = copy_fixture();
    let sessions = test_support::temp_dir("dry-sessions");
    let sessions_dir = sessions.path().join("never");
    let jev = judge(BackendId::Jev, "yes");
    let prepared = prepare(
        check_file(),
        config(repo.path(), &sessions_dir)
            .dry_run(true)
            .backends(set(&[BackendId::Jev, BackendId::Haiku]))
            .registry(registry_of(&[&jev])),
    )
    .expect("prepare");
    let digest = prepared.payload_sha256().clone();
    let response = run(prepared).expect("dry-run");
    assert!(response.dry_run());
    assert!(response.session_id().is_none());
    assert!(!response.session_saved());
    assert!(response.results().is_empty());
    assert_eq!(response.payload_sha256(), &digest);
    assert!(
        response
            .equal_input()
            .backends()
            .iter()
            .all(|entry| entry.attestation() == &Attestation::NotSent)
    );
    assert!(!response.equal_input().equal());
    assert_eq!(response.limits().len(), 2);
    assert!(response.payload().expect("payload")["questions"].is_array());
    assert!(jev.requests().is_empty());
    assert_eq!(
        jev.validations(),
        0,
        "a dry run constructs and calls no backend"
    );
    assert!(
        !sessions_dir.exists(),
        "a dry run creates no session directory"
    );
}

#[test]
fn packaged_prompt_is_injected_and_override_does_not_change_digest() {
    let repo = bare_repo();
    let sessions = test_support::temp_dir("prompt-sessions");
    std::fs::write(repo.path().join("override.md"), "OVERRIDE-PROMPT-TOKEN").expect("override");
    let first_judge = judge(BackendId::Haiku, "yes");
    let first = run(prepare(
        check_file(),
        config(repo.path(), sessions.path())
            .backends(set(&[BackendId::Haiku]))
            .registry(registry_of(&[&first_judge])),
    )
    .expect("prepare"))
    .expect("packaged");
    assert_eq!(
        system_prompt(&first_judge.requests()[0]),
        PACKAGED_SYSTEM_PROMPT
    );
    let second_judge = judge(BackendId::Haiku, "yes");
    let second = run(prepare(
        check_file(),
        config(repo.path(), sessions.path())
            .backends(set(&[BackendId::Haiku]))
            .system_prompt_override(repo.path().join("override.md"))
            .registry(registry_of(&[&second_judge])),
    )
    .expect("prepare"))
    .expect("override");
    assert_eq!(
        system_prompt(&second_judge.requests()[0]),
        "OVERRIDE-PROMPT-TOKEN"
    );
    assert_eq!(first.payload_sha256(), second.payload_sha256());
}

#[test]
fn check_file_prompt_beats_packaged_asset() {
    let repo = copy_fixture();
    let sessions = test_support::temp_dir("check-prompt");
    let luna = judge(BackendId::Luna, "yes");
    run(prepare(
        check_file(),
        config(repo.path(), sessions.path())
            .backends(set(&[BackendId::Luna]))
            .registry(registry_of(&[&luna])),
    )
    .expect("prepare"))
    .expect("run");
    let requests = luna.requests();
    let prompt = system_prompt(&requests[0]);
    assert!(prompt.contains("PROMPT-TOKEN-a3-check"));
    assert!(!prompt.contains(PACKAGED_SYSTEM_PROMPT.trim()));
}

#[test]
fn missing_llm_prompt_fails_in_prepare_and_jev_ignores_it() {
    let repo = bare_repo();
    std::fs::write(
        repo.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n\n[llm_judge]\nsystem_prompt_file = \"missing.md\"\n",
    )
    .expect("check");
    let sessions = test_support::temp_dir("missing-prompt");
    let haiku = judge(BackendId::Haiku, "yes");
    let error = prepare(
        check_file(),
        config(repo.path(), sessions.path())
            .backends(set(&[BackendId::Haiku]))
            .registry(registry_of(&[&haiku])),
    )
    .expect_err("missing prompt");
    assert_eq!(error.code(), "PROMPT_FILE_MISSING");
    assert!(haiku.requests().is_empty());
    assert_eq!(std::fs::read_dir(sessions.path()).expect("dir").count(), 0);
    let jev = run(prepare(
        check_file(),
        config(repo.path(), sessions.path())
            .registry(registry_of(&[&judge(BackendId::Jev, "yes")])),
    )
    .expect("jev ignores the missing prompt"))
    .expect("run");
    assert!(jev.primary().expect("primary").answers()["relevant"].choice() == "yes");
}

#[test]
fn refusing_backend_does_not_log_or_save() {
    let repo = bare_repo();
    let sessions = test_support::temp_dir("refuse-sessions");
    let log_dir = test_support::temp_dir("jsonl");
    let log_path = log_dir.path().join("session.jsonl");
    let log = Arc::new(JsonlSink {
        path: log_path.clone(),
    });
    let haiku = judge(BackendId::Haiku, "yes");
    let mut registry = BackendRegistry::new();
    registry
        .register(Arc::new(MockJudgeBackend::refusing(
            BackendId::Jev,
            JevError::input_too_large(4, 8),
        )))
        .expect("register");
    registry
        .register(Arc::clone(&haiku) as Arc<dyn JudgeBackend>)
        .expect("register");
    let error = run(prepare(
        check_file(),
        RunConfig::new(RepoRoot::new(repo.path()), log)
            .sessions_dir(sessions.path())
            .backends(set(&[BackendId::Haiku, BackendId::Jev]))
            .registry(Arc::new(registry)),
    )
    .expect("prepare"))
    .expect_err("refused");
    assert_eq!(error.code(), "INPUT_TOO_LARGE");
    assert!(
        haiku.requests().is_empty(),
        "validation of every backend runs before the first send"
    );
    assert!(!log_path.exists());
    assert_eq!(std::fs::read_dir(sessions.path()).expect("dir").count(), 0);
}

#[test]
fn ask_capable_live_run_without_sessions_dir_is_config_invalid_before_send() {
    let repo = bare_repo();
    let haiku = judge(BackendId::Haiku, "yes");
    let error = run(prepare(
        check_file(),
        RunConfig::new(RepoRoot::new(repo.path()), Arc::new(NoopSessionLog))
            .backends(set(&[BackendId::Haiku]))
            .registry(registry_of(&[&haiku])),
    )
    .expect("prepare"))
    .expect_err("no sessions dir");
    assert_eq!(error.code(), "CONFIG_INVALID");
    assert!(
        error.suggested_action().contains("sessions_dir"),
        "{}",
        error.suggested_action()
    );
    assert!(!error.suggested_action().contains("check.toml"));
    assert_eq!(haiku.validations(), 0);
    // A Jev-only live run saves no session, so it needs no directory.
    let jev = run(prepare(
        check_file(),
        RunConfig::new(RepoRoot::new(repo.path()), Arc::new(NoopSessionLog))
            .registry(registry_of(&[&judge(BackendId::Jev, "yes")])),
    )
    .expect("prepare"))
    .expect("jev run");
    assert!(!jev.session_saved());
    assert!(jev.session_id().is_none());
}

#[test]
fn unregistered_backend_is_a_registry_hint() {
    let repo = bare_repo();
    let error = run(prepare(
        check_file(),
        RunConfig::new(RepoRoot::new(repo.path()), Arc::new(NoopSessionLog))
            .backends(set(&[BackendId::Jev]))
            .registry(Arc::new(BackendRegistry::new())),
    )
    .expect("prepare"))
    .expect_err("not registered");
    assert_eq!(error.code(), "CONFIG_INVALID");
    assert!(error.message().contains("jev"), "{}", error.message());
    assert!(!error.message().contains("check.toml"));
}

#[test]
fn command_input_copies_producer() {
    let repo = test_support::temp_dir("command-run");
    std::fs::write(repo.path().join("questions.json"), QUESTIONS).expect("questions");
    let subjects = std::fs::read_to_string(
        test_support::workspace_root().join("tests/fixtures/jev/subjects-schema/v1/minimal.json"),
    )
    .expect("fixture");
    std::fs::write(repo.path().join("subjects.json"), subjects).expect("subjects");
    let python = test_support::python();
    let check = format!(
        "[questions]\npath = \"questions.json\"\n\n[inputs.live]\nkind = \"command\"\ncommand = [\"{python}\", \"-c\", \"print(open('subjects.json').read())\"]\ntimeout_seconds = 30\n"
    );
    std::fs::write(repo.path().join("check.toml"), check).expect("check");
    let prepared = prepare(
        check_file(),
        RunConfig::new(RepoRoot::new(repo.path()), Arc::new(NoopSessionLog))
            .registry(registry_of(&[&judge(BackendId::Jev, "yes")])),
    )
    .expect("prepare");
    assert_eq!(
        prepared.producer().expect("producer").name(),
        "sc-treesitter"
    );
    let response = run(prepared).expect("run");
    let producer = response.equal_input().producer().expect("producer");
    assert_eq!(producer.name(), "sc-treesitter");
    assert_eq!(producer.language(), "python");
}

#[test]
fn caller_request_uses_file_override_not_inline_prompt() {
    let repo = test_support::temp_dir("caller-prompt");
    let override_path = repo.path().join("override.md");
    std::fs::write(&override_path, "OVERRIDE-PROMPT-TOKEN-a7\n").expect("override");
    let inline = br#"{"questions":[{"id":"q","prompt":"p","choices":["no","yes"]}],"system_prompt":"INLINE-PROMPT-TOKEN"}"#;
    let rejected = CallerRequest::from_json(inline).expect_err("inline");
    assert!(!rejected.message().contains("INLINE-PROMPT-TOKEN"));
    assert!(rejected.message().contains("--llm-system-prompt-file"));
    let repeated = br#"{"questions":[{"id":"q","prompt":"p","choices":["no","yes"]}],"backends":["jev","jev"]}"#;
    assert_eq!(
        CallerRequest::from_json(repeated)
            .expect_err("repeat")
            .code(),
        "INPUT_INVALID"
    );

    let bytes = br#"{"questions":[{"id":"q","prompt":"Judge this","choices":["no","yes"]}],"backends":["haiku"]}"#;
    let request = CallerRequest::from_json(bytes).expect("request");
    let base = RunConfig::new(RepoRoot::new(repo.path()), Arc::new(NoopSessionLog));
    let packaged =
        prepare(CheckSource::CallerRequest(request.clone()), base.clone()).expect("packaged");
    assert_eq!(packaged.backends().as_slice(), [BackendId::Haiku]);
    assert_eq!(system_prompt(packaged.request()), PACKAGED_SYSTEM_PROMPT);
    let jev_only = prepare(
        CheckSource::CallerRequest(request.clone()),
        base.clone()
            .backends(set(&[BackendId::Jev]))
            .system_prompt_override(&override_path),
    )
    .expect("config backends win");
    assert_eq!(jev_only.backends().as_slice(), [BackendId::Jev]);
    assert!(jev_only.request().llm().is_none());
    let llm = prepare(
        CheckSource::CallerRequest(request),
        base.system_prompt_override(&override_path),
    )
    .expect("override");
    assert_eq!(system_prompt(llm.request()), "OVERRIDE-PROMPT-TOKEN-a7\n");
    assert_eq!(packaged.payload_sha256(), llm.payload_sha256());
    let text = llm.request().payload().as_str();
    assert!(!text.contains("OVERRIDE-PROMPT-TOKEN-a7"));
    assert!(text.contains("Judge this"));
}

#[test]
fn validate_for_send_repeats_only_for_context_bearing_requests() {
    let repo = bare_repo();
    let sessions = test_support::temp_dir("validate-sessions");
    let jev = judge(BackendId::Jev, "yes");
    let haiku = judge(BackendId::Haiku, "no");
    run(prepare(
        check_file(),
        config(repo.path(), sessions.path())
            .mode(MultiBackendMode::ChainContext)
            .backends(set(&[BackendId::Jev, BackendId::Haiku]))
            .registry(registry_of(&[&jev, &haiku])),
    )
    .expect("prepare"))
    .expect("run");
    assert_eq!(jev.validations(), 1, "Jev: pre-flight only");
    assert_eq!(
        haiku.validations(),
        2,
        "Haiku: pre-flight, then again with session context"
    );
}

/// REQ-SAG-0610: one log line per judge send, under the run's session id.
#[test]
fn every_judge_send_logs_once_under_the_saved_session_id() {
    let repo = bare_repo();
    let sessions = test_support::temp_dir("log-sessions");
    let log_dir = test_support::temp_dir("log-jsonl");
    let log_path = log_dir.path().join("session.jsonl");
    let log = Arc::new(JsonlSink {
        path: log_path.clone(),
    });
    let response = run(prepare(
        check_file(),
        RunConfig::new(RepoRoot::new(repo.path()), log)
            .sessions_dir(sessions.path())
            .backends(set(&[BackendId::Jev, BackendId::Luna]))
            .registry(registry_of(&[
                &judge(BackendId::Jev, "yes"),
                &judge(BackendId::Luna, "no"),
            ])),
    )
    .expect("prepare"))
    .expect("run");
    let text = std::fs::read_to_string(&log_path).expect("jsonl");
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("line"))
        .collect();
    assert_eq!(lines.len(), 2);
    let session_id = response.session_id().expect("saved").to_string();
    for (line, backend) in lines.iter().zip(["jev", "luna"]) {
        assert_eq!(line["session_id"], session_id);
        assert_eq!(line["backend"], backend);
        assert_eq!(line["payload_sha256"], response.payload_sha256().as_str());
        assert_eq!(line["operation"], "judge");
    }
}

#[test]
fn discover_repo_root_uses_the_git_toplevel() {
    let (dir, _sha) = test_support::git_fixture();
    let sub = dir.path().join("notes");
    let root = discover_repo_root(&sub).expect("root");
    assert_eq!(
        std::fs::canonicalize(root.as_path()).expect("canonical root"),
        std::fs::canonicalize(dir.path()).expect("canonical dir")
    );
}

#[test]
fn check_file_is_normalised_lexically_once() {
    let root = std::env::temp_dir().join("sc-jev-check-normalise-root");
    let repo = RepoRoot::new(&root);
    let inside = root.join("sub").join("check.toml");
    let outside = std::env::temp_dir().join("elsewhere").join("check.toml");
    let cases = [
        (PathBuf::from("check.toml"), "check.toml"),
        (PathBuf::from("./sub/check.toml"), "sub/check.toml"),
        (PathBuf::from("sub\\check.toml"), "sub/check.toml"),
        (inside, "sub/check.toml"),
    ];
    for (input, expected) in cases {
        match CheckSource::check_file(&repo, &input).expect("normalised") {
            CheckSource::CheckFile(path) => {
                assert_eq!(path.as_str(), expected, "{}", input.display())
            }
            other => panic!("expected a check file, got {other:?}"),
        }
    }
    for path in [
        PathBuf::from("../check.toml"),
        PathBuf::from("sub/../../x"),
        outside,
        PathBuf::from("."),
    ] {
        let error = CheckSource::check_file(&repo, &path).expect_err("refused");
        assert_eq!(error.code(), "INPUT_INVALID", "{}", path.display());
    }
}

#[test]
fn discovery_falls_back_only_outside_git_or_without_git() {
    let start = PathBuf::from("/work/repo/sub");
    let toplevel = RepoRoot::new("/work/repo");
    assert_eq!(
        classify_discovery(start.clone(), Ok(Some(toplevel.clone())), None).expect("toplevel"),
        toplevel
    );
    assert_eq!(
        classify_discovery(start.clone(), Ok(None), None).expect("outside git"),
        RepoRoot::new(&start)
    );
    let not_installed = std::io::Error::from(std::io::ErrorKind::NotFound);
    assert_eq!(
        classify_discovery(
            start.clone(),
            Err(JevError::io_failed("git", &not_installed)),
            None
        )
        .expect("git absent"),
        RepoRoot::new(&start)
    );
    let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    let error = classify_discovery(
        start.clone(),
        Err(JevError::io_failed("git", &denied)),
        None,
    )
    .expect_err("git failed to run");
    assert_eq!(error.code(), "IO_FAILED");
    let error = classify_discovery(start, Ok(None), Some(PathBuf::from("/work/repo/.git")))
        .expect_err("a .git above start but no work tree");
    assert_eq!(error.code(), "INPUT_INVALID");
    assert!(
        error.message().contains("/work/repo/.git"),
        "{}",
        error.message()
    );
}

#[test]
fn enclosing_git_entry_finds_the_nearest_dot_git() {
    let dir = test_support::temp_dir("enclosing");
    let sub = dir.path().join("a").join("b");
    std::fs::create_dir_all(&sub).expect("sub");
    std::fs::create_dir(dir.path().join(".git")).expect("dot git");
    assert_eq!(enclosing_git_entry(&sub), Some(dir.path().join(".git")));
}

#[test]
fn ask_sends_through_the_engine_and_refuses_unaskable_sessions() {
    let repo = bare_repo();
    let sessions = test_support::temp_dir("ask-sessions");
    let haiku = judge(BackendId::Haiku, "yes");
    let response = run(prepare(
        check_file(),
        config(repo.path(), sessions.path())
            .backends(set(&[BackendId::Haiku]))
            .registry(registry_of(&[&haiku])),
    )
    .expect("prepare"))
    .expect("run");
    let id = response.session_id().expect("saved");
    let store = FileSessionStore::new(sessions.path()).expect("store");
    let log_dir = test_support::temp_dir("ask-log");
    let log_path = log_dir.path().join("session.jsonl");
    let log = Arc::new(JsonlSink {
        path: log_path.clone(),
    });
    let follow = judge(BackendId::Haiku, "no");
    let asked = ask(
        &store,
        id,
        "why?",
        RunConfig::new(RepoRoot::new(repo.path()), log).registry(registry_of(&[&follow])),
    )
    .expect("ask");
    assert_eq!(asked.session_id(), id);
    assert_eq!(asked.payload_sha256(), response.payload_sha256());
    assert_eq!(asked.outcome().answers()["relevant"].choice(), "no");
    assert_eq!(follow.validations(), 1, "ask validates before its send");
    let requests = follow.requests();
    let prompt = &requests[0].llm().expect("llm").system_prompt;
    assert!(prompt.starts_with(PACKAGED_SYSTEM_PROMPT));
    assert!(prompt.ends_with("Follow-up:\nwhy?"), "{prompt}");
    let line: serde_json::Value =
        serde_json::from_str(std::fs::read_to_string(&log_path).expect("log").trim())
            .expect("line");
    assert_eq!(line["operation"], "ask");
    assert_eq!(line["session_id"], id.to_string());

    let empty = ask(&store, id, "  ", config(repo.path(), sessions.path())).expect_err("empty");
    assert_eq!(empty.code(), "INPUT_INVALID");
    let missing = ask(
        &store,
        SessionId::new(),
        "why?",
        config(repo.path(), sessions.path()),
    )
    .expect_err("missing");
    assert_eq!(missing.code(), "SESSION_NOT_FOUND");
}

#[test]
fn ask_envelope_carries_digest_warnings_and_readback_in_order() {
    let payload = PayloadBundle::encode(&sc_decision_schema::CanonicalJudgePayload::new(
        Vec::new(),
        Vec::new(),
    ))
    .expect("bundle");
    let request = JudgeRequest::new(Arc::new(payload.clone()), None);
    let outcome = JudgeOutcome::new(
        BackendId::Haiku,
        &request,
        std::collections::BTreeMap::new(),
    )
    .expect("outcome")
    .with_provider_meta(serde_json::json!({"model": "m", "stop_reason": "end_turn"}));
    let id = SessionId::new();
    let response = AskResponse::new(outcome, id, payload.sha256().clone())
        .with_warning("session log was not flushed: disk full");
    let envelope = Envelope::ask(&response)
        .with_readback(serde_json::json!({"session_id": id.to_string()}))
        .expect("readback");
    let text = serde_json::to_string(&envelope).expect("json");
    let data_at = text.find("\"data\":").expect("data");
    let order = [
        "\"version\"",
        "\"operation\"",
        "\"answers\"",
        "\"model\"",
        "\"session_id\"",
        "\"payload_sha256\"",
        "\"warnings\"",
        "\"readback\"",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|key| {
            if key.contains("version") || key.contains("operation") {
                text.find(key).expect(key)
            } else {
                data_at + text[data_at..].find(key).expect(key)
            }
        })
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{text}");
    let value: serde_json::Value = serde_json::from_str(&text).expect("value");
    assert_eq!(value["operation"], "jev.ask");
    assert_eq!(value["data"]["payload_sha256"], payload.sha256().as_str());
    assert_eq!(
        value["data"]["warnings"],
        serde_json::json!(["session log was not flushed: disk full"])
    );
}

#[derive(Default)]
struct CountingLog(std::sync::Mutex<usize>);

impl SessionLog for CountingLog {
    fn emit(&self, _event: LogEvent) -> Result<(), LogRejected> {
        *self.0.lock().expect("count") += 1;
        Ok(())
    }
}

/// Every call that reaches `judge` emits exactly one log event, a call
/// cancelled before it began included (REQ-SAG-0610, ADR-SAG-0013).
#[test]
fn cancelled_call_still_emits_one_log_event() {
    let failing = || {
        MockJudgeBackend::new(BackendId::Jev, std::collections::BTreeMap::new()).with_failure(
            JevError::network(
                NetworkErrorKind::Cancelled,
                "request cancelled before a send",
            ),
        )
    };
    let bundle = {
        let payload: sc_decision_schema::CanonicalJudgePayload =
            serde_json::from_value(serde_json::json!({
                "questions": [{"id": "q", "prompt": "p", "choices": ["no", "yes"]}],
                "state": []
            }))
            .expect("payload");
        Arc::new(PayloadBundle::encode(&payload).expect("bundle"))
    };
    let session = SessionId::new();

    let log = Arc::new(CountingLog::default());
    let shared: SharedSessionLog = log.clone();
    let config = RunConfig::new(RepoRoot::new("."), Arc::clone(&shared));
    let send_log = SendLog::new(&config, session, SystemTime::now(), Operation::Judge);
    let token = CancelToken::new();
    token.cancel();
    let request = JudgeRequest::new(Arc::clone(&bundle), None).with_cancel(token);
    let error = send(&failing(), &request, &send_log).expect_err("cancelled");
    assert_eq!(error.code(), "NETWORK_ERROR");
    assert_eq!(*log.0.lock().expect("count"), 1, "a failed call logs once");

    let live = JudgeRequest::new(bundle, None);
    send(&failing(), &live, &send_log).expect_err("failed");
    assert_eq!(*log.0.lock().expect("count"), 2, "each call logs once");
}
