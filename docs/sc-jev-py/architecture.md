# sc-jev-py — architecture

**Boundary:** `jev-py`  
**Depends on:** `sc-jev-host` (`=`-pinned, ADR-SAG-0023), `sc-jev-check` (Rust, statically linked in extension)

## Role

Official **Python integration** for TypeSafe Jev / check flows. Distributed as a
**PyPI wheel** (maturin). Python applications `import sc_jev_py` and call Rust
in-process — no subprocess to `sc-ai` required.

atm-bd `packages/jev-check` may remain a thin policy/typing wrapper over this
extension; it is not the primary deliverable.

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
through the `sc-jev-host` adapter over the v2 `LoggerBuilder` file sink
(ADR-SAG-0013, ADR-SAG-0016 r-3b amendment). This crate keeps argument
conversion, the GIL release and exception mapping only (ADR-SAG-0011 r-3b).

## Multi-backend runs (REQ-SAG-0604, REQ-SAG-0605)

`run_check(..., chain_context=False)`: independent is the default;
`chain_context=True` sets `MultiBackendMode::ChainContext` (ADR-SAG-0008).
`CheckResult.data` is the run response body (the `data` the `sc-ai` envelope
prints, without `readback`). When some, not all, backends fail, `run_check`
raises `JevError` carrying the first failure's `code`/`message` and a
`.response` attribute holding that body as a dict (the successful answers
included). When every backend fails, `JevError.response` is `None`.

## Stderr-shaped `schema_version` (ADR-SAG-0027, EXT-030)

Code that parses the `sc-ai` stderr line treats a `schema_version` other than
`1` as an **unknown error shape** and falls back to the exit code and the raw
line. The Python exception does not carry `schema_version`; `code` and
`message` are its fields.

## Test seam and release wheel (ADR-SAG-0029 (b))

`sc_jev_py._testing.run_check_scripted(check_path, *, scripted_answers, ...,
chain_context=False)` exists only in `--features extension-module,test-util`
builds (the pytest wheel). `scripted_answers` is the r-2b D5 JSON (shared or
per-backend form). The release wheel has no `_testing` submodule (CI job
`wheel`).
