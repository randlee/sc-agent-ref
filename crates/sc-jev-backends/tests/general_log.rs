//! The general log of the LLM backends: busy waits and failures are logged,
//! and neither the key nor the prompt text reaches it (NFR-SAG-0002).

use std::sync::Arc;
use std::time::Duration;

use sc_agent_http::test_util::{FakeClock, RecordingSleeper, ScriptedTransport};
use sc_agent_http::{BusyPolicy, HttpResponse};
use sc_agent_types::observability::v2::{RecordingGeneralLog, RejectingGeneralLog};
use sc_agent_types::{BackendKeyId, ResolvedApiKey, SendTrace};
use sc_decision_schema::{
    CanonicalJudgePayload, JudgeBackend, JudgeRequest, LlmWireContext, PayloadBundle,
};
use sc_jev_backends::AnthropicHaikuBackend;
use serde_json::{Value, json};

const PROMPT: &str = "SECRET-PROMPT-TEXT-7731";
const QUESTION: &str = "SECRET-QUESTION-TEXT-9912";
const KEY: &str = "sk-ant-SECRET-KEY-4455";

fn reply(status: u16, headers: Vec<(&str, &str)>, body: &Value) -> HttpResponse {
    HttpResponse::new(
        status,
        headers
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect(),
        body.to_string().into_bytes(),
    )
}

fn request() -> JudgeRequest {
    let payload: CanonicalJudgePayload = serde_json::from_value(json!({
        "questions": [{"id": "q", "prompt": QUESTION, "choices": ["yes", "no"]}],
        "state": [],
    }))
    .expect("payload");
    JudgeRequest::new(
        Arc::new(PayloadBundle::encode(&payload).expect("bundle")),
        Some(LlmWireContext::new(PROMPT)),
    )
}

fn backend(replies: Vec<HttpResponse>) -> AnthropicHaikuBackend<ScriptedTransport> {
    let clock = FakeClock::new();
    let sleeper = RecordingSleeper::new(clock.clone());
    let policy = BusyPolicy::with_test_hooks(clock, sleeper, Duration::ZERO);
    AnthropicHaikuBackend::new(
        ResolvedApiKey::try_new(BackendKeyId::Anthropic, KEY.to_owned()).expect("key"),
        ScriptedTransport::responses(replies),
    )
    .with_busy_policy(policy)
}

fn answered() -> Vec<HttpResponse> {
    let text = json!({"answers": {"q": {"choice": "no", "confidence": 0.7,
        "probabilities": {"yes": 0.3, "no": 0.7}}}})
    .to_string();
    vec![
        reply(
            429,
            vec![("retry-after", "2")],
            &json!({"error": {"message": format!("echo {KEY} {PROMPT}")}}),
        ),
        reply(200, Vec::new(), &json!({"input_tokens": 10})),
        reply(
            200,
            Vec::new(),
            &json!({
                "stop_reason": "end_turn",
                "content": [{"type": "text", "text": text}],
                "usage": {"input_tokens": 10, "output_tokens": 5}
            }),
        ),
    ]
}

#[test]
fn busy_wait_and_judge_are_logged_without_key_prompt_or_body() {
    let log = RecordingGeneralLog::new();
    let backend = backend(answered()).with_general_log(Arc::new(log.clone()));
    backend
        .judge(&request(), &SendTrace::new())
        .expect("answered");
    let actions = log.actions();
    for expected in [
        "judge.start",
        "http.send",
        "http.busy_wait",
        "judge.counted",
        "judge.done",
    ] {
        assert!(actions.contains(&expected), "{expected} in {actions:?}");
    }
    let wait = log
        .events()
        .into_iter()
        .find(|event| event.action() == "http.busy_wait")
        .expect("wait");
    assert_eq!(wait.fields()["status"], 429);
    assert_eq!(wait.fields()["wait_ms"], 2000);
    let rendered = log.rendered();
    for secret in [KEY, PROMPT, QUESTION, "echo"] {
        assert!(!rendered.contains(secret), "{secret} leaked: {rendered}");
    }
}

#[test]
fn a_failure_is_logged_with_its_typed_id_only() {
    let log = RecordingGeneralLog::new();
    let backend = backend(vec![reply(
        401,
        Vec::new(),
        &json!({"error": {"message": format!("bad key {KEY}")}}),
    )])
    .with_general_log(Arc::new(log.clone()));
    let error = backend
        .judge(&request(), &SendTrace::new())
        .expect_err("401");
    let failed = log
        .events()
        .into_iter()
        .find(|event| event.action() == "judge.failed")
        .expect("failed event");
    assert_eq!(failed.fields()["error_code"], error.code().as_str());
    let rendered = log.rendered();
    assert!(!rendered.contains(KEY), "{rendered}");
    assert!(!rendered.contains("bad key"), "{rendered}");
}

#[test]
fn a_rejecting_log_changes_nothing() {
    let silent = backend(answered())
        .judge(&request(), &SendTrace::new())
        .expect("silent");
    let rejecting = backend(answered())
        .with_general_log(Arc::new(RejectingGeneralLog))
        .judge(&request(), &SendTrace::new())
        .expect("rejecting");
    assert_eq!(silent.answers(), rejecting.answers());
}
