# r-4 QA-1 fix round (PR 21, reviewed at 59ec838)

Verdicts: req-qa FAIL (2B/3m), arch-qa PASS (1 minor), rust-qa 1 Important + 2 minor (all gates green, coverage 90.95), RBP 1 Important (same root cause as pins), RSH 1 Important + 1 minor, flaky 1 Important + 2 minor. CI: `dependency gates` job red on the expected sc-observability-types pin.

Reviewer JSON: .cursor/phase-a-orchestration/wave7-qa/r-4-qa1/*.json

## Fix now (same worktree/branch, commit per item, push)
1. Pins before r-1 lands (REQ-QA-R4-001, RSH-001, RBP-F001, QA-002): make the branch self-consistent and CI green now, with the correct end state after r-1: add `sc-observability-types` to the ADR-SAG-0023 lockstep list between the markers with an inline note "(transitional; r-1 deletes the dependency and this entry is removed when r-4 rebases onto r-1)". After r-1 merges you will rebase and remove the entry in the same commit that re-runs the gates. Record the transitional entry in the sprint doc and PR body.
2. QA-001 (rust-qa, Important): restore the `sc-observability-types` global_dependency_ownership record in sc-lint.toml (allowed only in crates/sc-agent-types) until r-1 deletes the dependency; r-1 has been told to delete the record when it deletes the dependency. Note in sprint doc.
3. FTQ-001 (Important): .github/scripts/cargo-tree-gates.sh — capture only stdout (`2>"$errfile"`, print stderr only on cargo failure); add `--locked` to the cargo tree calls and to the two `cargo build --no-default-features` CI steps.
4. FTQ-003: check-exact-pins.sh --self-test expect helper asserts exit code 1 AND a distinguishing substring ("NOT LISTED" / "exactly once each"), not any non-zero.
5. FTQ-002: add CI steps `cargo test -p sc-treesitter --test cli --features cli` and `cargo test -p sc-ai --test cli --features cli` so a renamed/dropped `cli` default cannot silently skip the CLI tests; one sentence in docs/development/ci.md that the skip under non-cli cells is deliberate (ci.md is not in owned_paths: if the sprint doc forbids, put the sentence in the sprint doc + PR body and report).
6. REQ-QA-R4-003: add REQ-SAG-0135 to the sprint requirements list as "constrained by". REQ-QA-R4-004: add Cargo.lock to owned_paths as a derived file. RBP minor: `tree-sitter = "=0.27.1"` string form or hoist to workspace (your call; record). QA-003 / RBP minor: check-exact-pins.sh — python version guard (>= 3.11 message) and document ubuntu-only. RSH-002: record in ADR-SAG-0023 or sprint doc that cargo audit/deny is tracked as bead agt-ci-advisory-scan (declined in plan). RSH note: one cargo-tree-gates row asserting mimalloc IS present in each bin crate's default tree (`both`/default mode) — cheap, add it.
7. ARCH-001 (minor, outside owned sections): leave; report as deferred to the next docs pass (ADR-SAG-0029 "Deferred" note now stale).
8. REQ-QA-R4-002 (upstream sc-publish issue URL): owner action; the agent is permission-blocked on `gh issue create`. Keep the placeholder; sprint doc frontmatter `status: in-review` (not complete) until the URL is recorded or the owner waives the row. Report it.

Then: full gate set, push, report SHA. Do not rebase onto r-1 yet (it has not merged).
