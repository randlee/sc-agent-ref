//! Packaged TypeSafe endpoint and retry limits, and the configured model.
//!
//! Later Jev endpoints extend this crate in place (ADR-SAG-0012). No other
//! crate sends Jev API traffic.

use std::time::Duration;

use sc_agent_http::Endpoint;
use sc_agent_types::{BackendId, JevError, ModelId};
use serde::Deserialize;

/// Most sends per call (owner decision (b), `BusyPolicy::default`).
const MAX_ATTEMPTS: u32 = 4;

/// Longest in-flight request (owner decision (b), ADR-SAG-0007).
const MAX_TIMEOUT: Duration = Duration::from_secs(120);

/// TypeSafe check/judge client settings.
///
/// The packaged defaults live in `jev.toml` and are the vendor pins for the
/// API version (`/v1`). The evaluator model is a [`ModelId`]: it defaults to
/// the catalog's Jev model and the backend factory sets it (the `[models]`
/// override of `check.toml`), so the catalog is the one source of the string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JevClientConfig {
    model: ModelId,
    host: String,
    path: String,
    endpoint: Endpoint,
    attempts: u32,
    timeout: Duration,
}

/// On-disk shape of `jev.toml`. Every field is required; unknown keys are
/// rejected. Range checks live in [`JevClientConfig::try_new`] only.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJevToml {
    host: String,
    path: String,
    attempts: u32,
    timeout_seconds: u64,
}

impl JevClientConfig {
    /// Loads the committed TypeSafe pins.
    ///
    /// # Panics
    ///
    /// Panics if `jev.toml` is missing a field or fails validation. That file
    /// is compiled into the crate, so this is a packaging bug.
    #[must_use]
    pub fn packaged() -> Self {
        Self::from_toml(include_str!("jev.toml")).expect("packaged jev.toml is valid")
    }

    /// Parses a Jev client config document.
    ///
    /// # Errors
    ///
    /// Returns [`JevError::ConfigInvalid`] when the text is not TOML, a
    /// required field is missing or has the wrong type, an unknown key is
    /// present, or a value fails [`Self::try_new`].
    pub fn from_toml(text: &str) -> Result<Self, JevError> {
        let raw: RawJevToml = toml::from_str(text)
            .map_err(|error| JevError::config_invalid(format!("jev.toml: {}", error.message())))?;
        Self::try_new(
            BackendId::Jev.descriptor().default_model(),
            raw.host,
            raw.path,
            raw.attempts,
            Duration::from_secs(raw.timeout_seconds),
        )
    }

    /// Builds a config from validated parts.
    ///
    /// # Errors
    ///
    /// Returns [`JevError::ConfigInvalid`] when the host, path, attempt
    /// count, or timeout cannot be used for a check request, or
    /// `https://{host}{path}` is not a valid https URL.
    pub fn try_new(
        model: ModelId,
        host: impl Into<String>,
        path: impl Into<String>,
        attempts: u32,
        timeout: Duration,
    ) -> Result<Self, JevError> {
        let host = host.into();
        let path = path.into();
        if host.is_empty()
            || host.contains("://")
            || host.contains('/')
            || host.bytes().any(|byte| byte.is_ascii_whitespace())
        {
            return Err(invalid(
                "host must be a bare hostname such as api.typesafe.ai",
            ));
        }
        if !path.starts_with('/') || path.bytes().any(|byte| byte.is_ascii_whitespace()) {
            return Err(invalid(
                "path must be an absolute URL path such as /v1/systemone",
            ));
        }
        // Owner decision (b): at most 4 sends and a 120 s in-flight request.
        if !(1..=MAX_ATTEMPTS).contains(&attempts) {
            return Err(invalid("attempts must be from 1 to 4"));
        }
        if timeout.is_zero() || timeout > MAX_TIMEOUT {
            return Err(invalid("timeout must be from 1 second to 120 seconds"));
        }
        let endpoint = Endpoint::parse(&format!("https://{host}{path}"))?;
        Ok(Self {
            model,
            host,
            path,
            endpoint,
            attempts,
            timeout,
        })
    }

    /// Evaluator model sent on the wire and reported by
    /// `JudgeBackend::model_id`.
    #[must_use]
    pub fn model(&self) -> &ModelId {
        &self.model
    }

    /// Replaces the model; the backend factory calls this with the catalog
    /// default or the `check.toml` `[models]` override.
    #[must_use]
    pub fn with_model(mut self, model: ModelId) -> Self {
        self.model = model;
        self
    }

    /// API hostname, without a scheme.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Absolute URL path, including the `/v1` API version prefix.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Total sends on a busy reply, including the first.
    ///
    /// The packaged value is 4, the `sc_agent_http::BusyPolicy::default`
    /// count (REQ-SAG-0505).
    #[must_use]
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Request timeout of the default transport, body read included.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// `https://{host}{path}` for this config.
    #[must_use]
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

fn invalid(detail: impl Into<String>) -> JevError {
    JevError::config_invalid(detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str =
        "host = \"h.example\"\npath = \"/v1/x\"\nattempts = 2\ntimeout_seconds = 5\n";

    #[test]
    fn from_toml_reads_every_field() {
        let config = JevClientConfig::from_toml(VALID).expect("valid");
        assert_eq!(config.model(), &ModelId::Jev1_13_0);
        assert_eq!(config.endpoint().as_str(), "https://h.example/v1/x");
        assert_eq!(config.attempts(), 2);
        assert_eq!(config.timeout(), Duration::from_secs(5));
    }

    #[test]
    fn ranges_stop_at_the_owner_decided_bounds() {
        let at_bounds = VALID
            .replace("attempts = 2", "attempts = 4")
            .replace("timeout_seconds = 5", "timeout_seconds = 120");
        let config = JevClientConfig::from_toml(&at_bounds).expect("bounds are valid");
        assert_eq!((config.attempts(), config.timeout()), (4, MAX_TIMEOUT));
    }

    #[test]
    fn from_toml_rejects_bad_documents() {
        let cases = [
            ("not = [toml", "jev.toml"),
            (&VALID.replace("attempts = 2\n", "") as &str, "attempts"),
            (&format!("{VALID}extra = 1\n"), "extra"),
            (
                &VALID.replace("timeout_seconds = 5", "timeout_seconds = 0"),
                "timeout",
            ),
            (
                &VALID.replace("timeout_seconds = 5", "timeout_seconds = 121"),
                "timeout",
            ),
            (
                &VALID.replace("timeout_seconds = 5", "timeout_seconds = -1"),
                "jev.toml",
            ),
            (&VALID.replace("attempts = 2", "attempts = 5"), "attempts"),
            (&VALID.replace("attempts = 2", "attempts = 0"), "attempts"),
            (
                &format!("{VALID}max_request_bytes = 10\n"),
                "max_request_bytes",
            ),
            (&format!("{VALID}model = \"jev-9\"\n"), "model"),
            (&VALID.replace("h.example", "https://h.example"), "host"),
            (&VALID.replace("/v1/x", "v1/x"), "path"),
        ];
        for (text, needle) in cases {
            let error = JevClientConfig::from_toml(text).expect_err(needle);
            assert_eq!(error.code(), "CONFIG_INVALID", "{needle}");
            assert!(error.to_string().contains(needle), "{needle}: {error}");
        }
    }
}
