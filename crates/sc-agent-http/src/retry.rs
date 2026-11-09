//! Busy retry (owner decision (b), 2026-11-06; ADR-SAG-0007 r-3 amendment;
//! REQ-SAG-0505).
//!
//! [`BusyPolicy::default`] encodes the decided numbers:
//!
//! - Busy statuses are 429, 502 and 529, and only those. The same body is
//!   resent. At most 5 sends. A reply whose body says the account is out of
//!   quota (documented Anthropic and OpenAI shapes, see
//!   [`api_failure`](crate::api_failure)) is not busy: no wait helps, so it
//!   is returned at once for the caller to map to `QUOTA_EXHAUSTED`. An
//!   unknown 429 body stays busy.
//! - A valid `Retry-After` is honoured, clamped to 0–30 s. Valid is 1–32
//!   bytes matching `^[0-9]+(\.[0-9]+)?$`. The digits are parsed as integers,
//!   so sign, exponent, `inf`, `NaN` and overflow cannot occur; a long
//!   integer part saturates to the 30 s clamp. Anything else (an HTTP-date,
//!   `-1`, `1e9`, empty, more than 32 bytes) is malformed.
//! - An absent **or malformed** `Retry-After` (treated alike) backs off
//!   exponentially: 1, 2, 4, 8 … s for the 1st, 2nd, 3rd, 4th … wait, capped
//!   at 30 s. With 5 sends there are at most four waits (1, 2, 4, 8 s, 15 s
//!   in all before jitter).
//! - Every wait adds up to 250 ms of jitter.
//! - A 90 s budget, from the first send, governs starting attempts. Every
//!   wait is `min(wait + jitter, budget remaining)`; when no budget remains
//!   no further attempt starts. An attempt in flight keeps its transport
//!   request timeout (120 s by default).
//! - The [`CancelToken`] is checked before each attempt and between wait
//!   slices of at most 100 ms.
//! - Errors are never retried: a connect or DNS failure, a timeout, or a
//!   failure after the body was sent returns at once. A provider may already
//!   have processed (and billed) a request whose reply was lost, so resending
//!   it could double-charge; only busy statuses, which say the request was
//!   not processed, are resent. (A 502 can still double-charge a provider
//!   that processed the request; ADR-SAG-0007 r-3 records that.) There is no
//!   connectivity probing; the caller decides whether to retry.

use std::hash::{BuildHasher, RandomState};
use std::thread;
use std::time::{Duration, Instant};

use sc_agent_types::observability::v2::{GeneralEvent, SharedGeneralLog};
use sc_agent_types::{
    BusyCause, CancelToken, JevError, NetworkErrorKind, RequestSha256, SendTrace,
};

use crate::failure::{busy_reply, is_quota_reply};
use crate::redact::redact;
use crate::request::{HttpRequest, HttpResponse};
use crate::retry_after::RetryAfter;
use crate::transport::Transport;

#[cfg(feature = "test-util")]
use crate::test_util::{FakeClock, RecordingSleeper};

/// HTTP statuses that mean "busy, resend the same body".
const BUSY_STATUSES: [u16; 3] = [429, 502, 529];

/// Sends per call under [`BusyPolicy::default`], including the first.
const DEFAULT_MAX_SENDS: u32 = 5;

/// Most sends per call under any policy (RSH-002). [`BusyPolicy::new`] and
/// [`BusyPolicy::with_max_sends`] clamp to `1..=MAX_SENDS`.
const MAX_SENDS: u32 = 5;

/// First backoff wait when `Retry-After` is absent or malformed.
const BACKOFF_FIRST: Duration = Duration::from_secs(1);

/// Cap of the exponential backoff.
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Largest jitter added to a wait.
const JITTER_MAX: Duration = Duration::from_millis(250);

/// Budget for starting attempts, measured from the first send.
const BUDGET: Duration = Duration::from_secs(90);

/// Longest uninterrupted sleep, so a cancel is seen within this time.
const WAIT_SLICE: Duration = Duration::from_millis(100);

/// Logging target of this module.
const TARGET: &str = "http.retry";

/// The optional general log of a policy. Not part of the policy's identity.
#[derive(Clone, Default)]
struct LogHandle(Option<SharedGeneralLog>);

impl std::fmt::Debug for LogHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(if self.0.is_some() {
            "GeneralLog(set)"
        } else {
            "GeneralLog(none)"
        })
    }
}

impl LogHandle {
    /// Sends `event` to the injected log; no log means discard.
    fn emit(&self, event: GeneralEvent) {
        if let Some(log) = &self.0 {
            event.emit(log.as_ref());
        }
    }
}

/// How busy replies are retried.
///
/// Private fields. [`BusyPolicy::default`] is the decided policy (module
/// docs); [`BusyPolicy::new`] changes only the number of sends, which is
/// clamped to 1 through 5 so no backend can exceed the decided bound. Sleeping,
/// the monotonic clock and jitter are part of the policy: production uses
/// the thread clock, `std::thread::sleep` and std-seeded jitter. With the
/// `test-util` feature, `BusyPolicy::with_test_hooks` replaces all three so
/// tests never sleep.
///
/// # Examples
///
/// ```
/// use sc_agent_http::BusyPolicy;
///
/// assert_eq!(BusyPolicy::default().max_sends(), 5);
/// assert_eq!(BusyPolicy::new(0).max_sends(), 1);
/// assert_eq!(BusyPolicy::new(99).max_sends(), 5);
/// ```
#[derive(Debug, Clone)]
pub struct BusyPolicy {
    max_sends: u32,
    hooks: Hooks,
    log: LogHandle,
}

impl BusyPolicy {
    /// The default policy with `max_sends` total sends, clamped to 1 through
    /// 5.
    #[must_use]
    pub fn new(max_sends: u32) -> Self {
        Self {
            max_sends: max_sends.clamp(1, MAX_SENDS),
            hooks: Hooks::System,
            log: LogHandle::default(),
        }
    }

    /// Sends this policy's retry and busy-wait events to `log`.
    ///
    /// Without a log the policy records nothing. The events carry statuses,
    /// counts and wait times; never a body, header or key.
    #[must_use]
    pub fn with_general_log(mut self, log: SharedGeneralLog) -> Self {
        self.log = LogHandle(Some(log));
        self
    }

    /// Replaces the number of sends (clamped to 1 through 5), keeping the
    /// hooks.
    #[must_use]
    pub fn with_max_sends(mut self, max_sends: u32) -> Self {
        self.max_sends = max_sends.clamp(1, MAX_SENDS);
        self
    }

    /// Total sends per call, including the first.
    #[must_use]
    pub fn max_sends(&self) -> u32 {
        self.max_sends
    }

    /// The default numbers with a fake clock, a recording sleeper and a fixed
    /// jitter (clamped to 250 ms). Nothing sleeps.
    ///
    /// `sleeper` advances `clock` by each slice it records.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn with_test_hooks(clock: FakeClock, sleeper: RecordingSleeper, jitter: Duration) -> Self {
        Self {
            max_sends: DEFAULT_MAX_SENDS,
            hooks: Hooks::Test {
                clock,
                sleeper,
                jitter: jitter.min(JITTER_MAX),
            },
            log: LogHandle::default(),
        }
    }
}

impl Default for BusyPolicy {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SENDS)
    }
}

/// Clock, sleeper and jitter source of a policy.
#[derive(Debug, Clone)]
enum Hooks {
    System,
    #[cfg(feature = "test-util")]
    Test {
        clock: FakeClock,
        sleeper: RecordingSleeper,
        jitter: Duration,
    },
}

/// Start of one retry loop on the policy's clock.
#[derive(Debug, Clone, Copy)]
enum Started {
    System(Instant),
    #[cfg(feature = "test-util")]
    Fake(Duration),
}

impl Hooks {
    fn start(&self) -> Started {
        match self {
            Self::System => Started::System(Instant::now()),
            #[cfg(feature = "test-util")]
            Self::Test { clock, .. } => Started::Fake(clock.now()),
        }
    }

    fn elapsed(&self, started: Started) -> Duration {
        match (self, started) {
            (Self::System, Started::System(at)) => at.elapsed(),
            #[cfg(feature = "test-util")]
            (Self::Test { clock, .. }, Started::Fake(at)) => clock.now().saturating_sub(at),
            #[cfg(feature = "test-util")]
            _ => unreachable!("a loop's start always comes from its own policy"),
        }
    }

    /// Jitter in `0..=JITTER_MAX`.
    ///
    /// Production hashes the current time with a fresh std `RandomState`:
    /// enough to de-synchronise clients, not a cryptographic source.
    fn jitter(&self) -> Duration {
        match self {
            Self::System => {
                let max = u64::try_from(JITTER_MAX.as_millis()).unwrap_or(u64::MAX);
                let hash = RandomState::new().hash_one(Instant::now());
                Duration::from_millis(hash % (max + 1))
            }
            #[cfg(feature = "test-util")]
            Self::Test { jitter, .. } => *jitter,
        }
    }

    /// Waits `total` in slices of at most [`WAIT_SLICE`], checking `cancel`
    /// before each slice.
    fn wait(&self, total: Duration, cancel: &CancelToken) -> Result<(), JevError> {
        #[cfg(feature = "test-util")]
        if let Self::Test { sleeper, .. } = self {
            sleeper.record_wait(total);
        }
        let mut left = total;
        while !left.is_zero() {
            if cancel.is_cancelled() {
                return Err(cancelled("during a busy-retry wait"));
            }
            let slice = left.min(WAIT_SLICE);
            match self {
                Self::System => thread::sleep(slice),
                #[cfg(feature = "test-util")]
                Self::Test { sleeper, .. } => sleeper.sleep(slice),
            }
            left -= slice;
        }
        Ok(())
    }
}

/// Sends `request`, retrying busy statuses under `policy`.
///
/// Returns the first non-busy response, whatever its status; the caller maps
/// non-2xx statuses. Every `Err` this function returns, including errors the
/// transport returned, has the request's secret removed by
/// [`redact`](crate::redact()) exactly once, here.
///
/// `trace` is this send's [`SendTrace`]: required, so no backend can send
/// without one. The digest of the exact body ([`HttpRequest::body`]) is
/// recorded into it once, immediately before the first [`Transport::send`]
/// (REQ-SAG-0613): retries resend the same bytes, so a send has one value,
/// and a cancel before the first attempt records nothing. Headers are not
/// hashed, so no secret is.
///
/// **Cancel latency.** `cancel` is checked before every attempt and between
/// wait slices of at most 100 ms, so a cancel during a wait returns within
/// 100 ms. A cancel during an in-flight request is observed only when that
/// request returns: the worst case is the transport's request timeout
/// (120 s for [`UreqTransport`](crate::UreqTransport) by default).
///
/// # Errors
///
/// - `NETWORK_ERROR` kind `Cancelled` when `cancel` is set before an attempt
///   or during a wait.
/// - `PROVIDER_BUSY` (`retryable() == true`) when the last allowed send was
///   busy (cause `sends`) or when the 90 s budget leaves no time for another
///   attempt (cause `budget`). It carries the last busy status, the number
///   of sends and the provider's `Retry-After` in seconds when it sent a
///   valid one.
/// - Any error of [`Transport::send`], unchanged except for redaction, and
///   never retried. A connect or DNS failure is `NETWORK_ERROR` kind
///   `Connect` (`retryable() == true`) and returns at once.
pub fn send_with_busy_retry(
    transport: &dyn Transport,
    request: &HttpRequest,
    policy: &BusyPolicy,
    cancel: &CancelToken,
    trace: SendTrace,
) -> Result<HttpResponse, JevError> {
    retry_loop(transport, request, policy, cancel, &trace).map_err(|error| redact(error, request))
}

/// Sends `request` like [`send_with_busy_retry`], then runs `parse` on the
/// response, redacting `parse`'s error too.
///
/// This is the one exchange every backend uses: transport, retry and
/// response-mapping errors all leave through [`redact`](crate::redact())
/// against `request`, so a call site cannot forget to redact a reply-parse
/// error that echoes the key. `parse` sees every non-busy response, whatever
/// its status, and maps non-2xx statuses itself.
///
/// # Errors
///
/// Every error of [`send_with_busy_retry`], and every error `parse`
/// returns, with the request's secret removed.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use sc_agent_http::{
///     BusyPolicy, Endpoint, HttpRequest, HttpResponse, Method, Transport, send_and_parse,
/// };
/// use sc_agent_types::{CancelToken, JevError, SendTrace};
///
/// struct Teapot;
///
/// impl Transport for Teapot {
///     fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, JevError> {
///         Ok(HttpResponse::new(418, Vec::new(), Vec::new()))
///     }
/// }
///
/// let request = HttpRequest::new(
///     Method::Post,
///     Endpoint::parse("https://api.example.test/v1/x")?,
///     Arc::from("{}"),
/// );
/// let status = send_and_parse(
///     &Teapot,
///     &request,
///     &BusyPolicy::default(),
///     &CancelToken::new(),
///     SendTrace::new(),
///     |response| Ok(response.status()),
/// )?;
/// assert_eq!(status, 418);
/// # Ok::<(), JevError>(())
/// ```
pub fn send_and_parse<R>(
    transport: &dyn Transport,
    request: &HttpRequest,
    policy: &BusyPolicy,
    cancel: &CancelToken,
    trace: SendTrace,
    parse: impl FnOnce(HttpResponse) -> Result<R, JevError>,
) -> Result<R, JevError> {
    let response = send_with_busy_retry(transport, request, policy, cancel, trace)?;
    parse(response).map_err(|error| redact(error, request))
}

/// A transport with its busy policy and cancel token: the HTTP state every
/// backend holds.
///
/// Backends keep one `HttpClient` and forward their `with_busy_policy` /
/// `with_cancel` builders to it, so the state and the exchange
/// ([`HttpClient::send_and_parse`]) are written once.
///
/// # Examples
///
/// ```
/// use sc_agent_http::{BusyPolicy, HttpClient, Timeouts, UreqTransport};
/// use sc_agent_types::CancelToken;
///
/// let client = HttpClient::new(UreqTransport::new(Timeouts::default()))
///     .with_busy_policy(BusyPolicy::new(2))
///     .with_cancel(CancelToken::new());
/// assert_eq!(client.busy_policy().max_sends(), 2);
/// ```
#[derive(Debug, Clone)]
pub struct HttpClient<T> {
    transport: T,
    policy: BusyPolicy,
    cancel: CancelToken,
}

impl<T: Transport> HttpClient<T> {
    /// `transport` with [`BusyPolicy::default`] and a token that is never
    /// cancelled.
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            policy: BusyPolicy::default(),
            cancel: CancelToken::new(),
        }
    }

    /// Replaces the busy policy. A log already set on this client stays
    /// unless `policy` carries its own.
    #[must_use]
    pub fn with_busy_policy(mut self, mut policy: BusyPolicy) -> Self {
        if policy.log.0.is_none() {
            policy.log = self.policy.log.clone();
        }
        self.policy = policy;
        self
    }

    /// Sends the busy-retry events of every exchange to `log`.
    #[must_use]
    pub fn with_general_log(mut self, log: SharedGeneralLog) -> Self {
        self.policy = self.policy.with_general_log(log);
        self
    }

    /// Replaces the cancel token.
    #[must_use]
    pub fn with_cancel(mut self, cancel: CancelToken) -> Self {
        self.cancel = cancel;
        self
    }

    /// The busy policy.
    #[must_use]
    pub fn busy_policy(&self) -> &BusyPolicy {
        &self.policy
    }

    /// The transport.
    #[must_use]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// [`send_and_parse`] with this client's transport, policy and token.
    ///
    /// # Errors
    ///
    /// As [`send_and_parse`].
    pub fn send_and_parse<R>(
        &self,
        request: &HttpRequest,
        trace: SendTrace,
        parse: impl FnOnce(HttpResponse) -> Result<R, JevError>,
    ) -> Result<R, JevError> {
        send_and_parse(
            &self.transport,
            request,
            &self.policy,
            &self.cancel,
            trace,
            parse,
        )
    }
}

fn retry_loop(
    transport: &dyn Transport,
    request: &HttpRequest,
    policy: &BusyPolicy,
    cancel: &CancelToken,
    trace: &SendTrace,
) -> Result<HttpResponse, JevError> {
    let hooks = &policy.hooks;
    let log = &policy.log;
    let started = hooks.start();
    let mut sends = 0_u32;
    loop {
        if cancel.is_cancelled() {
            log.emit(
                GeneralEvent::info(TARGET, "http.cancelled", "request cancelled before a send")
                    .with_field("sends", sends),
            );
            return Err(cancelled("before a send"));
        }
        if sends == 0 {
            // One value per send: every retry resends these bytes.
            trace.record(RequestSha256::of_text(request.body()));
        }
        sends += 1;
        log.emit(
            GeneralEvent::debug(TARGET, "http.send", "sending request")
                .with_field("method", request.method().as_str())
                .with_field("endpoint", request.endpoint().log_form())
                .with_field("attempt", sends)
                .with_field("max_sends", policy.max_sends),
        );
        let response = match transport.send(request) {
            Ok(response) => response,
            Err(error) => {
                log.emit(
                    GeneralEvent::warn(TARGET, "http.transport_failed", "transport failed")
                        .with_error_code(error.code().as_str())
                        .with_field("attempt", sends),
                );
                return Err(error);
            }
        };
        let status = response.status();
        if !is_busy(&response) {
            log.emit(
                GeneralEvent::debug(TARGET, "http.response", "response received")
                    .with_field("status", status)
                    .with_field("attempt", sends),
            );
            return Ok(response);
        }
        let retry_after = RetryAfter::parse(response.header("retry-after"));
        let reported_wait = retry_after.map(RetryAfter::report_seconds);
        if sends >= policy.max_sends {
            log.emit(
                GeneralEvent::warn(TARGET, "http.busy_exhausted", "busy retries exhausted")
                    .with_field("status", status)
                    .with_field("sends", sends),
            );
            return Err(JevError::provider_busy(
                format!("HTTP {status} busy; {sends} {} exhausted", plural(sends)),
                status,
                sends,
                BusyCause::Sends,
                reported_wait,
            )
            .with_busy_reply(busy_reply(request, &response)));
        }
        let remaining = BUDGET.saturating_sub(hooks.elapsed(started));
        if remaining.is_zero() {
            log.emit(
                GeneralEvent::warn(
                    TARGET,
                    "http.budget_exhausted",
                    "busy retry budget exhausted",
                )
                .with_field("status", status)
                .with_field("sends", sends),
            );
            return Err(JevError::provider_busy(
                format!(
                    "HTTP {status} busy after {sends} {}; retry budget exhausted",
                    plural(sends)
                ),
                status,
                sends,
                BusyCause::Budget,
                reported_wait,
            )
            .with_busy_reply(busy_reply(request, &response)));
        }
        let base = retry_after.map_or_else(|| backoff(sends), RetryAfter::wait);
        let wait = base.saturating_add(hooks.jitter()).min(remaining);
        log.emit(
            GeneralEvent::warn(
                TARGET,
                "http.busy_wait",
                "busy reply; waiting before a resend",
            )
            .with_field("status", status)
            .with_field("attempt", sends)
            .with_field(
                "wait_ms",
                u64::try_from(wait.as_millis()).unwrap_or(u64::MAX),
            )
            .with_field("retry_after_honoured", retry_after.is_some()),
        );
        if let Err(error) = hooks.wait(wait, cancel) {
            log.emit(
                GeneralEvent::info(
                    TARGET,
                    "http.cancelled",
                    "request cancelled during a busy wait",
                )
                .with_field("sends", sends),
            );
            return Err(error);
        }
    }
}

/// Whether `response` is a busy reply that is worth resending.
///
/// The classification hook: a busy status ([`BUSY_STATUSES`]) whose body
/// says the account is out of quota is not busy, because waiting cannot
/// help. Retry timing is unaffected.
fn is_busy(response: &HttpResponse) -> bool {
    BUSY_STATUSES.contains(&response.status())
        && !is_quota_reply(response.status(), response.body())
}

fn plural(sends: u32) -> &'static str {
    if sends == 1 { "send" } else { "sends" }
}

fn cancelled(when: &str) -> JevError {
    JevError::network(
        NetworkErrorKind::Cancelled,
        format!("request cancelled {when}"),
    )
}

/// Backoff before the `nth` wait (1-based): 1, 2, 4, 8 … s, capped at 30 s.
fn backoff(nth: u32) -> Duration {
    let factor = 1_u32.checked_shl(nth.saturating_sub(1)).unwrap_or(u32::MAX);
    BACKOFF_FIRST.saturating_mul(factor).min(BACKOFF_MAX)
}

/// The honoured wait of a valid `Retry-After`, clamped to 0–30 s (the one
/// parser, [`RetryAfter`]; `None` when absent or malformed). The table tests
/// below drive it.
#[cfg(test)]
fn parse_retry_after(header: Option<&str>) -> Option<Duration> {
    RetryAfter::parse(header).map(RetryAfter::wait)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retry_after::WAIT_MAX as RETRY_AFTER_MAX;

    #[test]
    fn backoff_doubles_to_the_cap() {
        let seconds: Vec<u64> = (1..=8).map(|nth| backoff(nth).as_secs()).collect();
        assert_eq!(seconds, [1, 2, 4, 8, 16, 30, 30, 30]);
        assert_eq!(backoff(u32::MAX), BACKOFF_MAX);
        assert_eq!(backoff(0), BACKOFF_FIRST);
    }

    #[test]
    fn retry_after_parser() {
        let cases: &[(Option<&str>, Option<Duration>)] = &[
            (None, None),
            (Some("3"), Some(Duration::from_secs(3))),
            (Some("2.5"), Some(Duration::from_millis(2_500))),
            (Some("0"), Some(Duration::ZERO)),
            (Some("0.000000001"), Some(Duration::from_nanos(1))),
            (Some("1.1234567899"), Some(Duration::new(1, 123_456_789))),
            (Some("29.999"), Some(Duration::from_millis(29_999))),
            (Some("30"), Some(RETRY_AFTER_MAX)),
            (Some("30.5"), Some(RETRY_AFTER_MAX)),
            (Some(&"9".repeat(32)), Some(RETRY_AFTER_MAX)),
            (Some(&"9".repeat(33)), None),
            (Some(""), None),
            (Some(" 7"), None),
            (Some("-1"), None),
            (Some("+1"), None),
            (Some("1e9"), None),
            (Some("inf"), None),
            (Some("NaN"), None),
            (Some("1."), None),
            (Some(".5"), None),
            (Some("1.2.3"), None),
            (Some("Wed, 21 Oct 2026 07:28:00 GMT"), None),
        ];
        for (header, expected) in cases {
            assert_eq!(parse_retry_after(*header), *expected, "{header:?}");
        }
    }

    #[test]
    fn system_jitter_stays_in_range() {
        for _ in 0..64 {
            assert!(Hooks::System.jitter() <= JITTER_MAX);
        }
    }

    #[test]
    fn sends_are_bounded_to_one_through_five() {
        assert_eq!(BusyPolicy::default().max_sends(), 5);
        let counts: Vec<u32> = [0, 1, 5, 6, u32::MAX]
            .into_iter()
            .map(|n| BusyPolicy::new(n).max_sends())
            .collect();
        assert_eq!(counts, [1, 1, 5, 5, 5]);
        assert_eq!(BusyPolicy::new(2).with_max_sends(99).max_sends(), 5);
        assert_eq!(BusyPolicy::new(2).with_max_sends(0).max_sends(), 1);
    }

    #[test]
    fn default_delays_are_one_two_four_eight() {
        let delays: Vec<u64> = (1..DEFAULT_MAX_SENDS)
            .map(|nth| backoff(nth).as_secs())
            .collect();
        assert_eq!(delays, [1, 2, 4, 8]);
        assert_eq!(delays.iter().sum::<u64>(), 15);
    }

    #[test]
    fn busy_set_is_exactly_three_statuses() {
        let busy: Vec<u16> = (100..600)
            .filter(|status| BUSY_STATUSES.contains(status))
            .collect();
        assert_eq!(busy, [429, 502, 529]);
    }
}
