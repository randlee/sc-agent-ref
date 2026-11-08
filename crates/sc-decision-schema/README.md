# sc-decision-schema

The data-only decision schema for the sc-agent workspace: the question and
answer payload (`PayloadBundle`, `QuestionAnswer`, `DecisionAnswers`), the
`JudgeBackend` contract whose signatures name them, the Haiku/Luna input and
output mapping (`llm::v1`), the TypeSafe Jev check request and reply types
(`jev::v1`), per-model-family variants (`variants`), the schema
version id (`DecisionSchemaId`, e.g. `canonical.v1`) and the session-log SQLite
schema with its `fields{}` projection (`session_log`).

Depends on `sc-agent-types` (core only), `serde` and `serde_json`; it does no
I/O. Non-default feature `test-util` enables `test_util::MockJudgeBackend`.

**Semver:** no promise beyond the schema versions it names, plus one rule:
`JudgeBackend` is open, and new methods get default bodies. The trait stays
dyn-compatible (`&self` receivers, no generic methods, no `Self` in
signatures).

Stability: all crates are 0.x; see the [stability statement](https://github.com/randlee/sc-agent/blob/develop/docs/stability.md).

Part of the [sc-agent](https://github.com/randlee/sc-agent) workspace. See the
repository `docs/` for architecture and requirements. Licensed under MIT.
