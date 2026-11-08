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

### Changed (public API shape before the first release)

- `Probability` moved from `sc_decision_schema::jev::v1` to the crate root
  (`sc_decision_schema::Probability`) and gained `complement()`.
  `QuestionAnswer::new` now takes `Option<Probability>` and
  `Option<BTreeMap<ChoiceId, Probability>>` and cannot fail (it returns `Self`);
  `QuestionAnswer::confidence()` and `probabilities()` return `Probability`
  values. JSON, session rows and Python floats are unchanged.
- `CheckResponse::parse` names the answer id and field (`confidence` or
  `probabilities`) of the first out-of-range value, never the value;
  `Probability::new` no longer echoes its value in its error text.
- `sc_treesitter::ExtractInput` is `#[non_exhaustive]`, no longer `Copy`,
  and has private fields with accessors (`language()`, `query()`,
  `captures()`, `source()`, `path()`, `rev()`, `blob_sha()`). `ExtractInput::new`
  takes `RepoRelativePath`, `Rev` and `BlobSha` after `language`, `query` and
  `source`, so a malformed value fails where the caller builds it and an input
  cannot lack one; there are no `with_path`, `with_rev` or `with_blob_sha`
  setters and the accessors return the values directly. Only `with_captures`
  remains. CLI error codes and messages are unchanged.
- `sc_agent_types::schema::subjects`: `SubjectsBuilder::push_subject(span,
  text, kind)` no longer takes a path (ids use `source.path`) and takes and
  returns `self`, so calls chain; `Subject::span()` returns `Span` by value.
  `GrammarRef` keeps its three public fields, which are frozen for
  `sc/subjects/1`. Serialized subject ids are unchanged.
- `sc_agent_types` ids: `BlobSha` gains `AsRef<str>` and `Ord`; `Rev` gains
  `FromStr`, `TryFrom<&str>` and `AsRef<str>`. `AnswerId` no longer
  implements `Default` (use `AnswerId::new()`), like `SessionId`.
- `sc_decision_schema::jev::v1::CheckResponse::parse` range-checks
  probabilities and confidence on typed values rather than by matching error
  text. Error text and codes are unchanged, except that a reply with several
  out-of-range answers still names the least answer id.
- `sc_jev_check::config`: `InputSpec` and its struct variants are
  `#[non_exhaustive]` and gain the constructors `file`, `table_column`,
  `subjects` and `command`. A command input is `InputSpec::Command(CommandSpec)`;
  `CommandSpec::new(argv, path)` and `with_timeout_seconds` return
  `CONFIG_INVALID` for `{path}` in the argv with no path and for a zero
  timeout, with the same text the extractor used. `with_producer` and the
  accessors are on `CommandSpec`, not `InputSpec`. Those two errors now
  surface when `check.toml` is loaded rather than when the input runs, so
  they win over a missing questions or state file and over an earlier
  input's failure. `CheckSpec` has private fields
  with accessors (`questions_path()`, `state_path()`, `system_prompt_file()`,
  `inputs()`, `backends()`, `models()`) and by-value setters
  (`with_state_path`, `with_system_prompt_file`, `with_input`,
  `with_backends`, `with_models`). New `InputName` newtype (`[A-Za-z0-9_-]+`)
  keys `CheckSpec::inputs()`; `check.toml` parsing and hand-built specs share
  its validation, so a bad name fails at construction. Other `check.toml`
  behaviour and error text are unchanged.
- `sc_decision_schema::jev::v1::JevAnswer` struct variants and
  `sc_decision_schema::QuestionKind` are `#[non_exhaustive]`: match struct
  variants with `..` and add a wildcard arm for `QuestionKind`.
- `BackendSet` implements `From<BackendId>` (a one-backend set, infallible).
- `sc_agent_types::schema::subjects::SubjectsProducer::new` takes a named
  `GrammarRef { language, grammar, grammar_version }` instead of three
  positional strings.

### Known limits

- Scoop and WinGet expose only `sc-ai`; `sc-treesitter` is in the archive but
  has no shim or alias (beads `agt-scoop-multi-shim`, `agt-sft`).
- The Luna live token count is unverified (bead `agt-luna-live-count`).
- Backend ids are written `openai_luna` / `openai`; the old spellings
  `open_ai_luna` / `open_ai` are still read in config and caller-request input.
  Removal is tracked by bead `agt-ho2`; no removal is
  scheduled and it would be announced here.
