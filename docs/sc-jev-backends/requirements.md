# sc-jev-backends — requirements

**Sprint** = the sprint that implements the requirement. The owning `req:`
acceptance root, when it differs, is shown as "root a-N"; the closure table in
[`plans/phase-a/plan.md`](../plans/phase-a/plan.md#requirement-closure-integration-req-roots) is authoritative.

| ID | Requirement | Sprint |
| --- | --- | --- |
| REQ-SAG-0500 | Haiku backend | a-5 |
| REQ-SAG-0501 | Luna backend + token count wire (feature 8); live probe optional | a-5 |
| REQ-SAG-0503 | Injectable system prompt (loaded by facade a-6); wire proof in a-5 under REQ-SAG-0500 | a-6 / a-5 (root a-7) |
| REQ-SAG-0504 | stop_reason / status → RESPONSE_INVALID | a-5 |
| REQ-SAG-0505 | LLM busy retry like Jev | a-5 |
| REQ-SAG-0502 | LLM oversize refusal | a-5 |

REQ-SAG-0400/0401 (Jev HTTP) belong to **`sc-jev`**, not this crate.

Each backend implements `JudgeBackend`.

## Phase A.1 (r-3)

The transport, busy retry (REQ-SAG-0505 as amended: absent or malformed
`Retry-After` backs off 1, 2, 4, 8 s), caps and redaction are
[`sc-agent-http`](../sc-agent-http/requirements.md) (REQ-SAG-0402). This crate
keeps the wire builders and reply parsers (ADR-SAG-0019).

## General log (agt-9eh, ADR-SAG-0013 amendment 2026-11-09)

| ID | Requirement | Sprint | Proof |
| --- | --- | --- | --- |
| REQ-SAG-0633 | `with_general_log` on both backends logs judge start, token count, end and failure by typed id, with the model | agt-9eh | `--test general_log` |
| REQ-SAG-0634 | The key, system prompt, question text and provider reply bodies never reach the general log | agt-9eh | `--test general_log` (`busy_wait_and_judge_are_logged_without_key_prompt_or_body`, `a_failure_is_logged_with_its_typed_id_only`) |
| REQ-SAG-0635 | A rejecting general log leaves the judged answers identical | agt-9eh | `--test general_log` (`a_rejecting_log_changes_nothing`) |
