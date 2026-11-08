# sc-ai — requirements

**Sprint** = the sprint that implements the requirement. The owning `req:`
acceptance root, when it differs, is shown as "root a-N"; the closure table in
[`plans/phase-a/plan.md`](../plans/phase-a/plan.md#requirement-closure-integration-req-roots) is authoritative.

| ID | Requirement | Sprint |
| --- | --- | --- |
| REQ-SAG-0700 | `sc-ai jev` subcommands, JSON envelope | a-7 |
| REQ-SAG-0701 | Typed stderr errors, exit 2: one `ScAgentErrorResponse` line per NFR-SAG-0001 (amended in r-1); a UUID `--session-id` is `INPUT_INVALID` with a ULID fix hint (r-6) | a-7, r-6 |
| REQ-SAG-0702 | Readback pairs | a-7 |
| REQ-SAG-0703 | Install documentation | a-7 |
| REQ-SAG-0710 | Integration test: versioned sc-ai-cli envelope; Jev answer + `session_id` in `data` | a-7 |
| REQ-SAG-0711 | `sc-ai` wires the logger through `sc_jev_host::obs` (the adapter and `sc-observability` live in `sc-jev-host`) | a-7, r-3b |
| REQ-SAG-0604 | Partial failure: envelope with the successful answers on stdout, the first failure's `ScAgentErrorResponse` on stderr, exit 2 | r-3b |
| REQ-SAG-0605 | `jev run --chain-context` selects chain context; independent is the default | r-3b |
| REQ-SAG-0712 | argv is parsed with clap derive behind the `cli` feature; error contract unchanged | r-3b |
| NFR-SAG-0002 | No secrets in output | a-7 |
| REQ-SAG-0615 | `jev run` / `jev ask` take `--project <TEXT>` (≤ 256 bytes, no control characters; stored as given, not scrubbed); `--session-id` is a ULID | r-6 |
| NFR-SAG-0003 | Document default log root, session files and the one log row per send (README "Session logs") | a-7, r-6 |

Phase A: `sc-ai jev` check parity only. Phase B: more Jev API + API-direct Claude/Codex (REQ-SAG-0900/0901); no vendor CLI wrappers in Phase A or B (a later agent kind, ADR-SAG-0030).
