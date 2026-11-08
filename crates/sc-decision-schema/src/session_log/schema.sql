-- schema_version 1. Authoritative.
-- Every column that can vary between two sends of one session or one
-- prompt lives on `answers`, so the loader's INSERT OR IGNORE loses nothing.
CREATE TABLE sessions (
  session_id          TEXT PRIMARY KEY,        -- ULID (r-6)
  session_created_at  TEXT NOT NULL            -- RFC 3339 UTC, ms; fixed per session
);
CREATE TABLE questions (
  payload_sha256    TEXT PRIMARY KEY,           -- SHA-256 of the canonical payload bytes
  question_ids     TEXT NOT NULL,              -- JSON array, payload order (fixed by the digest)
  first_seen_at    TEXT NOT NULL               -- loader: ts of the first row for this digest
);
CREATE TABLE answers (
  answer_id        TEXT PRIMARY KEY,           -- ULID, one per send
  schema_version   INTEGER NOT NULL,           -- SESSION_LOG_SCHEMA_VERSION of the row
  session_id       TEXT NOT NULL REFERENCES sessions(session_id),
  payload_sha256    TEXT NOT NULL REFERENCES questions(payload_sha256),
  decision_schema  TEXT NOT NULL,              -- DecisionSchemaId of this send, e.g. canonical.v1
  repo             TEXT NOT NULL,              -- main repo root, else calling folder (absolute)
  project          TEXT,                       -- caller text for this send; NULL when absent; not scrubbed
  request_sha256   TEXT,                       -- SHA-256 of the exact body sent; NULL when no body was built
  ts               TEXT NOT NULL,              -- RFC 3339 UTC, ms; when the send finished
  operation        TEXT NOT NULL CHECK (operation IN ('judge','ask')),
  backend          TEXT NOT NULL,              -- BackendId: how we called
  model_id         TEXT NOT NULL,              -- ModelId string: which model (never NULL)
  send_outcome     TEXT NOT NULL CHECK (send_outcome IN ('ok','failed')),
  error_code       TEXT,                       -- ErrorCode id; NOT NULL iff failed
  answers_json     TEXT,                       -- DecisionAnswers JSON; NOT NULL iff ok
  CHECK ((send_outcome = 'ok') = (error_code IS NULL)),
  CHECK ((send_outcome = 'ok') = (answers_json IS NOT NULL))
);
CREATE INDEX answers_by_question_model ON answers (payload_sha256, model_id);
CREATE INDEX answers_by_request        ON answers (request_sha256);
CREATE INDEX answers_by_repo_project   ON answers (repo, project);
