# r-2 QA-1 fix round (PR 27, reviewed 0d643f2)

Worktree: /Volumes/Extreme Pro/github/sc-agent-worktrees/sprint/r-2-engine-shape (branch sprint/r-2-engine-shape).
Reviewer JSON: .cursor/phase-a-orchestration/wave8-qa/r-2-qa1/*.json. Ceremony screen: r-2-qa1/ceremony-screen.json.
Gate: QA-1 FAIL, with 2 Blocking, 9 Important and 15 Minor findings. QA-2 is locked to these ids.

## 0. First: merge origin/integrate/phase-a (d88879a; it now contains r-3/PR 26)
Use a normal merge commit. Resolve conflicts keeping both sides; the likely files are cargo-tree-gates.sh, sc-agent-ci.yml, clippy.toml, plan.md and architecture.md. Then re-run the full validation list.

## Blocking (sprint doc)
1. REQ-QA-R2-001: add the missing requirement ids to the sprint frontmatter `requirements`, with upheld evidence or a one-line not-applicable reason for each. The ids are REQ-SAG-0311, 0309, 0303, 0611, 0702, 0703, 0711, 0801, 0802 and NFR-SAG-0004.
2. ARCH-R2-001: add ADR-SAG-0016 to the `adrs:` list, plus a one-line acceptance note: libraries take only SharedSessionLog, and sc-jev-check has no sc-observability edge.

## Important
3. REQ-QA-R2-006 + RBQA-F002 (+ closes RBQA-F003, so add no new snapshot gate): narrow the pub surface to the D2.9 facade.
   - Make config/equal_input/extract/payload/session internals `pub(crate)`. That includes build_payload, extract_input, load_check_str, parse_question_set, producer_from_extractions, StoredSession, SESSION_CONTEXT_HEADER and STALE_TMP_AGE.
   - Move the tests that need them in-crate, or put them behind test-util.
   - Anything that must stay public goes in D2.9 and the deviation table, with its ADR.
   - Update the boundary manifest facade string to match.
4. RBQA-F001: FileSessionStore::ask is a second public send path. Route ask through the facade, so that run is the only send path, or make the store's ask `pub(crate)` with a facade fn. Record the decision against ADR-SAG-0024.
5. RBP-F004: carry CheckSource::CheckFile as RepoRelativePath on the facade, with a single normalisation constructor. Delete the three hand-written normalisers.
6. FTQ-001/FTQ-002: one shared test-support GIT_* scrub helper. Reuse sc-agent-types' list (the r-3c form, which includes GIT_CONFIG_SYSTEM, GIT_DEFAULT_HASH and init --object-format=sha1). Apply it in sc-jev-check tests/run_response.rs and the sc-ai tests/cli.rs fixtures. No copy-paste lists.
7. REQ-QA-R2-004: restore the dropped CR-014 behaviour. The ask envelope must report a session-log flush failure as a warning: give ask a warnings slot, e.g. Envelope::ask(..).with_warning(..). Also restore prompt_sha256 on ask data, or record its removal in the ADR-SAG-0014 r-2 amendment.
8. REQ-QA-R2-002 (ceremony-screened, so no new artifact): cite the PR 27 CI job names and run id at the fix-round head in the Implementation notes.

## Minor (fix unless the change is disproportionate; say which)
- REQ-QA-R2-005 / RSH note: discover_repo_root falls back only on git NotFound or "not a git repository"; other errors become typed errors. Add a test and narrow the ADR-SAG-0003 r-2 amendment text.
- REQ-QA-R2-007: an optional `name` on the command input, falling back to the argv[0] stem. Alternatively document the interpreter limitation in the REQ-SAG-0313 row.
- REQ-QA-R2-008: reword the REQ-SAG-0710 amendment to "session_id (null unless session_saved)".
- REQ-QA-R2-009 / QA-002 / QA-003: no panics on library paths. Make the Envelope construction fallible, or use a raw-value path. Map the .expect calls on the child pipes to extraction_failed. Make default_backends infallible.
- QA-001: either degrade a session save failure to session_saved=false plus a warning, matching the pinned sample, or document that the sample is builder-only and fail-fast in r-2. Pick one and state it.
- QA-004: bound the AlreadyExists retry. A parent fsync failure after rename is best-effort; document it.
- FTQ-003: make the outside-git assertion unconditional, using a tempdir under a known non-git root or GIT_CEILING_DIRECTORIES.
- FTQ-004: use the existing deadline helper for the re-exec'd child.
- FTQ-005: scrub SC_JEV_CHECK_BLESS from the test env unless it is deliberately set.
- RBQA-F004: move RunResponseBuilder to the test-util facade group.
- ARCH-R2-002: optional; leave it.
- REQ-QA-R2-003: no action (ceremony-screened; already disclosed).
- QA-005: informational.

## Report
Fenced JSON with status, pushed head SHA, and a per-id table (id → commit or "won't fix + reason"). Include the validation log path and the PR 27 CI result at the new head. Use per-agent scratch dirs only. Use the commit trailer "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>".
