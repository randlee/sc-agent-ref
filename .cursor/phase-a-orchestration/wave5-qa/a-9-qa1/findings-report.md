# QA Findings Update

Generated: 2026-11-06
QA Pass: qa-a-9-1
Sprint/Task: a-9 / qa-a-9-1
Branch: `sprint/a-9-sc-publish-install`
Commit: `d2738c8f535c28cedc87fc4693f2d78e2bc0d47c`
PR: #17
Verdict: **FAIL**

## Machine Status (JSON)
```json
{
  "sprint": "a-9",
  "task": "qa-a-9-1",
  "branch": "sprint/a-9-sc-publish-install",
  "commit": "d2738c8f535c28cedc87fc4693f2d78e2bc0d47c",
  "pr": 17,
  "verdict": "FAIL",
  "deliverables": {
    "complete": 7,
    "total": 10,
    "percent": 70.0
  },
  "findings": {
    "blocking": 2,
    "important": 4,
    "minor": 14
  },
  "blocking_ids": ["REQ-QA-001", "ARCH-001"],
  "merge_readiness": "blocked",
  "merge_reason": "Finding gate FAIL: 2 Blocking, 4 Important, 14 Minor open (reviewer fenced JSON union TODO scan with 0 hits); deliverables 7/10 (70%) per req-qa. Spawn gate and CI are green.",
  "next_action": "triage_and_fix",
  "owner": "parent-orchestrator",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [{"agent": "req-qa", "task_id": "a93135f5a7f6a7687", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 1, "important": 1, "minor": 3}}, {"agent": "arch-qa", "task_id": "a9503f1935185c6d9", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 1, "important": 1, "minor": 0}}, {"agent": "rust-qa-agent", "task_id": "ae07ee917779ae499", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 0, "minor": 4}}, {"agent": "rust-best-practices-agent", "task_id": "abfe8d86ded68518a", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 1, "minor": 5}}, {"agent": "rust-service-hardening-agent", "task_id": "a8a4e204458debee7", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "SKIPPED", "findings": {"blocking": 0, "important": 0, "minor": 0}}, {"agent": "flaky-test-qa", "task_id": "ae1c92ee207fbcc04", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 1, "minor": 2}}],
  "evidence_chain": {"qa_pass": "qa-a-9-1", "commit": "d2738c8f535c28cedc87fc4693f2d78e2bc0d47c", "pr_number": 17, "pr_comment_url": "https://github.com/randlee/sc-agent/pull/17#issuecomment-6702488738", "coordinator_task_id": "cursor-quality-mgr-fallback", "reviewer_tasks": ["a93135f5a7f6a7687", "a9503f1935185c6d9", "ae07ee917779ae499", "abfe8d86ded68518a", "a8a4e204458debee7", "ae1c92ee207fbcc04"], "triage": {"phase_id": "phase-a", "ttl_paths": [], "qa_triage_task_ids": []}, "parent_correlation_required": true},
  "missing_reviewers": [],
  "unparsed_reviewers": []}
```

## Findings Summary
- Deliverables: 7/10 (70.0%)
- Blocking: 2
- Important: 4
- Minor: 14

## Blocking Findings
- **REQ-QA-001** (Blocking, req-qa) `docs/plans/phase-a/sprint-a-9-sc-publish-install.md:72, docs/plans/phase-a/sprint-a-9-sc-publish-install.md:52, docs/requirements.md:132 (targets .github/workflows/release-preflight.yml:3-4, .github/workflows/sc-publish-dry-run.yml:11-13, docs/development/sc-publish.md:96-101)` — Correction: Amend REQ-SAG-0132, sprint acceptance row and deliverable 3 to name 'sc-publish dry-run workflow green on PR'; state kit release-preflight is dispatch-only (run by publisher at release time); reference green PR #17 run as evidence.
- **ARCH-001** (Blocking, arch-qa) `docs/plans/phase-a/sprint-a-9-sc-publish-install.md:20` — Correction: Add ADR-SAG-0011 and ADR-SAG-0022 to sprint adrs list; re-run plan check.

## Detailed Findings
- **REQ-QA-001** (Blocking, req-qa) `docs/plans/phase-a/sprint-a-9-sc-publish-install.md:72, docs/plans/phase-a/sprint-a-9-sc-publish-install.md:52, docs/requirements.md:132 (targets .github/workflows/release-preflight.yml:3-4, .github/workflows/sc-publish-dry-run.yml:11-13, docs/development/sc-publish.md:96-101)` — Correction: Amend REQ-SAG-0132, sprint acceptance row and deliverable 3 to name 'sc-publish dry-run workflow green on PR'; state kit release-preflight is dispatch-only (run by publisher at release time); reference green PR #17 run as evidence.
- **ARCH-001** (Blocking, arch-qa) `docs/plans/phase-a/sprint-a-9-sc-publish-install.md:20` — Correction: Add ADR-SAG-0011 and ADR-SAG-0022 to sprint adrs list; re-run plan check.
- **REQ-QA-002** (Important, req-qa) `docs/plans/phase-a/sprint-a-9-sc-publish-install.md:23-34` — Correction: Add the four out-of-scope paths to owned_paths; note reasons in Deliverables; set status to in-review until QA closes. _[Triage flag: Likely duplicate of ARCH-002; both ids retained]_
- **ARCH-002** (Important, arch-qa) `docs/plans/phase-a/sprint-a-9-sc-publish-install.md:23` — Correction: Add scripts/sc-publish-dry-run.sh, .github/workflows/sc-publish-dry-run.yml, crates/sc-jev-py/Cargo.toml, crates/sc-jev-py/pyproject.toml to owned_paths; record consumer-owned dry-run gate and sc-jev-py PyPI-only decision in sprint doc or ADR-SAG-0018 amendment. _[Triage flag: Likely duplicate of REQ-QA-002; both ids retained]_
- **RBP-F001** (Important, rust-best-practices-agent) `crates/sc-agent-types/Cargo.toml:19` — Correction: Use caret ranges in library crates (exact pins only for leaf binaries) or document the lockstep reason beside the dependency.
- **FTQ-001** (Important, flaky-test-qa) `.github/workflows/sc-publish-dry-run.yml:30` — Correction: Pin actions to full commit SHAs; pin maturin exactly; use ubuntu-24.04; dtolnay/rust-toolchain@<sha> with toolchain 1.94.1. No retries. _[Triage flag: Likely duplicate of QA-003; both ids retained]_
- **REQ-QA-003** (Minor, req-qa) `docs/plans/phase-a/sprint-a-9-sc-publish-install.md:15-19` — Correction: Add NFR-SAG-0002 and REQ-SAG-0802 to the sprint requirements list.
- **REQ-QA-004** (Minor, req-qa) `docs/plans/phase-a/sprint-a-9-sc-publish-install.md:95-96 (scripts/sc-publish-dry-run.sh:25)` — Correction: Update sprint validation block to use SC_AGENT_WHEEL_DIR or the script.
- **REQ-QA-005** (Minor, req-qa) `docs/requirements.md:130 (release/sc-publish-consumer-input.json:135-198, docs/development/sc-publish.md:46-48)` — Correction: Add one sentence to docs/development/sc-publish.md: crates.io is enabled by the crates list, not a channels key.
- **QA-001** (Minor, rust-qa-agent) `crates/sc-agent-types/Cargo.toml:26` — Correction: Document as expected, set test=false for jev_cross_impl when packaging, or move the fixture inside the crate; keep CI running it from workspace. _[Triage flag: Likely duplicate of RBP-F003; both ids retained]_
- **QA-002** (Minor, rust-qa-agent) `.github/workflows/sc-publish-dry-run.yml:42` — Correction: Add toolchain to cache key plus restore-keys prefix, or use Swatinem/rust-cache.
- **QA-003** (Minor, rust-qa-agent) `.github/workflows/sc-publish-dry-run.yml:30` — Correction: Pin third-party actions to commit SHAs. _[Triage flag: Likely duplicate of FTQ-001; both ids retained]_
- **QA-004** (Minor, rust-qa-agent) `scripts/sc-publish-dry-run.sh:72` — Correction: Use mktemp -d when SC_AGENT_WHEEL_DIR unset or guard rm by sentinel; parse pin with python3 tomllib; validate repository URL. _[Triage flag: Likely duplicate of FTQ-002; both ids retained]_
- **RBP-F002** (Minor, rust-best-practices-agent) `crates/sc-agent-types/Cargo.toml:10` — Correction: Per-crate README + readme key; up to 5 keywords and 1-2 categories; share via [workspace.package].
- **RBP-F003** (Minor, rust-best-practices-agent) `crates/sc-agent-types/Cargo.toml:26` — Correction: Explicit include allowlist per crate (keep sc-treesitter/assets/queries/**); move or exclude cross-impl test; add LICENSE files. _[Triage flag: Likely duplicate of QA-001; both ids retained]_
- **RBP-F004** (Minor, rust-best-practices-agent) `crates/sc-jev-check/Cargo.toml:25` — Correction: Hoist toml, ureq, observability pins to [workspace.dependencies]; one toml major.
- **RBP-F005** (Minor, rust-best-practices-agent) `crates/sc-treesitter/Cargo.toml:18` — Correction: Gate mimalloc behind default-on feature with required-features on the bin, or document the cost.
- **RBP-F006** (Minor, rust-best-practices-agent) `crates/sc-jev-py/pyproject.toml:14` — Correction: Keep static version while sync check runs in CI; add readme/authors/urls/classifiers; confirm dry-run workflow fails on divergence.
- **FTQ-002** (Minor, flaky-test-qa) `scripts/sc-publish-dry-run.sh:25` — Correction: Default wheel dir to mktemp -d with trap cleanup or under target/; git reset --hard && clean -fdx kit checkout after pin checkout. _[Triage flag: Likely duplicate of QA-004; both ids retained]_
- **FTQ-003** (Minor, flaky-test-qa) `scripts/sc-publish-dry-run.sh:38` — Correction: Wrap network steps in timeout with per-step message; set pip --timeout, CARGO_HTTP_TIMEOUT, CARGO_NET_RETRY; per-step timeout-minutes. No retry loops.

## Resolved Since Last Pass
- none

## Merge Readiness
- Status: **blocked**
- Reason: Finding gate FAIL: 2 Blocking, 4 Important, 14 Minor open (reviewer fenced JSON union TODO scan with 0 hits); deliverables 7/10 (70%) per req-qa. Spawn gate and CI are green.

## Next Action
- Action: triage_and_fix
- Owner: parent-orchestrator
