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

Every `sc-ai` stderr line is one JSON object (ADR-SAG-0027, amended by bead
`agt-2a3`). Keys, in order:

| Key | Present | Meaning |
| --- | --- | --- |
| `error` | always | Stable id from the closed set in `tests/fixtures/jev/README-contract/errors.json`, shared with Python |
| `kind` | always | Broad class: `validation`, `not_found`, `invalid_state`, `dependency`, `permission`, `timeout`, `capacity` |
| `message` | always | What failed and why. Never the fix. Holds the provider's own status, type and message for provider errors |
| `suggested_action` | always | What to do next. Never the cause |
| `retryable` | always | `true` or `false`; advice to the caller, not a promise that sc-agent resends |
| `retry_after_seconds` | when known | The provider's `Retry-After` in whole seconds (rounded up, capped at one day), on `PROVIDER_BUSY` and on an `API_ERROR` that carried the header |
| `field` | when known | The failing flag, argument or provider-named request field |
| `details` | when the id has any | A closed object of machine fields (below) |
| `schema_version` | always | `1` |

The ids added before the first release are `AUTH_FAILED` (401, 403),
`API_REQUEST_INVALID` (400, 422), `QUOTA_EXHAUSTED` (out of tokens, credit or
spend allowance), `PROVIDER_BUSY` (rate limit or overload that outlasted the
busy retry), `API_ERROR` (every other failing status, 5xx included) and
`IO_FAILED` (a file, process, stream or dependency failed). `INPUT_TOO_LARGE`
now covers the token cap and a provider's own "too long" refusal as well as
the byte cap.

`details` keys, each present only when the error has it: `producer`,
`provider`, `http_status`, `provider_type`, `provider_code`,
`provider_request_id`, `cause`, `attempts`, `limit`, `actual`, `unit`, `what`,
`io_kind`, `network_kind`, `path`, `log_flush_failure`. By id:

| Id | `details` keys |
| --- | --- |
| `PROVIDER_BUSY` | `http_status`, `attempts` (sends made), `cause` (`sends` or `budget`) |
| `AUTH_FAILED`, `API_REQUEST_INVALID`, `QUOTA_EXHAUSTED`, `API_ERROR` | `provider`, `http_status`, `provider_type`, `provider_code`, `provider_request_id` (each when the provider sent it) |
| `INPUT_TOO_LARGE` | `unit` (`bytes` or `tokens`), `limit` and `actual` (absent when only the provider refused and gave no numbers), plus the provider keys above when the provider refused |
| `IO_FAILED` | `what`, `io_kind` (snake_case `std::io::ErrorKind`, such as `not_found`, `permission_denied`) |
| `NETWORK_ERROR` | `network_kind` (`timeout`, `connect`, `tls`, `cancelled`) |
| `RESPONSE_INVALID` | `cause` (`truncated`, `too_large`, `not_json`, `schema`) |
| `INPUT_MISSING`, `PROMPT_FILE_MISSING` | `path` |
| a producer's error | the producer's own `details`, plus `producer` |
| any, when the session log could not be flushed as the error left | `log_flush_failure` |

A consumer branches on `error` for the exact failure, on `kind` for a policy,
and on `retryable`; it never parses `message`. Retryability: `NETWORK_ERROR`
is retryable for `timeout` and `connect`, `PROVIDER_BUSY` always, `API_ERROR`
for status 500 and above (500, 503, 504 included: the provider failed on its
side and a later attempt can get past it, and Anthropic's guidance for 500 is
to retry with backoff), and a producer's error as the producer says. Nothing
else is. Only busy statuses (429, 502, 529) are resent automatically; a
non-busy failure is never resent by sc-agent, because a lost reply can mean
the provider already processed and billed the request.

Which id for which failure (one rule, `JevError` module docs): `INPUT_INVALID`
for what the caller handed in, command-line usage errors included;
`CONFIG_INVALID` for a configuration source (`check.toml`, `jev.toml`, the
backend registry, a credential's content, log setup a flag selects);
`INPUT_MISSING` for a named path that does not exist, whichever code path
reads it; `IO_FAILED` for the operating system or a dependency failing during
the work (a read that is not "not found", `git` that cannot start, stdin,
stdout, the working directory, the session store, a credential store that
cannot be opened). A consumer that must tell a bad input from a failed read
reads `error`, not `message`.

Provider error text is kept, bounded to 1024 bytes per field and single-line,
and passes through `redact` (the request's secret becomes `[redacted]`) before
it leaves. Prompt text and model output are never added; they appear only if
the provider's own error text repeats them, and then the same bound and
redaction apply (ADR-SAG-0020 amendment).

**Versioning.** `schema_version` is `1`. The first release has not shipped,
so the added keys, ids and the `message` / `suggested_action` split land in
version 1 instead of bumping it: no consumer has read an earlier shape. From
the first published release on, any added key at the top level or in
`details`, and any change to what a key means, is a breaking change and bumps
`schema_version`; a consumer that sees another value treats the line as an
unknown error shape and falls back to the exit code (ADR-SAG-0027). Adding an
id to the closed set is also breaking from that point.

## Python: `sc_jev_py`

The wheel version moves in lockstep with the Rust crates (one tag). The
exported names and signatures are in `crates/sc-jev-py/sc_jev_py.pyi`; the
`JevError` exception carries the same fields as the CLI error body (`code` is
the body's `error`; also `kind`, `message`, `suggested_action`, `retryable`,
`retry_after_seconds`, `field`, `details` as a dict or `None`, and
`schema_version`), built from the same serialization so the two cannot drift. The release wheel has no
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
