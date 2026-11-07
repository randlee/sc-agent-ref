//! `JevClient` as a [`JudgeBackend`].
//!
//! The transport, the resolved API key, the busy-retry policy and the cancel
//! token are construction state. `judge` takes none of them (ADR-SAG-0006).
//! HTTP, busy retry, the response cap and redaction are `sc-agent-http`
//! (ADR-SAG-0026); this module builds the request and reads the reply.

use std::fmt;
use std::sync::Arc;

use sc_agent_http::{
    BusyPolicy, HeaderName, HttpClient, HttpRequest, Method, SecretHeader, Timeouts, Transport,
    UreqTransport,
};
use sc_agent_types::{
    BackendId, CancelToken, JevError, JudgeBackend, JudgeOutcome, JudgeRequest, NetworkErrorKind,
    ResolvedApiKey,
};

#[cfg(feature = "test-util")]
use crate::check::WireFault;
use crate::check::{self, PreparedCheck};
use crate::config::JevClientConfig;

const CONTENT_TYPE: HeaderName = HeaderName::new("Content-Type");

/// TypeSafe check/judge client.
///
/// Posts `JudgeRequest::payload` bytes with the pinned model spliced in
/// front, as `Authorization: Bearer` over `T` (by default
/// [`UreqTransport`]). Ignores `JudgeRequest::llm`. Busy replies (429, 502,
/// 529) are retried with the same body under
/// [`BusyPolicy`] (REQ-SAG-0505): `Retry-After` honoured up to 30 s,
/// otherwise 1, 2, 4 s backoff, plus jitter, within a 90 s budget.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use sc_agent_http::{HttpRequest, HttpResponse, Transport};
/// use sc_agent_types::{
///     BackendKeyId, CanonicalJudgePayload, JevError, JudgeBackend, JudgeRequest, PayloadBundle,
///     ResolvedApiKey,
/// };
/// use sc_jev::JevClient;
///
/// struct Scripted;
///
/// impl Transport for Scripted {
///     fn send(&self, request: &HttpRequest) -> Result<HttpResponse, JevError> {
///         assert!(request.body().starts_with(br#"{"model":"jev-1.13.0","#));
///         assert!(request.secret().expect("bearer").matches("test-key"));
///         Ok(HttpResponse::new(
///             200,
///             Vec::new(),
///             br#"{"model":"jev-1.13.0","answers":{"q":{"choice":"yes","confidence":1,"probabilities":{"no":0,"yes":1}}}}"#.to_vec(),
///         ))
///     }
/// }
///
/// let payload: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
///     "questions": [{"id": "q", "prompt": "Is it?", "choices": ["no", "yes"]}],
///     "state": []
/// }))
/// .expect("payload");
/// let bundle = PayloadBundle::encode(&payload)?;
/// let client = JevClient::new(
///     ResolvedApiKey::try_new(BackendKeyId::TypeSafe, "test-key".to_owned())?,
///     Scripted,
/// );
/// let outcome = client.judge(&JudgeRequest::new(Arc::new(bundle), None))?;
/// assert_eq!(outcome.answers()["q"].choice().as_str(), "yes");
/// # Ok::<(), JevError>(())
/// ```
pub struct JevClient<T = UreqTransport> {
    config: JevClientConfig,
    api_key: ResolvedApiKey,
    http: HttpClient<T>,
    #[cfg(feature = "test-util")]
    wire_fault: Option<WireFault>,
}

impl JevClient<UreqTransport> {
    /// Creates a client for the packaged endpoint over [`UreqTransport`].
    ///
    /// The transport uses a 10 s connect timeout and the packaged `jev.toml`
    /// request timeout. The backend registry calls only this constructor, so
    /// the transport type can change without touching `sc-jev-check`.
    #[must_use]
    pub fn with_default_transport(api_key: ResolvedApiKey) -> Self {
        let config = JevClientConfig::packaged();
        let transport =
            UreqTransport::new(Timeouts::new(Timeouts::DEFAULT_CONNECT, config.timeout()));
        Self::with_config(config, api_key, transport)
    }
}

impl<T: Transport> JevClient<T> {
    /// Creates a client for the packaged TypeSafe endpoint over `transport`.
    ///
    /// `api_key` is the resolved credential, already normalized by
    /// [`ResolvedApiKey::try_new`]. It travels only as a [`SecretHeader`];
    /// the client does not read it (ADR-SAG-0020).
    ///
    /// Construction is infallible: key validation happens once, in
    /// [`ResolvedApiKey::try_new`] (ADR-SAG-0020 r-1 amendment).
    #[must_use]
    pub fn new(api_key: ResolvedApiKey, transport: T) -> Self {
        Self::with_config(JevClientConfig::packaged(), api_key, transport)
    }

    /// Creates a client with an explicit endpoint, model and send count.
    ///
    /// The busy policy is [`BusyPolicy::default`] with
    /// [`JevClientConfig::attempts`] sends.
    #[must_use]
    pub fn with_config(config: JevClientConfig, api_key: ResolvedApiKey, transport: T) -> Self {
        let http = HttpClient::new(transport).with_busy_policy(BusyPolicy::new(config.attempts()));
        Self {
            config,
            api_key,
            http,
            #[cfg(feature = "test-util")]
            wire_fault: None,
        }
    }

    /// Seeds `fault` into the wire builder (feature `test-util`).
    ///
    /// The fault is applied after the canonical bytes are embedded and
    /// before the reported `input_sha256` is recovered from the body, so a
    /// test shows the digest follows the bytes that would be sent
    /// (REQ-SAG-0606). Production builds leave the feature off.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn with_wire_fault(mut self, fault: WireFault) -> Self {
        self.wire_fault = Some(fault);
        self
    }

    /// Replaces the busy-retry policy, its send count included.
    ///
    /// Tests pass `BusyPolicy::with_test_hooks` (feature
    /// `sc-agent-http/test-util`) so nothing sleeps.
    #[must_use]
    pub fn with_busy_policy(mut self, policy: BusyPolicy) -> Self {
        self.http = self.http.with_busy_policy(policy);
        self
    }

    /// Sets the token that cancels a `judge` call between attempts and
    /// during busy waits.
    ///
    /// A cancel during an in-flight request is seen when it returns, so the
    /// worst case is the transport's request timeout.
    #[must_use]
    pub fn with_cancel(mut self, cancel: CancelToken) -> Self {
        self.http = self.http.with_cancel(cancel);
        self
    }

    /// Endpoint, model, and limits used for every `judge` call.
    #[must_use]
    pub fn config(&self) -> &JevClientConfig {
        &self.config
    }

    /// The Jev descriptor's request limit.
    fn limit_bytes(&self) -> u64 {
        BackendId::Jev.descriptor().request_limit_bytes()
    }

    #[cfg(feature = "test-util")]
    fn apply_wire_fault(&self, prepared: PreparedCheck, judged: &JudgeRequest) -> PreparedCheck {
        let Some(fault) = self.wire_fault else {
            return prepared;
        };
        // The embedded payload is everything after the spliced `{"model":..,`.
        let embedded = judged.payload().bytes().len().saturating_sub(1);
        let payload_start = prepared.body.len().saturating_sub(embedded);
        PreparedCheck {
            body: fault.apply(prepared.body, payload_start),
            criteria: prepared.criteria,
        }
    }

    #[cfg(not(feature = "test-util"))]
    fn apply_wire_fault(&self, prepared: PreparedCheck, _judged: &JudgeRequest) -> PreparedCheck {
        prepared
    }

    fn execute(
        &self,
        prepared: PreparedCheck,
        judged: &JudgeRequest,
    ) -> Result<JudgeOutcome, JevError> {
        let PreparedCheck { body, criteria } = prepared;
        let request = HttpRequest::new(
            Method::Post,
            self.config.endpoint().clone(),
            Arc::from(body),
        )
        .with_header(CONTENT_TYPE, "application/json")
        .with_secret(SecretHeader::bearer(self.api_key.clone()));
        // Every error, transport or reply, is redacted inside
        // `send_and_parse` and then labelled with the backend name.
        self.http
            .send_and_parse(&request, |response| {
                if response.status() != 200 {
                    return Err(JevError::network(
                        NetworkErrorKind::Status,
                        format!("HTTP {}", response.status()),
                    ));
                }
                check::parse_outcome(response.body(), self.config.model(), &criteria, judged)
            })
            .map_err(|error| error.map_detail(|detail| format!("Jev {detail}")))
    }
}

impl<T: Transport> JudgeBackend for JevClient<T> {
    fn id(&self) -> BackendId {
        BackendId::Jev
    }

    fn validate_for_send(&self, req: &JudgeRequest) -> Result<(), JevError> {
        check::prepare(&self.config, self.limit_bytes(), req.payload().bytes()).map(|_| ())
    }

    fn judge(&self, req: &JudgeRequest) -> Result<JudgeOutcome, JevError> {
        let prepared = check::prepare(&self.config, self.limit_bytes(), req.payload().bytes())?;
        let prepared = self.apply_wire_fault(prepared, req);
        // REQ-SAG-0606: the digest is recovered from the final serialized
        // body, before it is handed to `send_with_busy_retry`. A body that
        // cannot be parsed fails here and nothing is sent.
        let digest = check::recovered_digest(&prepared.body)?;
        Ok(self.execute(prepared, req)?.with_input_sha256(digest))
    }
}

impl<T: Transport> fmt::Debug for JevClient<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JevClient")
            .field("config", &self.config)
            .field("api_key", &self.api_key)
            .field("policy", self.http.busy_policy())
            .finish_non_exhaustive()
    }
}
