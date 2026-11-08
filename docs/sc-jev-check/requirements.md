# sc-jev-check — requirements

**Sprint** = the sprint that implements the requirement. The owning `req:`
acceptance root, when it differs, is shown as "root a-N"; the closure table in
[`plans/phase-a/plan.md`](../plans/phase-a/plan.md#requirement-closure-integration-req-roots) is authoritative.

| ID | Requirement | Sprint |
| --- | --- | --- |
| REQ-SAG-0300 | Extractors: `file`, `table-column`, `subjects`, `command` | a-3 |
| REQ-SAG-0311 | `subjects` input: validate `sc/subjects/1`, map ids/text | a-3 |
| REQ-SAG-0305 | `command` stdout = subjects schema | a-3 |
| REQ-SAG-0309 | `table-column` unescaped pipes | a-3 |
| REQ-SAG-0301 | Refuse send on config/input errors | a-3 |
| REQ-SAG-0302 | Repo-root resolution | a-3 |
| REQ-SAG-0303 | Question-set schema | a-3 |
| REQ-SAG-0503 | LLM system prompt load + inject (parse field a-3; read/inject a-6) | a-6 (root a-7) |
| REQ-SAG-0600 | Sessions; session id and file name are a ULID, session document version 3 (amended in r-6) | a-6 (root a-7), r-6 | `--features test-util --test sessions_exact` (`test_session_file_named_by_ulid_mode_0600_expiry`), `--test session` (`test_v2_session_document_is_invalid`) |
| REQ-SAG-0601 | `run_check` / dry-run; `prepare`/`run` with `RunConfig::dry_run` (r-2) | a-6 (root a-7), r-2 |
| REQ-SAG-0602 | Equal-input record | a-6 |
| REQ-SAG-0603 | Multi-backend run | a-6 (root a-7) |
| REQ-SAG-0606 | `equal` is computed from backend-reported digests of the canonical blob recovered from each final wire body | r-2b | `--features test-util --test multi_backend` (`test_equal_false_when_backend_digest_differs`, `test_equal_true_when_all_digests_match`); wire-builder digests in `sc-jev` and `sc-jev-backends` `--test wire` (`test_real_wire_builder_reports_prompt_digest`, `test_digest_detects_faulty_wire_builder`) |
| REQ-SAG-0610 | Session logging via `SharedSessionLog`; exactly one row per `judge`/`ask` send, `Failed` included, `NotSent` none (amended in r-6) | a-6 (root a-8), r-6 | `--features test-util --test session_log` (`test_one_row_per_send_failed_included_not_sent_none`) |
| REQ-SAG-0611 | Log fields, no secrets; every row is one ADR-SAG-0031 row with `session_id`, `repo`, `project`, `model_id`, `prompt_sha256` (amended in r-6) | a-6 (root a-8), r-6 | `--test session_log` (`test_every_row_carries_context`, `test_row_has_no_body_prompt_or_key_bytes`) |
| REQ-SAG-0613 | Per-send `SendTrace`; row `request_sha256` is the sent-body digest, `null` before a body exists | r-6 | `--test session_log` (`test_ok_row_request_sha256_is_sent_body_digest`, `test_failed_before_body_row_has_null_request_sha256`, `test_multi_backend_rows_carry_own_request_sha256`) |
| REQ-SAG-0614 | `discover_main_repo_root`: a worktree resolves to its main repository; separate git dir falls back to the toplevel; outside git the folder | r-6 | `--test repo` (`test_main_repo_root_resolves_worktree`, `test_main_repo_root_outside_git_is_cwd`, `test_main_repo_root_separate_git_dir_falls_back_to_toplevel`) |
| REQ-SAG-0616 | Row `model_id` is `JudgeBackend::model_id()`, `Failed` included | r-6 | `--test session_log` (`test_failed_row_carries_model_id`) |
| REQ-SAG-0617 | A rejected log row never fails the run | r-6 | `--test session_log` (`test_rejected_log_row_keeps_run_ok`) |
| REQ-SAG-0312 | `command` input `path` field substituted for `{path}` | r-2 |
| REQ-SAG-0313 | Command producer stderr error response re-raised with its code | r-2 |
| REQ-SAG-0314 | Request byte cap owned by each backend; dry-run reports `over_limit` | r-2 |
| REQ-SAG-0607 | Stateless `FileSessionStore`, no-lock concurrency model, one write per run | r-2 |
| REQ-SAG-0620 | One engine API (`prepare` → `run`) and one `RunResponse` | r-2 |

Consumer boundary: [`extraction.md`](extraction.md). Subjects schema: [`subjects-schema.md`](../sc-agent-types/subjects-schema.md).

## Phase A.1 (r-5): question kinds

Question-set files (`plan-anchoring/questions/1`, `check.toml`) stay
`choice`-only: they carry `criteria`, never `kind` or `descriptions`. Caller
`--request-json` documents may carry `kind` (`choice`, `score`, `noul`) and
`descriptions` on a question. `validate_question_set` requires choices only for
`choice` and `score` questions; the payload deserializer enforces the counts
(`choice` 2 to 255, `score` 2 to 10, `noul` none) and a violation is reported
with the rule text (question id, kind and counts, never the prompt). Haiku and
Luna refuse a `noul` question before any send; only `jev` answers it.
