# sc-jev-py — architecture

**Boundary:** `jev-py`  
**Depends on:** `sc-jev-host` (`=`-pinned, ADR-SAG-0023), `sc-jev-check` (Rust, statically linked in extension), `sc-observability-py`, `sc-observability-binding-runtime` and `sc-observability-dto` (`=1.5.0`, the shared host logger), `pyo3 =0.29.2`

## Role

Official **Python integration** for TypeSafe Jev / check flows. Distributed as a
**PyPI wheel** (maturin). Python applications `import sc_jev_py` and call Rust
in-process — no subprocess to `sc-ai` required.

atm-bd `packages/jev-check` may remain a thin policy/typing wrapper over this
extension; it is not the primary deliverable.

## Streaming (REQ-SAG-0726, ADR-SAG-0032)

`run_many(requests, concurrency=8, ...)` and `DecisionStream(...)` are thin
adapters over `sc_jev_host::stream`: the PyO3 crate only converts arguments,
detaches the GIL and maps errors; the batch driver (`stream::Batch`, feeder
thread, drop guard) lives in the host. `run_many` returns `(answers, summary)`
and validates every `RequestId` and body before a stream starts;
`DecisionStream.submit_line` takes the CLI's JSONL line form; `finish()` closes
the input and drops unread answers instead of hanging. An unread answer holds
capacity, so a single thread must read concurrently or submit with `timeout=`.
`timeout` is validated once at the boundary (NaN or negative is `INPUT_INVALID`
naming `timeout`; `inf` or a value too large to represent is no deadline).

**Loggers.** `DecisionStream` and `run_many` take `session_log=` and
`general_log=` like `run_check`. Unset, the general log is the shared host
logger (`HostGeneralLog`, the one `run_check` and
`sc_observability.get_host_logger()` use, so one file has one writer), flushed
on every way out. Every way out of a stream (`finish`, a `with` exit, a drop,
an interrupted `run_many`) logs `stream.end` and flushes both default logs;
all but `finish` wait at most 2 s for workers first (`ABANDON_WAIT`), with the
GIL released. A repeated `finish` writes one `stream.end`. The shared host logger admits a
row without blocking, so `HostGeneralLog::log` waits out a full runtime queue
(at most 10 s per row, no GIL held) instead of dropping; a row refused after the
bound is counted and reported (Warn row `general_log.rows_dropped`, and
`general_log_rows_dropped` in the `finish()`/`run_many` summary). Answers are the
CLI's output lines as dicts (`{"id", "ok", "envelope"?, "error"?}`). Every blocking call runs under
`py.detach` in 50 ms slices with a signal check, so the GIL is released while
workers run and `Ctrl-C` raises `KeyboardInterrupt`. A failed request is an
answer, never an exception. The test-util entry points are
`_testing.decision_stream_scripted` and `_testing.run_many_scripted`.

## Build and release

- **maturin** builds the extension against workspace **`sc-jev-check`**.
- Version **lockstep** with Rust crates on first release (single tag / workspace version).
- CI produces wheels (manylinux, macOS arm64 minimum); **PyPI** publish aligns
  with crates.io and **Homebrew / Scoop / WinGet** for `sc-ai` on one tag.

## Contract

PyO3 only — no duplicate Jev HTTP or canonicalization. `build_request` calls
`sc_jev_check::run::prepare` and does not run extractors (ADR-SAG-0005,
ADR-SAG-0024).
`run_check` has one signature in every build and never takes
`scripted_answers`; this crate does not implement `JudgeBackend`
(ADR-SAG-0009). The test-util entry point is the separate function
`sc_jev_py._testing.run_check_scripted(check_path, *, scripted_answers, ...)`.
The `_testing` submodule is registered only under the `test-util` feature,
and the function forwards the answers to
`sc_jev_check::registry::register_scripted_backends` (ADR-SAG-0022,
ADR-SAG-0029). Default features and the publish wheel leave that off. Exceptions expose the same
error codes as `JevError`.

Log and session paths follow ADR-SAG-0021 and are resolved by
`sc_jev_host::paths::PathPolicy::python()`. Log root, first match:
`log_dir`, `SC_AGENT_LOG_DIR`, `SC_LOG_ROOT`, then `~/.sc-agent/log`. Session
directory, first match: `session_dir`, `SC_AGENT_SESSION_DIR`, then
`{log_root}/sessions`. The JSONL file is `{log_root}/logs/sc-jev-py.log.jsonl`
through the `sc-jev-host` adapter over the v2 `LoggerBuilder` file sink, one
session-log row (ADR-SAG-0031) in each line's `fields`
(ADR-SAG-0013, ADR-SAG-0016 r-3b amendment). This crate keeps argument
conversion, the GIL release and exception mapping only (ADR-SAG-0011 r-3b).

## Project label and session ids (REQ-SAG-0615, REQ-SAG-0612)

`run_check(..., project=None)` and `CheckResult.ask` / `Session.ask(follow_up,
*, project=None)` take caller text for that call's log row (per call: a run's
project does not carry into a later `ask`). It is validated as a
`ProjectLabel`, so a label over 256 bytes or with a control character raises
`JevError` `INPUT_INVALID`; the text is stored as given and not scrubbed.
Session ids (`CheckResult.session_id`, `Session.session_id`) are 26-character
ULIDs. Python never accepts a session id as input, so the UUID refusal is
reached through the CLI.

## Multi-backend runs (REQ-SAG-0604, REQ-SAG-0605)

`run_check(..., chain_context=False)`: independent is the default;
`chain_context=True` sets `MultiBackendMode::ChainContext` (ADR-SAG-0008).
`CheckResult.data` is the run response body (the `data` the `sc-ai` envelope
prints, without `readback`). When some, not all, backends fail, `run_check`
raises `JevError` carrying the first failure's fields and a
`.response` attribute holding that body as a dict (the successful answers
included). When every backend fails, `JevError` carries the first failure and `.response`
is the same body with no answers and one typed error per backend
(`details.backend`); a run that selected no backend has no `.response`.

## Stderr-shaped `schema_version` (ADR-SAG-0027, EXT-030)

Code that parses the `sc-ai` stderr line treats a `schema_version` other than
`1` as an **unknown error shape** and falls back to the exit code and the raw
line. The Python exception carries the same fields as that line, produced
from the same serialization: `code` (the line's `error`), `kind`, `message`,
`suggested_action`, `retryable`, `retry_after_seconds`, `field`, `details`
(a dict or `None`) and `schema_version`; the optional ones are `None` when
the line omits them (REQ-SAG-0165). `str(exc)` is `CODE: message.
suggested_action`.

## Test seam and release wheel (ADR-SAG-0029 (b))

`sc_jev_py._testing.run_check_scripted(check_path, *, scripted_answers, ...,
chain_context=False)` exists only in `--features extension-module,test-util`
builds (the pytest wheel). `scripted_answers` is the r-2b D5 JSON (shared or
per-backend form). The release wheel has no `_testing` submodule (CI job
`wheel`).

## Logging (agt-9eh, ADR-SAG-0013 amendment 2026-11-09)

`run_check` writes the session log and a separate general log. Both are
injectable per call (`session_log=`, `general_log=` callables) and fall back to
the defaults. The default session log is `sc-jev-host`'s. The default general
log is the shared host logger (`logging.rs`):

- `Cargo.toml` links `sc-observability-py =1.5.0` (lib name `_native`, an
  rlib), `sc-observability-binding-runtime` and `sc-observability-dto`, and pins
  `pyo3 =0.29.2` to match. `pyo3` features unify to `abi3-py310` (the runtime's
  floor); `requires-python` stays `>=3.11`.
- Module init registers `_native` with `wrap_pymodule!` and, when
  `sys.modules` has no `sc_observability._native`, seeds it. The first default
  general log (or `install_host_logger(log_dir=None)`) creates a core backend
  with `create_core_backend`, installs it with `install_host_logger`, and keeps
  the `Arc` and its owner in statics for the process.
- Rust writes each row with `try_log` (`ProducerOrigin::RustHost`) from inside
  the run's `py.detach`: no row takes the GIL. A unit test proves a row is
  written by a thread while another holds the GIL and never releases it.
- Python gets the same queue and file from `sc_observability.get_host_logger()`.
  Import `sc_jev_py` first; if the real `sc_observability._native` loaded
  first, installation reports `CONFIG_INVALID` and Rust rows still reach the
  default file.
- One process has one host logger: a call with another `log_dir` writes its
  general log through its own backend, which `get_host_logger()` does not see.
- Callables take the GIL once per row (`Python::attach`); an exception is
  swallowed as a rejection.
