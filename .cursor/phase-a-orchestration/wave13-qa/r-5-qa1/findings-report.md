# QA Findings Update

Generated: 2026-11-08T05:57:07Z
QA Pass: qa-r5-1
Sprint/Task: r-5 / cursor-quality-mgr (QA-1)
Branch: `sprint/r-5-jev-wire-envelope`
Commit: `14f85f1`
PR: #34
Verdict: **FAIL**

## Machine Status (JSON)
```json
{
  "sprint": "r-5",
  "task": "cursor-quality-mgr (QA-1)",
  "branch": "sprint/r-5-jev-wire-envelope",
  "commit": "14f85f1",
  "pr": 34,
  "verdict": "FAIL",
  "deliverables": {
    "complete": 10,
    "total": 10,
    "percent": 100
  },
  "findings": {
    "blocking": 1,
    "important": 6,
    "minor": 9
  },
  "blocking_ids": ["REQ-QA-001"],
  "merge_readiness": "not-ready",
  "merge_reason": "Finding FAIL: 1 Blocking, 6 Important, 9 Minor open (union of reviewer JSON and TODO scan, which is 0 hits). Spawn gate passed; CI green.",
  "next_action": "triage_and_fix",
  "owner": "parent-orchestrator (cursor-orchestration)",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [{"agent": "req-qa", "task_id": "ab774793fab375b35", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 1, "important": 3, "minor": 2}}, {"agent": "arch-qa", "task_id": "a6fb5896229bb421c", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 1}}, {"agent": "rust-qa-agent", "task_id": "a183113a413b8f289", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FINDINGS", "findings": {"blocking": 0, "important": 0, "minor": 1}}, {"agent": "rust-best-practices-agent", "task_id": "af9a9dfacae1fde1f", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FINDINGS", "findings": {"blocking": 0, "important": 2, "minor": 4}}, {"agent": "rust-service-hardening-agent", "task_id": "a5f4c632b9baf48b6", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FINDINGS", "findings": {"blocking": 0, "important": 1, "minor": 1}}, {"agent": "flaky-test-qa", "task_id": "a4aeb36c03cb87bd9", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}}],
  "evidence_chain": {"qa_pass": "qa-r5-1", "commit": "14f85f1", "pr_comment_url": "https://github.com/randlee/sc-agent/pull/34#issuecomment-6717969592", "coordinator_task_id": "not-provided-to-enforcer", "triage": {"ttl_paths": []}, "todo_scan": {"script": ".claude/skills/todo-triage/scripts/find_todos.py", "hits": 0}, "ci": {"source": "gh pr checks 34", "result": "pass", "note": "all 13 checks green, not waited on"}},
  "missing_reviewers": [],
  "unparsed_reviewers": []}
```

## Findings Summary
- Deliverables: 10/10 (100%)
- Blocking: 1
- Important: 6
- Minor: 9

## Blocking Findings
- **REQ-QA-001** (Blocking, requirement-not-listed): the sprint doc `requirements` list omits REQ-SAG-0110, 0502, 0503, 0504, 0505, 0144, 0145, 0146 and NFR-SAG-0001 through 0004 that bind the crates and paths it owns. Ref: `docs/plans/phase-a-refine/sprint-r-5-jev-wire-envelope.md:17-29`. Remediation: add the missing ids to the sprint `requirements` list (NFR-SAG-0003 may be marked not-applicable) and name a regression command for each in the `boundary:jev` row. Sprint doc only; no code change.

## Detailed Findings
**Blocking (1)**
- REQ-QA-001 (Blocking, req-qa): see Blocking Findings above.

**Important (6)**
- RBP-F001 (important, rust-best-practices): `crates/sc-decision-schema/src/payload.rs:339-342`. `CanonicalJudgePayload::from_bytes` formats the raw serde error into INPUT_INVALID, which can echo payload values, contrary to its "never a payload byte outside a question id" doc. Remediation: route Data errors through the category/position-only mapper used in `jev/v1.rs` (`describe` / `encode_failed`).
- RBP-F002 (important, rust-best-practices): `crates/sc-decision-schema/src/jev/v1.rs:60-81, 258, 272` and `payload.rs:102`. Own-rule provenance is recovered by the `jev::v1: ` string prefix, and `TryFrom` uses `type Error = String`, which flattens structured errors. Remediation: a small private error enum (for example `RuleViolation(String)`) as the TryFrom error type, distinguished by downcast or custom serde data.
- RSH-001 (important, rust-service-hardening): `crates/sc-jev-backends/src/backend.rs:157-191, 328-331, 374-378`. One `judge()` can block about 420 s across two exchanges with no shared deadline, and CancelToken cannot preempt an in-flight request. Remediation: one overall deadline shared by both exchanges, and a documented or wired shutdown bound with a wall-time test. Reviewer notes that delta attribution (77fffdb..14f85f1) was not verified.
- REQ-QA-002 (important, req-qa): `crates/sc-jev-check/src/payload/questions.rs:102-106`, `caller.rs:43`, `questions.rs:45-50`. A noul question cannot be sent via `--request-json` because `validate_question_set` requires non-empty choices. Remediation: make `validate_question_set` kind-aware, add a caller_json noul test, and either extend question-set files or document them as choice-only.
- REQ-QA-003 (important, req-qa): `crates/sc-jev-check/src/payload/caller.rs:75-88`. Every Data error maps to "request JSON must contain a questions array", which is misleading and hides specific rule hints this sprint introduced. Remediation: surface marked rule messages while keeping raw values hidden, and add tests for score-11-levels and empty-choices hints.
- REQ-QA-004 (important, req-qa): `docs/sc-decision-schema/requirements.md:8-14` and `docs/sc-jev/requirements.md:7-11, 22-29`. REQ-SAG-0143 and 0403 have no crate requirement rows, and the 0103 row lacks the additive-fields note. Remediation: add the rows and the note.

**Minor (9)**
- QA-001 (minor, rust-qa): `crates/sc-jev-py/Cargo.toml:28`. `cargo test --all-features` fails to link the sc-jev-py test binary on macOS because of pyo3 `extension-module`. Reviewer reports it is pre-existing on baseline 77fffdb. Remediation: exclude sc-jev-py from all-features test runs, or gate the feature, and document it in CI.
- RBP-F003 (minor, rust-best-practices): `crates/sc-decision-schema/src/jev/v1.rs:517-526`. `CheckRequest::parse` documents INPUT_INVALID but returns ENCODE_FAILED for non-own serde failures. Remediation: align the documented codes with behaviour and drop the redundant `describe` computation.
- RBP-F004 (minor, rust-best-practices): `crates/sc-decision-schema/src/payload.rs:109-124`. Public infallible `Question::new` and `CanonicalJudgePayload::new` bypass the kind rules. Remediation: make them fallible or `pub(crate)`/`cfg(test)`, or document them as unchecked.
- RBP-F005 (minor, rust-best-practices): `crates/sc-decision-schema/src/payload.rs:224-227` and `jev/v1.rs:757-760`. `yes`/`no` labels go through fallible `ChoiceId::try_from`, and failures are swallowed with `filter_map(...ok())`. Remediation: add infallible `ChoiceId::yes()` / `no()` constants.
- RBP-F006 (minor, rust-best-practices): `crates/sc-decision-schema/src/jev/v1.rs:704-717`. Unreachable `ok_or_else` in `into_answers`, and an O(n^2) lookup. Remediation: zip the ordered maps, or drop the pre-check.
- RSH-002 (minor, rust-service-hardening): `crates/sc-jev-backends/src/backend.rs:253-261`. `http_failure` puts up to 300 bytes of the provider response body into NETWORK_ERROR detail, unlike the Jev backend (`sc-jev/src/client/mod.rs:217-221`, status only). Remediation: keep status only, or redact through an allow-list with a sentinel test.
- ARCH-001 (minor, arch-qa): `docs/architecture.md:140`. ADR-SAG-0030 index row status reads "jev::v1 (r-5)" rather than the "(Active, r-5)" marker form. Remediation: optional wording change to match the marker form.
- REQ-QA-005 (minor, req-qa): `docs/plans/phase-a-refine/plan.md:1020` and `crates/sc-jev/src/check.rs:78-79`. Stale "until r-5 lands" text, and the WireFault::DropEmbeddedByte doc no longer matches the seed behaviour. Remediation: update the plan D9 line at sprint close and correct the doc comment.
- REQ-QA-006 (minor, req-qa): `tests/fixtures/decision-schema/jev/check-request-all-types.json:3` and `crates/sc-decision-schema/tests/jev.rs:74`. The fixture carries answer `choice: yes` for question `quality`, whose choices are pass/fail. The `from_bytes` doc says errors never name payload bytes, but raw serde text is formatted. Remediation: use `choice: pass`, and either map Data errors to category and position or correct the doc.

## Resolved Since Last Pass
- none

## Merge Readiness
- Status: **not-ready**
- Reason: Finding FAIL: 1 Blocking, 6 Important, 9 Minor open (union of reviewer JSON and TODO scan, which is 0 hits). Spawn gate passed; CI green.

## Next Action
- Action: triage_and_fix
- Owner: parent-orchestrator (cursor-orchestration)
