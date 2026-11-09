# Phase A — status snapshot (context restore)

**Generated:** 2026-11-03 (local). **Do not commit** this file — handoff / resume notes only.

## Branches (remote synced)

| Branch | SHA (full) | Notes |
| --- | --- | --- |
| `develop` | `2fb25fdc104da7ec7cf8805aa696da5c1e8ba667` | Main clone checked out here; **pulled from origin** (already up to date). |
| `integrate/phase-a` | `0417aef60b6eba1fb4bf95fdfe8af0a1fa7d146b` | Phase A integration line; **37 commits ahead of `develop`**. Not merged back to `develop` yet. |
| `plan/phase-a` | (on remote) | Planning branch; plan merged via PR #4 → `develop` historically. |

**Repo root (develop):** `/Volumes/Extreme Pro/github/sc-agent`  
**Integrate worktree:** `/Volumes/Extreme Pro/github/sc-agent-worktrees/integrate/phase-a`  
**Sprint worktrees base:** `/Volumes/Extreme Pro/github/sc-agent-worktrees/sprint/`

## Phase intent (one line)

Deliver **sc-jev** check/judge stack, **sc-ai** CLI, **sc-jev-py** PyO3 wrapper, shared types/secrets, **sc-treesitter** producer — per `docs/plans/phase-a/plan.md` and `intent.md`.

**Authoritative plan:** `docs/plans/phase-a/plan.md`  
**Sprint count:** 10 (a-1 … a-9, a-11). **Critical path:** a-1 → a-3 → a-6 → a-7 → **a-9** (a-8 joins at a-9).

## Sprint completion matrix

| Sprint | Wave | Status on `integrate/phase-a` | PR / merge | QA (cursor-quality-mgr) |
| --- | --- | --- | --- | --- |
| **a-1** | 1 | Merged | [#5](https://github.com/randlee/sc-agent/pull/5) → `b5b5ec1` | Early merge (pre-orchestration JSON) |
| **a-2** | 2 | Merged | [#6](https://github.com/randlee/sc-agent/pull/6) → `1d7f055` | QA-2 **PASS** — [comment](https://github.com/randlee/sc-agent/pull/6#issuecomment-6590501956) |
| **a-3** | 2 | On integrate (via stack → a-6) | Stack [#10](https://github.com/randlee/sc-agent/pull/10) | Wave-2 stack QA-2 **PASS** — [comment](https://github.com/randlee/sc-agent/pull/11#issuecomment-6593632548) |
| **a-4** | 2 | On integrate (via stack) | Stack [#9](https://github.com/randlee/sc-agent/pull/9) | Same stack gate |
| **a-5** | 2 | On integrate (via stack) | Stack [#7](https://github.com/randlee/sc-agent/pull/7) | Same stack gate |
| **a-11** | 2 | Merged | [#8](https://github.com/randlee/sc-agent/pull/8) → `8c28f9d` | Included in wave-2 stack |
| **a-6** | 3 | Merged | [#13](https://github.com/randlee/sc-agent/pull/13) → `53644e2` | QA-2 **PASS** — [comment](https://github.com/randlee/sc-agent/pull/13#issuecomment-6595086538) |
| **a-7** | 4 | Merged | [#14](https://github.com/randlee/sc-agent/pull/14) → `4a5a204` (tip commit `844fc65`) | QA-2 **PASS** — [comment](https://github.com/randlee/sc-agent/pull/14#issuecomment-6598893463) |
| **a-8** | 4 | Merged | [#15](https://github.com/randlee/sc-agent/pull/15) → `0417aef` (sprint tip `bfd36cb`, merge fix `a924e78`) | QA-2 **PASS** at `bfd36cb` — [comment](https://github.com/randlee/sc-agent/pull/15#issuecomment-6598894697); post-merge conflict resolution **not** re-run through full QA-2 at `0417aef` |
| **a-9** | 5 | **Not started** | No PR | — |

**Wave-2 stack rollup PR:** [#11](https://github.com/randlee/sc-agent/pull/11) (`sprint/wave-2-stack-qa-top`); stack PRs `[8, 10, 9, 7, 11]`; head `99cd224` at QA pass (orchestration JSON).

**Open PRs targeting `integrate/phase-a`:** none.

## What is on integrate (crates)

Present at `0417aef`: `sc-agent-types`, `sc-agent-secrets`, `sc-jev`, `sc-jev-backends`, `sc-jev-check`, `sc-treesitter`, `sc-ai`, `sc-jev-py`.

**Not on integrate yet:** sc-publish release kit, release workflows, REQ-SAG-0803 wheel CI (owned by **a-9**).

## Next sprint: a-9

**Doc:** `docs/plans/phase-a/sprint-a-9-sc-publish-install.md`  
**Branch (planned):** `sprint/a-9-sc-publish-install`  
**Must follow:** a-7, a-8 (both merged).

**Outcome:** Pin [sc-publish](https://github.com/randlee/sc-publish), author `release/sc-publish-consumer-input.json`, run install dry-run, green **release-preflight** workflow, document and exercise `cargo publish --dry-run` + `maturin build` (REQ-SAG-0130–0132, REQ-SAG-0803, ADR-SAG-0017/0018).

**Does not close:** Actual tagged production release (owner-triggered after plan approval).

## Orchestration / QA mechanics (Cursor)

- **Enforcer:** `.cursor/agents/cursor-quality-mgr.md` — parent must spawn reviewers and pass **fenced JSON** in `<parent-reviewer-handoff>` (prose summaries fail spawn gate QMG-SPAWN-001).
- **Wave-4 QA artifacts (local, may be uncommitted):** `.cursor/phase-a-orchestration/wave4-qa/a-7-qa2/`, `a-8-qa2/` (`qa-assignment.xml`, reviewer JSON).
- **Stale file:** `.cursor/phase-a-orchestration.json` — integrate head still `8c28f9d`; update when resuming orchestration (a-7/a-8 not recorded).
- **Model policy (orchestration):** Grok 4.7 high/fast for rust-developer; composer-2.5-fast for cursor-quality-mgr; see `.cursor/orchestration-agent-models.yaml`.

**CI:** Sprint PRs had **no GitHub Actions checks** on branch; gates were local (`just lint`, `cargo test`, reviewers) + PR comments from quality-mgr.

## Worktrees (existing)

Under `sc-agent-worktrees/sprint/` (names → typical branch):

- `a-1-workspace-and-types`, `a-2-agent-secrets`, `a-3-check-build-and-payload`, `a-4-backend-jev`, `a-5-backend-llm`
- `a-6-sessions-and-orchestration`, `a-7-sc-ai-cli`, `a-8-pyo3-wrapper`
- `a-11-sc-treesitter`, wave-2 layer-fix dirs, `wave-2-stack-qa-top` (if present)

**Create a-9 worktree (when starting):** use `/sc-git-worktree` skill or:

```bash
git fetch origin integrate/phase-a
git worktree add -b sprint/a-9-sc-publish-install \
  "/Volumes/Extreme Pro/github/sc-agent-worktrees/sprint/a-9-sc-publish-install" \
  origin/integrate/phase-a
```

## Resume checklist (a-9)

1. `git fetch origin` on main clone; confirm `integrate/phase-a` at `0417aef`.
2. Create worktree from `origin/integrate/phase-a` → branch `sprint/a-9-sc-publish-install`.
3. Implement per sprint doc; open PR **base `integrate/phase-a`**.
4. **QA-1** (six reviewers incl. RBP, RSH, flaky where applicable) → Grok/fix loop → **QA-2** (req, arch, rust-qa, flaky if tests touched) → **cursor-quality-mgr** with full fenced JSON handoff.
5. Merge when PASS, 0B+0I+0M, `just lint` green.
6. After a-9: phase A code complete on integrate; **merge integrate → develop** is a separate product decision.

## Local validation (integrate smoke)

Last verified on integrate worktree:

```bash
cd "/Volumes/Extreme Pro/github/sc-agent-worktrees/integrate/phase-a"
just lint
cargo test -p sc-ai -p sc-jev-check cross_impl_branches
cargo tree -p sc-ai --depth 1   # should not list reqwest/hyper/ureq at depth 1
```

**Known harness footgun:** zsh treats `status` as read-only; use `exit_code=$?` in shell validation scripts (failed background job 776906 was this, not product failure).

## Beads / issues

Project uses **bd (beads)**; Dolt store was **unavailable** in some agent sessions (`bd doctor`). Use `bd ready` / `bd create` when store is up; do not rely on markdown TODOs.

## Git policy (agents)

Conservative default: **no commit/push** unless user asks. This status file is **intentionally untracked**.

## Key doc links

| Doc | Path |
| --- | --- |
| Phase plan | `docs/plans/phase-a/plan.md` |
| a-9 sprint | `docs/plans/phase-a/sprint-a-9-sc-publish-install.md` |
| Architecture | `docs/architecture.md` |
| Requirements | `docs/requirements.md` |

---

*To refresh SHAs after new merges: `git fetch origin && git rev-parse develop origin/integrate/phase-a`.*
