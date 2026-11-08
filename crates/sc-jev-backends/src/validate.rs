//! Pre-send checks for a canonical payload and its injected prompt.
//!
//! The payload bytes are deserialized as a `CanonicalJudgePayload` (the same
//! validator `sc-jev` uses; the question-kind rules run inside it). A payload
//! with no question, a question that breaks its kind's rules, or a `noul`
//! question refuses before any HTTP call.
//! The byte cap is the assembled judge body (system prompt plus canonical
//! payload), not the payload alone (ADR-SAG-0004, REQ-SAG-0502).

use sc_agent_types::JevError;
use sc_decision_schema::{CanonicalJudgePayload, PayloadBundle, QuestionKind};

/// Pre-send input token ceiling for one judge call (ADR-SAG-0004).
///
/// `validate_for_send` applies [`local_input_tokens`] to the count body and
/// the judge message before any HTTP send. `judge` checks the provider's
/// `input_tokens` against this same ceiling after the count call and before
/// the judge message.
pub(crate) const MAX_INPUT_TOKENS: u64 = 32_000;

/// Conservative token count of an assembled provider body.
///
/// One token per four bytes, rounded up. A body that already passed
/// the 96 000-byte descriptor limit estimates to at most 24_000 tokens, under
/// [`MAX_INPUT_TOKENS`]. The provider count is the check that can still
/// refuse the judge message.
pub(crate) fn local_input_tokens(body: &[u8]) -> u64 {
    let bytes = u64::try_from(body.len()).unwrap_or(u64::MAX);
    bytes.div_ceil(4)
}

/// Refuses `tokens` above [`MAX_INPUT_TOKENS`].
///
/// # Errors
///
/// Returns [`JevError::InputTooLarge`] (unit `tokens`) when `tokens` exceeds
/// the ceiling.
pub(crate) fn enforce_input_token_limit(tokens: u64) -> Result<(), JevError> {
    if tokens > MAX_INPUT_TOKENS {
        return Err(JevError::input_too_large_tokens(MAX_INPUT_TOKENS, tokens));
    }
    Ok(())
}

/// Byte and local token checks for the count body and the judge message.
///
/// # Errors
///
/// Returns [`JevError::InputTooLarge`] when either body exceeds
/// `limit_bytes` (the backend descriptor's `request_limit_bytes`; a larger
/// body is never split), and [`JevError::InputTooLarge`] (unit `tokens`) when
/// the local token estimate exceeds [`MAX_INPUT_TOKENS`].
pub(crate) fn enforce_count_and_message_limits(
    limit_bytes: u64,
    count_body: &str,
    message_body: &str,
) -> Result<(), JevError> {
    for body in [count_body, message_body] {
        let actual = u64::try_from(body.len()).unwrap_or(u64::MAX);
        if actual > limit_bytes {
            return Err(JevError::input_too_large(limit_bytes, actual));
        }
        enforce_input_token_limit(local_input_tokens(body.as_bytes()))?;
    }
    Ok(())
}

/// Validates the payload a Haiku or Luna call embeds.
///
/// The bytes are deserialized as a [`CanonicalJudgePayload`], the one
/// validator Jev uses too: unknown fields and question-kind rule violations
/// are refused there. A `noul` question is refused here, before any send.
///
/// # Errors
///
/// `INPUT_INVALID` when the payload does not deserialize (a `choice` question
/// needs 2 to 255 choices, a `score` question 2 to 10), has no question, or
/// has a `noul` question.
pub(crate) fn validate_questions(bundle: &PayloadBundle) -> Result<(), JevError> {
    let payload = CanonicalJudgePayload::from_bytes(bundle.bytes())?;
    if payload.questions().is_empty() {
        return Err(JevError::input_invalid(
            "a check needs at least one question",
        ));
    }
    if let Some(question) = payload
        .questions()
        .iter()
        .find(|question| question.kind() == QuestionKind::Noul)
    {
        return Err(JevError::input_invalid(format!(
            "question {} is a noul question; Haiku and Luna answer choice and score questions; send noul questions to jev",
            question.id()
        )));
    }
    Ok(())
}

/// Refusal paths of `validate_questions` on bundles built from payloads.
#[cfg(test)]
mod refusals {
    use sc_decision_schema::{
        CanonicalJudgePayload, ChoiceId, PayloadBundle, Question, QuestionId,
    };
    use serde_json::json;

    use super::validate_questions;

    fn bundle(questions: serde_json::Value) -> PayloadBundle {
        let payload: CanonicalJudgePayload =
            serde_json::from_value(json!({"questions": questions, "state": []})).expect("payload");
        PayloadBundle::encode(&payload).expect("bundle")
    }

    /// A bundle built with the infallible constructor, which skips the kind
    /// rules; `validate_questions` deserializes the bytes and applies them.
    fn unchecked(choices: &[&str]) -> PayloadBundle {
        let choices = choices
            .iter()
            .map(|label| ChoiceId::try_from(*label).expect("choice"))
            .collect();
        let question =
            Question::new_unchecked(QuestionId::try_from("q").expect("id"), "p", choices);
        PayloadBundle::encode(&CanonicalJudgePayload::new(vec![question], Vec::new()))
            .expect("bundle")
    }

    #[test]
    fn validate_questions_refuses_every_malformed_payload() {
        let cases = [
            (bundle(json!([])), "at least one question"),
            (unchecked(&["a"]), "needs 2 to 255 choices"),
            (unchecked(&["a", "a"]), "repeats a choice"),
            (
                bundle(json!([{"id": "q", "prompt": "p", "choices": [], "kind": "noul"}])),
                "send noul questions to jev",
            ),
        ];
        for (bundle, needle) in cases {
            let error = validate_questions(&bundle).expect_err(needle);
            assert_eq!(error.code(), "INPUT_INVALID", "{needle}");
            assert!(error.to_string().contains(needle), "{needle}: {error}");
        }
        let ok = bundle(json!([{"id": "q", "prompt": "p", "choices": ["a", "b"]}]));
        validate_questions(&ok).expect("valid");
        let score = bundle(
            json!([{"id": "q", "prompt": "p", "choices": ["1", "2", "3"], "kind": "score"}]),
        );
        validate_questions(&score).expect("score is valid");
    }
}
