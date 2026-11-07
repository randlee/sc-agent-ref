---
id: r-5
phase: r
status: planned
closure_type: boundary
target_boundary: jev
track: jev
wave: 6
size: L
branch: sprint/r-5-jev-wire-envelope
worktree: sprint/r-5-jev-wire-envelope
recommended_agent: sai
recommended_model: gpt-5.6-sol-medium
must_follow: [r-7, r-2b]   # r-3 and r-3b transitively through r-7
parallel_safe: [r-6]
vertical_rationale: "The Jev body builder (sc-jev), the payload type and the jev::v1 request/response structs (sc-decision-schema), and the validator and LLM wire parse that read the same payload (sc-jev-backends validate.rs, wire.rs) must change in one commit: replacing the splice with the documented Jev body, adding the score and noul question kinds, and switching both families to one deserializing validator cannot be split without a commit where the two backend families accept different payloads. One implementer."
requirements:
  - REQ-SAG-0403
  - REQ-SAG-0143   # jev::v1: every documented question type, state form and answer, adopted in sc-jev; splice deleted
  - REQ-SAG-0142   # kind/descriptions are additive optional payload fields, so the version stays canonical.v1
  - REQ-SAG-0103   # the payload contract gains the optional kind and descriptions fields (additive)
  - REQ-SAG-0400
  - REQ-SAG-0401
  - REQ-SAG-0500
  - REQ-SAG-0501
  - REQ-SAG-0102
  - REQ-SAG-0606
  - REQ-SAG-0140   # regression: jev::v1 stays inside sc-decision-schema's data-only dependency set
  - REQ-SAG-0141   # regression: jev::v1 types live only in sc-decision-schema
adrs:
  - ADR-SAG-0001  # validation errors stay JevError variants mapped by the CLI/PyO3
  - ADR-SAG-0002  # r-5 amendment: optional kind/descriptions; payloads without them are byte-identical
  - ADR-SAG-0004
  - ADR-SAG-0005
  - ADR-SAG-0006
  - ADR-SAG-0007
  - ADR-SAG-0012  # r-5 amendment: Jev wire types are data in sc_decision_schema::jev; sc-jev still owns every operation
  - ADR-SAG-0019  # Haiku/Luna still embed the canonical bytes; the Jev body is the documented API shape
  - ADR-SAG-0020  # r-3 amendment: single secret read site, redaction in send_with_busy_retry; the body builder adds no read site
  - ADR-SAG-0025  # input_sha256 recovery inverts the documented body back to canonical bytes
  - ADR-SAG-0026  # HTTP half: sc-jev builds HttpRequest and sends through sc-agent-http
  - ADR-SAG-0027  # error taxonomy: INPUT_INVALID / RESPONSE_INVALID via ErrorCode; private payload fields
  - ADR-SAG-0030  # payload types and versioning rule (r-7); r-5 flips the "Jev API structs" bullet
  - ADR-SAG-0031  # wiring (r-6, same wave) touches no r-5 file; the trace hand-off in sc-jev is r-7's and is kept
owned_paths:
  - "crates/sc-jev/src/** (keeps r-7's request-trace hand-off and fn model_id lines)"
  - "crates/sc-jev/tests/** (r-7's catalog.rs stays green, not edited)"
  - "crates/sc-jev-backends/src/validate.rs"
  - "crates/sc-jev-backends/src/wire.rs"
  - "crates/sc-jev-backends/tests/**#score and noul cases"
  - "crates/sc-decision-schema/src/payload.rs"
  - "crates/sc-decision-schema/src/jev/** (new)"
  - "crates/sc-decision-schema/src/lib.rs#pub mod jev"
  - "crates/sc-decision-schema/tests/jev.rs (new)"
  - "crates/sc-decision-schema/tests/judge.rs#question kind rules"
  - "tests/fixtures/decision-schema/jev/** (new)"
  - "boundaries/sc-decision-schema/**#facade +sc_decision_schema::jev, composition root jev"
  - "docs/sc-decision-schema/**#jev::v1 section"
  - "docs/sc-jev/**"
  - "docs/architecture.md#ADR-SAG-0002, ADR-SAG-0005, ADR-SAG-0012 r-5 amendments and their index markers"
  - "docs/architecture.md#ADR-SAG-0030 Jev API structs bullet marker; index row ADR-SAG-0030 (Status cell: r-5 part)"
  - "docs/requirements.md#REQ-SAG-0403 and REQ-SAG-0143 rows"
---

# Sprint r-5 — Jev wire body as the documented API; one canonical schema (D9)

D9 is resolved by the owner (plan.md "(c)"): the source of truth is the
TypeSafe Jev API as documented in
`.claude/skills/plan-anchoring/references/jev-api.md` (`POST /v1/systemone`),
and sc-agent supports every documented question format, with the schemas in
`sc_decision_schema::jev::v1`. Wave 6: it follows r-7, because the payload
types live in `sc-decision-schema` (and, through r-3b, the
`workspace-tests` `jev_cross_impl` target its validation runs). It runs
beside r-6 (`parallel_safe`) with disjoint files: r-6 edits nothing in
`sc-jev`, `sc-jev-backends` `validate.rs`/`wire.rs` or the
`sc-decision-schema` files r-5 owns; the request-trace hand-off and
`fn model_id` in `sc-jev` are r-7's and r-5 keeps them. **Publish gate:** no
crate is published to crates.io before this sprint lands (plan.md (c)).

## Outcome

`CanonicalJudgePayload` is the only schema both backend families validate,
by deserializing it, and it expresses all three Jev question types. The Jev
HTTP body is the documented flat object `{model, state, questions}` built by
serde from the canonical payload; the byte-splice is gone. Replies parse
through the documented response. `prompt_sha256` stays on canonical bytes
(unchanged for every existing payload); `input_sha256` recovery inverts the
body back to canonical bytes.

## Closes findings

ARCHC-002, ARCHC-003.

## The documented Jev API (what `jev::v1` encodes)

- Request: flat `{model, state, questions}`. `questions` is an object keyed
  by question id; each question has `type` and `instructions`:
  - `choice`: `criteria` maps option to description, 2 to 255 options;
  - `score`: `criteria` maps 2 to 10 ordered levels to descriptions;
  - `noul`: no criteria; the answer is a yes-probability with no confidence.
- `state` is a string, an object or an array.
- Response: `{model, answers, usage}`. `answers` is keyed by question id:
  `choice` and `score` answers are `{type, choice, probabilities,
  confidence}`; a `noul` answer is `{type, probabilities}` with the single
  key `yes`. sc-agent validates as the reference does, keeping today's
  checks: the response `model` equals the request model; answer ids equal
  the question ids, with no duplicate id; `choice` is a criteria option; the
  `choice`/`score` probability keys equal the criteria keys and sum to 1
  within 1e-6; a `noul` answer has exactly the key `yes`; `confidence` and
  every probability lie in [0, 1]. Anything else is `RESPONSE_INVALID`.
- The reference shows only the `choice` request and answer in full; the
  `score` (`criteria` as an ordered level map) and `noul` (`probabilities`
  with `yes`) field layouts above are this plan's reading of it and are
  what the goldens encode. A live reply that differs is a defect fixed in
  `jev::v1` under the versioning rule (ADR-SAG-0030), not a planning gate.

## Deliverables

Every deliverable below lands production-ready for the scope this sprint claims; none closes as shape-only or test-only.

1. **One deserializing validator.** `sc-jev` and `sc-jev-backends` validate
   by `serde_json::from_slice::<CanonicalJudgePayload>(bundle.bytes())`
   with `deny_unknown_fields`. The question-kind rules of item 2 run inside
   that deserialization (`Question` is `#[serde(try_from = "RawQuestion")]`
   in `sc-decision-schema`), so both crates call one validator and neither
   repeats the rules. No other schema check remains; the raw object-shape acceptance path in
   `sc-jev` (`criteria_from_object`) and its tests are deleted (callers
   express every type through the canonical payload).
2. **Canonical payload: question kinds (ADR-SAG-0002 r-5 amendment).**
   `Question` gains `kind: QuestionKind` (`Choice` | `Score` | `Noul`,
   `#[serde(default, skip_serializing_if = "QuestionKind::is_choice")]`, so
   a choice question writes no `kind` key) and `descriptions:
   Option<Vec<String>>` (`skip_serializing_if = "Option::is_none"`; one per
   choice; absent means each description is the choice label). The rules
   are checked by the one validating constructor, `Question::try_new(id,
   prompt, choices, kind, descriptions)`, which `TryFrom<RawQuestion>`
   (deserialization) also calls; `Question::new` (choice) stays for
   existing callers (`INPUT_INVALID` with a fix hint otherwise): `choice` has 2 to 255 choices; `score` has 2 to 10
   choices, in level order; `noul` has no choices and no descriptions;
   `descriptions`, when present, has exactly one entry per choice. A
   payload without the new fields serializes byte-identically, so
   `prompt_sha256` and the cross-impl fixture are unchanged and the
   version stays `canonical.v1` (additive optional fields).
3. **`jev::v1` (REQ-SAG-0143).** Types below. `CheckRequest::from_canonical(
   model, &payload)` maps each canonical question to its Jev question:
   `instructions` = `prompt`; `criteria` = choices to descriptions, in
   choice order; `questions` keys in payload order. Canonical `state` (the
   prior answers) is sent as the `state` array. `JevQuestion` holds the
   criteria counts (choice 2..=255, score 2..=10) through one validating
   constructor that both `from_canonical` and its `TryFrom` deserialization
   call; `OrderedMap` rejects a duplicate key (`INPUT_INVALID` on a request
   or recovered body, `RESPONSE_INVALID` on a reply, fix hint naming the
   key). `CheckRequest::to_body() -> Result<String, JevError>` writes
   compact JSON: `model`, `state`, `questions`, in that order. Before
   sending, `sc-jev` compares the body length to the Jev descriptor's
   `request_limit_bytes` and returns `InputTooLarge` (limit and actual
   bytes) as today.
   `sc-jev` sends that body and parses replies with `CheckResponse`, whose
   `into_answers` checks the rules above (model equality and probability
   key sets included) and the answer type against the question type. Answers become `DecisionAnswers`: `choice`/`score` answers keep
   `choice`, `probabilities` and `confidence`; a `noul` answer becomes
   `choice` `"yes"` when `p >= 0.5` else `"no"`, `probabilities`
   `{yes: p, no: 1 - p}`, no `confidence`. `splice_model` and every
   byte-splice in `sc-jev` are deleted.
4. **Fixtures (new, owned here).** Written by hand from the reference, not
   generated by the code under test:
   `tests/fixtures/decision-schema/jev/check-request.json` (the cross-impl
   fixture payload, choice questions, model `jev-1.13.0`),
   `check-request-all-types.json` (one `choice`, one `score`, one `noul`),
   `check-response.json` and `check-response-all-types.json` (the matching
   replies, `usage` included). `docs/sc-decision-schema/architecture.md`
   gains a `jev::v1` section.
5. **`input_sha256` recovery** (r-2b, ADR-SAG-0025) inverts the final body:
   parse it with `CheckRequest`'s order-preserving maps, map each Jev
   question back (`prompt` = `instructions`, choices = `criteria` keys in
   body order, `kind` from `type`, `descriptions` omitted when every
   description equals its label), canonical-encode, hash. It stays
   attestation-only; for a payload built by `from_canonical` the recovered
   digest equals `prompt_sha256`.
6. **Haiku/Luna.** `score` questions go through the same LLM path as
   `choice` (levels are the choices, descriptions included in the embedded
   canonical bytes). A `noul` question sent to Haiku or Luna is refused in
   `validate_for_send` with `INPUT_INVALID` ("Haiku and Luna answer choice
   and score questions; send noul questions to jev") before any send.
7. `test_envelope_robust_to_key_ordering` (`crates/sc-jev/tests/wire.rs`):
   a request whose question ids and criteria keys are unsorted (`zeta`,
   `alpha`, `mid`) produces a valid body that keeps that order.
8. Land the ADR-SAG-0002, ADR-SAG-0005 and ADR-SAG-0012 r-5 amendments, the
   ADR-SAG-0030 "Jev API structs" bullet, REQ-SAG-0403 and REQ-SAG-0143.
   At flip time r-5 appends to the ADR-SAG-0005 r-5 bullet: "ADR-SAG-0019
   is unchanged: the documented Jev body is the 'API shape the evaluator
   expects' for Jev, and Haiku/Luna still embed the same canonical bytes."
9. Errors (ADR-SAG-0001, ADR-SAG-0027): payload failures are `JevError`
   with `ErrorCode::InputInvalid` and a fix hint; reply failures
   `ErrorCode::ResponseInvalid`; no ad-hoc string. The body builder adds no
   secret read site and sends through `sc-agent-http` as r-3 left it.

## Explicit code samples (signatures are binding; bodies are not)

```rust
// crates/sc-decision-schema/src/payload.rs — additions
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind { #[default] Choice, Score, Noul }
impl QuestionKind { pub fn is_choice(&self) -> bool; }
// Question gains (private):
//     #[serde(default, skip_serializing_if = "QuestionKind::is_choice")] kind: QuestionKind,
//     #[serde(default, skip_serializing_if = "Option::is_none")] descriptions: Option<Vec<String>>,
// and is #[serde(try_from = "RawQuestion")], so deserialization runs try_new.
impl Question {
    pub fn try_new(id: QuestionId, prompt: impl Into<String>, choices: Vec<ChoiceId>,
                   kind: QuestionKind, descriptions: Option<Vec<String>>) -> Result<Self, JevError>;
    pub fn kind(&self) -> QuestionKind;
    pub fn descriptions(&self) -> Option<&[String]>;
}

// crates/sc-decision-schema/src/jev/v1.rs
/// Map serialized as a JSON object whose key order is the Vec order, and
/// deserialized in document order (custom visitor; no serde_json feature).
/// A duplicate key is an error that names the key.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedMap<K, V>(Vec<(K, V)>);

/// Built only through one validating constructor (criteria counts), used by
/// `from_canonical` and by `#[serde(try_from = "RawJevQuestion")]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum JevQuestion {
    Choice { instructions: String, criteria: OrderedMap<ChoiceId, String> }, // 2..=255
    Score  { instructions: String, criteria: OrderedMap<ChoiceId, String> }, // 2..=10, level order
    Noul   { instructions: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JevState { Text(String), Object(OrderedMap<String, serde_json::Value>), Array(Vec<serde_json::Value>) }
// top-level object keys keep their order; nested Values are serde_json values
// (their object keys are sorted), so only the top level round-trips verbatim

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRequest {
    model: ModelId,
    state: JevState,
    questions: OrderedMap<QuestionId, JevQuestion>,
}
impl CheckRequest {
    pub fn from_canonical(model: ModelId, payload: &CanonicalJudgePayload) -> Result<Self, JevError>;
    pub fn to_body(&self) -> Result<String, JevError>;            // compact; model, state, questions
    pub fn to_canonical(&self) -> Result<CanonicalJudgePayload, JevError>; // inverse, for input_sha256
    pub fn state(&self) -> &JevState;
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JevAnswer {
    Choice { choice: ChoiceId, probabilities: BTreeMap<ChoiceId, f64>, confidence: f64 },
    Score  { choice: ChoiceId, probabilities: BTreeMap<ChoiceId, f64>, confidence: f64 },
    Noul   { probabilities: NoulProbabilities },
}
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoulProbabilities { pub yes: f64 }

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CheckResponse {
    model: ModelId,
    answers: OrderedMap<QuestionId, JevAnswer>,                  // duplicate id -> RESPONSE_INVALID
    usage: serde_json::Map<String, serde_json::Value>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,          // stop_reason, ... (keys sorted)
}
impl CheckResponse {
    pub fn model(&self) -> &ModelId;
    pub fn usage(&self) -> &serde_json::Map<String, serde_json::Value>;
    pub fn extra(&self) -> &serde_json::Map<String, serde_json::Value>;
    /// Checks the reference's rules against `request` and maps to DecisionAnswers.
    pub fn into_answers(self, request: &CheckRequest) -> Result<DecisionAnswers, JevError>; // RESPONSE_INVALID
}
```

## Acceptance criteria (authoritative; every command here is also run)

Each named test prints `1 passed`; the scrum-master rejects `0 passed`.

| Root | Criterion |
| --- | --- |
| req:REQ-SAG-0403 | Both backends deserialize `CanonicalJudgePayload`: `cargo test -p sc-jev --features test-util --test wire -- test_envelope_robust_to_key_ordering --exact`; `cargo test -p sc-jev-backends --features test-util --test wire` and `cargo test -p sc-jev-backends --test llm` green; `cargo test -p sc-jev-backends --test llm -- test_noul_question_refused_before_send --exact` |
| req:REQ-SAG-0143 | `cargo test -p sc-decision-schema --test jev -- test_check_request_body_matches_golden --exact` and `-- test_all_question_types_body_matches_golden --exact` (byte-equal to the compact form of `check-request.json` / `check-request-all-types.json`); `cargo test -p sc-decision-schema --test jev -- test_check_response_parses_golden --exact` and `-- test_all_answer_types_parse_golden --exact`; `cargo test -p sc-decision-schema --test jev -- test_response_rules_rejected --exact` (model differing from the request model, id mismatch, duplicate id, choice outside criteria, probability keys differing from the criteria keys, value outside [0, 1], sum off by more than 1e-6, a `noul` answer with a key other than `yes` or a `yes` outside [0, 1], answer type differing from question type: each `RESPONSE_INVALID`); `cargo test -p sc-decision-schema --test jev -- test_state_forms_roundtrip --exact` (string, object with unsorted top-level keys, array); `cargo test -p sc-decision-schema --test jev -- test_duplicate_key_rejected --exact` (duplicate question id or criteria key on a request and on recovery: `INPUT_INVALID`); `cargo test -p sc-jev --features test-util --test wire -- test_body_over_limit_is_input_too_large --exact`; `cargo test -p sc-jev --features test-util --test wire -- test_jev_body_is_check_request_body --exact` (the body a recording transport captures equals `CheckRequest::to_body`, so the splice path is gone); `cargo test -p sc-jev --test jev_api` green over the `sc-agent-http` transport |
| req:REQ-SAG-0142 | `cargo test -p sc-decision-schema --test version -- test_known_versions_accepted_unknown_rejected --exact` still passes with `KNOWN = [canonical.v1]`; `cargo test -p workspace-tests --test jev_cross_impl` and `cargo test -p sc-jev-check --test cross_impl_branches` digests unchanged |
| req:REQ-SAG-0103 | `cargo test -p sc-decision-schema --test judge -- test_question_kind_rules --exact` (choice 2..=255, score 2..=10, noul without choices, descriptions one per choice; each violation `INPUT_INVALID`); `cargo test -p sc-decision-schema --test judge` green |
| boundary:jev | `cargo test -p sc-jev --features test-util --test wire -- test_digest_detects_faulty_wire_builder --exact` and `-- test_real_wire_builder_reports_prompt_digest --exact` green (recovery via `to_canonical`); regression: `just test`, `cargo test -p sc-agent-http --test retry` (REQ-SAG-0401), `cargo test -p sc-jev --test catalog` (r-7's lines kept), `cargo test -p sc-agent-types --test errors -- test_error_response_stderr_golden --exact` (ADR-SAG-0001/0027); REQ-SAG-0140 `cargo tree -p sc-decision-schema -e normal --depth 1` lists only `sc-agent-types`, `serde`, `serde_json`; REQ-SAG-0140/0141 `just lint` |
| adr (r-5) | No `(Proposed, r-5)` marker remains in `docs/architecture.md` |

## Required validation

Every command in the acceptance table, then:

```bash
just lint
just test
cargo test -p sc-jev-backends
```

## This sprint does not close

- Non-check Jev API endpoints (REQ-SAG-0110).
- `noul` questions on Haiku/Luna (refused before send).
- Canonical payloads with string or object `state` (the canonical `state`
  stays the prior-answers array; `jev::v1` encodes all three forms).
