# sc-decision-schema — requirements

Data-only decision schema crate (ADR-SAG-0030, ADR-SAG-0031 storage schema).
Root rows live in [`../requirements.md`](../requirements.md#phase-a1--refinement-proposed).

| ID | Requirement | Sprint |
| --- | --- | --- |
| REQ-SAG-0140 | Published, data-only crate above `sc-agent-types`: dependencies `sc-agent-types` (core only), `serde`, `serde_json`; no I/O | r-7 |
| REQ-SAG-0141 | Decision types move (not copy) here from `sc-agent-types`, with no re-export | r-7 |
| REQ-SAG-0142 | `DecisionSchemaId` versions (`canonical.v1`); readers accept `KNOWN` and refuse others; the version is outside the hashed bytes | r-7 |
| REQ-SAG-0143 | `jev::v1`: one serde request and response type per Jev API call known today (`CheckRequest`, `CheckResponse`); every documented question type (`choice`, `score`, `noul`), state form (string, object, array) and answer; data only, `sc-jev` owns every operation | r-5 |
| REQ-SAG-0144 | Haiku/Luna on the same schema: canonical bytes in, `DecisionAnswers` out (`llm::v1`) | r-7 |
| REQ-SAG-0145 | `schema_for(ModelFamily)`, static dispatch keyed by `SchemaFamily`; canonical variant is the identity | r-7 |
| REQ-SAG-0146 | Authoritative SQLite DDL (`session_log/schema.sql`) and the `fields{}` projection with `SessionLogRow` | r-7 |
| REQ-SAG-0103 | The frozen `JudgeBackend` / `JudgeRequest` contract and its contract tests live here; `judge` takes `&SendTrace`; `model_id()` is a provided method; `Question` gains the additive optional `kind` and `descriptions` fields (ADR-SAG-0002 r-5; payloads without them are byte-identical) | r-7, r-5 |
