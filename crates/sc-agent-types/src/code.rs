//! Stable error ids and the closed enums `JevError` variants carry.
//!
//! Private module, reached through the crate-root re-exports. Kept apart
//! from `error` because sc-boundary reports a type cycle inside one module.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::JevError;

/// One stable error id of the README `errors.json` contract.
///
/// `JevError::code` returns this type. [`Self::ALL`] lists every id; the
/// `errors` integration test compares it with `errors.json`, and an
/// exhaustive in-crate test keeps [`Self::ALL`] complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErrorCode {
    /// `CONFIG_INVALID`.
    ConfigInvalid,
    /// `KEY_MISSING`.
    KeyMissing,
    /// `INPUT_TOO_LARGE`.
    InputTooLarge,
    /// `INPUT_EMPTY`.
    InputEmpty,
    /// `INPUT_INVALID`.
    InputInvalid,
    /// `EXTRACTION_EMPTY`.
    ExtractionEmpty,
    /// `EXTRACTION_PARSE_ERROR`.
    ExtractionParseError,
    /// `EXTRACTION_INVALID`.
    ExtractionInvalid,
    /// `EXTRACTION_FAILED`.
    ExtractionFailed,
    /// `EXTRACTION_TIMEOUT`.
    ExtractionTimeout,
    /// `LANGUAGE_UNAVAILABLE`.
    LanguageUnavailable,
    /// `QUERY_INVALID`.
    QueryInvalid,
    /// `QUERY_NOT_FOUND`.
    QueryNotFound,
    /// `INPUT_MISSING`.
    InputMissing,
    /// `RESPONSE_INVALID`.
    ResponseInvalid,
    /// `NETWORK_ERROR`.
    NetworkError,
    /// `PROMPT_FILE_MISSING`.
    PromptFileMissing,
    /// `SESSION_NOT_FOUND`.
    SessionNotFound,
    /// `SESSION_EXPIRED`.
    SessionExpired,
    /// `AUTH_FAILED`: the provider rejected the credential (HTTP 401, 403).
    AuthFailed,
    /// `API_REQUEST_INVALID`: the provider rejected the request (HTTP 400, 422).
    ApiRequestInvalid,
    /// `QUOTA_EXHAUSTED`: out of tokens, credit or spend allowance.
    QuotaExhausted,
    /// `PROVIDER_BUSY`: rate limit or overload that outlasted the busy retry.
    ProviderBusy,
    /// `API_ERROR`: any other non-success provider status, 5xx included.
    ApiError,
    /// `IO_FAILED`: a file, process, stream or platform dependency failed.
    IoFailed,
}

impl ErrorCode {
    /// Every error id, in `errors.json` order.
    pub const ALL: &'static [Self] = &[
        Self::ConfigInvalid,
        Self::KeyMissing,
        Self::InputTooLarge,
        Self::InputEmpty,
        Self::InputInvalid,
        Self::ExtractionEmpty,
        Self::ExtractionParseError,
        Self::ExtractionInvalid,
        Self::ExtractionFailed,
        Self::ExtractionTimeout,
        Self::LanguageUnavailable,
        Self::QueryInvalid,
        Self::QueryNotFound,
        Self::InputMissing,
        Self::ResponseInvalid,
        Self::NetworkError,
        Self::PromptFileMissing,
        Self::SessionNotFound,
        Self::SessionExpired,
        Self::AuthFailed,
        Self::ApiRequestInvalid,
        Self::QuotaExhausted,
        Self::ProviderBusy,
        Self::ApiError,
        Self::IoFailed,
    ];

    /// The stable id text, such as `INPUT_INVALID`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigInvalid => "CONFIG_INVALID",
            Self::KeyMissing => "KEY_MISSING",
            Self::InputTooLarge => "INPUT_TOO_LARGE",
            Self::InputEmpty => "INPUT_EMPTY",
            Self::InputInvalid => "INPUT_INVALID",
            Self::ExtractionEmpty => "EXTRACTION_EMPTY",
            Self::ExtractionParseError => "EXTRACTION_PARSE_ERROR",
            Self::ExtractionInvalid => "EXTRACTION_INVALID",
            Self::ExtractionFailed => "EXTRACTION_FAILED",
            Self::ExtractionTimeout => "EXTRACTION_TIMEOUT",
            Self::LanguageUnavailable => "LANGUAGE_UNAVAILABLE",
            Self::QueryInvalid => "QUERY_INVALID",
            Self::QueryNotFound => "QUERY_NOT_FOUND",
            Self::InputMissing => "INPUT_MISSING",
            Self::ResponseInvalid => "RESPONSE_INVALID",
            Self::NetworkError => "NETWORK_ERROR",
            Self::PromptFileMissing => "PROMPT_FILE_MISSING",
            Self::SessionNotFound => "SESSION_NOT_FOUND",
            Self::SessionExpired => "SESSION_EXPIRED",
            Self::AuthFailed => "AUTH_FAILED",
            Self::ApiRequestInvalid => "API_REQUEST_INVALID",
            Self::QuotaExhausted => "QUOTA_EXHAUSTED",
            Self::ProviderBusy => "PROVIDER_BUSY",
            Self::ApiError => "API_ERROR",
            Self::IoFailed => "IO_FAILED",
        }
    }

    /// The default [`ErrorCategory`] of this id.
    ///
    /// `NETWORK_ERROR` is `dependency` here; [`JevError::category`] narrows
    /// a timeout to `timeout`.
    #[must_use]
    pub const fn category(self) -> ErrorCategory {
        match self {
            Self::ConfigInvalid
            | Self::InputTooLarge
            | Self::InputEmpty
            | Self::InputInvalid
            | Self::ExtractionParseError
            | Self::LanguageUnavailable
            | Self::QueryInvalid
            | Self::ApiRequestInvalid => ErrorCategory::Validation,
            Self::QueryNotFound
            | Self::InputMissing
            | Self::PromptFileMissing
            | Self::SessionNotFound => ErrorCategory::NotFound,
            Self::ExtractionEmpty | Self::SessionExpired => ErrorCategory::InvalidState,
            Self::KeyMissing | Self::AuthFailed => ErrorCategory::Permission,
            Self::ExtractionTimeout => ErrorCategory::Timeout,
            Self::QuotaExhausted | Self::ProviderBusy => ErrorCategory::Capacity,
            Self::ExtractionInvalid
            | Self::ExtractionFailed
            | Self::ResponseInvalid
            | Self::NetworkError
            | Self::ApiError
            | Self::IoFailed => ErrorCategory::Dependency,
        }
    }
}

/// The broad class of an error, next to its stable id.
///
/// The `kind` key of the stderr body and the Python `kind` attribute. The
/// spelling is snake_case. Callers branch on the id for the exact failure and
/// on the category for a policy (fix the input, wait, give up).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCategory {
    /// The caller's input, flags or configuration break a rule.
    Validation,
    /// A named file, query or session does not exist.
    NotFound,
    /// The request is valid but the current state refuses it.
    InvalidState,
    /// A dependency (provider, process, file system, producer) failed.
    Dependency,
    /// A credential is missing or rejected.
    Permission,
    /// A deadline expired.
    Timeout,
    /// The provider has no capacity for this caller right now (busy or out
    /// of quota).
    Capacity,
}

impl ErrorCategory {
    /// The snake_case spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::NotFound => "not_found",
            Self::InvalidState => "invalid_state",
            Self::Dependency => "dependency",
            Self::Permission => "permission",
            Self::Timeout => "timeout",
            Self::Capacity => "capacity",
        }
    }
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ErrorCode {
    type Err = JevError;

    /// Parses one of the [`Self::ALL`] ids.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::ErrorCode;
    ///
    /// assert_eq!("INPUT_EMPTY".parse::<ErrorCode>().unwrap(), ErrorCode::InputEmpty);
    /// assert!("NOPE".parse::<ErrorCode>().is_err());
    /// ```
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|code| code.as_str() == text)
            .ok_or_else(|| JevError::input_invalid(format!("{text:?} is not a stable error id")))
    }
}

impl PartialEq<str> for ErrorCode {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for ErrorCode {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<ErrorCode> for &str {
    fn eq(&self, other: &ErrorCode) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<str> for ErrorCode {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Transport failure class of a [`JevError::NetworkError`].
///
/// A transport failure is one where no usable HTTP reply arrived. A reply
/// with a failing status is never a `NetworkErrorKind`: it is `AUTH_FAILED`,
/// `API_REQUEST_INVALID`, `QUOTA_EXHAUSTED`, `PROVIDER_BUSY`, `API_ERROR` or
/// `INPUT_TOO_LARGE`. The one spelling is snake_case for serde, `Display`,
/// and `FromStr`; it is the `network_kind` key of the stderr `details`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum NetworkErrorKind {
    /// The request timed out.
    Timeout,
    /// The connection could not be opened.
    Connect,
    /// The TLS handshake failed.
    Tls,
    /// The caller cancelled the request.
    Cancelled,
    /// A transport error that is none of the above. Not retryable: an
    /// unclassified failure is not assumed to be transient.
    Other,
}

impl NetworkErrorKind {
    /// Every kind, in declaration order.
    pub const ALL: &'static [Self] = &[
        Self::Timeout,
        Self::Connect,
        Self::Tls,
        Self::Cancelled,
        Self::Other,
    ];

    /// The snake_case spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Connect => "connect",
            Self::Tls => "tls",
            Self::Cancelled => "cancelled",
            Self::Other => "other",
        }
    }

    /// Whether a later retry may succeed (`Timeout`, `Connect`).
    #[must_use]
    pub const fn retryable(self) -> bool {
        matches!(self, Self::Timeout | Self::Connect)
    }
}

impl fmt::Display for NetworkErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for NetworkErrorKind {
    type Err = JevError;

    /// Parses the snake_case spelling; anything else is `INPUT_INVALID`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == text)
            .ok_or_else(|| {
                JevError::input_invalid(format!(
                    "network error kind {text:?} is not timeout, connect, tls, cancelled, or other"
                ))
            })
    }
}

/// Why a busy provider gave up after the busy retry (`PROVIDER_BUSY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BusyCause {
    /// The last allowed send was busy.
    Sends,
    /// The retry budget left no time for another attempt.
    Budget,
}

impl BusyCause {
    /// The snake_case spelling (`sends`, `budget`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sends => "sends",
            Self::Budget => "budget",
        }
    }
}

/// What was wrong with a reply that arrived with status 200
/// (`RESPONSE_INVALID`).
///
/// No cause is retryable: an invalid reply is deterministic for the same
/// request, and resending a request the provider already billed could
/// double-charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ResponseInvalidCause {
    /// The provider stopped at its output limit; the reply is cut short.
    Truncated,
    /// The reply body exceeded the read cap.
    TooLarge,
    /// The reply body, or the model's answer inside it, is not JSON, or not
    /// the provider's JSON shape.
    NotJson,
    /// The reply parsed but breaks the answer schema (a wrong id, an
    /// out-of-range value, a missing answer).
    Schema,
}

impl ResponseInvalidCause {
    /// The snake_case spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Truncated => "truncated",
            Self::TooLarge => "too_large",
            Self::NotJson => "not_json",
            Self::Schema => "schema",
        }
    }
}

/// The unit of an [`INPUT_TOO_LARGE`](ErrorCode::InputTooLarge) limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LimitUnit {
    /// Request body bytes.
    Bytes,
    /// Model input tokens.
    Tokens,
}

impl LimitUnit {
    /// The snake_case spelling (`bytes`, `tokens`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bytes => "bytes",
            Self::Tokens => "tokens",
        }
    }
}
