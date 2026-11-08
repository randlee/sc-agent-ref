---
id: r-6
phase: r
status: complete # merged PR 35 (3a3add9), QA PASS qa-r6-4 b35a9c4
closure_type: contract
target_boundary: session-log
track: observability
wave: 6
size: L
branch: sprint/r-6-session-log-record
worktree: sprint/r-6-session-log-record
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
  - REQ-SAG-0401   # regression: busy retry unchanged by the digest recording
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
  - NFR-SAG-0004   # regression: ulid and sc-sha are caret workspace deps (check-exact-pins.sh)
  - REQ-SAG-0107   # regression: observability port and core deps change; every feature combination builds
  # Crate-level ids of the seven crates this sprint owns paths in. None is a
  # req: root of this sprint; their regression coverage is `just test` plus the
  # --features test-util runs named once in the boundary:session-log row.
  - REQ-SAG-0101   # sc-agent-types
  - REQ-SAG-0103
  - REQ-SAG-0120
  - REQ-SAG-0121
  - REQ-SAG-0147
  - REQ-SAG-0148
  - REQ-SAG-0505   # sc-agent-http
  - REQ-SAG-0140   # sc-decision-schema
  - REQ-SAG-0141
  - REQ-SAG-0142
  - REQ-SAG-0144
  - REQ-SAG-0145
  - REQ-SAG-0300   # sc-jev-check
  - REQ-SAG-0301
  - REQ-SAG-0302
  - REQ-SAG-0303
  - REQ-SAG-0305
  - REQ-SAG-0309
  - REQ-SAG-0311
  - REQ-SAG-0312
  - REQ-SAG-0313
  - REQ-SAG-0314
  - REQ-SAG-0503
  - REQ-SAG-0601
  - REQ-SAG-0602
  - REQ-SAG-0603
  - REQ-SAG-0620
  - REQ-SAG-0604   # sc-jev-host, sc-ai, sc-jev-py
  - REQ-SAG-0605
  - REQ-SAG-0702   # sc-ai
  - REQ-SAG-0703
  - REQ-SAG-0710
  - REQ-SAG-0712
  - REQ-SAG-0900
  - REQ-SAG-0803   # sc-jev-py
adrs:
  - ADR-SAG-0001  # new INPUT_INVALID paths (UUID session id, bad project text) are JevError constructors
  - ADR-SAG-0002  # prompt_sha256 stays the digest of the canonical bytes; sc-sha gives the same digest for them (D0)
  - ADR-SAG-0003  # r-2 amendment: git toplevel for .env and paths; the log `repo` field is a different root, defined in ADR-SAG-0031
  - ADR-SAG-0004  # a backend refused before send (NotSent) writes no row
  - ADR-SAG-0005  # sc-agent-http hashes the wire body at the send site; backends still never re-canonicalize
  - ADR-SAG-0006  # transport injection unchanged; a recording transport proves the digest
  - ADR-SAG-0007  # request_sha256 is recorded once per send, before the first attempt; retries resend the same bytes
  - ADR-SAG-0008  # r-6 amendment (ADR-SAG-0031): ULID session paths, session document v3
  - ADR-SAG-0009  # the row's model_id is JudgeBackend::model_id() (r-7 amendment)
  - ADR-SAG-0010  # --project flag on the thin CLI adapter
  - ADR-SAG-0011  # project= kwarg on the Python adapter
  - ADR-SAG-0012  # sc-jev is not edited; hashing the carried bytes in sc-agent-http is not Jev semantics
  - ADR-SAG-0013  # r-6 amendment (ADR-SAG-0031): the row lives in the v2 fields{} object; no bespoke writer
  - ADR-SAG-0014  # stdout session_id becomes a ULID; goldens updated, no new key
  - ADR-SAG-0016  # r-6 amendment (ADR-SAG-0031): LogEvent carries the row's fields
  - ADR-SAG-0019  # request_sha256 is the wire-body digest; prompt_sha256 stays the canonical-payload digest
  - ADR-SAG-0020  # hashing the body adds no secret read site; headers are not hashed
  - ADR-SAG-0021  # log and session paths unchanged (worktree root), proven by test
  - ADR-SAG-0022  # MockJudgeBackend and scripted backends record a sent-body digest into the request trace
  - ADR-SAG-0023  # new dependencies ulid and sc-sha, caret, not lockstep
  - ADR-SAG-0024  # RunResponse / FileSessionStore file names; prepare/run unchanged
  - ADR-SAG-0025  # input_sha256 attestation stays on canonical bytes; Failed emits a row
  - ADR-SAG-0026  # r-6 amendment (ADR-SAG-0031): sc-agent-http records the request digest; compose owns repo/project
  - ADR-SAG-0027  # r-6 amendment (ADR-SAG-0031): core dependencies ulid, sc-sha; LogEvent stable-surface change
  - ADR-SAG-0028  # no new spawn site: the git helper stays in sc-agent-types/src/git.rs
  - ADR-SAG-0029
  - ADR-SAG-0030  # decision schema id in the session document and the row
  - ADR-SAG-0031  # wiring subsection lands here (r-6 flips it)
owned_paths:
  - "crates/sc-agent-types/Cargo.toml#-uuid -sha2 +ulid +sc-sha"
  - "crates/sc-agent-types/src/ids.rs#SessionId and AnswerId (ULID), Sha256Digest::of_text (sc-sha)"
  - "crates/sc-agent-types/src/error.rs#test literal SessionId (ULID)"
  - "crates/sc-agent-types/src/observability/**"
  - "crates/sc-agent-types/src/git.rs#discover_common_dir (existing spawn helper)"
  - "crates/sc-agent-types/tests/ids.rs#ULID, sha256 reference vectors"
  - "crates/sc-agent-types/tests/obs.rs"
  - "crates/sc-agent-types/tests/errors.rs#ULID literals"
  - "crates/sc-agent-types/tests/git.rs#discover_common_dir cases"
  - "crates/sc-agent-types/tests/fixtures/no-default-features-deps.txt"
  - "crates/sc-decision-schema/src/test_util.rs#MockJudgeBackend::with_sent_body"
  - "crates/sc-decision-schema/tests/mock.rs#sent-body trace case"
  - "crates/sc-agent-http/src/retry.rs#record request_sha256 before the first send"
  - "crates/sc-agent-http/tests/trace.rs (new)"
  - "crates/sc-agent-http/Cargo.toml#[[test]] trace required-features = [\"test-util\"]"
  - "crates/sc-jev-check/src/run/**#session-log row emission, LogContext, per-send SendTrace"
  - "crates/sc-jev-check/src/session/**#ULID ids and file names, document v3"
  - "crates/sc-jev-check/src/registry/scripted.rs#sent-body digest recorded into the request trace"
  - "crates/sc-jev-check/src/**#discover_main_repo_root (beside discover_repo_root)"
  - "crates/sc-jev-check/Cargo.toml#[[test]] session_log, repo"
  - "crates/sc-jev-check/tests/session_log.rs (new)"
  - "crates/sc-jev-check/tests/repo.rs (new)"
  - "crates/sc-jev-check/tests/session.rs#v2 document invalid"
  - "crates/sc-jev-check/tests/sessions_exact.rs#ULID names, rows without secrets"
  - "crates/sc-jev-host/src/obs.rs"
  - "crates/sc-jev-host/src/compose.rs#ComposeInputs::project, repo resolution (the file defining ComposeInputs)"
  - "crates/sc-jev-host/tests/obs.rs"
  - "crates/sc-jev-host/tests/log_context.rs (new)"
  - "crates/sc-ai/Cargo.toml#-uuid"
  - "crates/sc-ai/src/**#--project flag, ULID --session-id, help text"
  - "crates/sc-ai/tests/cli.rs#--project and ULID cases"
  - "crates/sc-jev-py/src/**#project kwarg, ULID session ids"
  - "crates/sc-jev-py/sc_jev_py.pyi#project kwarg"
  - "tests/fixtures/jev-python/tests/test_run_check_and_ask.py#project and ULID cases"
  - "tests/fixtures/observability/**"
  - "tests/fixtures/decision-schema/session-log/fields.jsonl#session_id and answer_id values (ULID)"
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
  - "docs/sc-agent-types/**#ULID, sc-sha, LogEvent fields"
  - "docs/sc-agent-http/**#digest recording"
  - "docs/sc-jev-check/**#REQ-SAG-0600, 0610, 0611 rows; row emission; discover_main_repo_root"
  - "docs/sc-jev-host/**#log context (repo, project)"
  - "docs/sc-ai/**#--project; NFR-SAG-0003 row"
  - "docs/sc-jev-py/**#project kwarg"
  - "crates/sc-agent-http/Cargo.toml#[[test]] trace (QA fix: listed for completeness)"
  - "crates/sc-agent-types/tests/features.rs#core dependency set and module doc (QA fix)"
  - "crates/sc-agent-types/tests/fixtures/error-codes.json#nil ULID session id (QA fix)"
  - "crates/sc-ai/tests/argv.rs#ULID session id cases (QA fix)"
  - "crates/sc-jev-check/tests/run_response.rs#ULID session id (QA fix)"
  - "crates/sc-jev-check/tests/fixtures/run-response/*.json#ULID session id (QA fix)"
  - "crates/sc-jev-check/tests/multi_backend.rs#LogEvent fields accessors (QA fix)"
  - "boundaries/sc-agent-types/agent-types.toml#allowed_dependencies, notes (QA fix)"
  - "boundaries/sc-jev-check/jev-check.toml#facade, notes (QA fix)"
---

# Sprint r-6 — Session log wiring: ULID, sc-sha, request digest, repo, project

## Outcome

Every Jev and LLM send (`judge` and `ask`, `Ok` or `Failed`) writes one v2
log line whose `fields{}` object is one ADR-SAG-0031 row, so a line can be
found and filtered by session, repository, project, model or request digest
without reading any other line, and loads into the r-7 SQLite schema. Log
and session files stay where ADR-SAG-0021 puts them. Owner decisions:
plan.md "Owner decisions 2026-11-07", items 5, 6, 8 and 9. The row schema, the
model catalog, `JudgeBackend::model_id` and the `SendTrace` hand-off are
r-7's; this sprint wires them.

## sc-sha behaviour (D0, settled)

`sc-sha` 1.7.0 has one hashing API:
`sc_sha::calculate_hash(sc_sha::HashInput::TextFileBytes { utf8_file_bytes })`
returns `HashResult::Template(TemplateSha256)`. It requires strict UTF-8
(`ShaError::InvalidUtf8` otherwise) and normalizes CRLF and bare CR to LF
before hashing; there is no raw-bytes API. Every input sc-agent hashes is
UTF-8 serde_json output or text (`Sha256Digest::of_text` takes `&str` from r-7,
so the UTF-8 error cannot occur). For request bodies and canonical payloads
(compact JSON, no raw CR) the digest equals SHA-256 of the bytes sent; that
claim is made for them only. `query_sha256` identifies the LF-normalized
query text (unchanged for LF-only queries). `sc-sha` depends on `sha2`, so
`sha2` stays in the lock file transitively. Pinned by one unit test,
`test_sha256_reference_vectors` (`crates/sc-agent-types/tests/ids.rs`): the
empty string gives
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` and the
`jev_cross_impl` fixture body gives its committed reference digest.

## Deliverables

Every deliverable below lands production-ready for the scope this sprint claims; none closes as shape-only or test-only.

### D1 — ULID session ids (REQ-SAG-0612, REQ-SAG-0600 amendment, ADR-SAG-0008 r-6 amendment)

- `SessionId` wraps `ulid::Ulid` (caret). `SessionId::new()` is
  `Ulid::new()` (time plus 80 random bits; no shared generator state, so no
  lock). `Display`/`FromStr`/serde use the 26-character Crockford base32
  form. `from_uuid`/`as_uuid` are deleted.
- Session file names, session documents, the CLI `--session-id` value,
  stdout `session_id` and Python session ids are ULIDs. A UUID (or any
  non-ULID) id is `INPUT_INVALID` with the fix hint "session ids are ULIDs
  since Phase A.1; start a new session with `sc-ai jev run`". The refusal and
  its hint are **CLI-only**: Python only ever receives ULID session ids
  (`CheckResult.session_id`, `Session.session_id`) and has no UUID input path,
  so no Python hint exists. No migration of old files; they expire by TTL.
- Session document version 2 → **3** (adds `decision_schema`,
  ADR-SAG-0030). A v2 file is `SessionDocumentInvalid` (`INPUT_INVALID`)
  with "re-run the check" recovery, as r-1 did for v1. Mode 0600, expiry
  from `created_at`, the written-once and no-lock rules (REQ-SAG-0607) are
  unchanged.

### D2 — SHA-256 via sc-sha (REQ-SAG-0618)

- `Sha256Digest::of_text(&str)` computes through `sc-sha` `calculate_hash`
  (D0); its rustdoc states the LF normalization. `calculate_hash`'s
  `InvalidUtf8` arm cannot be reached from a `&str` and is `unreachable!`
  (a bug, never input-driven). `AnswerId` wraps `ulid::Ulid`.
  `sha2` and `uuid` leave `sc-agent-types` and `uuid` leaves `sc-ai`, so no
  workspace manifest lists them (`[workspace.dependencies]` included);
  `Cargo.lock` keeps them only if a third-party dependency still pulls them
  in. `sc-lint.toml` records `ulid` and `sc-sha` as owned by
  `crates/sc-agent-types` only. Both are core (always-on) dependencies
  because `SessionId` and `Sha256Digest` are core types (ADR-SAG-0027 r-6
  amendment); `tests/fixtures/no-default-features-deps.txt` is updated.
- Unchanged digests, proven: `prompt_sha256`, the cross-impl fixture,
  `input_sha256` recovery and `sc-treesitter` `query_sha256` for LF-only
  queries.

### D3 — Record request_sha256 at the send site (REQ-SAG-0613)

- r-7 left the seam in place: `SendTrace`, the per-send `trace` argument of
  `JudgeBackend::judge`, the required `trace: SendTrace` parameter of
  `send_with_busy_retry` / `send_and_parse` (filled by `sc-jev` and
  `sc-jev-backends` with `trace.clone()`), and a text `HttpRequest` body.
  r-6 adds the recording: `send_with_busy_retry` (and so `send_and_parse`,
  the one exchange every backend uses) calls
  `trace.record(Sha256Digest::of_text(request.body()))` immediately before
  the first `Transport::send`; no branch. Retries resend the same bytes, so one value per send.
  Headers are not hashed (no secret is hashed, ADR-SAG-0020). A cancel
  before the first send records nothing. No `sc-jev` or `sc-jev-backends`
  file changes, so this does not touch r-5's files.
- The engine creates a fresh `SendTrace` per backend send (one per row),
  never per run and never shared through a cloned `JudgeRequest`: in a
  multi-backend run (r-2b) each backend's `judge(req, &trace)` call gets its
  own `SendTrace::new()`, and so does each `ask`.
  The engine reads that trace after the call returns, `Ok` or `Err`. If a send fails before a body is built (key or
  encode failure inside `judge`), the trace is empty and the row's
  `request_sha256` is `null`.
- `MockJudgeBackend::with_sent_body(body)` and the scripted backends record
  `Sha256Digest::of_text(body)` into the send's trace, so engine tests cover
  both the set and the `null` case.

### D4 — Model id in the row (REQ-SAG-0616)

- The row's `model_id` is `JudgeBackend::model_id()` (r-7 D7.5: catalog
  default, overridden by each backend's configured model). It is never
  null, `Failed` included. `backend` (the `BackendId`, how we called) stays
  a separate column.

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
- `project`: r-7's `sc_agent_types::ProjectLabel` (UTF-8, ≤ 256 bytes, no
  control characters; otherwise `INPUT_INVALID`). `sc-ai` adds `--project <TEXT>` to
  `jev run` and `jev ask`; `sc-jev-py` adds `project=` to `run_check` and
  `Session.ask` (and the `.pyi`). Absent is `null`. It is per call (two
  sends of one session may differ; r-7 keeps `project` on `answers`), caller
  text, stored as given and **not scrubbed** (documented in the CLI and
  Python docs and in ADR-SAG-0031).
- `sc-jev-host` (`compose.rs`): `ComposeInputs::project(..)` setter;
  `run_from`/`ask_from` resolve `repo` with `discover_main_repo_root`, starting
  from the explicit repo root when one is set (`--repo-root`), else from cwd; a
  dry run skips git and uses the resolved root. They pass `LogContext { repo, project }` through
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
- `sc-jev-host` `obs`: the adapter copies `event.fields()` into the v2
  `fields{}` verbatim; envelope `target` `jev.session`, `action` from
  `operation`, envelope `outcome` stays `null`. No bespoke writer
  (ADR-SAG-0013).
- Golden `tests/fixtures/observability/session-event.json` is rewritten to
  the new lines (judge `ok`, judge `failed` with and without a body, ask).
- Session-log fixture: `tests/fixtures/decision-schema/session-log/fields.jsonl`
  (r-7) gets ULID `session_id` and `answer_id` values; its rows are
  otherwise unchanged.
- **Log-write failure policy** (plan.md "Owner decisions 2026-11-07" item
  9): a rejected or failed log row never fails the run; `SessionLog::emit`
  stays best effort and a logger flush failure stays one `data.warnings`
  entry (r-3b). Pinned by `test_rejected_log_row_keeps_run_ok` (new).
- **Wording reconciled with r-2b.** r-2b's "puts no path or secret in any
  event" means no session-store path and no secret; `repo` is the one path
  column, by design (ADR-SAG-0031). The plan PR narrowed r-2b's wording to
  that.

### D7 — Stale UUID / sha2 text (doc edits)

Edit: `README.md` (`--session-id <ULID>`), `crates/sc-ai/src/command.rs`
help, fix hint and its in-file unit test (it asserts `ULID` in the hint),
`docs/sc-agent-types/architecture.md` (depends-on list: `ulid`, `sc-sha`;
`LogEvent` field paragraph), the crate requirement rows for
REQ-SAG-0600/0610/0611 in `docs/sc-jev-check/requirements.md` and
NFR-SAG-0003 in `docs/sc-ai/requirements.md`, and the `sc-jev-py` module
docs (log line field list). Statements that a UUID id is refused stay. The
ADR-SAG-0008 base sentence "Session files are UUID paths" is superseded by
the ADR-SAG-0008 r-6 amendment (ADR-SAG-0031) and not edited;
`docs/plans/phase-a/**` is historical and not edited.

### D8 — ADR text flips

Flip ADR-SAG-0031's "Wiring" subsection (Status line, markers; ADR-level
Status and index Status cell become `Active (r-7 storage schema, r-6
wiring)`), and the r-6 amendment subsections in ADR-SAG-0008, 0013, 0016,
0026, 0027 with their index markers `ADR-SAG-0031 (Active, r-6)`.

## Explicit code samples (signatures are binding; bodies are not)

```rust
// sc-agent-types (core)
pub struct SessionId(Ulid);                       // Display/FromStr/serde: 26-char Crockford base32
impl SessionId { pub fn new() -> Self; }          // Ulid::new(); no shared generator state
impl FromStr for SessionId { type Err = JevError; /* non-ULID (incl. UUID) -> INPUT_INVALID, ULID fix hint */ }

impl Sha256Digest { pub fn of_text(text: &str) -> Self; }   // sc_sha::calculate_hash(HashInput::TextFileBytes { .. }); CR normalized to LF (D0)

// observability::v2 (stable-surface change, ADR-SAG-0016/0027 r-6)
pub struct LogEvent { /* private: session_id, operation, fields */ }
impl LogEvent {
    pub fn new(session_id: SessionId, operation: Operation,
               fields: serde_json::Map<String, serde_json::Value>) -> Self;
    pub fn session_id(&self) -> &SessionId;
    pub fn operation(&self) -> Operation;
    pub fn fields(&self) -> &serde_json::Map<String, serde_json::Value>;
}

// git.rs (feature git; existing argv-only runner and expect site)
pub fn discover_common_dir(start: &Path) -> Result<Option<PathBuf>, JevError>; // None outside git

// sc-agent-http send_with_busy_retry (trace is r-7's required parameter),
// immediately before the first Transport::send:
//     trace.record(Sha256Digest::of_text(request.body()));   // body() -> &str (r-7)

// sc-decision-schema test_util (feature test-util)
impl MockJudgeBackend {
    #[must_use]
    pub fn with_sent_body(self, body: impl Into<String>) -> Self; // records Sha256Digest::of_text(&body) into the send's trace
}

// sc-jev-check
pub fn discover_main_repo_root(start: &Path) -> Result<RepoRoot, JevError>;
#[derive(Debug, Clone)]
pub struct LogContext { /* private: repo: RepoRoot, project: Option<ProjectLabel> */ }
impl LogContext {
    pub fn new(repo: RepoRoot, project: Option<ProjectLabel>) -> Self;
    pub fn repo(&self) -> &RepoRoot;
    pub fn project(&self) -> Option<&ProjectLabel>;
}
impl RunConfig {
    #[must_use]
    pub fn log_context(self, context: LogContext) -> Self; // unset: repo = discover_main_repo_root(repo_root), project = None
}

// sc-jev-host
impl ComposeInputs {
    #[must_use]
    pub fn project(self, project: ProjectLabel) -> Self;
}
```

The row is built with r-7's `RowContext` and `SessionLogRow::ok` /
`SessionLogRow::failed`; `repo` is the `RepoRoot` path as text
(`to_string_lossy`; a non-UTF-8 byte is replaced, documented in
`docs/sc-jev-check`), `answer_id` is `Ulid::new().to_string()`.

## Acceptance criteria (authoritative; every command here is also run)

Each named test prints `1 passed`; the scrum-master rejects `0 passed`.

| Root | Criterion |
| --- | --- |
| req:REQ-SAG-0612 | `cargo test -p sc-agent-types --test ids -- test_session_id_is_ulid --exact` (round-trip through `Display`/`FromStr`/serde; UUID string refused); `cargo test -p sc-jev-check --features test-util --test session -- test_v2_session_document_is_invalid --exact`; `cargo test -p sc-ai --features test-util --test cli -- test_cli_uuid_session_id_is_input_invalid --exact` (one `ScAgentErrorResponse` line, `INPUT_INVALID`, fix hint mentions ULID, exit 2); `uv run --directory tests/fixtures/jev-python pytest -q tests/test_run_check_and_ask.py::test_session_ids_are_ulids` (Python only receives ULID ids; the UUID refusal is CLI-only) |
| req:REQ-SAG-0613 | `cargo test -p sc-agent-http --features test-util --test trace -- test_request_sha256_is_digest_of_sent_body --exact`; `cargo test -p sc-agent-http --features test-util --test trace -- test_request_sha256_recorded_once_across_retries --exact`; `cargo test -p sc-agent-http --features test-util --test trace -- test_cancel_before_first_send_records_nothing --exact`; `cargo test -p sc-jev-check --features test-util --test session_log -- test_ok_row_request_sha256_is_sent_body_digest --exact`; `cargo test -p sc-jev-check --features test-util --test session_log -- test_failed_before_body_row_has_null_request_sha256 --exact`; `cargo test -p sc-jev-check --features test-util --test session_log -- test_multi_backend_rows_carry_own_request_sha256 --exact` (two scripted backends send different bodies; each row carries its own backend's digest). The backend hand-off is r-7's required `SendTrace` parameter (compiler-enforced) |
| req:REQ-SAG-0614 | `cargo test -p sc-jev-check --test repo -- test_main_repo_root_resolves_worktree --exact`; `cargo test -p sc-jev-check --test repo -- test_main_repo_root_outside_git_is_cwd --exact`; `cargo test -p sc-jev-check --test repo -- test_main_repo_root_separate_git_dir_falls_back_to_toplevel --exact`; `cargo test -p sc-jev-host --test log_context -- test_log_context_repo_is_main_repo_paths_stay_on_worktree --exact` (run from a linked worktree with `PathPolicy::cli()`: the session file is under `<worktree>/.sc-ai/sessions`, the log under `<worktree>/.sc-ai/logs/sc-ai.log.jsonl`, and the row's `repo` is the main repository) |
| req:REQ-SAG-0615 | `cargo test -p sc-ai --features test-util --test cli -- test_cli_project_flag_reaches_log --exact`; `cargo test -p sc-ai --features test-util --test cli -- test_cli_project_over_limit_is_input_invalid --exact`; `uv run --directory tests/fixtures/jev-python pytest -q tests/test_run_check_and_ask.py::test_project_kwarg_reaches_log tests/test_run_check_and_ask.py::test_project_kwarg_control_char_is_input_invalid` prints `2 passed` |
| req:REQ-SAG-0616 | `cargo test -p sc-jev-check --features test-util --test session_log -- test_failed_row_carries_model_id --exact` (the row's `model_id` is the backend's `model_id()`, `Failed` included); r-7's `model_id()` tests are regression |
| req:REQ-SAG-0617 | `cargo test -p sc-jev-check --features test-util --test session_log -- test_one_row_per_send_failed_included_not_sent_none --exact`; `cargo test -p sc-jev-host --test obs -- test_log_line_fields_is_one_session_log_row --exact` (every line's `fields` passes `SessionLogRow::try_from_fields`, carries `send_outcome`, envelope `outcome` is `null`; matches `session-event.json`); log-write policy (D6): `cargo test -p sc-jev-check --features test-util --test session_log -- test_rejected_log_row_keeps_run_ok --exact` |
| req:REQ-SAG-0618 | D0: `cargo test -p sc-agent-types --test ids -- test_sha256_reference_vectors --exact`; `cargo tree -p sc-agent-types -e normal --depth 1 \| grep -qE '(^\|[^[:alnum:]_-])sc-sha v[0-9]'` succeeds and `! cargo tree -p sc-agent-types -e normal --depth 1 \| grep -qE '(^\|[^[:alnum:]_-])(sha2\|uuid) v[0-9]'` succeeds; `cargo test -p workspace-tests --test jev_cross_impl` and `cargo test -p sc-treesitter` green with unchanged digests |
| req:REQ-SAG-0600 (amended) | `cargo test -p sc-jev-check --features test-util --test sessions_exact -- test_session_file_named_by_ulid_mode_0600_expiry --exact` (ULID file name, mode 0600 on Unix, expiry from `created_at`) |
| req:REQ-SAG-0611 (amended) | `cargo test -p sc-jev-check --features test-util --test session_log -- test_every_row_carries_context --exact` (every row, `judge` and `ask`, `ok` and `failed`, carries `session_id`, `repo`, `project`, `model_id`, `prompt_sha256`); `cargo test -p sc-jev-check --features test-util --test session_log -- test_row_has_no_body_prompt_or_key_bytes --exact` |
| adr:ADR-SAG-0031 | Wiring subsection Active (r-6); index row `Active (r-7 storage schema, r-6 wiring)`; r-6 subsections in 0008, 0013, 0016, 0026, 0027 and their markers Active |
| boundary:session-log | Regression: `just test` plus the `required-features` runs `cargo test -p sc-jev-check --features test-util`, `cargo test -p sc-ai --features test-util`, `cargo test -p sc-jev --features test-util`, `cargo test -p sc-agent-http --features test-util`, `cargo test -p sc-decision-schema --features test-util`; `cargo test -p sc-jev-host --test log_context -- test_log_and_session_paths_unchanged_with_ulid_names --exact` (NFR-SAG-0003, ADR-SAG-0021); the crate-level ids added to `requirements:` (REQ-SAG-0101 … 0900) have no command of their own: their regression coverage is `just test` plus the `--features test-util` runs above; `cargo test -p sc-jev-host --test log_context -- test_log_context_explicit_repo_root_starts_discovery --exact` (D5 explicit-root rule); NFR-SAG-0004 `bash .github/scripts/check-exact-pins.sh`; REQ-SAG-0107 `cargo hack check -p sc-agent-types --feature-powerset --no-dev-deps` |

## Required validation

Every command in the acceptance table, then:

```bash
just lint
just test
bash .github/scripts/cargo-tree-gates.sh
bash .github/scripts/check-exact-pins.sh
bash scripts/sc-publish-dry-run.sh
```

## This sprint does not close

- The response cache (post-MVP; `request_sha256` is its future key).
- A SQLite sink or loader (the schema is r-7's; post-A.1).
- Any change to where logs or sessions are written (ADR-SAG-0021).
- The Jev body shape (r-5, D9).

## Implementation notes

Choices where the sprint doc is silent (all conservative):

- `sc-sha` is pinned to 1.7.0 in `Cargo.lock` (`cargo update -p sc-sha
  --precise 1.7.0`) and declared `sc-sha = "1.7.0"` (caret) in
  `[workspace.dependencies]`, because D0 describes 1.7.0; 1.8.0 resolved first.
  `sha2` 0.10 stays in the lock transitively; `uuid` is gone.
- `SessionId` and `AnswerId` parse the strict canonical text only: after
  `Ulid::from_string`, the re-rendered text must equal the input. That
  refuses lowercase, the `I`/`L`/`O` aliases and values above `7ZZ...`
  (`Ulid::from_string` wraps them), keeps the r-7 `AnswerId` rule, and gives
  one id one file name on case-insensitive file systems. Both are `Copy`.
  `SessionId` derives `PartialOrd`/`Ord`.
- The `SessionId` parse message is generic ("session ids are ULIDs since
  Phase A.1"). The CLI value parser (`sc-ai/src/cli.rs`) replaces it with the
  D1 hint "... start a new session with `sc-ai jev run`" as an
  `Operational { Io }` error (`INPUT_INVALID`, fix hint verbatim). Python has
  no entry point that takes a session id, so the "... with `run_check`"
  wording is unreachable and not added; `test_session_ids_are_ulids`
  checks that Python only ever sees 26-character ULID ids.
- `StoredSession` gets no `decision_schema` field (it is a public struct built
  by literal in tests). The v3 document always writes
  `DecisionSchemaId::CURRENT`; reading a document with an unknown id fails
  serde like any bad field. An `ask` row uses `CURRENT`.
- The v1/v2 recovery hint now reads "re-run the check; session files from
  before Phase A.1 cannot be resumed" (it said "Phase A session files").
- `session_created_at` is captured once at run start (`FileSessionStore::now()`
  when a store exists, else `SystemTime::now()`) and written both to the
  document `created_at` and to every row of the run. `ts` is taken when each
  send finishes.
- `RunConfig::log_context` unset resolves `repo` at the first send with
  `discover_main_repo_root(config repo root)`, falling back to the root as
  given if discovery fails (logging never fails a run). `LogContext` is
  exported from `sc_jev_check::run`.
- Compose starts `discover_main_repo_root` from the explicit repo root when
  one is set, else the working directory, so `--repo-root` and the logged
  `repo` agree; the plan names the working directory only. A dry run skips git
  and uses the resolved root. `ComposeInputs::project` is not carried into
  `sc-jev-py`'s live-session inputs: each `ask` names its own project.
- `--project` is on `RunArgs`, so `jev dry-run` accepts it too (it logs
  nothing). `Session.ask` and `CheckResult.ask` both take `project=`;
  `run_check_scripted` takes it as well.
- Scripted backends (and `MockJudgeBackend::with_sent_body`) record the digest
  of `{"scripted":"<backend id>"}` as their sent body; a refusing mock never
  records. A mock with `with_failure` and a sent body records before it fails
  (a send that failed after the body was built).
- `discover_main_repo_root` takes the parent of the git common dir only when
  the common dir is named `.git`; any other name (separate git dir, bare
  repository) falls back to `discover_repo_root`.
- Tests outside the listed owned paths that carry session-id literals were
  updated: `crates/sc-ai/tests/argv.rs`,
  `crates/sc-jev-check/tests/run_response.rs` and its two
  `tests/fixtures/run-response/*.json`, `crates/sc-jev-check/src/run/tests.rs`,
  `crates/sc-jev-check/tests/multi_backend.rs`,
  `crates/sc-agent-types/tests/features.rs`,
  `crates/sc-agent-types/tests/fixtures/error-codes.json`.
- `boundaries/sc-agent-types/agent-types.toml` (allowed dependencies, notes)
  and `boundaries/sc-jev-check/jev-check.toml` (facade, notes) are kept in
  sync with the code.
- QA-1 fixes (PR 35): the requirements list names the crate-level ids of the
  seven owned crates, covered by `just test` and the `--features test-util`
  runs; D1 states the UUID refusal is CLI-only and the pytest is
  `test_session_ids_are_ulids`; the Python fixture `git init` goes through one
  isolated, 60 s bounded helper; the GIL test has no worker deadline of its
  own; `log_context` asserts empty warnings; `FileSessionStore::new` sweeps
  expired `*.json` documents (v1/v2 included) against the default 24 h TTL plus one
  TTL of grace (a deviation from "same cutoff as is_expired": with no grace
  `SESSION_EXPIRED` could never reach a CLI user, because every command opens
  the store first; `with_ttl` set later does not change the sweep); `HostRun::run` and
  `ask_from` flush the log on the error path, and a flush failure there is
  appended once to the error detail (first text field), because no response can
  carry a warning; the adapter sets envelope `correlation_id` to the session id
  and `request_id` to the answer id (built through serde, since
  `sc-observability-types` is not a direct dependency) and logs failed sends at
  `Warn`; D5 states the explicit-root rule and a test covers it.
- Release input: no `release/` change. `Cargo.lock` now has `ulid`, `rand`
  0.9 and `sc-sha`; the parent runs `install.py` if the kit input needs a
  refresh.

- Status stayed `in-review` through QA; the coordinator flipped it to
  `complete` once follow-up QA left no open finding. Finding closure is
  recorded by the quality-mgr report on PR 35, not by the triage records
  (triaging-findings, Closure Rules).
