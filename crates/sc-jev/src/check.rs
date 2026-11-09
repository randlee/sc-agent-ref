//! Check/judge body encoding and response parsing.
//!
//! The body is the documented Jev object `{model, state, questions}`, built
//! by serde from the canonical payload ([`CheckRequest`]); replies parse
//! through [`CheckResponse`] (REQ-SAG-0143). `payload_sha256` stays on the
//! canonical bytes (ADR-SAG-0002); `input_sha256` is recovered by inverting
//! the final body back to canonical bytes (ADR-SAG-0025).

use sc_agent_types::{
    BackendId, JevError, ModelId, PayloadSha256, RequestSha256, ResponseInvalidCause,
};
use sc_decision_schema::jev::v1::{CheckRequest, CheckResponse};
use sc_decision_schema::{CanonicalJudgePayload, JudgeOutcome, JudgeRequest, PayloadBundle};
use serde_json::Value;

use crate::config::JevClientConfig;

/// Wire body plus the request used to validate the reply.
pub(crate) struct PreparedCheck {
    pub(crate) body: String,
    pub(crate) request: CheckRequest,
}

/// Validates `payload` and builds the Jev check body.
///
/// The payload is validated by deserializing its bytes (the one validator,
/// [`CanonicalJudgePayload::from_bytes`]); the body is then
/// [`CheckRequest::to_body`] for the pinned model.
///
/// # Errors
///
/// Returns [`JevError::InputInvalid`] when the payload has no question or
/// breaks a question rule (`choice` has 2 to 255 choices, `score` 2 to 10).
/// Returns [`JevError::InputTooLarge`] when the wire body exceeds
/// `limit_bytes` (the Jev descriptor's `request_limit_bytes`).
pub(crate) fn prepare(
    config: &JevClientConfig,
    limit_bytes: u64,
    payload: &PayloadBundle,
) -> Result<PreparedCheck, JevError> {
    let canonical = CanonicalJudgePayload::from_bytes(payload.bytes())?;
    let request = CheckRequest::from_canonical(config.model().clone(), &canonical)?;
    let body = request.to_body()?;
    let actual_bytes = body.len() as u64;
    if actual_bytes > limit_bytes {
        return Err(JevError::input_too_large(limit_bytes, actual_bytes));
    }
    Ok(PreparedCheck { body, request })
}

/// The SHA-256 of the exact body `send` would post for `payload` and `model`,
/// without a key or a send (the digest `SendTrace` records and the session-log
/// `request_sha256` column holds).
///
/// The size limit is not applied, so a dry run can report the digest of an
/// over-limit body next to its `over_limit` flag.
///
/// # Errors
///
/// As [`CheckRequest::from_canonical`].
pub(crate) fn request_sha256(
    model: &ModelId,
    payload: &PayloadBundle,
) -> Result<RequestSha256, JevError> {
    let canonical = CanonicalJudgePayload::from_bytes(payload.bytes())?;
    let request = CheckRequest::from_canonical(model.clone(), &canonical)?;
    Ok(RequestSha256::of_text(&request.to_body()?))
}

/// The digest of the canonical blob recovered from a serialized Jev wire
/// `body` (REQ-SAG-0606, ADR-SAG-0025).
///
/// Parses `body` with [`CheckRequest`]'s order-preserving maps, maps each Jev
/// question back to a canonical question, canonical-encodes and hashes. The
/// recovery is attestation-only: it never feeds the wire body, and the digest
/// is computed before the body is handed to `send_with_busy_retry`. For a
/// body built by [`CheckRequest::from_canonical`] it equals `payload_sha256`.
///
/// # Errors
///
/// `INPUT_INVALID` when `body` is not a check request. The error names the
/// failure category and position (or a repeated key), never body bytes
/// (ADR-SAG-0020).
pub(crate) fn recovered_digest(body: &str) -> Result<PayloadSha256, JevError> {
    Ok(CheckRequest::parse(body)?
        .recovered_bundle()?
        .sha256()
        .clone())
}

/// A seeded defect in the Jev wire builder (feature `test-util`).
///
/// The fault is applied after the body is built and before the digest is recovered from it, so a test proves the
/// reported digest comes from the bytes actually sent.
#[cfg(feature = "test-util")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireFault {
    /// Drops one letter of the first question's `instructions` value. The
    /// body stays a valid check request, so recovery succeeds and reports a digest that differs from the
    /// request's `payload_sha256`.
    DropEmbeddedByte,
    /// Cuts the serialized body in half, so digest recovery fails and the
    /// backend sends nothing.
    TruncateBody,
}

#[cfg(feature = "test-util")]
impl WireFault {
    /// Applies the fault to `body`.
    pub(crate) fn apply(self, mut body: String) -> String {
        match self {
            Self::DropEmbeddedByte => {
                // Only an ASCII letter or digit is removed, so UTF-8 stays valid.
                if let Some(index) = droppable_byte(body.as_bytes()) {
                    body.remove(index);
                }
            }
            Self::TruncateBody => {
                let half = body.floor_char_boundary(body.len() / 2);
                body.truncate(half);
            }
        }
        body
    }
}

/// Opening of the first question's text. Dropping a byte of that value
/// changes the recovered digest and keeps the body a valid check request
/// (a dropped key letter would make recovery fail instead).
#[cfg(feature = "test-util")]
const INSTRUCTIONS_MARKER: &str = "\"instructions\":\"";

/// Index of the first ASCII letter or digit inside the first `instructions`
/// string, skipping escape sequences, so removing it keeps the JSON valid.
#[cfg(feature = "test-util")]
fn droppable_byte(body: &[u8]) -> Option<usize> {
    let marker = INSTRUCTIONS_MARKER.as_bytes();
    let start = body
        .windows(marker.len())
        .position(|window| window == marker)?
        + marker.len();
    let mut index = start;
    while index < body.len() {
        match body[index] {
            b'\\' => index += 1,
            b'"' => return None,
            byte if byte.is_ascii_alphanumeric() => return Some(index),
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
/// Returns [`JevError::ResponseInvalid`] when the reply is not a
/// [`CheckResponse`] or fails its rules against `check`: model, answer ids,
/// answer types, choices, probability keys and sums, and unit-interval values.
pub(crate) fn parse_outcome(
    raw: &[u8],
    check: &CheckRequest,
    request: &JudgeRequest,
) -> Result<JudgeOutcome, JevError> {
    let response = CheckResponse::parse(raw)?;
    let meta: Value = serde_json::from_slice(raw).map_err(|_| {
        JevError::response_invalid_because(ResponseInvalidCause::NotJson, "response is not JSON")
    })?;
    let answers = response.into_answers(check)?;
    Ok(JudgeOutcome::new(BackendId::Jev, request, answers)?.with_provider_meta(meta))
}

#[cfg(test)]
mod response_shape {
    use super::{parse_outcome, prepare};
    use crate::config::JevClientConfig;

    #[test]
    fn parse_outcome_keeps_other_top_level_fields() {
        let raw = br#"{"model":"jev-1.13.0","answers":{"q":{"type":"choice","choice":"no","confidence":0.9,"probabilities":{"no":0.9,"yes":0.1}}},"usage":{"input_tokens":318,"output_tokens":31},"stop_reason":"end_turn"}"#;
        let payload: sc_decision_schema::CanonicalJudgePayload =
            serde_json::from_value(serde_json::json!({
                "questions": [{"id": "q", "prompt": "p", "choices": ["no", "yes"]}],
                "state": []
            }))
            .expect("payload");
        let bundle = sc_decision_schema::PayloadBundle::encode(&payload).expect("bundle");
        let prepared = prepare(&JevClientConfig::packaged(), 96_000, &bundle).expect("prepared");
        let request = sc_decision_schema::JudgeRequest::new(std::sync::Arc::new(bundle), None);
        let outcome = parse_outcome(raw, &prepared.request, &request).expect("parse");
        let meta = outcome.provider_meta().expect("meta");
        assert_eq!(meta["stop_reason"], "end_turn");
        assert_eq!(meta["model"], "jev-1.13.0");
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

    use sc_decision_schema::{CanonicalJudgePayload, JudgeRequest, PayloadBundle};
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
        // A one-choice or repeated-choice question cannot be deserialized, so
        // build those bundles from the infallible constructor and let
        // `prepare` (which deserializes the bytes) refuse them.
        let cases = [
            (json!([]), "at least one question"),
            (
                json!([{"id": "q", "prompt": "p", "choices": ["a"]}]),
                "question q is a choice question and needs 2 to 255",
            ),
            (
                json!([{"id": "q", "prompt": "p", "choices": ["a", "a"]}]),
                "question q repeats a choice",
            ),
        ];
        for (questions, needle) in cases {
            let bundle = unchecked_bundle(&questions);
            let rendered = refused(&bundle, 96_000);
            assert!(rendered.starts_with("INPUT_INVALID"), "{rendered}");
            assert!(rendered.contains(needle), "{needle}: {rendered}");
        }
        assert!(refused(&bundle(array_question()), 10).starts_with("INPUT_TOO_LARGE"));
    }

    /// A bundle whose questions skip the kind rules (built with
    /// `Question::new_unchecked`), as a caller using the infallible constructor could.
    fn unchecked_bundle(questions: &Value) -> PayloadBundle {
        let list = questions
            .as_array()
            .expect("array")
            .iter()
            .map(|question| {
                let choices = question["choices"]
                    .as_array()
                    .expect("choices")
                    .iter()
                    .map(|choice| {
                        sc_decision_schema::ChoiceId::try_from(choice.as_str().expect("text"))
                            .expect("choice")
                    })
                    .collect();
                sc_decision_schema::Question::new_unchecked(
                    sc_decision_schema::QuestionId::try_from(question["id"].as_str().expect("id"))
                        .expect("id"),
                    "p",
                    choices,
                )
            })
            .collect();
        PayloadBundle::encode(&CanonicalJudgePayload::new(list, Vec::new())).expect("bundle")
    }

    #[test]
    fn prepare_builds_the_documented_body() {
        let bundle = bundle(array_question());
        let prepared = prepare(&JevClientConfig::packaged(), 96_000, &bundle).expect("accepted");
        assert_eq!(
            prepared.body,
            r#"{"model":"jev-1.14.0","state":[],"questions":{"q":{"type":"choice","instructions":"p","criteria":{"no":"no","yes":"yes"}}}}"#
        );
    }

    fn request() -> JudgeRequest {
        JudgeRequest::new(Arc::new(bundle(array_question())), None)
    }

    #[test]
    fn parse_outcome_refuses_every_malformed_answer() {
        let answer = |q: Value| json!({"model": "jev-1.13.0", "answers": {"q": q}}).to_string();
        let good = |extra: Value| {
            let mut value = json!({"type": "choice", "choice": "no", "confidence": 1,
                "probabilities": {"no": 1, "yes": 0}});
            for (key, item) in extra.as_object().expect("object") {
                value[key] = item.clone();
            }
            answer(value)
        };
        let cases: Vec<(String, &str)> = vec![
            ("[1]".to_owned(), "model or shape"),
            ("not json".to_owned(), "not JSON"),
            (json!({"model": "jev-1.13.0"}).to_string(), "model or shape"),
            (
                json!({"model": "jev-other", "answers": {}}).to_string(),
                "answer ids differ",
            ),
            (answer(json!(1)), "model or shape"),
            (good(json!({"choice": "maybe"})), "not one of its criteria"),
            (
                good(json!({"probabilities": {"no": 1}})),
                "probabilities are not exactly",
            ),
            (
                good(json!({"probabilities": {"no": 0.2, "yes": 0.2}})),
                "sum to",
            ),
            (good(json!({"confidence": 2})), "number in [0, 1]"),
            (good(json!({"type": "score"})), "type differs"),
        ];
        let request = request();
        let prepared =
            prepare(&JevClientConfig::packaged(), 96_000, request.payload()).expect("prepared");
        for (raw, needle) in cases {
            let error =
                parse_outcome(raw.as_bytes(), &prepared.request, &request).expect_err(needle);
            assert_eq!(error.code(), "RESPONSE_INVALID", "{raw}");
            assert!(error.to_string().contains(needle), "{needle}: {error}");
        }
    }
}
