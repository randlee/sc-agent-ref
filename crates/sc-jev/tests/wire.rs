//! Wire-level digest tests on the real `JevClient` wire builder (REQ-SAG-0606,
//! ADR-SAG-0025). The `test-util` feature supplies `WireFault`; transports
//! come from `sc-agent-http`'s `test-util` test transport.

use std::sync::Arc;
use std::time::Duration;

use sc_agent_http::HttpResponse;
use sc_agent_http::test_util::ScriptedTransport;
use sc_agent_types::{BackendId, BackendKeyId, ResolvedApiKey, ScAgentErrorResponse, SendTrace};
use sc_decision_schema::jev::v1::CheckRequest;
use sc_decision_schema::{CanonicalJudgePayload, JudgeBackend, JudgeRequest, PayloadBundle};
use sc_jev::{JevClient, JevClientConfig, WireFault};
use serde_json::json;

const KEY: &str = "sentinel-key-9f3a";
const PAYLOAD_SENTINEL: &str = "sentinel-payload-text-4d2c";

fn config() -> JevClientConfig {
    JevClientConfig::try_new(
        "jev-test".parse().expect("model"),
        "jev.invalid",
        "/v1/x",
        1,
        Duration::from_secs(1),
    )
    .expect("config")
}

fn bundle() -> PayloadBundle {
    let payload: CanonicalJudgePayload = serde_json::from_value(json!({
        "questions": [{"id": "q", "prompt": PAYLOAD_SENTINEL, "choices": ["no", "yes"]}],
        "state": []
    }))
    .expect("payload");
    PayloadBundle::encode(&payload).expect("bundle")
}

fn ok_reply() -> HttpResponse {
    let body = json!({
        "model": "jev-test",
        "answers": {"q": {"type": "choice", "choice": "no", "confidence": 0.9, "probabilities": {"no": 0.9, "yes": 0.1}}}
    });
    HttpResponse::new(200, Vec::new(), serde_json::to_vec(&body).expect("json"))
}

fn client(transport: &ScriptedTransport) -> JevClient<ScriptedTransport> {
    let key = ResolvedApiKey::try_new(BackendKeyId::TypeSafe, KEY.to_owned()).expect("key");
    JevClient::with_config(config(), key, transport.clone())
}

fn request(bundle: &PayloadBundle) -> JudgeRequest {
    JudgeRequest::new(Arc::new(bundle.clone()), None)
}

#[test]
fn test_real_wire_builder_reports_prompt_digest() {
    let bundle = bundle();
    let transport = ScriptedTransport::responses(vec![ok_reply()]);
    let outcome = client(&transport)
        .judge(&request(&bundle), &SendTrace::new())
        .expect("judge");
    assert_eq!(outcome.input_sha256(), Some(bundle.sha256()));
}

#[test]
fn test_digest_detects_faulty_wire_builder() {
    let bundle = bundle();
    let transport = ScriptedTransport::responses(vec![ok_reply()]);
    let outcome = client(&transport)
        .with_wire_fault(WireFault::DropEmbeddedByte)
        .judge(&request(&bundle), &SendTrace::new())
        .expect("judge");
    let reported = outcome.input_sha256().expect("reported digest");
    assert_ne!(reported, bundle.sha256());
    assert_eq!(transport.sends(), 1);
}

#[test]
fn test_digest_recovery_error_carries_no_body_bytes() {
    let bundle = bundle();
    let transport = ScriptedTransport::responses(vec![ok_reply()]);
    let error = client(&transport)
        .with_wire_fault(WireFault::TruncateBody)
        .judge(&request(&bundle), &SendTrace::new())
        .expect_err("unparseable body");
    assert_eq!(error.code(), "INPUT_INVALID");
    assert_eq!(transport.sends(), 0, "nothing is sent");
    let rendered = format!(
        "{error} {} {}",
        error.message(),
        serde_json::to_string(&ScAgentErrorResponse::from_jev_error(&error)).expect("json")
    );
    assert!(!rendered.contains(KEY), "{rendered}");
    assert!(!rendered.contains(PAYLOAD_SENTINEL), "{rendered}");
}

/// A payload whose question ids and choices are deliberately unsorted.
fn unsorted_bundle() -> PayloadBundle {
    let payload: CanonicalJudgePayload = serde_json::from_value(json!({
        "questions": [
            {"id": "zeta", "prompt": "first", "choices": ["yes", "no"]},
            {"id": "alpha", "prompt": "second", "choices": ["zulu", "alpha", "mid"]},
            {"id": "mid", "prompt": "third", "choices": ["b", "a"]}
        ],
        "state": []
    }))
    .expect("payload");
    PayloadBundle::encode(&payload).expect("bundle")
}

fn unsorted_reply() -> HttpResponse {
    let body = json!({
        "model": "jev-test",
        "answers": {
            "zeta": {"type": "choice", "choice": "yes", "confidence": 0.9,
                     "probabilities": {"yes": 0.9, "no": 0.1}},
            "alpha": {"type": "choice", "choice": "zulu", "confidence": 0.8,
                      "probabilities": {"zulu": 0.8, "alpha": 0.1, "mid": 0.1}},
            "mid": {"type": "choice", "choice": "a", "confidence": 0.7,
                    "probabilities": {"b": 0.3, "a": 0.7}}
        },
        "usage": {}
    });
    HttpResponse::new(200, Vec::new(), serde_json::to_vec(&body).expect("json"))
}

#[test]
fn test_envelope_robust_to_key_ordering() {
    let bundle = unsorted_bundle();
    let transport = ScriptedTransport::responses(vec![unsorted_reply()]);
    client(&transport)
        .judge(&request(&bundle), &SendTrace::new())
        .expect("judge");
    let sent = transport.requests();
    let body = sent[0].body();
    // The body keeps the payload's question order and each criteria order.
    let position = |needle: &str| {
        body.find(needle)
            .unwrap_or_else(|| panic!("{needle}: {body}"))
    };
    assert!(position(r#""zeta":{"#) < position(r#""alpha":{"#));
    assert!(position(r#""alpha":{"type""#) < position(r#""mid":{"type""#));
    assert!(body.contains(r#""criteria":{"zulu":"zulu","alpha":"alpha","mid":"mid"}"#));
    assert!(body.starts_with(r#"{"model":"jev-test","state":[],"questions":{"zeta":"#));
    // It is a valid body: it parses with the order-preserving maps and
    // recovers the payload's digest.
    let parsed = CheckRequest::parse(body).expect("valid body");
    let ids: Vec<&str> = parsed.questions().keys().map(|id| id.as_str()).collect();
    assert_eq!(ids, ["zeta", "alpha", "mid"]);
    assert_eq!(
        parsed.to_canonical().expect("canonical"),
        serde_json::from_slice::<CanonicalJudgePayload>(bundle.bytes()).expect("payload")
    );
}

#[test]
fn test_jev_body_is_check_request_body() {
    let bundle = unsorted_bundle();
    let transport = ScriptedTransport::responses(vec![unsorted_reply()]);
    client(&transport)
        .judge(&request(&bundle), &SendTrace::new())
        .expect("judge");
    let payload: CanonicalJudgePayload = serde_json::from_slice(bundle.bytes()).expect("payload");
    let expected = CheckRequest::from_canonical("jev-test".parse().expect("model"), &payload)
        .expect("request")
        .to_body()
        .expect("body");
    assert_eq!(transport.requests()[0].body(), expected);
}

#[test]
fn test_body_over_limit_is_input_too_large() {
    let payload: CanonicalJudgePayload = serde_json::from_value(json!({
        "questions": [{"id": "q", "prompt": "x".repeat(100_000), "choices": ["no", "yes"]}],
        "state": []
    }))
    .expect("payload");
    let bundle = PayloadBundle::encode(&payload).expect("bundle");
    let transport = ScriptedTransport::responses(vec![ok_reply()]);
    let jev = client(&transport);
    let limit = BackendId::Jev.descriptor().request_limit_bytes();
    for error in [
        jev.validate_for_send(&request(&bundle))
            .expect_err("validate"),
        jev.judge(&request(&bundle), &SendTrace::new())
            .expect_err("judge"),
    ] {
        assert_eq!(error.code(), "INPUT_TOO_LARGE");
        let rendered = error.to_string();
        assert!(rendered.contains(&limit.to_string()), "{rendered}");
    }
    assert_eq!(transport.sends(), 0, "nothing is sent");
}

#[test]
fn test_all_question_types_round_trip_through_the_client() {
    let payload: CanonicalJudgePayload = serde_json::from_value(json!({
        "questions": [
            {"id": "quality", "prompt": "good?", "choices": ["pass", "fail"],
             "descriptions": ["ok", "not ok"]},
            {"id": "severity", "prompt": "how bad?", "choices": ["low", "high"], "kind": "score"},
            {"id": "urgent", "prompt": "urgent?", "choices": [], "kind": "noul"}
        ],
        "state": []
    }))
    .expect("payload");
    let bundle = PayloadBundle::encode(&payload).expect("bundle");
    let body = json!({
        "model": "jev-test",
        "answers": {
            "quality": {"type": "choice", "choice": "pass", "confidence": 0.9,
                        "probabilities": {"pass": 0.9, "fail": 0.1}},
            "severity": {"type": "score", "choice": "high", "confidence": 0.5,
                         "probabilities": {"low": 0.4, "high": 0.6}},
            "urgent": {"type": "noul", "probabilities": {"yes": 0.8}}
        },
        "usage": {}
    });
    let transport = ScriptedTransport::responses(vec![HttpResponse::new(
        200,
        Vec::new(),
        serde_json::to_vec(&body).expect("json"),
    )]);
    let outcome = client(&transport)
        .judge(&request(&bundle), &SendTrace::new())
        .expect("judge");
    assert_eq!(outcome.answers()["urgent"].choice().as_str(), "yes");
    assert_eq!(outcome.answers()["urgent"].confidence(), None);
    assert_eq!(outcome.answers()["severity"].choice().as_str(), "high");
    // Descriptions and kinds survive the body round trip: the digest matches.
    assert_eq!(outcome.input_sha256(), Some(bundle.sha256()));
}
