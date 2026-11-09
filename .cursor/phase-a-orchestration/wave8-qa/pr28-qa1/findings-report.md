# QA Findings Update

Generated: 2026-11-07
QA Pass: qa-pr28-1
Sprint/Task: r-1, r-4, r-3c, r-3, r-2 closeout / qa-pr28-1
Branch: `docs/close-wave1-2`
Commit: `fe375c2f72683f1b12b77f30080cb30ef6d9cdd6`
PR: #28
Verdict: **FAIL**

## Machine Status (JSON)
```json
{
  "sprint": "r-1, r-4, r-3c, r-3, r-2 closeout",
  "task": "qa-pr28-1",
  "branch": "docs/close-wave1-2",
  "commit": "fe375c2f72683f1b12b77f30080cb30ef6d9cdd6",
  "pr": 28,
  "verdict": "FAIL",
  "deliverables": {
    "complete": 4,
    "total": 6,
    "percent": 66.7
  },
  "findings": {
    "blocking": 0,
    "important": 1,
    "minor": 1
  },
  "blocking_ids": [],
  "merge_readiness": "blocked",
  "merge_reason": "Finding gate FAIL: 0 Blocking, 1 Important (ARCH-001), 1 Minor (REQ-QA-PR28-001). Both are stale r-4 Implementation-notes and Deferral wording that still describes status: in-review next to the new status: complete. Deliverables are 4/6 (66.7%) per req-qa JSON. Spawn gate pass (4/4 docs-only reviewers with task_id and parsed fenced JSON). TODO scan 0 hits. CI green (11/11 checks).",
  "next_action": "triage_and_fix",
  "owner": "parent-orchestrator",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [{"agent": "req-qa", "task_id": "a68f0de4953060a57", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 1}}, {"agent": "arch-qa", "task_id": "ab563e5b990147161", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 1, "minor": 0}}, {"agent": "rust-best-practices-agent", "task_id": "ad95b69128cdedb6f", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}}, {"agent": "rust-service-hardening-agent", "task_id": "aa7796039a40997d4", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "SKIPPED", "findings": {"blocking": 0, "important": 0, "minor": 0}}],
  "evidence_chain": {"qa_pass": "qa-pr28-1", "commit": "fe375c2f72683f1b12b77f30080cb30ef6d9cdd6", "pr_comment_url": "https://github.com/randlee/sc-agent/pull/28#issuecomment-6713937887", "coordinator_task_id": "not-provided-to-coordinator", "artifacts_dir": ".cursor/phase-a-orchestration/wave8-qa/pr28-qa1", "todo_scan": {"script": ".claude/skills/todo-triage/scripts/find_todos.py", "hits": 0}, "ci": {"required_checks": "all pass (11/11) at fe375c2"}, "triage": {"ttl_paths": []}},
  "missing_reviewers": [],
  "unparsed_reviewers": []}
```

## Findings Summary
- Deliverables: 4/6 (66.7%)
- Blocking: 0
- Important: 1
- Minor: 1

## Blocking Findings
- none

## Detailed Findings
- **ARCH-001** (Important, arch-qa, RULE-GATE / ADR-SAG-0017) `docs/plans/phase-a-refine/sprint-r-4-adrs-and-manifests.md:268`: The Implementation notes still say the doc "stays status: in-review until the URL is recorded or the owner waives the row", and lines 279-280 say the owner flips the doc to complete after recording the URL. The frontmatter (line 4) now says `complete` while the REQ-SAG-0133 URL is still unfiled, and `docs/architecture.md:672` still carries the "not yet filed" placeholder. Remediation: reword lines 268 and 279-280 to say the row is deferred to the owner (bead agt-scoop-multi-shim) and the status is complete with that row open, or record the owner's waiver there. Text edit only; no new gate, receipt or check.
- **REQ-QA-PR28-001** (Minor, req-qa, cross-doc-conflict) `docs/plans/phase-a-refine/sprint-r-4-adrs-and-manifests.md:265-269` (targets :4, :271-281): Same stale wording. Lines 267-269 still say the doc stays `status: in-review`, and the Deferral section (275-280) describes the flip to complete as a future step that this PR performs now. Remediation: at lines 268-269 and 279-280, state that the doc is marked complete with the REQ-SAG-0133 URL row deferred, and that the owner records the URL in a later one-line docs commit. Wording alignment only; no new gate or check.

Correlation: ARCH-001 and REQ-QA-PR28-001 point at the same text in the r-4 Implementation notes and Deferral section. One wording edit closes both.

Coordinator TODO scan (`.claude/skills/todo-triage/scripts/find_todos.py` on the review worktree at fe375c2): 0 hits.

Reviewer notes (not findings):
- req-qa reports `status: PASS` with deliverables 4/6 (66.7%). The two items it counts incomplete are the r-4 REQ-SAG-0133 deferral (recorded as an owner deferral) and the "no other content changed" check, which req-qa could not run without a shell. The parent-supplied `git diff --stat` shows 5 files changed, 5 insertions, 5 deletions (one line each). req-qa said its PASS depends on that diff being clean; the completion figure above is req-qa's own and is not overridden here.
- rust-best-practices-agent: pass, 0 findings (docs-only, no RBP practices applicable).
- rust-service-hardening-agent: skipped per its contract (no service indicators found).

## Resolved Since Last Pass
- none

## Merge Readiness
- Status: **blocked**
- Reason: Finding gate FAIL: 0 Blocking, 1 Important (ARCH-001), 1 Minor (REQ-QA-PR28-001). Both are stale r-4 Implementation-notes and Deferral wording that still describes status: in-review next to the new status: complete. Deliverables are 4/6 (66.7%) per req-qa JSON. Spawn gate pass (4/4 docs-only reviewers with task_id and parsed fenced JSON). TODO scan 0 hits. CI green (11/11 checks).

## Next Action
- Action: triage_and_fix
- Owner: parent-orchestrator
