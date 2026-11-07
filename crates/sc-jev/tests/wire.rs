//! Wire-level digest tests on the real `JevClient` wire builder (REQ-SAG-0606,
//! ADR-SAG-0025). The `test-util` feature supplies `WireFault`; transports
//! come from `sc-agent-http`'s `test-util` test transport.

use std::sync::Arc;
use std::time::Duration;

use sc_agent_http::HttpResponse;
use sc_agent_http::test_util::ScriptedTransport;
use sc_agent_types::{
    BackendKeyId, CanonicalJudgePayload, JudgeBackend, JudgeRequest, PayloadBundle, ResolvedApiKey,
    ScAgentErrorResponse,
};
use sc_jev::{JevClient, JevClientConfig, WireFault};
use serde_json::json;

const KEY: &str = "sentinel-key-9f3a";
const PAYLOAD_SENTINEL: &str = "sentinel-payload-text-4d2c";

fn config() -> JevClientConfig {
    JevClientConfig::try_new(
        "jev-test",
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
        "answers": {"q": {"choice": "no", "confidence": 0.9, "probabilities": {"no": 0.9, "yes": 0.1}}}
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
    let outcome = client(&transport).judge(&request(&bundle)).expect("judge");
    assert_eq!(outcome.input_sha256(), Some(bundle.sha256()));
}

#[test]
fn test_digest_detects_faulty_wire_builder() {
    let bundle = bundle();
    let transport = ScriptedTransport::responses(vec![ok_reply()]);
    let outcome = client(&transport)
        .with_wire_fault(WireFault::DropEmbeddedByte)
        .judge(&request(&bundle))
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
        .judge(&request(&bundle))
        .expect_err("unparseable body");
    assert_eq!(error.code(), "INPUT_INVALID");
    assert_eq!(transport.sends(), 0, "nothing is sent");
    let rendered = format!(
        "{error} {} {}",
        error.fix_hint(),
        serde_json::to_string(&ScAgentErrorResponse::from_jev_error(&error)).expect("json")
    );
    assert!(!rendered.contains(KEY), "{rendered}");
    assert!(!rendered.contains(PAYLOAD_SENTINEL), "{rendered}");
}
