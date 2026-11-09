# Changelog

All notable changes to sc-agent are recorded here. Nothing has been released
yet; every crate is at 0.1.0 and the stability promises (and the lack of them)
are in [`docs/stability.md`](docs/stability.md).

## [Unreleased] - 0.1.0

First release. Channels once published: crates.io (ten crates), PyPI
(`sc-jev-py`), Homebrew, Scoop and WinGet (`sc-ai`).

### Added

- Short backend names and a model catalog (bead agt-akp, closes agt-ho2).
  `--backend`, `backends=` and `check.toml` `backends` take `jev`, `haiku`,
  `luna` or a known model id of one of them; `anthropic_haiku`, `openai_luna`
  and the `open_ai_luna` / `open_ai` aliases are gone. `MODELS` and `ModelId`
  define `sonnet`, `opus`, `fable`, `terra`, `sol` and `astra` (not selectable
  yet) and `jev-1.14.0`, now the `jev` default. `~/.sc-agent/models.toml`
  adds models without recompiling; `sc-ai jev models` and
  `sc_jev_py.models()` list the resolved models and their source;
  `--models-file` reads another file. A local-only freshness test
  (`sc-agent-types/tests/model_freshness.rs`) fails when a provider lists a
  newer model than the crate table.

- `sc-ai`, the JSON-first CLI for Jev check parity: `jev run`, `jev dry-run`
  and `jev ask` (`jev stream` is below). Success writes one `{"version","operation","data"}` envelope
  to stdout; failure writes one `{"schema_version","error","message"}` line to
  stderr and exits 2. Backends: `jev` (default), `haiku`,
  `luna` (each the latest model of its line, or a known model id such as
  `claude-haiku-5-5` to pin it). Multi-backend runs report equal-input attestation.
- `sc-ai jev stream` and Python `run_many` / `DecisionStream` (bead agt-1aq,
  ADR-SAG-0032): many decision requests through one process on a bounded queue
  with N worker threads (`--concurrency`, default 8, 1 to 64). JSONL
  `{"id","request"}` in, one `{"id","ok","envelope"?,"error"?}` line out per
  request in completion order; a failed request is a typed error on its line
  and the stream continues; exit 0, or 3 when every failure is retryable, else
  2. Memory is bounded (backpressure to the producer); cancel stops new work.
  The default of 8 was tested only against scripted and loopback backends, not
  live providers. Out of scope: resume, sha256 lookup or any cache, a shared
  rate limiter.
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
- General log (bead agt-9eh): a second, separate `sc-observability` log of
  operator diagnostics, `<log-root>/logs/<service>-general.log.jsonl`, beside
  the session log. Start and end of operations, backend selection and
  resolved model, path resolution, credential source (never the value), busy
  retries and waits, extraction and git steps, session store actions, flush
  failures and failures by typed id; never keys, prompt text, state or
  provider bodies. The open `GeneralLog` port (`SharedGeneralLog`,
  `NoopGeneralLog`) sits beside `SessionLog` in `sc-agent-types`; both logs are
  injectable (`RunConfig::general_log`, `ComposeInputs::{session_log,
  general_log}`, Python `run_check(session_log=, general_log=)`) and fall back
  to the sc-agent defaults. `sc-ai` keeps the default loggers and `--log-dir`
  and has no logger flag. A dry run opens no log.
- `sc_jev_py` links the `sc-observability` Python runtime in its own binary:
  Rust writes the general log with no GIL, and
  `sc_observability.get_host_logger()` (after `sc_jev_py.install_host_logger()`)
  returns the same logger. New `general_log_file` and `install_host_logger`;
  the `sc-jev-py[observability]` extra pins `sc-observability==1.5.0`.
- A Rust/Python cross-implementation fixture for the canonical judge payload
  bytes (`tests/fixtures/README.md`).

- Streams: a cancel reaches a real busy-retry wait (the judge request's token
  governs the send), leaving a stream flushes its logs within 2 s, Python
  streams take `session_log=` and `general_log=`, `UreqTransport` keeps 64 idle
  connections per host, and a leading byte order mark on the input is ignored.

### Changed (public API shape before the first release)

- `pyo3` is pinned `=0.29.2` (the version the embedded `sc-observability`
  runtime pins), with `sc-observability-py`, `-binding-runtime` and `-dto`
  `=1.5.0`, on the lockstep list. The wheel's stable-ABI floor is `abi3-py310`;
  `requires-python` stays `>=3.11`.

- Jev layer (phase-end digests): `jev` is accepted as the latest Jev model
  next to versions such as `jev-1.13.0` (sent as `jev-latest`, not yet tested
  against the live API; it may break on an API change, so upgrade sc-agent),
  and a reply naming another model is no longer an error. Jev probability sums
  are accepted within 0.01 per option and never rounded. Question-set
  `criteria` descriptions are now sent, which changes the payload digest for
  sets whose criteria differ from their keys. Run output, Python and
  `RunResponse` gain `questions_sha256` (a question set, any state or order;
  `score` level order counts) and per-backend `request_sha256` (the exact
  first request body, computed before sending, for cached-response
  substitution; cache on pinned versions, not `jev`). `prompt_sha256` is
  renamed `payload_sha256` with no alias, and is a distinct type from the other
  two digests in Rust (`PayloadSha256`, `QuestionsSha256`, `RequestSha256`;
  wire and Python strings are unchanged). Python gains `request_json=` on
  `run_check` and `build_request`. The session log is schema version 2
  (`questions_sha256` on every row; tables `sessions`, `question_sets`,
  `payloads`, `answers`); version 1 rows and earlier session documents are
  refused.
- Errors (bead agt-2a3): the `sc-ai` stderr line is now `{error, kind, message,
  suggested_action, retryable, retry_after_seconds?, field?, details?,
  schema_version}` (`schema_version` stays 1; `retryable` is always present;
  `message` no longer holds the fix). New ids `AUTH_FAILED`,
  `API_REQUEST_INVALID`, `QUOTA_EXHAUSTED`, `PROVIDER_BUSY`, `API_ERROR` and
  `IO_FAILED`. Non-200 replies keep the provider's status, type, message and
  request id (1024 bytes per field, secrets redacted) instead of
  `NETWORK_ERROR` kind `status`; a 429 reporting exhausted quota is not
  retried. The token limit is `INPUT_TOO_LARGE` with `limit`/`actual`/`unit`.
  I/O failures are `IO_FAILED`; usage errors are `INPUT_INVALID` with `field`.
  `sc_jev_py.JevError` carries the same fields. Rust: `fix_hint()` is replaced
  by `message()` and `suggested_action()`; `Operational`, `OperationalArea`,
  `SessionStoreIo` and `BusyExhaustion` are removed.
  Exit codes: 2 for a failure that is not retryable, 3 for a retryable one
  (`sc-ai`, `sc-treesitter`). A run in which every backend failed now prints
  the envelope (no answers, each backend's typed error with
  `details.backend`) with the first failure on stderr, and `sc_jev_py`
  sets `.response`. A platform keyring read that blocks now fails with a
  retryable `IO_FAILED` after 15 s. A reader of an error line ignores unknown
  keys.

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
  text. Error text and codes are unchanged; a shape error anywhere in a reply
  now wins over an out-of-range value, and a reply that fails both ways
  reports the shape error.
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

### Changed

- Release kit re-pinned to sc-publish `34dab3f`. Scoop now shims both `sc-ai`
  and `sc-treesitter` (`[channels.scoop].binaries`; bead
  `agt-scoop-multi-shim` resolved). The kit pins Komac 2.16.0 for WinGet
  updates; the hand-submitted first manifest must list both binaries with
  aliases.
- Windows binaries link the C runtime statically (`.cargo/config.toml`), so
  they run without the Visual C++ redistributable.

### Known limits

- WinGet exposes only `sc-ai` until the first manifest is hand-submitted with
  an alias for each binary (bead `agt-sft`); `sc-treesitter` is in the
  package but has no alias yet.
- The Luna live token count is unverified (bead `agt-luna-live-count`).
- Backend ids are written `jev`, `haiku` and `luna`; no other spelling is read.
  Removal is tracked by bead `agt-ho2`; no removal is
  scheduled and it would be announced here.
