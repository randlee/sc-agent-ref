# QA Findings Update

Generated: 2026-11-07
QA Pass: qa-a-10-code-1
Sprint/Task: a-10-code / qa-a-10-code-1
Branch: `sprint/a-10-phase-end-hardening`
Commit: `1899319d1ca643696868c1c69c8d49437ba9f1ae`
PR: #20
Verdict: **FAIL**

## Machine Status (JSON)
```json
{
  "sprint": "a-10-code",
  "task": "qa-a-10-code-1",
  "branch": "sprint/a-10-phase-end-hardening",
  "commit": "1899319d1ca643696868c1c69c8d49437ba9f1ae",
  "pr": 20,
  "verdict": "FAIL",
  "deliverables": {
    "complete": 1,
    "total": 8,
    "percent": 12.5
  },
  "findings": {
    "blocking": 9,
    "important": 6,
    "minor": 12
  },
  "blocking_ids": ["REQ-QA-001", "REQ-QA-002", "REQ-QA-003", "REQ-QA-004", "REQ-QA-005", "REQ-QA-007", "ARCH-001", "ARCH-002", "CI-001"],
  "merge_readiness": "blocked",
  "merge_reason": "Finding gate FAIL: 9 Blocking, 6 Important, 12 Minor open; CI red on windows-2022 (CI-001); deliverables 1/8 per req-qa (no per-id disposition table yet). rust-qa gates all PASS (coverage 90.75% line, every crate \u003e= 80%). Spawn gate pass.",
  "next_action": "triage_and_fix",
  "owner": "parent-orchestrator",
  "reviewer_spawn_gate": "pass",
  "reviewer_manifest": [{"agent": "req-qa", "task_id": "a70f8f0b29a0a4a1e", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 6, "important": 2, "minor": 1}}, {"agent": "arch-qa", "task_id": "a6baa6e0fc335bdcf", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "FAIL", "findings": {"blocking": 2, "important": 1, "minor": 1}}, {"agent": "rust-qa-agent", "task_id": "a83de85669418f00d", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 0, "minor": 2}}, {"agent": "rust-best-practices-agent", "task_id": "afab5676ba29ea249", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 0, "minor": 4}}, {"agent": "rust-service-hardening-agent", "task_id": "a57f2817e2d3a0b42", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 0, "minor": 2}}, {"agent": "flaky-test-qa", "task_id": "a54a99fb6e1b8a843", "spawn_actor": "parent-orchestrator", "fenced_json_received": true, "verdict": "findings", "findings": {"blocking": 0, "important": 3, "minor": 2}}],
  "evidence_chain": {"qa_pass": "qa-a-10-code-1", "commit": "1899319d1ca643696868c1c69c8d49437ba9f1ae", "pr_number": 20, "pr_comment_url": "pending", "coordinator_task_id": "cursor-quality-mgr-fallback", "reviewer_tasks": {"req-qa": "a70f8f0b29a0a4a1e", "arch-qa": "a6baa6e0fc335bdcf", "rust-qa-agent": "a83de85669418f00d", "rust-best-practices-agent": "afab5676ba29ea249", "rust-service-hardening-agent": "a57f2817e2d3a0b42", "flaky-test-qa": "a54a99fb6e1b8a843"}, "ci": {"failing_check": "test (windows-2022)", "finding": "CI-001"}, "triage": {"phase_id": "phase-a", "ttl_paths": [], "qa_triage_task_ids": []}, "parent_correlation_required": true},
  "missing_reviewers": [],
  "unparsed_reviewers": []}
```

## Findings Summary
- Deliverables: 1/8 (12.5%)
- Blocking: 9
- Important: 6
- Minor: 12

## Blocking Findings
- **REQ-QA-001** (Blocking, req-qa) `docs/architecture.md:155-163 (ADR-SAG-0022 text) says test-util adds keyword-only scripted_answers to run_check; code ha` — Correction: Rewrite the bullet to match (planned ADR text edit — doc-only, authorised by EXT-003/ARCHP-018 routine items).
- **REQ-QA-002** (Blocking, req-qa) `docs/sc-jev-py/architecture.md:26-29 does not name run_check_scripted as the test-util entry; also sprint-a-8:70,120 wor` — Correction: Name it; state run_check never takes scripted_answers.
- **REQ-QA-003** (Blocking, req-qa) `docs/plans/phase-a/plan.md:406,408,423-425 still say doc gate continue-on-error and feature matrix check-only.` — Correction: Update rows/bullet; keep run ids as historical evidence; cross-check ci.md vs sc-agent-ci.yml.
- **REQ-QA-004** (Blocking, req-qa) `boundaries/sc-jev-py/jev-py.toml:24-25 facade lists Rust paths now pub(crate); notes omit run_check_scripted.` — Correction: Redefine facade as the Python module surface; update notes; sc-boundary must still pass.
- **REQ-QA-005** (Blocking, req-qa) `Error-code changes (Python unknown/kebab backend CONFIG_INVALID->INPUT_INVALID; encode_failed/io_failed INPUT_INVALID; s` — Correction: Confirm against tests/fixtures/jev/README-contract/errors.json + ADR-SAG-0001 (codes exist, no new ids); add a requirement/ADR note recording the remaps so REQ-SAG-0800/0101 can be marked upheld.
- **REQ-QA-007** (Blocking, req-qa) `No per-id disposition table (implemented/deferred+reason, file:line) for the ~90 assigned routine ids in the repo.` — Correction: Developer writes the table (e.g. docs/plans/phase-a/a-10-hardening-dispositions.md) from the three wave assignments; QA-2 re-verifies.
- **ARCH-001** (Blocking, arch-qa) `docs/architecture.md:155` — Correction: Add an ADR (or amendment, e.g. ADR-SAG-0023 amending ADR-SAG-0022) that records the _testing submodule with run_check_scripted, the single run_check signature, the pub ScriptedJudge/RequestLog under sc-jev-check test-util, and shorthand normalisation inside register_scripted_backends. Update the ADR index. This must be Active or Approved before merge.
- **ARCH-002** (Blocking, arch-qa) `docs/architecture.md:226` — Correction: Amend ADR-SAG-0020 by a later ADR. It should record that the doubles are crate-private cfg(test) code with no feature, that ResolvedApiKey::try_new is the one construction rule set, and that clippy.toml disallowed-methods restricts as_str to the two sending transports (crates/sc-jev/src/client/https.rs:84-88, crates/sc-jev-backends/src/transport.rs:81-85).
- **CI-001** (Blocking, ci) `crates/sc-treesitter tests/cli.rs (windows-2022)` — Correction: fix Windows SHA-256 repo test

## Detailed Findings
- **REQ-QA-001** (Blocking, req-qa) `docs/architecture.md:155-163 (ADR-SAG-0022 text) says test-util adds keyword-only scripted_answers to run_check; code ha` — Correction: Rewrite the bullet to match (planned ADR text edit — doc-only, authorised by EXT-003/ARCHP-018 routine items).
- **REQ-QA-002** (Blocking, req-qa) `docs/sc-jev-py/architecture.md:26-29 does not name run_check_scripted as the test-util entry; also sprint-a-8:70,120 wor` — Correction: Name it; state run_check never takes scripted_answers.
- **REQ-QA-003** (Blocking, req-qa) `docs/plans/phase-a/plan.md:406,408,423-425 still say doc gate continue-on-error and feature matrix check-only.` — Correction: Update rows/bullet; keep run ids as historical evidence; cross-check ci.md vs sc-agent-ci.yml.
- **REQ-QA-004** (Blocking, req-qa) `boundaries/sc-jev-py/jev-py.toml:24-25 facade lists Rust paths now pub(crate); notes omit run_check_scripted.` — Correction: Redefine facade as the Python module surface; update notes; sc-boundary must still pass.
- **REQ-QA-005** (Blocking, req-qa) `Error-code changes (Python unknown/kebab backend CONFIG_INVALID->INPUT_INVALID; encode_failed/io_failed INPUT_INVALID; s` — Correction: Confirm against tests/fixtures/jev/README-contract/errors.json + ADR-SAG-0001 (codes exist, no new ids); add a requirement/ADR note recording the remaps so REQ-SAG-0800/0101 can be marked upheld.
- **REQ-QA-007** (Blocking, req-qa) `No per-id disposition table (implemented/deferred+reason, file:line) for the ~90 assigned routine ids in the repo.` — Correction: Developer writes the table (e.g. docs/plans/phase-a/a-10-hardening-dispositions.md) from the three wave assignments; QA-2 re-verifies.
- **ARCH-001** (Blocking, arch-qa) `docs/architecture.md:155` — Correction: Add an ADR (or amendment, e.g. ADR-SAG-0023 amending ADR-SAG-0022) that records the _testing submodule with run_check_scripted, the single run_check signature, the pub ScriptedJudge/RequestLog under sc-jev-check test-util, and shorthand normalisation inside register_scripted_backends. Update the ADR index. This must be Active or Approved before merge.
- **ARCH-002** (Blocking, arch-qa) `docs/architecture.md:226` — Correction: Amend ADR-SAG-0020 by a later ADR. It should record that the doubles are crate-private cfg(test) code with no feature, that ResolvedApiKey::try_new is the one construction rule set, and that clippy.toml disallowed-methods restricts as_str to the two sending transports (crates/sc-jev/src/client/https.rs:84-88, crates/sc-jev-backends/src/transport.rs:81-85).
- **CI-001** (Blocking, ci) `crates/sc-treesitter tests/cli.rs (windows-2022)` — Correction: fix Windows SHA-256 repo test
- **REQ-QA-006** (Important, req-qa) `sc-treesitter API shape changes (languages() iterator, queries() Result<Vec>) and leading-dash rev rejection unrecorded ` — Correction: Record in docs/sc-treesitter/architecture.md or confirm internal.
- **REQ-QA-008** (Important, req-qa) `Could not confirm no D1-D13 code landed; serde spelling observation and try_new sealing touch D-adjacent areas.` — Correction: Developer states per D1-D13 that no code changed; cite the routine id authorising each code-change (FromStr follows documented spelling; serde unchanged; try_new = ARCHC-014/RBP-F010).
- **ARCH-003** (Important, arch-qa) `boundaries/sc-agent-secrets/agent-secrets.toml:24` — Correction: Remove MapEnv and MapKeyring from the facade and correct the notes.
- **FTQ-006** (Important, flaky-test-qa) `crates/sc-jev/src/test_server.rs (byte-identical copy: crates/sc-jev-backends/src/test_server.rs):88` — Correction: Redesign the helper with a hard deadline. Put the listener in nonblocking mode, or use `set_read_timeout` plus an accept loop bounded by `Instant`. Set `stream.set_read_timeout(Some(..))` in serve_one. Have `finish()` signal shutdown (e.g. an AtomicBool or a channel that stops the accept loop) and wait with `recv_timeout`. It should then panic with 'server expected N connections, saw M' instead of joining unboundedly. Do the same in both copies, or move the helper to one shared dev-dependency crate so the copies cannot drift.
- **FTQ-007** (Important, flaky-test-qa) `crates/sc-jev-py/src/run.rs (Python twin: tests/fixtures/jev-python/tests/test_run_check_and_ask.py):1081` — Correction: Replace the tick count with a handshake that is deterministic with a hard bound. Have the Python worker thread create a sentinel file (or set a flag the extractor can observe). Make the command extractor a small python script that polls for the sentinel for up to about 20 s. If the sentinel appears it prints a non-subjects payload, so the build fails with EXTRACTION_INVALID. If it never appears it exits non-zero, so the build fails with EXTRACTION_FAILED. Assert the EXTRACTION_INVALID outcome. The worker can only write the sentinel if the GIL was released, so the test fails deterministically if the GIL is held and never depends on a rate. Use try/finally (Python) or a drop guard (Rust) to stop and join the worker.
- **FTQ-008** (Important, flaky-test-qa) `crates/sc-jev-check/src/extract/mod.rs:159` — Correction: Route all three through a bounded runner (kill, reap and panic with captured stderr after a hard deadline). The runner is the `spawn_with_deadline` pattern from crates/sc-treesitter/tests/support/mod.rs. Put it in one shared dev-support location rather than another copy. Also add `GIT_CONFIG_NOSYSTEM=1` to sc-jev-check's `git_command`.
- **REQ-QA-009** (Minor, req-qa) `ADR-SAG-0020 still mentions removed secrets test-util feature; sprint-a-4 lists src/api/**; confirm boundaries/sc-jev/je` — Correction: Close in this PR (wave 2 already changed jev.toml module line; verify).
- **ARCH-004** (Minor, arch-qa) `docs/architecture.md:361` — Correction: Resolve with the D8 ADR amendment of the version text. No code change.
- **QA-004** (Minor, rust-qa-agent) `crates/sc-jev-py/src/logging.rs:263` — Correction: Move the adapter next to LogEvent in sc-agent-types::observability::v2, or into a shared helper, and call it from both sites. Alternatively, record the duplication as an accepted risk and add a cross-crate equivalence test.
- **QA-007** (Minor, rust-qa-agent) `crates/sc-treesitter/src/git_io.rs:247` — Correction: Share one lookup, or document why the two error mappings must differ.
- **RBP-F010** (Minor, rust-best-practices-agent) `crates/sc-agent-types/src/secret.rs:91` — Correction: Gate `new` behind #[cfg(any(test, feature = "test-util"))]. The test-util feature already exists on sc-agent-types, and every in-repo caller of `new` is a test or test double. That makes try_new the only production constructor. Zeroizing storage can be deferred.
- **RBP-F011** (Minor, rust-best-practices-agent) `crates/sc-agent-types/src/observability/v2.rs:27` — Correction: Add `enum LogOperation { Judge, Ask }` with Display. Add a QuestionId newtype and a BlobSha newtype that reuses the 40-hex validator. If the stringly operation stays, record the decision in a doc line.
- **RBP-F015** (Minor, rust-best-practices-agent) `crates/sc-jev-check/src/session/mod.rs:513` — Correction: Route the clock failures through JevError::session_store_io or operational_config, and utf8_payload through operational_io or session_document_invalid, so the hint matches the failure.
- **RBP-F002** (Minor, rust-best-practices-agent) `crates/sc-jev-py/src/error.rs:22` — Correction: In to_py_err, either ignore setattr with a comment saying why (PyErr attribute set on a fresh exception cannot fail), or fold the failure into the exception text. For read.rs:60, keep `Utf8Error::valid_up_to()` in the detail so the caller learns where the bad byte is.
- **RSH-007** (Minor, rust-service-hardening-agent) `crates/sc-treesitter/src/git_io.rs:71` — Correction: Mirror read.rs. In read_worktree, open the file and use take(MAX+1) with a metadata length check. In read_at_rev, run `git cat-file -s <sha>` after rev-parse and refuse above the cap, then read the blob. Reuse the same 8 MiB constant and return JevError::InputTooLarge.
- **RSH-008** (Minor, rust-service-hardening-agent) `crates/sc-jev-check/src/extract/read.rs:135` — Correction: Resolve the spec once with `git rev-parse --verify --end-of-options`. Then run -t, -s and `cat-file blob` against the returned object id, which also makes the blob call immune to option-like input. Alternatively, spawn the blob read with a bounded reader.
- **FTQ-009** (Minor, flaky-test-qa) `crates/sc-jev/src/test_server.rs (copy: crates/sc-jev-backends/src/test_server.rs):93` — Correction: Keep the listener alive but never accept on it. Make the helper return a guard that holds a bound listener with backlog 0, or use a connect target that is guaranteed to refuse. Then the port cannot be reallocated while the test runs. For the redirect target, the URL is never contacted, so use a reserved name such as `http://invalid.test/`.
- **FTQ-010** (Minor, flaky-test-qa) `crates/sc-jev/src/client/https.rs:229` — Correction: Make Stall block on a release signal instead of a sleep. Use a channel or `Barrier` that the test fires after the client call returns, with `recv_timeout` as the bound. The server then holds the connection open for exactly as long as the client is waiting, and the test has no wall-clock dependency apart from the client timeout under test. Use a client timeout that is small relative to a generous bound.

## Resolved Since Last Pass
- Carry-forwards verified closed this round: RSH-001..006, QA-001/002/003/005/006/008, FTQ-002/005, RBP-F004/F014.

## Merge Readiness
- Status: **blocked**
- Reason: Finding gate FAIL: 9 Blocking, 6 Important, 12 Minor open; CI red on windows-2022 (CI-001); deliverables 1/8 per req-qa (no per-id disposition table yet). rust-qa gates all PASS (coverage 90.75% line, every crate >= 80%). Spawn gate pass.

## Next Action
- Action: triage_and_fix
- Owner: parent-orchestrator
