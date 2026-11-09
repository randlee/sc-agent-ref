# sc-ai — architecture

**Boundary:** `sc-ai-cli`  
**Depends on:** `sc-jev-host` (`=`-pinned, ADR-SAG-0023), `sc-jev-check`, `sc-agent-types`, `clap` and `mimalloc` (optional, `cli` feature)

Binary crate. clap derive (bin-private `src/cli.rs`) → typed `Command` →
`sc_ai::execute` → **`sc-jev-host`** (session-log and general-log adapters, `PathPolicy::cli()`,
repo discovery, `run_from` / `ask_from`) → **`sc-jev-check`** → stdout JSON /
stderr errors. `sc-ai` no longer links `sc-observability` and holds no path
defaults or logger adapter (ADR-SAG-0010 r-3b amendment).
No HTTP crates as **direct** dependencies (`cargo tree -p sc-ai --depth 1` must not
list `reqwest` / `hyper` / `ureq`).

Follow Synaptic Canvas **sc-ai-cli** conventions: success envelope
`{ "version", "operation", "data" }` where `data` is the `RunResponse` body
(`Envelope::run`) plus `readback`. Stderr errors are one `ScAgentErrorResponse`
line (ADR-SAG-0027, NFR-SAG-0001).

## Argv (REQ-SAG-0712)

`--help` and `--version` print to stdout and exit 0; every other parse error is
one `INPUT_INVALID` stderr line naming the argument in `field` (usage
errors are `INPUT_INVALID`, never `CONFIG_INVALID`: they are the caller's
input) and exit 2 (a value parser's own `JevError`,
such as a malformed `--rev` or `--session-id`, is returned unwrapped). Value
flags accept values starting with `-`. `--rev` and `--check` conflict with
`--request-json`. `--chain-context` sets `MultiBackendMode::ChainContext`;
independent is the default (ADR-SAG-0008). `--session-id` is a ULID: any other
value, a UUID included, is one `INPUT_INVALID` line (`field` `--session-id`) whose suggested action says session
ids are ULIDs since Phase A.1 and to start a new session with `sc-ai jev run`.
`--project <TEXT>` on `jev run` and `jev ask` is validated as a `ProjectLabel`
(at most 256 bytes, no control characters) and reaches that call's log row
through `ComposeInputs::project`; it is per call and not scrubbed (REQ-SAG-0615).

## Streaming (REQ-SAG-0721..0725, ADR-SAG-0032)

`jev stream` is the one command that is not one request per process. Argv
becomes a `StreamCommand`; `sc_ai::serve_stream` builds `ComposeInputs` like
`jev run`, opens `--input` (a path relative to the repo root, or stdin) and
calls `sc_jev_host::stream::serve_jsonl`, which writes one answer line per
request as it completes. `main` exits with `StreamSummary::exit_code`.
Per-request errors are on their stdout lines; only a stream-level failure
writes a stderr line. The wire format, finish rule and exit code are in
`sc_jev_host::stream` and `docs/stability.md`.

## Partial failure (REQ-SAG-0604, ADR-SAG-0025)

When `RunResponse::is_partial()`, stdout carries the envelope with the
successful answers, stderr carries the first failure
(`RunResponse::first_failure()`), and the exit code is 2, or 3 when every failed
backend's error is retryable (retry only the failed backends; the order of the
backends does not matter). When every backend fails, `run` still returns the response: the
envelope has no answers and one typed error per backend (each with
`details.backend`), stderr carries the first failure, and the exit code follows
it the same way (agt-2a3; ADR-SAG-0025 amendment).

## Stderr `schema_version` (ADR-SAG-0027, EXT-030)

Every stderr line carries `schema_version`. A consumer that reads it treats a
value other than `1` as an **unknown error shape**: it falls back to the exit
code and the raw line instead of reading fields it does not know. The keys,
the ids and their `details` are listed in `docs/stability.md`; a failed read
of a named path is `INPUT_MISSING` when the file does not exist and
`IO_FAILED` otherwise, for the request file and every other path.

## Test seam (plan "Test seams")

The `test-util` feature (= `sc-jev-check/test-util`) adds the hidden
`--scripted-answers <file>` flag to the binary only; the file is the r-2b D5
scripted-answers JSON, injected through `ComposeInputs::registry`. The
published binary has no such flag (`workspace-tests --test test_util_off_binary`).
The same feature adds the hidden `--loopback-port <PORT>` flag to `jev run`: it
builds the real HTTP backends (`BackendRegistry::loopback_from_env`, keys from
the environment) aimed at `http://127.0.0.1:<PORT>` with one send each, so a
binary test can drive a real HTTP failure (a mapped 401, a busy reply, a key
redacted in the stderr line). The endpoint override exists only behind
`test-util` (`JevClient::loopback`, `AnthropicHaikuBackend::loopback`,
`OpenAiLunaBackend::loopback`, `JevClientConfig::with_loopback_port`) and the
published binary rejects the flag (`test_release_binary_rejects_loopback_port`).
Every `sc-ai` test drives the built binary; the a-10 library seam
`execute_with_overrides` is gone.

## Logging (agt-9eh)

`sc-ai` injects no logger. `execute` builds a `ComposeInputs` with the CLI
`PathPolicy` and `--log-dir`; `sc-jev-host` opens the default session log
(`logs/sc-ai.log.jsonl`) and the default general log
(`logs/sc-ai-general.log.jsonl`). A dry run opens neither. Injection is a Rust
and Python API concern (ADR-SAG-0013 amendment 2026-11-09, owner direction
2026-11-08).
