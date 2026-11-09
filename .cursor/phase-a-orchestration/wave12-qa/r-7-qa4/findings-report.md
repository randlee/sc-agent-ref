# QA Findings Update

Generated: 2026-11-08T04:54:55Z
QA Pass: qa-r7-4
Sprint/Task: r-7 / cursor-quality-mgr
Branch: `sprint/r-7-decision-schema`
Commit: `20a46a8`
PR: #33
Verdict: **FAIL**

## Machine Status (JSON)
```json
{
  "sprint": "r-7",
  "task": "cursor-quality-mgr",
  "branch": "sprint/r-7-decision-schema",
  "commit": "20a46a8",
  "pr": 33,
  "verdict": "FAIL",
  "deliverables": {
    "complete": 5,
    "total": 6,
    "percent": 83.3
  },
  "findings": {
    "blocking": 5,
    "important": 0,
    "minor": 0
  },
  "blocking_ids": ["REQ-QA-005", "REQ-QA-006", "REQ-QA-007", "ARCH-001", "ARCH-002"],
  "merge_readiness": "not-ready",
  "merge_reason": "5 open Blocking findings (REQ-QA-005/006/007, ARCH-001/002); deliverables 5/6 (83.3%); CI test (windows-2022) pending.",
  "next_action": "triage_and_fix",
  "owner": "cursor-orchestration (parent)",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [{"agent": "req-qa", "task_id": "a542fff0b4cbec9e5", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 3, "important": 0, "minor": 0}}, {"agent": "arch-qa", "task_id": "a4b17f9f831bae6a6", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 2, "important": 0, "minor": 0}}, {"agent": "rust-qa-agent", "task_id": "ac9ca27566f990c1e", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}}, {"agent": "flaky-test-qa", "task_id": "aae62bfe1a66badb9", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}}],
  "evidence_chain": {"qa_pass": "qa-r7-4", "commit": "20a46a8", "pr_comment_url": "pending", "coordinator_task_id": "cursor-quality-mgr", "triage": {"ttl_paths": []}},
  "missing_reviewers": [],
  "unparsed_reviewers": []}
```

## Findings Summary
- Deliverables: 5/6 (83.3%)
- Blocking: 5
- Important: 0
- Minor: 0

## Blocking Findings
- REQ-QA-005 (Blocking, cross-doc-conflict): r-3b sprint doc still names `--test test_util_off` as the holder of the release-binary test; line 613 now selects 0 tests.
- REQ-QA-006 (Blocking, cross-doc-conflict): plan.md:150-153 and validation list plan.md:917 omit `test_util_off_binary`.
- REQ-QA-007 (Blocking, cross-doc-conflict): docs/sc-ai/architecture.md:46 cites the wrong target for the published-binary proof.
- ARCH-001 (Blocking, RULE-ADR-CODE / ADR-SAG-0028): crates/workspace-tests/README.md:5-6 omits sc-decision-schema from dev-dependencies.
- ARCH-002 (Blocking, RULE-ADR-CODE / ADR-SAG-0028, carry-forward): boundaries/workspace-tests/workspace-tests.toml:18 notes string omits sc-decision-schema.

## Detailed Findings
### REQ-QA-005 (Blocking) - stale release-binary holder in r-3b doc
- Source: crates/workspace-tests/tests/test_util_off_binary.rs:28, crates/workspace-tests/tests/test_util_off.rs:1-4
- Stale refs: docs/plans/phase-a-refine/sprint-r-3b-compose-and-workspace-tests.md:613, :594, :420, :261
- Remediation: line 613 -> `cargo test -p workspace-tests --test test_util_off_binary -- test_release_binary_rejects_scripted_answers --exact`; lines 261, 420, 594 name test_util_off_binary. Add the r-3b doc to r-7 owned_paths first.

### REQ-QA-006 (Blocking) - plan.md omits the release-binary proof
- Source: crates/workspace-tests/tests/test_util_off.rs:1-4, crates/workspace-tests/tests/test_util_off_binary.rs:1-4
- Refs: docs/plans/phase-a-refine/plan.md:150-153, :917
- Remediation: rewrite plan.md:150-153 so test_util_off proves the graph half and test_util_off_binary proves the release-built sc-ai rejects --scripted-answers; add `cargo test -p workspace-tests --test test_util_off_binary` after plan.md:917. Add plan.md to r-7 owned_paths first.

### REQ-QA-007 (Blocking) - architecture doc cites the wrong proof target
- Source: crates/workspace-tests/tests/test_util_off_binary.rs:1-4, crates/workspace-tests/README.md:50-52
- Ref: docs/sc-ai/architecture.md:46
- Remediation: change to `--test test_util_off_binary`. Add a docs/sc-ai/** owned_paths entry to the r-7 sprint doc first.

### ARCH-001 (Blocking) - crate README dev-dependency list is stale
- Source: crates/workspace-tests/README.md:5-6 (ADR-SAG-0028, docs/architecture.md:1580-1584; crates/workspace-tests/Cargo.toml:24)
- Remediation: add sc-decision-schema (features = ["test-util"]) to the dev-dependency list.

### ARCH-002 (Blocking, carry-forward) - boundary manifest notes are stale
- Source: boundaries/workspace-tests/workspace-tests.toml:18 (allowed_dependencies line 31 already lists sc-decision-schema)
- Remediation: add sc-decision-schema (features = ["test-util"]) to the notes string on line 18.

### Reviewer caveats (no finding ids)
- req-qa and arch-qa reported that no shell tool was available, so their git and cargo acceptance commands were not re-run. rust-qa-agent reports fmt, clippy, and cargo test pass, and ran test_util_off_binary (4 passed).
- CI: `test (windows-2022)` was still pending when checked; the merge gate requires all required checks green.
- TODO scan (todo-triage find_todos.py, worktree HEAD 20a46a8): 0 rows.

## Resolved Since Last Pass
- none

## Merge Readiness
- Status: **not-ready**
- Reason: 5 open Blocking findings (REQ-QA-005/006/007, ARCH-001/002); deliverables 5/6 (83.3%); CI test (windows-2022) pending.

## Next Action
- Action: triage_and_fix
- Owner: cursor-orchestration (parent)
