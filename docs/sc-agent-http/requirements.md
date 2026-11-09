# sc-agent-http — requirements

**Sprint** = the sprint that implements the requirement. Root rows are in
[`../requirements.md`](../requirements.md); the A.1 closure table in
[`plans/phase-a-refine/plan.md`](../plans/phase-a-refine/plan.md) is
authoritative.

| ID | Requirement | Sprint | Proof |
| --- | --- | --- | --- |
| REQ-SAG-0402 | One shared HTTP layer for every backend family: `HttpRequest` / `HttpResponse`, `Transport`, `UreqTransport` (one agent, connect 10 s, request 120 s incl. body read, no redirects, https only, versioned `User-Agent`, 8 MiB streaming cap for every status), `Endpoint` (https only in every build), `SecretHeader`, `BusyPolicy` / `send_with_busy_retry`, `redact`. `sc-jev` and `sc-jev-backends` depend on it and not on ureq | r-3 | `--test ureq` (loopback), `--lib ureq_transport::tests::test_ureq_agent_config_values`, `--lib ureq_transport::tests::test_endpoint_parse_refuses_http`, `cargo tree --depth 1` of both backends |
| REQ-SAG-0505 (amended) | `BusyPolicy::default()`: 429/502/529 only; 5 sends, and no policy may exceed 5; valid Retry-After (1–32 bytes, `^[0-9]+(\.[0-9]+)?$`, parsed without floating point) honoured, clamped 0–30 s; absent or malformed → 1, 2, 4, 8 s; `Timeouts` clamps request to 1 ms–120 s and connect to 1 ms–10 s; ≤ 250 ms jitter; 90 s budget for starting attempts; waits sliced ≤ 100 ms with `CancelToken`; errors never retried; connect/DNS → `NETWORK_ERROR` kind `Connect`, `retryable() == true`; exhaustion is `PROVIDER_BUSY` with `cause` `sends` or `budget`, `attempts` and `retry_after_seconds` (REQ-SAG-0160); a busy status whose body marks quota is returned unretried (REQ-SAG-0161) | r-3 | `--features test-util --test retry` (`test_busy_policy_numbers`, `test_missing_and_malformed_retry_after_back_off_exponentially`, `test_retry_after_grammar`, `test_exhaustion_reason_distinguishable`, `test_retry_sleep_capped_by_budget`, `test_cancel_interrupts_sleep`, `test_no_retry_after_body_sent`, `test_connect_failure_returns_immediately_retryable`, `test_transport_error_with_secret_is_redacted`); lib tests `sends_are_bounded_to_one_through_five`, `default_delays_are_one_two_four_eight`, `deadlines_are_clamped_to_the_decided_bounds` |
| REQ-SAG-0613 | `send_with_busy_retry` (so `send_and_parse`, the one exchange every backend uses) records the SHA-256 of the exact request body in the send's `SendTrace` once, before the first attempt; headers are not hashed | r-6 | `--features test-util --test trace` (`test_request_sha256_is_digest_of_sent_body`, `test_request_sha256_recorded_once_across_retries`, `test_cancel_before_first_send_records_nothing`) |
| NFR-SAG-0002 | No secret in any error, log or `Debug` output: `send_with_busy_retry` redacts every `Err` it returns once, including errors from third-party transports; `HttpRequest` / `SecretHeader` `Debug` print `[redacted]` | r-3 | `--test retry -- test_transport_error_with_secret_is_redacted`, `--test ureq -- test_request_debug_and_secret_header_never_show_the_key`, `test_timeout_is_network_error_without_the_key` |
| ADR-SAG-0020 (r-3) | The single `ResolvedApiKey::as_str` call site is `src/ureq_transport.rs` (`#[expect(clippy::disallowed_methods, reason = ...)]`); lint-enforced convention, not a type seal | r-3 | `cargo clippy --workspace --all-targets -- -D warnings`; `rg` read-site check (sprint r-3 "Required validation"); r-3b `workspace-tests --test source_scans` |

`test-util` is non-default and enabled only by dev-dependencies; it never
appears in a normal-dependency graph (`cargo tree -p sc-agent-http -e
normal,features`).

## General log (agt-9eh, ADR-SAG-0013 amendment 2026-11-09)

| ID | Requirement | Sprint | Proof |
| --- | --- | --- | --- |
| REQ-SAG-0633 | `BusyPolicy::with_general_log` and `HttpClient::with_general_log` log each send, response status, busy wait (status, attempt, wait milliseconds), exhaustion, budget end and cancellation | agt-9eh | `sc-jev-backends` `--test general_log` (`busy_wait_and_judge_are_logged_without_key_prompt_or_body`) |
| REQ-SAG-0634 | Those events carry statuses, counts and times only, never a body, header or key | agt-9eh | same |
