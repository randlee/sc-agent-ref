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
| REQ-SAG-0622 | `run_check` and `build_request` take `request_json=` (`str` or `bytes`, the CLI's `--request-json` questions document) as an alternative to `check_path`; exactly one is given, else `INPUT_INVALID`. Same payload and digests as `sc-ai jev --request-json` (pinned in `consumer_parity.rs` and `test_request_json.py`) | phase-end |
| REQ-SAG-0615 | `run_check(project=...)` and `Session.ask(follow_up, project=...)` label that call's log row (≤ 256 bytes, no control characters, else `INPUT_INVALID`); session ids are ULIDs | r-6 |
| REQ-SAG-0165 | The Python `JevError` carries the same fields as the CLI error body (`code` for `error`, `kind`, `message`, `suggested_action`, `retryable`, `retry_after_seconds`, `field`, `details`, `schema_version`) from the same serialization; never less than the CLI | agt-2a3 |
| REQ-SAG-0632 | `run_check(session_log=, general_log=)` take callables that replace the defaults; unset, the defaults apply; a non-callable is `INPUT_INVALID`; a callable that raises is ignored | agt-9eh |
| REQ-SAG-0636 | The wheel links the `sc-observability` Python runtime (`_native`), seeds `sys.modules["sc_observability._native"]`, installs the host logger (`install_host_logger`) and keeps it in a static; Rust writes each row with `try_log` inside `py.detach`, so no row needs the GIL; Python's `get_host_logger()` shares the queue and file | agt-9eh |
| REQ-SAG-0638 | Python rows carry `run_id`, durations and failure `stage` like Rust's (same events) | agt-9eh |
| REQ-SAG-0640 | A real key in the environment and a request that fails validation leave a log with the credential source and no key or prompt (pytest `test_a_real_key_reaching_a_send_path...`) | agt-9eh |
| REQ-SAG-0149 | `backends=` takes `jev`, `haiku`, `luna` or a model id of one in the resolved catalog (crate table plus `models_file=` / `~/.sc-agent/models.toml`); other defined models, ids no row lists and old names are `INPUT_INVALID` | `test_short_names_and_model_ids_select_backends`, `test_other_names_are_input_invalid_and_list_the_valid_ones`, `test_models_file_ids_are_backends_and_unknown_ids_stay_refused` |
| REQ-SAG-0150 | `models_file=` and `~/.sc-agent/models.toml` extend the aliases; malformed or wrong-family rows are `CONFIG_INVALID` | `test_run_check_reads_the_home_models_file`, `test_models_file_kwarg_on_run_check_and_build_request`, `test_malformed_models_file_is_config_invalid`, `test_a_model_of_another_family_is_config_invalid` |
| REQ-SAG-0151 | `models()` lists the resolved models and their source, like `sc-ai jev models` | `test_models_lists_crate_and_file_rows` |
| REQ-SAG-0621 | `CheckResult.questions_sha256` and `request_sha256` (per backend), computed before sending, as in the CLI | `test_branches_fixture_digest_is_stable`, `test_dry_run_git_repo_returns_digest` |
| REQ-SAG-0731 | `ask(session_id, follow_up, *, project, repo_root, session_dir, log_dir, models_file, session_log, general_log)` resumes any stored session by ULID, from this or an earlier process, through the CLI's `ask_from`; a non-ULID id is `INPUT_INVALID` (field `session_id`); GIL released; Python is never less than `sc-ai jev ask` | `test_ask_by_id_matches_session_ask`, `test_ask_resumes_a_session_from_another_process`, `test_ask_refuses_non_ulid_ids`, `test_ask_unknown_ulid_is_session_not_found` (`test_ask_by_id.py`) |
| REQ-SAG-0637 | `pyo3 =0.29.2`, `sc-observability-py`, `-binding-runtime`, `-dto` `=1.5.0` | agt-9eh |

## Streaming (agt-1aq, ADR-SAG-0032)

| ID | Requirement | Proof |
| --- | --- | --- |
| REQ-SAG-0726 | `run_many` (returns `(answers, summary)`, validates every id and body before anything starts, interruptible) and `DecisionStream` (submit, submit_line, next, iterate, close_input, cancel, finish); the GIL is released while workers run | pytest `test_stream.py` (`test_run_many_answers_every_id_and_a_bad_request_does_not_raise`, `test_early_answers_become_follow_up_requests`, `test_run_many_validates_every_id_and_body_before_anything_starts`, `test_submit_line_takes_the_cli_jsonl_form`, `test_finish_closes_input_and_never_hangs_on_unread_answers`, `test_the_gil_is_released_while_waiting_for_an_answer`) |
| REQ-SAG-0729 | `DecisionStream` and `run_many` take `session_log=` and `general_log=` (default: the shared host logger, flushed on every exit); every way out of a stream logs `stream.end` and flushes within 2 s, a repeated `finish` writes one | pytest `test_a_stream_takes_session_and_general_log_callables`, `test_the_default_general_log_is_the_shared_host_logger_file`, `test_a_repeated_finish_writes_one_stream_end`, `test_every_way_out_of_a_stream_leaves_its_rows_on_disk` (`with`, plain iteration, SIGINT in `run_many`; fresh process, exits at once) |
| REQ-SAG-0730 | A stream at full rate keeps every general-log row: a full runtime queue is waited out (bounded), a row dropped after the bound is counted in the summary (`general_log_rows_dropped`) and a Warn row; Python rows on `get_host_logger()` are not starved | pytest `test_a_stream_at_full_rate_keeps_every_general_log_row` (`finish`, `with`, plain iteration; 3,000 requests at concurrency 16, fresh process), `test_python_rows_on_the_shared_logger_are_not_starved` |
| REQ-SAG-0724 | `concurrency=` 1 to 64, default 8 | `test_concurrency_is_validated` |
| REQ-SAG-0724 | `timeout` is zero or more seconds: NaN or negative is `INPUT_INVALID` (field `timeout`); `inf` or a huge value is no deadline | `test_a_nan_or_negative_timeout_is_input_invalid`, `test_an_infinite_or_huge_timeout_means_no_deadline` |
| REQ-SAG-0727 | A full queue applies backpressure; `cancel` stops new work | `test_a_full_queue_applies_backpressure`, `test_a_single_thread_submitting_past_capacity_times_out`, `test_cancel_stops_new_work` |
