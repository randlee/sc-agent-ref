//! Check/judge body encoding and response parsing.
//!
//! The payload bytes are posted unchanged, with the pinned `model` key spliced
//! in front. This client does not re-canonicalize those bytes for send
//! (ADR-SAG-0005). A `PayloadBundle` is canonical by construction
//! (ADR-SAG-0002), so there is no canonical-form oracle; its typed question
//! map is the reply criteria. [`splice_model`] copies the original payload.

use std::collections::{BTreeMap, BTreeSet};

use sc_agent_types::{
    BackendId, ChoiceId, JevError, JudgeOutcome, JudgeRequest, PayloadBundle, QuestionAnswer,
    QuestionId, Sha256Digest, canonical_json_bytes,
};
use serde_json::Value;

use crate::config::JevClientConfig;

/// Wire body plus the criteria used to validate the reply.
pub(crate) struct PreparedCheck {
    pub(crate) body: Vec<u8>,
    pub(crate) criteria: BTreeMap<QuestionId, BTreeSet<ChoiceId>>,
}

/// Validates `payload` and builds the byte-exact check body.
///
/// The wire body is the bundle's canonical bytes with the pinned model
/// spliced in front. This function does not re-encode those bytes for send.
/// The bundle is canonical by construction (ADR-SAG-0002), and its typed
/// question map is the reply criteria.
///
/// # Errors
///
/// Returns [`JevError::InputInvalid`] when the payload has no question or a
/// question has fewer than two distinct choices (a Jev check needs a choice
/// question with two or more criteria). Returns [`JevError::InputTooLarge`]
/// when the wire body exceeds `limit_bytes` (the Jev descriptor's
/// `request_limit_bytes`).
pub(crate) fn prepare(
    config: &JevClientConfig,
    limit_bytes: u64,
    payload: &PayloadBundle,
) -> Result<PreparedCheck, JevError> {
    let criteria = payload.question_choices();
    if criteria.is_empty() {
        return Err(JevError::input_invalid(
            "a check needs at least one question",
        ));
    }
    if let Some(id) = criteria
        .iter()
        .find_map(|(id, choices)| (choices.len() < 2).then_some(id))
    {
        return Err(JevError::input_invalid(format!(
            "question {id} is not a choice question with two or more criteria"
        )));
    }
    let body = splice_model(config.model(), payload.bytes());
    let actual_bytes = body.len() as u64;
    if actual_bytes > limit_bytes {
        return Err(JevError::input_too_large(limit_bytes, actual_bytes));
    }
    Ok(PreparedCheck {
        body,
        criteria: criteria.clone(),
    })
}

/// The digest of the canonical blob inside a serialized Jev wire `body`
/// (REQ-SAG-0606, ADR-SAG-0025).
///
/// Parses `body`, removes the pinned `model` key, re-encodes the rest with
/// `canonical_json_bytes` and hashes it. The re-encoding is attestation-only:
/// it never feeds the wire body (ADR-SAG-0005), and the digest is computed
/// before the body is handed to `send_with_busy_retry`.
///
/// # Errors
///
/// `INPUT_INVALID` when `body` is not a JSON object. The error names the
/// failure category and position only, never body bytes (ADR-SAG-0020).
pub(crate) fn recovered_digest(body: &[u8]) -> Result<Sha256Digest, JevError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| JevError::encode_failed("Jev wire body", &error))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| JevError::input_invalid("Jev wire body is not a JSON object"))?;
    object.remove("model");
    let canonical = canonical_json_bytes(&value)?;
    Ok(Sha256Digest::of(&canonical))
}

/// A seeded defect in the Jev wire builder (feature `test-util`).
///
/// The fault is applied after the canonical bytes are embedded in the wire
/// body and before the digest is recovered from it, so a test proves the
/// reported digest comes from the bytes actually sent.
#[cfg(feature = "test-util")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireFault {
    /// Drops one byte of the embedded payload text. The body stays valid
    /// JSON, so recovery succeeds and reports a digest that differs from the
    /// request's `prompt_sha256`.
    DropEmbeddedByte,
    /// Cuts the serialized body in half, so digest recovery fails and the
    /// backend sends nothing.
    TruncateBody,
}

#[cfg(feature = "test-util")]
impl WireFault {
    /// Applies the fault to `body`, whose embedded payload starts at
    /// `payload_start`.
    pub(crate) fn apply(self, mut body: Vec<u8>, payload_start: usize) -> Vec<u8> {
        match self {
            Self::DropEmbeddedByte => {
                if let Some(index) = droppable_byte(&body, payload_start) {
                    body.remove(index);
                }
            }
            Self::TruncateBody => body.truncate(body.len() / 2),
        }
        body
    }
}

/// Index of the first ASCII letter or digit inside a JSON string at or after
/// `from`, skipping escape sequences, so removing it keeps the JSON valid.
#[cfg(feature = "test-util")]
fn droppable_byte(body: &[u8], from: usize) -> Option<usize> {
    let mut in_string = false;
    let mut index = from;
    while index < body.len() {
        let byte = body[index];
        match byte {
            b'\\' if in_string => index += 1,
            b'"' => in_string = !in_string,
            b if in_string && b.is_ascii_alphanumeric() => return Some(index),
            _ => {}
        }
        index += 1;
    }
    None
}

/// Parses a Jev check response into a [`JudgeOutcome`].
///
/// Details carry no "Jev" label; `JevClient` prefixes every exchange error
/// with it.
///
/// `provider_meta` is that response object with every top-level field kept.
/// The CLI prints it plus `session_id` (ADR-SAG-0014).
///
/// # Errors
///
/// Returns [`JevError::ResponseInvalid`] when the model, answer ids, choice,
/// or probability keys do not match the request, or a confidence or
/// probability is not a finite number in `[0, 1]`.
pub(crate) fn parse_outcome(
    raw: &[u8],
    model: &str,
    criteria: &BTreeMap<QuestionId, BTreeSet<ChoiceId>>,
    request: &JudgeRequest,
) -> Result<JudgeOutcome, JevError> {
    let value: Value = serde_json::from_slice(raw)
        .map_err(|_| JevError::response_invalid("response is not JSON"))?;
    let body = value
        .as_object()
        .ok_or_else(|| JevError::response_invalid("unexpected Jev response model or shape"))?;
    if body.get("model").and_then(Value::as_str) != Some(model) {
        return Err(JevError::response_invalid(
            "unexpected Jev response model or shape",
        ));
    }
    let answers = body
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| JevError::response_invalid("unexpected Jev response model or shape"))?;
    let expected: BTreeSet<&str> = criteria.keys().map(QuestionId::as_str).collect();
    let actual: BTreeSet<&str> = answers.keys().map(String::as_str).collect();
    if expected != actual {
        return Err(JevError::response_invalid(
            "answer ids differ from the question ids",
        ));
    }

    let mut typed = BTreeMap::new();
    for (question, allowed) in criteria {
        let id = question.as_str();
        let not_a_criterion =
            || JevError::response_invalid(format!("answer {id} is not one of its criteria"));
        let answer = answers
            .get(id)
            .and_then(Value::as_object)
            .ok_or_else(not_a_criterion)?;
        let choice = answer
            .get("choice")
            .and_then(Value::as_str)
            .and_then(|label| allowed.iter().find(|choice| choice.as_str() == label))
            .ok_or_else(not_a_criterion)?;
        let confidence = require_number(answer.get("confidence"))?;
        let not_exact = || {
            JevError::response_invalid(format!(
                "answer {id} probabilities are not exactly the question criteria"
            ))
        };
        let probs = answer
            .get("probabilities")
            .and_then(Value::as_object)
            .ok_or_else(not_exact)?;
        let got: BTreeSet<&str> = probs.keys().map(String::as_str).collect();
        let want: BTreeSet<&str> = allowed.iter().map(ChoiceId::as_str).collect();
        if got != want {
            return Err(not_exact());
        }
        let mut probabilities = BTreeMap::new();
        for label in allowed {
            probabilities.insert(label.clone(), require_number(probs.get(label.as_str()))?);
        }
        let answer = QuestionAnswer::new(choice.clone(), Some(confidence), Some(probabilities))?;
        typed.insert(question.clone(), answer);
    }

    Ok(JudgeOutcome::new(BackendId::Jev, request, typed)?
        .with_provider_meta(Value::Object(body.clone())))
}

/// `{"model":<model>,` followed by `payload` without its opening brace.
///
/// A bundle's bytes are a canonical object with `questions` and `state`, so
/// they are never `{}` and always start with `{`.
fn splice_model(model: &str, payload: &[u8]) -> Vec<u8> {
    debug_assert!(payload.len() > 2 && payload[0] == b'{');
    let model_json = Value::String(model.to_owned()).to_string();
    let mut body = Vec::with_capacity(payload.len() + model_json.len() + 10);
    body.extend_from_slice(br#"{"model":"#);
    body.extend_from_slice(model_json.as_bytes());
    body.push(b',');
    body.extend_from_slice(&payload[1..]);
    body
}

fn require_number(value: Option<&Value>) -> Result<f64, JevError> {
    match value.and_then(Value::as_f64) {
        Some(number) => Ok(number),
        _ => Err(JevError::response_invalid(
            "confidence or probability is not a number",
        )),
    }
}

#[cfg(test)]
mod response_shape {
    use super::parse_outcome;

    #[test]
    fn parse_outcome_keeps_other_top_level_fields() {
        let raw = br#"{"model":"jev-test","answers":{"q":{"choice":"no","confidence":0.9,"probabilities":{"no":0.9,"yes":0.1}}},"usage":{"input_tokens":318,"output_tokens":31},"stop_reason":"end_turn"}"#;
        let payload: sc_agent_types::CanonicalJudgePayload =
            serde_json::from_value(serde_json::json!({
                "questions": [{"id": "q", "prompt": "p", "choices": ["no", "yes"]}],
                "state": []
            }))
            .expect("payload");
        let request = sc_agent_types::JudgeRequest::new(
            std::sync::Arc::new(sc_agent_types::PayloadBundle::encode(&payload).expect("bundle")),
            None,
        );
        let criteria = request.payload().question_choices();
        let outcome = parse_outcome(raw, "jev-test", criteria, &request).expect("parse");
        let meta = outcome.provider_meta().expect("meta");
        assert_eq!(meta["stop_reason"], "end_turn");
        assert_eq!(meta["model"], "jev-test");
        assert_eq!(meta["usage"]["input_tokens"], 318);
        assert_eq!(meta["answers"]["q"]["choice"], "no");
        assert_eq!(outcome.answers()["q"].choice().as_str(), "no");
    }
}

/// Refusal paths of `prepare` and `parse_outcome` (QA-1 QA-001: the r-3
/// rewire moved the loopback client tests out of this crate).
#[cfg(test)]
mod refusals {
    use std::sync::Arc;

    use sc_agent_types::{CanonicalJudgePayload, JudgeRequest, PayloadBundle};
    use serde_json::{Value, json};

    use super::{parse_outcome, prepare};
    use crate::config::JevClientConfig;

    fn bundle(questions: Value) -> PayloadBundle {
        let payload: CanonicalJudgePayload =
            serde_json::from_value(json!({"questions": questions, "state": []})).expect("payload");
        PayloadBundle::encode(&payload).expect("bundle")
    }

    fn array_question() -> Value {
        json!([{"id": "q", "prompt": "p", "choices": ["no", "yes"]}])
    }

    fn refused(bundle: &PayloadBundle, limit: u64) -> String {
        match prepare(&JevClientConfig::packaged(), limit, bundle) {
            Ok(_) => panic!("accepted {}", bundle.as_str()),
            Err(error) => format!("{} {error}", error.code()),
        }
    }

    #[test]
    fn prepare_refuses_every_malformed_payload() {
        let cases = [
            (json!([]), "at least one question"),
            (
                json!([{"id": "q", "prompt": "p", "choices": ["a"]}]),
                "question q is not a choice",
            ),
            (
                json!([{"id": "q", "prompt": "p", "choices": ["a", "a"]}]),
                "question q is not a choice",
            ),
        ];
        for (questions, needle) in cases {
            let rendered = refused(&bundle(questions), 96_000);
            assert!(rendered.starts_with("INPUT_INVALID"), "{rendered}");
            assert!(rendered.contains(needle), "{needle}: {rendered}");
        }
        assert!(refused(&bundle(array_question()), 10).starts_with("INPUT_TOO_LARGE"));
    }

    #[test]
    fn prepare_builds_typed_criteria_and_the_byte_exact_body() {
        let bundle = bundle(array_question());
        let prepared = prepare(&JevClientConfig::packaged(), 96_000, &bundle).expect("accepted");
        assert!(
            prepared
                .body
                .starts_with(br#"{"model":"jev-1.13.0","questions":["#)
        );
        assert_eq!(&prepared.criteria, bundle.question_choices());
    }

    fn request() -> JudgeRequest {
        JudgeRequest::new(Arc::new(bundle(array_question())), None)
    }

    #[test]
    fn parse_outcome_refuses_every_malformed_answer() {
        let answer = |q: Value| json!({"model": "jev-test", "answers": {"q": q}}).to_string();
        let cases: Vec<(String, &str)> = vec![
            ("[1]".to_owned(), "model or shape"),
            (json!({"model": "jev-test"}).to_string(), "model or shape"),
            (answer(json!(1)), "not one of its criteria"),
            (answer(json!({"confidence": 1})), "not one of its criteria"),
            (
                answer(json!({"choice": "maybe"})),
                "not one of its criteria",
            ),
            (
                answer(
                    json!({"choice": "no", "confidence": "high", "probabilities": {"no": 1, "yes": 0}}),
                ),
                "not a number",
            ),
            (
                answer(json!({"choice": "no", "confidence": 1})),
                "probabilities are not exactly",
            ),
            (
                answer(
                    json!({"choice": "no", "confidence": 1, "probabilities": {"no": "x", "yes": 0}}),
                ),
                "not a number",
            ),
        ];
        let request = request();
        for (raw, needle) in cases {
            let error = parse_outcome(
                raw.as_bytes(),
                "jev-test",
                request.payload().question_choices(),
                &request,
            )
            .expect_err(needle);
            assert_eq!(error.code(), "RESPONSE_INVALID", "{raw}");
            assert!(error.to_string().contains(needle), "{needle}: {error}");
        }
    }
}
