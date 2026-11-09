//! REQ-SAG-0610 (emission), REQ-SAG-0611 (fields) and REQ-SAG-0617 (the
//! line's `fields{}` is one session-log row) for the session-log adapter,
//! moved from the `sc-ai` and `sc-jev-py` roots.

use std::path::PathBuf;

use sc_agent_types::observability::v2::{LogEvent as SessionEvent, SessionLog};
use sc_agent_types::{Operation, SessionId};
use sc_decision_schema::session_log::{SendOutcome, SessionLogRow};
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

/// The committed rows of `tests/fixtures/decision-schema/session-log`.
fn fixture_rows() -> Vec<serde_json::Map<String, serde_json::Value>> {
    include_str!("../../../tests/fixtures/decision-schema/session-log/fields.jsonl")
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("fixture line"))
        .collect()
}

/// The event the engine emits for fixture row `index`.
fn event_for(index: usize) -> SessionEvent {
    let fields = fixture_rows().swap_remove(index);
    let row = SessionLogRow::try_from_fields(&fields).expect("fixture row");
    let context = row.context();
    SessionEvent::new(context.session_id, context.operation, row.to_fields())
}

/// Golden name and fixture row index: judge ok, judge failed with a body,
/// judge failed before a body was built, ask.
const GOLDEN_CASES: [(&str, usize); 4] = [
    ("judge_ok", 0),
    ("judge_failed", 1),
    ("judge_failed_no_body", 2),
    ("ask", 3),
];

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
/// pins the action, target and fields (REQ-SAG-0611): judge `ok`, judge
/// `failed` with and without a body, and `ask`.
#[test]
fn test_adapter_matches_the_shared_session_event_golden() {
    let root = TempDir::new().expect("temp");
    let adapter = ObservabilityAdapter::open(&policy(), root.path()).expect("adapter");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/observability/session-event.json"
    ))
    .expect("golden json");
    assert_eq!(
        golden.as_object().expect("object").len(),
        GOLDEN_CASES.len()
    );
    for (name, index) in GOLDEN_CASES {
        assert_eq!(
            projection(&adapter.log_event(&event_for(index)).expect("event")),
            golden[name],
            "{name}"
        );
    }
}

#[test]
fn test_every_operation_is_an_action_name() {
    let root = TempDir::new().expect("temp");
    let adapter = ObservabilityAdapter::open(&policy(), root.path()).expect("adapter");
    for operation in Operation::ALL.iter().copied() {
        let event = SessionEvent::new(SessionId::new(), operation, serde_json::Map::new());
        let event = adapter.log_event(&event).expect("valid");
        assert_eq!(event.action.as_str(), operation.as_str());
    }
}

/// Every written line's `fields` is one session-log row (REQ-SAG-0617): it
/// parses with `SessionLogRow::try_from_fields`, carries `send_outcome`, and
/// the envelope `outcome` stays `null`. No secret-bearing key is written.
#[test]
fn test_log_line_fields_is_one_session_log_row() {
    let root = TempDir::new().expect("temp");
    let adapter = ObservabilityAdapter::open(&policy(), root.path()).expect("logger");
    let rows = fixture_rows();
    for index in 0..rows.len() {
        adapter.emit(event_for(index)).expect("log");
    }
    adapter
        .flush()
        .unwrap_or_else(|error| panic!("log flush failed: {}", error.message()));
    let text = jsonl(root.path());
    let lines: Vec<serde_json::Value> = text
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("line"))
        .collect();
    assert_eq!(lines.len(), rows.len(), "{text}");
    for (line, fixture) in lines.iter().zip(&rows) {
        let fields = line["fields"].as_object().expect("fields object");
        assert_eq!(fields, fixture);
        let row = SessionLogRow::try_from_fields(fields).expect("a session log row");
        assert!(matches!(
            row.send_outcome(),
            SendOutcome::Ok | SendOutcome::Failed
        ));
        assert!(fields["send_outcome"].is_string());
        assert!(line["outcome"].is_null(), "{line}");
        assert_eq!(line["action"], fields["operation"]);
        assert_eq!(line["target"], "jev.session");
    }
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
    first.emit(event_for(0)).expect("first line");
    second.emit(event_for(3)).expect("second line");
    second
        .flush()
        .unwrap_or_else(|error| panic!("log flush failed: {}", error.message()));
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
    assert_eq!(error.code(), "IO_FAILED");
    let hint = error.suggested_action();
    assert!(!hint.contains("check.toml"), "{hint}");
    assert!(hint.contains("--log-dir"), "{hint}");
    assert!(
        error.message().contains("not-a-directory"),
        "{}",
        error.message()
    );
    assert!(error.details().expect("details").io_kind.is_some());
}

/// Makes `dir` read-only; returns false (so the test skips) when the process
/// can still write there, as root can.
#[cfg(unix)]
fn make_unwritable(dir: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).expect("chmod");
    let probe = dir.join(".probe");
    let writable = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(probe);
    !writable
}

/// Audit rows lost silently: with an unwritable `logs/` directory the writer
/// drops the row but the flush still succeeds. The loss is reported once, by
/// `take_lost_rows`, so a caller can warn.
#[cfg(unix)]
#[test]
fn test_rows_the_writer_cannot_write_are_reported_once() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().expect("temp");
    let logs = root.path().join("logs");
    std::fs::create_dir(&logs).expect("logs");
    if !make_unwritable(&logs) {
        return;
    }
    let adapter = ObservabilityAdapter::open(&policy(), root.path()).expect("adapter");
    adapter.emit(event_for(0)).expect("admitted");
    adapter.emit(event_for(3)).expect("admitted");
    adapter
        .flush()
        .expect("a flush of nothing to write succeeds");
    let warning = adapter.take_lost_rows().expect("the loss is visible");
    assert!(
        warning.starts_with("2 session-log row(s) could not be written"),
        "{warning}"
    );
    assert!(warning.contains(SERVICE), "{warning}");
    assert!(adapter.take_lost_rows().is_none(), "reported once");

    // A healthy writer afterwards reports nothing new.
    std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    adapter.emit(event_for(0)).expect("admitted");
    adapter.flush().expect("flush");
    assert!(adapter.take_lost_rows().is_none());
    assert_eq!(jsonl(root.path()).lines().count(), 1);
}

#[test]
fn test_a_healthy_writer_loses_nothing() {
    let root = TempDir::new().expect("temp");
    let adapter = ObservabilityAdapter::open(&policy(), root.path()).expect("adapter");
    adapter.emit(event_for(0)).expect("admitted");
    adapter.flush().expect("flush");
    assert!(adapter.take_lost_rows().is_none());
}

/// A short-lived process never reaches `sc-observability`'s first maintenance
/// pass (60 s after start): the adapter runs the startup pass itself.
#[test]
fn test_an_oversized_log_rotates_when_the_logger_starts() {
    let root = TempDir::new().expect("temp");
    let logs = root.path().join("logs");
    std::fs::create_dir(&logs).expect("logs");
    let active = logs.join(format!("{SERVICE}.log.jsonl"));
    // Sparse: 65 MiB, over the 64 MiB default, without writing it.
    let file = std::fs::File::create(&active).expect("file");
    file.set_len(65 * 1024 * 1024).expect("sparse");
    drop(file);

    let adapter = ObservabilityAdapter::open(&policy(), root.path()).expect("adapter");
    adapter.emit(event_for(0)).expect("row");
    adapter.flush().expect("flush");

    let rotated = logs.join(format!("{SERVICE}.log.jsonl.1"));
    assert_eq!(
        std::fs::metadata(&rotated).expect("rotated file").len(),
        65 * 1024 * 1024
    );
    let current = std::fs::read_to_string(&active).expect("active");
    assert_eq!(current.lines().count(), 1, "only the new row: {current}");
}

/// Log files hold answers and repo paths: directories this call creates are
/// `0700`, the active and a rotated file are `0600`.
#[cfg(unix)]
#[test]
fn test_log_dirs_and_files_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &std::path::Path| {
        std::fs::metadata(path).expect("meta").permissions().mode() & 0o777
    };
    let temp = TempDir::new().expect("temp");
    let root = temp.path().join("new").join("root");
    let logs = root.join("logs");
    let active = logs.join(format!("{SERVICE}.log.jsonl"));
    let adapter = ObservabilityAdapter::open(&policy(), &root).expect("adapter");
    adapter.emit(event_for(0)).expect("row");
    adapter.flush().expect("flush");
    assert_eq!(mode(&root), 0o700);
    assert_eq!(mode(&logs), 0o700);
    assert_eq!(mode(&active), 0o600);
    drop(adapter);

    // An older, world-readable file is tightened, and so is the file a
    // rotation creates.
    std::fs::set_permissions(&active, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&active)
        .expect("open");
    file.set_len(65 * 1024 * 1024).expect("sparse");
    drop(file);
    let adapter = ObservabilityAdapter::open(&policy(), &root).expect("reopen");
    adapter.emit(event_for(3)).expect("row");
    adapter.flush().expect("flush");
    assert_eq!(mode(&active), 0o600);
    assert_eq!(mode(&logs.join(format!("{SERVICE}.log.jsonl.1"))), 0o600);
}
