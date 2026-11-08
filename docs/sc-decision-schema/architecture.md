# sc-decision-schema — architecture

**Depends on:** `sc-agent-types` (`default-features = false`, core only),
`serde`, `serde_json`. Boundary: `boundaries/sc-decision-schema/decision-schema.toml`.

## Layering

```text
sc-agent-types        ids, JevError/ErrorCode, BackendId + BACKENDS, ModelId/ModelAliases,
  (below)             Sha256Digest, CancelToken, SendTrace, ProjectLabel, AnswerId, ...
sc-decision-schema    QuestionId, ChoiceId, Question, QuestionKind, ChoiceAnswer, CanonicalJudgePayload,
  (above)             PayloadBundle, QuestionAnswer, DecisionAnswers, JudgeOutcome,
                      JudgeRequest, LlmWireContext, JudgeBackend, BackendCapabilities,
                      Attestation, BackendAttestation, EqualInputRecord, MultiBackendMode,
                      DecisionSchemaId, jev::v1, llm::v1, variants, session_log, test_util
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

## Question kinds (ADR-SAG-0002, r-5)

`Question` has a `kind` (`QuestionKind::{Choice, Score, Noul}`) and optional
`descriptions`. `Question::try_new` is the one validating constructor and
deserialization runs it (`#[serde(try_from)]`): `choice` has 2 to 255 distinct
choices, `score` 2 to 10 in level order, `noul` none; `descriptions`, when
present, has one entry per choice, and entries equal to the labels are
dropped. A choice question writes no `kind` and a question without
descriptions writes none, so a Phase A payload keeps its bytes and
`prompt_sha256`; the version stays `canonical.v1`. `Question::new_unchecked` (tests only: `cfg(test)` or feature `test-util`) skips the rules. `CanonicalJudgePayload::from_bytes` is the one
validator both backend families call. A `noul` question's bundle choice set is
`{no, yes}`, so its derived answer (`yes` when `p >= 0.5`) passes
`JudgeOutcome::new`.

## `jev::v1` (REQ-SAG-0143)

The documented TypeSafe Jev check API (`POST /v1/systemone`) as data:

| Type | Role |
| --- | --- |
| `CheckRequest` | `{model, state, questions}`; `from_canonical` maps a payload (`prompt` to `instructions`, choices and descriptions to `criteria`, payload order kept), `to_body` writes compact JSON, `parse` and `to_canonical` invert it for the `input_sha256` recovery |
| `JevQuestion` | `choice` (2 to 255 criteria), `score` (2 to 10 ordered levels) or `noul` (no criteria); the counts are checked by one constructor that deserialization also runs |
| `JevState` | the `state`: string, object or array; top-level object keys keep their order, nested values are `serde_json` values (sorted keys) |
| `OrderedMap` | a JSON object that keeps key order both ways and refuses a duplicate key |
| `Probability` | a finite number in `[0, 1]` (private field, `new`/`get`); checked in `new` and `Deserialize`, serialized as the plain number. Every `probabilities` value and every `confidence` of a `JevAnswer` is one (the reference states `confidence` in `[0, 1]`), so an out-of-range reply is `RESPONSE_INVALID` when parsed |
| `NoulProbabilities` | the `noul` answer's `{yes}`; private field, `new`/`yes()` |
| `CheckResponse`, `JevAnswer` | `{model, answers, usage}`; `into_answers` checks the reply against the request: same model, answer ids equal question ids, answer type equals question type, `choice` in criteria, probability keys equal criteria keys and sum to 1 within 1e-6, every value in `[0, 1]`; a `noul` answer has exactly `yes` and becomes `yes` or `no` with `{yes: p, no: 1 - p}` and no confidence |

Violations of request rules are `INPUT_INVALID`, of reply rules
`RESPONSE_INVALID`. The module holds no HTTP, key or I/O; `sc-jev` sends the
body and maps the reply. The `score` and `noul` field layouts are the plan's
reading of the reference (which shows only `choice` in full); a live reply
that differs is fixed in `jev::v1` under the versioning rule. Goldens:
`tests/fixtures/decision-schema/jev/` (`check-request.json`,
`check-request-all-types.json`, `check-response.json`,
`check-response-all-types.json`), written by hand from the reference.

## Session-log schema

See [`session-log-schema.md`](session-log-schema.md) and
`crates/sc-decision-schema/src/session_log/schema.sql`. r-7 defines the DDL and
row types; r-6 emits them.
