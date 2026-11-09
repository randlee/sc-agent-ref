# sc-agent-types — requirements

Shared types for all sc-agent crates and future launchers.

**Sprint** = the sprint that implements the requirement. The owning `req:`
acceptance root, when it differs, is shown as "root a-N"; the closure table in
[`plans/phase-a/plan.md`](../plans/phase-a/plan.md#requirement-closure-integration-req-roots) is authoritative.

| ID | Requirement | Sprint |
| --- | --- | --- |
| REQ-SAG-0101 | `JevError` covers README error table | a-1 (root a-7) |
| REQ-SAG-0102 | Payload digest helpers for cross-impl fixture | a-1 (root a-8) |
| REQ-SAG-0103 | Frozen `JudgeBackend` / `JudgeRequest` / domain types + contract tests | a-1 |
| REQ-SAG-0104 | `sc/subjects/1` schema types + fixture | a-1 |
| NFR-SAG-0001 | `ScAgentErrorResponse` for CLI stderr JSON | a-1 |
| REQ-SAG-0120 | Temporary `observability::v2::SessionLog` port (`sc-observability-types` dep) | a-1 |
| REQ-SAG-0121 | Noop + test-util doubles; remove module when upstream v2 ships | a-1 |

Domain types: `Question`, `ChoiceAnswer`, `PayloadBundle`, `BackendId`,
`JudgeOutcome`, `QuestionAnswer`, `EqualInputRecord` (r-1 surface:
[`architecture.md`](architecture.md)).

Trait: `JudgeBackend` (see `architecture.md`).

## Phase A.1 amendments

Sprint r-1 flipped each row to `(Active, r-1)`; root rows
are in [`../requirements.md`](../requirements.md#phase-a1--refinement-landed).

| ID | Amendment |
| --- | --- |
| REQ-SAG-0101 | "One variant per id" becomes "every variant maps to exactly one id in `errors.json`; an id may have several variants" — (Active, r-1) |
| REQ-SAG-0103 | The a-1 frozen contract is unfrozen once by r-1 (types break) and re-frozen at r-1 merge; later changes are additive — (Active, r-1). Its home moves to `sc-decision-schema` (ADR-SAG-0030): `judge` takes the per-send `&SendTrace`, `model_id()` is added — (Active, r-7) |
| REQ-SAG-0105 | Open backend set: `#[non_exhaustive]` ids and static facts on `BackendDescriptor` rows in one `BACKENDS` catalog; `JudgeBackend::capabilities()` derives from the descriptor. Sprint r-1 (root table: [`../requirements.md`](../requirements.md)). Tests: `crates/sc-agent-types/tests/ids.rs` (`test_backend_catalog_roundtrip`); the production factory table is checked in `sc-jev-check` (`registry::tests::test_every_backend_has_factory`, `--lib`) — (Active, r-1) |
| REQ-SAG-0106 | Explicit `JevError` variants with unchanged `code()` ids, producer-neutral variants, `ProducerError`, `retryable()`, no detail-prefix parsing. Sprint r-1. Tests: `crates/sc-agent-types/tests/errors.rs` (`test_every_variant_golden`, `test_error_codes_match_errors_json`, `test_retryable_decisions`, `test_busy_exhaustion_machine_fields`, `test_message_never_holds_the_fix`, `test_from_error_response_round_trip`) and `error::tests::test_error_code_all_is_complete` (`src/error.rs`) — (Active, r-1) |
| REQ-SAG-0108 | `SubjectsDocument` is the one validated subjects type, built by the id-assigning `SubjectsBuilder`; producer and consumer share it. Sprint r-1. Tests: `crates/sc-agent-types/tests/subjects.rs` (`test_subjects_builder_ids_match_golden`, `test_unvalidated_subjects_rejected_on_deserialize`) — (Active, r-1) |
| REQ-SAG-0107 | `sc-agent-types` loses the `judge` and `test-util` features with the moved modules; features are `secret`, `obs` (default) and `git` (non-default); every combination builds — (Active, r-7) |
| REQ-SAG-0147 | Model catalog: open `ModelId` (known variants, including `Jev` = the latest Jev model, plus `Other(UnknownModel)`, which only parsing can build) — (Active, r-7) |
| REQ-SAG-0149 | `BackendChoice` / `BackendSet::pin`: a short name or a known model id of `jev`, `haiku`, `luna`; `BackendChoice::parse_in` / `BackendSet::parse_in` check ids against a `ModelAliases` catalog (`alias_of`, `known_models`), so models-file rows are pinnable; the other defined models are `INPUT_INVALID` — (Active, agt-akp) |
| REQ-SAG-0150 | `ModelAliases::with_file_model` (newest `released` wins; unknown alias, bad id or bad date is `CONFIG_INVALID`), `source`, `resolved`, `ModelSource` — (Active, agt-akp) |
| REQ-SAG-0152 | `tests/model_freshness.rs`, local only — (Active, agt-akp) |
| REQ-SAG-0148 | `ModelAliases::latest(alias)` for `jev`, `haiku`, `luna`; `with_override`; `BackendDescriptor::default_model_alias` / `default_model` — (Active, r-7) |
| REQ-SAG-0120 | The `observability::v2::SessionLog` port no longer depends on `sc-observability-types`; `emit` returns the local `LogRejected`; behind the default `obs` feature — (Active, r-1) |
| REQ-SAG-0121 | Noop and test-util doubles stay; the `sc-observability-types` dependency is removed; module deletion still waits on upstream v2 — (Active, r-1) |
| NFR-SAG-0001 | `ScAgentErrorResponse` field rule: keys `error`, `kind`, `message`, `suggested_action`, `retryable` (always), `retry_after_seconds`, `field`, `details`, `schema_version` (always `1`); `message` never holds the fix (ADR-SAG-0027, agt-2a3 amendment; supersedes the r-1 rule) — (Active, agt-2a3) |
| REQ-SAG-0612 | `SessionId` and `AnswerId` wrap `ulid::Ulid` (strict 26-character uppercase Crockford form); a UUID is `INPUT_INVALID` — (Active, r-6) |
| REQ-SAG-0618 | `Sha256Digest::of_text` goes through `sc-sha` (CRLF and bare CR normalized to LF); direct dependencies are `sc-sha`, `serde`, `serde_json`, `thiserror`, `ulid`; no `sha2` or `uuid` — (Active, r-6) |
| REQ-SAG-0611 | `observability::v2::LogEvent` is `LogEvent::new(session_id, operation, fields)` with `session_id()`, `operation()`, `fields()`; the `backend()` and `payload_sha256()` accessors are gone (the values are in `fields`) — (Active, r-6) |
| REQ-SAG-0614 | `git::discover_common_dir(start)` (feature `git`) runs `git rev-parse --path-format=absolute --git-common-dir` through the existing spawn site — (Active, r-6) |
| REQ-SAG-0160 | `ProviderFailure` and the bounded provider text (`bound_provider_text`, 1024 bytes per field, single-line) are in `sc-agent-types`; the classification is `sc_agent_http::api_failure` — (Active, agt-2a3) |
| REQ-SAG-0162 | `IO_FAILED` / `JevError::IoFailed { what, kind, recovery }`; `JevError::read_failed` maps "not found" to `INPUT_MISSING` and everything else to `IO_FAILED` — (Active, agt-2a3) |
| REQ-SAG-0163 | `INPUT_TOO_LARGE` carries `limit`, `actual`, `unit` — (Active, agt-2a3) |
| REQ-SAG-0164 | `ResponseInvalidCause` (`truncated`, `too_large`, `not_json`, `schema`) — (Active, agt-2a3) |
| REQ-SAG-0166 | `JevError::with_log_flush_failure` wraps an error with its flush failure as its own field — (Active, agt-2a3) |
| REQ-SAG-0631 | `observability::v2` holds `GeneralLog` (open, `Send + Sync`), `SharedGeneralLog`, `NoopGeneralLog`, `GeneralEvent`, `GeneralLevel` and the `RecordingGeneralLog` and `RejectingGeneralLog` test doubles, beside `SessionLog` (proof: doctests, `general_tests`) | agt-9eh |
| REQ-SAG-0634 | `GeneralEvent::with_error_code` records the typed id only; no helper takes error text | agt-9eh |
