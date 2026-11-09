//! TypeSafe Jev HTTP client.
//!
//! `sc-jev` owns every TypeSafe Jev HTTP operation (REQ-SAG-0110). Phase A
//! implements the check/judge slice: [`JevClient`] implements
//! [`JudgeBackend`](sc_decision_schema::JudgeBackend), posts the documented Jev
//! body (`{model, state, questions}`, from `sc_decision_schema::jev::v1`) with
//! the pinned model, and retries busy responses.
//!
//! The crate does not read `check.toml`, run extractors, or resolve API keys.
//! Callers pass a [`ResolvedApiKey`](sc_agent_types::ResolvedApiKey) and an
//! `sc_agent_http::Transport` at construction. HTTP, busy retry, the 8 MiB
//! response cap and redaction live in `sc-agent-http` (ADR-SAG-0026); this
//! crate owns the Jev wire body and the reply parser (ADR-SAG-0012).
//!
//! # Errors
//!
//! Failures use [`JevError`](sc_agent_types::JevError). Oversize input is
//! `INPUT_TOO_LARGE` and is not sent. Busy HTTP statuses (429, 502, 529)
//! retry under `sc_agent_http::BusyPolicy` (REQ-SAG-0505) before a
//! `NETWORK_ERROR` kind `Busy`. A reply that fails the answer contract is
//! `RESPONSE_INVALID`.

#![warn(missing_docs)]

mod check;
mod client;
mod config;

#[cfg(feature = "test-util")]
#[doc(inline)]
pub use check::WireFault;
#[doc(inline)]
pub use client::JevClient;
#[doc(inline)]
pub use config::JevClientConfig;

/// The SHA-256 of the exact request body the Jev client sends for `payload`
/// with `model`, computed before sending and without an API key.
///
/// It equals the `request_sha256` the session log records for the same send,
/// and changes when a question, the state or the model changes. It identifies
/// an identical request, so a consumer can substitute a cached response.
/// `jev` is sent as `jev-latest`, so a digest made with `jev` can go stale
/// when Jev moves; use a pinned version for caching. The size limit is not
/// applied here.
///
/// # Errors
///
/// `INPUT_INVALID` when the payload has no question or breaks a question rule.
///
/// # Examples
///
/// ```
/// use sc_decision_schema::PayloadBundle;
///
/// let payload = PayloadBundle::decode(
///     br#"{"questions":[{"choices":["no","yes"],"id":"q","prompt":"Is it?"}],"state":[]}"#,
/// )?;
/// let a = sc_jev::request_sha256(&"jev-1.13.0".parse()?, &payload)?;
/// let b = sc_jev::request_sha256(&"jev-1.14.0".parse()?, &payload)?;
/// assert_ne!(a, b);
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
pub fn request_sha256(
    model: &sc_agent_types::ModelId,
    payload: &sc_decision_schema::PayloadBundle,
) -> Result<sc_agent_types::RequestSha256, sc_agent_types::JevError> {
    check::request_sha256(model, payload)
}
