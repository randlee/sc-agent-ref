# sc-jev-host — architecture

**Boundary:** `jev-host`  
**Depends on:** `sc-jev-check`, `sc-agent-types`, `sc-observability =1.5.0`, `serde_json`  
**Dependents:** `sc-ai`, `sc-jev-py`, `workspace-tests` (dev)

Composition-root logic lives here once; `sc-ai` (argv) and `sc-jev-py` (PyO3)
are thin adapters, and a future MCP server is a third (ADR-SAG-0026 host
half). The crate is published to crates.io as internal support because the
published `sc-ai` cannot depend on an unpublished crate. Both consumers pin it
with `=` (ADR-SAG-0023) and `cargo tree -d` gates keep one version of
`sc-jev-check`, `sc-agent-types` and `sc-jev-host` in their graphs.

## Modules

- **`obs::ObservabilityAdapter`** implements the `SessionLog` port over the
  `sc-observability` v2 logger and maps `EventError` to `LogRejected`.
  `open(&PathPolicy, log_root)` starts, or reuses, the one logger per
  `(service, canonical log root)` (a process-wide registry of weak handles);
  the last handle to drop flushes and shuts it down. `flush()` is explicit.
  Action name and target are fixed (`log_event`); the event's `fields` (one
  ADR-SAG-0031 session-log row) are copied into the v2 `fields{}` verbatim and
  the envelope `outcome` stays `null` (ADR-SAG-0013, no bespoke writer). The
  `tests/fixtures/observability/session-event.json` golden pins judge `ok`,
  judge `failed` with and without a body, and `ask`.
- **`obs::GeneralLogAdapter`** implements the `GeneralLog` port over the same
  v2 logger machinery and the same per-`(service, root)` writer registry, as a
  second file: service `<service>-general`
  (`<log-root>/logs/<service>-general.log.jsonl`), level `debug` and above,
  console sink off. It maps an event's target, action, message and fields to
  the v2 event unchanged. `logger_config(policy, root)` exposes the
  configuration so a root that runs its own writer (the Python wheel) writes
  the same file shape.
- **`compose::ComposeInputs::{session_log, general_log}`** inject a
  replacement for either log; unset, `Logs::open` opens the default under the
  resolved log root. It emits `paths.resolved` (repo root source, log root,
  sessions dir, and whether each log is injected, default or none), logs the
  main-repo git step and session-log flush results, and flushes the defaults it
  opened. A dry run opens no default log. A default general log that cannot
  start is dropped (the call carries on); a default session log that cannot
  start is the call's error. An injected log's flush is the caller's.
- **`paths::PathPolicy`** is one plain data type with public fields
  (`service`, `log_root_flag`, `log_root_env`, `log_root_default`,
  `session_dir_env`, `session_dir_default`) and the enums `DefaultRoot` /
  `SessionDefault`. `cli()` and `python()` encode ADR-SAG-0021 per root. A new
  consumer writes its own literal and does not edit this crate (EXT-012).
  Every resolver has a `_with` form over an injected environment, so tests
  never set process environment.
- **`compose::ComposeInputs`** has private fields: `new(cwd, policy)` plus one
  by-value setter per optional field (`repo_root`, `log_dir`, `sessions_dir`,
  `backends`, `rev`, `dry_run`, `system_prompt_override`, `mode`, `cancel`,
  `registry`, `project`). A new field is a new setter with a default (EXT-021).
  `resolve()` returns the repo root and the log and session paths a run uses,
  so a root builds its `CheckSource` and readback from the same values.
- **`compose::run_from` / `prepare_from` / `ask_from`** call
  `discover_repo_root` (unless a root is set), build `RunConfig`, then
  `sc_jev_check::run::{prepare, run, ask}`; they never build a payload
  (ADR-SAG-0005) and add no backend dispatch (ADR-SAG-0009): the registry is
  `BackendRegistry::from_env` unless `ComposeInputs::registry` is set.
  `prepare_from` returns a `HostRun` for a root that needs the request
  (payload, resolved prompt) before the run; `run_from` is
  `prepare_from(..)?.run()`.

## Log context (REQ-SAG-0614, REQ-SAG-0615)

`run_from`, `prepare_from` and `ask_from` build the `LogContext` for
`RunConfig::log_context`: `repo` is `discover_main_repo_root` of the explicit
repo root when one is set (`--repo-root`), else of the working directory (a discovery failure falls back to
the resolved root; a dry run skips git), and `project` is
`ComposeInputs::project`, per call. Neither consumer builds log context. Paths
stay on the worktree (ADR-SAG-0021): `--test log_context`.

## Error path

When `run` or `ask` fails, `HostRun::run` and `ask_from` still flush the log
(the failed rows are durable). A flush failure there has no response to carry
a warning, so it is appended once to the error detail; the error code is
unchanged. The adapter sets the envelope `correlation_id` to the session id,
`request_id` to the answer id, and the level to `Warn` for a failed send.

## Warnings

When the logger reports a flush failure (CR-014), `run_from` and `ask_from`
append `session log was not flushed: <hint>` with `with_warning`, so both
consumers emit it identically in `data.warnings`. Neither consumer adds keys of
its own besides `sc-ai`'s `readback`.

## Environment ownership (ADR-SAG-0026)

API-key environment and `.env` reading stay in `sc-jev-check`
(`BackendRegistry::from_env` → `sc-agent-secrets`). Log and session path
environment variables are read only by `PathPolicy` in `src/paths.rs`
(`workspace-tests --test source_scans`, `test_env_reads_only_in_owners`).

## Type note (EXT-033)

`run_from`, `prepare_from` and `ask_from` name types owned by other crates:
`CheckSource`, `RunResponse` and `AskResponse` (`sc-jev-check`), and
`JevError` (`sc-agent-types`). This crate re-exports none of them (one path
per item). A consumer depends on `sc-jev-check` and `sc-agent-types`
directly, at the versions this release pins. External consumers with their
own composition needs call `sc_jev_check::run::{prepare, run}` directly.
