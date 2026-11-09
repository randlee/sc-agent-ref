# sc-agent-types — architecture

**Boundary:** `agent-types`
**Depends on:** `sc-sha`, `serde`, `serde_json`, `thiserror`, `ulid`. No
`sc-observability*`, HTTP, Python, or tree-sitter crate (REQ-SAG-0107).

**Contract status:** the a-1 frozen contract was unfrozen once by sprint r-1
(one deliberate pre-1.0 break, ADR-SAG-0027) and is re-frozen at r-1 merge.
Later changes are additive: new enum variants (every public enum is
`#[non_exhaustive]`), new `BACKENDS` rows, new default-bodied trait methods.

## Responsibilities

- The one error enum for Jev and the CLIs (`JevError`) and the stderr error
  shape (`ScAgentErrorResponse`).
- Ids and the static backend catalog (`BackendId`, `BACKENDS`).
- The model catalog (`ModelId`, `MODELS`, `ModelAliases`), the per-send
  `SendTrace` handle and `ProjectLabel` (r-7).
- Not here: the judge contract (`JudgeBackend`, `JudgeRequest`,
  `JudgeOutcome`) and the canonical payload (`PayloadBundle`) live in
  `sc-decision-schema`, one layer above this crate (ADR-SAG-0030). This crate
  re-exports none of them (a re-export would be a Cargo cycle).
- The `sc/subjects/1` schema (`schema::subjects`) and its id builder.
- The session and general log ports (`observability::v2`) and argv-only git helpers.

## Cargo features (module seams)

| Feature | Default | Modules | Types |
| --- | --- | --- | --- |
| (core) | always | `error`, `envelope`, `ids`, `models`, `schema`, `cancel`, `trace` | `JevError`, `ErrorCode`, `ErrorCategory`, `NetworkErrorKind`, `BusyCause`, `ResponseInvalidCause`, `LimitUnit`, `ProviderFailure`; `ScAgentErrorResponse`, `ErrorDetails`; `BackendId`, `BackendKeyId`, `BackendFamily`, `BackendDescriptor`, `BACKENDS`, `BackendChoice`, `BackendSet`, `SessionId`, `Rev` (built only by `Rev::parse` or `Rev::worktree()`; no unchecked constructor), `RepoRoot`, `RepoRelativePath`, `BlobSha`, `Sha256Digest` (`of_text`), `AnswerId`, `ProjectLabel`, `Operation`; `ModelId`, `UnknownModel`, `ModelFamily`, `ModelDescriptor`, `MODELS`, `ModelAliases`, `ModelSource`, `ResolvedModel`; `schema::subjects::*` (`SubjectsDocument`, `SubjectsBuilder`, `SubjectsSource` and `SubjectsProducer` with private fields, constructors and accessors, `GrammarRef` (the language, grammar crate and version a producer takes), `QueryMode`, `Span`, `Subject`); `CancelToken`, `SendTrace` |
| `secret` | yes | `secret` | `ResolvedApiKey` |
| `obs` | yes | `observability::v2` | `SessionLog`, `SharedSessionLog`, `LogEvent`, `LogRejected`, `NoopSessionLog`; `GeneralLog`, `SharedGeneralLog`, `GeneralEvent`, `GeneralLevel`, `NoopGeneralLog`, `RecordingGeneralLog`, `RejectingGeneralLog` |
| `git` | no | `git` | `discover_toplevel`, `read_at_rev`, `GitBlob` |

`secret`, `obs` and `git` are independent. The `judge` and `test-util` features were deleted in r-7 with the moved modules (REQ-SAG-0107 amended). Every
combination builds (`cargo hack check -p sc-agent-types --feature-powerset
--no-dev-deps`). Every type above is reached through a crate-root
re-export; the modules named in the table are private except `schema`,
`observability` and `git`. The catalog and the error-code enums
live in the private `catalog` and `code` modules because sc-boundary 0.8
reports a type cycle inside one module.

## Errors (ADR-SAG-0027)

Every variant maps to exactly one `errors.json` id; an id may have several
variants (REQ-SAG-0101 as amended). Recovery categories are explicit
variants (`CredentialStoreInvalid`, `RegistryInvalid`,
`SessionDocumentInvalid`, `EncodeFailed`, `IoFailed`); the provider-reply
variants (`AuthFailed`, `ApiRequestInvalid`, `QuotaExhausted`, `ApiError`)
carry a `ProviderFailure`, and `ProviderBusy` carries `status`, `sends`,
`cause` and `retry_after_seconds`; producer variants carry a
`producer: String`; `ProducerError` re-raises a producer's stderr with its
own `code` and every other field the producer sent. Every struct-like variant
has a snake-case constructor. `message()` (the `Display` text, what failed)
and `suggested_action()` (what to do) are the two renderers of text, and
`details()` is the machine fields; `fix_hint()` is gone. The wrapper variant
`Annotated { error, flush_failure, backend }` carries `details.backend` and
`details.log_flush_failure` while keeping the inner error's id, message,
action and retryability (ADR-SAG-0027 agt-2a3 amendment). `NetworkErrorKind`
includes `other` (unclassified, not retryable). `retryable()` is
true for `NetworkError` of kind `Timeout` or `Connect`, `ProviderBusy`,
`ApiError` from status 500 and above, and a producer's error as the producer says;
`Error::source()` is `None` for every variant. The id rule (`INPUT_INVALID`
vs `CONFIG_INVALID` vs `INPUT_MISSING` vs `IO_FAILED`) is in the `error`
module documentation and `docs/stability.md`.

**Git deadline (r-7, ADR-SAG-0027).** Every git subprocess in `git` has a fixed
60 s wall-clock deadline (`GIT_DEADLINE`, crate-private, not configurable). The
child is polled with `try_wait`; at the deadline it is killed and reaped and
the call returns `JevError::GitTimeout { seconds }`. `GitTimeout` maps to the
existing `EXTRACTION_TIMEOUT` id (an id may have several variants) and has its
own `Display` ("git did not finish within Ns") and hint (a stalled git
process, a lock file or a slow filesystem), not the extractor's "raise
`timeout_seconds`".

`ScAgentErrorResponse` keys, in order: `error`, `kind`, `message`,
`suggested_action`, `retryable` (always), `retry_after_seconds`, `field`,
`details` (the closed `ErrorDetails` object, keys in `docs/stability.md`),
`schema_version` (always `1`; unchanged because nothing was published).
Readers ignore unknown keys, accept `schema_version` absent or `1`, and require
only `error` and `message`. A golden test (`error-codes.json`) pins the whole
body of every variant shape; `SC_UPDATE_GOLDEN=1` rewrites it.

## Backends

`BACKENDS` holds one `BackendDescriptor` per built-in id: `name`, read
`key_id`, `family`, `requires_llm_context`,
`supports_session_ask`, `request_limit_bytes`. `BackendId::descriptor` is the
one exhaustive `match`; `Display`, serde and `FromStr` read the descriptor,
so `jev`, `haiku` and `luna` are the only spellings (no read aliases).
`BackendChoice` also accepts a known model id (a pin), which `BackendSet`
carries (`pin`). `JudgeBackend::capabilities()` is provided from
the descriptor. Callers branch on descriptor facts, never on id equality.

## Judge contract

`PayloadBundle::encode(&CanonicalJudgePayload)` builds every bundle (bytes,
digest, and the question → choices map); `decode` re-encodes stored bytes and
refuses non-canonical ones. `JudgeRequest` shares the bundle as an `Arc` and
carries a `CancelToken`. `JudgeOutcome::new(backend, &request, answers)`
refuses answers whose ids differ from the questions or whose choice (or
probability key) was not offered. `QuestionAnswer` keeps `choice`, optional
`confidence` and `probabilities` in `[0, 1]`. `EqualInputRecord.equal()` is
derived from per-backend `Attestation`s.

`JudgeBackend` is intentionally open; calls block (async hosts use
`spawn_blocking`). `MockJudgeBackend` (in `sc-decision-schema`'s `test_util`, not this crate) is the one double: it
answers through `JudgeOutcome::new`, records requests, counts validations,
reports the received digest, and enforces LLM context from the descriptor.

## Session log port (`observability::v2`)

`SessionLog::emit(LogEvent) -> Result<(), LogRejected>`; `LogEvent` carries
`session_id`, `operation: Operation` and `fields`, a JSON object that is one
ADR-SAG-0031 session-log row (`LogEvent::new(session_id, operation, fields)`,
accessors `session_id()`, `operation()`, `fields()`; r-6). The port does not
name the row type, and the row never holds a secret.

`SessionId` and `AnswerId` wrap `ulid::Ulid` (26-character Crockford base32,
strict uppercase; a UUID is `INPUT_INVALID`). `Sha256Digest::of_text` goes
through `sc-sha`, which normalizes CRLF and bare CR to LF, so the digest is
SHA-256 of the bytes only for text without CR (all compact JSON). With the
`git` feature, `git::discover_common_dir` finds the repository a linked
worktree belongs to; it reuses the one argv-only spawn site. `sc-ai` and `sc-jev-py` adapt their `sc_observability::v2::Logger` to
it; `sc-jev-check` takes a `SharedSessionLog`. Removal of the module still
waits on upstream v2 (bead `agt-obs-v2-shim-removal`).

## General log port (`observability::v2`, agt-9eh)

Beside `SessionLog` sits the general log for operator diagnostics
(ADR-SAG-0013 amendment 2026-11-09). `GeneralLog::log(GeneralEvent) ->
Result<(), LogRejected>` is an open `Send + Sync` trait; `SharedGeneralLog` is
`Arc<dyn GeneralLog>`; `NoopGeneralLog` is the library default. A
`GeneralEvent` is a `GeneralLevel`, a `'static` `target` and `action`, a
message and structured fields; `GeneralEvent::emit(&dyn GeneralLog)` sends it
and drops a rejection, because logging never changes a result.
`with_error_code` records the typed `ErrorCode` id only, never error text.
`RecordingGeneralLog` and `RejectingGeneralLog` are the test doubles. The port
links no `sc-observability*` crate; `sc-agent-http` enables `obs` for it, and
`sc-treesitter` and `sc-decision-schema` stay off it.

## Subjects (`schema::subjects`)

`SubjectsDocument` is the one validated type: private fields, built by
`SubjectsBuilder::build` or by parsing, both running the same validation.
`SubjectsBuilder` assigns ids (`{path}:{line}`, then `:1`, `:2`, ...). Rules:
[`subjects-schema.md`](subjects-schema.md). `InputSpec` moved to
`sc-jev-check::config` in r-1.

## Non-responsibilities

HTTP, keyring, `check.toml` parsing, CLI argv, building the real logger.
