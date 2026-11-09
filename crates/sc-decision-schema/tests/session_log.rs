//! Session-log storage schema and `fields{}` projection (REQ-SAG-0146).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use sc_agent_types::{
    AnswerId, BackendId, ErrorCode, ModelId, Operation, PayloadSha256, ProjectLabel,
    QuestionsSha256, SessionId,
};
use sc_decision_schema::session_log::{
    COLUMNS, RowContextBuilder, SESSION_LOG_DDL, SESSION_LOG_SCHEMA_VERSION, SendOutcome,
    SessionLogRow, TimestampMs,
};
use sc_decision_schema::{CANONICAL_V1, DecisionAnswers, QuestionId};
use serde_json::{Map, Value, json};

fn fixture_lines() -> Vec<Map<String, Value>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/decision-schema/session-log/fields.jsonl");
    std::fs::read_to_string(path)
        .expect("fixture")
        .lines()
        .map(
            |line| match serde_json::from_str(line).expect("json line") {
                Value::Object(map) => map,
                other => panic!("fixture line is not an object: {other}"),
            },
        )
        .collect()
}

/// The column names of every `CREATE TABLE` in the DDL, with the table.
fn ddl_columns() -> Vec<(String, String)> {
    let mut table = None;
    let mut columns = Vec::new();
    for line in SESSION_LOG_DDL.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("CREATE TABLE ") {
            table = rest.split_whitespace().next().map(str::to_owned);
        } else if line.starts_with(')') {
            table = None;
        } else if let Some(name) = &table {
            let first = line.split_whitespace().next().unwrap_or("");
            if !first.is_empty()
                && !["CHECK", "FOREIGN", "REFERENCES", "UNIQUE"].contains(&first)
                && !first.starts_with("--")
            {
                columns.push((name.clone(), first.to_owned()));
            }
        }
    }
    columns
}

#[test]
fn test_session_log_row_matches_ddl() {
    assert_eq!(SESSION_LOG_SCHEMA_VERSION, 2);
    assert!(SESSION_LOG_DDL.starts_with("-- schema_version 2."));
    assert_eq!(COLUMNS.len(), 18);
    assert_eq!(COLUMNS.iter().collect::<BTreeSet<_>>().len(), 18);

    let columns = ddl_columns();
    let tables: BTreeSet<&str> = columns.iter().map(|(table, _)| table.as_str()).collect();
    assert_eq!(
        tables,
        BTreeSet::from(["answers", "payloads", "question_sets", "sessions"])
    );
    // Every projected key is a DDL column; `first_seen_at` is derived by the loader.
    let ddl: BTreeSet<&str> = columns.iter().map(|(_, name)| name.as_str()).collect();
    let projected: BTreeSet<&str> = COLUMNS.iter().copied().collect();
    let derived: BTreeSet<&str> = ddl.difference(&projected).copied().collect();
    assert_eq!(derived, BTreeSet::from(["first_seen_at"]));
    assert!(projected.is_subset(&ddl));
    // Columns that can vary per send sit on `answers`.
    for varying in [
        "repo",
        "project",
        "decision_schema",
        "request_sha256",
        "model_id",
    ] {
        assert!(
            columns.contains(&("answers".to_owned(), varying.to_owned())),
            "{varying}"
        );
    }
    // The CHECK enums agree with the Rust spellings.
    for operation in Operation::ALL {
        assert!(
            SESSION_LOG_DDL.contains(&format!("'{operation}'")),
            "{operation}"
        );
    }
    for outcome in [SendOutcome::Ok, SendOutcome::Failed] {
        let text = serde_json::to_value(outcome).expect("outcome");
        assert!(SESSION_LOG_DDL.contains(&format!("'{}'", text.as_str().expect("text"))));
    }
    // The fixture's lines carry exactly the columns, and obey the nullability CHECKs.
    for fields in fixture_lines() {
        assert_eq!(
            fields.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            projected
        );
        let ok = fields["send_outcome"] == "ok";
        assert_eq!(ok, fields["error_code"].is_null());
        assert_eq!(ok, !fields["answers_json"].is_null());
        assert!(!fields["model_id"].is_null());
    }
}

#[test]
fn test_fields_roundtrip() {
    let lines = fixture_lines();
    assert_eq!(lines.len(), 7);
    let mut outcomes = Vec::new();
    for fields in &lines {
        let row = SessionLogRow::try_from_fields(fields).expect("fixture row");
        assert_eq!(&row.to_fields(), fields);
        assert_eq!(
            SessionLogRow::try_from_fields(&row.to_fields()).expect("again"),
            row
        );
        outcomes.push((row.send_outcome(), row.context().operation));
    }
    assert_eq!(
        outcomes
            .iter()
            .filter(|(o, _)| *o == SendOutcome::Failed)
            .count(),
        2
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|(_, op)| *op == Operation::Ask)
            .count(),
        2
    );
    let failed_without_body = SessionLogRow::try_from_fields(&lines[2]).expect("row");
    assert!(failed_without_body.context().request_sha256.is_none());
    assert_eq!(
        failed_without_body.error_code(),
        Some(ErrorCode::KeyMissing)
    );
    // Two sends of one session keep their own project.
    let first = SessionLogRow::try_from_fields(&lines[0]).expect("row");
    let sixth = SessionLogRow::try_from_fields(&lines[5]).expect("row");
    assert_eq!(first.context().session_id, sixth.context().session_id);
    assert_ne!(first.context().project, sixth.context().project);

    // A clock reading with sub-millisecond digits truncates, and the row then
    // equals its own round trip.
    let clock = UNIX_EPOCH + Duration::new(1_731_000_000, 123_456_789);
    let ts = TimestampMs::from_system_time(clock);
    assert_eq!(
        ts.as_system_time(),
        UNIX_EPOCH + Duration::new(1_731_000_000, 123_000_000)
    );
    let q = QuestionId::try_from("q").expect("id");
    let context = RowContextBuilder::new()
        .answer_id(AnswerId::new())
        .session_id(SessionId::new())
        .session_created_at(ts)
        .repo("/work/repo")
        .project(Some(ProjectLabel::new("p").expect("label")))
        .payload_sha256(PayloadSha256::of_text("payload"))
        .questions_sha256(QuestionsSha256::of_text("questions"))
        .decision_schema(CANONICAL_V1)
        .question_ids(vec![q.clone()])
        .request_sha256(None)
        .ts(ts)
        .operation(Operation::Judge)
        .backend(BackendId::OpenAiLuna)
        .model_id(ModelId::Gpt6Luna)
        .build()
        .expect("context");
    let row = SessionLogRow::failed(context.clone(), ErrorCode::NetworkError);
    let fields = row.to_fields();
    assert_eq!(fields["ts"], json!("2024-11-07T17:20:00.123Z"));
    assert_eq!(
        SessionLogRow::try_from_fields(&fields).expect("roundtrip"),
        row
    );
    let ok = SessionLogRow::ok(context, DecisionAnswers::new());
    assert_eq!(
        SessionLogRow::try_from_fields(&ok.to_fields()).expect("ok"),
        ok
    );
}

#[test]
fn test_unknown_or_missing_key_rejected() {
    let good = fixture_lines().remove(0);
    let refuse = |fields: &Map<String, Value>| {
        let error = SessionLogRow::try_from_fields(fields).expect_err("refused");
        assert_eq!(error.code(), "INPUT_INVALID");
        error.to_string() + &error.fix_hint()
    };

    let mut extra = good.clone();
    extra.insert("outcome".to_owned(), json!("ok"));
    assert!(refuse(&extra).contains("\"outcome\""));

    for key in COLUMNS {
        let mut missing = good.clone();
        missing.remove(*key);
        assert!(refuse(&missing).contains(key), "{key}");
    }

    // A schema_version other than the supported one is refused with a hint.
    for version in [json!(1), json!(3), json!(0), json!("2"), json!(null)] {
        let mut other = good.clone();
        other.insert("schema_version".to_owned(), version);
        let message = refuse(&other);
        assert!(message.contains("reads version 2"), "{message}");
        assert!(message.contains("written by a newer sc-agent"), "{message}");
    }

    // Wrong types and invalid values name the key.
    for (key, value) in [
        ("answer_id", json!("not-a-ulid")),
        ("session_id", json!(7)),
        ("repo", json!(null)),
        ("project", json!("a".repeat(257))),
        ("project", json!(5)),
        ("payload_sha256", json!("abc")),
        ("questions_sha256", json!("abc")),
        ("decision_schema", json!("canonical.v9")),
        ("question_ids", json!("relevant")),
        ("question_ids", json!([""])),
        ("request_sha256", json!("abc")),
        ("ts", json!("2026-11-07 10:00:01")),
        ("ts", json!("2026-02-30T10:00:01.000Z")),
        ("session_created_at", json!("2026-11-07T25:00:00.000Z")),
        ("operation", json!("think")),
        ("backend", json!("gpt")),
        ("model_id", json!("")),
        ("send_outcome", json!("maybe")),
        ("answers_json", json!([])),
    ] {
        let mut broken = good.clone();
        broken.insert(key.to_owned(), value);
        assert!(refuse(&broken).contains(key), "{key}");
    }

    // The ok/failed pairing is enforced like the DDL CHECKs.
    let mut ok_with_error = good.clone();
    ok_with_error.insert("error_code".to_owned(), json!("NETWORK_ERROR"));
    refuse(&ok_with_error);
    let mut failed_with_answers = good.clone();
    failed_with_answers.insert("send_outcome".to_owned(), json!("failed"));
    failed_with_answers.insert("error_code".to_owned(), json!("NETWORK_ERROR"));
    refuse(&failed_with_answers);
    let mut failed_no_code = fixture_lines().remove(1);
    failed_no_code.insert("error_code".to_owned(), Value::Null);
    refuse(&failed_no_code);
    let mut bad_code = fixture_lines().remove(1);
    bad_code.insert("error_code".to_owned(), json!("NOT_A_CODE"));
    assert!(refuse(&bad_code).contains("error_code"));
}

#[test]
fn test_row_context_builder_names_the_missing_value() {
    let error = RowContextBuilder::new()
        .answer_id(AnswerId::new())
        .build()
        .expect_err("incomplete");
    assert_eq!(error.code(), "INPUT_INVALID");
    assert!(error.to_string().contains("session_id"), "{error}");
}

/// `questions_sha256` identifies a question set whatever the state, and
/// `payload_sha256` fixes it: the same payload never has two question sets,
/// and the same questions can have several payloads (the loader's
/// `payloads.questions_sha256` and the composite key on `answers`).
#[test]
fn test_questions_sha256_groups_payloads_that_differ_by_state() {
    let rows: Vec<SessionLogRow> = fixture_lines()
        .iter()
        .map(|fields| SessionLogRow::try_from_fields(fields).expect("row"))
        .collect();
    let mut by_payload = std::collections::BTreeMap::new();
    for row in &rows {
        let context = row.context();
        let questions = by_payload
            .entry(context.payload_sha256.clone())
            .or_insert_with(|| context.questions_sha256.clone());
        assert_eq!(
            *questions, context.questions_sha256,
            "payload fixes questions"
        );
    }
    let first = rows[0].context();
    let other_state = rows[6].context();
    assert_eq!(first.questions_sha256, other_state.questions_sha256);
    assert_ne!(first.payload_sha256, other_state.payload_sha256);
}
