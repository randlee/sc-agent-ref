# Session log storage schema (SQLite, authoritative)

The DDL is `crates/sc-decision-schema/src/session_log/schema.sql`
(`SESSION_LOG_DDL`, `SESSION_LOG_SCHEMA_VERSION = 1`). This document does not
copy it or its column comments.

## Loader rule

A loader reads the `fields{}` object of each v2 log line (the keys are
`session_log::COLUMNS`). It inserts into `sessions` and `questions` with
`INSERT OR IGNORE` (deriving `first_seen_at` from `ts`) and into `answers` with
`INSERT`. This is lossless because the two deduplicated tables hold only values
fixed by their key (`session_created_at` by the session, `question_ids` by the
canonical bytes). `repo`, `project` and `decision_schema` are per send, on
`answers`, so two sends of one session with different `project` values both keep
theirs. The envelope `timestamp` is write time and the loader ignores it; `ts`
is the row's time.

## Migration notes

Adding or renaming a column, or changing a type or nullability, bumps
`SESSION_LOG_SCHEMA_VERSION` and adds a note here. `SessionLogRow::try_from_fields`
refuses any other `schema_version` (`INPUT_INVALID`; the hint says the row was
probably written by a newer sc-agent).

- Version 1: initial layout (r-7).

## Notes

`RowContext.repo` is absolute path text; a non-UTF-8 byte is replaced, so two
such repos can share one `answers_by_repo_project` key. `project` is caller text
validated by `ProjectLabel` and not scrubbed. No secret, payload text, prompt
text or raw response body appears in any column.

`RowContext` is `#[non_exhaustive]`: its fields are public to read, and other
crates build one with `RowContextBuilder`. A new column still bumps the
schema version.
