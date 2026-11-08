//! `register_scripted_backends` forms (ADR-SAG-0022 r-2b amendment, CPR-208).
//! Needs `--features test-util`.

use std::collections::BTreeMap;
use std::sync::Arc;

use sc_agent_types::{BackendId, NetworkErrorKind, SendTrace};
use sc_decision_schema::{CanonicalJudgePayload, JudgeRequest, LlmWireContext, PayloadBundle};
use sc_jev_check::registry::register_scripted_backends;
use serde_json::{Value, json};

fn request() -> JudgeRequest {
    let payload: CanonicalJudgePayload = serde_json::from_value(json!({
        "questions": [{"id": "relevant", "prompt": "Does it apply?", "choices": ["no", "yes"]}],
        "state": []
    }))
    .expect("payload");
    JudgeRequest::new(
        Arc::new(PayloadBundle::encode(&payload).expect("bundle")),
        Some(LlmWireContext::new("Judge.")),
    )
}

const IDS: [BackendId; 2] = [BackendId::Jev, BackendId::AnthropicHaiku];

#[test]
fn test_scripted_shared_form_unchanged() {
    for answers in [
        json!({"relevant": "yes"}),
        json!({"relevant": {"choice": "yes"}}),
    ] {
        let registry = register_scripted_backends(&IDS, answers).expect("shared form");
        for id in IDS {
            let judge = registry.get(id).expect("registered");
            let outcome = judge.judge(&request(), &SendTrace::new()).expect("judge");
            assert_eq!(outcome.answers()["relevant"].choice().as_str(), "yes");
            assert_eq!(
                outcome.input_sha256(),
                Some(request().payload().sha256()),
                "the scripted backend reports the digest it received"
            );
        }
    }
}

#[test]
fn test_scripted_per_backend_failure() {
    let registry = register_scripted_backends(
        &IDS,
        json!({"backends": {
            "jev": {"failure": {"error": "NETWORK_ERROR", "kind": "busy"}},
            "anthropic_haiku": {"relevant": "yes"}
        }}),
    )
    .expect("per-backend form");
    let failure = registry
        .get(BackendId::Jev)
        .expect("jev")
        .judge(&request(), &SendTrace::new())
        .expect_err("scripted failure");
    assert_eq!(failure.code(), "NETWORK_ERROR");
    assert!(matches!(
        failure,
        sc_agent_types::JevError::NetworkError {
            kind: NetworkErrorKind::Busy,
            ..
        }
    ));
    let outcome = registry
        .get(BackendId::AnthropicHaiku)
        .expect("haiku")
        .judge(&request(), &SendTrace::new())
        .expect("answers");
    assert_eq!(outcome.answers()["relevant"].choice().as_str(), "yes");
}

#[test]
fn test_scripted_validate_stage_not_sent() {
    let registry = register_scripted_backends(
        &IDS,
        json!({"backends": {
            "jev": {"failure": {"error": "INPUT_TOO_LARGE", "stage": "validate"}},
            "anthropic_haiku": {"relevant": "no"}
        }}),
    )
    .expect("per-backend form");
    let jev = registry.get(BackendId::Jev).expect("jev");
    assert_eq!(
        jev.validate_for_send(&request())
            .expect_err("refused")
            .code(),
        "INPUT_TOO_LARGE"
    );
    assert_eq!(
        jev.judge(&request(), &SendTrace::new())
            .expect_err("refused")
            .code(),
        "INPUT_TOO_LARGE"
    );
}

#[test]
fn test_scripted_invalid_form_rejected() {
    let cases: BTreeMap<&str, Value> = BTreeMap::from([
        ("not an object", json!(["yes"])),
        ("only question id is backends", json!({"backends": "yes"})),
        (
            "unknown backend id",
            json!({"backends": {"jev": {"q": "yes"}, "anthropic_haiku": {"q": "no"}, "openai_luna": {"q": "no"}}}),
        ),
        ("missing entry", json!({"backends": {"jev": {"q": "yes"}}})),
        (
            "unknown error",
            json!({"backends": {"jev": {"failure": {"error": "KEY_MISSING"}}, "anthropic_haiku": {"q": "no"}}}),
        ),
        (
            "network error without kind",
            json!({"backends": {"jev": {"failure": {"error": "NETWORK_ERROR"}}, "anthropic_haiku": {"q": "no"}}}),
        ),
        (
            "unknown stage",
            json!({"backends": {"jev": {"failure": {"error": "RESPONSE_INVALID", "stage": "later"}}, "anthropic_haiku": {"q": "no"}}}),
        ),
    ]);
    for (name, answers) in cases {
        let error = register_scripted_backends(&IDS, answers).expect_err(name);
        assert_eq!(error.code(), "INPUT_INVALID", "{name}");
        assert!(
            error.fix_hint().contains("shared form")
                && error.fix_hint().contains("per-backend form"),
            "{name}: the hint names both forms: {}",
            error.fix_hint()
        );
    }
}
