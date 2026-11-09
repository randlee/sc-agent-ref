# Quality Report: PR 47 (fix/pe-errors on integrate/phase-a), QA-2

QA Pass: qa-pr47-final-2 (second report; follows qa-pr47-final at https://github.com/randlee/sc-agent/pull/47#issuecomment-6729772154)
PR: #47, `feat(errors): distinct provider, quota, busy and I/O ids; CLI and Python error bodies`
Branch `fix/pe-errors`, head `a5bab66`, base `integrate/phase-a` (f310e8f)
Enforcer: cursor-quality-mgr
Final Verdict: **FAIL** (finding gate; spawn gate pass). One open Important: ERR-QA-6, the Jev 429 half. 0 Blocking. 0 open Minor under the owner's Minor rule (Minors tracked to beads agt-4ru and agt-gsp).
CI (reported separately from the verdict): at the time of the check, 10 of 16 checks pass, 6 pending, 0 failing at head a5bab66 (runs 42547160896 and 42547160895). CI was not awaited.

## Why FAIL

ERR-QA-6 is closed in the handoff and in bead agt-285 on the strength of a live HTTP 402 `billing_error` trace. That trace covers only the 402 half of the finding. The finding as written in qa-pr47-final is a Jev out-of-tokens **429** with no quota marker, which retries 5 times and ends `PROVIDER_BUSY`. Checked against the code and test at a5bab66, that 429 behaviour is unchanged and is now enforced by a test as intended behaviour. The known-limit text that disclosed it was removed from `docs/stability.md` and `docs/requirements.md`.

Details (verified in the repository, not by running cargo):

- `crates/sc-agent-http/src/failure.rs` at a5bab66, `is_quota_reply(status, body)` is `status == 402 || Body::read(body).marks_quota(status)`. 402 was already `QUOTA_EXHAUSTED` at 3da9128 (the same expression is at 3da9128:171-173). The 402 path was already non-retryable because 402 is not in `BUSY_STATUSES` (`retry.rs`: `[429, 502, 529]`). The a5bab66 change adds only the `detail` object reader (`error_type` and `message`). The owner's own comment (6730265561) says the same: "402 already mapped to QUOTA_EXHAUSTED and never retried".
- A 429 with no quota marker is still busy. `retry.rs` `is_busy` retries 429 unless `is_quota_reply` is true, with `MAX_SENDS = 5`. The doc comment at a5bab66 says "an unknown 429 body is busy, never quota".
- The test `quota_markers_come_from_documented_shapes_only` asserts `!is_quota_reply(429, ...)` for `{}`, `not json`, `""`, `{"error":"x"}`, and rate-limit bodies. So the 429 behaviour is pinned as busy.
- The new test `jev_out_of_credits_is_quota_exhausted_with_its_message` covers only status 402 and 401 bodies.
- Bead agt-285 is CLOSED with a close reason about 402 only. Its own description still asks for "a live 429/402 from Jev". Its notes say confirming needs "a key with no balance, or support".

What would close the Important: either (a) a live Jev 429 out-of-tokens reply, with a code or test change if it is quota, or (b) an explicit owner disposition in writing that the 429 case stays busy as a documented known limit. Option (b) is the owner's call. This QA round does not treat the 402 evidence as closing the 429 finding, and the disposition text is not in the repository yet.

## Diff verification (git diff 3da9128 a5bab66)

- 3 files changed, 45 insertions, 8 deletions: `crates/sc-agent-http/src/failure.rs` (+35), `docs/requirements.md` (+4/-4), `docs/stability.md` (+6/-4). The handoff claim is correct.
- 3da9128 is an ancestor of a5bab66 (checked with `git merge-base --is-ancestor`).
- The failure.rs code change adds the `Value::Object(detail)` arm and a test. Its doc comment for `llm_text_about_max_tokens_is_an_invalid_request` is split by the new test (this is REQ-QA-001, Minor).
- The docs change removes the ERR-QA-6 known-limit paragraph and labels the item "closed". The Anthropic "prompt is too long" known limit remains.
- `git diff acdf9ea 3da9128` (from the prior round) is three doc lines and is not re-verified here.
- No Cargo.toml, Cargo.lock or dependency change in the 3da9128..a5bab66 range.

## Reviewers (handoff, `.cursor/phase-a-orchestration/pr47-qa/reviewer-handoff.json`)

| Agent | task_id | Reviewed commit | Head a5bab66 covered? | Verdict | Findings |
|---|---|---|---|---|---|
| rust-qa-agent | aaf6b329b8f50d802 | acdf9ea | No (failure.rs not reviewed by this agent) | PASS, `data.status: pass` | 0 |
| rust-best-practices-agent | aa81e3a1edfe062a0 | a5bab66 | Yes | PASS | 0 in JSON (Minors in agt-4ru) |
| rust-service-hardening-agent | a67852e30ef9fe5c4 | a4c79b5 | No (no service indicators; its own JSON reports skipped) | SKIPPED (per its JSON) | 0 |
| flaky-test-qa | a171134f0d675c2ec | acdf9ea | No | PASS | 0 |
| req-qa | a31e65c810bb509a6 | a5bab66 | Yes | PASS, deliverables 100.0% | 1 Minor: REQ-QA-001 (agt-4ru) |
| arch-qa | ae65c3755aa48b8cc | acdf9ea | No | PASS, blocking 0, important 0, merge_ready true | 1 Minor: ARCH-106 (agt-4ru) |

Notes on the table:

- Spawn evidence: every manifest entry has `spawn_actor: parent-orchestrator`, a task_id, and fenced JSON that parses. I did not verify the Task records behind the task_ids.
- Head coverage: arch-qa, rust-qa-agent and flaky-test-qa reviewed acdf9ea, not the head a5bab66. The QA-2 default set requires req-qa, arch-qa and rust-qa-agent. Only req-qa and rust-best-practices reviewed the failure.rs change. The handoff says the orchestrator ran `cargo test --workspace` (673 passed, 0 failed), `doc` and `just lint` at a5bab66. That is parent evidence, not reviewer evidence, and I did not re-run it. The parent should respawn arch-qa and rust-qa-agent at a5bab66 before any PASS is declared on a head-based gate.
- rust-qa-agent provenance: the handoff notes say the run was made by a general-purpose agent following rust-qa-agent.md "to get a clean copy". The fenced JSON is present and parses. The parent must confirm this is an accepted substitute.
- Reviewer fenced JSON for a5bab66 is only in the handoff file. No PR comment or review holds it.

## Validated Scope

- Spawn gate: pass on the record. Six reviewers, each with spawn_actor `parent-orchestrator`, a task_id and parseable fenced JSON. No missing or unparsed reviewers.
- Deliverables: 100.0% per req-qa at a5bab66 (fenced JSON).
- Ancestry: a4c79b5 is an ancestor of acdf9ea, acdf9ea of 3da9128, 3da9128 of a5bab66.
- TODO scan: not re-run this round for the 3da9128..a5bab66 range. The prior round's scan (f310e8f..3da9128) found 0 hits. The new diff adds no TODO, FIXME, XXX or HACK text. I checked the added lines in the diff above by reading, not by a scan script. Treat as read-verified, not scan-verified.
- PR state: OPEN, base integrate/phase-a (f310e8f), head a5bab66 (`headRefOid` a5bab6666a10d0560929e4451754d54ebb7a279a). 17 PR comments at the time of the check; last is https://github.com/randlee/sc-agent/pull/47#issuecomment-6730265561 (owner ERR-QA-6 closure comment).
- CI: 10 pass, 6 pending, 0 fail at the time of the check. Pending: test (macos-15), test (ubuntu-24.04), test (windows-2022), workspace-tests, release wheel has no _testing (windows-2022), release wheel has no _testing (windows-11-arm). CI is reported separately from the QA verdict.

## Findings Summary

- Blocking: 0
- Important: 1 open. ERR-QA-6 (Jev 429 out-of-tokens without a marker, retries then PROVIDER_BUSY). The 402 half is closed by evidence. See Why FAIL.
- Minor: 0 open. Owner rule applied: Minors go to beads. 27 tracked (agt-4ru 22, agt-gsp 5).

## Open Findings

- **ERR-QA-6 (Important, open, re-scoped).** Source: error-contract audit round 2 at b3e5332 (PR comment 6728720287). Remedy: live Jev 429 evidence plus code or test change if quota, or an explicit owner written disposition keeping the 429 case as a documented busy known limit. Bead agt-285 is closed on 402 only and should be reopened or a new bead created for the 429 half. Route to triage_and_fix or to the owner.

## Tracked Minors (owner rule: beads, not counted as open)

- agt-4ru (P3, OPEN), 22 items:
  - From reviewer JSON this round: REQ-QA-001 (misplaced test doc comment, failure.rs:474-479), ARCH-106 (keyring mark text).
  - From prior rounds, in the bead: ERR-QA-33 remainder (ConnectionReset and BrokenPipe after send stay retryable connect; double-charge decision open), ERR-QA-34 to 38, RBP-F203 to F207, REQ-QA-105, REQ-QA-106, REQ M3, REQ M4.
  - Recorded in bead notes this round, not in any reviewer JSON for a5bab66: failure.rs:129-131 detail arm overwrites provider_type even when error.type is present; empty-string provider sentinel in read_failure and busy_reply; platform.rs:113-149 concurrent timed-out readers (compare_exchange suggested); platform.rs:75,83 NonZeroU64; failure.rs:17-18 malformed doc link.
- agt-gsp (P3, OPEN): RBP-F108 to F112. F108 appeared on the PR at b3e5332. F109 to F112 are only in the bead. None appear in the acdf9ea or a5bab66 reviewer JSON. Closure is not evidenced this round.

## Known limits (from the handoff and the record)

- ERR-QA-6 429 half: no live Jev 429 out-of-tokens reply in the record. Behaviour is busy with up to 5 sends.
- No live Haiku or Luna error body seen (the handoff says this).
- Real locked Keychain not tested.
- The intermittent coverage failure on #39 has no proven cause. CI coverage passed at 3da9128. Not re-checked at a5bab66 by this round.
- Windows is covered only by CI, and CI is still pending on Windows legs at the time of the check.
- Two audit probes made unintended live Jev calls (a4c79b5 audit). Disclosed on the PR.
- Not verified live: Haiku or Luna, a blocked keychain, Windows, sc-jev-py and Python tests in this round.
- The live 402 trace (unfunded organization) is the owner's evidence. It is consistent with the code path. I did not reproduce it.

## Evidence Gaps

1. ERR-QA-6 429 half has no evidence: no live 429 trace, no code change, and a test that pins the busy behaviour. Bead agt-285 closed on the 402 trace only.
2. arch-qa and rust-qa-agent did not review head a5bab66. The failure.rs change is unreviewed by them. Parent `cargo test` run is not reviewer evidence.
3. Provenance of rust-qa-agent: general-purpose substitute (handoff notes).
4. Task records for the six task_ids were not verified.
5. Reviewer fenced JSON for a5bab66 exists only in the handoff file, not on the PR.
6. CI: 6 of 16 checks pending at the time of the check. Not awaited, as instructed.
7. The docs at a5bab66 label ERR-QA-6 "closed" and cite live replies dated 2026-11-08. The live replies are the owner's evidence, not in the repository beyond bead notes.
8. Triage: no `.triage/` records cited for this round. The TTL paths are empty.
9. The pr_comment_url is returned by `gh pr comment` after posting. It cannot appear inside the comment that creates it.

## CI state (reported separately from the QA verdict)

- `gh pr checks 47 --repo randlee/sc-agent` at the time of the check: 10 pass, 6 pending, 0 fail (head a5bab66).
- Not awaited. Merge is not possible on this round regardless (QA verdict FAIL).

## Next Action

triage_and_fix for ERR-QA-6 (429 half), or an explicit owner written disposition. Also: respawn arch-qa and rust-qa-agent at a5bab66 for head coverage, and confirm the rust-qa-agent substitute. Do not merge on this round. cursor-quality-mgr did not approve, merge, spawn reviewers, or fix code.

## Machine Status (JSON)
```json
{
  "qa_pass": "qa-pr47-final-2",
  "pr": 47,
  "branch": "fix/pe-errors",
  "head": "a5bab66",
  "base": "integrate/phase-a",
  "base_sha": "f310e8f",
  "task": "cursor-quality-mgr",
  "verdict": "FAIL",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [
    {"agent": "rust-qa-agent", "task_id": "aaf6b329b8f50d802", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "acdf9ea", "head_covered": false, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "provenance_note": "run by a general-purpose agent following rust-qa-agent.md (handoff notes)"},
    {"agent": "rust-best-practices-agent", "task_id": "aa81e3a1edfe062a0", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "a5bab66", "head_covered": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}, "minor_tracked_in": "agt-4ru"},
    {"agent": "rust-service-hardening-agent", "task_id": "a67852e30ef9fe5c4", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "a4c79b5", "head_covered": false, "verdict": "SKIPPED", "findings": {"blocking": 0, "important": 0, "minor": 0}, "skip_basis": "its own fenced JSON reports skipped: no service indicators"},
    {"agent": "flaky-test-qa", "task_id": "a171134f0d675c2ec", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "acdf9ea", "head_covered": false, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}},
    {"agent": "req-qa", "task_id": "a31e65c810bb509a6", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "a5bab66", "head_covered": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 1}, "minor_tracked_in": "agt-4ru", "minor_ids": ["REQ-QA-001"], "deliverable_completion_percent": 100.0},
    {"agent": "arch-qa", "task_id": "ae65c3755aa48b8cc", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "reviewed_commit": "acdf9ea", "head_covered": false, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 1}, "minor_tracked_in": "agt-4ru", "minor_ids": ["ARCH-106"]}
  ],
  "missing_reviewers": [],
  "unparsed_reviewers": [],
  "open_findings": [
    {"id": "ERR-QA-6", "severity": "Important", "source": "error-contract audit round 2 at b3e5332 (PR comment 6728720287); re-scoped this round to the Jev 429 out-of-tokens case", "state": "open. The 402 billing_error half is evidenced and fixed in message fidelity only (402 was already QUOTA_EXHAUSTED). The 429 half is unchanged, pinned as busy by test, and no live 429 evidence exists. Bead agt-285 closed on 402 only."}
  ],
  "tracked_minor_findings": {
    "agt-4ru": ["ARCH-106", "REQ-QA-001", "ERR-QA-33 remainder", "ERR-QA-34", "ERR-QA-35", "ERR-QA-36", "ERR-QA-37", "ERR-QA-38", "RBP-F203", "RBP-F204", "RBP-F205", "RBP-F206", "RBP-F207", "REQ-QA-105", "REQ-QA-106", "REQ M3", "REQ M4", "failure.rs:129-131 provider_type overwrite (bead note)", "empty-string provider sentinel (bead note)", "platform.rs:113-149 concurrent readers (bead note)", "platform.rs:75,83 NonZeroU64 (bead note)", "failure.rs:17-18 doc link (bead note)"],
    "agt-gsp": ["RBP-F108", "RBP-F109", "RBP-F110", "RBP-F111", "RBP-F112"]
  },
  "findings": {"blocking": 0, "important": 1, "minor_open": 0, "minor_tracked": 27},
  "aggregation_source": "reviewer_fenced_json_union_todo_read_check",
  "todo_scan_hits": "not re-scanned for 3da9128..a5bab66; diff read, no TODO text added",
  "todo_scan_range": "f310e8f...3da9128 (prior round: 0 hits)",
  "ci": {"state": "10 pass, 6 pending, 0 fail at check time (not awaited)", "runs": ["42547160896", "42547160895"], "headSha": "a5bab6666a10d0560929e4451754d54ebb7a279a", "conclusion": "in progress"},
  "head_correlation": {
    "3da9128_to_a5bab66": "3 files: failure.rs (+35, detail arm and test), docs/requirements.md, docs/stability.md (ERR-QA-6 known-limit text removed); verified with git diff; no Cargo or dependency change",
    "acdf9ea_and_a4c79b5_to_a5bab66": "arch-qa, rust-qa-agent and flaky-test-qa reviewed acdf9ea; service-hardening reviewed a4c79b5; only req-qa and rust-best-practices reviewed a5bab66"
  },
  "evidence_chain": {
    "qa_pass": "qa-pr47-final-2",
    "commit": "a5bab66",
    "reviewed_commits": ["acdf9ea", "a4c79b5", "a5bab66"],
    "pr_comment_url": "posted by gh pr comment (see handback)",
    "prior_report_url": "https://github.com/randlee/sc-agent/pull/47#issuecomment-6729772154",
    "coordinator_task_id": "cursor-quality-mgr",
    "triage": {"ttl_paths": []}
  },
  "next_action": "triage_and_fix"
}
```
