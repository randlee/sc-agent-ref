//! `test_util::MockJudgeBackend` contract (D10.11, ADR-SAG-0022 r-1).
//!
//! Needs `--features test-util` (`[[test]] required-features`, D0 outcome).

use std::collections::BTreeMap;
use std::sync::Arc;

use sc_agent_types::{BackendId, JevError, ModelId, PayloadSha256, RequestSha256, SendTrace};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_decision_schema::{
    CanonicalJudgePayload, JudgeBackend, JudgeRequest, LlmWireContext, PayloadBundle,
    QuestionAnswer, QuestionId,
};

fn bundle() -> Arc<PayloadBundle> {
    let payload: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
        "questions": [{"id": "q", "prompt": "Is it?", "choices": ["no", "yes"]}],
        "state": []
    }))
    .expect("payload");
    Arc::new(PayloadBundle::encode(&payload).expect("bundle"))
}

fn answers(choice: &str) -> BTreeMap<QuestionId, QuestionAnswer> {
    serde_json::from_value(serde_json::json!({"q": {"choice": choice}})).expect("answers")
}

#[test]
fn test_mock_judge_backend_contract() {
    let payload = bundle();
    let bare = JudgeRequest::new(Arc::clone(&payload), None);
    let prompted = JudgeRequest::new(Arc::clone(&payload), Some(LlmWireContext::new("judge")));

    // Outcome built at judge time against the request; requests recorded;
    // validations counted; input digest is the received bundle's.
    let jev = Arc::new(MockJudgeBackend::new(BackendId::Jev, answers("yes")));
    let registered: Arc<dyn JudgeBackend> = Arc::clone(&jev) as Arc<dyn JudgeBackend>;
    registered.validate_for_send(&bare).expect("validate");
    let outcome = registered.judge(&bare, &SendTrace::new()).expect("judge");
    assert_eq!(outcome.backend(), BackendId::Jev);
    assert_eq!(outcome.answers()["q"].choice().as_str(), "yes");
    assert_eq!(outcome.input_sha256(), Some(payload.sha256()));
    assert_eq!(jev.validations(), 1);
    assert_eq!(jev.requests(), vec![bare.clone()]);

    // Answers that do not match the request's questions are refused.
    let wrong = MockJudgeBackend::new(BackendId::Jev, answers("maybe"));
    assert_eq!(
        wrong
            .judge(&bare, &SendTrace::new())
            .expect_err("choice")
            .code(),
        "RESPONSE_INVALID"
    );

    // Provider meta and a reported digest override.
    let reported = PayloadSha256::of_text("something else");
    let meta = MockJudgeBackend::new(BackendId::Jev, answers("no"))
        .with_provider_meta(serde_json::json!({"model": "mock"}))
        .with_reported_digest(reported.clone());
    let outcome = meta.judge(&bare, &SendTrace::new()).expect("judge");
    assert_eq!(outcome.provider_meta().expect("meta")["model"], "mock");
    assert_eq!(outcome.input_sha256(), Some(&reported));

    // Failure after validation: sent (recorded) and failed.
    let failing = MockJudgeBackend::new(BackendId::Jev, answers("no")).with_failure(
        JevError::provider_busy("busy", 429, 4, sc_agent_types::BusyCause::Sends, None),
    );
    failing.validate_for_send(&bare).expect("validation passes");
    assert_eq!(
        failing
            .judge(&bare, &SendTrace::new())
            .expect_err("fails")
            .code(),
        "PROVIDER_BUSY"
    );
    assert_eq!(failing.requests().len(), 1);

    // Refusal: validate_for_send fails, nothing is recorded.
    let refusing = MockJudgeBackend::refusing(BackendId::Jev, JevError::input_too_large(4, 8));
    assert_eq!(
        refusing
            .validate_for_send(&bare)
            .expect_err("refused")
            .code(),
        "INPUT_TOO_LARGE"
    );
    assert!(refusing.judge(&bare, &SendTrace::new()).is_err());
    assert_eq!(refusing.validations(), 1);
    assert!(refusing.requests().is_empty());

    // LLM ids require LLM context (ADR-SAG-0022, CPR-302).
    let haiku = MockJudgeBackend::new(BackendId::AnthropicHaiku, answers("yes"));
    assert_eq!(
        haiku
            .validate_for_send(&bare)
            .expect_err("no context")
            .code(),
        "INPUT_INVALID"
    );
    assert_eq!(
        haiku
            .judge(&bare, &SendTrace::new())
            .expect_err("no context")
            .code(),
        "INPUT_INVALID"
    );
    assert_eq!(haiku.validations(), 1);
    assert!(haiku.requests().is_empty());
    haiku.validate_for_send(&prompted).expect("with context");
    haiku
        .judge(&prompted, &SendTrace::new())
        .expect("with context");
    assert_eq!(haiku.validations(), 2);
    assert_eq!(haiku.requests().len(), 1);
}

/// `model_id()` defaults to the catalog default of the backend id (D7.5) and
/// `with_model_id` replaces it.
#[test]
fn test_model_id_default_from_catalog() {
    for id in [
        BackendId::Jev,
        BackendId::AnthropicHaiku,
        BackendId::OpenAiLuna,
    ] {
        let mock = MockJudgeBackend::new(id, BTreeMap::new());
        assert_eq!(mock.model_id(), id.descriptor().default_model());
    }
    let pinned: ModelId = "claude-haiku-9".parse().expect("model");
    let mock = MockJudgeBackend::new(BackendId::AnthropicHaiku, BTreeMap::new())
        .with_model_id(pinned.clone());
    assert_eq!(mock.model_id(), pinned);
    // The trait's provided method serves a backend that does not override it.
    struct Plain;
    impl JudgeBackend for Plain {
        fn id(&self) -> BackendId {
            BackendId::OpenAiLuna
        }
        fn validate_for_send(&self, _req: &JudgeRequest) -> Result<(), JevError> {
            Ok(())
        }
        fn judge(
            &self,
            _req: &JudgeRequest,
            _trace: &SendTrace,
        ) -> Result<sc_decision_schema::JudgeOutcome, JevError> {
            Err(JevError::config_invalid("not used"))
        }
    }
    assert_eq!(Plain.model_id(), ModelId::Gpt6Luna);
    // The trait is dyn-compatible.
    let _: &dyn JudgeBackend = &Plain;
}

#[test]
fn test_mock_records_sent_body_digest_into_trace() {
    let request = JudgeRequest::new(bundle(), None);
    let silent = MockJudgeBackend::new(BackendId::Jev, answers("yes"));
    let trace = SendTrace::new();
    silent.judge(&request, &trace).expect("judge");
    assert_eq!(trace.request_sha256(), None);

    let sending = MockJudgeBackend::new(BackendId::Jev, answers("yes")).with_sent_body("{\"a\":1}");
    let trace = SendTrace::new();
    sending.judge(&request, &trace).expect("judge");
    assert_eq!(
        trace.request_sha256(),
        Some(RequestSha256::of_text("{\"a\":1}"))
    );

    let failing = MockJudgeBackend::new(BackendId::Jev, answers("yes"))
        .with_sent_body("{\"a\":1}")
        .with_failure(JevError::input_invalid("boom"));
    let trace = SendTrace::new();
    failing
        .judge(&request, &trace)
        .expect_err("fails after send");
    assert!(trace.request_sha256().is_some());

    let refusing = MockJudgeBackend::refusing(BackendId::Jev, JevError::input_invalid("no"))
        .with_sent_body("{\"a\":1}");
    let trace = SendTrace::new();
    refusing.judge(&request, &trace).expect_err("refused");
    assert_eq!(trace.request_sha256(), None);
}
