//! `jev::v1` against hand-written golden documents (REQ-SAG-0143, D9).
//!
//! The fixtures in `tests/fixtures/decision-schema/jev` are written from the
//! documented Jev API, not produced by the code under test.

use std::path::PathBuf;

use sc_agent_types::{JevError, ModelId};
use sc_decision_schema::jev::v1::{CheckRequest, CheckResponse, JevState, OrderedMap};
use sc_decision_schema::{CanonicalJudgePayload, ChoiceId, QuestionId};
use serde_json::{Value, json};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/decision-schema/jev")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The compact form of a pretty-printed fixture: whitespace outside strings
/// removed, key order and text untouched.
fn compact(text: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for character in text.chars() {
        if in_string {
            out.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
            out.push(character);
        } else if !character.is_whitespace() {
            out.push(character);
        }
    }
    out
}

fn model() -> ModelId {
    "jev-1.13.0".parse().expect("model")
}

fn payload(value: Value) -> CanonicalJudgePayload {
    serde_json::from_value(value).expect("payload")
}

fn cross_impl_payload() -> CanonicalJudgePayload {
    payload(json!({
        "questions": [
            {"id": "relevant", "prompt": "Does the café policy apply?", "choices": ["yes", "no"]},
            {"id": "risk", "prompt": "Is the change risky?", "choices": ["high", "low"]}
        ],
        "state": [{"question_id": "relevant", "choice": "yes"}]
    }))
}

fn all_types_payload() -> CanonicalJudgePayload {
    payload(json!({
        "questions": [
            {"id": "quality", "prompt": "Is the change good?", "choices": ["pass", "fail"],
             "descriptions": ["Meets the bar", "Does not meet the bar"]},
            {"id": "severity", "prompt": "How severe is the defect?",
             "choices": ["low", "medium", "high"], "kind": "score",
             "descriptions": ["Cosmetic", "Likely to bite", "Breaks users"]},
            {"id": "urgent", "prompt": "Is it urgent?", "choices": [], "kind": "noul"}
        ],
        "state": [{"question_id": "quality", "choice": "pass"}]
    }))
}

fn qid(text: &str) -> QuestionId {
    QuestionId::try_from(text).expect("id")
}

fn choice(text: &str) -> ChoiceId {
    ChoiceId::try_from(text).expect("choice")
}

#[test]
fn test_check_request_body_matches_golden() {
    let request = CheckRequest::from_canonical(model(), &cross_impl_payload()).expect("request");
    assert_eq!(
        request.to_body().expect("body"),
        compact(&fixture("check-request.json"))
    );
}

#[test]
fn test_all_question_types_body_matches_golden() {
    let request = CheckRequest::from_canonical(model(), &all_types_payload()).expect("request");
    let golden = compact(&fixture("check-request-all-types.json"));
    assert_eq!(request.to_body().expect("body"), golden);
    // The body parses back and recovers the canonical payload it came from.
    let parsed = CheckRequest::parse(&golden).expect("parse");
    assert_eq!(parsed, request);
    assert_eq!(
        parsed.to_canonical().expect("canonical"),
        all_types_payload()
    );
}

#[test]
fn test_check_response_parses_golden() {
    let request = CheckRequest::parse(&fixture("check-request.json")).expect("request");
    let response = CheckResponse::parse(fixture("check-response.json").as_bytes()).expect("parse");
    assert_eq!(response.model(), &model());
    assert_eq!(response.usage()["input_tokens"], 410);
    assert!(response.extra().is_empty());
    let answers = response.into_answers(&request).expect("answers");
    let relevant = &answers[&qid("relevant")];
    assert_eq!(relevant.choice().as_str(), "yes");
    assert_eq!(relevant.confidence(), Some(0.94));
    assert_eq!(
        relevant.probabilities().expect("probabilities")[&choice("no")],
        0.04
    );
    assert_eq!(answers[&qid("risk")].choice().as_str(), "low");
}

#[test]
fn test_all_answer_types_parse_golden() {
    let request = CheckRequest::parse(&fixture("check-request-all-types.json")).expect("request");
    let response =
        CheckResponse::parse(fixture("check-response-all-types.json").as_bytes()).expect("parse");
    let answers = response.into_answers(&request).expect("answers");
    let severity = &answers[&qid("severity")];
    assert_eq!(severity.choice().as_str(), "medium");
    assert_eq!(severity.confidence(), Some(0.65));
    // A noul answer is yes when p >= 0.5, else no; no confidence.
    let urgent = &answers[&qid("urgent")];
    assert_eq!(urgent.choice().as_str(), "no");
    assert_eq!(urgent.confidence(), None);
    let probabilities = urgent.probabilities().expect("probabilities");
    assert_eq!(probabilities[&choice("yes")], 0.25);
    assert_eq!(probabilities[&choice("no")], 0.75);

    let at_half = fixture("check-response-all-types.json").replace("0.25", "0.5");
    let answers = CheckResponse::parse(at_half.as_bytes())
        .expect("parse")
        .into_answers(&request)
        .expect("answers");
    assert_eq!(answers[&qid("urgent")].choice().as_str(), "yes");
}

/// Parses `reply` and checks it against the all-types request; the error of
/// either step.
fn rejected(reply: &Value) -> JevError {
    let request = CheckRequest::parse(&fixture("check-request-all-types.json")).expect("request");
    let bytes = serde_json::to_vec(reply).expect("json");
    match CheckResponse::parse(&bytes).and_then(|response| response.into_answers(&request)) {
        Ok(_) => panic!("accepted {reply}"),
        Err(error) => error,
    }
}

fn golden_reply() -> Value {
    serde_json::from_str(&fixture("check-response-all-types.json")).expect("reply")
}

#[test]
fn test_response_rules_rejected() {
    let edit = |change: &dyn Fn(&mut Value)| {
        let mut reply = golden_reply();
        change(&mut reply);
        reply
    };
    let cases: Vec<(&str, Value)> = vec![
        ("model", edit(&|r| r["model"] = json!("jev-9.9.9"))),
        (
            "id mismatch",
            edit(&|r| {
                let answer = r["answers"]["urgent"].take();
                r["answers"].as_object_mut().expect("o").remove("urgent");
                r["answers"]["other"] = answer;
            }),
        ),
        (
            "missing answer",
            edit(&|r| {
                r["answers"].as_object_mut().expect("o").remove("urgent");
            }),
        ),
        (
            "choice outside criteria",
            edit(&|r| r["answers"]["quality"]["choice"] = json!("maybe")),
        ),
        (
            "probability keys differ",
            edit(&|r| r["answers"]["quality"]["probabilities"] = json!({"pass": 0.7, "meh": 0.3})),
        ),
        (
            "probability keys missing",
            edit(&|r| r["answers"]["severity"]["probabilities"] = json!({"low": 1.0})),
        ),
        (
            "probability outside [0, 1]",
            edit(&|r| {
                r["answers"]["quality"]["probabilities"] = json!({"pass": 1.5, "fail": -0.5})
            }),
        ),
        (
            "confidence outside [0, 1]",
            edit(&|r| r["answers"]["quality"]["confidence"] = json!(1.2)),
        ),
        (
            "sum off by more than 1e-6",
            edit(&|r| {
                r["answers"]["quality"]["probabilities"] = json!({"pass": 0.7, "fail": 0.300_01})
            }),
        ),
        (
            "noul extra key",
            edit(&|r| r["answers"]["urgent"]["probabilities"] = json!({"yes": 0.2, "no": 0.8})),
        ),
        (
            "noul without yes",
            edit(&|r| r["answers"]["urgent"]["probabilities"] = json!({"no": 0.8})),
        ),
        (
            "noul yes outside [0, 1]",
            edit(&|r| r["answers"]["urgent"]["probabilities"] = json!({"yes": 1.5})),
        ),
        (
            "answer type differs from question type",
            edit(&|r| r["answers"]["severity"]["type"] = json!("choice")),
        ),
        (
            "noul answer for a choice question",
            edit(&|r| {
                r["answers"]["quality"] = json!({"type": "noul", "probabilities": {"yes": 0.5}});
            }),
        ),
    ];
    for (name, reply) in cases {
        assert_eq!(rejected(&reply).code(), "RESPONSE_INVALID", "{name}");
    }
    // The tolerance is inclusive of rounding: a sum within 1e-6 is accepted.
    let request = CheckRequest::parse(&fixture("check-request-all-types.json")).expect("request");
    let mut near = golden_reply();
    near["answers"]["quality"]["probabilities"] = json!({"pass": 0.7, "fail": 0.300_000_5});
    let bytes = serde_json::to_vec(&near).expect("json");
    CheckResponse::parse(&bytes)
        .expect("parse")
        .into_answers(&request)
        .expect("within tolerance");
    // Not JSON.
    assert_eq!(
        CheckResponse::parse(b"not json").expect_err("json").code(),
        "RESPONSE_INVALID"
    );
}

#[test]
fn test_state_forms_roundtrip() {
    let text = r#"{"model":"jev-1.13.0","state":"plain text","questions":{"q":{"type":"noul","instructions":"Yes?"}}}"#;
    let object = r#"{"model":"jev-1.13.0","state":{"zeta":1,"alpha":{"a":1,"b":2},"mid":[1,2]},"questions":{"q":{"type":"noul","instructions":"Yes?"}}}"#;
    let array = r#"{"model":"jev-1.13.0","state":[1,"two",{"three":3}],"questions":{"q":{"type":"noul","instructions":"Yes?"}}}"#;
    for (body, form) in [(text, "text"), (object, "object"), (array, "array")] {
        let request = CheckRequest::parse(body).expect(form);
        assert_eq!(request.to_body().expect(form), body, "{form}");
        match (form, request.state()) {
            ("text", JevState::Text(value)) => assert_eq!(value, "plain text"),
            ("object", JevState::Object(map)) => {
                let keys: Vec<&str> = map.keys().map(String::as_str).collect();
                assert_eq!(keys, ["zeta", "alpha", "mid"], "top-level key order kept");
            }
            ("array", JevState::Array(items)) => assert_eq!(items.len(), 3),
            other => panic!("wrong state form: {other:?}"),
        }
    }
    // Nested objects are `serde_json` values: their keys come back sorted.
    let nested = object.replace(r#""alpha":{"a":1,"b":2}"#, r#""alpha":{"b":2,"a":1}"#);
    assert_eq!(
        CheckRequest::parse(&nested)
            .expect("nested")
            .to_body()
            .expect("body"),
        object
    );
    // Only a prior-answers array recovers to a canonical payload.
    assert_eq!(
        CheckRequest::parse(text)
            .expect("text")
            .to_canonical()
            .expect_err("state")
            .code(),
        "INPUT_INVALID"
    );
    // A state that is none of the three forms is not a request.
    assert!(CheckRequest::parse(r#"{"model":"jev-1.13.0","state":7,"questions":{}}"#).is_err());
}

#[test]
fn test_duplicate_key_rejected() {
    // An ordered map names the duplicate key.
    let error =
        serde_json::from_str::<OrderedMap<String, u8>>(r#"{"a":1,"a":2}"#).expect_err("duplicate");
    assert!(error.to_string().contains("duplicate key `a`"), "{error}");

    // A request or recovered body repeating a question id or criteria key is
    // INPUT_INVALID with a fix hint naming the key.
    let question = r#"{"type":"choice","instructions":"?","criteria":{"no":"no","yes":"yes"}}"#;
    let repeated_id = format!(
        r#"{{"model":"jev-1.13.0","state":[],"questions":{{"zeta":{question},"zeta":{question}}}}}"#
    );
    let repeated_criterion = r#"{"model":"jev-1.13.0","state":[],"questions":{"q":{"type":"choice","instructions":"?","criteria":{"no":"a","no":"b"}}}}"#;
    for (body, key) in [(repeated_id.as_str(), "zeta"), (repeated_criterion, "no")] {
        let error = CheckRequest::parse(body).expect_err("duplicate");
        assert_eq!(error.code(), "INPUT_INVALID", "{key}");
        assert!(
            error.fix_hint().contains(&format!("duplicate key `{key}`")),
            "{}",
            error.fix_hint()
        );
    }

    // A payload that repeats a question id cannot be mapped (a repeated
    // choice is already refused by `Question::try_new`).
    let two = |label: &str| {
        sc_decision_schema::Question::try_new(
            qid("q"),
            label,
            vec![choice("a"), choice("b")],
            sc_decision_schema::QuestionKind::Choice,
            None,
        )
        .expect("question")
    };
    let repeated_question = CanonicalJudgePayload::new(vec![two("x"), two("y")], Vec::new());
    assert_eq!(
        CheckRequest::from_canonical(model(), &repeated_question)
            .expect_err("id")
            .code(),
        "INPUT_INVALID"
    );

    // A reply repeating an answer id is RESPONSE_INVALID naming the id.
    let reply = r#"{"model":"jev-1.13.0","answers":{"q":{"type":"noul","probabilities":{"yes":0.5}},"q":{"type":"noul","probabilities":{"yes":0.5}}},"usage":{}}"#;
    let error = CheckResponse::parse(reply.as_bytes()).expect_err("duplicate answer");
    assert_eq!(error.code(), "RESPONSE_INVALID");
    assert!(
        error.fix_hint().contains("duplicate key `q`"),
        "{}",
        error.fix_hint()
    );

    // The criteria counts hold on parse: one choice option is refused.
    let one = r#"{"model":"jev-1.13.0","state":[],"questions":{"q":{"type":"choice","instructions":"?","criteria":{"only":"only"}}}}"#;
    assert_eq!(
        CheckRequest::parse(one).expect_err("count").code(),
        "INPUT_INVALID"
    );
}

/// A malformed caller body is a fix-the-body error, not an encoder bug: the
/// hint carries the category and position only.
#[test]
fn test_malformed_body_hint_is_fix_the_body() {
    let secret = "SECRET-BODY-TEXT";
    for error in [
        CheckRequest::parse(&format!(r#"{{"model": "{secret}", "#)).expect_err("eof"),
        CheckRequest::parse(r#"{"model":"jev-1.13.0"}"#).expect_err("data"),
        CanonicalJudgePayload::from_bytes(format!("{{{secret}").as_bytes()).expect_err("syntax"),
    ] {
        assert_eq!(error.code(), "INPUT_INVALID");
        let hint = error.fix_hint();
        assert!(hint.contains("fix the"), "{hint}");
        assert!(!hint.contains("encoded"), "{hint}");
        assert!(!hint.contains("bug"), "{hint}");
        assert!(!hint.contains(secret), "{hint}");
    }
}
