# Stack review at e4ede9f (#39, #40, #42, #43): findings so far

CI is 16 of 16 at this commit. Rust quality and flaky tests are still running and post separately.

| Review | Result |
|---|---|
| Architecture | **FAIL**: 1 Blocking, 2 Important |
| Requirements | **FAIL**: 3 Blocking, 2 Minor |

Code and tests match the amended REQ-SAG-0505 and ADR-SAG-0007; ARCH-001 (`+crt-static`) is closed. All findings are doc drift.

| ID | Severity | Finding | Location |
|---|---|---|---|
| ARCH-101, REQ-QA-PE-004 | Blocking | Still says 4 sends | `docs/sc-agent-http/architecture.md:77` |
| REQ-QA-PE-003 | Blocking | Row says 4 sends, omits the 5-send bound and transport bounds, and cites "nine named tests" with no names | `docs/sc-agent-http/requirements.md:11` |
| ARCH-102, REQ-QA-PE-005 | Blocking | Still says 1, 2, 4 s | `docs/sc-jev-backends/requirements.md:22-24` |
| ARCH-103 | Important | First-release dry-run checklist omits `sc-decision-schema` | `docs/development/sc-publish.md:281` |
| REQ-QA-PE-006 | Minor | Garbled retry comment; stale "after 4 sends" example | `crates/sc-agent-http/tests/retry.rs:128`, `crates/sc-agent-types/tests/errors.rs:187` |
| REQ-QA-PE-007 | Minor | Scoop check `eval`s an expression from the pinned kit workflow; relies on the pin staying trusted | `scripts/check-scoop-manifest.py:55` |

Fixes land as a new layer above #43.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
