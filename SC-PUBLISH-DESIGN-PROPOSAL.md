# Proposed design: stable sc-publish through extension points

Status: proposal for owner review, 2026-11-08. Nothing here is decided or scheduled.

## Summary

Consumers of sc-publish open release-time PRs because the kit has bugs that only appear during a real release, and because it hard-codes policies that belong to each repository. Both push repositories to patch their vendored copy or ask for repo-specific kit changes.

The proposal has four parts:

1. Sort every request into a kit bug, a hard-coded policy, or a repo concern.
2. Replace hard-coded policies with one extension mechanism: hook points at fixed release stages, each calling a workflow the repository owns.
3. Keep consistency only where it matters: artifact layout, checksums, channel submission and credentials.
4. Gate every kit PR on scope and on measured impact to all consumers.

The extension design below is a direction to test. It has not been checked against every issue and local patch.

## The problem

The goal was consistency enforced through the kit. In practice, each repository pushes its own concerns onto the kit, or patches around it. CONTRIBUTING.md forbids both patching (rule 6) and repo-specific logic (rule 2), but offers few alternatives.

### Why repositories open PRs at publish time

From a review of release history in sc-compose, atm-core and sc-observability. Develop-to-main promotions, version bumps and announcement PRs were excluded as normal release flow.

| Cause | PRs | Examples | Smallest fix | Effect on other repos |
|---|---|---|---|---|
| A. Kit bugs found mid-release, fixed in the consumer | About 40, the largest group | sc-compose #544-#550, #466, #467; atm-core #1589, #1590; sc-observability #1199 (immutable-release 403) | Fix each in the kit; issues #145-#153 are open | Each fix reaches every repo at its next kit update |
| B. Release gate requires `main` to contain the candidate (`release_gate.sh:44`, `:48`) | About 7 | sc-compose #463, #472, #495, #504, #538, #543; atm-core #1071 | Make the branch model configurable, default unchanged (#147) | None unless a repo opts in |
| C. Pin and parity bookkeeping | A few | sc-compose #973, #975, #992, #1030 | Partly the consumer's own test; loosen it, or have the installer write the pin | sc-compose only, if done there |
| D. Kit pins the sc-compose version | Recurring | atm-core #2404, forced by sc-compose releases | Consumer-supplied version (#127) | Needs a config change unless the default stays |

The sc-compose 1.8.0 count includes consumer CI work touching kit paths, so the total is approximate. atm-core PRs after about #2404 were not reviewed one by one, and I did not check which of the cause A issues are already fixed in the latest kit.

### Hard-coded policy in the kit

| Hard-coded | Issue |
|---|---|
| `develop`/`main`/release-candidate branching | #147 |
| sc-lint fields and lint actions | #152 |
| Webview and Wayland apt packages on every Rust build | #148 |
| Cargo or pyproject as the only version source | #149 |
| A `crates-io` environment, even with no crates | #145 |
| `run_by_agent` must be `publisher` | #151 |
| Non-empty `release_targets`, even with no binaries | #153 |

## Proposed design

### 1. Sort every request

| Kind | Test | Action |
|---|---|---|
| Kit bug | Fails for a valid configuration, or differs from its docs | Fix in the kit, with no new option |
| Hard-coded policy | One repo's choice imposed on all | Move behind an extension point |
| Repo concern | Only one repo needs it | Reject; the repo handles it |

### 2. One extension mechanism

Adding a setting per request is how the kit became complicated. Instead, define hook points at fixed release stages, such as before build, after packaging and before each channel submission. Each hook calls a workflow the repository owns. A repository that declares no hook behaves as it does today. The preflight workflow calls merged in sc-publish #169 are a first example of this shape.

Mapping to test: #147 (branch model) and #148 (build setup) look like the first candidates. #147 should probably be built as an extension point rather than a one-off setting.

### 3. Where consistency stays

Kit-owned: artifact layout, checksums, channel submission and credentials.
Repo-owned: branching, lint, build setup and version source.

### 4. Getting consumers off local patches

Each repository should update once, onto a workflow that works for all of them. A kit update overwrites a vendored workflow, so a local fix is lost unless the kit has it. On default branches today, wyvern's `develop` and sc-lint have locally changed WinGet workflows; the others surveyed are byte-identical to the kit (see the survey below).

### 5. QA gate on every kit PR

Now in force as an owner rule (bd memory `owner-rule-2026-11-08-sc-publish-pr-qa`). A QA agent rejects anything outside the requested scope, then renders and validates each consumer's real input against `develop`. It reports a verdict and does not post, approve or merge.

It stops new breakage, but it cannot catch bugs that only appear in a real release. It rejected #182 on its first pass for an extra gate and an unrequested branding change.

Consumers today (12): atm-core, sc-runtime, wyvern, sc-compose, beads-cache, docli, sc-agent, sc-lint, sc-observability, agent-team-mail, schook, atm-bd-orchestration.

## Case study: package-channel parity

Requirement: if a package ships several binaries, all of them reach PATH on every channel.

| Channel | Setting | Result |
|---|---|---|
| Homebrew | `formulas[].binaries` list | Works |
| Scoop | `binary` string only, until #176 | Fixed by `binaries` list, merged in #176 |
| WinGet | None; the list lives in the first hand-submitted manifest | Works only if that manifest lists every binary |

### WinGet evidence

| Test | Result |
|---|---|
| Komac 2.17.2, `Gyan.FFmpeg.Essentials`, 8.1.1 zip | Aliases kept; `RelativeFilePath` stale at `ffmpeg-9.0.1-essentials_build/...` |
| Komac 2.16.0, same input | Path corrected to `ffmpeg-8.1.1-essentials_build/...` |
| Komac 2.16.0, wyvern 0.8.0 zip | `wyvern_0.8.0_x86_64-pc-windows-msvc/bin/wyvern.exe`, matches the zip |
| Komac 2.16.0, atm 1.6.6 zip | `atm_1.6.6_.../bin/atm.exe` correct, but `atm-daemon.exe` in the zip has no alias |
| winget-pkgs#458446 (`randlee.wyvern` 0.8.0, Komac 2.17.1) | `Validation-Installation-Error`; a comment attributes it to the reused 0.7.0 path. The label stayed after a hand fix, so there may be a second cause |

Kit archives include the version in their top folder, and the pinned action installs Komac with an unpinned `cargo binstall komac -y`. So any WinGet consumer is exposed once Komac reaches 2.17.

All Komac runs used `--dry-run` or `--exit`; nothing was submitted.

### Survey of upstream submissions

22 winget-pkgs PRs reviewed across the kit's packages. Only four were rejected for a manifest or path cause; most others merged first time. Azure DevOps validation logs were not accessible, and zip contents were not inspected.

| Cause | Count | Repos | Would #182 prevent it? |
|---|---|---|---|
| Komac 2.17 carries the previous nested path forward | 1 | wyvern 0.8.0 | Yes. Every Komac 2.16.0 manifest has the correct versioned path |
| Hand-written manifest with wrong path or no alias | 2 | wyvern 0.5.0, 0.6.0 | No; the first submission is hand-made. The new docs bullet addresses it |
| Hash typo | 1 | wyvern 0.5.0 | Yes; Komac computes the hash |
| Defender or validator infrastructure | About 4 | atm 1.0.0, atm 1.2.3, claude-history, wyvern 0.5.0 | No; outside the workflow |
| DLL not found (C0000135) | 2 | wyvern 0.5.0, sc-lint 0.8.3 | No. sc-lint fixed it with `+crt-static`; sc-compose carries a VCRedist dependency |
| Label stuck after a correct hand fix | 2 | wyvern 0.6.0, 0.8.0 | No; needs a moderator or a bot re-run |

The remaining error on wyvern 0.8.0 (winget-pkgs#458446) is unexplained. wyvern 0.6.0 followed the same pattern and cleared on manual validation, but that does not establish the cause.

| Package | Binaries in repo | Aliases on WinGet |
|---|---|---|
| atm | atm, atm-daemon | atm only |
| wyvern | wyvern, wyvern-viewer | wyvern only |
| docli | docli, cargo-docli | docli only (open PR) |
| sc-lint | 4 | All 4 |
| sc-compose | 1 | OK |
| claude-history | 1 | OK |

| Consumer workflow | State |
|---|---|
| atm-core, sc-compose, docli, beads-cache, sc-runtime, sc-observability, sc-agent | Byte-identical to kit `develop` on the default branch |
| wyvern | `main` matches the kit. `develop` (wyvern PR #179) uses unpinned Komac, checks the zip layout, overwrites `NestedInstallerFiles` with one entry, then submits. It needs config keys the kit lacks |
| sc-lint | Adds an arm64 installer pattern through `installer_arm64_target`, which the kit lacks |
| claude-history | Not on the kit; its own `release.yml` uses `winget-releaser@v2` |

What one workflow still needs beyond #182:
- Installer URLs for every target. #182 passes only `asset_patterns[0]`, so sc-lint would lose arm64 at its next kit update; that loss comes from any kit re-install overwriting its local change.
- A check before submission that each `RelativeFilePath` exists in the zip (wyvern's local fix).
- A way to set aliases after the first version, or acceptance that this stays a manual winget-pkgs edit.

Windows binaries also need a CRT decision (`+crt-static` or a VCRedist dependency), or validation can fail with C0000135. This is outside the kit workflow.

## Status of related PRs (randlee/sc-publish)

| PR | Change | State | QA |
|---|---|---|---|
| #176 | Scoop `binaries` list | Merged | Pass (ran after merge finished) |
| #178 | Homebrew `test_binary` default (#177) | Open | Pass at 08642b8 |
| #179 | WinGet `fork_user` | Closed; not needed | None |
| #182 | Pin Komac 2.16.0 (#181) | Open at 262f656 | Rejected at 7d2c4ce; fixes pushed; re-run needed |

#182 has never run on a real runner. It pins an older Komac rather than fixing the upstream behaviour, so it would need revisiting if winget-pkgs stops accepting 2.16.0 manifests. Docs in atm-core, wyvern, sc-compose and sc-lint that mention the old action would go stale.

## What was not done

- The extension design has not been tested against all issues and local patches.
- `install.py --dry-run` could not run across consumers in the churn review because it needs sc-compose bindings. Whether a kit update's copied files themselves drive PRs is unverified.
- Issues and PRs are cited from agent reviews; I checked several directly but not all.

## Decisions needed

1. Whether to adopt the sorting rule and one-mechanism approach.
2. Which hook points to define first. Suggested: #147.
3. Whether to merge #178 and #182, once #182 passes QA again, ahead of that work.
