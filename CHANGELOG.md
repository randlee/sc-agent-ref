# Changelog

All notable changes to sc-agent are recorded here. Nothing has been released
yet; every crate is at 0.1.0 and the stability promises (and the lack of them)
are in [`docs/stability.md`](docs/stability.md).

## [Unreleased] - 0.1.0

First release. Channels once published: crates.io (ten crates), PyPI
(`sc-jev-py`), Homebrew, Scoop and WinGet (`sc-ai`).

### Added

- `sc-ai`, the JSON-first CLI for Jev check parity: `jev run`, `jev dry-run`
  and `jev ask`. Success writes one `{"version","operation","data"}` envelope
  to stdout; failure writes one `{"schema_version","error","message"}` line to
  stderr and exits 2. Backends: `jev` (default), `anthropic_haiku`,
  `openai_luna`. Multi-backend runs report equal-input attestation.
- `sc-treesitter`, a tree-sitter producer of `sc/subjects/1` documents
  (Python and Rust grammars behind features).
- `sc_jev_py`, a Python wheel (PyO3, maturin) that runs the same engine
  in-process: `run_check`, `build_request`, sessions, and the canonical-bytes
  helpers, with type stubs.
- Library crates: `sc-agent-types`, `sc-decision-schema` (payload, judge
  contract, `jev::v1` wire types, session-log schema),
  `sc-agent-secrets`, `sc-agent-http`, `sc-jev`, `sc-jev-backends`,
  `sc-jev-check`, and `sc-jev-host` (internal support, no semver promise).
- Session logs: one JSONL line per send under `<repo-root>/.sc-ai`, each
  carrying one session-log row; no API keys, prompt text or request bodies are
  recorded.
- A Rust/Python cross-implementation fixture for the canonical judge payload
  bytes (`tests/fixtures/README.md`).

### Known limits

- Scoop and WinGet expose only `sc-ai`; `sc-treesitter` is in the archive but
  has no shim or alias (beads `agt-scoop-multi-shim`, `agt-sft`).
- The Luna live token count is unverified (bead `agt-luna-live-count`).
- Backend ids are written `openai_luna` / `openai`; the old spellings
  `open_ai_luna` / `open_ai` are still read in config and caller-request input.
  Removal is tracked by bead `agt-backend-serde-alias-removal`; no removal is
  scheduled and it would be announced here.
