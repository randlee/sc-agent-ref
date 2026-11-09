# QA Findings Update

Generated: 2026-11-08T06:20:42Z
QA Pass: qa-r5-2
Sprint/Task: r-5 / qa-r5-2
Branch: `sprint/r-5-jev-wire-envelope`
Commit: `085dab4`
PR: #34
Verdict: **FAIL**

## Machine Status (JSON)
```json
{
  "sprint": "r-5",
  "task": "qa-r5-2",
  "branch": "sprint/r-5-jev-wire-envelope",
  "commit": "085dab4",
  "pr": 34,
  "verdict": "FAIL",
  "deliverables": {
    "complete": 13,
    "total": 13,
    "percent": 100
  },
  "findings": {
    "blocking": 0,
    "important": 1,
    "minor": 0
  },
  "blocking_ids": [],
  "merge_readiness": "not ready",
  "merge_reason": "Open Important finding ARCH-002 (arch-qa). CI not green at PR head f6b2b86 (6 checks pending). Reviewed commit 085dab4 is not PR head f6b2b86.",
  "next_action": "Parent runs triaging-findings for ARCH-002, then dispatches a fix that maps decode failures in CheckRequest::parse and CanonicalJudgePayload::from_bytes to InputInvalid with a fix hint. Then QA-3 must cover the fix commit and f6b2b86.",
  "owner": "parent-orchestrator",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [{"agent": "req-qa", "task_id": "a21e4b4b3c4196fa4", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 0}}, {"agent": "arch-qa", "task_id": "ab87438fa57f1a831", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 1, "minor": 0}}, {"agent": "rust-qa-agent", "task_id": "a15ea565daaa7e378", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "pass", "findings": {"blocking": 0, "important": 0, "minor": 0}}, {"agent": "flaky-test-qa", "task_id": "a15ef2689151deee3", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "pass", "findings": {"blocking": 0, "important": 0, "minor": 0}}],
  "evidence_chain": {"qa_pass": "qa-r5-2", "commit": "085dab4", "pr_comment_url": "pending", "coordinator_task_id": "unavailable-in-session", "triage": {"ttl_paths": []}},
  "missing_reviewers": [],
  "unparsed_reviewers": []}
```

## Findings Summary
- Deliverables: 13/13 (100%)
- Blocking: 0
- Important: 1
- Minor: 0

## Blocking Findings
- none (0 Blocking). Open Important: ARCH-002 (see Detailed Findings).

## Detailed Findings
- **ARCH-002** (Important, arch-qa, RULE-ADR-CODE / ADR-SAG-0027): `crates/sc-decision-schema/src/jev/v1.rs:563` (`CheckRequest::parse`) and `crates/sc-decision-schema/src/payload.rs:408-409` (`CanonicalJudgePayload::from_bytes`) map every serde decode failure to `JevError::encode_failed`. The code is INPUT_INVALID as required, but the EncodeFailed hint (`crates/sc-agent-types/src/error.rs:833`) tells the caller to retry or report a bug, which is the wrong guidance for a malformed body. Remediation: return `JevError::input_invalid` with a fix hint carrying only the category and line/column, at both sites, or record a tracked follow-up bead. Triage input: `.cursor/phase-a-orchestration/wave13-qa/r-5-qa2/triage-R5-QA2-ARCH-002.json`.
- Scope note (not a finding): reviewers covered `085dab4`. PR head is now `f6b2b86` (parent CI fix, `Pairs::<K, V>` annotation). `f6b2b86` has not been reviewed and is covered by QA-3.
- CI note (not a finding): at `f6b2b86`, 7 checks pass and 6 are pending (dry-run, python windows-2022, sc-treesitter feature matrix, test macos-15/ubuntu-24.04/windows-2022, coverage). Not awaited per assignment.

## Resolved Since Last Pass
- none

## Merge Readiness
- Status: **not ready**
- Reason: Open Important finding ARCH-002 (arch-qa). CI not green at PR head f6b2b86 (6 checks pending). Reviewed commit 085dab4 is not PR head f6b2b86.

## Next Action
- Action: Parent runs triaging-findings for ARCH-002, then dispatches a fix that maps decode failures in CheckRequest::parse and CanonicalJudgePayload::from_bytes to InputInvalid with a fix hint. Then QA-3 must cover the fix commit and f6b2b86.
- Owner: parent-orchestrator
