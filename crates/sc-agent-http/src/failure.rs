//! Classification of a provider reply with a failing status.
//!
//! One place decides which error id a non-200 reply from Jev, Haiku or Luna
//! becomes, so no backend maps statuses on its own. The decision uses the
//! status first and the body second, and keeps what the provider said.
//!
//! # Mapping
//!
//! | Reply | Error |
//! | --- | --- |
//! | 401, 403 | `AUTH_FAILED` |
//! | 402, or a quota body (below) | `QUOTA_EXHAUSTED` |
//! | 413, or 400/422 with a structured marker (`context_length_exceeded`, `request_too_large`), or, for Haiku and Luna ([`ReplyShape::Llm`]), Anthropic's `invalid_request_error` whose message starts "prompt is too long" (unverified wording), or, for Jev only ([`ReplyShape::Jev`]), a message naming `max_tokens` or tokens exceeded (below) | `INPUT_TOO_LARGE` (unit `bytes` for 413, else `tokens`) |
//! | 400, 422 | `API_REQUEST_INVALID` |
//! | every other status, 5xx included | `API_ERROR` |
//!
//! 429, 502 and 529 are busy and retried by [`send_with_busy_retry`]
//! ([`crate::send_with_busy_retry`]); they reach this module only with a
//! quota body, which [`retry`](crate::retry) hands over instead of retrying.
//!
//! # Documented body shapes
//!
//! The bodies below are the shapes the providers document, not observed
//! against a live endpoint here, except Jev's `detail` object, below.
//!
//! - **Anthropic** ([errors](https://platform.claude.com/docs/en/api/errors),
//!   [rate limits](https://platform.claude.com/docs/en/api/rate-limits)):
//!   `{"type":"error","error":{"type":"...","message":"...","details":{"error_code":"..."}},"request_id":"req_..."}`.
//!   The tier spend cap is a 429 `rate_limit_error` with
//!   `error.details.error_code == "enforced_spend_limit_reached"` and no
//!   `retry-after` header; a spend limit the customer set is a 400
//!   `invalid_request_error` whose message starts `You have reached your
//!   specified`; 402 is `billing_error`; 413 is `request_too_large`.
//! - **OpenAI** ([error codes](https://developers.openai.com/api/docs/guides/error-codes)):
//!   `{"error":{"message":"...","type":"...","param":null,"code":"..."}}`.
//!   A 429 with `code` `insufficient_quota` or `credit_balance_exhausted` is
//!   out of quota; a 429 with `rate_limit_exceeded` is a rate limit. A 400
//!   `context_length_exceeded` is a too-long input (that code is widely
//!   reported but not listed on the page above).
//! - **Jev** (`plan-anchor`'s `jev_judge.py.j2`, `http_error`): a 4xx whose
//!   message mentions `max_tokens`, or `token` and `exceed`, is a context
//!   overflow (matched on the message text, never on JSON keys, and only for
//!   [`ReplyShape::Jev`]; for Haiku and Luna a message about `max_tokens`, the
//!   output cap, is an ordinary invalid request); 401 is a credential failure; 422 carries the field error text;
//!   429 and 529 are busy; 5xx is transient. Jev's own error JSON is not
//!   documented here, so the reader also accepts `{"detail": "..."}`,
//!   `{"detail": [{"loc": [...], "msg": "..."}]}` and `{"message": "..."}`,
//!   and falls back to the bounded raw body. Live replies on 2026-11-08
//!   showed `{"detail": {"error_type": "billing_error", "message": "..."}}`
//!   on 402 and the same shape with `authentication_error` on 401.
//!
//! A body that is none of these stays what its status says: an unknown 429
//! body is busy, never quota.

use sc_agent_types::{JevError, LimitUnit, ProviderFailure};
use serde_json::Value;

use crate::request::{HttpRequest, HttpResponse};
use crate::retry_after::RetryAfter;

/// `error.code`, `error.type` or `error.details.error_code` values that
/// mean the account is out of tokens, credit or spend allowance.
const QUOTA_MARKERS: [&str; 4] = [
    "insufficient_quota",
    "credit_balance_exhausted",
    "billing_error",
    "enforced_spend_limit_reached",
];

/// Message starts (lower case) that mean out of quota. Anthropic's customer
/// spend limit is a 400 with the first; the second is Anthropic's
/// credit-balance text, reported by users but not listed in its docs.
const QUOTA_MESSAGES: [&str; 2] = [
    "you have reached your specified",
    "your credit balance is too low",
];

/// What the reader found in a failing reply's body.
#[derive(Debug, Default)]
struct Body {
    provider_type: Option<String>,
    provider_code: Option<String>,
    detail_code: Option<String>,
    message: Option<String>,
    param: Option<String>,
    request_id: Option<String>,
}

fn string_of(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

impl Body {
    fn read(bytes: &[u8]) -> Self {
        let Ok(Value::Object(root)) = serde_json::from_slice::<Value>(bytes) else {
            return Self::default();
        };
        let mut body = Self {
            request_id: root.get("request_id").and_then(string_of),
            ..Self::default()
        };
        match root.get("error") {
            Some(Value::Object(error)) => {
                body.provider_type = error.get("type").and_then(string_of);
                body.provider_code = error.get("code").and_then(string_of);
                body.message = error.get("message").and_then(string_of);
                body.param = error.get("param").and_then(string_of);
                body.detail_code = error
                    .get("details")
                    .and_then(|details| details.get("error_code"))
                    .and_then(string_of);
            }
            Some(Value::String(text)) if !text.is_empty() => body.message = Some(text.clone()),
            _ => {}
        }
        if body.message.is_none() {
            body.message = root.get("message").and_then(string_of);
        }
        if body.provider_code.is_none() {
            body.provider_code = root.get("code").and_then(string_of);
        }
        if body.message.is_none() {
            match root.get("detail") {
                Some(Value::String(text)) if !text.is_empty() => body.message = Some(text.clone()),
                // Jev's billing and auth errors, observed live 2026-11-08:
                // {"detail":{"error_type":"billing_error","message":"..."}}.
                Some(Value::Object(detail)) => {
                    if body.provider_type.is_none() {
                        body.provider_type = detail.get("error_type").and_then(string_of);
                    }
                    body.message = detail.get("message").and_then(string_of);
                }
                Some(Value::Array(items)) => {
                    let first = items.first();
                    body.message = first.and_then(|item| item.get("msg")).and_then(string_of);
                    body.param = first
                        .and_then(|item| item.get("loc"))
                        .and_then(Value::as_array)
                        .and_then(|loc| loc.last())
                        .and_then(string_of);
                }
                _ => {}
            }
        }
        body
    }

    fn marks_quota(&self, status: u16) -> bool {
        let marker = |value: &Option<String>| {
            value
                .as_deref()
                .is_some_and(|text| QUOTA_MARKERS.contains(&text))
        };
        if marker(&self.provider_code) || marker(&self.provider_type) || marker(&self.detail_code) {
            return true;
        }
        matches!(status, 400 | 429)
            && self.message.as_deref().is_some_and(|message| {
                let lower = message.to_ascii_lowercase();
                QUOTA_MESSAGES.iter().any(|start| lower.starts_with(start))
            })
    }
}

/// Whether a reply with `status` and `body` says the account is out of
/// tokens, credit or spend allowance.
///
/// This is the classification hook of the busy retry: a 429 for which this
/// returns `true` is handed to the caller instead of being retried, because
/// no wait will make it succeed.
pub(crate) fn is_quota_reply(status: u16, body: &[u8]) -> bool {
    status == 402 || Body::read(body).marks_quota(status)
}

/// Whole seconds of a valid `Retry-After` header, for reporting (the report
/// view of the one parser, [`RetryAfter`]).
pub(crate) fn retry_after_seconds(header: Option<&str>) -> Option<u64> {
    RetryAfter::parse(header).map(RetryAfter::report_seconds)
}

/// Which provider family sent a failing reply; decides how "the input is
/// too long" is recognised.
///
/// The Jev service has no documented error code for it, so its 4xx message
/// text is matched. This is a tightening of `plan-anchor`'s `jev_judge.py.j2`
/// rule, not the same rule: that script treats any 4xx whose body mentions
/// `max_tokens` as a context overflow, which would turn "max_tokens must be
/// positive" into `INPUT_TOO_LARGE`. Here the message must name tokens
/// (`max_tokens` contains "token") AND say something is over (`exceed`,
/// `too long`, `too large`, `too many`). Haiku and
/// Luna document structured markers, so only those and status 413 count: a
/// message that merely mentions `max_tokens` (the output cap, a request
/// parameter) is an ordinary invalid request there. The one text rule for
/// them is Anthropic's "prompt is too long" 400 (type
/// `invalid_request_error`); that wording is reported widely but is not in the
/// documented error table, so it is unverified against a live reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReplyShape {
    /// The Jev check service.
    Jev,
    /// An LLM provider (Anthropic, OpenAI).
    Llm,
}

/// Whether a 4xx reply says the input is longer than the model accepts.
fn says_input_too_long(shape: ReplyShape, status: u16, body: &Body, text_lower: &str) -> bool {
    if !matches!(status, 400 | 413 | 422) {
        return false;
    }
    if status == 413 {
        return true;
    }
    let marked = |value: &Option<String>| {
        value
            .as_deref()
            .is_some_and(|text| matches!(text, "context_length_exceeded" | "request_too_large"))
    };
    if marked(&body.provider_code) || marked(&body.provider_type) {
        return true;
    }
    match shape {
        ReplyShape::Jev => {
            let says_over = ["exceed", "too long", "too large", "too many"]
                .iter()
                .any(|word| text_lower.contains(word));
            text_lower.contains("token") && says_over
        }
        // Anthropic reports an over-long prompt as an ordinary 400
        // `invalid_request_error` whose message starts "prompt is too long".
        // That wording is reported widely but is not in the documented error
        // table, so this rule is unverified against a live reply.
        ReplyShape::Llm => {
            body.provider_type.as_deref() == Some("invalid_request_error")
                && text_lower.starts_with("prompt is too long")
        }
    }
}

/// Maps a reply with a failing status to its error.
///
/// `provider` labels the provider in messages (`Jev`, `luna`). The result
/// keeps the provider's status, type, code, message, parameter and request
/// id, each bounded by `sc_agent_types::bound_provider_text`; a body that is not a
/// recognised error JSON falls back to the bounded raw text. The caller's
/// `send_and_parse` redacts the secret from the result.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use sc_agent_http::{Endpoint, HttpRequest, HttpResponse, Method, ReplyShape, api_failure};
///
/// let body = br#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"},"request_id":"req_1"}"#;
/// let request = HttpRequest::new(
///     Method::Post,
///     Endpoint::parse("https://api.example.test/v1/x")?,
///     Arc::from("{}"),
/// );
/// let reply = HttpResponse::new(401, Vec::new(), body.to_vec());
/// let error = api_failure("luna", ReplyShape::Llm, &request, &reply);
/// assert_eq!(error.code(), "AUTH_FAILED");
/// assert!(error.message().contains("invalid x-api-key"));
/// assert!(!error.retryable());
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[must_use]
pub fn api_failure(
    provider: &str,
    shape: ReplyShape,
    request: &HttpRequest,
    response: &HttpResponse,
) -> JevError {
    let status = response.status();
    let (failure, body) = read_failure(provider, request, response);
    // Only the provider's message (or, when the body is not error JSON, the
    // bounded raw text) is matched, never JSON keys.
    let text_lower = failure.message.to_ascii_lowercase();
    if matches!(status, 401 | 403) {
        return JevError::auth_failed(failure);
    }
    if status == 402 || body.marks_quota(status) {
        return JevError::quota_exhausted(failure);
    }
    if says_input_too_long(shape, status, &body, &text_lower) {
        let unit = if status == 413 {
            LimitUnit::Bytes
        } else {
            LimitUnit::Tokens
        };
        return JevError::input_too_large_by_provider(unit, failure);
    }
    if matches!(status, 400 | 422) {
        return JevError::api_request_invalid(failure);
    }
    JevError::api_error(failure)
}

/// What a busy reply said, for `PROVIDER_BUSY`: the same fields as a failing
/// reply, with no provider label yet.
pub(crate) fn busy_reply(request: &HttpRequest, response: &HttpResponse) -> ProviderFailure {
    read_failure("", request, response).0
}

/// The provider's status, type, code, message, parameter, request id and
/// `Retry-After` from a failing reply, bounded. `provider` may be empty (the
/// busy retry does not know the backend name; the caller's label is added
/// later by `JevError::prefix_detail`).
fn read_failure(
    provider: &str,
    request: &HttpRequest,
    response: &HttpResponse,
) -> (ProviderFailure, Body) {
    // The secret is removed from every provider text *before* it is cut to
    // the length bound: a key that straddles the cut would otherwise leave
    // its front half behind.
    let scrub = |text: &str| match request.secret() {
        Some(secret) => secret.key().redact_text(text),
        None => text.to_owned(),
    };
    let bytes = response.body();
    let body = Body::read(bytes);
    // The whole body is scrubbed before anything is cut: the 1024-byte bound
    // happens in `ProviderFailure`, after the key is gone.
    let raw = String::from_utf8_lossy(bytes);
    let message = body.message.as_deref().map_or_else(|| scrub(&raw), scrub);
    let mut failure = ProviderFailure::new(provider, response.status(), &message)
        .with_retry_after(retry_after_seconds(response.header("retry-after")));
    if let Some(text) = &body.provider_type {
        failure = failure.with_type(&scrub(text));
    }
    if let Some(text) = &body.provider_code {
        failure = failure.with_code(&scrub(text));
    }
    if let Some(text) = &body.param {
        failure = failure.with_param(&scrub(text));
    }
    let request_id = body
        .request_id
        .as_deref()
        .or_else(|| response.header("request-id"))
        .or_else(|| response.header("x-request-id"));
    if let Some(text) = request_id {
        failure = failure.with_request_id(&scrub(text));
    }
    (failure, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retry_after::REPORT_MAX_SECONDS;

    fn req() -> HttpRequest {
        HttpRequest::new(
            crate::Method::Post,
            crate::Endpoint::parse("https://api.example.test/v1/x").expect("https"),
            std::sync::Arc::from("{}"),
        )
    }

    fn reply(status: u16, body: &str) -> HttpResponse {
        HttpResponse::new(status, Vec::new(), body.as_bytes().to_vec())
    }

    #[test]
    fn retry_after_rounds_up_and_caps() {
        let cases: &[(Option<&str>, Option<u64>)] = &[
            (None, None),
            (Some("3"), Some(3)),
            (Some("2.5"), Some(3)),
            (Some("2.0"), Some(2)),
            (Some("0"), Some(0)),
            (Some("99999999"), Some(REPORT_MAX_SECONDS)),
            (Some(&"9".repeat(33)), None),
            (Some("-1"), None),
            (Some("Wed, 21 Oct 2026 07:28:00 GMT"), None),
            (Some(""), None),
        ];
        for (header, expected) in cases {
            assert_eq!(retry_after_seconds(*header), *expected, "{header:?}");
        }
    }

    #[test]
    fn quota_markers_come_from_documented_shapes_only() {
        let anthropic_cap = r#"{"type":"error","error":{"type":"rate_limit_error","message":"You have reached your API usage limits","details":{"error_code":"enforced_spend_limit_reached"}}}"#;
        let openai_quota = r#"{"error":{"message":"You exceeded your current quota","type":"insufficient_quota","param":null,"code":"insufficient_quota"}}"#;
        let openai_credit = r#"{"error":{"message":"no credit","type":"billing","code":"credit_balance_exhausted"}}"#;
        let anthropic_limit = r#"{"type":"error","error":{"type":"invalid_request_error","message":"You have reached your specified API usage limits. You will regain access on 2026-12-01."}}"#;
        for (status, body) in [
            (429, anthropic_cap),
            (429, openai_quota),
            (429, openai_credit),
            (400, anthropic_limit),
            (402, "{}"),
        ] {
            assert!(is_quota_reply(status, body.as_bytes()), "{status} {body}");
            assert_eq!(
                api_failure("p", ReplyShape::Llm, &req(), &reply(status, body)).code(),
                "QUOTA_EXHAUSTED"
            );
        }
        let rate = r#"{"error":{"message":"Rate limit reached","type":"requests","code":"rate_limit_exceeded"}}"#;
        let anthropic_rate =
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#;
        for body in [
            rate,
            anthropic_rate,
            "",
            "not json",
            "{}",
            r#"{"error":"x"}"#,
        ] {
            assert!(!is_quota_reply(429, body.as_bytes()), "{body}");
        }
    }

    #[test]
    fn statuses_map_to_the_distinct_ids() {
        let cases = [
            (401, "AUTH_FAILED", false),
            (403, "AUTH_FAILED", false),
            (400, "API_REQUEST_INVALID", false),
            (422, "API_REQUEST_INVALID", false),
            (402, "QUOTA_EXHAUSTED", false),
            (404, "API_ERROR", false),
            (408, "API_ERROR", false),
            (418, "API_ERROR", false),
            (500, "API_ERROR", true),
            (503, "API_ERROR", true),
            (504, "API_ERROR", true),
            (302, "API_ERROR", false),
        ];
        for (status, id, retryable) in cases {
            let error = api_failure("Jev", ReplyShape::Jev, &req(), &reply(status, "oops"));
            assert_eq!(error.code(), id, "{status}");
            assert_eq!(error.retryable(), retryable, "{status}");
            assert!(error.message().contains("oops"), "{status}");
        }
    }

    #[test]
    fn input_too_long_is_one_id_with_units() {
        let openai = r#"{"error":{"message":"maximum context length","type":"invalid_request_error","code":"context_length_exceeded"}}"#;
        let anthropic =
            r#"{"type":"error","error":{"type":"request_too_large","message":"too big"}}"#;
        for (status, body, unit) in [
            (400, openai, "tokens"),
            (413, anthropic, "bytes"),
            (413, "plain", "bytes"),
        ] {
            let error = api_failure("p", ReplyShape::Llm, &req(), &reply(status, body));
            assert_eq!(error.code(), "INPUT_TOO_LARGE", "{status} {body}");
            assert_eq!(error.details().and_then(|d| d.unit).as_deref(), Some(unit));
        }
        // Jev has no documented code: its message text is matched (plan-anchor's rule).
        for body in [
            "request exceeds max_tokens",
            r#"{"detail":"prompt tokens exceed the window"}"#,
            r#"{"detail":"too many tokens"}"#,
        ] {
            let error = api_failure("Jev", ReplyShape::Jev, &req(), &reply(422, body));
            assert_eq!(error.code(), "INPUT_TOO_LARGE", "{body}");
        }
        // A 401 that mentions tokens is still an auth failure.
        assert_eq!(
            api_failure("p", ReplyShape::Jev, &req(), &reply(401, "token exceeded")).code(),
            "AUTH_FAILED"
        );
    }

    /// The exact bodies Jev returned on live calls (2026-11-08): an unfunded
    /// organization gets 402 `billing_error`, ahead of body validation.
    #[test]
    fn jev_out_of_credits_is_quota_exhausted_with_its_message() {
        let message = "Your organization has no available TypeSafe API credits. \
                       Please add more credits and/or set up auto-reload at \
                       https://console.typesafe.ai/settings/billing";
        let body =
            format!(r#"{{"detail":{{"error_type":"billing_error","message":"{message}"}}}}"#);
        assert!(is_quota_reply(402, body.as_bytes()));
        let error = api_failure("Jev", ReplyShape::Jev, &req(), &reply(402, &body));
        assert_eq!(error.code(), "QUOTA_EXHAUSTED");
        assert!(!error.retryable());
        assert!(
            error
                .message()
                .contains("no available TypeSafe API credits")
        );
        assert!(!error.message().contains("error_type"));
        let details = error.details().expect("details");
        assert_eq!(details.provider_type.as_deref(), Some("billing_error"));

        // The marker counts at any status: billing is never retried as busy.
        let on_429 = r#"{"detail":{"error_type":"billing_error","message":"no credits"}}"#;
        assert!(is_quota_reply(429, on_429.as_bytes()));
        let rate = r#"{"detail":{"error_type":"rate_limit_error"}}"#;
        assert!(!is_quota_reply(429, rate.as_bytes()));

        let denied =
            r#"{"detail":{"error_type":"authentication_error","message":"Cannot authenticate"}}"#;
        let error = api_failure("Jev", ReplyShape::Jev, &req(), &reply(401, denied));
        assert_eq!(error.code(), "AUTH_FAILED");
        assert!(error.message().contains("Cannot authenticate"));
    }

    /// RBP-F002: for Haiku and Luna raw text never decides. A 400 that names
    /// `max_tokens` (the output cap parameter) stays an invalid request, and
    /// so does one that mentions tokens exceeding something else.
    #[test]
    fn llm_text_about_max_tokens_is_an_invalid_request() {
        let output_cap = r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: 99999 > 8192, which is the maximum allowed number of output tokens"}}"#;
        let exceed = r#"{"error":{"message":"the number of tokens requested exceeds your plan","type":"invalid_request_error","code":null}}"#;
        for body in [output_cap, exceed, "max_tokens is too large"] {
            let error = api_failure("haiku", ReplyShape::Llm, &req(), &reply(400, body));
            assert_eq!(error.code(), "API_REQUEST_INVALID", "{body}");
        }
        // ERR-QA-26: a Jev message that names max_tokens without saying it is
        // over something is an ordinary invalid request.
        for body in [
            r#"{"detail":"max_tokens must be positive"}"#,
            "max_tokens is required",
        ] {
            assert_eq!(
                api_failure("Jev", ReplyShape::Jev, &req(), &reply(422, body)).code(),
                "API_REQUEST_INVALID",
                "{body}"
            );
        }
        // Jev matching looks at the message, not at JSON key names.
        let key_only = r#"{"detail":"bad","max_tokens":1}"#;
        assert_eq!(
            api_failure("Jev", ReplyShape::Jev, &req(), &reply(422, key_only)).code(),
            "API_REQUEST_INVALID"
        );
    }

    #[test]
    fn provider_fields_are_kept_and_bounded() {
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad field","param":"model"},"request_id":"req_9"}"#;
        let error = api_failure("luna", ReplyShape::Llm, &req(), &reply(400, body));
        assert_eq!(error.field(), Some("model"));
        let details = error.details().expect("details");
        assert_eq!(details.http_status, Some(400));
        assert_eq!(
            details.provider_type.as_deref(),
            Some("invalid_request_error")
        );
        assert_eq!(details.provider_request_id.as_deref(), Some("req_9"));
        assert_eq!(details.provider.as_deref(), Some("luna"));
        let huge = "x".repeat(10_000);
        let error = api_failure("luna", ReplyShape::Llm, &req(), &reply(500, &huge));
        assert!(error.message().len() < 1200, "{}", error.message().len());
        assert!(error.message().contains("..."));
    }

    #[test]
    fn jev_style_bodies_and_raw_fallback() {
        let detail =
            r#"{"detail":[{"loc":["body","questions",0,"choices"],"msg":"too few choices"}]}"#;
        let error = api_failure("Jev", ReplyShape::Jev, &req(), &reply(422, detail));
        assert_eq!(error.field(), Some("choices"));
        assert!(error.message().contains("too few choices"));
        let plain = api_failure(
            "Jev",
            ReplyShape::Jev,
            &req(),
            &reply(422, r#"{"detail":"bad question"}"#),
        );
        assert!(plain.message().contains("bad question"));
        let raw = api_failure(
            "Jev",
            ReplyShape::Jev,
            &req(),
            &reply(502, "<html>\nBad gateway\n</html>"),
        );
        assert!(
            raw.message().contains("<html> Bad gateway </html>"),
            "{}",
            raw.message()
        );
    }

    #[test]
    fn retry_after_header_is_carried_on_api_errors() {
        let response = HttpResponse::new(
            503,
            vec![("Retry-After".to_owned(), "7".to_owned())],
            Vec::new(),
        );
        assert_eq!(
            api_failure("p", ReplyShape::Llm, &req(), &response).retry_after_seconds(),
            Some(7)
        );
    }

    /// ERR-QA-4: a 422 that blames `max_tokens` by location but whose message
    /// is about a value is an invalid request, and keeps `field`.
    #[test]
    fn jev_422_blaming_max_tokens_by_location_keeps_the_field() {
        let body = r#"{"detail":[{"loc":["body","max_tokens"],"msg":"value must be positive"}]}"#;
        let error = api_failure("Jev", ReplyShape::Jev, &req(), &reply(422, body));
        assert_eq!(error.code(), "API_REQUEST_INVALID");
        assert_eq!(error.field(), Some("max_tokens"));
    }

    /// ERR-QA-17 (unverified wording): Anthropic's "prompt is too long".
    #[test]
    fn anthropic_prompt_too_long_is_input_too_large() {
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 215000 tokens > 200000 maximum"}}"#;
        let error = api_failure("haiku", ReplyShape::Llm, &req(), &reply(400, body));
        assert_eq!(error.code(), "INPUT_TOO_LARGE");
        let other =
            r#"{"type":"error","error":{"type":"api_error","message":"prompt is too long"}}"#;
        assert_eq!(
            api_failure("haiku", ReplyShape::Llm, &req(), &reply(400, other)).code(),
            "API_REQUEST_INVALID"
        );
    }

    /// A body far over any read cut, with the key straddling the 1024-byte
    /// bound: nothing of the key survives.
    #[test]
    fn key_in_a_large_raw_body_is_scrubbed_before_the_cut() {
        use sc_agent_types::{BackendKeyId, ResolvedApiKey};
        const KEY: &str = "sk-big-body-secret-0123456789";
        let key = ResolvedApiKey::try_new(BackendKeyId::OpenAi, KEY.to_owned()).expect("key");
        let request = HttpRequest::new(
            crate::Method::Post,
            crate::Endpoint::parse("https://api.example.test/v1/x").expect("https"),
            std::sync::Arc::from("{}"),
        )
        .with_secret(crate::SecretHeader::bearer(key));
        for lead in [1000_usize, 1010, 1020, 4090, 4096, 4100] {
            let body = format!("{}{KEY}{}", "a".repeat(lead), "z".repeat(6000));
            let error = api_failure("p", ReplyShape::Llm, &request, &reply(500, &body));
            let rendered = error.message();
            for size in 4..=KEY.len() {
                assert!(
                    !rendered.contains(&KEY[..size])
                        && !rendered.contains(&KEY[KEY.len() - size..]),
                    "lead {lead}: a {size}-byte key fragment survived"
                );
            }
        }
    }

    /// ERR-QA-3: the key is removed before the 1024-byte cut, so a key that
    /// straddles the cut leaves no fragment, in the message or any other
    /// provider field.
    #[test]
    fn key_straddling_the_length_cut_leaves_no_fragment() {
        use sc_agent_types::{BackendKeyId, PROVIDER_TEXT_MAX_BYTES, ResolvedApiKey};
        const KEY: &str = "sk-straddle-secret-0123456789";
        let key = ResolvedApiKey::try_new(BackendKeyId::OpenAi, KEY.to_owned()).expect("key");
        let request = HttpRequest::new(
            crate::Method::Post,
            crate::Endpoint::parse("https://api.example.test/v1/x").expect("https"),
            std::sync::Arc::from("{}"),
        )
        .with_secret(crate::SecretHeader::bearer(key));
        // Every offset of the key around the cut, for the message, the raw
        // fallback and the type field.
        for lead in (PROVIDER_TEXT_MAX_BYTES - KEY.len() - 4)..=(PROVIDER_TEXT_MAX_BYTES + 2) {
            let text = format!("{}{KEY}tail", "a".repeat(lead));
            let json = format!(r#"{{"error":{{"type":"{text}","message":"{text}"}}}}"#);
            for body in [text.clone(), json] {
                let error = api_failure("p", ReplyShape::Llm, &request, &reply(500, &body));
                let rendered = format!(
                    "{} {}",
                    error.message(),
                    serde_json::to_string(&error.details()).expect("json")
                );
                for size in 4..=KEY.len() {
                    assert!(
                        !rendered.contains(&KEY[..size])
                            && !rendered.contains(&KEY[KEY.len() - size..]),
                        "lead {lead}: a {size}-byte key fragment survived"
                    );
                }
            }
        }
    }
}
