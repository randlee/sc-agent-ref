//! `JevError` is the only CLI and PyO3 error-id source (ADR-SAG-0001).
//!
//! Every variant maps to exactly one id of
//! `tests/fixtures/jev/README-contract/errors.json`; an id may have several
//! variants (REQ-SAG-0101 as amended by r-1). Three renderers read a variant
//! and nothing else (ADR-SAG-0027 as amended):
//!
//! - [`JevError::message`] says what failed and why (the `Display` text);
//! - [`JevError::suggested_action`] says what to do next;
//! - [`JevError::details`] holds the machine fields.
//!
//! No detail text is parsed for a prefix, and the message never holds the
//! fix hint.
//!
//! # Which id for which failure
//!
//! One rule decides between the look-alike ids, and every call site follows
//! it:
//!
//! - `INPUT_INVALID`: what the caller handed in is malformed or breaks a
//!   rule. That includes command-line usage errors (an unknown flag, a
//!   malformed value, a bad session id) and a stored document that cannot
//!   be read as what it claims to be.
//! - `CONFIG_INVALID`: a configuration source (`check.toml`, `jev.toml`, the
//!   backend registry, a credential's content, the repo-root or log setup
//!   that a flag or the environment selects) is wrong.
//! - `INPUT_MISSING`: a path the caller named does not exist (the OS said
//!   not found), whichever code path read it.
//! - `IO_FAILED`: the operating system or a dependency failed while doing
//!   the work: a read that is not "not found", a write, `git` that cannot
//!   start, stdin or stdout, the working directory, the session store, a
//!   credential store that cannot be opened. It carries `what` and the
//!   [`io::ErrorKind`].
//!
//! # Construction
//!
//! The enum and every struct-like variant are `#[non_exhaustive]`, so other
//! crates build errors only through the snake-case constructors
//! ([`JevError::input_invalid`], [`JevError::network`], ...). Matching from
//! another crate needs `{ .. }` patterns and a wildcard arm.
//!
//! # Sources
//!
//! [`std::error::Error::source`] is `None` for every variant. The variants
//! that summarise a cause (`CredentialStoreInvalid`, `NetworkError`,
//! `EncodeFailed`, `IoFailed`) keep only a redacted label or an
//! [`io::ErrorKind`], never the original error, so no unredacted text can be
//! reached through the source chain.
//!
//! # Provider text
//!
//! The provider's own error text (status, type, message) is kept on the
//! provider variants, bounded to [`PROVIDER_TEXT_MAX_BYTES`] bytes and
//! single-line ([`bound_provider_text`]). The HTTP layer redacts the secret
//! from it before it leaves (ADR-SAG-0020). Prompt text and model output are
//! never added; they appear only if the provider's own error text repeats
//! them, and then the same bound and redaction apply.

use std::fmt;
use std::io;

pub use crate::code::{
    BusyCause, ErrorCategory, ErrorCode, LimitUnit, NetworkErrorKind, ResponseInvalidCause,
};
use crate::envelope::{ErrorDetails, ScAgentErrorResponse};
use crate::ids::{BackendKeyId, SessionId};

/// Longest provider-supplied text kept in an error, in bytes.
///
/// One kibibyte. It applies to each provider field (message, type, code,
/// parameter, request id) and to the raw-body fallback. Longer text is cut
/// at a character boundary and ends in `...`.
pub const PROVIDER_TEXT_MAX_BYTES: usize = 1024;

/// Longest caller-supplied path, flag name or label echoed in an error, in
/// bytes. The echo is cut here, at the point it is built, so a cap never has
/// to cut the instruction off a `suggested_action`.
pub const ECHO_MAX_BYTES: usize = 512;

/// Longest caller-supplied detail text echoed in an error, in bytes.
pub const DETAIL_MAX_BYTES: usize = 1024;

fn cut_to(text: &str, max: usize) -> String {
    let cleaned: String = text
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    if cleaned.len() <= max {
        return cleaned;
    }
    let mut end = max - 3;
    while !cleaned.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &cleaned[..end])
}

/// A caller-supplied path, flag or name, cut to [`ECHO_MAX_BYTES`].
fn echo(text: impl Into<String>) -> String {
    cut_to(&text.into(), ECHO_MAX_BYTES)
}

/// A caller-supplied detail, cut to [`DETAIL_MAX_BYTES`].
fn detail_echo(text: impl Into<String>) -> String {
    cut_to(&text.into(), DETAIL_MAX_BYTES)
}

/// Makes provider-supplied `text` safe to carry in an error.
///
/// Control characters (newlines included) become spaces, surrounding space
/// is trimmed, and the result is cut to [`PROVIDER_TEXT_MAX_BYTES`] bytes at
/// a character boundary, ending in `...` when cut. Redaction of the secret
/// is the HTTP layer's job and happens after this.
///
/// # Examples
///
/// ```
/// use sc_agent_types::bound_provider_text;
///
/// assert_eq!(bound_provider_text(" a\nb "), "a b");
/// assert!(bound_provider_text(&"x".repeat(5000)).len() <= 1024);
/// ```
#[must_use]
pub fn bound_provider_text(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.len() <= PROVIDER_TEXT_MAX_BYTES {
        return cleaned.to_owned();
    }
    let mut end = PROVIDER_TEXT_MAX_BYTES - 3;
    while !cleaned.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &cleaned[..end])
}

/// What a provider said when it refused a request with a failing status.
///
/// Built by the HTTP layer from the status line, the headers and the reply
/// body (`sc_agent_http::api_failure`). Every text field is bounded by
/// [`bound_provider_text`]; the HTTP layer redacts the secret afterwards.
/// When the body is not the provider's JSON error shape, `message` holds the
/// bounded raw body text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProviderFailure {
    /// The provider's label in messages, such as `Jev` or `luna`.
    pub provider: String,
    /// The HTTP status.
    pub status: u16,
    /// The provider's error type (`invalid_request_error`), when it sent one.
    pub provider_type: Option<String>,
    /// The provider's error code (`insufficient_quota`), when it sent one.
    pub provider_code: Option<String>,
    /// The provider's message, or the bounded raw body.
    pub message: String,
    /// The request field the provider blamed, when it named one.
    pub param: Option<String>,
    /// The provider's request id, when it sent one.
    pub request_id: Option<String>,
    /// The `Retry-After` value in whole seconds, when the provider sent a
    /// valid one.
    pub retry_after_seconds: Option<u64>,
}

impl ProviderFailure {
    /// A failure of `provider` with `status` and `message` (bounded here).
    #[must_use]
    pub fn new(provider: impl Into<String>, status: u16, message: &str) -> Self {
        Self {
            provider: provider.into(),
            status,
            provider_type: None,
            provider_code: None,
            message: bound_provider_text(message),
            param: None,
            request_id: None,
            retry_after_seconds: None,
        }
    }

    /// Sets the provider's error type (bounded).
    #[must_use]
    pub fn with_type(mut self, text: &str) -> Self {
        self.provider_type = Some(bound_provider_text(text));
        self
    }

    /// Sets the provider's error code (bounded).
    #[must_use]
    pub fn with_code(mut self, text: &str) -> Self {
        self.provider_code = Some(bound_provider_text(text));
        self
    }

    /// Sets the blamed request field (bounded).
    #[must_use]
    pub fn with_param(mut self, text: &str) -> Self {
        self.param = Some(bound_provider_text(text));
        self
    }

    /// Sets the provider's request id (bounded).
    #[must_use]
    pub fn with_request_id(mut self, text: &str) -> Self {
        self.request_id = Some(bound_provider_text(text));
        self
    }

    /// Sets the `Retry-After` seconds.
    #[must_use]
    pub fn with_retry_after(mut self, seconds: Option<u64>) -> Self {
        self.retry_after_seconds = seconds;
        self
    }

    fn map_text(self, map: &impl Fn(&str) -> String) -> Self {
        Self {
            provider: map(&self.provider),
            status: self.status,
            provider_type: self.provider_type.as_deref().map(map),
            provider_code: self.provider_code.as_deref().map(map),
            message: map(&self.message),
            param: self.param.as_deref().map(map),
            request_id: self.request_id.as_deref().map(map),
            retry_after_seconds: self.retry_after_seconds,
        }
    }
}

impl fmt::Display for ProviderFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} HTTP {}", self.provider, self.status)?;
        match (&self.provider_type, &self.provider_code) {
            (Some(kind), Some(code)) => write!(formatter, " ({kind}, {code})")?,
            (Some(text), None) | (None, Some(text)) => write!(formatter, " ({text})")?,
            (None, None) => {}
        }
        if !self.message.is_empty() {
            write!(formatter, ": {}", self.message)?;
        }
        Ok(())
    }
}

/// Text of a `PROVIDER_BUSY` message: the retry outcome, then the provider's
/// own words from its last busy reply when it said any.
fn busy_text(detail: &str, reply: Option<&ProviderFailure>) -> String {
    let Some(reply) = reply else {
        return detail.to_owned();
    };
    let kind = match (&reply.provider_type, &reply.provider_code) {
        (Some(kind), Some(code)) => format!("{kind}, {code}: "),
        (Some(text), None) | (None, Some(text)) => format!("{text}: "),
        (None, None) => String::new(),
    };
    if kind.is_empty() && reply.message.is_empty() {
        detail.to_owned()
    } else {
        format!("{detail} ({kind}{})", reply.message)
    }
}

/// Text of an `INPUT_TOO_LARGE` message.
fn too_large_text(
    limit: Option<u64>,
    actual: Option<u64>,
    unit: LimitUnit,
    provider: Option<&ProviderFailure>,
) -> String {
    let head = match (limit, actual) {
        (Some(limit), Some(actual)) => format!("input is {actual} {unit}; limit is {limit} {unit}"),
        (Some(limit), None) => format!("input exceeds the limit of {limit} {unit}"),
        (None, Some(actual)) => format!("input is {actual} {unit}, over the provider's limit"),
        (None, None) => format!("input exceeds the provider's {unit} limit"),
    };
    match provider {
        Some(failure) => format!("{head} ({failure})"),
        None => head,
    }
}

impl fmt::Display for LimitUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Every value of `details.io_kind`, a closed set. Anything the standard
/// library adds later, or reports as uncategorized, is `other`.
pub const IO_KIND_IDS: &[&str] = &[
    "not_found",
    "permission_denied",
    "connection_refused",
    "connection_reset",
    "host_unreachable",
    "network_unreachable",
    "connection_aborted",
    "not_connected",
    "addr_in_use",
    "addr_not_available",
    "network_down",
    "broken_pipe",
    "already_exists",
    "would_block",
    "not_a_directory",
    "is_a_directory",
    "directory_not_empty",
    "read_only_filesystem",
    "stale_network_file_handle",
    "invalid_input",
    "invalid_data",
    "timed_out",
    "write_zero",
    "storage_full",
    "not_seekable",
    "quota_exceeded",
    "file_too_large",
    "resource_busy",
    "executable_file_busy",
    "deadlock",
    "crosses_devices",
    "too_many_links",
    "invalid_filename",
    "argument_list_too_long",
    "interrupted",
    "unsupported",
    "unexpected_eof",
    "out_of_memory",
    "other",
];

/// `io::ErrorKind` as a fixed id (an explicit table, not the `Debug` text,
/// which is not a stability promise). Kinds not listed, such as
/// `Uncategorized` or a kind added to the standard library later, are
/// `other`.
fn io_kind_id(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::NotFound => "not_found",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::ConnectionRefused => "connection_refused",
        io::ErrorKind::ConnectionReset => "connection_reset",
        io::ErrorKind::HostUnreachable => "host_unreachable",
        io::ErrorKind::NetworkUnreachable => "network_unreachable",
        io::ErrorKind::ConnectionAborted => "connection_aborted",
        io::ErrorKind::NotConnected => "not_connected",
        io::ErrorKind::AddrInUse => "addr_in_use",
        io::ErrorKind::AddrNotAvailable => "addr_not_available",
        io::ErrorKind::NetworkDown => "network_down",
        io::ErrorKind::BrokenPipe => "broken_pipe",
        io::ErrorKind::AlreadyExists => "already_exists",
        io::ErrorKind::WouldBlock => "would_block",
        io::ErrorKind::NotADirectory => "not_a_directory",
        io::ErrorKind::IsADirectory => "is_a_directory",
        io::ErrorKind::DirectoryNotEmpty => "directory_not_empty",
        io::ErrorKind::ReadOnlyFilesystem => "read_only_filesystem",
        io::ErrorKind::StaleNetworkFileHandle => "stale_network_file_handle",
        io::ErrorKind::InvalidInput => "invalid_input",
        io::ErrorKind::InvalidData => "invalid_data",
        io::ErrorKind::TimedOut => "timed_out",
        io::ErrorKind::WriteZero => "write_zero",
        io::ErrorKind::StorageFull => "storage_full",
        io::ErrorKind::NotSeekable => "not_seekable",
        io::ErrorKind::QuotaExceeded => "quota_exceeded",
        io::ErrorKind::FileTooLarge => "file_too_large",
        io::ErrorKind::ResourceBusy => "resource_busy",
        io::ErrorKind::ExecutableFileBusy => "executable_file_busy",
        io::ErrorKind::Deadlock => "deadlock",
        io::ErrorKind::CrossesDevices => "crosses_devices",
        io::ErrorKind::TooManyLinks => "too_many_links",
        io::ErrorKind::InvalidFilename => "invalid_filename",
        io::ErrorKind::ArgumentListTooLong => "argument_list_too_long",
        io::ErrorKind::Interrupted => "interrupted",
        io::ErrorKind::Unsupported => "unsupported",
        io::ErrorKind::UnexpectedEof => "unexpected_eof",
        io::ErrorKind::OutOfMemory => "out_of_memory",
        io::ErrorKind::Other => "other",
        _ => "other",
    }
}

/// Stable error ids, messages, fix text and machine fields for Jev, check,
/// extraction, and the CLI.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum JevError {
    /// `check.toml`, a packaged asset, or the run setup failed validation.
    #[error("config is invalid: {detail}")]
    #[non_exhaustive]
    ConfigInvalid {
        /// What failed validation.
        detail: String,
        /// The caller-facing fix; `None` means the `check.toml` hint.
        recovery: Option<String>,
    },
    /// A credential whose content is unusable (`CONFIG_INVALID`).
    ///
    /// A store that cannot be opened is [`Self::IoFailed`], not this.
    #[error("credential store is invalid: {what} (cause: {cause})")]
    #[non_exhaustive]
    CredentialStoreInvalid {
        /// The credential, such as the Jev API key.
        what: String,
        /// A sanitized cause label.
        cause: String,
        /// The caller-facing fix.
        recovery: String,
    },
    /// A backend-registry failure (`CONFIG_INVALID`).
    #[error("backend registry is invalid: {detail}")]
    #[non_exhaustive]
    RegistryInvalid {
        /// What is wrong, naming the backend id.
        detail: String,
        /// The caller-facing fix.
        recovery: String,
    },
    /// A session file that is not a readable session document (`INPUT_INVALID`).
    #[error("session document is invalid: {detail}")]
    #[non_exhaustive]
    SessionDocumentInvalid {
        /// Which file and what is wrong.
        detail: String,
        /// The caller-facing fix.
        recovery: String,
    },
    /// A value `serde_json` could not encode or decode (`INPUT_INVALID`).
    #[error("{what} could not be encoded as JSON ({cause})")]
    #[non_exhaustive]
    EncodeFailed {
        /// What was being encoded, such as `response JSON`.
        what: String,
        /// The `serde_json` category and position only.
        cause: String,
    },
    /// A file, process, stream or platform dependency failed (`IO_FAILED`).
    #[error("{what}: I/O failed ({kind})")]
    #[non_exhaustive]
    IoFailed {
        /// The stream, file or dependency, such as `stdout` or `git`.
        what: String,
        /// The I/O error class; never the OS message.
        kind: io::ErrorKind,
        /// The caller-facing fix; `None` means a generic access check.
        recovery: Option<String>,
    },
    /// No credential was found for a backend.
    #[error("API key for {backend} is missing")]
    #[non_exhaustive]
    KeyMissing {
        /// Backend whose credential is missing.
        backend: BackendKeyId,
    },
    /// The input exceeds a size cap, ours (bytes, tokens) or the provider's.
    #[error("{}", too_large_text(*.limit, *.actual, *.unit, .provider.as_deref()))]
    #[non_exhaustive]
    InputTooLarge {
        /// The cap; `None` when the provider did not state it.
        limit: Option<u64>,
        /// The measured size; `None` when the provider did not state it.
        actual: Option<u64>,
        /// What `limit` and `actual` count.
        unit: LimitUnit,
        /// What the provider said, when the provider refused the input.
        provider: Option<Box<ProviderFailure>>,
    },
    /// The check input contained no usable content.
    #[error("input is empty")]
    InputEmpty,
    /// The input, flag or argument did not match the expected shape.
    #[error("input is invalid: {detail}")]
    #[non_exhaustive]
    InputInvalid {
        /// What about the input is wrong.
        detail: String,
        /// The failing field, flag or argument, when known.
        field: Option<String>,
        /// The caller-facing fix; `None` means the generic one.
        recovery: Option<String>,
    },
    /// Extraction succeeded and produced zero subjects.
    #[error("extraction produced no subjects")]
    ExtractionEmpty,
    /// A subject span contains a parse error.
    #[error("parse error in {path}:{line}")]
    #[non_exhaustive]
    ExtractionParseError {
        /// Source path that failed to parse.
        path: String,
        /// 1-based line of the parse error.
        line: u32,
    },
    /// Extractor stdout was not a valid subjects document.
    #[error("extraction output is invalid: {detail}")]
    #[non_exhaustive]
    ExtractionInvalid {
        /// What about the artifact is wrong.
        detail: String,
    },
    /// The extractor process failed.
    #[error("extraction failed: {detail}")]
    #[non_exhaustive]
    ExtractionFailed {
        /// Producer failure detail.
        detail: String,
    },
    /// A `command` extractor input exceeded its `timeout_seconds` limit.
    ///
    /// The id `EXTRACTION_TIMEOUT` also covers a git subprocess that did not
    /// finish ([`Self::GitTimeout`]).
    #[error("extraction timed out after {timeout_seconds}s")]
    #[non_exhaustive]
    ExtractionTimeout {
        /// Limit that was exceeded.
        timeout_seconds: u64,
    },
    /// A git subprocess did not finish within its fixed wall-clock deadline.
    ///
    /// Maps to the existing `EXTRACTION_TIMEOUT` id (ADR-SAG-0027: an id may
    /// have several variants). The limit is not configurable, so the hint
    /// does not point at `timeout_seconds`.
    #[error("git did not finish within {seconds}s")]
    #[non_exhaustive]
    GitTimeout {
        /// The deadline that expired, in seconds.
        seconds: u64,
    },
    /// The requested grammar is not built into the producer.
    #[error("language {language} is unavailable in {producer}")]
    #[non_exhaustive]
    LanguageUnavailable {
        /// Producer command name, such as `sc-treesitter`.
        producer: String,
        /// Requested language id.
        language: String,
    },
    /// A producer query failed to compile.
    #[error("query is invalid: {detail}")]
    #[non_exhaustive]
    QueryInvalid {
        /// Producer command name.
        producer: String,
        /// Query compiler detail.
        detail: String,
    },
    /// The named query asset does not exist.
    #[error("query {query} was not found")]
    #[non_exhaustive]
    QueryNotFound {
        /// Producer command name.
        producer: String,
        /// Query name that was requested.
        query: String,
    },
    /// A producer reported a stable error on stderr; `code()` is the carried id.
    ///
    /// Every field this build knows of the producer's response is carried
    /// through; a key it does not know is ignored on read (so it is not
    /// re-emitted).
    #[error("{producer} failed with {code}: {message}")]
    #[non_exhaustive]
    ProducerError {
        /// Producer command name from `check.toml`.
        producer: String,
        /// The producer's stable error id.
        code: ErrorCode,
        /// The producer's message.
        message: String,
        /// The producer's suggested action; empty when it sent none.
        suggested_action: Box<str>,
        /// The producer's `retryable` flag.
        retryable: bool,
        /// The producer's `retry_after_seconds`.
        retry_after_seconds: Option<u64>,
        /// The producer's failing field.
        field: Option<Box<str>>,
        /// The producer's `details` object.
        details: Option<Box<ErrorDetails>>,
    },
    /// An input path does not exist.
    #[error("input path {path} is missing")]
    #[non_exhaustive]
    InputMissing {
        /// Path that could not be read.
        path: String,
        /// The caller-facing fix; `None` means the generic one.
        recovery: Option<String>,
    },
    /// A judge response failed validation.
    #[error("response is invalid ({cause}): {detail}", cause = .cause.as_str())]
    #[non_exhaustive]
    ResponseInvalid {
        /// Which kind of invalid reply; decides the suggested action.
        cause: ResponseInvalidCause,
        /// What about the response is wrong.
        detail: String,
    },
    /// A backend transport failed before any HTTP reply arrived.
    #[error("network error ({kind}): {detail}", kind = .kind.as_str())]
    #[non_exhaustive]
    NetworkError {
        /// Failure class; decides [`JevError::retryable`].
        kind: NetworkErrorKind,
        /// Transport failure detail, with secrets removed.
        detail: String,
    },
    /// The provider rejected the credential (HTTP 401, 403).
    #[error("provider rejected the credential: {failure}")]
    #[non_exhaustive]
    AuthFailed {
        /// What the provider said.
        failure: Box<ProviderFailure>,
    },
    /// The provider rejected the request (HTTP 400, 422).
    #[error("provider rejected the request: {failure}")]
    #[non_exhaustive]
    ApiRequestInvalid {
        /// What the provider said.
        failure: Box<ProviderFailure>,
    },
    /// The account is out of tokens, credit or spend allowance.
    #[error("provider quota is exhausted: {failure}")]
    #[non_exhaustive]
    QuotaExhausted {
        /// What the provider said.
        failure: Box<ProviderFailure>,
    },
    /// Any other failing provider status, 5xx included.
    #[error("provider API error: {failure}")]
    #[non_exhaustive]
    ApiError {
        /// What the provider said.
        failure: Box<ProviderFailure>,
    },
    /// The provider stayed busy (rate limit or overload) through the busy
    /// retry.
    #[error("provider is busy: {}", busy_text(.detail, .reply.as_deref()))]
    #[non_exhaustive]
    ProviderBusy {
        /// The last busy status and how the retry ended.
        detail: String,
        /// The last busy HTTP status.
        status: u16,
        /// How many sends were made.
        sends: u32,
        /// Why the retry stopped.
        cause: BusyCause,
        /// The provider's `Retry-After` in whole seconds, when it sent a
        /// valid one.
        retry_after_seconds: Option<u64>,
        /// What the provider said in its last busy reply (type, code,
        /// message, request id), bounded; `None` when not recorded.
        reply: Option<Box<ProviderFailure>>,
    },
    /// An LLM system-prompt file could not be read.
    #[error("prompt file {path} is missing")]
    #[non_exhaustive]
    PromptFileMissing {
        /// Path that could not be read.
        path: String,
    },
    /// The session id does not exist in the store.
    #[error("session {session_id} was not found")]
    #[non_exhaustive]
    SessionNotFound {
        /// Id that was requested.
        session_id: SessionId,
    },
    /// The session is past its TTL.
    #[error("session {session_id} has expired")]
    #[non_exhaustive]
    SessionExpired {
        /// Id that expired.
        session_id: SessionId,
    },
    /// `error` with machine annotations: which backend failed
    /// ([`Self::with_backend`]) and/or that the session log could not be
    /// flushed while `error` was leaving ([`Self::with_log_flush_failure`]).
    /// The id, message, action and retryability are `error`'s.
    #[error("{error}")]
    #[non_exhaustive]
    Annotated {
        /// The failure the caller is told about.
        error: Box<JevError>,
        /// Why the log flush failed, redacted.
        flush_failure: Option<String>,
        /// The backend that failed (its descriptor name), when known.
        backend: Option<String>,
    },
}

impl JevError {
    /// `CONFIG_INVALID` for a `check.toml` or packaged-asset problem.
    ///
    /// [`Self::suggested_action`] points at `check.toml`. For a setup
    /// problem that has nothing to do with it, use [`Self::config_invalid_with`].
    #[must_use]
    pub fn config_invalid(detail: impl Into<String>) -> Self {
        Self::ConfigInvalid {
            detail: detail_echo(detail),
            recovery: None,
        }
    }

    /// `CONFIG_INVALID` for a setup problem with its own fix.
    ///
    /// `detail` says what is wrong; `recovery` is the next step.
    /// [`Self::suggested_action`] returns `recovery` and does not mention
    /// `check.toml`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let error = JevError::config_invalid_with(
    ///     "no git toplevel was found",
    ///     "Pass --repo-root with the repository root.",
    /// );
    /// assert_eq!(error.code(), "CONFIG_INVALID");
    /// assert!(error.suggested_action().contains("--repo-root"));
    /// assert!(!error.suggested_action().contains("check.toml"));
    /// ```
    #[must_use]
    pub fn config_invalid_with(detail: impl Into<String>, recovery: impl Into<String>) -> Self {
        Self::ConfigInvalid {
            detail: detail_echo(detail),
            recovery: Some(recovery.into()),
        }
    }

    /// Builds `CONFIG_INVALID` for a credential whose content is unusable.
    ///
    /// `what` names the credential, `cause` is a sanitized label. It must
    /// not contain secret bytes, passwords, or the raw platform error text.
    /// `recovery` is the caller-facing fix. A store that cannot be opened is
    /// [`Self::io_failed_with`] instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let error = JevError::credential_store_invalid(
    ///     "the Jev API key",
    ///     "key contains whitespace",
    ///     "Store the key again without spaces.",
    /// );
    /// assert_eq!(error.code(), "CONFIG_INVALID");
    /// assert!(error.message().contains("key contains whitespace"));
    /// assert!(!error.suggested_action().contains("check.toml"));
    /// ```
    #[must_use]
    pub fn credential_store_invalid(
        what: impl Into<String>,
        cause: impl Into<String>,
        recovery: impl Into<String>,
    ) -> Self {
        Self::CredentialStoreInvalid {
            what: what.into(),
            cause: cause.into(),
            recovery: recovery.into(),
        }
    }

    /// Builds `CONFIG_INVALID` for a backend-registry failure.
    ///
    /// `detail` says what is wrong and names the backend id; `recovery` is
    /// the next step.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let error = JevError::registry_invalid(
    ///     "Backend jev is not registered",
    ///     "Register it before the run.",
    /// );
    /// assert_eq!(error.code(), "CONFIG_INVALID");
    /// assert_eq!(error.suggested_action(), "Register it before the run.");
    /// ```
    #[must_use]
    pub fn registry_invalid(detail: impl Into<String>, recovery: impl Into<String>) -> Self {
        Self::RegistryInvalid {
            detail: detail.into(),
            recovery: recovery.into(),
        }
    }

    /// Builds `INPUT_INVALID` for a session file that cannot be parsed.
    ///
    /// `detail` names the file and the problem; `recovery` is the fix.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let error = JevError::session_document_invalid(
    ///     "Session file sessions/abc is not a session document",
    ///     "Delete it and start a new session.",
    /// );
    /// assert_eq!(error.code(), "INPUT_INVALID");
    /// assert!(error.message().contains("sessions/abc"));
    /// ```
    #[must_use]
    pub fn session_document_invalid(
        detail: impl Into<String>,
        recovery: impl Into<String>,
    ) -> Self {
        Self::SessionDocumentInvalid {
            detail: detail.into(),
            recovery: recovery.into(),
        }
    }

    /// Builds `INPUT_INVALID` for a value that `serde_json` could not encode
    /// or decode.
    ///
    /// `what` names what was being encoded, such as `"response JSON"`. Only
    /// the `serde_json` error category and the line and column are kept,
    /// never the document or the `serde_json` message, so no payload or
    /// secret text can leak through it.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let cause = serde_json::from_str::<serde_json::Value>("{\"secret\": ").unwrap_err();
    /// let error = JevError::encode_failed("response JSON", &cause);
    /// assert_eq!(error.code(), "INPUT_INVALID");
    /// assert!(error.message().contains("response JSON"));
    /// assert!(!error.message().contains("secret"));
    /// ```
    #[must_use]
    pub fn encode_failed(what: impl Into<String>, error: &serde_json::Error) -> Self {
        let category = match error.classify() {
            serde_json::error::Category::Io => "I/O",
            serde_json::error::Category::Syntax => "syntax",
            serde_json::error::Category::Data => "data",
            serde_json::error::Category::Eof => "unexpected end of input",
        };
        let location = if error.line() == 0 {
            String::new()
        } else {
            format!(" at line {} column {}", error.line(), error.column())
        };
        Self::EncodeFailed {
            what: what.into(),
            cause: format!("{category} error{location}"),
        }
    }

    /// Builds `IO_FAILED` for a failed read, write or process.
    ///
    /// Only the [`io::ErrorKind`] is kept, never the OS message, which can
    /// echo paths or payload bytes. A read of a named path that fails with
    /// "not found" is [`Self::input_missing`], not this: use
    /// [`Self::read_failed`] for such reads.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let cause = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "secret");
    /// let error = JevError::io_failed("stdout", &cause);
    /// assert_eq!(error.code(), "IO_FAILED");
    /// assert!(error.message().contains("broken pipe"));
    /// assert!(!error.message().contains("secret"));
    /// ```
    #[must_use]
    pub fn io_failed(what: impl Into<String>, error: &io::Error) -> Self {
        Self::io_failed_with(what, error.kind(), None)
    }

    /// `IO_FAILED` with an explicit [`io::ErrorKind`] and optional fix.
    ///
    /// `recovery` of `None` gives the generic access check.
    #[must_use]
    pub fn io_failed_with(
        what: impl Into<String>,
        kind: io::ErrorKind,
        recovery: Option<String>,
    ) -> Self {
        Self::IoFailed {
            what: echo(what),
            kind,
            recovery,
        }
    }

    /// The error of a failed read of the path the caller named.
    ///
    /// "Not found" is `INPUT_MISSING` naming `path`; every other failure is
    /// `IO_FAILED` of `what`. One rule, so a missing request file and a
    /// missing worktree file give the same id.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let gone = std::io::Error::from(std::io::ErrorKind::NotFound);
    /// assert_eq!(JevError::read_failed("a.json", "request file", &gone).code(), "INPUT_MISSING");
    /// let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    /// assert_eq!(JevError::read_failed("a.json", "request file", &denied).code(), "IO_FAILED");
    /// ```
    #[must_use]
    pub fn read_failed(
        path: impl Into<String>,
        what: impl Into<String>,
        error: &io::Error,
    ) -> Self {
        if error.kind() == io::ErrorKind::NotFound {
            Self::input_missing(path)
        } else {
            Self::io_failed(what, error)
        }
    }

    /// Like [`Self::read_failed`], with its own fix for the "not found" case
    /// (for a path that is a flag value, not a `check.toml` entry).
    #[must_use]
    pub fn read_failed_with(
        path: impl Into<String>,
        what: impl Into<String>,
        missing_recovery: impl Into<String>,
        error: &io::Error,
    ) -> Self {
        if error.kind() == io::ErrorKind::NotFound {
            Self::input_missing_with(path, missing_recovery)
        } else {
            Self::io_failed(what, error)
        }
    }

    /// `KEY_MISSING` for `backend`.
    #[must_use]
    pub fn key_missing(backend: BackendKeyId) -> Self {
        Self::KeyMissing { backend }
    }

    /// `INPUT_TOO_LARGE` for a request body over `limit_bytes`.
    #[must_use]
    pub fn input_too_large(limit_bytes: u64, actual_bytes: u64) -> Self {
        Self::InputTooLarge {
            limit: Some(limit_bytes),
            actual: Some(actual_bytes),
            unit: LimitUnit::Bytes,
            provider: None,
        }
    }

    /// `INPUT_TOO_LARGE` for an input over `limit` model tokens.
    #[must_use]
    pub fn input_too_large_tokens(limit: u64, actual: u64) -> Self {
        Self::InputTooLarge {
            limit: Some(limit),
            actual: Some(actual),
            unit: LimitUnit::Tokens,
            provider: None,
        }
    }

    /// `INPUT_TOO_LARGE` for a provider that refused the input as too large
    /// without stating the numbers; `failure` is what it said.
    #[must_use]
    pub fn input_too_large_by_provider(unit: LimitUnit, failure: ProviderFailure) -> Self {
        Self::InputTooLarge {
            limit: None,
            actual: None,
            unit,
            provider: Some(Box::new(failure)),
        }
    }

    /// `INPUT_INVALID` with the check-schema hint.
    #[must_use]
    pub fn input_invalid(detail: impl Into<String>) -> Self {
        Self::InputInvalid {
            detail: detail_echo(detail),
            field: None,
            recovery: None,
        }
    }

    /// `INPUT_INVALID` that names the failing `field`, flag or argument.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let error = JevError::input_invalid_field("--backend", "unknown backend nope");
    /// assert_eq!(error.field(), Some("--backend"));
    /// ```
    #[must_use]
    pub fn input_invalid_field(field: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::InputInvalid {
            detail: detail_echo(detail),
            field: Some(echo(field)),
            recovery: None,
        }
    }

    /// `INPUT_INVALID` naming `field` with its own fix (not the generic
    /// "correct the argument" action).
    #[must_use]
    pub fn input_invalid_field_with(
        field: impl Into<String>,
        detail: impl Into<String>,
        recovery: impl Into<String>,
    ) -> Self {
        Self::InputInvalid {
            detail: detail_echo(detail),
            field: Some(echo(field)),
            recovery: Some(recovery.into()),
        }
    }

    /// `EXTRACTION_PARSE_ERROR` at `path:line`.
    #[must_use]
    pub fn extraction_parse_error(path: impl Into<String>, line: u32) -> Self {
        Self::ExtractionParseError {
            path: path.into(),
            line,
        }
    }

    /// `EXTRACTION_INVALID`.
    #[must_use]
    pub fn extraction_invalid(detail: impl Into<String>) -> Self {
        Self::ExtractionInvalid {
            detail: detail.into(),
        }
    }

    /// `EXTRACTION_FAILED`.
    #[must_use]
    pub fn extraction_failed(detail: impl Into<String>) -> Self {
        Self::ExtractionFailed {
            detail: detail.into(),
        }
    }

    /// `EXTRACTION_TIMEOUT`.
    #[must_use]
    pub fn extraction_timeout(timeout_seconds: u64) -> Self {
        Self::ExtractionTimeout { timeout_seconds }
    }

    /// `EXTRACTION_TIMEOUT` for a git subprocess that exceeded its deadline.
    #[must_use]
    pub fn git_timeout(seconds: u64) -> Self {
        Self::GitTimeout { seconds }
    }

    /// `LANGUAGE_UNAVAILABLE` from `producer`.
    #[must_use]
    pub fn language_unavailable(producer: impl Into<String>, language: impl Into<String>) -> Self {
        Self::LanguageUnavailable {
            producer: producer.into(),
            language: language.into(),
        }
    }

    /// `QUERY_INVALID` from `producer`.
    #[must_use]
    pub fn query_invalid(producer: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::QueryInvalid {
            producer: producer.into(),
            detail: detail.into(),
        }
    }

    /// `QUERY_NOT_FOUND` from `producer`.
    #[must_use]
    pub fn query_not_found(producer: impl Into<String>, query: impl Into<String>) -> Self {
        Self::QueryNotFound {
            producer: producer.into(),
            query: query.into(),
        }
    }

    /// A producer's stable error, re-raised; `code()` returns `code`.
    ///
    /// The result is not retryable and has no suggested action beyond the
    /// producer's name; [`Self::from_error_response`] carries the producer's
    /// own fields through.
    #[must_use]
    pub fn producer_error(
        producer: impl Into<String>,
        code: ErrorCode,
        message: impl Into<String>,
    ) -> Self {
        Self::ProducerError {
            producer: producer.into(),
            code,
            message: message.into(),
            suggested_action: Box::from(""),
            retryable: false,
            retry_after_seconds: None,
            field: None,
            details: None,
        }
    }

    /// `INPUT_MISSING` for `path`.
    #[must_use]
    pub fn input_missing(path: impl Into<String>) -> Self {
        Self::InputMissing {
            path: echo(path),
            recovery: None,
        }
    }

    /// `INPUT_MISSING` for a path that came from `check.toml`; the action
    /// points back at it.
    #[must_use]
    pub fn input_missing_from_check(path: impl Into<String>) -> Self {
        let path = echo(path);
        let recovery = format!("Create {path} or correct the path in check.toml, then retry.");
        Self::InputMissing {
            path,
            recovery: Some(recovery),
        }
    }

    /// `INPUT_MISSING` for `path` with its own fix, for a path that is not a
    /// `check.toml` entry (a flag value, for instance).
    #[must_use]
    pub fn input_missing_with(path: impl Into<String>, recovery: impl Into<String>) -> Self {
        Self::InputMissing {
            path: echo(path),
            recovery: Some(recovery.into()),
        }
    }

    /// The flag a command-line parser blamed, as a `field` value: the first
    /// word of `argument` when it starts with `-` (`--backend`), else `None`.
    ///
    /// A stray positional value, such as the `hello` in `unexpected argument
    /// 'hello'`, is the caller's text, not an argument name, so it is never
    /// a `field`.
    #[must_use]
    pub fn usage_field(argument: &str) -> Option<String> {
        let first = argument.split_whitespace().next()?;
        let first = first.trim_end_matches(',');
        first.starts_with('-').then(|| echo(first))
    }

    /// Names `field` on an `INPUT_INVALID` that has none; any other error is
    /// returned unchanged.
    ///
    /// Field names are the argument as the caller wrote it: the `--flag`
    /// spelling for a command-line argument, the parameter name for a
    /// library or Python argument.
    #[must_use]
    pub fn with_field(self, field: impl Into<String>) -> Self {
        match self {
            Self::InputInvalid {
                detail,
                field: None,
                recovery,
            } => Self::InputInvalid {
                detail,
                field: Some(echo(field)),
                recovery,
            },
            Self::Annotated {
                error,
                flush_failure,
                backend,
            } => Self::Annotated {
                error: Box::new(error.with_field(field)),
                flush_failure,
                backend,
            },
            other => other,
        }
    }

    /// `RESPONSE_INVALID` of cause `Schema`: the reply parsed but breaks the
    /// answer schema.
    #[must_use]
    pub fn response_invalid(detail: impl Into<String>) -> Self {
        Self::response_invalid_because(ResponseInvalidCause::Schema, detail)
    }

    /// `RESPONSE_INVALID` of the given `cause`.
    #[must_use]
    pub fn response_invalid_because(
        cause: ResponseInvalidCause,
        detail: impl Into<String>,
    ) -> Self {
        Self::ResponseInvalid {
            cause,
            detail: detail.into(),
        }
    }

    /// `NETWORK_ERROR` of class `kind`. `detail` must already be redacted.
    #[must_use]
    pub fn network(kind: NetworkErrorKind, detail: impl Into<String>) -> Self {
        Self::NetworkError {
            kind,
            detail: detail.into(),
        }
    }

    /// `AUTH_FAILED` from the provider's reply.
    #[must_use]
    pub fn auth_failed(failure: ProviderFailure) -> Self {
        Self::AuthFailed {
            failure: Box::new(failure),
        }
    }

    /// `API_REQUEST_INVALID` from the provider's reply.
    #[must_use]
    pub fn api_request_invalid(failure: ProviderFailure) -> Self {
        Self::ApiRequestInvalid {
            failure: Box::new(failure),
        }
    }

    /// `QUOTA_EXHAUSTED` from the provider's reply.
    #[must_use]
    pub fn quota_exhausted(failure: ProviderFailure) -> Self {
        Self::QuotaExhausted {
            failure: Box::new(failure),
        }
    }

    /// `API_ERROR` from the provider's reply.
    #[must_use]
    pub fn api_error(failure: ProviderFailure) -> Self {
        Self::ApiError {
            failure: Box::new(failure),
        }
    }

    /// `PROVIDER_BUSY` after `sends` busy replies.
    ///
    /// `detail` must already be redacted. `status` is the last busy status;
    /// `cause` says why the retry stopped; `retry_after_seconds` is the last
    /// valid `Retry-After`.
    #[must_use]
    pub fn provider_busy(
        detail: impl Into<String>,
        status: u16,
        sends: u32,
        cause: BusyCause,
        retry_after_seconds: Option<u64>,
    ) -> Self {
        Self::ProviderBusy {
            detail: detail.into(),
            status,
            sends,
            cause,
            retry_after_seconds,
            reply: None,
        }
    }

    /// Records the provider's last busy reply on a `PROVIDER_BUSY` (message,
    /// type, code, request id; bounded by [`ProviderFailure`]). Any other
    /// error is returned unchanged.
    #[must_use]
    pub fn with_busy_reply(mut self, failure: ProviderFailure) -> Self {
        if let Self::ProviderBusy { reply, .. } = &mut self {
            *reply = Some(Box::new(failure));
        }
        self
    }

    /// `PROMPT_FILE_MISSING` for `path`.
    #[must_use]
    pub fn prompt_file_missing(path: impl Into<String>) -> Self {
        Self::PromptFileMissing { path: echo(path) }
    }

    /// `SESSION_NOT_FOUND` for `session_id`.
    #[must_use]
    pub fn session_not_found(session_id: SessionId) -> Self {
        Self::SessionNotFound { session_id }
    }

    /// `SESSION_EXPIRED` for `session_id`.
    #[must_use]
    pub fn session_expired(session_id: SessionId) -> Self {
        Self::SessionExpired { session_id }
    }

    /// Records that the session log could not be flushed while `self` was
    /// leaving.
    ///
    /// The id, message, action, retryability and every other field stay
    /// those of `self`; the failure is its own field (`log_flush_failure` in
    /// `details`). Only the first call counts; a later one is ignored.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let error = JevError::InputEmpty.with_log_flush_failure("disk full");
    /// assert_eq!(error.code(), "INPUT_EMPTY");
    /// assert_eq!(error.log_flush_failure(), Some("disk full"));
    /// assert_eq!(error.message(), JevError::InputEmpty.message());
    /// ```
    #[must_use]
    pub fn with_log_flush_failure(self, flush_failure: impl Into<String>) -> Self {
        match self {
            Self::Annotated {
                error,
                flush_failure: None,
                backend,
            } => Self::Annotated {
                error,
                flush_failure: Some(flush_failure.into()),
                backend,
            },
            already @ Self::Annotated { .. } => already,
            other => Self::Annotated {
                error: Box::new(other),
                flush_failure: Some(flush_failure.into()),
                backend: None,
            },
        }
    }

    /// Records which backend this error belongs to (`details.backend`).
    ///
    /// `backend` is the backend's descriptor name, such as `haiku`.
    /// Only the first call counts. Id, message, action and retryability stay
    /// those of `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let error = JevError::InputEmpty.with_backend("jev");
    /// assert_eq!(error.code(), "INPUT_EMPTY");
    /// assert_eq!(error.backend(), Some("jev"));
    /// ```
    #[must_use]
    pub fn with_backend(self, backend: impl Into<String>) -> Self {
        match self {
            Self::Annotated {
                error,
                flush_failure,
                backend: None,
            } => Self::Annotated {
                error,
                flush_failure,
                backend: Some(backend.into()),
            },
            already @ Self::Annotated { .. } => already,
            other => Self::Annotated {
                error: Box::new(other),
                flush_failure: None,
                backend: Some(backend.into()),
            },
        }
    }

    /// Re-raises a producer's stderr error response.
    ///
    /// A known id becomes [`JevError::ProducerError`]: its `code()` is that
    /// id, and its message, suggested action, `retryable`,
    /// `retry_after_seconds`, `field` and `details` are the producer's. An
    /// unknown id becomes `EXTRACTION_FAILED` naming the id.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::{JevError, ScAgentErrorResponse};
    ///
    /// let line = r#"{"error":"LANGUAGE_UNAVAILABLE","message":"No go.","suggested_action":"Use python.","retryable":false,"schema_version":1}"#;
    /// let response: ScAgentErrorResponse = serde_json::from_str(line).unwrap();
    /// let error = JevError::from_error_response("sc-treesitter", &response);
    /// assert_eq!(error.code(), "LANGUAGE_UNAVAILABLE");
    /// assert_eq!(error.suggested_action(), "sc-treesitter: Use python.");
    /// ```
    #[must_use]
    pub fn from_error_response(producer: &str, response: &ScAgentErrorResponse) -> Self {
        match response.error().parse::<ErrorCode>() {
            Ok(code) => Self::ProducerError {
                producer: producer.to_owned(),
                code,
                message: response.message().to_owned(),
                suggested_action: Box::from(response.suggested_action()),
                retryable: response.retryable(),
                retry_after_seconds: response.retry_after_seconds(),
                field: response.field().map(Box::from),
                details: response.details().cloned().map(Box::new),
            },
            Err(_) => {
                // Keep what the producer said (bounded), so an id this build
                // does not know loses neither its message nor its action.
                let said = bound_provider_text(response.message());
                let advice = bound_provider_text(response.suggested_action());
                let advice = if advice.is_empty() {
                    String::new()
                } else {
                    format!("; it suggested: {advice}")
                };
                Self::extraction_failed(format!(
                    "{producer} reported unknown error id {}: {said}{advice}",
                    bound_provider_text(response.error())
                ))
            }
        }
    }

    /// Rewrites every text field of this error with `map`.
    ///
    /// Covers every `String` field of every variant, the provider's text
    /// and the machine `details` strings included. Variants without text
    /// are returned unchanged. The match is exhaustive, so adding a variant
    /// with a text field fails to compile until it is handled here.
    /// `ResolvedApiKey::redact_error` is built on this method.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let error = JevError::input_missing("a/b");
    /// let mapped = error.map_detail(|text| text.to_uppercase());
    /// assert_eq!(mapped, JevError::input_missing("A/B"));
    /// ```
    #[must_use]
    pub fn map_detail(self, map: impl Fn(&str) -> String) -> Self {
        self.map_text(&map)
    }

    fn map_text(self, map: &impl Fn(&str) -> String) -> Self {
        match self {
            Self::ConfigInvalid { detail, recovery } => Self::ConfigInvalid {
                detail: map(&detail),
                recovery: recovery.as_deref().map(map),
            },
            Self::CredentialStoreInvalid {
                what,
                cause,
                recovery,
            } => Self::credential_store_invalid(map(&what), map(&cause), map(&recovery)),
            Self::RegistryInvalid { detail, recovery } => {
                Self::registry_invalid(map(&detail), map(&recovery))
            }
            Self::SessionDocumentInvalid { detail, recovery } => {
                Self::session_document_invalid(map(&detail), map(&recovery))
            }
            Self::EncodeFailed { what, cause } => Self::EncodeFailed {
                what: map(&what),
                cause: map(&cause),
            },
            Self::IoFailed {
                what,
                kind,
                recovery,
            } => Self::IoFailed {
                what: map(&what),
                kind,
                recovery: recovery.as_deref().map(map),
            },
            Self::InputInvalid {
                detail,
                field,
                recovery,
            } => Self::InputInvalid {
                detail: map(&detail),
                field: field.as_deref().map(map),
                recovery: recovery.as_deref().map(map),
            },
            Self::ExtractionParseError { path, line } => {
                Self::extraction_parse_error(map(&path), line)
            }
            Self::ExtractionInvalid { detail } => Self::extraction_invalid(map(&detail)),
            Self::ExtractionFailed { detail } => Self::extraction_failed(map(&detail)),
            Self::LanguageUnavailable { producer, language } => {
                Self::language_unavailable(map(&producer), map(&language))
            }
            Self::QueryInvalid { producer, detail } => {
                Self::query_invalid(map(&producer), map(&detail))
            }
            Self::QueryNotFound { producer, query } => {
                Self::query_not_found(map(&producer), map(&query))
            }
            Self::ProducerError {
                producer,
                code,
                message,
                suggested_action,
                retryable,
                retry_after_seconds,
                field,
                details,
            } => Self::ProducerError {
                producer: map(&producer),
                code,
                message: map(&message),
                suggested_action: map(&suggested_action).into_boxed_str(),
                retryable,
                retry_after_seconds,
                field: field.as_deref().map(map).map(String::into_boxed_str),
                details: details.map(|details| Box::new((*details).map_text(map))),
            },
            Self::InputMissing { path, recovery } => Self::InputMissing {
                path: map(&path),
                recovery: recovery.as_deref().map(map),
            },
            Self::ResponseInvalid { cause, detail } => {
                Self::response_invalid_because(cause, map(&detail))
            }
            Self::NetworkError { kind, detail } => Self::network(kind, map(&detail)),
            Self::AuthFailed { failure } => Self::AuthFailed {
                failure: Box::new((*failure).map_text(map)),
            },
            Self::ApiRequestInvalid { failure } => Self::ApiRequestInvalid {
                failure: Box::new((*failure).map_text(map)),
            },
            Self::QuotaExhausted { failure } => Self::QuotaExhausted {
                failure: Box::new((*failure).map_text(map)),
            },
            Self::ApiError { failure } => Self::ApiError {
                failure: Box::new((*failure).map_text(map)),
            },
            Self::ProviderBusy {
                detail,
                status,
                sends,
                cause,
                retry_after_seconds,
                reply,
            } => Self::ProviderBusy {
                detail: map(&detail),
                status,
                sends,
                cause,
                retry_after_seconds,
                reply: reply.map(|failure| Box::new((*failure).map_text(map))),
            },
            Self::PromptFileMissing { path } => Self::prompt_file_missing(map(&path)),
            Self::Annotated {
                error,
                flush_failure,
                backend,
            } => Self::Annotated {
                error: Box::new(error.map_text(map)),
                flush_failure: flush_failure.as_deref().map(map),
                backend: backend.as_deref().map(map),
            },
            Self::InputTooLarge {
                limit,
                actual,
                unit,
                provider,
            } => Self::InputTooLarge {
                limit,
                actual,
                unit,
                provider: provider.map(|failure| Box::new((*failure).map_text(map))),
            },
            unchanged @ (Self::KeyMissing { .. }
            | Self::InputEmpty
            | Self::ExtractionEmpty
            | Self::ExtractionTimeout { .. }
            | Self::GitTimeout { .. }
            | Self::SessionNotFound { .. }
            | Self::SessionExpired { .. }) => unchanged,
        }
    }

    /// Puts `prefix` and a space before the first free-text field of this
    /// error, which is its primary detail.
    ///
    /// Used to label a transport or reply error with the backend name
    /// (`Jev HTTP 500 ...`). Provider replies already carry their provider
    /// name and are returned unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_agent_types::JevError;
    ///
    /// let error = JevError::input_invalid("bad").prefix_detail("Jev");
    /// assert_eq!(error, JevError::input_invalid("Jev bad"));
    /// ```
    #[must_use]
    pub fn prefix_detail(self, prefix: &str) -> Self {
        match self {
            Self::AuthFailed { .. }
            | Self::ApiRequestInvalid { .. }
            | Self::QuotaExhausted { .. }
            | Self::ApiError { .. }
            | Self::InputTooLarge { .. } => self,
            Self::Annotated {
                error,
                flush_failure,
                backend,
            } => Self::Annotated {
                error: Box::new(error.prefix_detail(prefix)),
                flush_failure,
                backend,
            },
            Self::ProviderBusy {
                detail,
                status,
                sends,
                cause,
                retry_after_seconds,
                reply,
            } => Self::ProviderBusy {
                detail: format!("{prefix} {detail}"),
                status,
                sends,
                cause,
                retry_after_seconds,
                reply: reply.map(|mut failure| {
                    if failure.provider.is_empty() {
                        failure.provider = prefix.to_owned();
                    }
                    failure
                }),
            },
            other => {
                let done = std::cell::Cell::new(false);
                other.map_text(&|text: &str| {
                    if done.replace(true) {
                        text.to_owned()
                    } else {
                        format!("{prefix} {text}")
                    }
                })
            }
        }
    }

    /// Returns the stable machine-readable id for this error.
    ///
    /// Ids match `tests/fixtures/jev/README-contract/errors.json` and are the
    /// only values CLI and PyO3 may emit in the `error` field. Several
    /// variants share an id (for example `RegistryInvalid` and
    /// `ConfigInvalid` are both `CONFIG_INVALID`).
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::ConfigInvalid { .. }
            | Self::CredentialStoreInvalid { .. }
            | Self::RegistryInvalid { .. } => ErrorCode::ConfigInvalid,
            Self::SessionDocumentInvalid { .. }
            | Self::EncodeFailed { .. }
            | Self::InputInvalid { .. } => ErrorCode::InputInvalid,
            Self::IoFailed { .. } => ErrorCode::IoFailed,
            Self::KeyMissing { .. } => ErrorCode::KeyMissing,
            Self::InputTooLarge { .. } => ErrorCode::InputTooLarge,
            Self::InputEmpty => ErrorCode::InputEmpty,
            Self::ExtractionEmpty => ErrorCode::ExtractionEmpty,
            Self::ExtractionParseError { .. } => ErrorCode::ExtractionParseError,
            Self::ExtractionInvalid { .. } => ErrorCode::ExtractionInvalid,
            Self::ExtractionFailed { .. } => ErrorCode::ExtractionFailed,
            Self::ExtractionTimeout { .. } | Self::GitTimeout { .. } => {
                ErrorCode::ExtractionTimeout
            }
            Self::LanguageUnavailable { .. } => ErrorCode::LanguageUnavailable,
            Self::QueryInvalid { .. } => ErrorCode::QueryInvalid,
            Self::QueryNotFound { .. } => ErrorCode::QueryNotFound,
            Self::ProducerError { code, .. } => *code,
            Self::InputMissing { .. } => ErrorCode::InputMissing,
            Self::ResponseInvalid { .. } => ErrorCode::ResponseInvalid,
            Self::NetworkError { .. } => ErrorCode::NetworkError,
            Self::AuthFailed { .. } => ErrorCode::AuthFailed,
            Self::ApiRequestInvalid { .. } => ErrorCode::ApiRequestInvalid,
            Self::QuotaExhausted { .. } => ErrorCode::QuotaExhausted,
            Self::ApiError { .. } => ErrorCode::ApiError,
            Self::ProviderBusy { .. } => ErrorCode::ProviderBusy,
            Self::PromptFileMissing { .. } => ErrorCode::PromptFileMissing,
            Self::SessionNotFound { .. } => ErrorCode::SessionNotFound,
            Self::SessionExpired { .. } => ErrorCode::SessionExpired,
            Self::Annotated { error, .. } => error.code(),
        }
    }

    /// The broad class of this error (the `kind` key of the stderr body).
    ///
    /// It follows the id ([`ErrorCode::category`]) except that a timed-out
    /// transport is `timeout`.
    #[must_use]
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::NetworkError {
                kind: NetworkErrorKind::Timeout,
                ..
            } => ErrorCategory::Timeout,
            Self::Annotated { error, .. } => error.category(),
            other => other.code().category(),
        }
    }

    /// Whether retrying the same request later may succeed.
    ///
    /// This is advice to the caller, not a promise that sc-agent resends:
    /// only busy statuses are resent automatically (a lost reply may mean the
    /// provider already processed and billed the request, so resending a
    /// non-busy failure could double-charge).
    ///
    /// - `NETWORK_ERROR`: true for `timeout` and `connect`, false for `tls`,
    ///   `cancelled` and `other` (an error the transport could not classify
    ///   is not assumed to be transient).
    /// - `PROVIDER_BUSY`: true; the provider said it is a matter of time.
    /// - `API_ERROR`: true for status 500 and above (500, 503 and 504
    ///   included): the provider reports a failure on its side that a later
    ///   attempt can get past, and Anthropic's own guidance for 500 is to
    ///   retry with backoff. False for other statuses, which a retry
    ///   without a change will repeat.
    /// - `IO_FAILED`: true for `interrupted` and `timed_out`, which are
    ///   transient by definition; false for the rest (a permission or a
    ///   missing file does not fix itself).
    /// - `EXTRACTION_TIMEOUT`: one id, one answer, false for both variants
    ///   ([`Self::GitTimeout`], [`Self::ExtractionTimeout`]). Each timed out
    ///   because of something the caller or the machine has to fix (a stuck
    ///   git, a lock file, a slow disk; a limit too small for the input), and
    ///   an unchanged retry meets the same cause. The action names the likely
    ///   cause for each.
    /// - A producer's error: the producer's own flag.
    /// - Everything else, `QUOTA_EXHAUSTED`, `AUTH_FAILED`,
    ///   `API_REQUEST_INVALID` and `RESPONSE_INVALID` included: false.
    #[must_use]
    pub fn retryable(&self) -> bool {
        match self {
            Self::NetworkError { kind, .. } => kind.retryable(),
            Self::ProviderBusy { .. } => true,
            Self::ApiError { failure } => failure.status >= 500,
            Self::ProducerError { retryable, .. } => *retryable,
            Self::IoFailed { kind, .. } => {
                matches!(kind, io::ErrorKind::Interrupted | io::ErrorKind::TimedOut)
            }
            Self::Annotated { error, .. } => error.retryable(),
            _ => false,
        }
    }

    /// Seconds to wait before a retry, when the provider or producer said.
    #[must_use]
    pub fn retry_after_seconds(&self) -> Option<u64> {
        match self {
            Self::ProviderBusy {
                retry_after_seconds,
                ..
            }
            | Self::ProducerError {
                retry_after_seconds,
                ..
            } => *retry_after_seconds,
            Self::ApiError { failure } => failure.retry_after_seconds,
            Self::Annotated { error, .. } => error.retry_after_seconds(),
            _ => None,
        }
    }

    /// The failing field, flag or argument, when the error names one.
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::InputInvalid { field, .. } => field.as_deref(),
            Self::ProducerError { field, .. } => field.as_deref(),
            Self::ApiRequestInvalid { failure } => failure.param.as_deref(),
            Self::Annotated { error, .. } => error.field(),
            _ => None,
        }
    }

    /// The session-log flush failure recorded by
    /// [`Self::with_log_flush_failure`].
    #[must_use]
    pub fn log_flush_failure(&self) -> Option<&str> {
        match self {
            Self::Annotated { flush_failure, .. } => flush_failure.as_deref(),
            _ => None,
        }
    }

    /// The backend recorded by [`Self::with_backend`].
    #[must_use]
    pub fn backend(&self) -> Option<&str> {
        match self {
            Self::Annotated { backend, .. } => backend.as_deref(),
            _ => None,
        }
    }

    /// What failed and why, for a person or a log: the `Display` text.
    ///
    /// It never holds the fix; that is [`Self::suggested_action`].
    #[must_use]
    pub fn message(&self) -> String {
        self.to_string()
    }

    /// The machine fields of this error (the `details` key of the stderr
    /// body), or `None` when the error has none.
    #[must_use]
    pub fn details(&self) -> Option<ErrorDetails> {
        let mut details = ErrorDetails::default();
        match self {
            Self::IoFailed { what, kind, .. } => {
                details.what = Some(what.clone());
                details.io_kind = Some(io_kind_id(*kind).to_owned());
            }
            Self::InputTooLarge {
                limit,
                actual,
                unit,
                provider,
            } => {
                details.limit = *limit;
                details.actual = *actual;
                details.unit = Some(unit.as_str().to_owned());
                if let Some(failure) = provider {
                    details.set_provider(failure);
                }
            }
            Self::InputMissing { path, .. } | Self::PromptFileMissing { path } => {
                details.path = Some(path.clone());
            }
            Self::ResponseInvalid { cause, .. } => {
                details.cause = Some(cause.as_str().to_owned());
            }
            Self::NetworkError { kind, .. } => {
                details.network_kind = Some(kind.as_str().to_owned());
            }
            Self::AuthFailed { failure }
            | Self::ApiRequestInvalid { failure }
            | Self::QuotaExhausted { failure }
            | Self::ApiError { failure } => {
                details.set_provider(failure);
            }
            Self::ProviderBusy {
                status,
                sends,
                cause,
                reply,
                ..
            } => {
                if let Some(failure) = reply {
                    details.set_provider(failure);
                }
                details.http_status = Some(*status);
                details.attempts = Some(*sends);
                details.cause = Some(cause.as_str().to_owned());
            }
            Self::ProducerError {
                producer,
                details: producer_details,
                ..
            } => {
                details = producer_details.as_deref().cloned().unwrap_or_default();
                details.producer = Some(producer.clone());
            }
            Self::Annotated {
                error,
                flush_failure,
                backend,
            } => {
                details = error.details().unwrap_or_default();
                details.log_flush_failure.clone_from(flush_failure);
                details.backend.clone_from(backend);
            }
            _ => return None,
        }
        Some(details)
    }

    /// Returns the actionable next step for this error, apart from the
    /// message.
    ///
    /// The CLI copies this string into the stderr `suggested_action` field.
    /// It is the one renderer of recovery text: the `recovery` fields are
    /// data it formats. Plain `CONFIG_INVALID` keeps the `check.toml` hint
    /// and plain `INPUT_INVALID` keeps the schema hint; the explicit recovery
    /// variants do not mention either.
    #[must_use]
    pub fn suggested_action(&self) -> String {
        match self {
            Self::ConfigInvalid { recovery, .. } => recovery.clone().unwrap_or_else(|| {
                "Fix the check.toml field or packaged jev.toml entry named in the message, then retry."
                    .to_owned()
            }),
            Self::CredentialStoreInvalid { recovery, .. }
            | Self::RegistryInvalid { recovery, .. }
            | Self::SessionDocumentInvalid { recovery, .. } => recovery.clone(),
            Self::EncodeFailed { .. } => {
                "Retry; if it persists, report it as a bug.".to_owned()
            }
            Self::IoFailed { what, recovery, .. } => recovery.clone().unwrap_or_else(|| {
                format!("Check that {what} exists and is readable or writable, then retry.")
            }),
            Self::KeyMissing { backend } => format!(
                "Provide a credential for {backend} via the process environment, the repo-root .env file, or the keyring, then retry."
            ),
            Self::InputTooLarge {
                limit,
                actual,
                unit,
                ..
            } => match (limit, actual) {
                (Some(limit), Some(actual)) => format!(
                    "Shrink the check input from {actual} {unit} to at most {limit} {unit}. One check is one request and is never split."
                ),
                _ => format!(
                    "Shorten the check input; the provider refused it as over its {unit} limit. One check is one request and is never split."
                ),
            },
            Self::InputEmpty => "Provide a non-empty input and retry.".to_owned(),
            Self::InputInvalid {
                field, recovery, ..
            } => match (recovery, field) {
                (Some(recovery), _) => recovery.clone(),
                (None, Some(field)) => {
                    format!("Correct {field} as the message says (see --help), then retry.")
                }
                (None, None) => {
                    "Correct the input so it matches the expected schema, then retry.".to_owned()
                }
            },
            Self::ExtractionEmpty => {
                "Change the query or input so extraction emits at least one subject.".to_owned()
            }
            Self::ExtractionParseError { path, line } => {
                format!("Fix the syntax error at {path}:{line} and run extraction again.")
            }
            Self::ExtractionInvalid { .. } => {
                "Emit a valid sc/subjects/1 document on stdout and retry.".to_owned()
            }
            Self::ExtractionFailed { .. } => {
                "Fix the producer failure named in the message and run extraction again."
                    .to_owned()
            }
            Self::ExtractionTimeout { timeout_seconds } => format!(
                "Raise timeout_seconds above {timeout_seconds} or reduce the input so extraction finishes in time."
            ),
            Self::GitTimeout { seconds } => format!(
                "Check for a stalled git process, a lock file (such as .git/index.lock) or a slow filesystem; the same call times out again after {seconds}s until that is fixed."
            ),
            Self::LanguageUnavailable { producer, language } => {
                format!("Use a language built into {producer} instead of {language}.")
            }
            Self::QueryInvalid { .. } => {
                "Fix the tree-sitter query syntax named in the message and retry.".to_owned()
            }
            Self::QueryNotFound { query, .. } => {
                format!("Choose a query asset that exists for this language instead of {query}.")
            }
            Self::ProducerError {
                producer,
                suggested_action,
                ..
            } => {
                if suggested_action.is_empty() {
                    format!("See the {producer} message above.")
                } else {
                    format!("{producer}: {suggested_action}")
                }
            }
            Self::InputMissing { path, recovery } => match recovery {
                Some(recovery) => recovery.clone(),
                None => format!("Create {path} or correct the path, then retry."),
            },
            Self::ResponseInvalid { cause, .. } => match cause {
                ResponseInvalidCause::Truncated => "The provider stopped at its output limit. Ask fewer questions per check or shorten the state, then run it again; retrying the same check gives the same cut-off.".to_owned(),
                ResponseInvalidCause::TooLarge => "The reply was larger than the read cap. Retrying gives the same reply; report it, and check whether a proxy rewrites responses.".to_owned(),
                ResponseInvalidCause::NotJson => "The reply was not valid JSON. Check the endpoint and any proxy that rewrites responses; if the provider's own reply is fine, report the model's answer as a backend failure. Retrying the same request is unlikely to help.".to_owned(),
                ResponseInvalidCause::Schema => "The provider's answers do not fit the questions. Retrying is unlikely to help; report it as a backend failure with the detail above.".to_owned(),
            },
            Self::NetworkError { kind, .. } => match kind {
                NetworkErrorKind::Timeout => "The request timed out. Retry later; a busy or slow provider may need a minute.".to_owned(),
                NetworkErrorKind::Connect => "Check connectivity and DNS to the provider, then retry.".to_owned(),
                NetworkErrorKind::Tls => "Fix the TLS setup (system clock, certificates, proxy) before retrying; retrying alone will fail again.".to_owned(),
                NetworkErrorKind::Cancelled => "The request was cancelled. Run it again if you still need the result.".to_owned(),
                NetworkErrorKind::Other => "The transport failed in a way sc-agent could not classify, so it is not assumed to be transient. Check the endpoint, proxy and network; if it keeps happening, report it with the detail above.".to_owned(),
            },
            Self::AuthFailed { failure } => format!(
                "Check that the {} credential is valid, not revoked, and allowed to use this model; replace it in the process environment, the repo-root .env file or the keyring. Retrying with the same credential fails again.",
                failure.provider
            ),
            Self::ApiRequestInvalid { failure } => match &failure.param {
                Some(param) => format!(
                    "Correct {param} as the provider's message says. Retrying the same request fails again."
                ),
                None => "Correct the request as the provider's message says. Retrying the same request fails again.".to_owned(),
            },
            Self::QuotaExhausted { failure } => format!(
                "{} reports no tokens, credit or spend allowance left. Add credit, raise the limit, or wait for it to renew; retrying now fails again. Use another backend meanwhile.",
                failure.provider
            ),
            Self::ApiError { failure } => {
                if failure.status >= 500 {
                    format!(
                        "{} failed on its side. Wait and retry; if it keeps failing, check the provider's status page.",
                        failure.provider
                    )
                } else {
                    format!(
                        "{} answered with an unexpected status. Check the endpoint and the request; retrying unchanged is unlikely to help.",
                        failure.provider
                    )
                }
            }
            Self::ProviderBusy { cause, retry_after_seconds, .. } => {
                let wait = match retry_after_seconds {
                    Some(seconds) => format!("Wait at least {seconds} seconds"),
                    None => "Wait a minute".to_owned(),
                };
                match cause {
                    BusyCause::Sends => format!("{wait}, then retry; every allowed send was refused as busy. Another backend may have room."),
                    BusyCause::Budget => format!("{wait}, then retry; the retry budget ran out while the provider stayed busy. Another backend may have room."),
                }
            }
            Self::PromptFileMissing { path } => format!(
                "Provide the LLM system prompt at {path}, or select it with check.toml or the CLI override."
            ),
            Self::SessionNotFound { session_id } => format!(
                "Create a session or pass the id of a session that still exists. Missing session: {session_id}."
            ),
            Self::SessionExpired { session_id } => format!(
                "Start a new session. Session {session_id} expired; expiry is measured from the session created_at timestamp."
            ),
            Self::Annotated { error, .. } => error.suggested_action(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Completeness of [`ErrorCode::ALL`]: this `match` has no wildcard arm,
    /// so a new variant fails to compile until it is listed here, and each
    /// arm asserts the variant is in `ALL`.
    #[test]
    fn test_error_code_all_is_complete() {
        let in_all = |code: ErrorCode| assert!(ErrorCode::ALL.contains(&code), "{code}");
        for code in ErrorCode::ALL.iter().copied() {
            match code {
                ErrorCode::ConfigInvalid => in_all(ErrorCode::ConfigInvalid),
                ErrorCode::KeyMissing => in_all(ErrorCode::KeyMissing),
                ErrorCode::InputTooLarge => in_all(ErrorCode::InputTooLarge),
                ErrorCode::InputEmpty => in_all(ErrorCode::InputEmpty),
                ErrorCode::InputInvalid => in_all(ErrorCode::InputInvalid),
                ErrorCode::ExtractionEmpty => in_all(ErrorCode::ExtractionEmpty),
                ErrorCode::ExtractionParseError => in_all(ErrorCode::ExtractionParseError),
                ErrorCode::ExtractionInvalid => in_all(ErrorCode::ExtractionInvalid),
                ErrorCode::ExtractionFailed => in_all(ErrorCode::ExtractionFailed),
                ErrorCode::ExtractionTimeout => in_all(ErrorCode::ExtractionTimeout),
                ErrorCode::LanguageUnavailable => in_all(ErrorCode::LanguageUnavailable),
                ErrorCode::QueryInvalid => in_all(ErrorCode::QueryInvalid),
                ErrorCode::QueryNotFound => in_all(ErrorCode::QueryNotFound),
                ErrorCode::InputMissing => in_all(ErrorCode::InputMissing),
                ErrorCode::ResponseInvalid => in_all(ErrorCode::ResponseInvalid),
                ErrorCode::NetworkError => in_all(ErrorCode::NetworkError),
                ErrorCode::PromptFileMissing => in_all(ErrorCode::PromptFileMissing),
                ErrorCode::SessionNotFound => in_all(ErrorCode::SessionNotFound),
                ErrorCode::SessionExpired => in_all(ErrorCode::SessionExpired),
                ErrorCode::AuthFailed => in_all(ErrorCode::AuthFailed),
                ErrorCode::ApiRequestInvalid => in_all(ErrorCode::ApiRequestInvalid),
                ErrorCode::QuotaExhausted => in_all(ErrorCode::QuotaExhausted),
                ErrorCode::ProviderBusy => in_all(ErrorCode::ProviderBusy),
                ErrorCode::ApiError => in_all(ErrorCode::ApiError),
                ErrorCode::IoFailed => in_all(ErrorCode::IoFailed),
            }
        }
        let unique: std::collections::BTreeSet<_> = ErrorCode::ALL.iter().collect();
        assert_eq!(unique.len(), ErrorCode::ALL.len());
    }

    fn failure() -> ProviderFailure {
        ProviderFailure::new("Jev", 500, "boom")
            .with_type("api_error")
            .with_code("c")
            .with_param("p")
            .with_request_id("r")
    }

    /// Number of text values `map_detail` must visit in `error`, written
    /// without `map_detail`.
    fn text_fields(error: &JevError) -> usize {
        let provider_texts = 6; // provider, message, type, code, param, request id
        match error {
            JevError::CredentialStoreInvalid { .. } => 3,
            JevError::ConfigInvalid { recovery, .. } => 1 + usize::from(recovery.is_some()),
            JevError::IoFailed { recovery, .. } => 1 + usize::from(recovery.is_some()),
            JevError::InputInvalid {
                field, recovery, ..
            } => 1 + usize::from(field.is_some()) + usize::from(recovery.is_some()),
            JevError::ProducerError { field, details, .. } => {
                3 + usize::from(field.is_some())
                    + details
                        .as_ref()
                        .map_or(0, |d| usize::from(d.cause.is_some()))
            }
            JevError::RegistryInvalid { .. }
            | JevError::SessionDocumentInvalid { .. }
            | JevError::EncodeFailed { .. }
            | JevError::LanguageUnavailable { .. }
            | JevError::QueryInvalid { .. }
            | JevError::QueryNotFound { .. } => 2,
            JevError::ExtractionParseError { .. }
            | JevError::ExtractionInvalid { .. }
            | JevError::ExtractionFailed { .. }
            | JevError::InputMissing { .. }
            | JevError::ResponseInvalid { .. }
            | JevError::NetworkError { .. }
            | JevError::PromptFileMissing { .. } => 1,
            JevError::ProviderBusy { reply, .. } => {
                1 + if reply.is_some() { provider_texts } else { 0 }
            }
            JevError::AuthFailed { .. }
            | JevError::ApiRequestInvalid { .. }
            | JevError::QuotaExhausted { .. }
            | JevError::ApiError { .. } => provider_texts,
            JevError::InputTooLarge { provider, .. } => {
                if provider.is_some() {
                    provider_texts
                } else {
                    0
                }
            }
            JevError::Annotated {
                error,
                flush_failure,
                backend,
            } => {
                text_fields(error)
                    + usize::from(flush_failure.is_some())
                    + usize::from(backend.is_some())
            }
            JevError::KeyMissing { .. }
            | JevError::InputEmpty
            | JevError::ExtractionEmpty
            | JevError::ExtractionTimeout { .. }
            | JevError::GitTimeout { .. }
            | JevError::SessionNotFound { .. }
            | JevError::SessionExpired { .. } => 0,
        }
    }

    fn samples() -> Vec<JevError> {
        let session: SessionId = "00000000000000000000000000".parse().unwrap();
        let cause = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        vec![
            JevError::config_invalid("missing model"),
            JevError::config_invalid_with("no toplevel", "Pass --repo-root."),
            JevError::credential_store_invalid("the key", "permission denied", "Fix it."),
            JevError::registry_invalid("Backend jev is not registered", "Register it."),
            JevError::session_document_invalid("Session file s is bad", "Delete it."),
            JevError::encode_failed("payload", &cause),
            JevError::io_failed("stdout", &io::Error::from(io::ErrorKind::BrokenPipe)),
            JevError::io_failed_with("git", io::ErrorKind::NotFound, Some("Install.".into())),
            JevError::key_missing(BackendKeyId::TypeSafe),
            JevError::input_too_large(10, 11),
            JevError::input_too_large_by_provider(LimitUnit::Tokens, failure()),
            JevError::InputEmpty,
            JevError::input_invalid("bad json"),
            JevError::input_invalid_field("--backend", "bad"),
            JevError::ExtractionEmpty,
            JevError::extraction_parse_error("a.py", 3),
            JevError::extraction_invalid("schema"),
            JevError::extraction_failed("exit 1"),
            JevError::extraction_timeout(30),
            JevError::git_timeout(60),
            JevError::language_unavailable("sc-treesitter", "go"),
            JevError::query_invalid("sc-treesitter", "syntax"),
            JevError::query_not_found("sc-treesitter", "missing"),
            JevError::producer_error("sc-treesitter", ErrorCode::QueryInvalid, "bad"),
            JevError::input_missing("gone.toml"),
            JevError::response_invalid("truncated"),
            JevError::network(NetworkErrorKind::Connect, "reset"),
            JevError::auth_failed(failure()),
            JevError::api_request_invalid(failure()),
            JevError::quota_exhausted(failure()),
            JevError::api_error(failure()),
            JevError::provider_busy("busy", 429, 4, BusyCause::Sends, Some(3)),
            JevError::provider_busy("busy", 429, 4, BusyCause::Sends, None)
                .with_busy_reply(failure()),
            JevError::prompt_file_missing("prompt.md"),
            JevError::session_not_found(session),
            JevError::session_expired(session),
            JevError::InputEmpty.with_log_flush_failure("disk full"),
            JevError::InputEmpty.with_backend("jev"),
        ]
    }

    #[test]
    fn map_detail_touches_every_text_field_of_every_variant() {
        const MARK: &str = "<<mapped>>";
        for error in samples() {
            let calls = std::cell::Cell::new(0_usize);
            let mapped = error.clone().map_detail(|text| {
                calls.set(calls.get() + 1);
                format!("{MARK}{text}")
            });
            assert_eq!(calls.get(), text_fields(&error), "{error:?}");
            assert_eq!(mapped.code(), error.code());
            if text_fields(&error) == 0 {
                assert_eq!(mapped, error);
            } else {
                assert_ne!(mapped, error, "{error:?}");
            }
        }
    }

    #[test]
    fn prefix_detail_labels_the_primary_detail_once() {
        let labelled = JevError::network(NetworkErrorKind::Connect, "reset").prefix_detail("Jev");
        assert_eq!(labelled.message(), "network error (connect): Jev reset");
        let busy = JevError::provider_busy("HTTP 429 busy", 429, 1, BusyCause::Sends, None)
            .prefix_detail("luna");
        assert!(busy.message().contains("luna HTTP 429 busy"));
        // Provider replies already name their provider.
        let api = JevError::api_error(failure());
        assert_eq!(api.clone().prefix_detail("Jev"), api);
    }

    #[test]
    fn source_is_none_for_every_variant() {
        use std::error::Error as _;
        for error in samples() {
            assert!(error.source().is_none(), "{error:?}");
        }
    }

    /// Every id is pinned: each listed kind maps to its fixed literal, every
    /// literal is in the closed set, and the set has no duplicates.
    #[test]
    fn io_kind_ids_are_a_fixed_closed_set() {
        let pinned: &[(io::ErrorKind, &str)] = &[
            (io::ErrorKind::NotFound, "not_found"),
            (io::ErrorKind::PermissionDenied, "permission_denied"),
            (io::ErrorKind::ConnectionRefused, "connection_refused"),
            (io::ErrorKind::ConnectionReset, "connection_reset"),
            (io::ErrorKind::HostUnreachable, "host_unreachable"),
            (io::ErrorKind::NetworkUnreachable, "network_unreachable"),
            (io::ErrorKind::ConnectionAborted, "connection_aborted"),
            (io::ErrorKind::NotConnected, "not_connected"),
            (io::ErrorKind::AddrInUse, "addr_in_use"),
            (io::ErrorKind::AddrNotAvailable, "addr_not_available"),
            (io::ErrorKind::NetworkDown, "network_down"),
            (io::ErrorKind::BrokenPipe, "broken_pipe"),
            (io::ErrorKind::AlreadyExists, "already_exists"),
            (io::ErrorKind::WouldBlock, "would_block"),
            (io::ErrorKind::NotADirectory, "not_a_directory"),
            (io::ErrorKind::IsADirectory, "is_a_directory"),
            (io::ErrorKind::DirectoryNotEmpty, "directory_not_empty"),
            (io::ErrorKind::ReadOnlyFilesystem, "read_only_filesystem"),
            (
                io::ErrorKind::StaleNetworkFileHandle,
                "stale_network_file_handle",
            ),
            (io::ErrorKind::InvalidInput, "invalid_input"),
            (io::ErrorKind::InvalidData, "invalid_data"),
            (io::ErrorKind::TimedOut, "timed_out"),
            (io::ErrorKind::WriteZero, "write_zero"),
            (io::ErrorKind::StorageFull, "storage_full"),
            (io::ErrorKind::NotSeekable, "not_seekable"),
            (io::ErrorKind::QuotaExceeded, "quota_exceeded"),
            (io::ErrorKind::FileTooLarge, "file_too_large"),
            (io::ErrorKind::ResourceBusy, "resource_busy"),
            (io::ErrorKind::ExecutableFileBusy, "executable_file_busy"),
            (io::ErrorKind::Deadlock, "deadlock"),
            (io::ErrorKind::CrossesDevices, "crosses_devices"),
            (io::ErrorKind::TooManyLinks, "too_many_links"),
            (io::ErrorKind::InvalidFilename, "invalid_filename"),
            (io::ErrorKind::ArgumentListTooLong, "argument_list_too_long"),
            (io::ErrorKind::Interrupted, "interrupted"),
            (io::ErrorKind::Unsupported, "unsupported"),
            (io::ErrorKind::UnexpectedEof, "unexpected_eof"),
            (io::ErrorKind::OutOfMemory, "out_of_memory"),
            (io::ErrorKind::Other, "other"),
        ];
        assert_eq!(pinned.len(), IO_KIND_IDS.len());
        for (kind, id) in pinned {
            assert_eq!(io_kind_id(*kind), *id, "{kind:?}");
            assert!(IO_KIND_IDS.contains(id), "{id}");
        }
        let unique: std::collections::BTreeSet<_> = IO_KIND_IDS.iter().collect();
        assert_eq!(unique.len(), IO_KIND_IDS.len());
        // Uncategorized (and any future kind) falls back to `other`.
        let uncategorized = io::Error::from_raw_os_error(i32::MAX).kind();
        assert!(IO_KIND_IDS.contains(&io_kind_id(uncategorized)));
    }

    #[test]
    fn bound_provider_text_cuts_on_a_character_boundary() {
        let text = "ä".repeat(PROVIDER_TEXT_MAX_BYTES);
        let bounded = bound_provider_text(&text);
        assert!(bounded.len() <= PROVIDER_TEXT_MAX_BYTES);
        assert!(bounded.ends_with("..."));
        assert_eq!(bound_provider_text("  a\tb\n"), "a b");
    }
}
