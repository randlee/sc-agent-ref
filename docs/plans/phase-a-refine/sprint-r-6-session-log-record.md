---
id: r-6
phase: r
status: blocked
closure_type: contract
target_boundary: session-log
track: observability
wave: 6
size: L
branch: sprint/r-6-session-log-wiring
worktree: sprint/r-6-session-log-wiring
recommended_agent: sai
recommended_model: gpt-5.6-sol-medium
must_follow: [r-7]   # r-2b, r-3, r-3b transitively through r-7
parallel_safe: [r-5]
requirements:
  - REQ-SAG-0612
  - REQ-SAG-0613
  - REQ-SAG-0614
  - REQ-SAG-0615
  - REQ-SAG-0616
  - REQ-SAG-0617
  - REQ-SAG-0618
  - REQ-SAG-0600   # amended: ULID session ids and file names
  - REQ-SAG-0611   # amended: every line's fields{} is one ADR-SAG-0031 row
  - REQ-SAG-0607   # regression: written-once, no-lock store unchanged apart from ULID names and document v3
  - REQ-SAG-0610   # every judge/ask send emits one row, Failed included, NotSent none
  - REQ-SAG-0146   # consumes r-7's row types and DDL unchanged
  - REQ-SAG-0102   # regression: cross-impl digest unchanged after the sc-sha switch
  - REQ-SAG-0104   # regression: sc-treesitter query_sha256 unchanged after the sc-sha switch
  - REQ-SAG-0310
  - REQ-SAG-0401   # regression: busy retry unchanged by the send-trace record
  - REQ-SAG-0402
  - REQ-SAG-0606   # regression: input_sha256 recovery unchanged
  - REQ-SAG-0700   # sc-ai --project flag; --session-id is a ULID
  - REQ-SAG-0701   # UUID --session-id: one ScAgentErrorResponse line, exit 2
  - REQ-SAG-0711
  - REQ-SAG-0800   # sc-jev-py project= kwarg; errors keep JevError codes
  - REQ-SAG-0801   # pytest harness gains project and ULID cases
  - REQ-SAG-0802
  - NFR-SAG-0001
  - NFR-SAG-0002
  - NFR-SAG-0003
adrs:
  - ADR-SAG-0001  # new INPUT_INVALID paths (UUID session id, bad project text) are JevError constructors
  - ADR-SAG-0002  # prompt_sha256 stays the digest of the canonical bytes; sc-sha must hash them unchanged (D0)
  - ADR-SAG-0003  # r-2 amendment: git toplevel for .env and paths; the log `repo` field is a different root, defined in ADR-SAG-0031
  - ADR-SAG-0004  # a backend refused before send (NotSent) writes no row
  - ADR-SAG-0005  # backends hash the wire body at the send site; they still never re-canonicalize
  - ADR-SAG-0006  # transport injection unchanged; RecordingTransport proves the digest
  - ADR-SAG-0007  # request_sha256 is recorded once per send, before the first attempt; retries resend the same bytes
  - ADR-SAG-0008  # r-6 amendment (ADR-SAG-0031): ULID session paths, session document v3
  - ADR-SAG-0009  # JudgeBackend::model_id is a provided method (new methods have default bodies)
  - ADR-SAG-0010  # --project flag on the thin CLI adapter
  - ADR-SAG-0011  # project= kwarg on the Python adapter
  - ADR-SAG-0012  # sc-jev keeps the Jev body; r-6 only hands the send trace to sc-agent-http and reports the configured model
  - ADR-SAG-0013  # r-6 amendment (ADR-SAG-0031): the row lives in the v2 fields{} object; no bespoke writer
  - ADR-SAG-0014  # stdout session_id becomes a ULID; goldens updated, no new key
  - ADR-SAG-0016  # r-6 amendment (ADR-SAG-0031): LogEvent carries the row's fields
  - ADR-SAG-0019  # model_id on the wire for Jev, Haiku and Luna is the backend's configured model
  - ADR-SAG-0020  # hashing the body adds no secret read site; headers are not hashed
  - ADR-SAG-0021  # log and session paths unchanged (worktree root), proven by test
  - ADR-SAG-0022  # MockJudgeBackend and scripted backends report model_id and a send trace
  - ADR-SAG-0023  # new dependencies ulid and sc-sha, caret, not lockstep
  - ADR-SAG-0024  # RunResponse / FileSessionStore file names; prepare/run unchanged
  - ADR-SAG-0025  # input_sha256 attestation stays on canonical bytes; Failed emits a row
  - ADR-SAG-0026  # r-6 amendment (ADR-SAG-0031): sc-agent-http records the request digest; compose owns repo/project
  - ADR-SAG-0027  # r-6 amendment (ADR-SAG-0031): core gains SendTrace, ProjectLabel, ulid, sc-sha; LogEvent stable-surface change
  - ADR-SAG-0028  # no new spawn site: the git helper stays in sc-agent-types/src/git.rs
  - ADR-SAG-0029
  - ADR-SAG-0030  # model catalog (ModelId default) and decision schema id in the session document
  - ADR-SAG-0031  # wiring subsection lands here (r-6 flips it)
owned_paths:
  - "crates/sc-agent-types/Cargo.toml#-uuid -sha2 +ulid +sc-sha"
  - "crates/sc-agent-types/src/ids.rs#SessionId (ULID), Sha256Digest::of (sc-sha), ProjectLabel (new)"
  - "crates/sc-agent-types/src/trace.rs (new: SendTrace)"
  - "crates/sc-agent-types/src/lib.rs#re-exports SendTrace, ProjectLabel"
  - "crates/sc-agent-types/src/error.rs#test literal SessionId (ULID)"
  - "crates/sc-agent-types/src/observability/**"
  - "crates/sc-agent-types/src/git.rs#discover_common_dir (existing spawn helper)"
  - "crates/sc-agent-types/tests/ids.rs#ULID, ProjectLabel, sha256 reference vectors"
  - "crates/sc-agent-types/tests/obs.rs"
  - "crates/sc-agent-types/tests/errors.rs#ULID literals"
  - "crates/sc-agent-types/tests/trace.rs (new)"
  - "crates/sc-agent-types/tests/git.rs#discover_common_dir cases"
  - "crates/sc-agent-types/tests/fixtures/no-default-features-deps.txt"
  - "boundaries/sc-agent-types/**#facade +SendTrace +ProjectLabel"
  - "crates/sc-decision-schema/src/judge.rs#JudgeBackend::model_id (provided), JudgeRequest::send_trace"
  - "crates/sc-decision-schema/src/test_util.rs#MockJudgeBackend::with_model_id, with_sent_body"
  - "crates/sc-decision-schema/tests/mock.rs#model_id and send-trace cases"
  - "crates/sc-agent-http/src/request.rs#HttpRequest::with_trace"
  - "crates/sc-agent-http/src/retry.rs#record request_sha256 before the first send"
  - "crates/sc-agent-http/tests/trace.rs (new)"
  - "crates/sc-agent-http/Cargo.toml#[[test]] trace required-features = [\"test-util\"]"
  - "crates/sc-jev/src/client/mod.rs#send-trace hand-off (one call; r-5 owns the rest of the file)"
  - "crates/sc-jev/src/check.rs#impl JudgeBackend for JevClient: fn model_id (r-5 owns the rest of the file)"
  - "crates/sc-jev/tests/send_trace.rs (new)"
  - "crates/sc-jev-backends/src/backend.rs#send-trace hand-off and fn model_id"
  - "crates/sc-jev-backends/tests/send_trace.rs (new)"
  - "crates/sc-jev-check/src/run/**#session-log row emission, LogContext, per-send SendTrace"
  - "crates/sc-jev-check/src/session/**#ULID ids and file names, document v3"
  - "crates/sc-jev-check/src/registry/scripted.rs#model_id and send-trace reporting"
  - "crates/sc-jev-check/src/**#discover_main_repo_root (beside discover_repo_root)"
  - "crates/sc-jev-check/Cargo.toml#[[test]] session_log, repo"
  - "crates/sc-jev-check/tests/session_log.rs (new)"
  - "crates/sc-jev-check/tests/repo.rs (new)"
  - "crates/sc-jev-check/tests/session.rs#v2 document invalid"
  - "crates/sc-jev-check/tests/sessions_exact.rs#ULID names, rows without secrets"
  - "crates/sc-agent-compose/src/obs.rs"
  - "crates/sc-agent-compose/src/compose.rs#ComposeInputs::project, repo resolution (the file defining ComposeInputs)"
  - "crates/sc-agent-compose/tests/obs.rs"
  - "crates/sc-agent-compose/tests/log_context.rs (new)"
  - "crates/sc-ai/Cargo.toml#-uuid"
  - "crates/sc-ai/src/**#--project flag, ULID --session-id, help text"
  - "crates/sc-ai/tests/cli.rs#--project and ULID cases"
  - "crates/sc-jev-py/src/**#project kwarg, ULID session ids"
  - "crates/sc-jev-py/sc_jev_py.pyi#project kwarg"
  - "tests/fixtures/jev-python/tests/test_run_check_and_ask.py#project and ULID cases"
  - "tests/fixtures/observability/**"
  - "tests/fixtures/jev/**#session ids in goldens (ULID)"
  - "crates/workspace-tests/tests/consumer_parity.rs#session ids in goldens (ULID)"
  - "Cargo.toml#[workspace.dependencies] +ulid +sc-sha -uuid -sha2"
  - "Cargo.lock (regenerated)"
  - "sc-lint.toml#ulid and sc-sha ownership records"
  - "README.md#ask --session-id <ULID>"
  - "docs/architecture.md#ADR-SAG-0031 wiring subsection (Status line and bullet markers)"
  - "docs/architecture.md#ADR index row ADR-SAG-0031 (Status cell: r-6 part)"
  - "docs/architecture.md#ADR-SAG-0008, 0013, 0016, 0026, 0027 amendment subsections (r-6) and their index Amended-by markers (r-6)"
  - "docs/requirements.md#REQ-SAG-0600 and 0611 amendment rows; REQ-SAG-0612..0618 rows"
  - "docs/sc-agent-types/**#ULID, sc-sha, SendTrace, ProjectLabel, LogEvent fields"
  - "docs/sc-agent-http/**#send trace"
  - "docs/sc-jev-check/**#REQ-SAG-0600, 0610, 0611 rows; row emission; discover_main_repo_root"
  - "docs/sc-agent-compose/**#log context (repo, project)"
  - "docs/sc-ai/**#--project; NFR-SAG-0003 row"
  - "docs/sc-jev-py/**#project kwarg"
---

# Sprint r-6 — Session log wiring: ULID, sc-sha, send trace, repo, project, model id

## Blocked on

Owner confirmation of the **log-write failure policy** (D0.2). Dispatch is
blocked until the owner records the choice in plan.md "Owner decisions
2026-11-07" and in this doc's Implementation notes; then `status` becomes
`planned`. If the developer finds the choice unrecorded, they stop and
report.

## Owner decisions this sprint encodes (2026-11-07)

ULID session ids (breaking, pre-1.0); `request_sha256` is the SHA-256 of
exactly what is sent and the future cache key; `repo` is the main
repository (a worktree resolves to it), else the calling folder; `project`
is free caller text; context on every line; `model_id` is the catalog model
string; SHA-256 via `sc-sha`; the row lives in the sc-observability v2
`fields{}` object with `send_outcome` (not `outcome`); `sc-agent-http`
hashes the body at the send site; `model_id` comes from a provided
`JudgeBackend` method; a failed send logs `model_id` always and
`request_sha256` NULL when no body was built. The row schema and DDL are
r-7's (ADR-SAG-0031 "Storage schema"); this sprint only wires them.

## Outcome

Every Jev and LLM send (`judge` and `ask`, `Ok` or `Failed`) writes one v2
log line whose `fields{}` object is one ADR-SAG-0031 row, so a line can be
found and filtered by session, repository, project, model or request digest
without reading any other line, and loads into the r-7 SQLite schema. Log
and session files stay where ADR-SAG-0021 puts them.

## Pre-flight (D0, before any code)

1. **sc-sha raw-bytes gate.** `sc-sha` on crates.io describes itself as
   "deterministic text and resolved-template composition SHA-256
   identities", so normalization is a real risk. Confirm it exposes a SHA-256
   over raw bytes with no normalization (line endings, Unicode, trailing
   whitespace, BOM). Prove it with `test_sha256_reference_vectors`
   (`crates/sc-agent-types/tests/ids.rs`): the empty input gives
   `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`, a
   CRLF/trailing-space byte string gives the `sha2` value, and the
   `jev_cross_impl` fixture bytes give the committed reference digest.
   Record the `sc-sha` version and API used in "Implementation notes → D0".
   If `sc-sha` only offers a normalizing text hash, **stop and report**
   (owner decision: keep `sha2`, or add a raw-bytes API to `sc-sha` first).
2. **Log-write failure policy — owner confirmation gate.** Today a failed
   log write never fails the run: `SessionLog::emit` errors are best effort
   and a logger flush failure is one entry in `data.warnings` (CR-014,
   routed by r-3b's `run_from`); the run succeeds. The owner chooses before
   dispatch:
   - **(a) keep today's policy** (recommended): pinned by
     `test_rejected_log_row_keeps_run_ok` (new, `sc-jev-check --test
     session_log`) and `test_cli_log_flush_warning_in_data` (r-3b);
   - **(b) fail the run** on a log-write failure: then this doc is revised
     before dispatch (new error mapping, exit code, Python exception), and
     the acceptance rows change.

## Deliverables

### D1 — ULID session ids (REQ-SAG-0612, REQ-SAG-0600 amendment, ADR-SAG-0008 r-6 amendment)

- `SessionId` wraps `ulid::Ulid` (caret). `SessionId::new()` is
  `Ulid::new()` (time plus 80 random bits; no shared generator state, so no
  lock). `Display`/`FromStr`/serde use the 26-character Crockford base32
  form. `from_uuid`/`as_uuid` are deleted.
- Session file names, session documents, the CLI `--session-id` value,
  stdout `session_id` and Python session ids are ULIDs. A UUID (or any
  non-ULID) id is `INPUT_INVALID` with the fix hint "session ids are ULIDs
  since Phase A.1; start a new session with `sc-ai jev run`" (CLI) or "…
  with `run_check`" (Python). No migration of old files; they expire by TTL.
- Session document version 2 → **3** (adds `decision_schema`,
  ADR-SAG-0030). A v2 file is `SessionDocumentInvalid` (`INPUT_INVALID`)
  with "re-run the check" recovery, as r-1 did for v1. Mode 0600, expiry
  from `created_at`, the written-once and no-lock rules (REQ-SAG-0607) are
  unchanged.

### D2 — SHA-256 via sc-sha (REQ-SAG-0618)

- `Sha256Digest::of` computes through `sc-sha` (raw bytes, per D0.1).
  `sha2` and `uuid` leave `sc-agent-types`; `uuid` leaves `sc-ai`; both
  leave `[workspace.dependencies]` and `Cargo.lock` when nothing else uses
  them. `sc-lint.toml` records `ulid` and `sc-sha` as owned by
  `crates/sc-agent-types` only. Both are core (always-on) dependencies
  because `SessionId` and `Sha256Digest` are core types (ADR-SAG-0027 r-6
  amendment); `tests/fixtures/no-default-features-deps.txt` is updated.
- Unchanged digests, proven: `prompt_sha256`, the cross-impl fixture,
  `input_sha256` recovery and `sc-treesitter` `query_sha256`.

### D3 — Send trace: request_sha256 at the send site (REQ-SAG-0613)

- `sc_agent_types::SendTrace` (core, next to `CancelToken`): a cloneable
  handle over `Arc<OnceLock<Sha256Digest>>` with `new()`,
  `record(&self, Sha256Digest)` (first write wins) and
  `request_sha256() -> Option<Sha256Digest>`.
- `sc_decision_schema::JudgeRequest::send_trace()` / `with_send_trace(..)`
  (default: a fresh, empty trace). The engine creates one per `judge` and
  per `ask` call and reads it after the call returns, `Ok` or `Err`.
- `sc-agent-http`: `HttpRequest::with_trace(SendTrace)`;
  `send_with_busy_retry` (and so `send_and_parse`, the one exchange every
  backend uses) records `Sha256Digest::of(request.body())` immediately
  before the first `Transport::send`. Retries resend the same bytes, so one
  value per send. Headers are not hashed (no secret is hashed, ADR-SAG-0020).
  A cancel before the first send records nothing.
- Backends pass the request's trace into the `HttpRequest` they build:
  `sc-jev` (`client/mod.rs`, the one `send_and_parse` call) and
  `sc-jev-backends` (`backend.rs`). This does not depend on r-5's body
  change: whatever body r-5 builds is hashed where it is sent.
- If a send fails before a body is built (key or encode failure inside
  `judge`), the trace is empty and the row's `request_sha256` is `null`.

### D4 — Model id (REQ-SAG-0616)

- `JudgeBackend::model_id(&self) -> ModelId`, a **provided** method
  (ADR-SAG-0009: new methods have default bodies) returning
  `ModelAliases::builtin().latest(self.id().descriptor().default_model_alias())`,
  falling back to `ModelId::Other(<backend name>)` if the alias has no row.
- Overrides return the backend's configured model (r-7 D8): `JevClient`
  (`check.rs`) returns its `JevConfig` model; the Haiku and Luna backends
  (`backend.rs`) return the model they send. `MockJudgeBackend::with_model_id`
  and the scripted backends report a model too. The row's `model_id` is
  never null, `Failed` included.
- `backend` (the `BackendId`, how we called) stays a separate column.

### D5 — Repo and project context (REQ-SAG-0614, REQ-SAG-0615)

- **Resolver placement (ADR-SAG-0027 git-enabler rule).**
  `sc_agent_types::git::discover_common_dir(start)` (behind the existing
  `git` feature, in `git.rs`, reusing its private argv-only runner, so the
  spawn site is the existing `#[expect(clippy::disallowed_methods)]` site in
  `crates/sc-agent-types/src/git.rs` and the ADR-SAG-0028 source scan's
  expect-site list is unchanged) runs
  `git rev-parse --path-format=absolute --git-common-dir`.
  `sc_jev_check::discover_main_repo_root(start) -> RepoRoot`, beside
  `discover_repo_root`, returns the common dir's parent when the common dir
  is a `.git` directory inside a work tree (a linked worktree resolves to
  its main repository); otherwise (separate git dir, bare common dir) the
  current toplevel from `discover_toplevel`; outside git, `start`
  absolutized. The enablers of `git` stay `sc-jev-check` and
  `sc-treesitter`'s `cli` feature; compose calls `sc-jev-check`, not git.
- `repo` changes **only** the log field. Session and log paths keep using
  the worktree's own root (ADR-SAG-0021, `discover_repo_root`).
- `project`: `sc_agent_types::ProjectLabel` (UTF-8, ≤ 256 bytes, no control
  characters; otherwise `INPUT_INVALID`). `sc-ai` adds `--project <TEXT>` to
  `jev run` and `jev ask`; `sc-jev-py` adds `project=` to `run_check` and
  `Session.ask` (and the `.pyi`). Absent is `null`. It is caller text,
  stored as given and **not scrubbed** (documented in the CLI and Python
  docs and in ADR-SAG-0031).
- `sc-agent-compose` (`compose.rs`): `ComposeInputs::project(..)` setter;
  `run_from`/`ask_from` resolve `repo` with `discover_main_repo_root(cwd)`
  and pass `LogContext { repo, project }` through
  `RunConfig::log_context(..)`, so neither consumer builds log context.

### D6 — Emit the row in `fields{}` (REQ-SAG-0617, REQ-SAG-0611 amendment, REQ-SAG-0610)

- `sc-jev-check`'s run loop and `ask` build one
  `sc_decision_schema::session_log::SessionLogRow` per send:
  `SessionLogRow::ok(..)` with the typed answers, or
  `SessionLogRow::failed(..)` with the `ErrorCode`. `answer_id` is a fresh
  ULID; `session_created_at` is the run's session creation time;
  `decision_schema` is `DecisionSchemaId::CURRENT`; `model_id` from D4;
  `request_sha256` from D3. A backend refused before send (`NotSent`)
  writes no row (r-2b). A session-save failure writes no row of its own.
- `observability::v2::LogEvent` becomes
  `LogEvent::new(session_id, operation, fields: serde_json::Map<String, Value>)`
  with accessors `session_id()`, `operation()`, `fields()`; the
  `backend()` / `prompt_sha256()` accessors are removed (the values are in
  `fields`). The emitter passes `row.to_fields()`. This is a stable-surface
  change of `observability::v2` (ADR-SAG-0027/0016 r-6 amendments). The port
  stays in `sc-agent-types`; it does not name the row type.
- `sc-agent-compose` `obs`: the adapter copies `event.fields()` into the v2
  `fields{}` verbatim; envelope `target` `jev.session`, `action` from
  `operation`, envelope `outcome` stays `null`. No bespoke writer
  (ADR-SAG-0013).
- Golden `tests/fixtures/observability/session-event.json` is rewritten to
  the new lines (judge `ok`, judge `failed` with and without a body, ask).
- **Wording reconciled with r-2b.** r-2b's "puts no path or secret in any
  event" means no session-store path and no secret; `repo` is the one path
  column, by design (ADR-SAG-0031). The plan PR narrows r-2b's wording to
  that.

### D7 — Stale UUID / sha2 text

Fix: `README.md` (`--session-id <ULID>`), `crates/sc-ai/src/command.rs`
help and fix hint, `docs/sc-agent-types/architecture.md` (depends-on list:
`ulid`, `sc-sha`; `LogEvent` field paragraph), the crate requirement rows
for REQ-SAG-0600/0610/0611 in `docs/sc-jev-check/requirements.md` and
NFR-SAG-0003 in `docs/sc-ai/requirements.md`, and the `sc-jev-py` module
docs (log line field list). The ADR-SAG-0008 base sentence "Session files
are UUID paths" (`docs/architecture.md`, ADR-SAG-0008 body) is superseded by
the ADR-SAG-0008 r-6 amendment (ADR-SAG-0031); `docs/plans/phase-a/**` is
historical and not edited.

### D8 — ADR text flips

Flip ADR-SAG-0031's "Wiring" subsection (Status line, markers; ADR-level
Status and index Status cell become `Active (r-7 storage schema, r-6
wiring)`), and the r-6 amendment subsections in ADR-SAG-0008, 0013, 0016,
0026, 0027 with their index markers `ADR-SAG-0031 (Active, r-6)`. Record the
D0.2 policy as the wiring subsection's last bullet if (a); if (b), the doc
is revised first.

## Acceptance criteria (authoritative)

| Root | Criterion |
| --- | --- |
| req:REQ-SAG-0612 | `cargo test -p sc-agent-types --test ids -- test_session_id_is_ulid --exact` (round-trip through `Display`/`FromStr`/serde; UUID string refused); `cargo test -p sc-jev-check --features test-util --test session -- test_v2_session_document_is_invalid --exact`; `cargo test -p sc-ai --features test-util --test cli -- test_cli_uuid_session_id_is_input_invalid --exact` (one `ScAgentErrorResponse` line, `INPUT_INVALID`, fix hint mentions ULID, exit 2); `uv run --directory tests/fixtures/jev-python pytest -q tests/test_run_check_and_ask.py::test_uuid_session_id_is_input_invalid` — each prints `1 passed` |
| req:REQ-SAG-0613 | `cargo test -p sc-agent-http --features test-util --test trace -- test_request_sha256_is_digest_of_sent_body --exact`, `-- test_request_sha256_recorded_once_across_retries --exact`, `-- test_cancel_before_first_send_records_nothing --exact`; `cargo test -p sc-jev --test send_trace -- test_jev_send_records_request_sha256 --exact`; `cargo test -p sc-jev-backends --test send_trace -- test_llm_send_records_request_sha256 --exact`; `cargo test -p sc-jev-check --features test-util --test session_log -- test_failed_before_body_row_has_null_request_sha256 --exact` — each prints `1 passed` |
| req:REQ-SAG-0614 | `cargo test -p sc-jev-check --test repo -- test_main_repo_root_resolves_worktree --exact`, `-- test_main_repo_root_outside_git_is_cwd --exact`, `-- test_main_repo_root_separate_git_dir_falls_back_to_toplevel --exact`; `cargo test -p sc-agent-compose --test log_context -- test_log_context_repo_is_main_repo_paths_stay_on_worktree --exact` (run from a linked worktree with `PathPolicy::cli()`: the session file is under `<worktree>/.sc-ai/sessions`, the log under `<worktree>/.sc-ai/logs/sc-ai.log.jsonl`, and the row's `repo` is the main repository) — each prints `1 passed` |
| req:REQ-SAG-0615 | `cargo test -p sc-agent-types --test ids -- test_project_label_limits --exact`; `cargo test -p sc-ai --features test-util --test cli -- test_cli_project_flag_reaches_log --exact` and `-- test_cli_project_over_limit_is_input_invalid --exact` — each prints `1 passed`; `uv run --directory tests/fixtures/jev-python pytest -q tests/test_run_check_and_ask.py::test_project_kwarg_reaches_log tests/test_run_check_and_ask.py::test_project_kwarg_control_char_is_input_invalid` prints `2 passed` |
| req:REQ-SAG-0616 | `cargo test -p sc-decision-schema --features test-util --test mock -- test_model_id_default_from_catalog --exact`; `cargo test -p sc-jev --test send_trace -- test_jev_model_id_is_configured_model --exact`; `cargo test -p sc-jev-backends --test send_trace -- test_llm_model_id_is_configured_model --exact`; `cargo test -p sc-jev-check --features test-util --test session_log -- test_failed_row_carries_model_id --exact` — each prints `1 passed` |
| req:REQ-SAG-0617 | `cargo test -p sc-jev-check --features test-util --test session_log -- test_one_row_per_send_failed_included_not_sent_none --exact`; `cargo test -p sc-agent-compose --test obs -- test_log_line_fields_is_one_session_log_row --exact` (every line's `fields` passes `SessionLogRow::try_from_fields`, carries `send_outcome`, envelope `outcome` is `null`; matches `session-event.json`); D0.2 policy (a): `cargo test -p sc-jev-check --features test-util --test session_log -- test_rejected_log_row_keeps_run_ok --exact` and `cargo test -p sc-ai --features test-util --test cli -- test_cli_log_flush_warning_in_data --exact` — each prints `1 passed` |
| req:REQ-SAG-0618 | D0.1 proof: `cargo test -p sc-agent-types --test ids -- test_sha256_reference_vectors --exact` prints `1 passed`, result recorded in "Implementation notes → D0"; the two `cargo tree -p sc-agent-types -e normal` lines in Required validation succeed (`sc-sha` present; `sha2` and `uuid` absent); `cargo test -p workspace-tests --test jev_cross_impl` and `cargo test -p sc-treesitter` green with unchanged digests |
| req:REQ-SAG-0600 (amended) | `cargo test -p sc-jev-check --features test-util --test sessions_exact -- test_session_file_named_by_ulid_mode_0600_expiry --exact` prints `1 passed` (ULID file name, mode 0600 on Unix, expiry from `created_at`) |
| req:REQ-SAG-0611 (amended) | `cargo test -p sc-jev-check --features test-util --test session_log -- test_every_row_carries_context --exact` (every row, `judge` and `ask`, `ok` and `failed`, carries `session_id`, `repo`, `project`, `model_id`, `prompt_sha256`) and `-- test_row_has_no_body_prompt_or_key_bytes --exact` — each prints `1 passed` |
| adr:ADR-SAG-0031 | Wiring subsection Active (r-6); index row `Active (r-7 storage schema, r-6 wiring)`; r-6 subsections in 0008, 0013, 0016, 0026, 0027 and their markers Active; D0.2 policy recorded |
| docs | The `rg` UUID line in Required validation exits 0 (no stale UUID text outside historical plans); no `uuid` or `sha2` entry remains in any `Cargo.toml` (`! rg -n '^(uuid\|sha2)\b' Cargo.toml crates/*/Cargo.toml` exits 0) |
| boundary:session-log | Regression, each with its command: REQ-SAG-0607 `cargo test -p sc-jev-check --features test-util --test session -- test_session_written_once_only_with_llm --exact`; REQ-SAG-0610 covered by the 0617 row; REQ-SAG-0146 `cargo test -p sc-decision-schema --test session_log`; REQ-SAG-0102 `cargo test -p workspace-tests --test jev_cross_impl`; REQ-SAG-0104/0310 `cargo test -p sc-treesitter`; REQ-SAG-0401/0402 `cargo test -p sc-agent-http --test retry`; REQ-SAG-0606 `cargo test -p sc-jev --features test-util --test wire -- test_real_wire_builder_reports_prompt_digest --exact`; REQ-SAG-0700/0701/0711 `cargo test -p sc-ai --features test-util --test cli`; REQ-SAG-0800/0801/0802 maturin test-wheel build + `uv run --directory tests/fixtures/jev-python pytest -q` (incl. `test_jev_ask_and_missing_check_map_error_codes`: errors keep `JevError` codes); NFR-SAG-0001 `cargo test -p sc-agent-types --test errors -- test_error_response_stderr_golden --exact`; NFR-SAG-0002 `cargo test -p sc-jev-check --features test-util --test sessions_exact -- test_log_event_has_no_secrets --exact` and the 0611 row; NFR-SAG-0003 `cargo test -p sc-agent-compose --test paths` (default and configured roots unchanged) and `cargo test -p sc-agent-compose --test log_context -- test_log_and_session_paths_unchanged_with_ulid_names --exact`; ADR-SAG-0021 precedence `cargo test -p sc-agent-compose --test paths -- test_custom_path_policy_resolves --exact`; ADR-SAG-0028 spawn sites `cargo test -p workspace-tests --test source_scans` |

## Required validation

```bash
just lint
just test
cargo test -p sc-agent-types --test ids -- test_sha256_reference_vectors --exact
cargo test -p sc-agent-types --test ids -- test_session_id_is_ulid --exact
cargo test -p sc-agent-types --test ids -- test_project_label_limits --exact
cargo test -p sc-agent-types --test trace
cargo test -p sc-agent-types --features git --test git
cargo test -p sc-agent-types --test obs
cargo test -p sc-agent-types --test errors -- test_error_response_stderr_golden --exact
cargo test -p sc-decision-schema --features test-util --test mock -- test_model_id_default_from_catalog --exact
cargo test -p sc-decision-schema --test session_log
cargo test -p sc-agent-http --features test-util --test trace -- test_request_sha256_is_digest_of_sent_body --exact
cargo test -p sc-agent-http --features test-util --test trace -- test_request_sha256_recorded_once_across_retries --exact
cargo test -p sc-agent-http --features test-util --test trace -- test_cancel_before_first_send_records_nothing --exact
cargo test -p sc-agent-http --test retry
cargo test -p sc-jev --test send_trace -- test_jev_send_records_request_sha256 --exact
cargo test -p sc-jev --test send_trace -- test_jev_model_id_is_configured_model --exact
cargo test -p sc-jev --features test-util --test wire -- test_real_wire_builder_reports_prompt_digest --exact
cargo test -p sc-jev-backends --test send_trace -- test_llm_send_records_request_sha256 --exact
cargo test -p sc-jev-backends --test send_trace -- test_llm_model_id_is_configured_model --exact
cargo test -p sc-jev-check --test repo -- test_main_repo_root_resolves_worktree --exact
cargo test -p sc-jev-check --test repo -- test_main_repo_root_outside_git_is_cwd --exact
cargo test -p sc-jev-check --test repo -- test_main_repo_root_separate_git_dir_falls_back_to_toplevel --exact
cargo test -p sc-jev-check --features test-util --test session -- test_v2_session_document_is_invalid --exact
cargo test -p sc-jev-check --features test-util --test session -- test_session_written_once_only_with_llm --exact
cargo test -p sc-jev-check --features test-util --test sessions_exact -- test_session_file_named_by_ulid_mode_0600_expiry --exact
cargo test -p sc-jev-check --features test-util --test sessions_exact -- test_log_event_has_no_secrets --exact
cargo test -p sc-jev-check --features test-util --test session_log -- test_one_row_per_send_failed_included_not_sent_none --exact
cargo test -p sc-jev-check --features test-util --test session_log -- test_failed_before_body_row_has_null_request_sha256 --exact
cargo test -p sc-jev-check --features test-util --test session_log -- test_failed_row_carries_model_id --exact
cargo test -p sc-jev-check --features test-util --test session_log -- test_every_row_carries_context --exact
cargo test -p sc-jev-check --features test-util --test session_log -- test_row_has_no_body_prompt_or_key_bytes --exact
cargo test -p sc-jev-check --features test-util --test session_log -- test_rejected_log_row_keeps_run_ok --exact
cargo test -p sc-agent-compose --test obs -- test_log_line_fields_is_one_session_log_row --exact
cargo test -p sc-agent-compose --test log_context -- test_log_context_repo_is_main_repo_paths_stay_on_worktree --exact
cargo test -p sc-agent-compose --test log_context -- test_log_and_session_paths_unchanged_with_ulid_names --exact
cargo test -p sc-agent-compose --test paths
cargo test -p sc-ai --features test-util --test cli -- test_cli_uuid_session_id_is_input_invalid --exact
cargo test -p sc-ai --features test-util --test cli -- test_cli_project_flag_reaches_log --exact
cargo test -p sc-ai --features test-util --test cli -- test_cli_project_over_limit_is_input_invalid --exact
cargo test -p sc-ai --features test-util --test cli -- test_cli_log_flush_warning_in_data --exact
cargo test -p sc-ai --features test-util --test cli
cargo test -p sc-treesitter
cargo test -p workspace-tests --test jev_cross_impl
cargo test -p workspace-tests --test consumer_parity -- test_cli_and_python_data_match_golden --exact
cargo test -p workspace-tests --test source_scans
cargo tree -p sc-agent-types -e normal | grep -qE '(^|[^[:alnum:]_-])sc-sha v[0-9]'
! cargo tree -p sc-agent-types -e normal | grep -qE '(^|[^[:alnum:]_-])(sha2|uuid) v[0-9]'
! rg -n -i '\buuid\b' README.md docs --glob '!docs/plans/**'
cargo clippy --workspace --all-targets -- -D warnings
bash .github/scripts/cargo-tree-gates.sh
bash .github/scripts/check-exact-pins.sh
maturin build -m crates/sc-jev-py/Cargo.toml --features extension-module,test-util -o /tmp/sc-agent-wheels-test
uv run --directory tests/fixtures/jev-python pytest -q
bash scripts/sc-publish-dry-run.sh
```

Each named test prints `1 passed`; the scrum-master rejects `0 passed`.

## Implementation notes

- **D0.1** (sc-sha): version, API, reference-vector result. Required before
  any D2 code.
- **D0.2** (log-write failure policy): the owner's choice, date and source.
  Required before dispatch.

## This sprint does not close

- The response cache (post-MVP; `request_sha256` is its future key).
- A SQLite sink or loader (the schema is r-7's; post-A.1).
- Any change to where logs or sessions are written (ADR-SAG-0021).
- The Jev body shape (r-5, D9).
