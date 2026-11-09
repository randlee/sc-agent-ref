# Changelog

All notable changes to sc-agent are recorded here. Every crate is at 0.1.0 and
the stability promises (and the lack of them) are in
[`docs/stability.md`](docs/stability.md).

## [Unreleased] - 0.1.0

First release. Channels: crates.io (ten crates), PyPI (`pip install sc-jev-py`,
the Python wheel), and Homebrew, Scoop and WinGet (`sc-ai`). The install
commands are in [`README.md`](README.md).

### Added

- `--backend` and Python `backends=` accept any model id of the resolved
  catalog (the crate table plus `~/.sc-agent/models.toml`, `--models-file` and
  `models_file=` included) that belongs to `jev`, `haiku` or `luna`, so a
  models-file row such as `jev-1.15.0` or `claude-haiku-5-6` can be pinned
  without recompiling. Ids no row lists, and ids of `sonnet`, `opus` and the
  other definition-only aliases, stay `INPUT_INVALID`. New API:
  `BackendChoice::parse_in`, `BackendSet::{parse_in, from_names_in}`,
  `ModelAliases::{alias_of, known_models}` and
  `sc_jev_host::compose::backend_set_from_names`; `check.toml` `backends`
  still takes the crate table's ids only.
- Session-log rows carry `answered_model_id`, the model the reply named
  (`jev-1.14.0`), next to `model_id`, the model requested (`jev`); it is null
  for a failed send. `SESSION_LOG_SCHEMA_VERSION` is 3 (a new column), with
  `RowContext::answered_model_id` and `RowContextBuilder::answered_model_id`.
- The stderr of a `check.toml` `command` extractor has bearer-token shapes
  (`Bearer <token>` and `sk-` keys) replaced by `[redacted]` in errors, not
  only the values of `*_API_KEY` and `SC_*` environment variables. A secret
  of another shape that this process does not hold is not recognized.
- A Quick start in `README.md` and the `sc-ai` crate page (a minimal
  `check.toml`, `dry-run`, `run`, the key variable per backend and its lookup
  order, build prerequisites) and `docs/check-reference.md` (`check.toml`, both
  question formats, a `--request-json` example, Python `run_check`). Their
  examples are tested.
- `KEY_MISSING` and a keystore `IO_FAILED` now name the backend's variable
  (`TYPESAFE_API_KEY`, `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`) and the
  repo-root `.env` in `suggested_action`; the error ids and retryability are
  unchanged. `sc_jev_py.JevError` carries the same text.
- `ANTHROPIC_KEYCHAIN_SERVICE` is read from the process environment only; a
  repo-root `.env` can no longer choose the keychain item (REQ-SAG-0201).

- `sc_jev_py.ask(session_id, follow_up, *, project, repo_root, session_dir,
  log_dir, models_file, session_log, general_log)` resumes a stored session by
  ULID, from any process, through the same `ask_from` as `sc-ai jev ask`.
- Short backend names and a model catalog (bead agt-akp, closes agt-ho2).
  `--backend`, `backends=` and `check.toml` `backends` take `jev`, `haiku`,
  `luna` or a known model id of one of them; `anthropic_haiku`, `openai_luna`
  and the `open_ai_luna` / `open_ai` aliases are gone. `MODELS` and `ModelId`
  define `sonnet`, `opus`, `fable`, `terra`, `sol` and `astra` (not selectable
  yet) and `jev-1.14.0` (a pin; `jev` still means the latest). `~/.sc-agent/models.toml`
  adds models without recompiling; `sc-ai jev models` and
  `sc_jev_py.models()` list the resolved models and their source;
  `--models-file` reads another file. A local-only freshness test
  (`sc-agent-types/tests/model_freshness.rs`) fails when a provider lists a
  newer model than the crate table.

- `sc-ai`, the JSON-first CLI for Jev check parity: `jev run`, `jev dry-run`
  and `jev ask` (`jev stream` is below). Success writes one `{"version","operation","data"}` envelope
  to stdout; failure writes one error line to stderr, with `error`, `kind`, `message`,
  `suggested_action`, `retryable`, `schema_version` and, when they apply,
  `retry_after_seconds`, `field` and `details`. Exit is 0 on success, 3 when the
  failure is retryable, otherwise 2. Backends: `jev` (default), `haiku`,
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

None of this shipped in a release. It records how the API changed during
development, for anyone who used a pre-release build. Where an entry here
describes an older shape, the `Added` section above is what 0.1.0 does.

- `sc_decision_schema::jev::v1::JevState` is `#[non_exhaustive]`, so a later
  `state` form is a new variant, not a breaking change; a `match` outside the
  crate needs a wildcard arm.
- `sc_decision_schema::ChoiceAnswer` has private fields, like `Question` and
  `QuestionAnswer`: build it with `ChoiceAnswer::new(question_id, choice)` and
  read it with `question_id()` and `choice()`. The JSON form is unchanged.
- `sc_ai::RunCommand::backends` and `StreamCommand::backends` are `Vec<String>`
  (checked against the catalog when the command runs), not `Vec<BackendChoice>`.
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
  non-retryable `IO_FAILED` (`io_kind` `other`) after 15 s. A reader of an error line ignores unknown
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
- `sc_decision_schema::LlmWireContext` has a private `system_prompt` field;
  read it with `system_prompt() -> &str`. `LlmWireContext::new` is the only
  constructor.
- `sc_decision_schema::session_log::SendOutcome` is `#[non_exhaustive]`: add a
  wildcard arm when matching it outside the crate. Serde values (`ok` /
  `failed`) are unchanged.
- `BackendSet` implements `From<BackendId>` (a one-backend set, infallible).
- `sc_agent_types::schema::subjects::SubjectsProducer::new` takes a named
  `GrammarRef { language, grammar, grammar_version }` instead of three
  positional strings.

### Changed

- Release kit re-pinned again to sc-publish `44cebf2`; the Windows `RUSTFLAGS`
  re-qualification is done (ADR-SAG-0023 amendment). CI's `darwin-link` and
  macOS wheel jobs run on `macos-15` (`macos-14` is unsupported).
- Release kit re-pinned to sc-publish `34dab3f`. Scoop now shims both `sc-ai`
  and `sc-treesitter` (`[channels.scoop].binaries`; bead
  `agt-scoop-multi-shim` resolved). The kit pins Komac 2.16.0 for WinGet
  updates; the hand-submitted first manifest must list both binaries with
  aliases.
- Windows binaries link the C runtime statically (`.cargo/config.toml`), so
  they run without the Visual C++ redistributable.
- `NETWORK_ERROR` kinds on real failures: a DNS lookup failure is now
  `connect` (retryable, exit 3) instead of `other`, and a TLS failure (an
  untrusted or self-signed certificate, a peer that does not speak TLS) is now
  `tls` (not retryable) instead of `other`.

### Known limits

- WinGet exposes only `sc-ai` until the first manifest is hand-submitted with
  an alias for each binary (bead `agt-sft`); `sc-treesitter` is in the
  package but has no alias yet.
- `cargo install sc-ai` installs only `sc-ai`; install `sc-treesitter`
  separately (`cargo install sc-treesitter --locked`) or use Homebrew or Scoop.
- A TLS-inspecting proxy is not supported: the HTTP client (`ureq` with
  `rustls`) trusts only the bundled `webpki-roots`, not the platform store.
- `sc-ai jev dry-run` runs `command` inputs from `check.toml`, so a
  `check.toml` from an untrusted change is code execution.
- A Jev run saves no session (`session_id` is `null`), so `jev ask` applies to
  `haiku` and `luna` runs only. The CLI and Python use different default log
  and session roots (`<repo>/.sc-ai` and `~/.sc-agent/log`); Python also reads
  `SC_AGENT_LOG_DIR`, the CLI does not.
- The Luna live token count is unverified (bead `agt-luna-live-count`).
- Backend ids are written `jev`, `haiku` and `luna`; no other spelling is read.
