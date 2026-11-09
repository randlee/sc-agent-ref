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

## Streaming (agt-1aq, ADR-SAG-0032)

| ID | Requirement | Proof |
| --- | --- | --- |
| REQ-SAG-0726 | `run_many` (returns `(answers, summary)`, validates every id and body before anything starts, interruptible) and `DecisionStream` (submit, submit_line, next, iterate, close_input, cancel, finish); the GIL is released while workers run | pytest `test_stream.py` (`test_run_many_answers_every_id_and_a_bad_request_does_not_raise`, `test_early_answers_become_follow_up_requests`, `test_run_many_validates_every_id_and_body_before_anything_starts`, `test_submit_line_takes_the_cli_jsonl_form`, `test_finish_closes_input_and_never_hangs_on_unread_answers`, `test_the_gil_is_released_while_waiting_for_an_answer`) |
| REQ-SAG-0724 | `concurrency=` 1 to 64, default 8 | `test_concurrency_is_validated` |
| REQ-SAG-0727 | A full queue applies backpressure; `cancel` stops new work | `test_a_full_queue_applies_backpressure`, `test_a_single_thread_submitting_past_capacity_times_out`, `test_cancel_stops_new_work` |
