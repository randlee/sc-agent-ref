# Stability

Every crate in this workspace is 0.x (version 0.1.0, unreleased; see
[`CHANGELOG.md`](../CHANGELOG.md)). This page states what the repository has
already decided about each public contract, and marks what it has not. Where a
decision is missing it says so; it does not invent a promise.

## Crate APIs (Rust)

- `sc-jev-host` makes **no semver promise** of its own. It is published only so
  `sc-ai` can be installed; `sc-ai` and `sc-jev-py` pin it with an exact `=`
  requirement (ADR-SAG-0023, ADR-SAG-0026). Pin the exact version.
- `sc-decision-schema` makes **no promise beyond the schema versions it
  names**, plus one rule: `JudgeBackend` is open, new methods get default
  bodies, and the trait stays dyn-compatible.
- No decision about semver for the other crates exists in the ADRs. Treat them
  as 0.x with no promise until the owner decides (see below).

## Jev wire types: `sc_decision_schema::jev::v1`

`jev::v1` is the one versioned module for the TypeSafe Jev check API: the flat
request body `{model, state, questions}` (all question types and `state`
forms) and the `{model, answers, usage}` reply (ADR-SAG-0030). The repo keeps
one module per API shape and per version, and `v1` is the only version today.
It encodes the Jev service's documented API, so its shape is set by that
service.

### Models

sc-agent accepts `jev` or any supported version such as `jev-1.13.0`
(`ModelId`). `jev` means the latest Jev model. It follows the vendor's
releases, so an API change this build does not know can break it. That is
expected; the remedy is upgrading sc-agent. Pin a version such as
`jev-1.13.0` (`check.toml` `[models]`) to avoid it.

The wire value for `jev` is `jev-latest`, the value plan-anchor's `jev-api.md`
documents; `jev::v1::wire_model` is the one place that maps it. This is
**unverified against the live API**: nothing confirms the API accepts the bare
`jev`, and no test in this repository calls the live API.

A reply naming a different model than the request is not an error (`jev` is
answered by a concrete version). The answered model is the reply's `model`,
printed as `model` in `sc-ai jev run` output and read with
`JudgeOutcome::answered_model`. Output is byte-identical where the models match.

### Questions digest

Run output carries `questions_sha256`, the SHA-256 of the question set alone,
for a consumer's cache lookup (sc-agent builds no cache). Its exact bytes are
in [`tests/fixtures/README.md`](../tests/fixtures/README.md); a change to those
bytes changes every consumer's cache keys, so it is a breaking change. The key
is added after `prompt_sha256`; consumers that read keys by name are
unaffected.

### Numbers

Probabilities, confidence and combined statistics are `f64`. sc-agent never
rounds: values pass through unchanged, and a combined statistic is returned at
full double precision. Jev rounds what it sends, so the sum of a reply's
probabilities is checked within 0.01 per option (the plan-anchor rule), which
validates input and changes no value. Each probability and confidence must lie
in `[0, 1]`.

## Session-log record: `SESSION_LOG_SCHEMA_VERSION`

The record is the `fields` object of each log line (the keys are
`session_log::COLUMNS`), stored by the SQLite DDL in `sc-decision-schema`.
`SESSION_LOG_SCHEMA_VERSION` is `1`. Adding or renaming a column, or changing
a type or nullability, bumps the version and adds a migration note to
[`docs/sc-decision-schema/session-log-schema.md`](sc-decision-schema/session-log-schema.md).
`SessionLogRow::try_from_fields` refuses any other version with
`INPUT_INVALID`. Logs never contain API keys, prompt text or request bodies.
Session logs default to `<repo-root>/.sc-ai`; add that directory to your
`.gitignore`.

## Subjects schema: `sc/subjects/1`

The producer/consumer document emitted by `sc-treesitter` and read by
`sc-jev-check`. The schema id carries the version, and the normative rules
(1-based inclusive lines, half-open UTF-8 byte spans, subject ordering and id
form) are in
[`docs/sc-agent-types/subjects-schema.md`](sc-agent-types/subjects-schema.md).

## Error shape

Every `sc-ai` stderr line carries `schema_version` (`1`). Any added key, at the
top level or in `details`, is a breaking change and bumps it; a consumer that
sees another value treats the line as an unknown error shape and falls back to
the exit code (ADR-SAG-0027). Error ids come from a closed set shared with
Python (`tests/fixtures/jev/README-contract/errors.json`).

`INPUT_INVALID` is not only a malformed-input code. It also covers I/O
failures (`JevError::IoFailed`: a file or process operation failed, with the
`io::ErrorKind` in the message), as well as session-document and encode
failures. The code alone does not tell a bad input from a failed read, so a
consumer that must distinguish them reads the `message`. A missing file stays
`INPUT_MISSING`.

## Python: `sc_jev_py`

The wheel version moves in lockstep with the Rust crates (one tag). The
exported names and signatures are in `crates/sc-jev-py/sc_jev_py.pyi`; the
exceptions expose the same error codes as `JevError`. The release wheel has no
`_testing` submodule; it is test-only and not part of the API.

## Needs an owner decision

These are not decided in any ADR or README, so no promise is made:

1. The semver policy for 0.x crates other than `sc-jev-host` and
   `sc-decision-schema` (for example whether breaking changes only land in
   minor bumps).
2. Whether `jev::v1` is frozen (any change becomes `v2`) or may grow
   additively, and whether a Jev-side change is followed by a new version.
3. Whether `sc/subjects/1` may gain optional keys in place or any change needs
   `sc/subjects/2`.
4. Whether the Python `sc_jev_py` API (names, signatures, `CheckResult` and
   `JevError` shapes) has any compatibility promise before 1.0.
5. How long a deprecated spelling such as the backend alias
   `open_ai_luna` / `open_ai` is kept, and when it is removed (bead
   `agt-ho2`).
