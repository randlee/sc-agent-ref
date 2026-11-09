# NOTES r-3b (untracked, resume file)
S=/private/tmp/claude-501/-Users-randlee--claude-spares-spare-ZVG33J/84ad1f90-7f83-4b6e-b4fc-63121d353959/scratchpad/r3b-only
CARGO_TARGET_DIR=$S/target
Order: D2 compose crate -> D12/D3 sc-ai -> sc-jev-py -> D7 workspace-tests -> CI/gates -> ADR/docs -> validate
## progress
- compose crate src+tests(obs,paths) written (not yet built); workspace-tests manifest stub; root Cargo.toml members added
- Decisions: ComposeInputs has extra repo_root setter + resolve()->ResolvedPaths + explicit_repo_root(); ask_from returns AskResponse; empty flag = absent; python CheckResult gets `data` getter
- sc-ai: lib (Command/Output/execute), bin cli.rs (clap), argv.rs written, observe.rs deleted; cli.rs being rewritten (binary-only, --scripted-answers); flush warning test uses unix FIFO trick (15s)
- obs::open takes &PathPolicy (flag name in hints)
- TODO: clippy.toml Command::new entry + sweep (tests/support/*.rs outside owned_paths -> report), sc-jev-py rewiring, workspace-tests targets, publish input+install.py, CI, ADR docs, docs/sc-agent-compose, README
- RENAMED sc-agent-compose -> sc-jev-host (owner 2026-11-07); docs edited via sed (all docs/** mentions, boundaries/sc-agent-http/agent-http.toml comment). release json/install.py still TODO; report kit re-render
- FINAL NAME sc-jev-host (owner 2026-11-07, supersedes sc-jev-host). run_from/prepare_from(HostRun) added; PathPolicy::default_log_root added
- sc-jev-py rewired to host (run.rs rewritten, logging.rs deleted, chain_context, JevError.response, CheckResult.data). TODO: .pyi, pytest additions, golden fixture, workspace-tests targets, clippy.toml sweep, publish input, CI, docs
- FINAL FINAL NAME sc-jev-host (owner 2026-11-07; sc-decision-host disregarded). Workspace-tests: jev_cross_impl moved, support/bin_path.rs written; pyi+pytest+golden done; TODO other wt targets
- wt targets done: jev_cross_impl,e2e,consumer_parity,git_read_parity,test_util_off. TODO source_scans, clippy.toml sweep, README wt, publish input/install, CI/gates, docs, host tests extra, sc-jev-py crate docs
- done: boundaries manifests, sc-lint.toml, clippy sweep, release input json (sc-jev-host order7; kit not present -> parent re-render), CI workflow, tree gates (dup mode; spec -d grep is unanchored -> deviation). TODO docs (crate docs, ADR flips, req), README wt, host extra tests, sprint doc impl notes, full validation
