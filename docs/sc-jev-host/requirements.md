# sc-jev-host — requirements

`sc-jev-host` is the composition-root support crate for `sc-ai` and
`sc-jev-py` (ADR-SAG-0026 host half). It is published as internal support,
`=`-pinned by both consumers, with no semver promise of its own.

| ID | Requirement | Sprint | Proof |
| --- | --- | --- | --- |
| REQ-SAG-0604 | `run_from` returns the `RunResponse` unchanged so each consumer decides on `is_partial()` / `first_failure()`; an all-backends-failed run is an `Err` | r-3b | `sc-ai` `test_cli_partial_failure_both_streams_exit_2`; pytest `test_partial_failure_raises_with_response` |
| REQ-SAG-0605 | `ComposeInputs::new` defaults to `MultiBackendMode::Independent`; `mode(..)` selects chain context | r-3b | `test_cli_chain_context_flag`; pytest `test_chain_context_kwarg`; `compose::tests::defaults_are_independent_and_unset` |
| REQ-SAG-0711 | One `SessionLog` adapter (`obs::ObservabilityAdapter`) shared by both roots; one logger per `(service, canonical log root)`; flush failure is a warning in `data.warnings` | r-3b | `cargo test -p sc-jev-host --test obs`; `test_cli_log_flush_warning_in_data` |
| REQ-SAG-0610 | One JSONL line per judge send and per `ask`, action = operation | r-3b | `--test obs` (`test_adapter_writes_one_line_without_secret_fields`) |
| REQ-SAG-0611 | Line fields `session_id`, `backend`, `prompt_sha256`, `operation`; no secrets | r-3b | `--test obs` (`test_adapter_matches_the_shared_session_event_golden`) |
| ADR-SAG-0021 | `PathPolicy::cli()` and `python()` reproduce each root's precedence unchanged; a new consumer writes its own `PathPolicy` literal (EXT-031) | r-3b | `--test paths` (`test_cli_path_policy_precedence`, `test_python_path_policy_precedence`, `test_custom_path_policy_resolves`) |
| NFR-SAG-0003 | Default and configured session and log roots | r-3b | `--test paths` (`test_default_and_configured_roots`) |
