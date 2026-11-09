# sc-jev-py — requirements

**Sprint** = the sprint that implements the requirement. The owning `req:`
acceptance root, when it differs, is shown as "root a-N"; the closure table in
[`plans/phase-a/plan.md`](../plans/phase-a/plan.md#requirement-closure-integration-req-roots) is authoritative.

| ID | Requirement | Sprint |
| --- | --- | --- |
| REQ-SAG-0800 | PyO3 API wrapping **`sc-jev-check`** (not `sc-jev` directly) | a-8 |
| REQ-SAG-0801 | Cross-impl pytest harness | a-8 |
| REQ-SAG-0802 | maturin + `pyproject.toml`; installable wheel | a-8 |
| REQ-SAG-0803 | First release: all five sc-publish channels — kit install + preflight/dry-run in CI (a-9) | a-9 |
| REQ-SAG-0604 | Partial failure raises `JevError` with `.response` (the response body as a dict); all-failed also sets `.response` (no answers, every backend's typed error; agt-2a3) | r-3b, agt-2a3 |
| REQ-SAG-0605 | `run_check(chain_context=...)`; independent is the default | r-3b |
| REQ-SAG-0711 | Session log through `sc_jev_host::obs` | r-3b |
| REQ-SAG-0615 | `run_check(project=...)` and `Session.ask(follow_up, project=...)` label that call's log row (≤ 256 bytes, no control characters, else `INPUT_INVALID`); session ids are ULIDs | r-6 |
| REQ-SAG-0165 | The Python `JevError` carries the same fields as the CLI error body (`code` for `error`, `kind`, `message`, `suggested_action`, `retryable`, `retry_after_seconds`, `field`, `details`, `schema_version`) from the same serialization; never less than the CLI | agt-2a3 |
| REQ-SAG-0632 | `run_check(session_log=, general_log=)` take callables that replace the defaults; unset, the defaults apply; a non-callable is `INPUT_INVALID`; a callable that raises is ignored | agt-9eh |
| REQ-SAG-0636 | The wheel links the `sc-observability` Python runtime (`_native`), seeds `sys.modules["sc_observability._native"]`, installs the host logger (`install_host_logger`) and keeps it in a static; Rust writes each row with `try_log` inside `py.detach`, so no row needs the GIL; Python's `get_host_logger()` shares the queue and file | agt-9eh |
| REQ-SAG-0638 | Python rows carry `run_id`, durations and failure `stage` like Rust's (same events) | agt-9eh |
| REQ-SAG-0640 | A real key in the environment and a request that fails validation leave a log with the credential source and no key or prompt (pytest `test_a_real_key_reaching_a_send_path...`) | agt-9eh |
| REQ-SAG-0637 | `pyo3 =0.29.2`, `sc-observability-py`, `-binding-runtime`, `-dto` `=1.5.0` | agt-9eh |
