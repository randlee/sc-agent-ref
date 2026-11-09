# Quality Report: PR 47 (fix/pe-errors on integrate/phase-a), final pass

QA Pass: qa-pr47-final
PR: #47, `feat(errors): distinct provider, quota, busy and I/O ids; CLI and Python error bodies`
Branch `fix/pe-errors`, head `3da9128`, base `integrate/phase-a` (f310e8f)
Enforcer: cursor-quality-mgr
Final Verdict: **FAIL** (finding gate; spawn gate pass). One open Important: ERR-QA-6. 0 Blocking, 0 open Minor under the owner's Minor rule (Minors tracked to beads).
CI (reported separately from the verdict): 16 of 16 required checks pass at head 3da9128.

Why FAIL: ERR-QA-6 (Important) from the error-contract audit at b3e5332 is still open. A Jev out-of-tokens 429 with no quota marker retries 5 times and ends `PROVIDER_BUSY`. The handoff records this as a documented limit (bead agt-285, P2, OPEN, needs a live Jev reply). It is not a closure, and no later round on the PR records it closed. The owner's rule in the handoff says Blocking and Important must be fixed before merge. Only an explicit owner disposition of ERR-QA-6 would change this verdict; a documented limit does not close a finding under the gate.

## Machine Status (JSON)
```json
{
  "qa_pass": "qa-pr47-final",
  "pr": 47,
  "branch": "fix/pe-errors",
  "head": "3da9128",
  "base": "integrate/phase-a",
  "base_sha": "f310e8f",
  "task": "cursor-quality-mgr",
  "verdict": "FAIL",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [
    {"agent": "rust-qa-agent", "task_id": "aaf6b329b8f50d802", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "acdf9ea", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "provenance_note": "handoff notes say the run was done by a general-purpose agent following rust-qa-agent.md, not the rust-qa-agent subagent type"},
    {"agent": "rust-best-practices-agent", "task_id": "a32d473103b79aee2", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "acdf9ea", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "minor_tracked_in": "agt-4ru"},
    {"agent": "rust-service-hardening-agent", "task_id": "a67852e30ef9fe5c4", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "a4c79b5", "verdict": "SKIPPED", "findings": {"blocking": 0, "important": 0, "minor": 0}, "skip_basis": "its own fenced JSON reports skipped: no service indicators"},
    {"agent": "flaky-test-qa", "task_id": "a171134f0d675c2ec", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "acdf9ea", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}},
    {"agent": "req-qa", "task_id": "a674a98915e4e6f8b", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "3da9128", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "deliverable_completion_percent": 100.0},
    {"agent": "arch-qa", "task_id": "ae65c3755aa48b8cc", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "acdf9ea", "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 1}, "minor_tracked_in": "agt-4ru", "minor_ids": ["ARCH-106"]}
  ],
  "missing_reviewers": [],
  "unparsed_reviewers": [],
  "open_findings": [
    {"id": "ERR-QA-6", "severity": "Important", "source": "error-contract audit round 2 at b3e5332 (PR comment 6728720287)", "state": "open; documented limit, bead agt-285 (P2, OPEN)"}
  ],
  "tracked_minor_findings": {
    "agt-4ru": ["ARCH-106", "ERR-QA-33 remainder", "ERR-QA-34", "ERR-QA-35", "ERR-QA-36", "ERR-QA-37", "ERR-QA-38", "RBP-F203", "RBP-F204", "RBP-F205", "RBP-F206", "RBP-F207", "REQ-QA-105", "REQ-QA-106", "REQ M3", "REQ M4"],
    "agt-gsp": ["RBP-F108", "RBP-F109", "RBP-F110", "RBP-F111", "RBP-F112"]
  },
  "findings": {"blocking": 0, "important": 1, "minor_open": 0, "minor_tracked": 21},
  "aggregation_source": "reviewer_fenced_json_union_pr_comments_union_todo_scan",
  "todo_scan_hits": 0,
  "todo_scan_range": "f310e8f...3da9128 (added lines)",
  "ci": {"state": "16/16 pass at 3da9128", "runs": ["42541519841", "42541519820"], "headSha": "3da9128261562c50285ce638002bee60e308b0f3", "conclusion": "success"},
  "head_correlation": {
    "acdf9ea_to_3da9128": "three doc lines (REQ-SAG-0604 row in docs/requirements.md; I/O table cell in docs/architecture.md; `other` in docs/sc-agent-http/architecture.md); verified with git diff",
    "a4c79b5_to_3da9128": "16 files; no Cargo.toml, Cargo.lock or deny.toml change; no tokio/axum/hyper/tonic/reqwest/actix/warp/rocket/tower added; verified with git diff; supports the service-hardening skip"
  },
  "evidence_chain": {
    "qa_pass": "qa-pr47-final",
    "commit": "3da9128",
    "reviewed_commits": ["acdf9ea", "a4c79b5", "3da9128"],
    "pr_comment_url": "posted after gh pr comment (see handback)",
    "coordinator_task_id": "cursor-quality-mgr",
    "triage": {"ttl_paths": []}
  },
  "next_action": "triage_and_fix"
}
```

## Validated Scope

- Spawn gate: pass, on the record. `.cursor/phase-a-orchestration/pr47-qa/reviewer-handoff.json` lists six reviewers, each with `spawn_actor: parent-orchestrator`, a task_id, and fenced JSON. I did not verify the Task records behind the task_ids.
- Reviewer results, as recorded in the handoff:
  - rust-qa-agent at acdf9ea: PASS, 0 findings. fmt, clippy, workspace and feature tests, workspace-tests, just test-python 28 passed, coverage 93.27% lines (per its notes).
  - rust-best-practices-agent at acdf9ea: PASS, 0 findings. RBP-F201 and F202 closed. Minors are in agt-4ru.
  - rust-service-hardening-agent at a4c79b5: SKIPPED per its own JSON (no service indicators).
  - flaky-test-qa at acdf9ea: PASS, 0 findings.
  - req-qa at 3da9128: PASS, deliverables 100%, 0 findings.
  - arch-qa at acdf9ea: PASS, blocking 0, important 0, merge_ready true. ARCH-106 Minor, deferred to agt-4ru.
- Handoff claim checked: "acdf9ea to 3da9128 changes only three doc lines" is correct (3 files, 3 insertions, 3 deletions, all docs).
- Ancestry checked: a4c79b5 is an ancestor of acdf9ea; acdf9ea is an ancestor of 3da9128.
- Error-contract audit (separate from the six reviewers): a4c79b5 PR comment 6729354317 reads PASS, Minor findings only. Its Minors go to agt-4ru. The comment also discloses two unintended live `sc-ai jev run --backend jev` calls (93-byte payload, 304 input and 33 output tokens, real Jev service, keyring or .env key). Both returned `jev-1.13.0`.
- TODO scan: 0 TODO, FIXME, XXX or HACK hits in added lines of f310e8f...3da9128.
- PR state: open, base integrate/phase-a, head 3da9128 (confirmed with gh).
- Owner rule applied to Minors (per the handoff notes, given by the user this session): Minors from every reviewer are tracked to beads, not counted as open. Blocking and Important are not covered by the rule.

## Findings Summary

- Blocking: 0
- Important: 1 open (ERR-QA-6)
- Minor: 0 open. 21 tracked to beads (agt-4ru 16 items, agt-gsp 5 items).

## Open Findings

- **ERR-QA-6 (Important, open).** Jev out-of-tokens 429 without a quota marker ends `PROVIDER_BUSY` after 5 retries. Jev's signal is unknown. Bead agt-285 (P2, OPEN) requires a live Jev 429/402 and a live Anthropic 'prompt is too long' 400 to add markers to `sc_agent_http::failure`. Route to triage and fix, or to an explicit owner waiver.

## Tracked Minors (not counted as open, per the owner rule)

- agt-4ru (P3, OPEN): ARCH-106 (keyring mark text); ERR-QA-33 remainder (ConnectionReset and BrokenPipe after send stay retryable `connect`, double-charge decision open); ERR-QA-34 to 38 (audit a4c79b5); RBP-F203 to F207 (best practices a4c79b5); REQ-QA-105, REQ-QA-106 (listed in the bead; not re-checked on the PR); REQ M3, M4 (bead notes).
- agt-gsp (P3, OPEN): RBP-F108 to F112. F108 appears on the PR at b3e5332 (comment 6728561570). F109 to F112 appear only in the bead. None appear in the acdf9ea reviewer JSON, which reports zero findings, so their closure is not evidenced this round.

## Known limits (from the handoff notes, not closed by this round)

- ERR-QA-6 Jev out-of-tokens signal unknown (bead agt-285). This is the open Important above.
- No real provider error body seen live.
- Real locked Keychain not tested.
- The intermittent coverage failure on #39 has no proven cause. The helper hypothesis was removed. CI coverage passed at 3da9128.
- Two audit probes made unintended live Jev calls. Disclosed on the PR.
- Windows covered only by CI.
- Not verified live: Haiku or Luna, real blocked keychain, Windows, sc-jev-py and Python tests (per the audit comment).

## Evidence Gaps

1. **Task records.** The task_ids are recorded in the handoff. I did not verify the Task spawns.
2. **Reviewer outputs at acdf9ea and 3da9128.** No PR comment or review holds these results (the last PR comment is the a4c79b5 audit, 2026-11-09T00:28:20Z; the PR has no reviews). The handoff JSON is the only record of the six reviewer results.
3. **Provenance of rust-qa.** The manifest names rust-qa-agent, but the handoff notes say the run was done by a general-purpose agent following rust-qa-agent.md, to get a clean copy. The fenced JSON is present and parses. The parent should confirm this is an accepted substitute before the verdict is used.
4. **ERR-QA-6 status.** I found no round that closes it. The handoff calls it a documented limit. I did not treat that as closure.
5. **agt-gsp status.** F109 to F112 are not evidenced as closed or open this round.
6. **Live behaviour.** No live provider error body or live Jev 429 was observed in the record. The Keychain path was not tested with a real locked Keychain.
7. **Coverage flake.** One passing coverage run at 3da9128 is not proof the earlier intermittent failure on #39 is fixed.
8. **Triage records.** No `.triage/` TTL paths are cited. No triage run is recorded for this round.
9. **Evidence chain URL.** The pr_comment_url is returned by `gh pr comment` and reported in the handback. It cannot appear inside the comment that creates it.

## CI state (reported separately from the QA verdict)

- `gh pr checks 47 --repo randlee/sc-agent`: 16 of 16 pass. Both workflow runs (42541519841, headSha 3da9128; 42541519820 dry-run, headSha 3da9128) completed with conclusion success.

## Next Action

triage_and_fix for ERR-QA-6, or an explicit owner disposition. Do not merge on this round. cursor-quality-mgr did not approve, merge, spawn reviewers, or fix code.
