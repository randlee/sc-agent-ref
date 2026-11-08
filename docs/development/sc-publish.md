# sc-publish release kit

sc-agent publishes through the shared
**[sc-publish](https://github.com/randlee/sc-publish)** kit (ADR-SAG-0017,
ADR-SAG-0018). The kit owns the release workflows, scripts, agents and rendered
manifests. This repository owns two input files and one PR dry-run gate.

Kit reference after install: [`README.sc-publish.md`](../../README.sc-publish.md),
[`docs/getting-started.md`](../getting-started.md),
[`docs/consumer-preflight-and-docli.md`](../consumer-preflight-and-docli.md).

## Files

| File | Owner | Role |
| --- | --- | --- |
| `release/sc-publish-pin.toml` | sc-agent | Exact 40-character kit SHA |
| `release/sc-publish-consumer-input.json` | sc-agent | The only release manifest you edit |
| `release/publish-artifacts.toml` | kit (rendered) | Crates, binaries, targets, Python dists, channels |
| `release/publish-channel-contracts.toml` | kit (vendored) | Channel identity, secret names, registry endpoints |
| `release/publish-preflight.toml` | kit (rendered) | Consumer preflight checks (none declared) |
| `.github/workflows/release*.yml`, `*-publish.yml`, `.github/scripts/`, `.github/actions/` | kit | Release pipeline |
| `.claude/agents/*-publisher.md`, `.cursor/agents/publisher.md`, `.claude/skills/publishing/` | kit | Publisher agents |
| `README.sc-publish.md`, `docs/getting-started.md`, `docs/consumer-preflight-and-docli.md`, `docs/npm-channel-compatibility.md`, `install.py`, `release/**/*.j2` | kit | Kit docs and templates |
| `scripts/sc-publish-dry-run.sh` | sc-agent | Local and PR release dry-run |
| `.github/workflows/sc-publish-dry-run.yml` | sc-agent | Runs the dry-run on every PR |

**Never hand-edit a kit or rendered file** (ADR-SAG-0018, REQ-SAG-0131). To
change the release surface, edit `release/sc-publish-consumer-input.json` or the
pin and re-run install. `install.py` overwrites kit paths without warning.

## What the consumer input declares

- **crates.io**, in publish order: `sc-agent-types` (1), `sc-decision-schema`
  (2), `sc-agent-secrets` (3), `sc-agent-http` (4), `sc-jev` (5),
  `sc-jev-backends` (6), `sc-jev-check` (7), `sc-jev-host` (8, internal
  support, `=`-pinned by `sc-ai`), `sc-treesitter` (9), `sc-ai` (10). Crates 1
  to 8 have dependents, so each waits 30 s and sets
  `verify_registry_visibility` before the next upload. crates.io is enabled by
  this `crates` list (entries with `publish: true`), not by a `channels` key.
- **PyPI**: `sc-jev-py`, a maturin wheel plus sdist on Linux, macOS and Windows.
  `sc-jev-py` is a workspace member with `publish = false`. It is not in the
  `crates` list, so it never reaches crates.io.
- **Release binaries** `sc-ai` and `sc-treesitter` (ADR-SAG-0023; per-channel
  coverage in ADR-SAG-0017's r-4 amendment). Each target gets one archive
  with `bin/sc-ai` and `bin/sc-treesitter`. Targets: `x86_64-unknown-linux-gnu`,
  `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`
  (`tar.gz`), `x86_64-pc-windows-msvc` (`zip`).
- **Homebrew**: formula `Formula/sc-ai.rb` (class `ScAi`) in
  `randlee/homebrew-tap`. It installs both binaries and tests `sc-ai --version`.
- **Scoop**: `bucket/sc-ai.json` in `randlee/scoop-bucket`, shimming both
  `bin/sc-ai.exe` and `bin/sc-treesitter.exe` (`[channels.scoop].binaries` in
  the rendered `release/publish-artifacts.toml`, available since the kit
  re-pin to sc-publish `34dab3f`; upstream item 1 below).
- **WinGet**: identifier `randlee.sc-ai`. The kit pins Komac 2.16.0 for
  updates. Komac only updates an existing package, so the hand-submitted first
  manifest must list both binaries, each with a portable command alias
  (`sc-ai`, `sc-treesitter`), or `sc-treesitter` never reaches `PATH` (bead
  `agt-sft`, [First-release checklist](#first-release-checklist-req-sag-0803));
  this is not an upstream kit issue.

### Upstream kit items

These are sc-publish kit limitations. Fix them in the kit and bump the pin;
do not hand-edit kit or rendered files (ADR-SAG-0018). Item 1 was raised as a
PR instead of an issue (owner decision) and has merged:
https://github.com/randlee/sc-publish/pull/176. Item 2 is not filed.
sc-lint kit issues (Windows bootstrap) are listed separately in
[`ci.md`](ci.md#upstream-issues-to-file-sc-lint).

1. **Scoop template was single-binary. Resolved.** (Merged upstream:
   https://github.com/randlee/sc-publish/pull/176, optional
   `[channels.scoop].binaries`.) sc-agent re-pinned the kit to sc-publish
   `34dab3f` and sets `binaries = ["sc-ai", "sc-treesitter"]`, so Scoop shims
   both executables. Bead `agt-scoop-multi-shim` is closed; ADR-SAG-0017 r-4
   amendment (phase-end ARCH-002, decision D8). WinGet is not part of this
   item: the first hand-submitted manifest lists a command alias per binary
   (bead `agt-sft`).
2. **`list-publish-plan` ignores `publish: false`.** `release_artifacts.py
   list-publish-plan` prints every entry in the consumer `crates` list without
   checking its `publish` flag, and `cargo publish` and the dry-run consume
   that list. sc-agent avoids the gap by leaving `sc-jev-py` (`publish =
   false`) out of `crates` entirely. Ask: skip entries with `publish: false`
   in the plan, or reject them in `validate-manifest`.

## Install

The kit is cloned at the pinned SHA into `.sc-publish-kit/`. The `sc-compose`
renderer lives in `.sc-publish-venv/`. Both are gitignored.

```bash
SC_PUBLISH_REVISION="$(python3 -c 'import tomllib; print(tomllib.load(open("release/sc-publish-pin.toml", "rb"))["sc-publish"]["revision"])')"
git clone --depth 1 "https://github.com/randlee/sc-publish" .sc-publish-kit
git -C .sc-publish-kit fetch origin "$SC_PUBLISH_REVISION" --depth 1
git -C .sc-publish-kit checkout "$SC_PUBLISH_REVISION"
publish_python="$(python3 .sc-publish-kit/plugins/sc-publish/.github/scripts/bootstrap_sc_compose.py --venv .sc-publish-venv)"
"$publish_python" .sc-publish-kit/plugins/sc-publish/install.py \
  --input release/sc-publish-consumer-input.json .
"$publish_python" .sc-publish-kit/plugins/sc-publish/install.py \
  --dry-run --input release/sc-publish-consumer-input.json .   # must exit 0
```

Before the first install, or before a pin bump that adds kit paths, move any
sc-agent file that sits at a kit path (see the Files table). Commit everything
install writes. Release workflows run the kit from the release tag.

## Dry-run drift check

`install.py --dry-run` exits 0 when every kit and rendered file matches the pin
and consumer input. It exits 1 and prints a diff when anything drifted. The full
local recipe is the same script PR CI runs:

```bash
bash scripts/sc-publish-dry-run.sh
# with uncommitted edits:
SC_PUBLISH_ALLOW_DIRTY=1 bash scripts/sc-publish-dry-run.sh
# keep the wheel (dir must be missing, empty, or named sc-agent-wheels):
SC_AGENT_WHEEL_DIR="$PWD/target/sc-agent-wheels" bash scripts/sc-publish-dry-run.sh
```

The script runs these phases in order. Pass phase names (`kit`, `checks`,
`publish`, `wheel`) to run only some of them; CI runs one per step.

1. `kit`: parse and validate the pin (`tomllib`; https URL, 40-hex SHA),
   clone or fetch the kit at it, then `git reset --hard` and `git clean -fdx`
   the checkout. Bootstrap `sc-compose` and run `install.py --dry-run`, which
   fails on drift.
2. `checks`: run `release_artifacts.py validate-manifest` and
   `verify-version-lockstep`, then `.github/scripts/run-consumer-preflight.sh`.
   The preflight runs the checks in `release/publish-preflight.toml` and is a
   no-op while none are declared.
3. `publish`: run `cargo publish --dry-run --locked` for every manifest crate,
   in publish order.
4. `wheel`: run `maturin build --release -m crates/sc-jev-py/Cargo.toml`. It
   needs `maturin` on `PATH`; CI pins `maturin==1.16.0`. Without
   `SC_AGENT_WHEEL_DIR` the wheel goes to a `mktemp -d` directory that is
   removed on exit.

Network steps run under a deadline (`SC_PUBLISH_NET_TIMEOUT`, default 300 s;
cargo and maturin `SC_PUBLISH_BUILD_TIMEOUT`, default 1800 s) and fail with the
step name. The script sets `CARGO_HTTP_TIMEOUT=60`, `CARGO_NET_RETRY=2` and
`PIP_DEFAULT_TIMEOUT=60` unless already set, and never retries a step.

`.github/workflows/sc-publish-dry-run.yml` runs that script on every pull
request (REQ-SAG-0132). It pins every action to a commit SHA, runs on
`ubuntu-24.04` with Rust 1.94.1, Python 3.11 and `maturin==1.16.0`, and gives
each phase its own `timeout-minutes`. The kit's Release Preflight
(`release-preflight.yml`) is `workflow_dispatch`-only and is run by the
publisher at release time, because it needs the release secrets and
environments below. Its kit-owned checks are the same manifest, lockstep and
consumer-preflight steps, plus credential liveness, registry state and
`cargo package`.

## Dependency pinning policy

ADR-SAG-0023 (`docs/architecture.md`) is the policy; NFR-SAG-0004 is its
requirement.

- Every dependency uses a caret range except deliberate lockstep. The
  lockstep crates, with their reasons, are the list between the
  `lockstep-pins` markers in ADR-SAG-0023; that list is the only copy, so it
  is not repeated here. `.github/scripts/check-exact-pins.sh` reads it and fails on
  any other `=` requirement in any `Cargo.toml`; `--self-test` proves it
  fails on an unlisted pin. CI runs both (`sc-agent-ci.yml`, job `deps`).
- `mimalloc` and `clap` are optional dependencies of the two binary crates,
  enabled by their default `cli` feature. The `[[bin]]` targets and the
  `cli` test targets use `required-features = ["cli"]`, so
  `cargo build -p sc-ai --lib --no-default-features` (and the same for
  `sc-treesitter`) builds the library without the allocator
  (REQ-SAG-0134; `.github/scripts/cargo-tree-gates.sh`).
- Shared requirements live in root `[workspace.dependencies]`; crates
  reference them with `.workspace = true`.

## Installing the binaries from crates.io

```bash
cargo install sc-ai --locked
cargo install sc-treesitter --locked
```

`--locked` builds with the `Cargo.lock` shipped in the package, so transitive
dependency versions match the tested release. Both binaries need the default
`cli` feature; do not pass `--no-default-features` to `cargo install`.

## Running the binaries on Windows

| Channel | `sc-ai` | `sc-treesitter` |
| --- | --- | --- |
| Scoop (`scoop install sc-ai` from `randlee/scoop-bucket`) | on `PATH` (shim) | on `PATH` (shim) |
| WinGet (`winget install randlee.sc-ai`) | on `PATH` (alias) | run `bin\sc-treesitter.exe` from the package directory under `%LOCALAPPDATA%\Microsoft\WinGet\Packages\` (`randlee.sc-ai_*`) |
| GitHub Release zip (`x86_64-pc-windows-msvc`) | `bin\sc-ai.exe` | `bin\sc-treesitter.exe` |
| crates.io | `cargo install sc-ai --locked` | `cargo install sc-treesitter --locked` |

Scoop shims both binaries. WinGet puts only `sc-ai` on `PATH` until the first
manifest is hand-submitted with both aliases (bead `agt-sft`; Komac 2.16.0
handles later updates). Until then, to get `sc-treesitter` on `PATH` from
WinGet, add the package's `bin` directory to `PATH` or install it with `cargo
install`.

## Pin bump

1. Pick a qualified kit SHA on `randlee/sc-publish` (full 40 characters; see
   the qualification checklist in `README.sc-publish.md`).
2. Set `revision` in `release/sc-publish-pin.toml`.
3. Remove `.sc-publish-kit/` (or `git -C .sc-publish-kit fetch origin <sha>
   --depth 1 && git -C .sc-publish-kit checkout <sha>`).
4. Diff the kit file inventory against this repository, and move any new
   colliding sc-agent files.
5. Run the install commands above, then `--dry-run` (exit 0).
6. Run `bash scripts/sc-publish-dry-run.sh`.
7. Commit the pin together with every file install changed, in one PR.

## Secrets and environments

The names are fixed by the kit's channel contract and are the same in every
repository. Create them in the GitHub repository settings. Never commit them.

| Channel | Secret | Scope | Environment |
| --- | --- | --- | --- |
| crates.io | `CARGO_REGISTRY_TOKEN` (publish-new, publish-update) | repository secret | jobs run in `crates-io` |
| PyPI | `PYPI_API_TOKEN` | environment secret | `pypi` |
| TestPyPI rehearsal | `TEST_PYPI_API_TOKEN` | environment secret | `testpypi` |
| Homebrew | `HOMEBREW_TAP_TOKEN` (contents write on `randlee/homebrew-tap`) | repository secret | none |
| Scoop | `SCOOP_BUCKET_TOKEN` (contents write on `randlee/scoop-bucket`) | repository secret | none |
| WinGet | `WINGET_GITHUB_TOKEN` (classic, `public_repo`, fork owner) | repository secret | none |

Release Preflight fails closed when an environment is missing. List what the
manifest needs with:

```bash
python3 .github/scripts/release_artifacts.py preflight-secret-plan \
  --manifest release/publish-artifacts.toml
```

## First-release checklist (REQ-SAG-0803)

One-time setup:

- [ ] Turn on immutable releases (Settings → General → Releases). `release.yml`
      fails closed without it.
- [ ] Create environments `crates-io`, `pypi` and `testpypi`, and add the
      secrets above.
- [ ] Make sure `randlee/homebrew-tap` and `randlee/scoop-bucket` each exist
      with at least one commit.
- [ ] Fork `microsoft/winget-pkgs` into the owner account, and get the first
      `randlee.sc-ai` version merged by hand (`wingetcreate new`). The workflow
      only updates an existing package.
- [ ] Have a PyPI account-scoped token ready for the first `sc-jev-py` upload.
- [ ] Have a `main` branch for the `develop` → `main` release-candidate flow.

Dry-run gate (local and CI, on the release candidate commit):

- [ ] `just lint` and `just test`.
- [ ] `install.py --dry-run` exits 0.
- [ ] `bash .github/scripts/run-consumer-preflight.sh` exits 0.
- [ ] `cargo publish --dry-run --locked -p sc-agent-types -p sc-agent-secrets
      -p sc-agent-http -p sc-jev -p sc-jev-backends -p sc-jev-check -p sc-jev-host
      -p sc-treesitter -p sc-ai`.
      A single multi-package invocation resolves the unpublished in-workspace
      dependencies in this order.
- [ ] `maturin build -m crates/sc-jev-py/Cargo.toml -o <dir>` produces a
      `.whl` (or run `bash scripts/sc-publish-dry-run.sh` for all of the above).
- [ ] `[workspace.package].version` equals `crates/sc-jev-py/pyproject.toml`
      `[project].version` (static, not `dynamic`).

Tag and release gate (owner sign-off only; sprint a-9 does neither):

1. Bump `[workspace.package].version` and the `sc-jev-py` `pyproject.toml`
   version together, and refresh `Cargo.lock`.
2. Run `release-candidate.yml` to tag `release-candidate-vX.Y.Z` on
   `origin/develop`. Merge to `main`.
3. Dispatch Release Preflight on the `main` commit (`run_by_agent=publisher`).
   Every channel must pass.
4. `release.yml` runs the TestPyPI rehearsal, then the production run: crates
   in publish order, tag `vX.Y.Z`, and the immutable GitHub Release with
   `checksums.txt`.
5. The publisher dispatches `pypi-publish.yml`, `homebrew-publish.yml`,
   `scoop-publish.yml` and `winget-publish.yml`. Each one can be retried by
   tag on its own.

Run the release through `/cursor-publish` (`.cursor/agents/publisher.md`) or
the ATM `publisher` teammate, never by hand-dispatching workflows out of order.
