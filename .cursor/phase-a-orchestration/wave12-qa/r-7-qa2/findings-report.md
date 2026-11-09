# QA Findings Update

Generated: 2026-11-08T02:51:09Z
QA Pass: qa-r7-2
Sprint/Task: r-7 / cursor-quality-mgr
Branch: `sprint/r-7-decision-schema`
Commit: `4eb8265`
PR: #33
Verdict: **FAIL**

## Machine Status (JSON)
```json
{
  "sprint": "r-7",
  "task": "cursor-quality-mgr",
  "branch": "sprint/r-7-decision-schema",
  "commit": "4eb8265",
  "pr": 33,
  "verdict": "FAIL",
  "deliverables": {
    "complete": 3,
    "total": 3,
    "percent": 100
  },
  "findings": {
    "blocking": 3,
    "important": 2,
    "minor": 5
  },
  "blocking_ids": ["ARCH-001", "ARCH-002", "ARCH-003"],
  "merge_readiness": "not-ready",
  "merge_reason": "Spawn gate passed and req-qa reports 3/3 deliverables, and CI checks are green, but 10 open findings remain (3 Blocking, 2 Important, 5 Minor). Merge is blocked until all are fixed or resolved and QA-3 passes.",
  "next_action": "triage_and_fix",
  "owner": "cursor-orchestration (parent)",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [{"agent": "req-qa", "task_id": "a7e71440cea34884a", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "PASS", "findings": {"blocking": 0, "important": 0, "minor": 1}}, {"agent": "arch-qa", "task_id": "addafbd840ceb9e80", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 3, "important": 0, "minor": 0}}, {"agent": "rust-qa-agent", "task_id": "a54b9258d4ede5c82", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 1, "minor": 2}}, {"agent": "flaky-test-qa", "task_id": "af470d85db0bcc580", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 1, "minor": 2}}],
  "evidence_chain": {"qa_pass": "qa-r7-2", "commit": "4eb8265", "pr_comment_url": "pending", "coordinator_task_id": "cursor-quality-mgr (task id not supplied to this run)", "triage": {"ttl_paths": [], "triage_record_files": ["triage-R7-QA2-ARCH-001.json", "triage-R7-QA2-ARCH-002.json", "triage-R7-QA2-ARCH-003.json", "triage-R7-QA2-RQA-QA-001.json", "triage-R7-QA2-RQA-QA-002.json", "triage-R7-QA2-RQA-QA-003.json", "triage-R7-QA2-FTQ-004.json", "triage-R7-QA2-FTQ-005.json", "triage-R7-QA2-FTQ-006.json", "triage-R7-QA2-REQ-QA-003.json"]}},
  "missing_reviewers": [],
  "unparsed_reviewers": []}
```

## Findings Summary
- Deliverables: 3/3 (100%)
- Blocking: 3
- Important: 2
- Minor: 5

## Blocking Findings
- **ARCH-001** (arch-qa, RULE-ADR-CODE, BLOCKING) `crates/sc-jev-check/src/run/mod.rs:806`: send() suppresses the LogEvent when the token was cancelled at entry, but the result is still recorded as Failed. Conflicts with REQ-SAG-0610 read with ADR-SAG-0013. Remediation: emit one LogEvent for every judge call that reaches judge(), and change `cancelled_before_send_emits_no_log_event` (run/tests.rs:643) to match. Any pre-send NotSent classification needs an Active ADR amendment first.
- **ARCH-002** (arch-qa, RULE-ADR-CODE, BLOCKING) `crates/workspace-tests/Cargo.toml:24`: new dev-dependency on sc-decision-schema contradicts the "exactly" dev-dependency list in ADR-SAG-0028 (docs/architecture.md:1537-1546); no r-7 amendment exists. Remediation: land an Active ADR-SAG-0028 amendment in this sprint and update boundaries/workspace-tests/workspace-tests.toml:31, or remove the edge.
- **ARCH-003** (arch-qa, RULE-ADR-CODE, BLOCKING) `crates/sc-agent-types/src/git.rs:43`: fixed 60 s git deadline and the git-to-EXTRACTION_TIMEOUT mapping are a new public process and error contract with no ADR. Remediation: record it in an ADR-SAG-0027 amendment, or narrow GIT_DEADLINE to pub(crate) and add a git-scoped error variant; also fix the EXTRACTION_TIMEOUT wording in error.rs:160 and docs/sc-jev-check/extraction.md:37.

## Detailed Findings
### Blocking (3)
- **ARCH-001** (arch-qa, RULE-ADR-CODE, BLOCKING) `crates/sc-jev-check/src/run/mod.rs:806`: send() suppresses the LogEvent when the token was cancelled at entry, but the result is still recorded as Failed. Conflicts with REQ-SAG-0610 read with ADR-SAG-0013. Remediation: emit one LogEvent for every judge call that reaches judge(), and change `cancelled_before_send_emits_no_log_event` (run/tests.rs:643) to match. Any pre-send NotSent classification needs an Active ADR amendment first.
- **ARCH-002** (arch-qa, RULE-ADR-CODE, BLOCKING) `crates/workspace-tests/Cargo.toml:24`: new dev-dependency on sc-decision-schema contradicts the "exactly" dev-dependency list in ADR-SAG-0028 (docs/architecture.md:1537-1546); no r-7 amendment exists. Remediation: land an Active ADR-SAG-0028 amendment in this sprint and update boundaries/workspace-tests/workspace-tests.toml:31, or remove the edge.
- **ARCH-003** (arch-qa, RULE-ADR-CODE, BLOCKING) `crates/sc-agent-types/src/git.rs:43`: fixed 60 s git deadline and the git-to-EXTRACTION_TIMEOUT mapping are a new public process and error contract with no ADR. Remediation: record it in an ADR-SAG-0027 amendment, or narrow GIT_DEADLINE to pub(crate) and add a git-scoped error variant; also fix the EXTRACTION_TIMEOUT wording in error.rs:160 and docs/sc-jev-check/extraction.md:37.

### Important (2)
- **QA-001 / R7-QA2-RQA-QA-001** (rust-qa-agent, correctness, important) `crates/sc-agent-types/src/error.rs:841`: git timeouts reuse EXTRACTION_TIMEOUT, whose hint says to raise timeout_seconds, which does not apply to git. Remediation: add a git-specific timeout code or hint, or return EXTRACTION_FAILED with a "git timed out after 60s" detail; update git.rs module doc and the test at git.rs:487.
- **FTQ-004 / R7-QA2-FTQ-004** (flaky-test-qa, masking_fix, important) `crates/workspace-tests/tests/test_util_off.rs:28`: the FTQ-001 fix widens CARGO_TREE_DEADLINE to 5 min, which hides a cargo lock race rather than removing it; tree() is not serialized with BUILD_LOCK. The same widening appears at `crates/sc-agent-types/tests/features.rs:24` and `crates/workspace-tests/tests/support/bin_path.rs:27`. Remediation: serialize or isolate cargo calls (single OnceLock or private CARGO_TARGET_DIR), and restore a near-60 s bound with a message that names lock contention.

### Minor (5)
- **REQ-QA-003 / R7-QA2-REQ-QA-003** (req-qa, plan): `docs/plans/phase-a-refine/sprint-r-7-decision-schema.md:88-179`: `crates/sc-agent-types/src/git.rs` (edited by QA-1 fix RSH-002) is not in owned_paths. Remediation: add the owned_paths entry.
- **QA-002 / R7-QA2-RQA-QA-002** (rust-qa-agent, artifacts): `.github/workflows/sc-agent-ci.yml` lines 256, 276, 280, 316, 320, 535, 542-543 still run cargo without `--locked`. Remediation: add `--locked` to those steps (or set it globally).
- **QA-003 / R7-QA2-RQA-QA-003** (rust-qa-agent, guideline): `crates/sc-jev-check/src/run/mod.rs:50-61`: the "about 17 minutes" worst-case figure does not state whether the Jev call includes the busy-retry budget (960 s vs 1050 s). Remediation: state the Jev bound explicitly and give the total in seconds.
- **FTQ-005 / R7-QA2-FTQ-005** (flaky-test-qa, unbounded_wait): `tests/support/deadline.rs:98` and `:180-187`: cleanup has an unbounded `child.wait()` and an unbounded `kill` subprocess, which contradicts the module doc "Every wait is bounded". Remediation: poll try_wait against a budget and use a direct signal.
- **FTQ-006 / R7-QA2-FTQ-006** (flaky-test-qa, timing_assertion): `crates/sc-agent-types/src/git.rs:488`: `started.elapsed() < 10 s` in `test_hung_git_is_killed_at_the_deadline` is redundant and adds flake exposure. Remediation: delete it and keep the EXTRACTION_TIMEOUT assertion.

### TODO scan
- 0 Rust TODO/FIXME/XXX hits (`find_todos.py` exit 0 on 4eb8265).

### Resolved since QA-1
- R7-QA1-REQ-QA-002, RSH-004, RSH-006 (main steps), RSH-002, FTQ-001..003, RSH-003 were reviewed as closed or partly closed; see the open items above for the residual gaps. Note: RSH-006 is only partly closed (QA-002 above).


## Resolved Since Last Pass
- none

## Merge Readiness
- Status: **not-ready**
- Reason: Spawn gate passed and req-qa reports 3/3 deliverables, and CI checks are green, but 10 open findings remain (3 Blocking, 2 Important, 5 Minor). Merge is blocked until all are fixed or resolved and QA-3 passes.

## Next Action
- Action: triage_and_fix
- Owner: cursor-orchestration (parent)
