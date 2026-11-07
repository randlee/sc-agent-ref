//! Provider request and reply shapes.
//!
//! The user message is the canonical payload decoded as UTF-8. The injected
//! system prompt is `system` for Haiku and `instructions` for Luna. Neither
//! body is the Jev wire document (ADR-SAG-0019).

use std::collections::{BTreeMap, BTreeSet};

use sc_agent_http::{HeaderName, SecretHeader};
use sc_agent_types::{
    ChoiceId, JevError, QuestionId, ResolvedApiKey, Sha256Digest, canonical_json_bytes,
};
use serde_json::{Map, Value};

/// Haiku model id pinned from the jev-check README table.
///
/// Changing it changes the provider body. It is not part of `prompt_sha256`.
pub(crate) const HAIKU_MODEL: &str = "claude-haiku-5-5";

/// Luna model id pinned from the jev-check README table.
pub(crate) const LUNA_MODEL: &str = "gpt-6-luna";

pub(crate) const ANTHROPIC_HOST: &str = "api.anthropic.com";
pub(crate) const OPENAI_HOST: &str = "api.openai.com";
pub(crate) const HAIKU_MESSAGE_PATH: &str = "/v1/messages";
pub(crate) const HAIKU_COUNT_PATH: &str = "/v1/messages/count_tokens";
pub(crate) const LUNA_MESSAGE_PATH: &str = "/v1/responses";
pub(crate) const LUNA_COUNT_PATH: &str = "/v1/responses/input_tokens";

/// Anthropic REST version sent on every Haiku call.
///
/// Pinned here because this crate does not read `check.toml`. A different
/// version is a different wire contract.
pub(crate) const ANTHROPIC_VERSION: &str = "2026-10-21";

/// Haiku completion cap.
///
/// A reply whose `stop_reason` is `max_tokens` is `RESPONSE_INVALID`. The
/// token-count body omits this field.
pub(crate) const HAIKU_MAX_TOKENS: u64 = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderKind {
    Anthropic,
    OpenAi,
}

pub(crate) fn message_body(
    kind: ProviderKind,
    model: &str,
    payload_text: &str,
    system_prompt: &str,
) -> Vec<u8> {
    encode(&provider_document(
        kind,
        model,
        payload_text,
        system_prompt,
        true,
    ))
}

/// The digest of the canonical blob inside a serialized provider message
/// `body` (REQ-SAG-0606, ADR-SAG-0025).
///
/// Reads the user message's content (`messages[0].content` for Anthropic,
/// `input[0].content` for OpenAI), parses it, re-encodes it with
/// `canonical_json_bytes` and hashes it. The re-encoding is attestation-only:
/// it never feeds the wire body (ADR-SAG-0005). Callers run it on the body
/// before it is handed to `send_with_busy_retry`.
///
/// # Errors
///
/// `INPUT_INVALID` when the body or the embedded payload is not JSON, or the
/// body has no user message. The error names the failure category and
/// position only, never body bytes (ADR-SAG-0020).
pub(crate) fn recovered_digest(kind: ProviderKind, body: &[u8]) -> Result<Sha256Digest, JevError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| JevError::encode_failed("provider wire body", &error))?;
    let messages = match kind {
        ProviderKind::Anthropic => "messages",
        ProviderKind::OpenAi => "input",
    };
    let text = value
        .get(messages)
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| JevError::input_invalid("provider wire body has no embedded payload"))?;
    let embedded: Value = serde_json::from_str(text)
        .map_err(|error| JevError::encode_failed("embedded payload", &error))?;
    let canonical = canonical_json_bytes(&embedded)?;
    Ok(Sha256Digest::of(&canonical))
}

/// A seeded defect in the provider wire builder (feature `test-util`).
///
/// The fault is applied after the canonical bytes are embedded in the
/// message and before the digest is recovered, so a test shows the digest
/// follows the bytes that would be sent.
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
    /// The embedded payload text after the fault.
    pub(crate) fn apply_to_text(self, text: &str) -> String {
        let mut bytes = text.as_bytes().to_vec();
        if self == Self::DropEmbeddedByte
            && let Some(index) = droppable_byte(&bytes)
        {
            bytes.remove(index);
        }
        // Only an ASCII letter or digit is removed, so UTF-8 stays valid;
        // the fallback keeps the text unchanged rather than panicking.
        String::from_utf8(bytes).unwrap_or_else(|_| text.to_owned())
    }

    /// The serialized body after the fault.
    pub(crate) fn apply_to_body(self, mut body: Vec<u8>) -> Vec<u8> {
        if self == Self::TruncateBody {
            body.truncate(body.len() / 2);
        }
        body
    }
}

/// Index of the first ASCII letter or digit inside a JSON string, skipping
/// escape sequences, so removing it keeps the JSON valid.
#[cfg(feature = "test-util")]
fn droppable_byte(text: &[u8]) -> Option<usize> {
    let mut in_string = false;
    let mut index = 0;
    while index < text.len() {
        let byte = text[index];
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

/// Token-count body for `kind`.
///
/// OpenAI counts the judge document. Anthropic counts the same document
/// without `max_tokens`. Both documents are built here. This function does
/// not parse the judge body.
pub(crate) fn count_body(
    kind: ProviderKind,
    model: &str,
    payload_text: &str,
    system_prompt: &str,
) -> Vec<u8> {
    encode(&provider_document(
        kind,
        model,
        payload_text,
        system_prompt,
        !matches!(kind, ProviderKind::Anthropic),
    ))
}

fn provider_document(
    kind: ProviderKind,
    model: &str,
    payload_text: &str,
    system_prompt: &str,
    include_max_tokens: bool,
) -> Value {
    let user = serde_json::json!([{ "role": "user", "content": payload_text }]);
    match kind {
        ProviderKind::Anthropic if include_max_tokens => serde_json::json!({
            "model": model,
            "max_tokens": HAIKU_MAX_TOKENS,
            "messages": user,
            "system": system_prompt,
        }),
        ProviderKind::Anthropic => serde_json::json!({
            "model": model,
            "messages": user,
            "system": system_prompt,
        }),
        ProviderKind::OpenAi => serde_json::json!({
            "model": model,
            "input": user,
            "instructions": system_prompt,
        }),
    }
}

/// Provider headers that do not carry the API key.
///
/// The credential goes separately, as [`secret_header`]; only the sending
/// transport writes its value (ADR-SAG-0020).
pub(crate) fn unsigned_headers(kind: ProviderKind) -> Vec<(HeaderName, &'static str)> {
    let mut headers = match kind {
        ProviderKind::Anthropic => vec![(ANTHROPIC_VERSION_HEADER, ANTHROPIC_VERSION)],
        ProviderKind::OpenAi => Vec::new(),
    };
    headers.push((CONTENT_TYPE, "application/json"));
    headers
}

/// The credential header each provider expects (ADR-SAG-0020).
pub(crate) fn secret_header(kind: ProviderKind, key: ResolvedApiKey) -> SecretHeader {
    match kind {
        ProviderKind::Anthropic => SecretHeader::new(X_API_KEY, key),
        ProviderKind::OpenAi => SecretHeader::bearer(key),
    }
}

const ANTHROPIC_VERSION_HEADER: HeaderName = HeaderName::new("anthropic-version");
const CONTENT_TYPE: HeaderName = HeaderName::new("Content-Type");
const X_API_KEY: HeaderName = HeaderName::new("x-api-key");

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VendorReply {
    pub(crate) text: String,
    pub(crate) input_tokens: u64,
    /// Content blocks that are not text (tool calls and other items).
    ///
    /// Reported as `provider_meta.commands` for compatibility; not related to
    /// `EqualInputRecord.commands`.
    pub(crate) non_text_blocks: u64,
    pub(crate) stop_reason: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) truncation: Option<String>,
}

pub(crate) fn parse_count(body: &[u8]) -> Result<u64, JevError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| JevError::response_invalid("token count is not JSON"))?;
    let tokens = value.get("input_tokens");
    tokens
        .and_then(Value::as_u64)
        .ok_or_else(|| JevError::response_invalid("token count has no input_tokens"))
}

/// Parses a vendor reply. Error details carry no provider name; the caller
/// labels them.
pub(crate) fn parse_vendor(kind: ProviderKind, body: &[u8]) -> Result<VendorReply, JevError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| JevError::response_invalid("reply is not JSON"))?;
    let object = value
        .as_object()
        .ok_or_else(|| JevError::response_invalid("reply is not JSON"))?;
    let usage = object.get("usage").and_then(Value::as_object);
    let (text, non_text_blocks, input_tokens) = match kind {
        ProviderKind::Anthropic => parse_anthropic(object, usage)?,
        ProviderKind::OpenAi => parse_openai(object, usage)?,
    };
    let stop_reason = object
        .get("stop_reason")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let truncation = truncation_reason(object, stop_reason.as_deref(), status.as_deref());
    if text.is_none() && truncation.is_none() {
        return Err(JevError::response_invalid("reply has no text"));
    }
    Ok(VendorReply {
        text: text.unwrap_or_default(),
        input_tokens,
        non_text_blocks,
        stop_reason,
        status,
        truncation,
    })
}

pub(crate) fn parse_answers(
    text: &str,
    questions: &BTreeMap<QuestionId, BTreeSet<ChoiceId>>,
) -> Result<Value, JevError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|_| JevError::response_invalid("LLM reply is not JSON"))?;
    let answers = value
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| JevError::response_invalid("LLM answer ids differ from the question ids"))?;
    let expected: BTreeSet<&str> = questions.keys().map(QuestionId::as_str).collect();
    let actual: BTreeSet<&str> = answers.keys().map(String::as_str).collect();
    if expected != actual {
        return Err(JevError::response_invalid(
            "LLM answer ids differ from the question ids",
        ));
    }

    let mut normalized = Map::new();
    for (id, choices) in questions {
        let id = id.as_str();
        let item = answers
            .get(id)
            .and_then(Value::as_object)
            .ok_or_else(|| bad_answer(id))?;
        let choice = item.get("choice").and_then(Value::as_str).unwrap_or("");
        if !choices.iter().any(|label| label.as_str() == choice) {
            return Err(bad_answer(id));
        }
        let confidence = item.get("confidence").ok_or_else(|| bad_answer(id))?;
        if !is_number(confidence) {
            return Err(not_a_number());
        }
        let probabilities = item
            .get("probabilities")
            .and_then(Value::as_object)
            .ok_or_else(|| bad_answer(id))?;
        let expected_labels: BTreeSet<&str> = choices.iter().map(ChoiceId::as_str).collect();
        let actual_labels: BTreeSet<&str> = probabilities.keys().map(String::as_str).collect();
        if expected_labels != actual_labels {
            return Err(bad_answer(id));
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
        normalized.insert(id.to_owned(), Value::Object(answer));
    }
    Ok(Value::Object(normalized))
}

pub(crate) fn provider_meta(reply: &VendorReply, model: &str, prompt_tokens: u64) -> Value {
    serde_json::json!({
        "model": model,
        "stop_reason": reply.stop_reason,
        "status": reply.status,
        "input_tokens": reply.input_tokens,
        "prompt_tokens": prompt_tokens,
        "commands": reply.non_text_blocks,
    })
}

fn parse_anthropic(
    object: &Map<String, Value>,
    usage: Option<&Map<String, Value>>,
) -> Result<(Option<String>, u64, u64), JevError> {
    let mut text = String::new();
    let mut non_text_blocks = 0_u64;
    let mut saw_text = false;
    for block in object
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(block) = block.as_object() else {
            continue;
        };
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(piece) = block.get("text").and_then(Value::as_str) {
                    text.push_str(piece);
                    saw_text = true;
                }
            }
            Some(kind) if kind.ends_with("tool_use") => {
                non_text_blocks = non_text_blocks.saturating_add(1)
            }
            _ => {}
        }
    }
    let input_tokens = usage_i64(usage, "input_tokens")
        .saturating_add(usage_i64(usage, "cache_creation_input_tokens"))
        .saturating_add(usage_i64(usage, "cache_read_input_tokens"));
    Ok((
        saw_text.then_some(text),
        non_text_blocks,
        nonneg(input_tokens)?,
    ))
}

fn parse_openai(
    object: &Map<String, Value>,
    usage: Option<&Map<String, Value>>,
) -> Result<(Option<String>, u64, u64), JevError> {
    let mut text = String::new();
    let mut non_text_blocks = 0_u64;
    let mut saw_text = false;
    for item in object
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(item) = item.as_object() else {
            non_text_blocks = non_text_blocks.saturating_add(1);
            continue;
        };
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                for block in item
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let Some(block) = block.as_object() else {
                        continue;
                    };
                    if let Some(piece) = block.get("text").and_then(Value::as_str) {
                        text.push_str(piece);
                        saw_text = true;
                    }
                }
            }
            Some("reasoning") => {}
            _ => non_text_blocks = non_text_blocks.saturating_add(1),
        }
    }
    Ok((
        saw_text.then_some(text),
        non_text_blocks,
        nonneg(usage_i64(usage, "input_tokens"))?,
    ))
}

fn truncation_reason(
    object: &Map<String, Value>,
    stop_reason: Option<&str>,
    status: Option<&str>,
) -> Option<String> {
    if stop_reason == Some("max_tokens") {
        return Some("reply truncated at max_tokens (stop_reason=max_tokens)".to_owned());
    }
    if let Some(status) =
        status.filter(|status| matches!(*status, "incomplete" | "max_tokens" | "max_output_tokens"))
    {
        return Some(format!("reply truncated at max_tokens (status={status})"));
    }
    let incomplete = object
        .get("incomplete_details")
        .and_then(|value| value.get("reason"))
        .and_then(Value::as_str);
    if incomplete == Some("max_output_tokens") {
        return Some(
            "reply truncated at max_tokens (incomplete_details.reason=max_output_tokens)"
                .to_owned(),
        );
    }
    None
}

/// A usage counter; a provider that omits one reports zero tokens for it.
fn usage_i64(usage: Option<&Map<String, Value>>, key: &str) -> i64 {
    usage
        .and_then(|object| object.get(key))
        .and_then(Value::as_i64)
        .unwrap_or_default()
}

fn nonneg(value: i64) -> Result<u64, JevError> {
    u64::try_from(value)
        .map_err(|_| JevError::response_invalid("provider input_tokens is negative"))
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

fn encode(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("provider JSON values are finite")
}
