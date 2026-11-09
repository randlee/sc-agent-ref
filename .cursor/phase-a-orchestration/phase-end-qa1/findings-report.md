# QA Findings Update

Generated: 2026-11-08T17:17:03Z
QA Pass: qa-phase-end-1
Sprint/Task: phase-a-refine (Phase A.1 integration) / qa-phase-end-1
Branch: `integrate/phase-a`
Commit: `91d36b3`
PR: #36
Verdict: **FAIL**

## Machine Status (JSON)
```json
{
  "sprint": "phase-a-refine (Phase A.1 integration)",
  "task": "qa-phase-end-1",
  "branch": "integrate/phase-a",
  "commit": "91d36b3",
  "pr": 36,
  "verdict": "FAIL",
  "deliverables": {
    "complete": 2,
    "total": 7,
    "percent": 28.6
  },
  "findings": {
    "blocking": 2,
    "important": 3,
    "minor": 2
  },
  "blocking_ids": ["ARCH-001", "REQ-QA-001"],
  "merge_readiness": "not ready",
  "merge_reason": "Spawn gate passed (6/6 reviewers, all with task_id and parsed fenced JSON, all at 91d36b3). Open findings: 2 Blocking, 3 Important, 2 Minor (7 total; TODO scan 0 actionable). req-qa deliverables 2/7 (28.6%). CI at head 91d36b3 green on all checks listed by gh pr checks.",
  "next_action": "Triage and fix all 7 open findings (ARCH-001, REQ-QA-001, ARCH-002, REQ-QA-002, REQ-QA-003, REQ-QA-004, QA-001), then re-QA at a new head. Route fix assignments through cursor-orchestration.",
  "owner": "parent orchestrator (cursor-orchestration)",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [{"agent": "rust-qa-agent", "task_id": "a1e889a0586b90215", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}}, {"agent": "rust-best-practices-agent", "task_id": "a886db6859626ee05", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}}, {"agent": "rust-service-hardening-agent", "task_id": "ab37aadc99b7c4d2f", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "SKIPPED", "findings": {"blocking": 0, "important": 0, "minor": 0}}, {"agent": "flaky-test-qa", "task_id": "aff2241174892cdc1", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}}, {"agent": "req-qa", "task_id": "a6c7d7d0f054d4906", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 1, "important": 2, "minor": 1}}, {"agent": "arch-qa", "task_id": "a9fa798d7f18f1833", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 1, "important": 1, "minor": 0}}],
  "evidence_chain": {"qa_pass": "qa-phase-end-1", "commit": "91d36b3", "pr_comment_url": "pending", "coordinator_task_id": "not-recorded", "triage": {"ttl_paths": [], "record_paths": ["/Volumes/Extreme Pro/github/sc-agent/.cursor/phase-a-orchestration/phase-end-qa1/triage-PE-QA1-ARCH-001.json", "/Volumes/Extreme Pro/github/sc-agent/.cursor/phase-a-orchestration/phase-end-qa1/triage-PE-QA1-ARCH-002.json", "/Volumes/Extreme Pro/github/sc-agent/.cursor/phase-a-orchestration/phase-end-qa1/triage-PE-QA1-QA-001.json", "/Volumes/Extreme Pro/github/sc-agent/.cursor/phase-a-orchestration/phase-end-qa1/triage-PE-QA1-REQ-QA-001.json", "/Volumes/Extreme Pro/github/sc-agent/.cursor/phase-a-orchestration/phase-end-qa1/triage-PE-QA1-REQ-QA-002.json", "/Volumes/Extreme Pro/github/sc-agent/.cursor/phase-a-orchestration/phase-end-qa1/triage-PE-QA1-REQ-QA-003.json", "/Volumes/Extreme Pro/github/sc-agent/.cursor/phase-a-orchestration/phase-end-qa1/triage-PE-QA1-REQ-QA-004.json"]}},
  "missing_reviewers": [],
  "unparsed_reviewers": []}
```

## Findings Summary
- Deliverables: 2/7 (28.6%)
- Blocking: 2
- Important: 3
- Minor: 2

## Blocking Findings
- **ARCH-001** (Blocking, RULE-ADR-CODE, ADR-SAG-0013): `docs/architecture.md:23-40` 'Phase A crate graph' still says only `sc-ai` and `sc-jev-py` depend on full `sc-observability`. Verified at 91d36b3: neither `crates/sc-ai/Cargo.toml` nor `crates/sc-jev-py/Cargo.toml` lists `sc-observability`; `crates/sc-jev-host/Cargo.toml:27` does. Fix: rewrite lines 23-49 to the integrated shape (or mark the graph historical), make the 38-40 sentence say `sc-jev-host` is the only crate linking `sc-observability`, and align `docs/sc-ai`, `docs/sc-jev-py` and `docs/sc-jev-host/architecture.md`.
- **REQ-QA-001** (Blocking, acceptance-gap): `docs/requirements.md:162` and `docs/plans/phase-a-refine/sprint-r-4-adrs-and-manifests.md:183,265-284`. REQ-SAG-0133 is Active, its sole closing sprint r-4 defers the `req:` acceptance row (upstream sc-publish issue URL pending), and the sprint is still marked `status: complete`. Fix: either file the randlee/sc-publish issue and record the URL in the ADR-SAG-0017 r-4 amendment, `docs/development/sc-publish.md` and the requirement row before merge, or change REQ-SAG-0133 through a planned, reviewed requirements change and amend the sprint doc to match.

## Detailed Findings
- **ARCH-001** (Blocking, RULE-ADR-CODE, ADR-SAG-0013) `docs/architecture.md:38` (block 23-49): see Blocking Findings above. Also omits `sc-jev-host`, `sc-agent-http` and `sc-decision-schema` from the graph.
- **REQ-QA-001** (Blocking, acceptance-gap) `docs/requirements.md:162`, `docs/plans/phase-a-refine/sprint-r-4-adrs-and-manifests.md:183`: see Blocking Findings above.
- **ARCH-002** (Important, RULE-ADR-CODE) `docs/requirements.md:148` and paragraph at `:150`: heading 'Phase A.1 - refinement (proposed)' and the '(Proposed, r-N)' marker text remain although no Proposed markers remain at phase end. Fix: retitle to 'Phase A.1 - refinement (landed)' and reword the paragraph so (Active, r-N) records the landing sprint.
- **REQ-QA-002** (Important, plan) `docs/plans/phase-a-refine/plan.md:38-44` and `:905-906`, `docs/requirements.md:148-153`: plan still says `Status: **proposed**`, header omits round 5, Phase acceptance lists only seven of ten sprints (r-7, r-5, r-6 missing), and the 'proposed' anchor is linked from `docs/sc-agent-types/requirements.md:28`. Fix: set plan status to complete (or the state the phase-end gate defines), list all ten sprints, retitle the requirements heading and update the anchor link.
- **REQ-QA-003** (Important, cross-doc-conflict) `docs/requirements.md:157-160,173` vs `docs/sc-agent-types/requirements.md:25-43` and `docs/sc-jev-check/requirements.md:7-31`: REQ-SAG-0105, 0106, 0108 are absent from the sc-agent-types crate table; REQ-SAG-0606 is absent from sc-jev-check and every other crate doc. Fix: add crate-level rows with owning sprint and test reference, or state in each crate doc that the root table is authoritative for these ids.
- **REQ-QA-004** (Minor, deliverable-missing) `docs/plans/phase-a-refine/sprint-r-5-jev-wire-envelope.md:4`, `sprint-r-6-session-log-record.md:4`, `sprint-r-7-decision-schema.md:4`: bare `status: complete` with no merge PR, commit or QA reference (other seven sprints cite them). Fix: add merged PR and commit and QA result to each status line.
- **QA-001** (Minor, coverage, rust-qa-agent) `crates/sc-ai/src/command.rs:1`: default-feature llvm-cov workspace run is 54.1% lines / 58.0% functions; branch 0.0 (no branch data). Zero or near-zero coverage in `sc-ai/src/observe.rs`, `sc-jev-check/src/run/builder.rs`, `sc-jev-backends/src/transport.rs`, `sc-jev-backends/src/retry.rs`, `sc-jev/src/client/https.rs`, `sc-jev/src/api/config.rs`, `sc-agent-types/src/subjects.rs`. Fix: confirm these are exercised only by feature-gated or subprocess suites that llvm-cov does not instrument, and run with those features, or add tests, or record the gap as an accepted risk. Accepting the gap is a parent decision.

## Resolved Since Last Pass
- none

## Merge Readiness
- Status: **not ready**
- Reason: Spawn gate passed (6/6 reviewers, all with task_id and parsed fenced JSON, all at 91d36b3). Open findings: 2 Blocking, 3 Important, 2 Minor (7 total; TODO scan 0 actionable). req-qa deliverables 2/7 (28.6%). CI at head 91d36b3 green on all checks listed by gh pr checks.

## Next Action
- Action: Triage and fix all 7 open findings (ARCH-001, REQ-QA-001, ARCH-002, REQ-QA-002, REQ-QA-003, REQ-QA-004, QA-001), then re-QA at a new head. Route fix assignments through cursor-orchestration.
- Owner: parent orchestrator (cursor-orchestration)
