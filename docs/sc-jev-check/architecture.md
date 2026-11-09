# sc-jev-check — architecture

**Boundary:** `jev-check`  
**Depends on:** `sc-agent-types` (features `git`), `sc-agent-secrets`, `sc-jev`, `sc-jev-backends`

Library **engine** for the jev-check parity slice: `check.toml`, extraction,
canonical payload build, backend selection, sessions, equal-input, LLM
system-prompt loading, and the one run API (ADR-SAG-0024). Does **not** send
TypeSafe HTTP itself — delegates to `JudgeBackend` impls in `sc-jev` and
`sc-jev-backends`.

## Modules

| Module | Purpose |
| --- | --- |
| `config` | `check.toml` (`RepoRelativePath` paths, optional top-level `backends`, optional `[models]` table of `alias = model string` over the catalog aliases `jev`, `haiku`, `luna`; an unknown alias or invalid model string is `CONFIG_INVALID` before any send; `RunConfig::models` wins), `[llm_judge]` path, packaged prompt |
| `extract` | `file`, `table-column`, `subjects`, `command` — matched directly by `extract_input` (no extractor trait); see [`extraction.md`](extraction.md); **no** tree-sitter deps |
| `payload` | `build_payload`: one canonical `PayloadBundle` per check; caller request documents. No byte cap (each backend owns its cap, REQ-SAG-0314) |
| `run` | `prepare` → `run`, `discover_repo_root`, `discover_main_repo_root`, `RunConfig` (with `log_context(LogContext)`), `RunResponse` + `Envelope` (ADR-SAG-0024); the one send step writes the session-log row (below) |
| `session` | Stateless `FileSessionStore`: `save`, `load`, `ask(&registry, &log)`; ULID file names, session document version 3 (`decision_schema`); `new` removes `*.json` documents past TTL plus one TTL of grace (v1/v2 included); no-lock concurrency model (REQ-SAG-0607) |
| `equal_input` | Equal-input record helpers; `equal` comes from backend-reported digests (REQ-SAG-0606, ADR-SAG-0025) |
| `registry` | `BackendRegistry::{from_env, from_resolver, register}` plus `*_with_models` variants taking `&ModelAliases`; private `FACTORIES` table of `fn(ResolvedApiKey, &ModelAliases)` (ADR-SAG-0009). Feature `test-util` adds `register_scripted_backends` and `RunResponse::test_builder` for tests only (ADR-SAG-0022). |

## Composition

```text
prepare(CheckSource, RunConfig) -> PreparedRun
    load check.toml (or a caller request) at rev → build PayloadBundle once
    select backends: RunConfig::backends | source `backends` | [jev]
    resolve the LLM system prompt when an LLM backend is selected
run(PreparedRun) -> RunResponse
    dry run: payload, payload_bytes, per-backend limits; nothing sent
    live:    registry (injected | from_env) → validate_for_send on every backend
             → judge each in order, once; Independent by default, ChainContext
               (RunConfig::mode) gives later LLM backends prior Ok answers
             → a failed backend keeps the others' answers (leak = any Failed;
               all failed = still a response, no answers, one typed error
               per backend with details.backend); attest each reported digest
             → save the session once when an ask-capable backend answered
               (a save failure is a warning, answers kept)
```

## Session-log rows (REQ-SAG-0610, REQ-SAG-0617, ADR-SAG-0031)

The engine's one send step (`run/log.rs`) creates a fresh `SendTrace` per
backend send, calls `judge(req, &trace)`, then emits exactly one `LogEvent`
whose `fields` is one `SessionLogRow`: `ok` with the typed answers or `failed`
with the `ErrorCode`. `request_sha256` is the trace's value (`null` when the
send failed before a body was built), `model_id` is `JudgeBackend::model_id()`,
`answer_id` is a fresh ULID and `session_created_at` is the session's creation
instant (the saved document's `created_at`; for `ask`, the stored one). A
backend refused before send (`NotSent`) and a session-save failure write no
row. A rejected row never fails the run (best effort). `repo` and `project`
come from `RunConfig::log_context(LogContext)`; unset, `repo` is
`discover_main_repo_root` of the config's repo root (a linked worktree
resolves to its main repository; `.git` common dir parent, else the toplevel,
else the folder) and `project` is `null`. `repo` changes only the log field:
session and log paths keep using the worktree's own root (ADR-SAG-0021). A
non-UTF-8 byte in `repo` is replaced (`to_string_lossy`). The scripted
registry records the digest of `{"scripted":"<backend id>"}` as its sent body
(ADR-SAG-0022).

`sc-ai` and `sc-jev-py` depend on **`sc-jev-check`**, not on `sc-jev`
directly, and print `Envelope::run(&response)` unchanged. `prepare` is the
public payload assembly entry (ADR-SAG-0005); callers do not build payloads.
Without an explicit root both consumers use `discover_repo_root` (git
toplevel, ADR-SAG-0003).

## Test seams

Integration targets `run_response`, `session_log`, `session` and `sessions_exact` need this crate's `test-util` feature and
declares `[[test]] required-features = ["test-util"]`; its validation lines
pass `--features test-util`. There is no self dev-dependency
(`boundaries/README.md`, D0 outcome 4).

## Non-responsibilities

- TypeSafe / Anthropic / OpenAI HTTP (backend crates).
- CLI argv, PyO3 glue (thin adapters only).

## General log (agt-9eh)

`RunConfig::general_log` takes a `SharedGeneralLog` (default `NoopGeneralLog`)
beside the session log. `prepare` logs the source, rev, selected backends,
prompt source and the payload size and digest; `payload::build_payload_logged`
and `extract::extract_input_logged` log each extraction (kind, whether it reads
git, subject count); `run` logs start, per-backend send start and end, session
store open and save, and the outcome; `ask` logs the session load and the
follow-up. `BackendRegistry::from_env_with_models_logged` and
`from_resolver_with_models_logged` log the credential source (through
`resolve_key_logged`) and each backend's resolved model, and give every backend
the same log. A rejected session-log row is logged to the general log. Failures
are logged by `ErrorCode` only.
