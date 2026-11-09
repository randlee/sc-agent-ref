# Quality Report: stack #41 (PRs 39, 40, 42, 43, 44, 45), second pass

QA Pass: qa-stack-41-2
Stack top: PR #45, branch `fix/pe-plan-notes`, head `6b946d2`, trunk `integrate/phase-a`
Enforcer: cursor-quality-mgr
Supersedes: qa-stack-41-1 (FAIL), https://github.com/randlee/sc-agent/pull/45#issuecomment-6727797346
Final Verdict: **FAIL** (evidence correlation; spawn gate pass; findings 0B+0I+0M open; CI 16/16 pass at head)

## Machine Status (JSON)
```json
{
  "sprint": "stack #41 (PRs 39, 40, 42, 43, 44, 45)",
  "task": "cursor-quality-mgr",
  "branch": "fix/pe-plan-notes",
  "commit": "6b946d2",
  "pr": 45,
  "verdict": "FAIL",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [
    {"agent": "rust-qa-agent", "task_id": "a64b6f8584a934ed9", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "e4ede9f", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "head_correlation": "diff e4ede9f..6b946d2 in crates/sc-agent-http/tests/retry.rs and scripts/check-scoop-manifest.py is comment-only (verified by cursor-quality-mgr); all other changed files are docs"},
    {"agent": "rust-best-practices-agent", "task_id": "a92c3bac3bcd80015", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "741f8ce", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "head_correlation": "NOT COVERED: 17 commits and 18 non-doc files (*.rs, *.toml, *.yml, *.py, *.sh, *.json) changed between 741f8ce and 6b946d2; handoff notes do not state this diff"},
    {"agent": "flaky-test-qa", "task_id": "a5b68d90af97a30c2", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "e4ede9f", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "head_correlation": "same as rust-qa-agent"},
    {"agent": "req-qa", "task_id": "abb7349fb95eb6e78", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "d2121c9", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "deliverable_completion_percent": 100.0, "head_correlation": "NOT COVERED: 3 commits (3b5171d, 6b946d2) changed docs/plans/phase-a-refine/plan.md and sprint-r-3-shared-http.md after d2121c9"},
    {"agent": "arch-qa", "task_id": "aa390d7377f235a92", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "6b946d2", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "note": "ARCH-002 (Important) and ARCH-003 (Minor) CLOSED in arch-qa JSON at head; merge_ready true"},
    {"agent": "rust-service-hardening-agent", "task_id": "a3b843eddf01e1495", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "6b946d2", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "note": "RSH-001 and RSH-002 reported fixed at head in reviewer JSON"}
  ],
  "missing_reviewers": [],
  "unparsed_reviewers": [],
  "open_findings": [],
  "findings": {"blocking": 0, "important": 0, "minor": 0},
  "aggregation_source": "reviewer_fenced_json_union_todo_scan",
  "todo_scan_hits": 0,
  "todo_scan_range": "integrate/phase-a...6b946d2 (added lines)",
  "ci": {"state": "16/16 pass at 6b946d2", "source": "gh pr checks 45 --repo randlee/sc-agent; runs 42527649843 and 42527649820 headSha 6b946d2"},
  "head_correlation_gaps": [
    {"agent": "rust-best-practices-agent", "reviewed_commit": "741f8ce", "required": "rerun at 6b946d2"},
    {"agent": "req-qa", "reviewed_commit": "d2121c9", "required": "rerun at 6b946d2"}
  ],
  "evidence_chain": {
    "qa_pass": "qa-stack-41-2",
    "commit": "6b946d2",
    "reviewed_commits": ["e4ede9f", "741f8ce", "d2121c9", "6b946d2"],
    "pr_comment_url": "pending",
    "coordinator_task_id": "cursor-quality-mgr",
    "triage": {"ttl_paths": []}
  },
  "next_action": "parent_respawn_reviewers"
}
```

Note on `next_action`: `parent_respawn_reviewers` is used here for a head-correlation respawn. It is not a spawn-gate failure. `reviewer_spawn_gate` stays `pass`.

## Validated Scope
- Spawn gate: pass. The handoff at `.cursor/phase-a-orchestration/phase-end-qa5/reviewer-handoff.json` lists six reviewers. Each has `spawn_actor: parent-orchestrator`, a recorded task_id, and fenced JSON in `reviewer_outputs`. No missing or unparsed reviewers. The arch-qa and rust-service-hardening task_ids changed from qa-stack-41-1.
- Reviewer commits, as recorded in the handoff:
  - rust-qa-agent at e4ede9f: PASS, 0 findings (611 passed, 0 failed, 3 ignored per its notes).
  - flaky-test-qa at e4ede9f: PASS, 0 findings.
  - rust-best-practices-agent at 741f8ce: PASS, 0 findings. Its diff to head is not covered (see gaps).
  - req-qa at d2121c9: PASS, deliverables 100%, 0 findings. Its diff to head is docs-only but not covered (see gaps).
  - arch-qa at 6b946d2: PASS, blocking 0, important 0. ARCH-002 and ARCH-003 closed in its JSON at head. This closes the gap that failed qa-stack-41-1.
  - rust-service-hardening-agent at 6b946d2: PASS, 0 findings. RSH-001 and RSH-002 reported fixed at head.
- Head diffs, checked by cursor-quality-mgr with `git diff`:
  - e4ede9f..6b946d2: `crates/sc-agent-http/tests/retry.rs` (doc comment only) and `scripts/check-scoop-manifest.py` (one comment line added). Other changed files are docs. The rust-qa and flaky results at e4ede9f are accepted for behaviour.
  - 3b5171d..6b946d2: `docs/architecture.md` and `docs/sc-agent-http/architecture.md` only. The arch-qa result at head covers this.
  - d2121c9..6b946d2: 3 commits, docs only. `docs/architecture.md`, `docs/plans/phase-a-refine/plan.md`, `docs/plans/phase-a-refine/sprint-r-3-shared-http.md`, `docs/sc-agent-http/architecture.md`.
  - 741f8ce..6b946d2: 17 commits. 18 non-doc files changed, including `crates/sc-agent-http/src/retry.rs`, `transport.rs`, `ureq_transport.rs`, `crates/sc-jev/src/client/mod.rs`, `config.rs`, `.cargo/config.toml`, `.github/workflows/sc-agent-ci.yml`, `deny.toml`, and release scripts.
- TODO scan: 0 TODO, FIXME, XXX, or HACK hits in added lines of `integrate/phase-a...6b946d2` (42 files, 1172 insertions).
- CI (separate from the QA verdict): `gh pr checks 45` shows 16 of 16 checks pass. The `sc-agent CI` and `sc-publish dry-run` runs report headSha `6b946d2a3700dc1055765b99e67dad77e36ca20f`.
- PR head confirmed by `gh pr view 45`: headRefOid `6b946d2a3700dc1055765b99e67dad77e36ca20f`.

## Findings Summary
- Blocking: 0
- Important: 0
- Minor: 0
- Closed at head per reviewer JSON: ARCH-002 (Important), ARCH-003 (Minor), RSH-001 and RSH-002 (minor, fixed in #40 and at head).

## Why the verdict is FAIL
Every reviewer finding is zero and the spawn gate passes. The FAIL is about what the evidence proves at head.

1. **rust-best-practices-agent did not review the code at head.** Its PASS covers 741f8ce. Seventeen commits and 18 non-doc files changed after it. These include the busy-retry change to five sends (6553e76), the transport timeout bounds (83370df, RSH-002), the credential header marking (759a810, RSH-001), and the sc-jev and CI changes. The handoff notes do not state this diff, so the PASS cannot be carried to head. rust-service-hardening at head covers only part of this code.
2. **req-qa did not review head.** Its PASS covers d2121c9. Three later docs commits changed `plan.md` and `sprint-r-3-shared-http.md`, which are the deliverable documents. The deliverable count is therefore not verified at head.

The qa-stack-41-1 FAIL was for the same reason: reviewer evidence not verified at head. The same rule applies here. This verdict is cursor-quality-mgr's call under that rule. It is not a spawn-gate failure, and the hard gate text does not list head correlation directly.

## Evidence Gaps
1. **Head correlation, rust-best-practices-agent.** Reviewed 741f8ce. Head has 17 later commits and 18 later non-doc files. Not covered. Required: rerun at 6b946d2.
2. **Head correlation, req-qa.** Reviewed d2121c9. Later docs commits changed the plan and sprint-r-3 deliverable docs. Not covered. Required: rerun at 6b946d2.
3. **Coverage flake, unproven.** rust-qa did not run coverage. The `coverage (llvm-cov)` CI job passed at 6b946d2 (run 42527649843). The earlier intermittent coverage failure on #39 has no proven cause. Asserts now print stderr (96967d7), and stale profile data is cleared (e4ede9f). No reproduction was seen in this round. Passing CI is not proof the flake is fixed.
4. **Non-blocking doc note, from the handoff notes.** `docs/architecture.md` lines 709 and 715-716 still show the old four-send text. The handoff says the ADR-SAG-0007 amendment governs and an inline pointer lands in a later layer. cursor-quality-mgr did not verify this text. arch-qa at head did not list it as a finding.
5. **Scope exclusions.** The Jev layer and the error-contract layer are excluded. The error-contract audit on #39 (FAIL, bead agt-2a3) lands later and is not gated here.
6. **Stack identifier.** The handoff calls this "stack #41". No PR numbered 41 exists in randlee/sc-agent. The stack is identified here by its six PRs.
7. **Triage records.** No `.triage/` TTL paths are cited for this round.

## CI state (reported separately from the QA verdict)
- `gh pr checks 45 --repo randlee/sc-agent`: 16 of 16 pass at head 6b946d2. Passing CI does not change the QA verdict.

## Next Action
parent_respawn_reviewers (head correlation, not a spawn-gate failure):
- Rerun rust-best-practices-agent at 6b946d2. Its scope should cover the code changed from 741f8ce.
- Rerun req-qa at 6b946d2.
- rust-qa-agent and flaky-test-qa at e4ede9f are accepted by the comment-only diff above. A rust-qa run at head is optional and would also add the coverage evidence gap.
- Route the new fenced JSON through the parent. cursor-quality-mgr will publish qa-stack-41-3 on the new evidence.

Routing belongs to the parent orchestrator. cursor-quality-mgr did not approve, merge, spawn reviewers, or fix code.

FAIL line:
`Sprint stack #41 (PRs 39,40,42,43,44,45) QA: FAIL — deliverables 100% (req-qa at d2121c9, not verified at head); reviewer_spawn_gate=pass; next_action=parent_respawn_reviewers; missing_reviewers=none; req-qa=PASS@d2121c9 (head gap); arch-qa=PASS@6b946d2; rust-qa=PASS@e4ede9f; rust-best-practices=PASS@741f8ce (head gap); rust-service-hardening=PASS@6b946d2; flaky-test-qa=PASS@e4ede9f; open findings: none; pr_comment=pending; enforcer=cursor-quality-mgr; PR #45; worktree (sc-agent checkout, head 6b946d2)`
