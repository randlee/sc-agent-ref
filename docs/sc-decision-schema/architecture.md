# sc-decision-schema — architecture

**Depends on:** `sc-agent-types` (`default-features = false`, core only),
`serde`, `serde_json`. Boundary: `boundaries/sc-decision-schema/decision-schema.toml`.

## Layering

```text
sc-agent-types        ids, JevError/ErrorCode, BackendId + BACKENDS, ModelId/ModelAliases,
  (below)             Sha256Digest, CancelToken, SendTrace, ProjectLabel, AnswerId, ...
sc-decision-schema    QuestionId, ChoiceId, Question, ChoiceAnswer, CanonicalJudgePayload,
  (above)             PayloadBundle, QuestionAnswer, DecisionAnswers, JudgeOutcome,
                      JudgeRequest, LlmWireContext, JudgeBackend, BackendCapabilities,
                      Attestation, BackendAttestation, EqualInputRecord, MultiBackendMode,
                      DecisionSchemaId, llm::v1, variants, session_log, test_util
```

## Data-only rule

The dependency set is the rule, enforced by the boundary manifest and the
`cargo-tree-gates.sh` feature row (it reaches `sc-agent-types` core only: no
`secret`, `obs`, `git` or `default` feature). There is no HTTP, git, process,
file or logging code and no I/O in `src`.

## Moved types

`judge`, `payload`, `test_util`, `QuestionId` and `ChoiceId` moved with
`git mv`; fields, constructors, accessors and serde attributes are unchanged.
`sc-agent-types` re-exports none of them. Errors stay `JevError` variants
(ADR-SAG-0001).

## Versioning rule

`DecisionSchemaId { family, version }` is written `"<family>.v<version>"`. A
reader accepts every id in `KNOWN` and refuses any other (`INPUT_INVALID` for
input documents, `RESPONSE_INVALID` for backend output) with a hint naming the
supported versions. Additive optional fields keep the version; a removed,
renamed or retyped field, or a changed meaning, bumps it. The version never
enters `PayloadBundle::bytes()`, so `prompt_sha256` is unchanged.

## Variant mechanism

`variants::schema_for(ModelFamily)` maps a model family to a schema; every
family maps to `canonical.v1` today. There is no trait until a second variant
exists. `llm::v1::parse_output` holds the one `match` on `SchemaFamily`; a new
family adds a `SchemaFamily` variant, a module in `variants` and one arm.

## Session-log schema

See [`session-log-schema.md`](session-log-schema.md) and
`crates/sc-decision-schema/src/session_log/schema.sql`. r-7 defines the DDL and
row types; r-6 emits them.
