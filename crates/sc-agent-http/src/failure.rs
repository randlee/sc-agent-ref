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
//! | 400, 413, 422 whose body says the input is too long (below) | `INPUT_TOO_LARGE` (unit `bytes` for 413, else `tokens`) |
//! | 400, 422 | `API_REQUEST_INVALID` |
//! | every other status, 5xx included | `API_ERROR` |
//!
//! 429, 502 and 529 are busy and retried by [`send_with_busy_retry`]
//! ([`crate::send_with_busy_retry`]); they reach this module only with a
//! quota body, which [`retry`](crate::retry) hands over instead of retrying.
//!
//! # Documented body shapes
//!
//! The bodies below are the shapes the providers document; none was
//! observed against a live endpoint in this repository.
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
//!   body mentions `max_tokens`, or `token` and `exceed`, is a context
//!   overflow; 401 is a credential failure; 422 carries the field error text;
//!   429 and 529 are busy; 5xx is transient. Jev's own error JSON is not
//!   documented here, so the reader also accepts `{"detail": "..."}`,
//!   `{"detail": [{"loc": [...], "msg": "..."}]}` and `{"message": "..."}`,
//!   and falls back to the bounded raw body.
//!
//! A body that is none of these stays what its status says: an unknown 429
//! body is busy, never quota.

use sc_agent_types::{JevError, LimitUnit, ProviderFailure, bound_provider_text};
use serde_json::Value;

use crate::request::HttpResponse;

/// Largest `Retry-After` reported, in seconds (one day).
const RETRY_AFTER_REPORT_MAX: u64 = 86_400;

/// Longest `Retry-After` value read, in bytes.
const RETRY_AFTER_MAX_LEN: usize = 32;

/// How much of an unparsed body is read for the raw-text fallback.
const RAW_FALLBACK_BYTES: usize = 4096;

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

/// Whole seconds of a valid `Retry-After` header, for reporting.
///
/// Valid is 1 to 32 bytes of digits with an optional decimal part; a
/// fraction rounds up. The value is capped at one day. An HTTP-date, a
/// negative number and anything else is `None`. This is for the error
/// fields only; the wait the retry loop honours is clamped separately.
pub(crate) fn retry_after_seconds(header: Option<&str>) -> Option<u64> {
    let text = header?;
    if text.is_empty() || text.len() > RETRY_AFTER_MAX_LEN {
        return None;
    }
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(whole) || fraction.is_some_and(|part| !digits(part)) {
        return None;
    }
    let seconds = whole.bytes().fold(0_u64, |acc, byte| {
        acc.saturating_mul(10)
            .saturating_add(u64::from(byte - b'0'))
    });
    let rounds_up = fraction.is_some_and(|part| part.bytes().any(|byte| byte != b'0'));
    Some(
        seconds
            .saturating_add(u64::from(rounds_up))
            .min(RETRY_AFTER_REPORT_MAX),
    )
}

/// Whether a 4xx body says the input is longer than the model accepts.
fn says_input_too_long(status: u16, body: &Body, raw_lower: &str) -> bool {
    if !matches!(status, 400 | 413 | 422) {
        return false;
    }
    let marked = |value: &Option<String>| {
        value
            .as_deref()
            .is_some_and(|text| matches!(text, "context_length_exceeded" | "request_too_large"))
    };
    status == 413
        || marked(&body.provider_code)
        || marked(&body.provider_type)
        || raw_lower.contains("max_tokens")
        || (raw_lower.contains("token") && raw_lower.contains("exceed"))
}

/// Maps a reply with a failing status to its error.
///
/// `provider` labels the provider in messages (`Jev`, `luna`). The result
/// keeps the provider's status, type, code, message, parameter and request
/// id, each bounded by [`bound_provider_text`]; a body that is not a
/// recognised error JSON falls back to the bounded raw text. The caller's
/// `send_and_parse` redacts the secret from the result.
///
/// # Examples
///
/// ```
/// use sc_agent_http::{HttpResponse, api_failure};
///
/// let body = br#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"},"request_id":"req_1"}"#;
/// let error = api_failure("luna", &HttpResponse::new(401, Vec::new(), body.to_vec()));
/// assert_eq!(error.code(), "AUTH_FAILED");
/// assert!(error.message().contains("invalid x-api-key"));
/// assert!(!error.retryable());
/// ```
#[must_use]
pub fn api_failure(provider: &str, response: &HttpResponse) -> JevError {
    let status = response.status();
    let bytes = response.body();
    let body = Body::read(bytes);
    let raw_len = bytes.len().min(RAW_FALLBACK_BYTES);
    let raw = String::from_utf8_lossy(&bytes[..raw_len]).into_owned();
    let raw_lower = raw.to_ascii_lowercase();
    let message = body
        .message
        .clone()
        .unwrap_or_else(|| bound_provider_text(&raw));
    let mut failure = ProviderFailure::new(provider, status, &message)
        .with_retry_after(retry_after_seconds(response.header("retry-after")));
    if let Some(text) = &body.provider_type {
        failure = failure.with_type(text);
    }
    if let Some(text) = &body.provider_code {
        failure = failure.with_code(text);
    }
    if let Some(text) = &body.param {
        failure = failure.with_param(text);
    }
    let request_id = body
        .request_id
        .as_deref()
        .or_else(|| response.header("request-id"))
        .or_else(|| response.header("x-request-id"));
    if let Some(text) = request_id {
        failure = failure.with_request_id(text);
    }
    if matches!(status, 401 | 403) {
        return JevError::auth_failed(failure);
    }
    if status == 402 || body.marks_quota(status) {
        return JevError::quota_exhausted(failure);
    }
    if says_input_too_long(status, &body, &raw_lower) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
            (Some("99999999"), Some(RETRY_AFTER_REPORT_MAX)),
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
                api_failure("p", &reply(status, body)).code(),
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
            let error = api_failure("Jev", &reply(status, "oops"));
            assert_eq!(error.code(), id, "{status}");
            assert_eq!(error.retryable(), retryable, "{status}");
            assert!(error.message().contains("oops"), "{status}");
        }
    }

    #[test]
    fn input_too_long_is_one_id_with_units() {
        let openai = r#"{"error":{"message":"maximum context length","type":"invalid_request_error","code":"context_length_exceeded"}}"#;
        let jev = "request exceeds max_tokens";
        let anthropic =
            r#"{"type":"error","error":{"type":"request_too_large","message":"too big"}}"#;
        for (status, body, unit) in [
            (400, openai, "tokens"),
            (422, jev, "tokens"),
            (413, anthropic, "bytes"),
            (400, "prompt tokens exceed the window", "tokens"),
        ] {
            let error = api_failure("p", &reply(status, body));
            assert_eq!(error.code(), "INPUT_TOO_LARGE", "{status} {body}");
            assert_eq!(error.details().and_then(|d| d.unit).as_deref(), Some(unit));
        }
        // A 401 that mentions tokens is still an auth failure.
        assert_eq!(
            api_failure("p", &reply(401, "token exceeded")).code(),
            "AUTH_FAILED"
        );
    }

    #[test]
    fn provider_fields_are_kept_and_bounded() {
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad field","param":"model"},"request_id":"req_9"}"#;
        let error = api_failure("luna", &reply(400, body));
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
        let error = api_failure("luna", &reply(500, &huge));
        assert!(error.message().len() < 1200, "{}", error.message().len());
        assert!(error.message().contains("..."));
    }

    #[test]
    fn jev_style_bodies_and_raw_fallback() {
        let detail =
            r#"{"detail":[{"loc":["body","questions",0,"choices"],"msg":"too few choices"}]}"#;
        let error = api_failure("Jev", &reply(422, detail));
        assert_eq!(error.field(), Some("choices"));
        assert!(error.message().contains("too few choices"));
        let plain = api_failure("Jev", &reply(422, r#"{"detail":"bad question"}"#));
        assert!(plain.message().contains("bad question"));
        let raw = api_failure("Jev", &reply(502, "<html>\nBad gateway\n</html>"));
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
        assert_eq!(api_failure("p", &response).retry_after_seconds(), Some(7));
    }
}
