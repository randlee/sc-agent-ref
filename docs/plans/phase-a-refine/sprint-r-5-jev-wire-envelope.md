---
id: r-5
phase: r
status: blocked
closure_type: boundary
target_boundary: jev
track: jev
wave: 6
size: M
branch: sprint/r-5-jev-wire-envelope
worktree: sprint/r-5-jev-wire-envelope
recommended_agent: sai
recommended_model: gpt-5.6-sol-medium
must_follow: [r-7, r-2b]   # r-3 and r-3b transitively through r-7
parallel_safe: [r-6]
vertical_rationale: "The Jev body builder (sc-jev), the payload type and its jev::v1 request/response structs (sc-decision-schema), and the validator and LLM wire parse that read the same payload (sc-jev-backends validate.rs, wire.rs) must change in one commit: deleting the splice, adopting CheckRequest/CheckResponse and switching both families to one deserializing validator cannot be split without a commit where the two backend families validate different schemas. One implementer; the sc-decision-schema additions are jev::v1 (both branches) and, in Branch B / both only, the optional criteria field."
requirements:
  - REQ-SAG-0403
  - REQ-SAG-0143   # moved from r-7: jev::v1 CheckRequest/CheckResponse, adopted in sc-jev; splice deleted
  - REQ-SAG-0142   # Branch B / both: criteria is an additive optional field, so the version stays canonical.v1
  - REQ-SAG-0400
  - REQ-SAG-0401
  - REQ-SAG-0500
  - REQ-SAG-0501
  - REQ-SAG-0102
  - REQ-SAG-0606
adrs:
  - ADR-SAG-0001  # validation errors stay JevError variants mapped by the CLI/PyO3
  - ADR-SAG-0002  # Branch B / "both": r-5 lands an ADR-SAG-0002 amendment (optional criteria field)
  - ADR-SAG-0004
  - ADR-SAG-0005
  - ADR-SAG-0006
  - ADR-SAG-0007
  - ADR-SAG-0012  # r-5 amendment (moved from r-7): Jev wire types are data in sc_decision_schema::jev; sc-jev still owns every operation
  - ADR-SAG-0019  # wording unchanged; the ADR-SAG-0005 r-5 text says so
  - ADR-SAG-0020  # r-3 amendment: single secret read site, redaction in send_with_busy_retry; the envelope builder adds no read site
  - ADR-SAG-0025
  - ADR-SAG-0026  # HTTP half: sc-jev builds HttpRequest and sends through sc-agent-http
  - ADR-SAG-0027  # error taxonomy: INPUT_INVALID via ErrorCode; private payload fields
  - ADR-SAG-0030  # r-7 (Active after wave 5): payload types and versioning rule; r-5 flips the "Jev API structs" bullet
  - ADR-SAG-0031  # wiring (r-6, same wave) touches no r-5 file; the trace hand-off in sc-jev is r-7's and is kept
owned_paths:
  - "crates/sc-jev/src/** (keeps r-7's request-trace hand-off and fn model_id lines)"
  - "crates/sc-jev/tests/** (r-7's send_trace.rs and catalog.rs stay green, not edited)"
  - "crates/sc-jev-backends/src/validate.rs"
  - "crates/sc-jev-backends/src/wire.rs"
  - "crates/sc-decision-schema/src/payload.rs"
  - "crates/sc-decision-schema/src/jev/** (new)"
  - "crates/sc-decision-schema/src/lib.rs#pub mod jev"
  - "crates/sc-decision-schema/tests/jev.rs (new)"
  - "crates/sc-decision-schema/tests/judge.rs#exactly-one-of questions/criteria (Branch B / both)"
  - "tests/fixtures/decision-schema/jev/** (new)"
  - "boundaries/sc-decision-schema/**#facade +sc_decision_schema::jev, composition root jev"
  - "docs/sc-decision-schema/**#jev::v1 section"
  - "docs/sc-jev/**"
  - "docs/architecture.md#ADR-SAG-0005 amendment (r-5)"
  - "docs/architecture.md#ADR index Amended-by marker (r-5) in ADR-SAG-0005"
  - "docs/architecture.md#ADR-SAG-0012 amendment (r-5) and its index marker"
  - "docs/architecture.md#ADR-SAG-0030 Jev API structs bullet marker; index row ADR-SAG-0030 (Status cell: r-5 part)"
  - "docs/architecture.md#ADR-SAG-0002 amendment (r-5) (Branch B or both only; inserted and flipped by r-5)"
  - "docs/architecture.md#ADR index row ADR-SAG-0002 (Amends cell: \"A.1 subsection (Active, r-5)\" marker; Branch B or both only)"
  - "docs/requirements.md#REQ-SAG-0403 and REQ-SAG-0143 rows"
---

# Sprint r-5 — Jev wire body as envelope; one canonical schema (D9) — BLOCKED

## Blocked on

Owner decision (c) in plan.md, which waits on the TypeSafe Jev API owner's
answer to:

> Does the TypeSafe Jev API accept `questions` as an **array of
> `{id, prompt, choices}`**, an **object of `{type, instructions, criteria}`**,
> or **both**? Which shape does production use today? Is the request body
> flat (`model` beside the payload fields) or nested (`{model, payload}`)?
> For the object form, what is one element of `criteria`?

This is an open owner item (plan.md "Owner decisions 2026-11-07" → "Open
owner items"). Record the answer (question shape, envelope shape, and for
Branch B or both the `criteria` element shape), with source and date, in
plan.md before setting `status: planned`. Wave 6: it follows r-7, because the payload types
live in `sc-decision-schema` (and, through r-3b, the `workspace-tests`
`jev_cross_impl` target its validation runs). It runs beside r-6
(`parallel_safe`) with disjoint files: r-6 edits nothing in `sc-jev`,
`sc-jev-backends` `validate.rs`/`wire.rs` or the `sc-decision-schema` files
r-5 owns; the request-trace hand-off and `fn model_id` in `sc-jev` are r-7's
and r-5 keeps them. **Publish gate:** no crate is published to crates.io
before this sprint lands (plan.md (c)).

## Outcome

`CanonicalJudgePayload` is the only schema both backend families validate,
by deserializing it. The Jev HTTP body is a JSON envelope built around the
canonical object instead of a byte-splice of `"model"`. `prompt_sha256` stays
on canonical bytes; `input_sha256` recovery (r-2b) is unchanged in method.

## Closes findings

ARCHC-002, ARCHC-003.

## Deliverables (common to both branches)

Every deliverable below, and the branch section that applies, lands production-ready for the scope this sprint claims; none closes as shape-only or test-only.

1. `sc-jev` and `sc-jev-backends` validate by
   `serde_json::from_slice::<CanonicalJudgePayload>(bundle.bytes())` with
   `deny_unknown_fields`. No other schema check remains.
2. **`jev::v1` (REQ-SAG-0143, moved from r-7).** `sc_decision_schema::jev::v1`
   has one serde request and one response type per Jev API call known today
   (check), signatures below. The Jev body is built by
   `CheckRequest::to_body` with `serde_json` as one **flat** object: `model`
   first, then the canonical payload's keys in canonical order, compact (the
   shape production receives today, so the body is byte-identical to the
   splice output). Flat is the default; if the recorded vendor answer says
   nested, `to_body` and the golden are rewritten to that shape before
   dispatch. `sc-jev` sends that body and parses replies
   through `CheckResponse`. `splice_model` and every byte-splice in `sc-jev`
   are deleted; there is no key-order refusal. A branch that changes the
   body's meaning uses r-7's versioning rule (ADR-SAG-0030): additive
   optional fields keep `canonical.v1`. Fixtures (new, owned here):
   `tests/fixtures/decision-schema/jev/check-request.json`, written by hand
   in the flat shape (rewritten if the vendor answer differs) for the
   cross-impl fixture payload and model
   `jev-1.13.0` (not generated by the code under test), and
   `check-response.json`, copied from the existing `sc-jev` test reply.
   `docs/sc-decision-schema/architecture.md` gains a `jev::v1` section.

   ```rust
   // crates/sc-decision-schema/src/jev/v1.rs
   #[derive(Debug, Clone, Copy)]
   pub struct CheckRequest<'a> { pub model: &'a ModelId, pub payload: &'a PayloadBundle }
   impl CheckRequest<'_> {
       pub fn to_body(&self) -> Result<Vec<u8>, JevError>;        // serde, no splice
   }
   #[derive(Debug, Clone, PartialEq, Deserialize)]
   pub struct CheckResponse {
       model: String,
       answers: DecisionAnswers,
       #[serde(flatten)]
       extra: serde_json::Map<String, serde_json::Value>,         // usage, stop_reason, ... verbatim
   }
   impl CheckResponse {
       pub fn model(&self) -> &str;
       pub fn answers(&self) -> &DecisionAnswers;
       pub fn extra(&self) -> &serde_json::Map<String, serde_json::Value>;
       pub fn into_answers(self) -> DecisionAnswers;
   }
   ```

   r-2b's recovery (parse the final body, remove `model` or take `payload`,
   re-encode, hash) is adapted to the new body; it stays attestation-only (ADR-SAG-0025 carve-out).
3. `test_envelope_robust_to_key_ordering` (`crates/sc-jev/tests/wire.rs`): a
   payload with keys `context`, `inputs`, `metadata` (sorting before `model`)
   produces a valid body.
4. Land the ADR-SAG-0005 r-5 amendment and REQ-SAG-0403. At flip time
   r-5 appends to the ADR-SAG-0005 r-5 bullet: "ADR-SAG-0019 is unchanged:
   the envelope is the 'API shape the evaluator expects' for Jev, and
   Haiku/Luna still embed the same canonical bytes. `input_sha256` recovery
   (ADR-SAG-0019/0025 r-2b) is unchanged in method." Plus the branch
   sentence below.
5. Errors (ADR-SAG-0001, ADR-SAG-0027): a payload that fails
   deserialization is `JevError` with `ErrorCode::InputInvalid` and a fix
   hint; no ad-hoc string. The envelope builder adds no secret read site
   and sends through `sc-agent-http` as r-3 left it (ADR-SAG-0020 r-3,
   ADR-SAG-0026 HTTP half).

## Branch A: array `{id, prompt, choices}` only

- `CanonicalJudgePayload.questions` stays as is. Delete the object-shape
  acceptance path in `sc-jev` and the tests that use it; switch those tests
  to the array fixture. `crates/sc-decision-schema/src/payload.rs` is not touched.
- ADR text: the ADR-SAG-0005 r-5 bullet gains "The object-shape
  (`{type, instructions, criteria}`) acceptance path in `sc-jev` is deleted;
  `questions` is an array of `{id, prompt, choices}` only." No ADR-SAG-0002
  text changes and no ADR-SAG-0002 marker is added.

## Branch B: object `{type, instructions, criteria}` only, or both

- Additive change only (r-1 made `CanonicalJudgePayload` fields private):
  `criteria: Option<CriteriaSpec>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`; validation
  requires exactly one of non-empty `questions` or `Some(criteria)`. An array
  payload serializes byte-identically (cross-impl fixture unchanged). No
  untagged enum, no new tag on existing payloads.

  ```rust
  // crates/sc-decision-schema/src/payload.rs (Branch B / both)
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct CriteriaSpec {
      #[serde(rename = "type")]
      kind: String,
      instructions: String,
      criteria: Vec<serde_json::Value>,   // element type: the criteria element shape recorded under "Blocked on"
  }
  ```
- `sc-jev-backends` accepts `criteria` only if the LLM system prompt asset
  supports it; otherwise `INPUT_INVALID` with a fix hint.
- "Both": as above, with both forms accepted by Jev.
- ADR text (Branch B and "both"): r-5 inserts and flips an ADR-SAG-0002
  amendment, and adds the marker "A.1 subsection (Active, r-5)" to the
  ADR-SAG-0002 index row Amends cell (an exception to "never adds an entry",
  like the plan review 4 exceptions in plan.md "ADR ownership"; the plan PR
  could not write it because the branch was unknown):

  > ### Amendment (Phase A.1, r-5)
  >
  > **Status:** Active (Phase A.1) — landed in r-5.
  >
  > - (Active, r-5) The canonical judge payload has an optional `criteria`
  >   field (`CriteriaSpec`, `{type, instructions, criteria}`), omitted when
  >   absent. A valid payload has exactly one of non-empty `questions` or
  >   `criteria`; anything else is `INPUT_INVALID`. A payload without
  >   `criteria` serializes byte-identically to the Phase A form, so
  >   `prompt_sha256` and the cross-language fixture are unchanged. There is
  >   one payload type (`CanonicalJudgePayload`), no untagged enum and no new
  >   tag.

  The ADR-SAG-0005 r-5 bullet names `CriteriaSpec` as part of the one
  schema.

## Acceptance criteria (authoritative; every command here is also run)

Each named test prints `1 passed`; the scrum-master rejects `0 passed`.

| Root | Criterion |
| --- | --- |
| req:REQ-SAG-0403 | Both backends deserialize `CanonicalJudgePayload`: `cargo test -p sc-jev --features test-util --test wire -- test_envelope_robust_to_key_ordering --exact`; `cargo test -p sc-jev-backends --features test-util --test wire` and `cargo test -p sc-jev-backends --test llm` green |
| req:REQ-SAG-0143 | `cargo test -p sc-decision-schema --test jev -- test_check_request_body_matches_golden --exact` (byte-equal to `check-request.json`, compact); `cargo test -p sc-decision-schema --test jev -- test_check_response_parses_golden --exact`; `cargo test -p sc-jev --features test-util --test wire -- test_jev_body_is_check_request_body --exact` (the body a recording transport captures equals `CheckRequest::to_body` for the same payload and model, so the splice path is gone); `cargo test -p sc-jev --test jev_api` green over the `sc-agent-http` transport |
| req:REQ-SAG-0142 (Branch B / both) | `cargo test -p sc-decision-schema --test version -- test_known_versions_accepted_unknown_rejected --exact` still passes with `KNOWN = [canonical.v1]` and `CURRENT = canonical.v1` (criteria is additive, no bump); `cargo test -p workspace-tests --test jev_cross_impl` and `cargo test -p sc-jev-check --test cross_impl_branches` digests unchanged for array payloads. Branch A: not applicable |
| adr:ADR-SAG-0005 | r-5 subsection Active, including the ADR-SAG-0019-unchanged sentence and the branch sentence (Branch A: object-shape path deleted) |
| adr:ADR-SAG-0012, ADR-SAG-0030 | ADR-SAG-0012 r-5 subsection and index marker Active; ADR-SAG-0030 "Jev API structs" bullet Active (r-5) and its index Status cell `Active (r-7, r-5)` |
| adr:ADR-SAG-0002 | Branch B or both: r-5 subsection Active as quoted above and index marker "A.1 subsection (Active, r-5)"; `cargo test -p sc-decision-schema --test judge -- test_payload_requires_exactly_one_of_questions_or_criteria --exact` rejects a payload with both `questions` and `criteria` and one with neither (`INPUT_INVALID`). Branch A: ADR-SAG-0002 text and row unchanged |
| adr:ADR-SAG-0001, 0020, 0026, 0027 | Validation failures map through `ErrorCode` (`cargo test -p sc-agent-types --test errors -- test_error_response_stderr_golden --exact`); `cargo test -p workspace-tests --test source_scans` green (no `disallowed_methods` expect outside `crates/sc-agent-http/src/ureq_transport.rs` and the ADR-SAG-0028 spawn sites) |
| boundary:jev | `cargo test -p sc-jev --features test-util --test wire -- test_digest_detects_faulty_wire_builder --exact` and `-- test_real_wire_builder_reports_prompt_digest --exact` green; regression: `cargo test -p sc-agent-http --test retry` (REQ-SAG-0401), `cargo test -p sc-jev --test send_trace` and `cargo test -p sc-jev --test catalog` (r-7's lines kept) |

## Required validation

Every command in the acceptance table, then:

```bash
just lint
just test
cargo test -p sc-jev-backends
```

## This sprint does not close

Non-check Jev API endpoints (REQ-SAG-0110).
