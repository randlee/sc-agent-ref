//! Session store: document versioning (r-1 D7.6, REQ-SAG-0600) and the
//! stateless store with its no-lock concurrency model (REQ-SAG-0607, D4).
//!
//! Version 3 is the only accepted session document. A Phase A (version 1)
//! file, a Phase A.1 version 2 file and a file written by a newer sc-agent
//! all load as `SessionDocumentInvalid` (`INPUT_INVALID`) with their own
//! recovery text.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use sc_agent_types::observability::v2::{NoopSessionLog, SharedSessionLog};
use sc_agent_types::{BackendId, BackendSet, JevError, RepoRoot, SessionId};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_decision_schema::{
    CanonicalJudgePayload, JudgeBackend, JudgeOutcome, JudgeRequest, PayloadBundle,
};
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::{CheckSource, RunConfig, ask, prepare, run};
use sc_jev_check::session::{FileSessionStore, StoredSession};
use tempfile::TempDir;

/// The id the old-version fixtures are stored under (a ULID since r-6; the
/// committed v1 file still names the UUID it was written with).
const OLD_SESSION_ID: &str = "01JC4Z7A5B0000000000000001";

/// The committed Phase A document, re-keyed to a ULID so the store can find
/// it by id; only the `version` decides the outcome.
fn fixture() -> String {
    let text = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/session-v1.json"),
    )
    .expect("session-v1.json");
    let mut document: serde_json::Value = serde_json::from_str(&text).expect("json");
    document["session_id"] = serde_json::json!(OLD_SESSION_ID);
    document.to_string()
}

/// A store whose clock sits just after the fixture's `created_at`, so the
/// only possible failure is the document version.
fn store(dir: &tempfile::TempDir) -> FileSessionStore {
    fn clock() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_millis(1_700_000_001_000)
    }
    FileSessionStore::new(dir.path())
        .expect("store")
        .with_clock(clock)
}

fn load(text: &str) -> JevError {
    let dir = tempfile::tempdir().expect("temp");
    let store = store(&dir);
    let id: SessionId = serde_json::from_value(serde_json::json!(OLD_SESSION_ID)).expect("id");
    std::fs::write(store.path(&id), text).expect("write session");
    store.load(&id).expect_err("refused")
}

#[test]
fn test_phase_a_session_is_invalid() {
    let error = load(&fixture());
    assert!(
        matches!(error, JevError::SessionDocumentInvalid { .. }),
        "{error:?}"
    );
    assert_eq!(error.code(), "INPUT_INVALID");
    let hint = error.suggested_action();
    assert!(
        hint.contains("re-run the check; session files from before Phase A.1 cannot be resumed"),
        "{hint}"
    );
    assert!(
        error.message().contains(OLD_SESSION_ID),
        "the message names the file: {}",
        error.message()
    );

    let mut unversioned: serde_json::Value = serde_json::from_str(&fixture()).expect("json");
    unversioned
        .as_object_mut()
        .expect("object")
        .remove("version");
    let error = load(&unversioned.to_string());
    assert!(
        error
            .suggested_action()
            .contains("session files from before Phase A.1 cannot be resumed"),
        "{error:?}"
    );
}

/// A version 2 document (UUID-era, no `decision_schema`) is refused with the
/// "re-run the check" recovery (REQ-SAG-0612).
#[test]
fn test_v2_session_document_is_invalid() {
    let mut v2: serde_json::Value = serde_json::from_str(&fixture()).expect("json");
    v2["version"] = serde_json::json!(2);
    let error = load(&v2.to_string());
    assert!(
        matches!(error, JevError::SessionDocumentInvalid { .. }),
        "{error:?}"
    );
    assert_eq!(error.code(), "INPUT_INVALID");
    let message = error.message();
    assert!(message.contains("is version 2"), "{message}");
    assert!(
        message.contains(OLD_SESSION_ID),
        "message names the file: {message}"
    );
    assert!(error.suggested_action().contains("re-run the check"));
}

/// `FileSessionStore::new` removes expired session documents, version 1 and 2
/// included, and leaves fresh ones and files that are not documents (RSH-001).
#[test]
fn test_new_store_sweeps_expired_session_documents() {
    let dir = tempfile::tempdir().expect("temp");
    let ids = [
        "01JC4Z7A5B0000000000000011",
        "01JC4Z7A5B0000000000000012",
        "01JC4Z7A5B0000000000000013",
    ];
    // Phase A (v1), Phase A.1 (v2) and current (v3) documents, all created
    // in 2023, long past the 24 h default TTL.
    for (id, version) in ids.iter().zip([1, 2, 3]) {
        let mut document: serde_json::Value = serde_json::from_str(&fixture()).expect("json");
        document["session_id"] = serde_json::json!(id);
        document["version"] = serde_json::json!(version);
        fs::write(dir.path().join(format!("{id}.json")), document.to_string()).expect("write");
    }
    for id in ids {
        assert!(dir.path().join(format!("{id}.json")).is_file());
    }
    // Opening the store sweeps them; a fresh document (created now) survives.
    let fresh_store = FileSessionStore::new(dir.path()).expect("store");
    let fresh_id = SessionId::new();
    let payload = {
        let document: serde_json::Value = serde_json::from_str(&fixture()).expect("json");
        PayloadBundle::decode(
            document["payload_utf8"]
                .as_str()
                .expect("payload")
                .as_bytes(),
        )
        .expect("bundle")
    };
    fresh_store
        .save(&StoredSession {
            session_id: fresh_id,
            backend: BackendId::AnthropicHaiku,
            created_at: SystemTime::now(),
            payload,
            system_prompt: None,
            outcomes: Vec::new(),
            follow_ups: Vec::new(),
        })
        .expect("save fresh");
    // Not documents: left alone.
    fs::write(dir.path().join("notes.json"), "{\"unrelated\":true}").expect("notes");
    fs::write(dir.path().join("broken.json"), "{").expect("broken");
    fs::write(dir.path().join("keep.txt"), "x").expect("txt");

    for id in ids {
        assert!(
            !dir.path().join(format!("{id}.json")).exists(),
            "{id} swept"
        );
    }
    let names = |dir: &Path| {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    };
    let mut expected = vec![
        format!("{fresh_id}.json"),
        "broken.json".to_owned(),
        "keep.txt".to_owned(),
        "notes.json".to_owned(),
    ];
    expected.sort();
    let reopened = FileSessionStore::new(dir.path()).expect("reopen");
    assert_eq!(names(dir.path()), expected);
    reopened.load(&fresh_id).expect("fresh session still loads");
}

/// A document between TTL and TTL plus grace survives the open-time sweep and
/// still reads as `SESSION_EXPIRED` (RSH-001).
#[test]
fn test_expired_document_inside_grace_survives_sweep_and_reads_expired() {
    let dir = tempfile::tempdir().expect("temp");
    let store = FileSessionStore::new(dir.path()).expect("store");
    let id = SessionId::new();
    let document: serde_json::Value = serde_json::from_str(&fixture()).expect("json");
    let payload = PayloadBundle::decode(
        document["payload_utf8"]
            .as_str()
            .expect("payload")
            .as_bytes(),
    )
    .expect("bundle");
    // 30 h old: past the 24 h TTL, inside TTL plus one TTL of grace.
    store
        .save(&StoredSession {
            session_id: id,
            backend: BackendId::AnthropicHaiku,
            created_at: SystemTime::now() - Duration::from_secs(30 * 60 * 60),
            payload,
            system_prompt: None,
            outcomes: Vec::new(),
            follow_ups: Vec::new(),
        })
        .expect("save");
    let reopened = FileSessionStore::new(dir.path()).expect("reopen sweeps");
    assert!(store.path(&id).is_file(), "inside the grace window");
    assert_eq!(
        reopened.load(&id).expect_err("expired").code(),
        "SESSION_EXPIRED"
    );
}

#[test]
fn test_newer_session_version_is_invalid() {
    let mut newer: serde_json::Value = serde_json::from_str(&fixture()).expect("json");
    newer["version"] = serde_json::json!(4);
    let error = load(&newer.to_string());
    assert!(
        matches!(error, JevError::SessionDocumentInvalid { .. }),
        "{error:?}"
    );
    assert_eq!(error.code(), "INPUT_INVALID");
    assert!(
        error
            .suggested_action()
            .contains("written by a newer sc-agent; upgrade or re-run the check"),
        "{}",
        error.suggested_action()
    );
}

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

fn temp(label: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("sc-jev-check-session-{label}-"))
        .tempdir()
        .expect("temp")
}

fn repo() -> TempDir {
    let dir = temp("repo");
    fs::write(dir.path().join("questions.json"), QUESTIONS).expect("questions");
    fs::write(
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

fn registry(judges: &[&Arc<MockJudgeBackend>]) -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    for judge in judges {
        registry
            .register(Arc::clone(*judge) as Arc<dyn JudgeBackend>)
            .expect("register");
    }
    registry
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// One live run of `backends` against `repo`, saving into `sessions`.
fn live_run(
    repo: &Path,
    sessions: &Path,
    backends: &[BackendId],
    judges: &[&Arc<MockJudgeBackend>],
) -> sc_jev_check::run::RunResponse {
    let config = RunConfig::new(RepoRoot::new(repo), Arc::new(NoopSessionLog))
        .backends(BackendSet::try_from(backends.to_vec()).expect("set"))
        .sessions_dir(sessions)
        .registry(Arc::new(registry(judges)));
    let source =
        CheckSource::check_file(&RepoRoot::new(repo), Path::new("check.toml")).expect("check path");
    run(prepare(source, config).expect("prepare")).expect("run")
}

fn payload() -> PayloadBundle {
    let payload: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
        "questions": [{"id": "relevant", "prompt": "Does it apply?", "choices": ["no", "yes"]}],
        "state": []
    }))
    .expect("payload");
    PayloadBundle::encode(&payload).expect("bundle")
}

fn stored(store: &FileSessionStore, id: SessionId, choice: &str) -> StoredSession {
    let bundle = payload();
    let request = JudgeRequest::new(Arc::new(bundle.clone()), None);
    let answers = serde_json::from_value(serde_json::json!({"relevant": {"choice": choice}}))
        .expect("answers");
    StoredSession {
        session_id: id,
        backend: BackendId::AnthropicHaiku,
        created_at: store.now(),
        payload: bundle,
        system_prompt: Some(format!("prompt-{choice}")),
        outcomes: vec![
            JudgeOutcome::new(BackendId::AnthropicHaiku, &request, answers).expect("outcome"),
        ],
        follow_ups: Vec::new(),
    }
}

#[test]
fn test_session_written_once_only_with_llm() {
    let repo = repo();
    let sessions = temp("once");
    let jev_only = live_run(
        repo.path(),
        sessions.path(),
        &[BackendId::Jev],
        &[&judge(BackendId::Jev, "yes")],
    );
    assert!(!jev_only.session_saved());
    assert!(jev_only.session_id().is_none());
    assert!(
        names(sessions.path()).is_empty(),
        "a run with no ask-capable backend writes no session"
    );

    let jev = judge(BackendId::Jev, "yes");
    let haiku = judge(BackendId::AnthropicHaiku, "no");
    let both = live_run(
        repo.path(),
        sessions.path(),
        &[BackendId::Jev, BackendId::AnthropicHaiku],
        &[&jev, &haiku],
    );
    assert!(both.session_saved());
    let id = both.session_id().expect("saved");
    assert_eq!(
        names(sessions.path()),
        [format!("{id}.json")],
        "one file, no temporary leftovers"
    );
    let store = FileSessionStore::new(sessions.path()).expect("store");
    let loaded = store.load(&id).expect("load");
    assert_eq!(loaded.backend, BackendId::AnthropicHaiku);
    assert_eq!(
        loaded
            .outcomes
            .iter()
            .map(JudgeOutcome::backend)
            .collect::<Vec<_>>(),
        [BackendId::Jev, BackendId::AnthropicHaiku],
        "the one write happens after the last backend"
    );
    assert_eq!(loaded.payload.sha256(), both.payload_sha256());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(store.path(&id))
            .expect("meta")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn test_ask_takes_registry() {
    let repo = repo();
    let sessions = temp("ask");
    let run_judge = judge(BackendId::AnthropicHaiku, "yes");
    let response = live_run(
        repo.path(),
        sessions.path(),
        &[BackendId::AnthropicHaiku],
        &[&run_judge],
    );
    let id = response.session_id().expect("saved");
    let store = FileSessionStore::new(sessions.path()).expect("fresh store, no state");
    let path = store.path(&id);
    let before = fs::read(&path).expect("session bytes");

    let ask_judge = judge(BackendId::AnthropicHaiku, "no");
    let log: SharedSessionLog = Arc::new(NoopSessionLog);
    let ask_config = |registry: BackendRegistry| {
        RunConfig::new(RepoRoot::new(repo.path()), Arc::clone(&log)).registry(Arc::new(registry))
    };
    let asked = ask(
        &store,
        id,
        "why the choice?",
        ask_config(registry(&[&ask_judge])),
    )
    .expect("ask");
    assert_eq!(asked.outcome().answers()["relevant"].choice(), "no");
    assert_eq!(asked.payload_sha256(), response.payload_sha256());
    assert_eq!(
        run_judge.requests().len(),
        1,
        "the run's judge is not reused"
    );
    let requests = ask_judge.requests();
    assert_eq!(requests.len(), 1, "ask uses the registry it is given");
    let prompt = &requests[0].llm().expect("prompt").system_prompt;
    assert!(prompt.contains("why the choice?"), "{prompt}");
    assert!(prompt.contains("Session context:"), "{prompt}");
    assert_eq!(
        fs::read(&path).expect("session bytes"),
        before,
        "ask does not rewrite the file"
    );

    let missing = ask(&store, id, "again?", ask_config(BackendRegistry::new()))
        .expect_err("backend not in the registry");
    assert_eq!(missing.code(), "CONFIG_INVALID");
}

#[test]
fn test_two_stores_same_dir_last_writer_wins() {
    let dir = temp("two");
    let first = FileSessionStore::new(dir.path()).expect("first");
    let second = FileSessionStore::new(dir.path()).expect("second");
    let id = SessionId::new();
    first.save(&stored(&first, id, "yes")).expect("first save");
    second
        .save(&stored(&second, id, "no"))
        .expect("second save");
    let loaded = first.load(&id).expect("load");
    assert_eq!(loaded.outcomes[0].answers()["relevant"].choice(), "no");
    assert_eq!(loaded.system_prompt.as_deref(), Some("prompt-no"));

    // Concurrent writers of one id: every load in between sees a whole,
    // verified document, and the final file is one writer's. POSIX rename
    // replaces atomically under concurrency; on Windows a concurrent replace
    // may fail with a sharing error instead (module docs), so this half is
    // Unix-only.
    #[cfg(unix)]
    concurrent_writers(&first, &second, id);
    assert_eq!(
        names(dir.path()),
        [format!("{id}.json")],
        "no temporary leftovers"
    );
}

/// Longest a writer thread may take: ten save/load rounds finish in well under
/// a second, so this only fires on a real hang.
#[cfg(unix)]
const WRITER_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(unix)]
fn concurrent_writers(first: &FileSessionStore, second: &FileSessionStore, id: SessionId) {
    let (done, rx) = std::sync::mpsc::channel::<usize>();
    let writers: Vec<_> = (0..8)
        .map(|index| {
            let store = if index % 2 == 0 {
                first.clone()
            } else {
                second.clone()
            };
            let choice = if index % 2 == 0 { "yes" } else { "no" };
            let done = done.clone();
            std::thread::spawn(move || {
                for _ in 0..10 {
                    store.save(&stored(&store, id, choice)).expect("save");
                    store.load(&id).expect("a reader never sees a torn file");
                }
                // The receiver may have timed out and gone; nothing to do then.
                let _ = done.send(index);
            })
        })
        .collect();
    // Each writer reports completion; a stuck one fails the test by name
    // instead of hanging the join.
    let mut finished = std::collections::BTreeSet::new();
    for _ in 0..writers.len() {
        match rx.recv_timeout(WRITER_TIMEOUT) {
            Ok(index) => {
                finished.insert(index);
            }
            Err(_) => {
                let stuck: Vec<usize> = (0..writers.len())
                    .filter(|index| !finished.contains(index))
                    .collect();
                panic!("writer(s) {stuck:?} did not finish within {WRITER_TIMEOUT:?}");
            }
        }
    }
    for writer in writers {
        writer.join().expect("writer");
    }
    let last = second.load(&id).expect("final");
    let choice = last.outcomes[0].answers()["relevant"]
        .choice()
        .as_str()
        .to_owned();
    assert!(choice == "yes" || choice == "no", "{choice}");
}

#[test]
fn test_save_error_removes_tmp() {
    let dir = temp("tmp");
    let store = FileSessionStore::new(dir.path()).expect("store");
    let id = SessionId::new();
    // A non-empty directory at `<id>.json` makes the final rename fail
    // after the temporary file was written.
    let blocker = store.path(&id);
    fs::create_dir(&blocker).expect("blocker");
    fs::write(blocker.join("keep"), b"x").expect("keep");
    let error = store
        .save(&stored(&store, id, "yes"))
        .expect_err("rename fails");
    assert_eq!(error.code(), "IO_FAILED");
    assert!(
        error.message().contains(&blocker.display().to_string()),
        "{}",
        error.message()
    );
    assert_eq!(
        names(dir.path()),
        [format!("{id}.json")],
        "the temporary file is removed"
    );
}

#[test]
fn test_stale_tmp_swept() {
    let dir = temp("sweep");
    let id = SessionId::new();
    let stale = dir.path().join(format!("{id}.json.tmp.1.00000000deadbeef"));
    let fresh = dir.path().join(format!("{id}.json.tmp.2.00000000cafef00d"));
    let session = dir.path().join(format!("{id}.json"));
    for path in [&stale, &fresh, &session] {
        fs::write(path, b"{}").expect("write");
    }
    // Older than the store's one-hour stale-tmp age.
    let old = SystemTime::now() - Duration::from_secs(60 * 60 + 60);
    for path in [&stale, &session] {
        fs::File::options()
            .write(true)
            .open(path)
            .expect("open")
            .set_modified(old)
            .expect("mtime");
    }
    FileSessionStore::new(dir.path()).expect("store sweeps on open");
    assert!(
        !stale.exists(),
        "a temporary file older than one hour is removed"
    );
    assert!(
        fresh.exists(),
        "a recent temporary file may belong to a live writer"
    );
    assert!(session.exists(), "session files are never swept");
}
