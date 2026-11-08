# workspace-tests

Cross-crate proofs for the sc-agent workspace (`publish = false`; ADR-SAG-0028,
REQ-SAG-0109). The crate has no normal dependencies and an empty `src/lib.rs`.
Its dev-dependencies are `sc-agent-types`, `sc-jev-check`, `sc-jev-host`,
`serde_json` and `tempfile`; `sc-ai` and `sc-treesitter` are never linked.
They run as built binaries, so `pyo3` and `tree-sitter` stay out of this graph.
There is no `docs/workspace-tests/`; this file is the documentation.

**Built binaries.** `tests/support/bin_path.rs` runs
`cargo build --release -p <pkg> --features cli --target-dir <isolated>
--message-format=json` (through `env!("CARGO")`) and reads the executable
from the `compiler-artifact` message. `<isolated>` is
`$CARGO_TARGET_DIR/workspace-tests-bin`, else
`<workspace root>/target/workspace-tests-bin`. It is never the directory
`cargo test` uplifts a binary into, which may be built with `test-util` on, so
the helper runs release-shaped binaries only.

- **`jev_cross_impl`**: Rust canonical bytes match the committed Python
  reference bytes (`tests/fixtures/jev/cross_impl`; REQ-SAG-0102).
- **`e2e_treesitter_producer`**: a `check.toml` with a `kind = "command"`
  input running the built `sc-treesitter` goes through
  `sc_jev_host::compose::run_from` with `dry_run`; the payload digest equals
  the committed `branches` fixture, and a bad language is
  `LANGUAGE_UNAVAILABLE` re-raised as `ProducerError` (REQ-SAG-0310,
  REQ-SAG-0313).
- **`consumer_parity`**: the built `sc-ai` dry-run's `data` equals
  `tests/fixtures/jev/run-response/dry_run_cli.json`; the pytest
  `test_run_data_matches_golden` pins Python to the same file.
- **`git_read_parity`**: `sc_agent_types::git::read_at_rev` matches
  `git cat-file blob`, the blob id `sc-treesitter` reports, and what
  `sc-jev-check` reads at a revision; `--output=FILE` as `--rev` creates no
  file.
- **`source_scans`**: reads files, links nothing. Needles are built from
  fragments so the file never contains a literal it hunts; attributes are read
  whole (from `#` to the matching `]`, across lines, whitespace removed,
  comments and strings skipped) over every target under `crates/**/*.rs`.
  `test_no_allow_attributes` is the one robust allow check (it replaces the
  `rg` gate); `test_disallowed_methods_expect_sites` allows an expect for the
  disallowed-method lint only at the secret read and the listed process-spawn
  sites, or in a test file with an item-level expect whose reason names a
  process spawn and that never mentions the secret type;
  `test_lint_mechanism_present` fails if `clippy.toml`, the workspace lint
  levels, `resolver = "2"`, the member layout or the `just lint` clippy command
  is weakened; `test_every_crate_inherits_workspace_lints`;
  `test_env_reads_only_in_owners` (a text scan: an aliased import evades it,
  and review covers that).
- **`test_util_off`**: no workspace crate's `test-util` feature is on a normal
  edge (`cargo tree -e normal,features`, with a positive control).
- **`test_util_off_binary`**: the release-built `sc-ai` rejects
  `--scripted-answers`. A separate target, so its `cargo build` never runs
  alongside `test_util_off`'s serialized `cargo tree` calls.
