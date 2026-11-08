# Session log storage schema (SQLite, authoritative)

The DDL is `crates/sc-decision-schema/src/session_log/schema.sql`
(`SESSION_LOG_DDL`, `SESSION_LOG_SCHEMA_VERSION = 2`). This document does not
copy it or its column comments.

## Loader rule

A loader reads the `fields{}` object of each v2 log line (the keys are
`session_log::COLUMNS`). It inserts into `sessions`, `question_sets` and
`payloads` with `INSERT OR IGNORE` (deriving `first_seen_at` from `ts`) and into
`answers` with `INSERT`. This is lossless because the deduplicated tables hold
only values fixed by their key (`session_created_at` by the session,
`question_ids` by the question set, `questions_sha256` by the payload bytes). `repo`, `project` and `decision_schema` are per send, on
`answers`, so two sends of one session with different `project` values both keep
theirs. The envelope `timestamp` is write time and the loader ignores it; `ts`
is the row's time.

## Migration notes

Adding or renaming a column, or changing a type or nullability, bumps
`SESSION_LOG_SCHEMA_VERSION` and adds a note here. `SessionLogRow::try_from_fields`
refuses any other `schema_version` (`INPUT_INVALID`; the hint says the row was
probably written by a newer sc-agent).

- Version 1: initial layout (r-7). Tables `sessions`, `questions` (keyed by the
  payload digest) and `answers`.
- Version 2 (phase-end digests): the digest column `prompt_sha256` is
  `payload_sha256`; `questions_sha256` is added to every row. The `questions`
  table, keyed by the payload digest, becomes `question_sets` (keyed by
  `questions_sha256`, holding `question_ids`) plus `payloads` (keyed by
  `payload_sha256`, holding its `questions_sha256`). `answers` carries
  `questions_sha256` and `payload_sha256` under a composite foreign key to
  `payloads`, and its index `answers_by_question_model` on `(prompt_sha256,
  model_id)` is replaced by `answers_by_questions_model` on `(questions_sha256,
  model_id)` and `answers_by_payload_model` on `(payload_sha256, model_id)`.
  Why: the old key was the payload digest, which includes the state, so the
  same questions asked against different state were unrelated rows and a
  question-set query was impossible. Version 1 rows are refused by
  `SessionLogRow::try_from_fields`; a version 1 log has no `questions_sha256` and
  cannot be migrated from its row alone (compute it from the questions).

## Queries

Same questions against different state, per model:

```sql
SELECT payload_sha256, model_id, send_outcome, answers_json
FROM answers WHERE questions_sha256 = ? ORDER BY ts;
```

Exact repeat of a request (a cached response can stand in for it):
`SELECT answers_json FROM answers WHERE request_sha256 = ? AND send_outcome = 'ok'`.
The `payloads` table lists each state seen for a question set.

## Notes

`RowContext.repo` is absolute path text; a non-UTF-8 byte is replaced, so two
such repos can share one `answers_by_repo_project` key. `project` is caller text
validated by `ProjectLabel` and not scrubbed. No secret, payload text, prompt
text or raw response body appears in any column.

`RowContext` is `#[non_exhaustive]`: its fields are public to read, and other
crates build one with `RowContextBuilder`. A new column still bumps the
schema version.
