# Quality Report: stack #41 (PR 56 top, head ad669c5)

QA Pass: qa-stack-41-final. Enforcer: cursor-quality-mgr. Base of PR 56: fix/stream-cancel-flush (layers 47, 49, 51, 50, 52, 53, 54, 55, 56).
Spawn gate: pass (six manifest entries with task_ids, spawn_actor parent-orchestrator). Reviewer outputs in the handoff are parent-written summaries; flaky-test-qa and rust-qa-agent reviewed 23df9f6 and a52c776, not head.

## Verdict

**QA: FAIL (finding gate).** Stack-top diff is clean. Open Blocking and Important findings on lower layers have no closure evidence at head (list below). Owner decides disposition; this report does not approve or merge.

CI (reported separately from the verdict): at check time most legs pending; passing: RustSec advisories, rustdoc (-D warnings). Not awaited.

## Stack-top fix (git diff 6eca341 ad669c5)

- Files: docs/architecture.md (+4), docs/requirements.md (+1), docs/sc-jev-py/requirements.md (+/-1), tests/fixtures/jev-python/tests/test_ask_by_id.py (docstring only). No code.
- REQ-QA-005 closed: the ask row now carries REQ-SAG-0731. REQ-SAG-0605 remains only in its chain_context uses (docs/requirements.md:174 and crate docs). 0731 is defined once in docs/requirements.md:301.
- ARCH-008 closed: amendment note inside the ADR-SAG-0011 section of docs/architecture.md (about line 872).

## Open at head (no closure evidence found)

1. PR 50 (head 15c139a), Blocking: Python general-log rows lost at full rate (about 89%, from a non-blocking try_log on a full queue), reported at 5b92b15. No closure comment. try_log is still on the py.detach path (crates/sc-jev-py/src/logging.rs:361).
2. PR 50, Important I3: Python streams do not accept session_log= or general_log= (5b92b15). No closure comment.
3. PR 52 (head 395845636), Blocking: the new durability test passes with the fix reverted (needs 2 to 3 threads). No closure comment.
4. PR 52 and PR 51, Important: unconditional `-undefined dynamic_lookup` on darwin in .cargo/config.toml:22 and :25 is still at head. Needed only for wheels; a missing symbol links and segfaults.
5. PR 51 (last review 979cb0c, FAIL, three Important): flush retry path now present (logging.rs:240-250) and the test calls Python::initialize (logging.rs:579, 603). Neither has a closure comment. Item 4 is the macOS wheel point.

## Closed with evidence

- PR 50: ARCH-001..003, REQ-QA-001/002/004/006, RBP-F001..F007, FTQ-001..007 (closures at ca2768f and 23df9f6); cancel at 1.11 s with no resends and connection churn fixed (5b92b15 re-stress, hands-on).
- PR 51: round 1 closed at 979cb0c per the handoff (see item 5 for the remainder).
- PR 55 stack top: REQ-QA-001..004 closed at 6eca341 per the handoff; ask-by-id feature in 0599cfc.
- ERR-QA-6 (PR 47): closed by owner-supplied live Jev evidence, per the owner rule; no further round.

## Gaps stated plainly

- The handoff says the row-loss evidence at 7f68e8e was posted on PRs 50 and 52. It is not in either PR's comments; I could not verify it.
- Minors: PR 51's minors are marked "bead pending" and no bead exists for them. I did not create beads. PR 50 and PR 55 minors point to agt-8e2; agt-8e2 exists and is OPEN. The owner rule (Minors to beads) is not fully applied.
- The brief named integrate/phase-a as the stack base; PR 56's base is fix/stream-cancel-flush.
- Tests and cargo were not run; conclusions rest on reviewer comments, git objects and head-code grep.

## Machine Status

```json
{
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [
    {"agent": "req-qa", "task_id": "ad8be831eca1761a7", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS at 6eca341 (fixed at ad669c5)", "findings": {"blocking": 0, "important": 0, "minor": 0}},
    {"agent": "arch-qa", "task_id": "a6e4739ec2fe6dca8", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS at 6eca341", "findings": {"blocking": 0, "important": 1, "minor": 0}},
    {"agent": "flaky-test-qa", "task_id": "a34f6c857bb41ed37", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "pass at 23df9f6", "findings": {"blocking": 0, "important": 0, "minor": 0}},
    {"agent": "rust-qa-agent", "task_id": "aa7fc8247a69190c8", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "pass at a52c776", "findings": {"blocking": 0, "important": 0, "minor": 0}},
    {"agent": "rust-best-practices-agent", "task_id": "a864f11f803825548", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings fixed at 23df9f6", "findings": {"blocking": 0, "important": 0, "minor": 0}},
    {"agent": "rust-service-hardening-agent", "task_id": "a67852e30ef9fe5c4", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "SKIPPED (no service indicators)", "findings": {"blocking": 0, "important": 0, "minor": 0}}
  ],
  "missing_reviewers": [],
  "unparsed_reviewers": [],
  "open_layer_findings": ["PR50 Blocking row loss (no closure)", "PR50 Important I3 (no closure)", "PR52 Blocking durability test (no closure)", "PR52/PR51 Important dynamic_lookup (open at head)", "PR51 Important x3 (no closure)"],
  "aggregation_source": "reviewer_fenced_json_union_todo_scan (layer comments; head grep)",
  "evidence_chain": {
    "qa_pass": "qa-stack-41-final",
    "commit": "ad669c5",
    "pr_comment_url": "see handback",
    "coordinator_task_id": "cursor-quality-mgr",
    "triage": {"ttl_paths": []}
  },
  "next_action": "triage_and_fix"
}
```
