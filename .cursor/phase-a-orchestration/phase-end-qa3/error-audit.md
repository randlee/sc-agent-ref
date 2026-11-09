# Error contract audit against `creating-ai-clis` and `reviewing-ai-clis`: FAIL

Audited `integrate/phase-a` at c638993, read-only. Tracked as bead `agt-2a3`, being fixed on layer `fix/pe-errors`.

The CLI error body is `{error, message, schema_version, retryable?, details?}`, with no category, failing field, `retry_after` or separate suggested action. `retryable` is omitted when false, the Python exception has only `code` and `message`, and `NetworkErrorKind` is never serialized.

| # | Failure | Current | Gap |
|---|---|---|---|
| 1 | Non-200 from Jev, Haiku, Luna | `NETWORK_ERROR` "HTTP n", body dropped (`crates/sc-jev/src/client/mod.rs:217-222`, `crates/sc-jev-backends/src/backend.rs:254-259`) | A bad API key reads as a connectivity problem; provider detail is lost |
| 2 | Out of tokens or quota | 429 always busy and retried; body unread (`crates/sc-agent-http/src/retry.rs:45`) | An agent keeps retrying a billing failure. Provider bodies unverified |
| 3 | Busy exhausted | `retryable` true, no wait time (`retry.rs:452`) | No `retry_after`, no machine cause |
| 4 | `IoFailed` | Maps to `INPUT_INVALID` (`crates/sc-agent-types/src/error.rs:774`) | Missing git or a closed pipe reads as bad input; docs tell consumers to parse prose |
| 5 | Same failure, different code | `crates/sc-ai/src/command.rs:285-290` vs `crates/sc-jev-check/src/extract/read.rs:97-128`; `operational_io` on non-I/O | No rule between `CONFIG_INVALID` and `INPUT_INVALID` |
| 6 | Token limit | `INPUT_INVALID` (`crates/sc-jev-backends/src/validate.rs:19,36-43`), byte limit is `INPUT_TOO_LARGE` | Two ids, no numeric fields |
| 7 | `RESPONSE_INVALID` | Hint says retry, `retryable()` is false | Hint contradicts the flag |
| 8 | Python exception | `code`, `message` only (`crates/sc-jev-py/src/error.rs:20-27`) | No parity with CLI or library |
| 9 | Producer failures | `retryable` and `details` dropped | Detail lost across the process boundary |
| 10 | Session store, credential store, registry, log flush | All `CONFIG_INVALID`; flush warning folded into prose (`crates/sc-jev-host/src/compose.rs:443-467`) | Config and runtime I/O share an id |

**Passes:** exit code 2 with one-line JSON on stderr; redaction on every path; per-backend partial failure; timeout, connect and TLS distinct.

**Not verified:** tests weren't run, real provider error bodies weren't seen, and crate docs under `docs/sc-*/` weren't read beyond `stability.md`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
