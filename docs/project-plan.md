# sc-agent project plan

**Status:** Phase A and Phase A.1 (`r`) implementation have landed on `integrate/phase-a`; the phase is in phase-end review and is not closed — see `PHASE-A-REVIEW.md` items  
**Default branch:** `develop`

## Phases

| Phase | Status | Integration branch | Plan |
| --- | --- | --- | --- |
| Bootstrap | Complete | `develop` | Agent teams, plan-anchor, beads, policy |
| **A — sc-jev + sc-ai** | **Implementation landed (a-1 to a-10); in phase-end review, not closed** | `integrate/phase-a` | [`docs/plans/phase-a/plan.md`](plans/phase-a/plan.md) |
| **r (A.1) — architectural refinement** | **Implementation landed (r-1 to r-7); in phase-end review, not closed** | `integrate/phase-a` | [`docs/plans/phase-a-refine/plan.md`](plans/phase-a-refine/plan.md) |
| B (draft) | Not planned | — | Remaining Jev API (REQ-SAG-0110); check features 4–7; **API-direct** Claude + Codex via `sc-ai`. **Not** claude-code/codex-cli/cursor-cli wrappers |

## Lead

| Phase | Lead (ATM identity) |
| --- | --- |
| Bootstrap / A | `ai-lead` |

## Phase A sprints (dependency order)

Wave table in [`docs/plans/phase-a/plan.md`](plans/phase-a/plan.md) is authoritative.

1. **a-1** — workspace, errors, cross-impl fixture  
2. **Wave 2 (parallel):** a-2, a-3, a-11, a-4, a-5 — secrets, check build, sc-treesitter, Jev HTTP, LLM backends  
3. **a-6** — sessions + orchestration (wave 3)  
4. **Wave 4 (parallel):** a-7, a-8 — CLI ∥ sc-jev-py wheel  
5. **Wave 5:** a-9 sc-publish kit install + preflight + publish dry-run  

**First release:** crates.io, PyPI, Homebrew, Scoop, WinGet via [sc-publish](https://github.com/randlee/sc-publish).

Sprint docs: `docs/plans/phase-a/sprint-a-*.md`.

## Phase A.1 (`r`) sprints (dependency order)

Wave table in [`docs/plans/phase-a-refine/plan.md`](plans/phase-a-refine/plan.md#wave-table) is authoritative.

1. **Wave 1 (parallel):** r-1 sc-agent-types break ∥ r-4 pins, allocator, released binaries
2. **Wave 2 (parallel):** r-2 engine shape, r-3 `sc-agent-http`, r-3c sc-treesitter clap + git helper
3. **Wave 3:** r-2b multi-backend semantics
4. **Wave 4:** r-3b composition crate (`sc-jev-host`), workspace tests, consumers
5. **Wave 5:** r-7 `sc-decision-schema` crate and model catalog
6. **Wave 6 (parallel):** r-5 Jev wire body (`jev::v1`) ∥ r-6 session log wiring

Sprint docs: `docs/plans/phase-a-refine/sprint-r-*.md`.

## Approval gate

Phase A plan approved (PR #4 merged 2026-11-02). Phase A (a-1 to a-10) and
Phase A.1 (r-1 to r-7) have landed on `integrate/phase-a`. The phase is in
phase-end review and is not closed; open items are listed in
[`plans/phase-a/plan.md`](plans/phase-a/plan.md#phase-close-notes--open-items).

## After Phase A

- Migrate orchestration / plan-anchor call sites to the new client (separate repos).
- API-direct Claude and Codex agent crates (Phase B).
- Subprocess wrappers for claude-code, codex-cli, cursor-cli (never).
- Optional: `decider-4b` local backend crate.
