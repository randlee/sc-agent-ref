//! Pre-send checks for a canonical payload and its injected prompt.
//!
//! A payload with no question, or a question with fewer than two choices,
//! refuses before any HTTP call. Question ids and choices are the typed map
//! the `PayloadBundle` already carries; nothing is re-parsed from bytes. An
//! unknown question type or a missing prompt cannot be built into a bundle.
//! The byte cap is the assembled judge body (system prompt plus canonical
//! payload), not the payload alone (ADR-SAG-0004, REQ-SAG-0502).

use std::collections::{BTreeMap, BTreeSet};

use sc_agent_types::JevError;
use sc_decision_schema::{ChoiceId, PayloadBundle, QuestionId};

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
/// Returns [`JevError::InputInvalid`] when `tokens` exceeds the ceiling.
pub(crate) fn enforce_input_token_limit(tokens: u64) -> Result<(), JevError> {
    if tokens > MAX_INPUT_TOKENS {
        return Err(JevError::input_invalid(format!(
            "input is {tokens} tokens; limit is {MAX_INPUT_TOKENS} tokens"
        )));
    }
    Ok(())
}

/// Byte and local token checks for the count body and the judge message.
///
/// # Errors
///
/// Returns [`JevError::InputTooLarge`] when either body exceeds
/// `limit_bytes` (the backend descriptor's `request_limit_bytes`; a larger
/// body is never split), and [`JevError::InputInvalid`] when the local token
/// estimate exceeds [`MAX_INPUT_TOKENS`].
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

/// The bundle's typed question map, after the choice-question checks the
/// bundle does not cover.
///
/// # Errors
///
/// `INPUT_INVALID` when the payload has no question or a question has fewer
/// than two distinct choices.
pub(crate) fn validate_questions(
    bundle: &PayloadBundle,
) -> Result<&BTreeMap<QuestionId, BTreeSet<ChoiceId>>, JevError> {
    let questions = bundle.question_choices();
    if questions.is_empty() {
        return Err(JevError::input_invalid(
            "a check needs at least one question",
        ));
    }
    if let Some(id) = questions
        .iter()
        .find_map(|(id, choices)| (choices.len() < 2).then_some(id))
    {
        return Err(JevError::input_invalid(format!(
            "question {id} is not a choice question with two or more criteria"
        )));
    }
    Ok(questions)
}

/// Refusal paths of `validate_questions` on bundles built from payloads.
#[cfg(test)]
mod refusals {
    use sc_decision_schema::{CanonicalJudgePayload, PayloadBundle};
    use serde_json::json;

    use super::validate_questions;

    fn bundle(questions: serde_json::Value) -> PayloadBundle {
        let payload: CanonicalJudgePayload =
            serde_json::from_value(json!({"questions": questions, "state": []})).expect("payload");
        PayloadBundle::encode(&payload).expect("bundle")
    }

    #[test]
    fn validate_questions_refuses_every_malformed_payload() {
        let cases = [
            (json!([]), "at least one question"),
            (
                json!([{"id": "q", "prompt": "p", "choices": ["a"]}]),
                "question q is not a choice question",
            ),
            (
                json!([{"id": "q", "prompt": "p", "choices": ["a", "a"]}]),
                "question q is not a choice question",
            ),
        ];
        for (questions, needle) in cases {
            let bundle = bundle(questions);
            let error = validate_questions(&bundle).expect_err(needle);
            assert_eq!(error.code(), "INPUT_INVALID", "{needle}");
            assert!(error.to_string().contains(needle), "{needle}: {error}");
        }
        let ok = bundle(json!([{"id": "q", "prompt": "p", "choices": ["a", "b"]}]));
        assert_eq!(validate_questions(&ok).expect("valid").len(), 1);
    }
}
