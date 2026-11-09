# Wave-2 layer QA-1 findings → fix on stack top (PR #11)

Base: `origin/sprint/wave-2-stack-qa-top` @ f404a43 (rebase your branch onto latest qa-top before push).

## a-11 (PR #8, layer commit 675fd16)

- REQ-QA-001/002: Add REQ-SAG-0104, NFR-SAG-0001 to sprint frontmatter — **doc fix landed 3f50468**; verify complete.
- REQ-QA-003: owned_paths `boundaries/sc-treesitter/**` — **landed 3f50468**; verify.
- REQ-QA-004 / ARCH-001: CLI stderr must derive ids from JevError only — **emit fix 3f50468**; verify tests.
- Run `cargo test -p sc-treesitter`, `just lint`.

## a-3 (PR #10, e997cf4)

- REQ-QA-001/002: `just lint` failed on layer due to agent-types — **fixed on qa-top union**; verify at stack head.
- REQ-QA-003: owned_paths `boundaries/sc-jev-check/**` — **landed 3f50468**.
- ARCH-001: ADR-SAG-0001 in sprint adrs — **landed 3f50468**.
- Close any remaining sprint `status: complete` vs validation mismatch in sprint doc if still wrong.
- Scope: `crates/sc-jev-check/**`, `boundaries/sc-jev-check/**`, `docs/sc-jev-check/**`, sprint-a-3 doc.

## a-4 (PR #9, b55f74a)

- REQ-QA-001 lint — qa-top; verify.
- REQ-QA-002 owned_paths — **landed 3f50468**.
- ARCH plan ADRs — **landed 3f50468**.
- ARCH-008 ADR-SAG-0005 check.rs — **f404a43**; verify tests.
- ARCH-009 ADR-SAG-0020 JevClient keys — **f404a43** (ResolvedApiKey in sc-agent-types); verify all call sites/tests.
- Scope: `crates/sc-jev/**`, `boundaries/sc-jev/**`, sprint-a-4 doc.

## a-5 (PR #7, d73ee28)

- REQ agent-types breakage — qa-top; verify.
- REQ-QA-003 owned_paths — **landed 3f50468**.
- ARCH adrs list — **landed 3f50468**.
- ARCH-002 validate_for_send token preflight — **f404a43**; verify `tests/llm.rs`.
- ARCH-003 secret handoff — **f404a43**; verify constructors use ResolvedApiKey.
- Scope: `crates/sc-jev-backends/**`, `boundaries/sc-jev-backends/**`, sprint-a-5 doc.

## Cross-layer (only if touched by your layer)

- `crates/sc-agent-types/src/secret.rs`, boundaries manifests, `docs/architecture.md` — coordinate via minimal diffs; prefer single owner (a-4 agent for sc-jev + types secret API if needed).

## Done criteria per agent

- All listed findings for your layer closed or explicitly documented as N/A at stack head.
- `just lint` passes in worktree.
- Layer-specific `cargo test` commands from sprint doc pass.
- Commit on your fix branch; push origin.
