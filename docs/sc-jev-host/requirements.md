# sc-jev-host — requirements

`sc-jev-host` is the composition-root support crate for `sc-ai` and
`sc-jev-py` (ADR-SAG-0026 host half). It is published as internal support,
`=`-pinned by both consumers, with no semver promise of its own.

| ID | Requirement | Sprint | Proof |
| --- | --- | --- | --- |
| REQ-SAG-0604 | `run_from` returns the `RunResponse` unchanged so each consumer decides on `is_partial()` / `first_failure()`; an all-backends-failed run is also returned (no answers, every backend's typed error, `details.backend`; agt-2a3), `Err` only when no backend was selected | r-3b | `sc-ai` `test_cli_partial_failure_both_streams_exit_2`; pytest `test_partial_failure_raises_with_response` |
| REQ-SAG-0605 | `ComposeInputs::new` defaults to `MultiBackendMode::Independent`; `mode(..)` selects chain context | r-3b | `test_cli_chain_context_flag`; pytest `test_chain_context_kwarg`; `compose::tests::defaults_are_independent_and_unset` |
| REQ-SAG-0711 | One `SessionLog` adapter (`obs::ObservabilityAdapter`) shared by both roots; one logger per `(service, canonical log root)`; flush failure is a warning in `data.warnings` | r-3b | `cargo test -p sc-jev-host --test obs`; `test_cli_log_flush_warning_in_data` |
| REQ-SAG-0610 | One JSONL line per judge send and per `ask`, action = operation | r-3b | `cargo test -p sc-jev-host --test obs test_log_line_fields_is_one_session_log_row` (one line per event, `action` = operation, no secret-bearing key); `test_every_operation_is_an_action_name` |
| REQ-SAG-0611 | Every line's `fields{}` is one ADR-SAG-0031 session-log row (`session_id`, `repo`, `project`, `model_id`, `payload_sha256`, `questions_sha256`, `request_sha256`, `send_outcome`, …); no secrets (amended in r-6) | r-3b, r-6 | `--test obs` (`test_adapter_matches_the_shared_session_event_golden`) |
| REQ-SAG-0617 | The adapter copies `LogEvent::fields()` into `fields{}` verbatim; envelope `outcome` is `null` | r-6 | `--test obs` (`test_log_line_fields_is_one_session_log_row`) |
| REQ-SAG-0614 | `repo` is the main repository; session and log paths stay on the worktree | r-6 | `--test log_context` (`test_log_context_repo_is_main_repo_paths_stay_on_worktree`) |
| REQ-SAG-0615 | `ComposeInputs::project(..)` labels one call's rows | r-6 | `--test log_context` (same test); `sc-ai` `test_cli_project_flag_reaches_log` |
| ADR-SAG-0021 | `PathPolicy::cli()` and `python()` reproduce each root's precedence unchanged; a new consumer writes its own `PathPolicy` literal (EXT-031) | r-3b | `--test paths` (`test_cli_path_policy_precedence`, `test_python_path_policy_precedence`, `test_custom_path_policy_resolves`) |
| NFR-SAG-0003 | Default and configured session and log roots; unchanged with ULID file names | r-3b, r-6 | `--test paths` (`test_default_and_configured_roots`); `--test log_context` (`test_log_and_session_paths_unchanged_with_ulid_names`) |
| REQ-SAG-0630 | `obs::GeneralLogAdapter` writes the default general log to `<log-root>/logs/<service>-general.log.jsonl`, separate from the session file, at `debug` and above | agt-9eh | `--test general_log` (`defaults_write_two_separate_logs`) |
| REQ-SAG-0632 | `ComposeInputs::session_log` and `general_log` each replace one default; unset, the default opens under the resolved log root | agt-9eh | `--test general_log` (`an_injected_general_log_replaces_the_default_file`, `an_injected_session_log_replaces_the_default_file`) |
| REQ-SAG-0633 | Compose logs path resolution, the main-repo git step, session-log flush results and open failures | agt-9eh | `--test general_log` (`defaults_write_two_separate_logs`) |
| REQ-SAG-0638 | One `run_id` per run on every event; durations on `run.done`/`send.done`; failure rows carry `stage` and the typed id | agt-9eh | `--test general_log` (`runs_in_one_file_are_told_apart_by_run_id_and_carry_durations`, `a_failed_run_records_stage_backend_and_typed_id`) |
| REQ-SAG-0639 | A root-discovery failure leaves a row; a path with no known root is silent; a git failure is logged by its caller | agt-9eh | `--test general_log` (`a_root_discovery_failure_leaves_a_general_log_row`, `a_failure_with_no_known_log_root_is_silent`, `a_git_failure_is_logged_by_its_caller_and_the_run_still_succeeds`) |
| REQ-SAG-0634 | The default general log holds no prompt text | agt-9eh | `--test general_log` (`the_general_log_never_holds_prompt_text`) |
| REQ-SAG-0635 | A rejecting general log changes no result; a dry run opens no default log; a default general log that cannot start does not fail the call | agt-9eh | `--test general_log` (`a_rejecting_general_log_changes_nothing`, `a_dry_run_opens_no_default_log`) |

## Streaming (agt-1aq, ADR-SAG-0032)

| ID | Requirement | Proof |
| --- | --- | --- |
| REQ-SAG-0720 | Bounded job and answer queues, N `std` worker threads, engine unchanged | `stream::pool::tests` (`workers_run_jobs_in_parallel_not_in_turn`); `--test stream` `the_default_runs_eight_requests_in_parallel` (rendezvous of 8, not timing) |
| REQ-SAG-0721 | Ids echoed; a bad line has `id: null` or its readable id | `--test stream` `answers_echo_ids_and_a_failure_does_not_stop_the_stream` |
| REQ-SAG-0722 | `StreamSummary::exit_code` applies the stability rule across the stream | `--test stream` `exit_code_is_three_only_when_every_failure_is_retryable`, `a_partial_failure_line_has_both_envelope_and_error` |
| REQ-SAG-0723 | Finished = input closed, queue empty, workers idle; follow-up requests on the same stream | `--test stream` `a_stream_is_finished_only_when_input_is_closed_and_everything_drained`, `early_answers_can_become_follow_up_requests_on_the_same_stream` |
| REQ-SAG-0724 | `Concurrency` 1 to 64, default 8 | `stream::tests::concurrency_is_one_to_sixty_four_with_a_default_of_eight` |
| REQ-SAG-0727 | Bounded memory under a slow consumer; cancel stops new work and sets the token | `--test stream` `a_slow_writer_stops_the_reader_and_memory_stays_bounded`, `cancel_stops_new_work_and_sets_the_streams_token, finish_closes_input_and_never_hangs_on_unread_answers, submitting_past_capacity_times_out_instead_of_deadlocking`; `stream::pool::tests::a_slow_consumer_stops_the_producer_at_capacity`, `a_panicking_skipped_function_still_gives_one_answer_per_job`, `a_dead_worker_still_lets_the_stream_finish` |
| REQ-SAG-0728 | Stream and per-request `run_id` correlation in a real general log; injected logs replace the defaults | `--test stream` `a_parallel_stream_writes_a_whole_general_log_correlated_by_run_id`, `injected_logs_replace_the_defaults` |
| REQ-SAG-0611 | Concurrent workers write whole session-log rows through the one shared adapter | `--test stream` `concurrent_workers_write_whole_session_log_rows` |
