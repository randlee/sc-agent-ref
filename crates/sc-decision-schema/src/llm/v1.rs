//! `llm::v1`: the Haiku/Luna input and output on the canonical schema.
//!
//! **Input** is the canonical payload bytes, embedded in the provider
//! message unchanged ([`input_bytes`]; ADR-SAG-0019: the system prompt stays
//! outside the digest). **Output** is [`DecisionAnswers`], the same map
//! [`JudgeOutcome`](crate::JudgeOutcome) carries for Jev ([`parse_output`]).
//! Provider envelopes (Anthropic messages, OpenAI responses) stay in
//! `sc-jev-backends`.

use std::collections::BTreeSet;

use sc_agent_types::{JevError, ModelFamily};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::payload::{DecisionAnswers, PayloadBundle};
use crate::variants::{self, canonical};
use crate::version::SchemaFamily;

/// What a model's reply text parses to before a variant validates it.
#[derive(Debug, Clone, Deserialize)]
pub struct LlmOutput {
    answers: Map<String, Value>,
}

impl LlmOutput {
    /// Parses the model's reply text.
    ///
    /// # Errors
    ///
    /// `RESPONSE_INVALID` when the text is not JSON ("LLM reply is not
    /// JSON") or has no `answers` object ("LLM answer ids differ from the
    /// question ids").
    pub fn parse(text: &str) -> Result<Self, JevError> {
        let value: Value = serde_json::from_str(text)
            .map_err(|_| JevError::response_invalid("LLM reply is not JSON"))?;
        match value {
            Value::Object(mut object) => match object.remove("answers") {
                Some(Value::Object(answers)) => Ok(Self { answers }),
                _ => Err(ids_differ()),
            },
            _ => Err(ids_differ()),
        }
    }

    /// The raw `answers` object.
    #[must_use]
    pub fn answers(&self) -> &Map<String, Value> {
        &self.answers
    }

    pub(crate) fn into_answers(self) -> Map<String, Value> {
        self.answers
    }
}

pub(crate) fn ids_differ() -> JevError {
    JevError::response_invalid("LLM answer ids differ from the question ids")
}

/// The bytes embedded in the provider message: `payload.bytes()` itself, with
/// no re-serialization (ADR-SAG-0005).
#[must_use]
pub fn input_bytes(payload: &PayloadBundle) -> &[u8] {
    payload.bytes()
}

/// Parses a model's reply text into answers for `payload`.
///
/// Parses [`LlmOutput`], then dispatches on the decision schema of `family`
/// (the one `match` on [`SchemaFamily`]) to the variant's validation.
///
/// # Errors
///
/// `RESPONSE_INVALID`: the reply is not JSON; `answers` is missing, not an
/// object, or its ids differ from the question ids; an answer is not an
/// object, its `choice` is not one of the question's choices, `confidence`
/// is missing, or `probabilities` is missing or its labels differ from the
/// choices; `confidence` or a probability is not a number or is outside
/// `[0, 1]`.
///
/// # Examples
///
/// ```
/// use sc_agent_types::ModelFamily;
/// use sc_decision_schema::{CanonicalJudgePayload, PayloadBundle, llm::v1};
///
/// let payload: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
///     "questions": [{"id": "q", "prompt": "Is it?", "choices": ["no", "yes"]}],
///     "state": []
/// }))?;
/// let bundle = PayloadBundle::encode(&payload)?;
/// let reply = r#"{"answers":{"q":{"choice":"yes","confidence":0.9,
///     "probabilities":{"no":0.1,"yes":0.9}}}}"#;
/// let answers = v1::parse_output(ModelFamily::AnthropicClaude, reply, &bundle)?;
/// assert_eq!(answers.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_output(
    family: ModelFamily,
    text: &str,
    payload: &PayloadBundle,
) -> Result<DecisionAnswers, JevError> {
    let output = LlmOutput::parse(text)?;
    match variants::schema_for(family).family() {
        SchemaFamily::Canonical => canonical::answers(output, payload),
    }
}

pub(crate) fn id_set(map: &Map<String, Value>) -> BTreeSet<&str> {
    map.keys().map(String::as_str).collect()
}
