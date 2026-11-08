# sc-jev — requirements

**Sprint** = the sprint that implements the requirement. The owning `req:`
acceptance root, when it differs, is shown as "root a-N"; the closure table in
[`plans/phase-a/plan.md`](../plans/phase-a/plan.md#requirement-closure-integration-req-roots) is authoritative.

| ID | Requirement | Sprint |
| --- | --- | --- |
| REQ-SAG-0110 | **`sc-jev` owns 100% of TypeSafe Jev HTTP**; Phase A check/judge slice | a-4+ |
| REQ-SAG-0400 | Injectable transport at construction; the judge body is built once and every retry resends it unchanged | a-4 |
| REQ-SAG-0401 | Jev busy retry policy | a-4 |
| REQ-SAG-0403 | Both backends validate `CanonicalJudgePayload` by deserializing; the Jev body is the documented flat `{model, state, questions}` object, no byte-splice | r-5 |
| REQ-SAG-0143 | `sc-jev` builds the body with `jev::v1::CheckRequest` and parses replies with `CheckResponse` (every question type, no byte-splice) | r-5 |

Public HTTP library; no argv parsing, no `check.toml`, no extractors.

## Phase A.1 (r-3)

Transport, busy retry (REQ-SAG-0505 as amended), caps and redaction moved to
[`sc-agent-http`](../sc-agent-http/requirements.md) (REQ-SAG-0402).
REQ-SAG-0400 is kept: the `sc_agent_http::Transport` is injected at
construction and a retry resends the same body.

## Phase A.1 (r-5)

REQ-SAG-0403 and REQ-SAG-0143: the body is the documented Jev object
`{model, state, questions}` built from the canonical payload by
`sc_decision_schema::jev::v1::CheckRequest` (all question types, no
byte-splice); replies parse through `CheckResponse`. `payload_sha256` stays on
the canonical bytes. `input_sha256` is recovered by inverting the body. REQ-SAG-0401 is now the shared
`BusyPolicy` with the packaged `attempts = 5` (4 at r-5, 5 since the
ADR-SAG-0007 phase-end amendment).
