//! Model-family variants (REQ-SAG-0145, ADR-SAG-0030).
//!
//! [`schema_for`] maps a [`ModelFamily`] to the decision schema its model
//! reads and writes; one module per [`SchemaFamily`](crate::SchemaFamily) holds plain functions.
//! There is no trait until a second variant exists. Dispatch is static and
//! keyed by [`SchemaFamily`](crate::SchemaFamily): `llm::v1::parse_output` holds the one `match`,
//! so a new family edits this crate only (a `SchemaFamily` variant, a module
//! and one `match` arm). A future local decision model with a slightly
//! different schema is recorded here, not built.

use sc_agent_types::ModelFamily;

use crate::version::{CANONICAL_V1, DecisionSchemaId};

/// The decision schema `family` uses. Every family maps to [`CANONICAL_V1`]
/// today. A family this build does not know also reads the canonical schema.
#[must_use]
pub fn schema_for(family: ModelFamily) -> DecisionSchemaId {
    match family {
        ModelFamily::TypeSafeJev
        | ModelFamily::AnthropicClaude
        | ModelFamily::OpenAiGpt
        | ModelFamily::Local
        | ModelFamily::Unknown => CANONICAL_V1,
        // `ModelFamily` is `#[non_exhaustive]`: a family added later reads the
        // canonical schema until it gets its own variant and `match` arm.
        _ => CANONICAL_V1,
    }
}

/// The `canonical.v1` variant.
pub mod canonical {
    use std::collections::BTreeSet;

    use sc_agent_types::JevError;
    use serde_json::{Map, Value};

    use crate::ids::{ChoiceId, QuestionId};
    use crate::judge::QuestionAnswer;
    use crate::llm::v1::{LlmOutput, id_set, ids_differ};
    use crate::payload::{CanonicalJudgePayload, DecisionAnswers, PayloadBundle};

    /// The model input: the canonical payload itself. Its serialization
    /// equals `PayloadBundle::bytes()`, so `prompt_sha256` is the digest of
    /// what is embedded.
    #[must_use]
    pub fn input(payload: &CanonicalJudgePayload) -> &CanonicalJudgePayload {
        payload
    }

    /// Validates a parsed reply against `payload` and returns typed answers.
    ///
    /// # Errors
    ///
    /// `RESPONSE_INVALID`; see `llm::v1::parse_output` for the cases.
    pub fn answers(
        output: LlmOutput,
        payload: &PayloadBundle,
    ) -> Result<DecisionAnswers, JevError> {
        let answers = output.into_answers();
        let questions = payload.question_choices();
        let expected: BTreeSet<&str> = questions.keys().map(QuestionId::as_str).collect();
        if expected != id_set(&answers) {
            return Err(ids_differ());
        }
        let mut typed = DecisionAnswers::new();
        for (id, choices) in questions {
            let key = id.as_str();
            let item = answers
                .get(key)
                .and_then(Value::as_object)
                .ok_or_else(|| bad_answer(key))?;
            let choice = item.get("choice").and_then(Value::as_str).unwrap_or("");
            if !choices.iter().any(|label| label.as_str() == choice) {
                return Err(bad_answer(key));
            }
            let confidence = item.get("confidence").ok_or_else(|| bad_answer(key))?;
            if !is_number(confidence) {
                return Err(not_a_number());
            }
            let probabilities = item
                .get("probabilities")
                .and_then(Value::as_object)
                .ok_or_else(|| bad_answer(key))?;
            let expected_labels: BTreeSet<&str> = choices.iter().map(ChoiceId::as_str).collect();
            if expected_labels != id_set(probabilities) {
                return Err(bad_answer(key));
            }
            if !probabilities.values().all(is_number) {
                return Err(not_a_number());
            }
            let mut answer = Map::new();
            answer.insert("choice".to_owned(), Value::String(choice.to_owned()));
            answer.insert("confidence".to_owned(), confidence.clone());
            answer.insert(
                "probabilities".to_owned(),
                Value::Object(probabilities.clone()),
            );
            let answer: QuestionAnswer =
                serde_json::from_value(Value::Object(answer)).map_err(|error| {
                    JevError::response_invalid(format!("answer {key} is invalid: {error}"))
                })?;
            typed.insert(id.clone(), answer);
        }
        Ok(typed)
    }

    fn is_number(value: &Value) -> bool {
        matches!(value, Value::Number(_))
    }

    fn bad_answer(id: &str) -> JevError {
        JevError::response_invalid(format!(
            "LLM answer {id} needs a criterion choice and a probability per criterion"
        ))
    }

    fn not_a_number() -> JevError {
        JevError::response_invalid("LLM confidence or probability is not a number")
    }
}
