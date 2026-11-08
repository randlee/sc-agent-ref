//! REQ-SAG-0610 (emission) and REQ-SAG-0611 (fields) for the session-log
//! adapter, moved from the `sc-ai` and `sc-jev-py` roots.

use std::path::PathBuf;

use sc_agent_types::observability::v2::{LogEvent as SessionEvent, SessionLog};
use sc_agent_types::{BackendId, Operation, SessionId, Sha256Digest};
use sc_jev_host::obs::ObservabilityAdapter;
use sc_jev_host::paths::{DefaultRoot, PathPolicy, SessionDefault};
use tempfile::TempDir;

const SERVICE: &str = "sc-example";

fn policy() -> PathPolicy {
    PathPolicy {
        service: SERVICE,
        log_root_flag: "--log-dir",
        log_root_env: &[],
        log_root_default: DefaultRoot::Fixed(PathBuf::from("log")),
        session_dir_env: &[],
        session_dir_default: SessionDefault::UnderLogRoot(PathBuf::from("sessions")),
    }
}

fn event(session_id: SessionId, digest: &Sha256Digest, operation: Operation) -> SessionEvent {
    SessionEvent::new(
        session_id,
        BackendId::AnthropicHaiku,
        digest.clone(),
        operation,
    )
}

fn golden_event(operation: Operation) -> SessionEvent {
    SessionEvent::new(
        serde_json::from_str("\"00000000-0000-0000-0000-000000000000\"").expect("nil session id"),
        BackendId::AnthropicHaiku,
        Sha256Digest::of(b"payload"),
        operation,
    )
}

/// Stable projection of an adapted event: every key except the per-process
/// `timestamp`, `service` and `identity`.
fn projection(event: &sc_observability::LogEvent) -> serde_json::Value {
    let mut value = serde_json::to_value(event).expect("serialize");
    let object = value.as_object_mut().expect("object");
    for volatile in ["timestamp", "service", "identity"] {
        object.remove(volatile);
    }
    value
}

fn jsonl(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("logs").join(format!("{SERVICE}.log.jsonl"))).expect("jsonl")
}

/// QA-004: both composition roots share this one adapter, so one golden
/// (every `Operation`) pins the action, target and fields (REQ-SAG-0611).
#[test]
fn test_adapter_matches_the_shared_session_event_golden() {
    let root = TempDir::new().expect("temp");
    let adapter = ObservabilityAdapter::open(&policy(), root.path()).expect("adapter");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/observability/session-event.json"
    ))
    .expect("golden json");
    for operation in Operation::ALL.iter().copied() {
        assert_eq!(
            projection(&adapter.log_event(&golden_event(operation)).expect("event")),
            golden[operation.as_str()],
            "{operation}"
        );
    }
}

#[test]
fn test_every_operation_is_an_action_name() {
    let root = TempDir::new().expect("temp");
    let adapter = ObservabilityAdapter::open(&policy(), root.path()).expect("adapter");
    for operation in Operation::ALL.iter().copied() {
        let event = adapter.log_event(&golden_event(operation)).expect("valid");
        assert_eq!(event.action.as_str(), operation.as_str());
    }
}

#[test]
fn test_adapter_writes_one_line_without_secret_fields() {
    let root = TempDir::new().expect("temp");
    let adapter = ObservabilityAdapter::open(&policy(), root.path()).expect("logger");
    let digest = Sha256Digest::of(b"payload");
    let session_id = SessionId::new();
    adapter
        .emit(event(session_id, &digest, Operation::Judge))
        .expect("log");
    adapter.flush().expect("flush");
    let text = jsonl(root.path());
    let lines: Vec<_> = text.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert!(lines[0].contains(digest.as_str()));
    assert!(lines[0].contains("anthropic_haiku"));
    assert!(lines[0].contains("judge"));
    assert!(lines[0].contains(&session_id.to_string()));
    let lower = text.to_ascii_lowercase();
    for forbidden in ["api_key", "authorization", "bearer ", "password"] {
        assert!(!lower.contains(forbidden), "{forbidden} in {text}");
    }
}

#[test]
fn test_one_writer_per_log_root_while_any_handle_lives() {
    let root = TempDir::new().expect("temp");
    let first = ObservabilityAdapter::open(&policy(), root.path()).expect("first");
    // A different spelling of the same directory maps to the same key.
    let second = ObservabilityAdapter::open(&policy(), &root.path().join(".")).expect("second");
    assert!(first.shares_writer_with(&second));
    let digest = Sha256Digest::of(b"payload");
    first
        .emit(event(SessionId::new(), &digest, Operation::Judge))
        .expect("first line");
    second
        .emit(event(SessionId::new(), &digest, Operation::Ask))
        .expect("second line");
    second.flush().expect("flush");
    let text = jsonl(root.path());
    assert_eq!(text.lines().filter(|line| !line.is_empty()).count(), 2);

    let other = TempDir::new().expect("other");
    let third = ObservabilityAdapter::open(&policy(), other.path()).expect("third");
    assert!(!third.shares_writer_with(&first));

    drop((first, second));
    let fresh = ObservabilityAdapter::open(&policy(), root.path()).expect("fresh");
    assert!(
        fresh.flush().is_ok(),
        "a fresh logger starts after the last drop"
    );
}

#[test]
fn test_service_name_is_validated() {
    let root = TempDir::new().expect("temp");
    let bad = PathPolicy {
        service: "not a valid service!",
        ..policy()
    };
    let error = ObservabilityAdapter::open(&bad, root.path()).expect_err("invalid service name");
    assert_eq!(error.code(), "CONFIG_INVALID");
}

/// A log root that cannot be created names the policy's flag and never
/// `check.toml`.
#[test]
fn test_unwritable_log_root_hint_names_the_flag() {
    let root = TempDir::new().expect("temp");
    let file = root.path().join("not-a-directory");
    std::fs::write(&file, b"x").expect("file");
    let error = ObservabilityAdapter::open(&policy(), &file.join("nested"))
        .expect_err("a parent path is a file");
    assert_eq!(error.code(), "CONFIG_INVALID");
    let hint = error.fix_hint();
    assert!(!hint.contains("check.toml"), "{hint}");
    assert!(hint.contains("--log-dir"), "{hint}");
}
