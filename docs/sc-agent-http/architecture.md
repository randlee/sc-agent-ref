# sc-agent-http — architecture

**Boundary:** `agent-http` (`boundaries/sc-agent-http/agent-http.toml`)
**Depends on:** `sc-agent-types` (`default-features = false`,
`features = ["secret"]`), `serde_json` (only in `failure`, ADR-SAG-0026
agt-2a3 amendment), `ureq`
**Dependents:** `sc-jev`, `sc-jev-backends`
**Governing ADRs:** ADR-SAG-0026 (HTTP half), ADR-SAG-0006 / 0007 / 0017 /
0020 r-3 amendments, ADR-SAG-0012 (unchanged: `sc-jev` owns every Jev API
operation; this crate carries bytes and, in `failure`, reads only a failing
reply's error fields: ADR-SAG-0026 agt-2a3 amendment).

## Modules

All modules are private; the crate root re-exports the public items, plus the
`test_util` module under `test-util`.

| Module | Public items | Role |
| --- | --- | --- |
| `request` | `HttpRequest`, `HttpResponse`, `Method`, `HeaderName`, `SecretHeader`, `Endpoint` | Values with private fields, constructors and accessors. `HttpRequest::new(method, endpoint, body: Arc<str>)`, `with_header`, `with_secret`; `HttpResponse::new(status, headers, body)`, `status`, `header` (case-insensitive), `body`, `into_body`. `HeaderName` is a `&'static str` RFC 9110 token (`const fn new`). `Endpoint::parse` accepts only `https://` URLs |
| `transport` | `Transport`, `Timeouts` | The open transport trait; connect and request deadlines (10 s / 120 s default) |
| `ureq_transport` | `UreqTransport` | The production transport and the single secret read site |
| `retry` | `BusyPolicy`, `send_with_busy_retry`, `send_and_parse`, `HttpClient` | Busy retry with the decided numbers and the cancel hook; the one exchange (send, retry, parse, redact) and the transport + policy + token state every backend holds |
| `retry_after` | (crate-private) | The one `Retry-After` parser: a wait view (clamped 0–30 s) for the retry loop and a report view (whole seconds, rounded up, capped at a day) for error fields |
| `failure` | `api_failure` | Maps a reply with a failing status to its error id, keeping the provider's status, type, code, message, parameter and request id (bounded); also the quota hook of the busy retry |
| `redact` | `redact` | Removes the request's secret from every text field of an error |
| `test_util` (`test-util`) | `FakeClock`, `RecordingSleeper`, `ScriptedTransport`, `RecordingTransport` | Doubles for this crate's and the backend crates' tests |

## `Transport` contract

`Transport::send(&self, &HttpRequest) -> Result<HttpResponse, JevError>` sends
once. Every HTTP status is `Ok`; connection, send, read and timeout failures
are `NETWORK_ERROR` with a `NetworkErrorKind` (`timeout`, `connect`, `tls`,
`cancelled`); a body over the cap is `RESPONSE_INVALID` with cause
`too_large`. The trait is open (ADR-SAG-0006) and object-safe, with
blanket impls for `&T` and `Arc<T>`. A direct `send` caller bypasses busy
retry **and** redaction; production code calls only `send_and_parse` (which wraps `send_with_busy_retry`).
A transport other than `UreqTransport` cannot read the key: it sees
`SecretHeader::name` and `redacted` only, plus, under `test-util`, the
`matches` equality oracle for test transports.

`UreqTransport::new(Timeouts)` builds one ureq agent from one private
`AgentConfig` value (connect timeout, global request timeout covering the
body read, `max_redirects = 0`, https only, `User-Agent: sc-agent-http/<ver>`,
statuses not errors) and reuses it for every send and every clone. The body
is read through a `take(8 MiB + 1)` reader, so the cap holds while streaming
and for error statuses. No ureq type is in any public signature; URL
validation uses ureq's re-exported `http::Uri` inside the private module.

## Secret read site (ADR-SAG-0020 r-3)

A credential travels only as a `SecretHeader` (`new(name, key)` or
`bearer(key)`) inside `HttpRequest`. `ureq_transport::expose` is the one call
of `ResolvedApiKey::as_str` in the workspace and carries
`#[expect(clippy::disallowed_methods, reason = "single secret read site (ADR-SAG-0020)")]`;
`SecretHeader::matches` (`test-util` only) is implemented in the same file
through it. Root
`Cargo.toml` denies `allow_attributes` and `allow_attributes_without_reason`,
so a second site needs a visible `#[expect]`. This is a lint-enforced
workspace convention, not a type seal: crates outside the workspace are not
constrained.

## Redaction rule

Backends call `send_and_parse(transport, &request, &policy, &cancel, parse)`
(or `HttpClient::send_and_parse(&request, parse)`), which runs
`send_with_busy_retry` and then `parse(response)`, and redacts `parse`'s
error against the same request (RBP-F007). The backend's non-200 mapping and
reply parser live inside `parse`, so no call site redacts by hand; the
backend adds only its provider label (`Jev …`, `haiku …`) afterwards.

`send_with_busy_retry` passes every `Err` it returns — the busy and cancel
errors it builds and every error a transport returns, third-party transports
included, whatever the variant — through `redact` exactly once.
`redact(error, &request)` uses `ResolvedApiKey::redact_error`, which maps
every text field, so a header echo or body excerpt containing the key becomes
`[redacted]`.

## Busy retry and cancel latency

`BusyPolicy::default()` encodes owner decision (b) (REQ-SAG-0505 amended,
ADR-SAG-0007 r-3 and its phase-end amendment): 429/502/529 only; 5 sends; a valid Retry-After clamped to
0–30 s; absent or malformed → 1, 2, 4, 8 s; up to 250 ms
jitter (std `RandomState` hashed with the current time); a 90 s budget from
the first send, every wait `min(wait + jitter, remaining)`, no attempt
starts once it is spent. Exhausting the sends gives detail
"HTTP <s> busy; N sends exhausted"; exhausting the budget gives
"HTTP <s> busy after N sends; retry budget exhausted". Both are
`PROVIDER_BUSY` (`retryable() == true`) and carry machine fields: `cause`
(`sends` or `budget`), `attempts` (N), the last busy `http_status`, and
`retry_after_seconds`, the last valid `Retry-After` in whole seconds (rounded
up, capped at one day, unclamped by the 30 s wait clamp; absent when none was
valid). Callers never match on text.

**Quota is not busy.** A busy status (429, 502, 529) whose body marks quota,
credit or spend-limit exhaustion (the documented Anthropic and OpenAI
shapes in `failure`, e.g. `insufficient_quota`, `credit_balance_exhausted`,
`enforced_spend_limit_reached`) is not resent: the retry loop returns it on
the first send, and the caller maps it with `api_failure` to
`QUOTA_EXHAUSTED`. An unknown 429 body stays busy. The hook is
`is_quota_reply` (called from `is_busy` in `retry`) and does not touch retry
timing.

**Failing statuses.** `api_failure(provider, shape, &response)` is the one mapping
for a non-200 reply: 401 and 403 are `AUTH_FAILED`; 402 or a quota body is
`QUOTA_EXHAUSTED`; a 413, or a 400/422 with a structured marker
(`context_length_exceeded`, `request_too_large`), or for Jev only
(`ReplyShape::Jev`) a message naming `max_tokens` or tokens exceeded, is
`INPUT_TOO_LARGE`; a Haiku or Luna message about `max_tokens` (the output cap)
is an ordinary invalid request; other 400 and 422 are `API_REQUEST_INVALID`; every
other status is `API_ERROR` (retryable from 500 up). Provider text is
single-line and bounded to 1024 bytes per field, and leaves through `redact`
(ADR-SAG-0020 amendment).

The `CancelToken` is checked before every attempt and before each wait slice
(≤ 100 ms), so a cancel during a wait returns `Cancelled` within 100 ms. A
cancel during an in-flight request is observed only when the request returns:
the worst-case cancel latency is the request timeout (120 s by default).
Errors are never retried, so a connect or DNS failure returns
`NETWORK_ERROR` kind `Connect` at once and the caller decides; there is no
connectivity probing.

## `test-util` seam

Non-default, additive, enabled only by dev-dependencies and
`--features test-util` (plan "Test seams"; D0 outcome 4: no self
dev-dependency, the `retry` and `ureq` targets declare
`[[test]] required-features = ["test-util"]`). It adds:

- `BusyPolicy::with_test_hooks(FakeClock, RecordingSleeper, jitter)`: a fake
  monotonic clock, a sleeper that records waits and slices and advances the
  clock (optionally cancelling a token after a set time), and a fixed jitter.
  There is no public sleeper trait in a normal build.
- `Endpoint::loopback_http(port, path)`: only `http://127.0.0.1:<port>/…`, so
  even a mistaken build cannot send plain HTTP off the machine.
- `UreqTransport::loopback_for_tests(Timeouts)`: the production
  `AgentConfig` with https-only off.
- `test_util::{ScriptedTransport, RecordingTransport}`.

## `SendTrace` parameter (r-7, ADR-SAG-0026 r-7)

`send_with_busy_retry`, `send_and_parse` and `HttpClient::send_and_parse` take
a required `SendTrace` (from `sc-agent-types`), so no backend can send without
one. `HttpRequest`'s body is text (`Arc<str>`, `body() -> &str`); transports
send `body().as_bytes()`. `send_with_busy_retry` records
`Sha256Digest::of_text(request.body())` into the trace once, immediately
before the first `Transport::send` (r-6, REQ-SAG-0613): retries resend the
same bytes, so a send has one value; a cancel before the first attempt
records nothing; headers are not hashed, so no secret is. `trace.rs` pins this
(`--features test-util --test trace`).
