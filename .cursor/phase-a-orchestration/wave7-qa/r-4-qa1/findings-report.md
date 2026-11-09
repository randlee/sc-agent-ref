# QA Findings Update

Generated: 2026-11-07
QA Pass: qa-r-4-1
Sprint/Task: r-4 / qa-r-4-1
Branch: `sprint/r-4-adrs-and-manifests`
Commit: `59ec83882480f05ff4b1203d48ecd834afd8aced`
PR: #21
Verdict: **FAIL**

## Machine Status (JSON)
```json
{
  "sprint": "r-4",
  "task": "qa-r-4-1",
  "branch": "sprint/r-4-adrs-and-manifests",
  "commit": "59ec83882480f05ff4b1203d48ecd834afd8aced",
  "pr": 21,
  "verdict": "FAIL",
  "deliverables": {
    "complete": 6,
    "total": 9,
    "percent": 66.7
  },
  "findings": {
    "blocking": 2,
    "important": 4,
    "minor": 9
  },
  "blocking_ids": ["REQ-QA-R4-001", "REQ-QA-R4-002"],
  "merge_readiness": "blocked",
  "merge_reason": "Finding gate FAIL: 2 Blocking, 4 Important, 9 Minor. Root cause of 4 findings: root sc-observability-types pin stays until r-1 merges (plan Wave-1 order) — transitional lockstep entry + restored sc-lint record requested; cargo-tree-gates greps stderr (FTQ-001); upstream sc-publish issue URL is an owner action (REQ-QA-R4-002). rust-qa gates all green, coverage 90.95% unchanged. r-4 merges after r-1 by plan.",
  "next_action": "triage_and_fix",
  "owner": "parent-orchestrator",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [{"agent": "req-qa", "task_id": "a083a869ccc67cfa1", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 2, "important": 0, "minor": 3}}, {"agent": "arch-qa", "task_id": "a773afa17a63cc175", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 1}}, {"agent": "rust-qa-agent", "task_id": "a21dcf0ab71d5b57b", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 1, "minor": 2}}, {"agent": "rust-best-practices-agent", "task_id": "abfdda82981ff7f78", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 1, "minor": 0}}, {"agent": "rust-service-hardening-agent", "task_id": "adeac310a614593cc", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 1, "minor": 1}}, {"agent": "flaky-test-qa", "task_id": "a495666e9d95793cd", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 1, "minor": 2}}],
  "evidence_chain": {"qa_pass": "qa-r-4-1", "commit": "59ec83882480f05ff4b1203d48ecd834afd8aced", "pr_number": 21, "pr_comment_url": "pending", "coordinator_task_id": "cursor-quality-mgr-fallback", "reviewer_tasks": {"req-qa": "a083a869ccc67cfa1", "arch-qa": "a773afa17a63cc175", "rust-qa-agent": "a21dcf0ab71d5b57b", "rust-best-practices-agent": "abfdda82981ff7f78", "rust-service-hardening-agent": "adeac310a614593cc", "flaky-test-qa": "a495666e9d95793cd"}, "ci": {"failing_check": "dependency gates (allocator, pins)", "cause": "root sc-observability-types =1.5.0 not on lockstep list (pending r-1)"}, "triage": {"phase_id": "phase-a.1", "ttl_paths": [], "qa_triage_task_ids": []}, "parent_correlation_required": true},
  "missing_reviewers": [],
  "unparsed_reviewers": []}
```

## Findings Summary
- Deliverables: 6/9 (66.7%)
- Blocking: 2
- Important: 4
- Minor: 9

## Blocking Findings
- **REQ-QA-R4-001** (Blocking, req-qa) `Cargo.toml:39` — Correction: Rule 'pending r-1' acceptable for QA-1 only as a conditional hold. The branch cannot merge, and NFR-SAG-0004 stays unproven, until r-1 is on integrate/phase-a. Then rebase, confirm Cargo.toml:39 is gone, and re-run check-exact-pins.sh plus --self-test. Run the main.rs merge-base diff against the rebased base, since it is meaningful only after the rebase. Re-run req-qa after the rebase (QA-2) rather than passing now.
- **REQ-QA-R4-002** (Blocking, req-qa) `docs/architecture.md:609-610` — Correction: File the upstream sc-publish issue (Scoop and WinGet multi-binary support). Record its URL in the ADR-SAG-0017 r-4 amendment (docs/architecture.md:609-610) and docs/development/sc-publish.md:57-58 and 66-68. Confirm bead agt-scoop-multi-shim exists. Alternatively, edit the acceptance row through a reviewed plan change. A repo-local placeholder is not compliance.

## Detailed Findings
- **REQ-QA-R4-001** (Blocking, req-qa) `Cargo.toml:39` — Correction: Rule 'pending r-1' acceptable for QA-1 only as a conditional hold. The branch cannot merge, and NFR-SAG-0004 stays unproven, until r-1 is on integrate/phase-a. Then rebase, confirm Cargo.toml:39 is gone, and re-run check-exact-pins.sh plus --self-test. Run the main.rs merge-base diff against the rebased base, since it is meaningful only after the rebase. Re-run req-qa after the rebase (QA-2) rather than passing now.
- **REQ-QA-R4-002** (Blocking, req-qa) `docs/architecture.md:609-610` — Correction: File the upstream sc-publish issue (Scoop and WinGet multi-binary support). Record its URL in the ADR-SAG-0017 r-4 amendment (docs/architecture.md:609-610) and docs/development/sc-publish.md:57-58 and 66-68. Confirm bead agt-scoop-multi-shim exists. Alternatively, edit the acceptance row through a reviewed plan change. A repo-local placeholder is not compliance.
- **QA-001** (Important, rust-qa-agent) `sc-lint.toml:86` — Correction: Keep the sc-observability-types ownership rule until r-1 deletes the dependency, and remove it in r-1. Alternatively, state in the sprint doc that the gap is accepted until r-1.
- **RBP-F001** (Important, rust-best-practices-agent) `Cargo.toml:39` — Correction: Keep the list as it is. Before final validation, rebase onto integrate/phase-a once r-1 has merged, since r-1 deletes the user and the root entry, then re-run the script. If r-1 cannot merge first, put the exact pin on the list temporarily and say so in the PR. The sprint plan already sets the merge order (r-1, then r-4).
- **RSH-001** (Important, rust-service-hardening-agent) `Cargo.toml:39` — Correction: Either relax the pin to a caret range ("1.5"), or add sc-observability-types to the lockstep list with its reason until r-1 removes it. Prefer the caret range if the ADR is the source of truth. Confirm by running `bash .github/scripts/check-exact-pins.sh` on the branch.
- **FTQ-001** (Important, flaky-test-qa) `.github/scripts/cargo-tree-gates.sh:43` — Correction: Redesign the capture so only stdout is grepped: `tree=$(cargo tree --locked ... 2>"$errfile")`, and print stderr only when cargo fails. Alternatively run `cargo fetch --locked` as an explicit earlier step. Add --locked to the tree calls and to the two `cargo build` steps so Cargo.lock drift fails deterministically. Do not add retries or a filter that hides specific 'Downloaded' lines.
- **REQ-QA-R4-003** (Minor, req-qa) `docs/requirements.md:164` — Correction: Add REQ-SAG-0135 to the requirements list as 'constrained by', or drop the citation from sc-lint.toml:97-98.
- **REQ-QA-R4-004** (Minor, req-qa) `Cargo.lock` — Correction: Add Cargo.lock to owned_paths, or state in the PR that it is a derived file. Confirm it contains no unrelated churn.
- **REQ-QA-R4-005** (Minor, req-qa) `docs/plans/phase-a-refine/sprint-r-4-adrs-and-manifests.md:182,184-203` — Correction: QA must attach validation logs, and the PR 21 body must state the clap manifest deferral (CPR-M212).
- **ARCH-001** (Minor, arch-qa) `docs/architecture.md:847` — Correction: Drop or update the note in a later doc pass. It is outside r-4's owned sections, so it is not a violation.
- **QA-002** (Minor, rust-qa-agent) `.github/workflows/sc-agent-ci.yml:258` — Correction: Confirm r-1 merges before or together with r-4 on integrate/phase-a, or sequence the step. This matches the expectation in the task notes, so it is informational.
- **QA-003** (Minor, rust-qa-agent) `.github/scripts/check-exact-pins.sh:45` — Correction: Document that the script is POSIX/ubuntu-only, or resolve the interpreter (`python3` if available, otherwise `python`) if it must run on Windows bash.
- **RSH-002** (Minor, rust-service-hardening-agent) `.github/workflows/sc-agent-ci.yml:213` — Correction: Add a SHA-pinned, version-pinned cargo-deny or cargo-audit step to the `deps` job, or record in the ADR that it is covered elsewhere, for example by sc-publish-dry-run.yml.
- **FTQ-002** (Minor, flaky-test-qa) `crates/sc-treesitter/Cargo.toml:51` — Correction: No sleep or retry is involved. Add a deterministic guard: a CI step running `cargo test -p sc-treesitter --test cli --features cli` and the same for sc-ai. Cargo hard-errors if the target is missing or its features are unsatisfied. Alternatively, assert that `cargo test --list` output includes cli tests. Document the intended skip in ci.md.
- **FTQ-003** (Minor, flaky-test-qa) `.github/scripts/check-exact-pins.sh:151` — Correction: Have the expect helper assert exit code 1 and a distinguishing substring in out.txt. Use 'NOT LISTED' for the pin cases and 'exactly once each' for the markers case. This is a stricter assertion, not a relaxed one.

## Resolved Since Last Pass
- n/a (first round)

## Merge Readiness
- Status: **blocked**
- Reason: Finding gate FAIL: 2 Blocking, 4 Important, 9 Minor. Root cause of 4 findings: root sc-observability-types pin stays until r-1 merges (plan Wave-1 order) — transitional lockstep entry + restored sc-lint record requested; cargo-tree-gates greps stderr (FTQ-001); upstream sc-publish issue URL is an owner action (REQ-QA-R4-002). rust-qa gates all green, coverage 90.95% unchanged. r-4 merges after r-1 by plan.

## Next Action
- Action: triage_and_fix
- Owner: parent-orchestrator
