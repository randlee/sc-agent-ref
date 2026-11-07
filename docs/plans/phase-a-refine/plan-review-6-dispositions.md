# Phase A.1 plan review round 6 — dispositions (r-6 pre-screen; r-7 split)

Input: req-qa and arch-qa pre-screens of the r-6 draft on
`plan/phase-a-refine-r6` (off `integrate/phase-a` @ `1965b49`),
`.cursor/phase-a-orchestration/refine-plan/r6-prescreen/{req-qa,arch-qa}.json`,
and the owner decisions of 2026-11-07 (`owner_decisions_2026_11_07`, incl.
`sc_decision_schema` and `sc_decision_schema.resolved`).

The owner decisions split the draft: the decision schema, the session-log
SQLite DDL and its `fields{}` projection, and the model catalog move to a new
sprint **r-7** (`sprint-r-7-decision-schema.md`, wave 5); r-6 becomes
"session log wiring" (wave 6). Unlike rounds 4–5, this round edits
`docs/architecture.md`: ADR-SAG-0030 and ADR-SAG-0031 are inserted as
Proposed with their index rows, Amended-by markers and amendment
subsections, per the plan-time convention (the earlier "exception" wording is
dropped). Abbreviations: `plan` = `plan.md`, `r-N` = that sprint doc,
`arch` = `docs/architecture.md`, `reqs` = `docs/requirements.md`.

## Dispositions

| id | Disposition | Where |
| --- | --- | --- |
| R6-001 (Blocking, requirements not listed) | Applied. r-6 lists REQ-SAG-0607, 0700, 0701, 0800, 0801 (plus 0802, 0711, 0102, 0104, 0310, 0401, 0402, 0606, 0146, NFR-SAG-0001), each with a named regression command in `boundary:session-log`. Crate rows in `docs/sc-jev-check`, `docs/sc-ai`, `docs/sc-jev-py` are in `owned_paths` | r-6 `requirements`, `owned_paths`, acceptance `boundary:session-log` |
| R6-002 (Blocking, ids without acceptance rows) | Applied. New rows: `req:REQ-SAG-0600 (amended)` (`test_session_file_named_by_ulid_mode_0600_expiry`), `req:REQ-SAG-0611 (amended)` (`test_every_row_carries_context`, every line incl. `ask`), REQ-SAG-0610 via the 0617 row (`test_one_row_per_send_failed_included_not_sent_none`), NFR-SAG-0003 (`--test paths` + `test_log_and_session_paths_unchanged_with_ulid_names`), NFR-SAG-0002 (`test_log_event_has_no_secrets` + `test_row_has_no_body_prompt_or_key_bytes`) | r-6 acceptance |
| R6-003 (Blocking, plan REQ closure and amendments) | Applied. REQ-closure rows for r-7 (0140–0148) and r-6 (0612–0618); amendments paragraph gains 0103, 0107, 0130 (r-7) and 0600, 0611 (r-6). In `reqs` the new ids moved from the amendments table to the Phase A.1 requirements table | plan "REQ closure"; `reqs` |
| R6-004 (Blocking, log-write failure policy silent default) | Applied. D0.2 is an owner-confirmation gate: r-6 `status: blocked`, "Blocked on" section, dispatch blocked until recorded in plan "Owner decisions 2026-11-07" item 9 and r-6 "Implementation notes" (section added). Policy (a) is pinned by `test_rejected_log_row_keeps_run_ok` and `test_cli_log_flush_warning_in_data`; (b) requires a doc revision before dispatch. ADR-SAG-0031 "Wiring" carries the pending bullet | r-6 frontmatter, "Blocked on", D0.2, acceptance 0617, Implementation notes; plan; arch ADR-SAG-0031 |
| R6-005 (Important, failed-before-body nullability) | Applied (per owner decision 6). `request_sha256` is nullable (NULL when no body was built); `model_id` is NOT NULL always; CHECK constraints tie `error_code`/`answers_json` to `send_outcome`. Golden has a failed-before-body line; `test_failed_before_body_row_has_null_request_sha256`, `test_failed_row_carries_model_id` | r-7 D7 (DDL, fixture); r-6 D3, D4, acceptance 0613/0616; arch ADR-SAG-0031 "Storage schema" nullability bullet |
| R6-006 (Important, rows without commands) | Applied. Every r-6 row gives `cargo test -p <crate> [--features ..] --test <t> -- <name> --exact` or a pytest node id, expected `1 passed`. CLI and Python UUID tests added under 0612 (`test_cli_uuid_session_id_is_input_invalid`, `test_uuid_session_id_is_input_invalid`). D0 proof location: r-6 "Implementation notes → D0" plus `test_sha256_reference_vectors` | r-6 acceptance, Required validation, Implementation notes |
| R6-007 (Important, worktree paths untested) | Applied. `test_log_context_repo_is_main_repo_paths_stay_on_worktree` (`sc-jev-host --test log_context`): from a linked worktree, session and log files are under the worktree root and `repo` is the main repo. ADR-SAG-0021 precedence regression `test_custom_path_policy_resolves` | r-6 D5, acceptance 0614 and `boundary:session-log` |
| R6-008 (Important, stale UUID/sha2 text unowned) | Applied. Owned and fixed by r-6 D7: `README.md`, `sc-ai` help/fix hint, `docs/sc-agent-types/architecture.md` (deps, `LogEvent`), crate rows in `docs/sc-jev-check` and `docs/sc-ai`, `sc-jev-py` docs. `arch` ADR-SAG-0008 "UUID paths" is superseded by the ADR-SAG-0008 r-6 amendment (inserted now). Docs grep `! rg -n -i '\buuid\b' README.md docs --glob '!docs/plans/**'` is an acceptance step | r-6 D7, `owned_paths`, acceptance `docs`; arch ADR-SAG-0008 r-6 subsection |
| R6-009 (Minor, SQLite load and `project` scrubbing) | Applied. The SQLite load test moved to r-7 with the DDL: `test_fixture_loads_into_sqlite` (Python stdlib `sqlite3`, no Rust SQLite dependency). `project` is documented as caller text, not scrubbed, in REQ-SAG-0615, ADR-SAG-0031 and r-6 D5 | r-7 D7, acceptance 0146; `reqs` 0615; arch ADR-SAG-0031 |
| R6-ARCH-001 (Blocking, adrs list incomplete) | Applied. r-6 `adrs` lists 0001–0014, 0016, 0019–0031 with one-line reasons. The 0003 comment now reads "r-2 amendment: git toplevel for .env and paths; the log `repo` field is a different root, defined in ADR-SAG-0031". r-7 has its own full list | r-6 and r-7 `adrs` |
| R6-ARCH-002 (Blocking, ADR-SAG-0008 contradicted) | Applied. ADR-SAG-0008 "Amendment (Phase A.1, r-6)" supersedes the "UUID paths" sentence and the r-1 "document version 2" sentence (v3); index marker `ADR-SAG-0031 (Proposed, r-6)`; plan ADR-ownership row; r-6 `owned_paths` | arch ADR-SAG-0008; plan "ADR ownership"; r-6 |
| R6-ARCH-003 (Blocking, flat line vs v2 envelope) | Superseded by owner decision 5 and applied: the row is the v2 `fields{}` object (no bespoke writer); key renamed `send_outcome`; `ts` vs envelope `timestamp` stated; SQLite mapping restated against `fields{}`. The `LogEvent` constructor/accessor change is recorded as a stable-surface change in ADR-SAG-0016 and ADR-SAG-0027 r-6 amendments | r-7 D7; r-6 D6; arch ADR-SAG-0031 "Storage schema", ADR-SAG-0016/0027 r-6 subsections |
| R6-ARCH-004 (Blocking, no channel for model_id/request_sha256 on failure) | Superseded by owner decision 6 and applied: `SendTrace` (core) carried by `JudgeRequest` into `HttpRequest`, recorded by `sc-agent-http` before the first send, read by the engine on `Ok` and `Err`; `JudgeBackend::model_id()` provided method (ADR-SAG-0009 default-body rule). `request_sha256` nullable. r-2b's "Failed emits one LogEvent" stands: a Failed-before-body row has NULL `request_sha256` | r-6 D3, D4; arch ADR-SAG-0031 "Wiring", ADR-SAG-0026 r-6 subsection |
| R6-ARCH-005 (Blocking, git enabler rule) | Applied. `discover_main_repo_root` lives in `sc-jev-check` beside `discover_repo_root` and calls `sc_agent_types::git::discover_common_dir`, inside the existing `git.rs` spawn site; compose calls `sc-jev-check`. The `git` enablers and the ADR-SAG-0028 expect-site list are unchanged (`source_scans` regression) | r-6 D5; arch ADR-SAG-0031 "Wiring" |
| R6-ARCH-006 (Blocking, parallel_safe with r-5 false) | Applied. r-5 and r-6 are both wave 6, `must_follow` r-7, `parallel_safe` with each other, with disjoint ownership: r-6 owns two narrow anchors in `sc-jev` (send-trace hand-off in `client/mod.rs`, `fn model_id` in `check.rs`), `sc-jev-backends/src/backend.rs`, and `sc-decision-schema/src/judge.rs` (trait method + `send_trace` only); r-5 owns the rest of `crates/sc-jev/src/**`, `validate.rs`/`wire.rs`, and `sc-decision-schema` `payload.rs` + `jev/**` (the old `sc-agent-types/src/judge.rs` row is gone: r-7 moved it). r-6 no longer reads r-5's body (sc-agent-http hashes what is sent). Sprint docs and the wave table agree | r-5 and r-6 frontmatter; plan wave table and "Why r-7 runs alone…" note; plan boundary map |
| R6-ARCH-007 (Blocking, ADR-SAG-0030 not inserted) | Applied, renumbered: ADR-SAG-0030 is now "Decision schema crate and model catalog" (r-7) and the session-log ADR is ADR-SAG-0031 "Session log storage schema" (storage schema r-7, wiring r-6). Both inserted Proposed with index rows, every Amended-by marker and subsection. The "exception" wording is removed from plan. ADR-SAG-0031 states every-send-emits / NotSent-none, ADR-SAG-0021 unchanged, `ulid`/`sc-sha` placement, and the 0008/0013/0016/0026/0027 amendments | arch; plan "ADR ownership" |
| R6-ARCH-008 (Important, dependency placement and owned paths) | Applied. `ulid` and `sc-sha` are core (always-on) dependencies of `sc-agent-types` (ADR-SAG-0027 r-6 amendment); `sc-lint.toml` records them for `crates/sc-agent-types` only. Hashing happens in `sc-agent-http` (no new edge on `sc-jev*`); every touched `Cargo.toml`, `boundaries/` and `sc-agent-http` path is in r-6 `owned_paths`. `sc-treesitter` `query_sha256` regression added. D0.1 is an explicit stop-and-report gate; note that the crates.io description of `sc-sha` ("deterministic text … identities") makes normalization a real risk | r-6 D0.1, D2, `owned_paths`, acceptance 0618; plan sc-lint table |
| R6-ARCH-009 (Important, r-2b "no path" wording; ComposeInputs file) | Applied. r-2b acceptance wording narrowed to "no secret and no session-store path in any event", noting `repo` (r-6) as the one allowed path; ADR-SAG-0031 states `repo` is the only path and `project` is unredacted caller text. `crates/sc-jev-host/src/compose.rs` (defines `ComposeInputs`) is in r-6 `owned_paths` | r-2b acceptance `adr:ADR-SAG-0013`; r-6 D5, D6; arch ADR-SAG-0031 |
| R6-ARCH-010 (Minor, wrong citation for the warning policy) | Applied. The policy now cites what states it: r-3b `run_from` routes the CR-014 flush warning into `data.warnings` via `RunResponse::with_warning` (ADR-SAG-0024), and `SessionLog::emit` is best effort (port rustdoc). It is stated in ADR-SAG-0031 "Wiring" as pending owner confirmation | r-6 D0.2; arch ADR-SAG-0031 |

## Decisions made in this round (for owner review)

1. `JudgeBackend` moves to `sc-decision-schema` with the types its
   signatures name; "data-only" is read as the dependency set (no I/O), not
   "no traits".
2. `QuestionId` and `ChoiceId` move too (decision vocabulary only).
3. `observability::v2::LogEvent` carries the row as a JSON object so the port
   in `sc-agent-types` does not name `sc-decision-schema` types.
4. `BackendId` spellings (`anthropic_haiku`, `openai_luna`) are kept; renaming
   is out of scope.
5. r-7 defines `jev::v1` and proves byte identity; r-5 adopts it in `sc-jev`.
6. r-6 is `status: blocked` until the log-write failure policy is confirmed.
7. Requirement block REQ-SAG-0140..0148 (free; highest 01xx id in use is
   0135; no id ≥ 0136 below 0200 appears in `docs/` on any branch).
