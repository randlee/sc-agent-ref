//! Wire-level digest tests on the real Haiku and Luna backends
//! (REQ-SAG-0606, ADR-SAG-0025). The `test-util` feature supplies
//! `WireFault`; transports come from `sc-agent-http`'s `test-util` test
//! transport.

use std::sync::Arc;

use sc_agent_http::HttpResponse;
use sc_agent_http::test_util::ScriptedTransport;
use sc_agent_types::{BackendKeyId, ResolvedApiKey, ScAgentErrorResponse, SendTrace};
use sc_decision_schema::{
    CanonicalJudgePayload, JudgeBackend, JudgeRequest, LlmWireContext, PayloadBundle,
};
use sc_jev_backends::{AnthropicHaikuBackend, OpenAiLunaBackend, WireFault};
use serde_json::{Value, json};

const KEY: &str = "sentinel-key-9f3a";
const PAYLOAD_SENTINEL: &str = "sentinel-payload-text-4d2c";

fn bundle() -> PayloadBundle {
    let payload: CanonicalJudgePayload = serde_json::from_value(json!({
        "questions": [{"id": "q", "prompt": PAYLOAD_SENTINEL, "choices": ["no", "yes"]}],
        "state": []
    }))
    .expect("payload");
    PayloadBundle::encode(&payload).expect("bundle")
}

fn request(bundle: &PayloadBundle) -> JudgeRequest {
    JudgeRequest::new(
        Arc::new(bundle.clone()),
        Some(LlmWireContext::new("Judge.")),
    )
}

fn json_reply(body: &Value) -> HttpResponse {
    HttpResponse::new(200, Vec::new(), serde_json::to_vec(body).expect("json"))
}

fn answer_text() -> String {
    json!({"answers": {"q": {"choice": "no", "confidence": 0.7,
        "probabilities": {"yes": 0.3, "no": 0.7}}}})
    .to_string()
}

fn haiku_replies() -> ScriptedTransport {
    ScriptedTransport::responses(vec![
        json_reply(&json!({"input_tokens": 10})),
        json_reply(&json!({
            "stop_reason": "end_turn",
            "content": [{"type": "text", "text": answer_text()}],
            "usage": {"input_tokens": 10}
        })),
    ])
}

fn luna_replies() -> ScriptedTransport {
    ScriptedTransport::responses(vec![
        json_reply(&json!({"input_tokens": 10})),
        json_reply(&json!({
            "status": "completed",
            "output": [{"type": "message", "content": [{"type": "output_text", "text": answer_text()}]}],
            "usage": {"input_tokens": 10}
        })),
    ])
}

fn haiku(transport: &ScriptedTransport) -> AnthropicHaikuBackend<ScriptedTransport> {
    let key = ResolvedApiKey::try_new(BackendKeyId::Anthropic, KEY.to_owned()).expect("key");
    AnthropicHaikuBackend::new(key, transport.clone())
}

fn luna(transport: &ScriptedTransport) -> OpenAiLunaBackend<ScriptedTransport> {
    let key = ResolvedApiKey::try_new(BackendKeyId::OpenAi, KEY.to_owned()).expect("key");
    OpenAiLunaBackend::new(key, transport.clone())
}

#[test]
fn test_real_wire_builder_reports_prompt_digest() {
    let bundle = bundle();
    let transport = haiku_replies();
    let outcome = haiku(&transport)
        .judge(&request(&bundle), &SendTrace::new())
        .expect("haiku");
    assert_eq!(outcome.input_sha256(), Some(bundle.sha256()));
    let transport = luna_replies();
    let outcome = luna(&transport)
        .judge(&request(&bundle), &SendTrace::new())
        .expect("luna");
    assert_eq!(outcome.input_sha256(), Some(bundle.sha256()));
}

#[test]
fn test_digest_detects_faulty_wire_builder() {
    let bundle = bundle();
    let transport = haiku_replies();
    let outcome = haiku(&transport)
        .with_wire_fault(WireFault::DropEmbeddedByte)
        .judge(&request(&bundle), &SendTrace::new())
        .expect("haiku");
    assert_ne!(outcome.input_sha256().expect("digest"), bundle.sha256());
    let transport = luna_replies();
    let outcome = luna(&transport)
        .with_wire_fault(WireFault::DropEmbeddedByte)
        .judge(&request(&bundle), &SendTrace::new())
        .expect("luna");
    assert_ne!(outcome.input_sha256().expect("digest"), bundle.sha256());
}

#[test]
fn test_digest_recovery_error_carries_no_body_bytes() {
    let bundle = bundle();
    for (name, backend_error, sends) in [
        ("haiku", {
            let transport = haiku_replies();
            let error = haiku(&transport)
                .with_wire_fault(WireFault::TruncateBody)
                .judge(&request(&bundle), &SendTrace::new())
                .expect_err("unparseable body");
            (error, transport.sends())
        }),
        ("luna", {
            let transport = luna_replies();
            let error = luna(&transport)
                .with_wire_fault(WireFault::TruncateBody)
                .judge(&request(&bundle), &SendTrace::new())
                .expect_err("unparseable body");
            (error, transport.sends())
        }),
    ]
    .map(|(name, (error, sends))| (name, error, sends))
    {
        assert_eq!(backend_error.code(), "INPUT_INVALID", "{name}");
        assert_eq!(sends, 0, "{name}: nothing is sent");
        let rendered = format!(
            "{backend_error} {} {}",
            backend_error.message(),
            serde_json::to_string(&ScAgentErrorResponse::from_jev_error(&backend_error))
                .expect("json")
        );
        assert!(!rendered.contains(KEY), "{name}: {rendered}");
        assert!(!rendered.contains(PAYLOAD_SENTINEL), "{name}: {rendered}");
    }
}
