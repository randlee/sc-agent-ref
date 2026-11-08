//! Haiku/Luna output on the decision schema (REQ-SAG-0144).

use sc_agent_types::ModelFamily;
use sc_decision_schema::llm::v1;
use sc_decision_schema::{CanonicalJudgePayload, PayloadBundle, Probability, QuestionId};

fn bundle() -> PayloadBundle {
    let payload: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
        "questions": [
            {"id": "a", "prompt": "A?", "choices": ["no", "yes"]},
            {"id": "b", "prompt": "B?", "choices": ["high", "low"]}
        ],
        "state": []
    }))
    .expect("payload");
    PayloadBundle::encode(&payload).expect("bundle")
}

fn parse(text: &str) -> Result<sc_decision_schema::DecisionAnswers, sc_agent_types::JevError> {
    v1::parse_output(ModelFamily::AnthropicClaude, text, &bundle())
}

fn refused(text: &str, message: &str) {
    let error = parse(text).expect_err(text);
    assert_eq!(error.code(), "RESPONSE_INVALID", "{text}");
    assert!(error.to_string().contains(message), "{text}: {error}");
}

#[test]
fn test_llm_output_is_decision_answers() {
    let bundle = bundle();
    assert_eq!(v1::input_bytes(&bundle), bundle.bytes());

    let good = r#"{"answers":{
        "a":{"choice":"yes","confidence":0.9,"probabilities":{"no":0.1,"yes":0.9}},
        "b":{"choice":"low","confidence":0.6,"probabilities":{"high":0.4,"low":0.6}}}}"#;
    let answers = parse(good).expect("valid reply");
    let a = QuestionId::try_from("a").expect("id");
    assert_eq!(answers.len(), 2);
    assert_eq!(answers[&a].choice().as_str(), "yes");
    assert_eq!(answers[&a].confidence().map(Probability::get), Some(0.9));
    // Every ModelFamily reads the same schema today.
    for family in [
        ModelFamily::TypeSafeJev,
        ModelFamily::OpenAiGpt,
        ModelFamily::Local,
        ModelFamily::Unknown,
    ] {
        assert_eq!(
            v1::parse_output(family, good, &bundle).expect("family"),
            answers
        );
    }

    // The four D4.2 cases keep today's messages.
    refused("not json", "LLM reply is not JSON");
    refused("{}", "LLM answer ids differ from the question ids");
    refused(
        r#"{"answers":[]}"#,
        "LLM answer ids differ from the question ids",
    );
    refused(
        r#"{"answers":{"a":{"choice":"yes","confidence":1,"probabilities":{"no":0,"yes":1}}}}"#,
        "LLM answer ids differ from the question ids",
    );
    let need = "needs a criterion choice and a probability per criterion";
    refused(r#"{"answers":{"a":1,"b":1}}"#, "LLM answer a needs");
    refused(
        r#"{"answers":{
            "a":{"choice":"maybe","confidence":1,"probabilities":{"no":0,"yes":1}},
            "b":{"choice":"low","confidence":1,"probabilities":{"high":0,"low":1}}}}"#,
        need,
    );
    refused(
        r#"{"answers":{
            "a":{"choice":"yes","probabilities":{"no":0,"yes":1}},
            "b":{"choice":"low","confidence":1,"probabilities":{"high":0,"low":1}}}}"#,
        need,
    );
    refused(
        r#"{"answers":{
            "a":{"choice":"yes","confidence":1},
            "b":{"choice":"low","confidence":1,"probabilities":{"high":0,"low":1}}}}"#,
        need,
    );
    refused(
        r#"{"answers":{
            "a":{"choice":"yes","confidence":1,"probabilities":{"no":0}},
            "b":{"choice":"low","confidence":1,"probabilities":{"high":0,"low":1}}}}"#,
        need,
    );
    let number = "LLM confidence or probability is not a number";
    refused(
        r#"{"answers":{
            "a":{"choice":"yes","confidence":"high","probabilities":{"no":0,"yes":1}},
            "b":{"choice":"low","confidence":1,"probabilities":{"high":0,"low":1}}}}"#,
        number,
    );
    refused(
        r#"{"answers":{
            "a":{"choice":"yes","confidence":1,"probabilities":{"no":"x","yes":1}},
            "b":{"choice":"low","confidence":1,"probabilities":{"high":0,"low":1}}}}"#,
        number,
    );
    // A number outside [0, 1] is refused by the answer type.
    refused(
        r#"{"answers":{
            "a":{"choice":"yes","confidence":2,"probabilities":{"no":0,"yes":1}},
            "b":{"choice":"low","confidence":1,"probabilities":{"high":0,"low":1}}}}"#,
        "answer a is invalid",
    );
}
