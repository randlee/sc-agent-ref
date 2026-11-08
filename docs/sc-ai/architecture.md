# sc-ai — architecture

**Boundary:** `sc-ai-cli`  
**Depends on:** `sc-jev-host` (`=`-pinned, ADR-SAG-0023), `sc-jev-check`, `sc-agent-types`, `clap` and `mimalloc` (optional, `cli` feature)

Binary crate. clap derive (bin-private `src/cli.rs`) → typed `Command` →
`sc_ai::execute` → **`sc-jev-host`** (session-log adapter, `PathPolicy::cli()`,
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
one `INPUT_INVALID` stderr line and exit 2 (a value parser's own `JevError`,
such as a malformed `--rev` or `--session-id`, is returned unwrapped). Value
flags accept values starting with `-`. `--rev` and `--check` conflict with
`--request-json`. `--chain-context` sets `MultiBackendMode::ChainContext`;
independent is the default (ADR-SAG-0008).

## Partial failure (REQ-SAG-0604, ADR-SAG-0025)

When `RunResponse::is_partial()`, stdout carries the envelope with the
successful answers, stderr carries the first failure
(`RunResponse::first_failure()`), and the exit code is 2. When every backend
fails, `run` returns an error: stderr only, exit 2.

## Stderr `schema_version` (ADR-SAG-0027, EXT-030)

Every stderr line carries `schema_version`. A consumer that reads it treats a
value other than `1` as an **unknown error shape**: it falls back to the exit
code and the raw line instead of reading fields it does not know.

## Test seam (plan "Test seams")

The `test-util` feature (= `sc-jev-check/test-util`) adds the hidden
`--scripted-answers <file>` flag to the binary only; the file is the r-2b D5
scripted-answers JSON, injected through `ComposeInputs::registry`. The
published binary has no such flag (`workspace-tests --test test_util_off_binary`).
Every `sc-ai` test drives the built binary; the a-10 library seam
`execute_with_overrides` is gone.
