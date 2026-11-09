# sc-ai — requirements

**Sprint** = the sprint that implements the requirement. The owning `req:`
acceptance root, when it differs, is shown as "root a-N"; the closure table in
[`plans/phase-a/plan.md`](../plans/phase-a/plan.md#requirement-closure-integration-req-roots) is authoritative.

| ID | Requirement | Sprint |
| --- | --- | --- |
| REQ-SAG-0700 | `sc-ai jev` subcommands, JSON envelope | a-7 |
| REQ-SAG-0701 | Typed stderr errors, exit 2 (3 when `retryable`): one `ScAgentErrorResponse` line per NFR-SAG-0001 (amended in r-1, reshaped by agt-2a3: `kind`, `suggested_action`, always-present `retryable`, optional `retry_after_seconds`, `field`, `details`); a UUID `--session-id` is `INPUT_INVALID` naming `--session-id`, with a ULID action (r-6); usage errors are `INPUT_INVALID`, never `CONFIG_INVALID` | a-7, r-6, agt-2a3 |
| REQ-SAG-0702 | Readback pairs | a-7 |
| REQ-SAG-0703 | Install documentation | a-7 |
| REQ-SAG-0710 | Integration test: versioned sc-ai-cli envelope; Jev answer + `session_id` in `data` | a-7 |
| REQ-SAG-0711 | `sc-ai` wires the logger through `sc_jev_host::obs` (the adapter and `sc-observability` live in `sc-jev-host`) | a-7, r-3b |
| REQ-SAG-0604 | Partial or all-failed run: envelope on stdout (the successful answers, if any, and every backend's typed error), the first failure's `ScAgentErrorResponse` on stderr, exit 2, or 3 when every failed backend's error is retryable (agt-2a3) | r-3b, agt-2a3 |
| REQ-SAG-0151 | `jev models` prints `{"version","operation":"models","data":{"models_file","models_file_present","models":[{"alias","model","source","released","backend"}]}}`; `--backend` takes a short name or a known model id; `--models-file` reads another file; `--help` pins all three (`test_help_pins_short_names_and_models`) | agt-akp |
| REQ-SAG-0605 | `jev run --chain-context` selects chain context; independent is the default | r-3b |
| REQ-SAG-0712 | argv is parsed with clap derive behind the `cli` feature; error contract unchanged | r-3b |
| NFR-SAG-0002 | No secrets in output | a-7 |
| REQ-SAG-0615 | `jev run` / `jev ask` take `--project <TEXT>` (≤ 256 bytes, no control characters; stored as given, not scrubbed); `--session-id` is a ULID | r-6 |
| REQ-SAG-0162 | A failed read or write (a directory as `--request-json`, stdout, git, the session store) is `IO_FAILED` with `details.what` and `details.io_kind`; a missing named path stays `INPUT_MISSING` (`JevError::read_failed`) | agt-2a3 |
| REQ-SAG-0167 | A `test-util`-only hidden `--loopback-port <PORT>` flag on `jev run` builds the real HTTP backends aimed at `127.0.0.1:<PORT>` (keys from the environment); the published binary rejects it (`workspace-tests --test test_util_off_binary`) | agt-2a3 |
| NFR-SAG-0003 | Document default log root, session files and the one log row per send (README "Session logs") | a-7, r-6 |
| REQ-SAG-0639 | A bad `--repo-root`, `--request-json` or check source leaves a general-log row by typed id | agt-9eh |
| REQ-SAG-0632 | The CLI injects no logger: it keeps the default session and general logs (`<log-root>/logs/sc-ai.log.jsonl`, `sc-ai-general.log.jsonl`) and `--log-dir`; there is no logger flag | agt-9eh |

Phase A: `sc-ai jev` check parity only. Phase B: more Jev API + API-direct Claude/Codex (REQ-SAG-0900/0901); no vendor CLI wrappers in Phase A or B (a later agent kind, ADR-SAG-0030).

## Streaming (agt-1aq, ADR-SAG-0032)

| ID | Requirement | Proof |
| --- | --- | --- |
| REQ-SAG-0721 | `jev stream` JSONL in, one id-echoing JSONL answer per request, completion order | `cli` `stream_answers_every_request_and_a_failure_does_not_stop_it`, `stream_reads_a_jsonl_file_and_exits_zero_on_success` |
| REQ-SAG-0722 | Per-request failures are error lines; exit 0 / 3 (all retryable) / 2 | `stream_exit_code_follows_the_retryable_rule` |
| REQ-SAG-0724 | `--concurrency` 1 to 64, default 8 | `cli::tests::stream_concurrency_is_validated`; `stream_runs_concurrent_requests_over_real_http_with_one_shared_client` |
| REQ-SAG-0725 | `--help` pins the stream contract | `cli::tests::help_notes_describe_the_stream` |
