//! `JevClient` as a [`JudgeBackend`].
//!
//! The transport, the resolved API key, the busy-retry policy and the cancel
//! token are construction state. `judge` takes none of them (ADR-SAG-0006).
//! HTTP, busy retry, the response cap and redaction are `sc-agent-http`
//! (ADR-SAG-0026); this module builds the request and reads the reply.

use std::fmt;
use std::sync::Arc;

use sc_agent_http::{
    BusyPolicy, HeaderName, HttpClient, HttpRequest, Method, ReplyShape, SecretHeader, Timeouts,
    Transport, UreqTransport, api_failure,
};
use sc_agent_types::observability::v2::{GeneralEvent, NoopGeneralLog, SharedGeneralLog};
use sc_agent_types::{BackendId, CancelToken, JevError, ModelId, ResolvedApiKey, SendTrace};
use sc_decision_schema::{JudgeBackend, JudgeOutcome, JudgeRequest};

#[cfg(feature = "test-util")]
use crate::check::WireFault;
use crate::check::{self, PreparedCheck};
use crate::config::JevClientConfig;

/// Logging target of this module.
const TARGET: &str = "jev.client";

const CONTENT_TYPE: HeaderName = HeaderName::new("Content-Type");

/// TypeSafe check/judge client.
///
/// Posts the documented Jev body (`{model, state, questions}`, built from
/// `JudgeRequest::payload` by `sc_decision_schema::jev::v1::CheckRequest`)
/// with the pinned model, as `Authorization: Bearer` over `T` (by default
/// [`UreqTransport`]). Ignores `JudgeRequest::llm`. Busy replies (429, 502,
/// 529) are retried with the same body under
/// [`BusyPolicy`] (REQ-SAG-0505): `Retry-After` honoured up to 30 s,
/// otherwise 1, 2, 4, 8 s backoff, plus jitter, within a 90 s budget.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use sc_agent_http::{HttpRequest, HttpResponse, Transport};
/// use sc_agent_types::{BackendKeyId, JevError, ResolvedApiKey, SendTrace};
/// use sc_decision_schema::{CanonicalJudgePayload, JudgeBackend, JudgeRequest, PayloadBundle};
/// use sc_jev::JevClient;
///
/// struct Scripted;
///
/// impl Transport for Scripted {
///     fn send(&self, request: &HttpRequest) -> Result<HttpResponse, JevError> {
///         assert!(request.body().starts_with(r#"{"model":"jev-1.13.0","state":[],"questions":{"q":"#));
///         assert!(request.secret().expect("bearer").matches("test-key"));
///         Ok(HttpResponse::new(
///             200,
///             Vec::new(),
///             br#"{"model":"jev-1.13.0","answers":{"q":{"type":"choice","choice":"yes","confidence":1,"probabilities":{"no":0,"yes":1}}},"usage":{}}"#.to_vec(),
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
/// let outcome = client.judge(&JudgeRequest::new(Arc::new(bundle), None), &SendTrace::new())?;
/// assert_eq!(outcome.answers()["q"].choice().as_str(), "yes");
/// # Ok::<(), JevError>(())
/// ```
pub struct JevClient<T = UreqTransport> {
    config: JevClientConfig,
    api_key: ResolvedApiKey,
    http: HttpClient<T>,
    log: SharedGeneralLog,
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

#[cfg(feature = "test-util")]
impl JevClient<UreqTransport> {
    /// A client for `http://127.0.0.1:<port>` over a real loopback HTTP
    /// transport, making at most `sends` sends (feature `test-util`).
    ///
    /// For the binary's loopback test seam only. Production builds have no
    /// such constructor.
    #[must_use]
    pub fn loopback(api_key: ResolvedApiKey, port: u16, sends: u32) -> Self {
        let config = JevClientConfig::packaged().with_loopback_port(port);
        let transport = UreqTransport::loopback_for_tests(Timeouts::default());
        Self::with_config(config, api_key, transport).with_busy_policy(BusyPolicy::new(sends))
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
            log: Arc::new(NoopGeneralLog),
            #[cfg(feature = "test-util")]
            wire_fault: None,
        }
    }

    /// Seeds `fault` into the wire builder (feature `test-util`).
    ///
    /// The fault is applied after the body is built and before the reported `input_sha256` is recovered from the body, so a
    /// test shows the digest follows the bytes that would be sent
    /// (REQ-SAG-0606). Production builds leave the feature off.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn with_wire_fault(mut self, fault: WireFault) -> Self {
        self.wire_fault = Some(fault);
        self
    }

    /// Sets the evaluator model (the catalog default unless the factory
    /// applies a `check.toml` `[models]` override).
    #[must_use]
    pub fn with_model(mut self, model: ModelId) -> Self {
        self.config = self.config.with_model(model);
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

    /// Injects the general log: the client's own events and, through the
    /// HTTP layer, its busy-retry events. Unset, nothing is logged.
    #[must_use]
    pub fn with_general_log(mut self, log: SharedGeneralLog) -> Self {
        self.http = self.http.with_general_log(Arc::clone(&log));
        self.log = log;
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
    fn apply_wire_fault(&self, prepared: PreparedCheck) -> PreparedCheck {
        let Some(fault) = self.wire_fault else {
            return prepared;
        };
        PreparedCheck {
            body: fault.apply(prepared.body),
            request: prepared.request,
        }
    }

    #[cfg(not(feature = "test-util"))]
    fn apply_wire_fault(&self, prepared: PreparedCheck) -> PreparedCheck {
        prepared
    }

    fn execute(
        &self,
        prepared: PreparedCheck,
        judged: &JudgeRequest,
        trace: &SendTrace,
    ) -> Result<JudgeOutcome, JevError> {
        let PreparedCheck {
            body,
            request: check,
        } = prepared;
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
            .send_and_parse(&request, trace.clone(), |response| {
                if response.status() != 200 {
                    return Err(api_failure("Jev", ReplyShape::Jev, &request, &response));
                }
                check::parse_outcome(response.body(), &check, judged)
            })
            .map_err(|error| error.prefix_detail("Jev"))
    }
}

impl<T: Transport> JudgeBackend for JevClient<T> {
    fn id(&self) -> BackendId {
        BackendId::Jev
    }

    fn validate_for_send(&self, req: &JudgeRequest) -> Result<(), JevError> {
        check::prepare(&self.config, self.limit_bytes(), req.payload()).map(|_| ())
    }

    fn model_id(&self) -> ModelId {
        self.config.model().clone()
    }

    fn judge(&self, req: &JudgeRequest, trace: &SendTrace) -> Result<JudgeOutcome, JevError> {
        let model = self.config.model().to_string();
        GeneralEvent::debug(TARGET, "judge.start", "Jev judge started")
            .with_field("model", model.as_str())
            .with_field("endpoint", self.config.endpoint().as_str())
            .emit(self.log.as_ref());
        let result = self.judge_inner(req, trace);
        match &result {
            Ok(_) => GeneralEvent::debug(TARGET, "judge.done", "Jev judge answered")
                .with_field("model", model.as_str())
                .emit(self.log.as_ref()),
            Err(error) => GeneralEvent::warn(TARGET, "judge.failed", "Jev judge failed")
                .with_field("model", model.as_str())
                .with_error_code(error.code().as_str())
                .emit(self.log.as_ref()),
        }
        result
    }
}

impl<T: Transport> JevClient<T> {
    fn judge_inner(&self, req: &JudgeRequest, trace: &SendTrace) -> Result<JudgeOutcome, JevError> {
        let prepared = check::prepare(&self.config, self.limit_bytes(), req.payload())?;
        let prepared = self.apply_wire_fault(prepared);
        // REQ-SAG-0606: the digest is recovered from the final serialized
        // body, before it is handed to `send_with_busy_retry`. A body that
        // cannot be parsed fails here and nothing is sent.
        let digest = check::recovered_digest(&prepared.body)?;
        Ok(self
            .execute(prepared, req, trace)?
            .with_input_sha256(digest))
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
