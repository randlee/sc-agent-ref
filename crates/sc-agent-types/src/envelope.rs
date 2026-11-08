//! CLI stderr JSON shape (NFR-SAG-0001, ADR-SAG-0027 field rule).
//!
//! Success envelopes stay in the composition roots. This crate only defines
//! the error body so every CLI crate copies ids from [`crate::JevError`].

use serde::{Deserialize, Serialize};

use crate::code::ErrorCategory;
use crate::error::JevError;

/// The only `schema_version` this crate writes or accepts.
pub(crate) const ERROR_RESPONSE_SCHEMA_VERSION: u32 = 1;

/// stderr body for a failed CLI command.
///
/// Serialized keys, in order: `error`, `kind`, `message`,
/// `suggested_action`, `retryable`, `retry_after_seconds`, `field`,
/// `details`, `schema_version`.
///
/// - `error` is the stable id ([`crate::ErrorCode`]); `kind` is its broad
///   class ([`ErrorCategory`]).
/// - `message` says what failed and why. It never holds the fix.
///   `suggested_action` says what to do next. Neither holds a secret.
/// - `retryable` is always present, `false` included.
/// - `retry_after_seconds`, `field` and `details` are present only when the
///   error has them. `details` is a closed object ([`ErrorDetails`]).
/// - `schema_version` is always `1` from [`Self::from_jev_error`]. It
///   versions this error shape only, independent of the stdout envelope
///   `version`.
///
/// Reading (producer stderr) denies unknown keys at the top level and inside
/// `details`, and accepts `schema_version` absent or `1`; anything else does
/// not parse. Only `error` and `message` are required, so a producer that
/// writes the short shape still parses. Consumers that see a
/// `schema_version` other than `1` treat the line as an unknown error shape
/// and fall back to the exit code and the raw line.
///
/// # Examples
///
/// ```
/// use sc_agent_types::{JevError, ScAgentErrorResponse};
///
/// let body = ScAgentErrorResponse::from_jev_error(&JevError::InputEmpty);
/// assert_eq!(
///     serde_json::to_string(&body).unwrap(),
///     r#"{"error":"INPUT_EMPTY","kind":"validation","message":"input is empty","suggested_action":"Provide a non-empty input and retry.","retryable":false,"schema_version":1}"#
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawErrorResponse")]
pub struct ScAgentErrorResponse {
    error: String,
    kind: Option<ErrorCategory>,
    message: String,
    suggested_action: String,
    retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<ErrorDetails>,
    schema_version: u32,
}

/// The closed `details` object of an error response: the machine fields of
/// one error.
///
/// Every key is optional and written only when the error has it. Which keys
/// an error carries is part of its id's contract; see `docs/stability.md`.
/// Text values come from redacted, bounded sources.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ErrorDetails {
    /// The producer that reported the error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    /// The provider's label, such as `Jev` or `luna`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The provider's HTTP status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    /// The provider's error type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_type: Option<String>,
    /// The provider's error code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_code: Option<String>,
    /// The provider's request id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_request_id: Option<String>,
    /// The sub-cause: `sends` or `budget` (`PROVIDER_BUSY`);
    /// `truncated`, `too_large`, `not_json` or `schema` (`RESPONSE_INVALID`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    /// Sends made before a busy provider was given up on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    /// The cap of `INPUT_TOO_LARGE`, in `unit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    /// The measured size of `INPUT_TOO_LARGE`, in `unit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<u64>,
    /// `bytes` or `tokens`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// What an `IO_FAILED` operation was about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what: Option<String>,
    /// The `io::ErrorKind` of `IO_FAILED`, snake_case (`not_found`,
    /// `permission_denied`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_kind: Option<String>,
    /// The `NETWORK_ERROR` class: `timeout`, `connect`, `tls`, `cancelled`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_kind: Option<String>,
    /// The missing path of `INPUT_MISSING` and `PROMPT_FILE_MISSING`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// A session-log flush failure that happened while the error left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_flush_failure: Option<String>,
}

impl ErrorDetails {
    /// Rewrites every text value with `map`.
    pub(crate) fn map_text(self, map: &impl Fn(&str) -> String) -> Self {
        let text = |value: Option<String>| value.as_deref().map(map);
        Self {
            producer: text(self.producer),
            provider: text(self.provider),
            http_status: self.http_status,
            provider_type: text(self.provider_type),
            provider_code: text(self.provider_code),
            provider_request_id: text(self.provider_request_id),
            cause: text(self.cause),
            attempts: self.attempts,
            limit: self.limit,
            actual: self.actual,
            unit: text(self.unit),
            what: text(self.what),
            io_kind: text(self.io_kind),
            network_kind: text(self.network_kind),
            path: text(self.path),
            log_flush_failure: text(self.log_flush_failure),
        }
    }

    /// Copies the provider's status, label, type, code and request id.
    pub(crate) fn set_provider(&mut self, failure: &crate::error::ProviderFailure) {
        self.http_status = Some(failure.status);
        self.provider = Some(failure.provider.clone());
        self.provider_type.clone_from(&failure.provider_type);
        self.provider_code.clone_from(&failure.provider_code);
        self.provider_request_id.clone_from(&failure.request_id);
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawErrorResponse {
    error: String,
    #[serde(default)]
    kind: Option<ErrorCategory>,
    message: String,
    #[serde(default)]
    suggested_action: String,
    #[serde(default)]
    retryable: bool,
    #[serde(default)]
    retry_after_seconds: Option<u64>,
    #[serde(default)]
    field: Option<String>,
    #[serde(default)]
    details: Option<ErrorDetails>,
    #[serde(default)]
    schema_version: Option<u32>,
}

impl TryFrom<RawErrorResponse> for ScAgentErrorResponse {
    type Error = String;

    fn try_from(raw: RawErrorResponse) -> Result<Self, Self::Error> {
        match raw.schema_version {
            None | Some(ERROR_RESPONSE_SCHEMA_VERSION) => Ok(Self {
                error: raw.error,
                kind: raw.kind,
                message: raw.message,
                suggested_action: raw.suggested_action,
                retryable: raw.retryable,
                retry_after_seconds: raw.retry_after_seconds,
                field: raw.field,
                details: raw.details,
                schema_version: ERROR_RESPONSE_SCHEMA_VERSION,
            }),
            Some(other) => Err(format!(
                "error response schema_version {other} is not {ERROR_RESPONSE_SCHEMA_VERSION}"
            )),
        }
    }
}

impl ScAgentErrorResponse {
    /// Builds the response for `error` by the field rule above.
    ///
    /// This is the only Rust constructor. `message` is
    /// [`JevError::message`] and `suggested_action` is
    /// [`JevError::suggested_action`].
    #[must_use]
    pub fn from_jev_error(error: &JevError) -> Self {
        Self {
            error: error.code().as_str().to_owned(),
            kind: Some(error.category()),
            message: error.message(),
            suggested_action: error.suggested_action(),
            retryable: error.retryable(),
            retry_after_seconds: error.retry_after_seconds(),
            field: error.field().map(str::to_owned),
            details: error.details().filter(|details| !details.is_empty()),
            schema_version: ERROR_RESPONSE_SCHEMA_VERSION,
        }
    }

    /// Returns the stable error id.
    #[must_use]
    pub fn error(&self) -> &str {
        &self.error
    }

    /// The broad class; `None` when a producer's short shape omitted it.
    #[must_use]
    pub fn kind(&self) -> Option<ErrorCategory> {
        self.kind
    }

    /// What failed and why.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// What to do next; empty when the producer sent none.
    #[must_use]
    pub fn suggested_action(&self) -> &str {
        &self.suggested_action
    }

    /// Returns the shape version (always `1`).
    #[must_use]
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Whether the error is retryable.
    #[must_use]
    pub fn retryable(&self) -> bool {
        self.retryable
    }

    /// Seconds to wait before a retry, when known.
    #[must_use]
    pub fn retry_after_seconds(&self) -> Option<u64> {
        self.retry_after_seconds
    }

    /// The failing field, flag or argument, when the error names one.
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// The machine fields, when the error has any.
    #[must_use]
    pub fn details(&self) -> Option<&ErrorDetails> {
        self.details.as_ref()
    }

    /// The producer named in `details`, if any.
    #[must_use]
    pub fn producer(&self) -> Option<&str> {
        self.details
            .as_ref()
            .and_then(|details| details.producer.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_includes_safe_detail_and_no_secret_key() {
        let body = ScAgentErrorResponse::from_jev_error(&JevError::config_invalid("missing model"));
        assert!(body.message().contains("missing model"));
        assert!(!body.message().contains("check.toml"));
        assert!(body.suggested_action().contains("check.toml"));
        let value = serde_json::to_value(&body).expect("serialize");
        for key in value.as_object().expect("object").keys() {
            let lower = key.to_ascii_lowercase();
            for forbidden in ["secret", "token", "password", "authorization", "credential"] {
                assert!(!lower.contains(forbidden), "{key}");
            }
        }
    }

    #[test]
    fn retryable_is_always_written() {
        let value =
            serde_json::to_value(ScAgentErrorResponse::from_jev_error(&JevError::InputEmpty))
                .expect("serialize");
        assert_eq!(value["retryable"], false);
    }

    #[test]
    fn reader_rejects_unknown_keys_and_versions() {
        for line in [
            r#"{"error":"INPUT_EMPTY","message":"m","extra":1}"#,
            r#"{"error":"INPUT_EMPTY","message":"m","schema_version":2}"#,
            r#"{"error":"INPUT_EMPTY","message":"m","details":{"producer":"p","x":1}}"#,
            r#"{"error":"INPUT_EMPTY","message":"m","kind":"nope"}"#,
        ] {
            assert!(
                serde_json::from_str::<ScAgentErrorResponse>(line).is_err(),
                "{line}"
            );
        }
        let short: ScAgentErrorResponse =
            serde_json::from_str(r#"{"error":"INPUT_EMPTY","message":"m"}"#).expect("short");
        assert_eq!(short.schema_version(), 1);
        assert_eq!(short.suggested_action(), "");
        assert!(!short.retryable());
    }
}
