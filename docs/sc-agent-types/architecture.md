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
- The session log port (`observability::v2`) and argv-only git helpers.

## Cargo features (module seams)

| Feature | Default | Modules | Types |
| --- | --- | --- | --- |
| (core) | always | `error`, `envelope`, `ids`, `models`, `schema`, `cancel`, `trace` | `JevError`, `ErrorCode`, `NetworkErrorKind`, `OperationalArea`; `ScAgentErrorResponse`; `BackendId`, `BackendKeyId`, `BackendFamily`, `BackendDescriptor`, `BACKENDS`, `BackendSet`, `SessionId`, `Rev` (built only by `Rev::parse` or `Rev::worktree()`; no unchecked constructor), `RepoRoot`, `RepoRelativePath`, `BlobSha`, `Sha256Digest` (`of_text`), `AnswerId`, `ProjectLabel`, `Operation`; `ModelId`, `UnknownModel`, `ModelFamily`, `ModelDescriptor`, `MODELS`, `ModelAliases`; `schema::subjects::*` (`SubjectsDocument`, `SubjectsBuilder`, `SubjectsSource` and `SubjectsProducer` with private fields, constructors and accessors, `QueryMode`, `Span`, `Subject`); `CancelToken`, `SendTrace` |
| `secret` | yes | `secret` | `ResolvedApiKey` |
| `obs` | yes | `observability::v2` | `SessionLog`, `SharedSessionLog`, `LogEvent`, `LogRejected`, `NoopSessionLog` |
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
variants (`CredentialStoreInvalid`, `RegistryInvalid`, `SessionStoreIo`,
`Operational { area }`, `SessionDocumentInvalid`, `EncodeFailed`, `IoFailed`);
producer variants carry a `producer: String`; `ProducerError` re-raises a
producer's stderr with its own `code`. Every struct-like variant has a
snake-case constructor. `fix_hint()` is the one renderer of recovery text;
`retryable()` is true only for `NetworkError` of kind `Busy`, `Timeout`,
`Connect`; `Error::source()` is `None` for every variant.

**Git deadline (r-7, ADR-SAG-0027).** Every git subprocess in `git` has a fixed
60 s wall-clock deadline (`GIT_DEADLINE`, crate-private, not configurable). The
child is polled with `try_wait`; at the deadline it is killed and reaped and
the call returns `JevError::GitTimeout { seconds }`. `GitTimeout` maps to the
existing `EXTRACTION_TIMEOUT` id (an id may have several variants) and has its
own `Display` ("git did not finish within Ns") and hint (a stalled git
process, a lock file or a slow filesystem), not the extractor's "raise
`timeout_seconds`".

`ScAgentErrorResponse` keys, in order: `error`, `message`, `schema_version`
(always `1`), `retryable` (only when `true`), `details` (only for
`ProducerError`, `{"producer": ...}`). Readers deny unknown keys and accept
`schema_version` absent or `1`.

## Backends

`BACKENDS` holds one `BackendDescriptor` per built-in id: `name`, read
`aliases`, `key_id`, `family`, `requires_llm_context`,
`supports_session_ask`, `request_limit_bytes`. `BackendId::descriptor` is the
one exhaustive `match`; `Display`, serde and `FromStr` read the descriptor,
so `openai_luna` / `openai` are the only written spellings and `open_ai_luna`
/ `open_ai` are read aliases. `JudgeBackend::capabilities()` is provided from
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
`spawn_blocking`). `test_util::MockJudgeBackend` is the one double: it
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

## Subjects (`schema::subjects`)

`SubjectsDocument` is the one validated type: private fields, built by
`SubjectsBuilder::build` or by parsing, both running the same validation.
`SubjectsBuilder` assigns ids (`{path}:{line}`, then `:1`, `:2`, ...). Rules:
[`subjects-schema.md`](subjects-schema.md). `InputSpec` moved to
`sc-jev-check::config` in r-1.

## Non-responsibilities

HTTP, keyring, `check.toml` parsing, CLI argv, building the real logger.
