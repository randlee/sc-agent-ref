# sc-agent-secrets — requirements

**Sprint** = the sprint that implements the requirement. The owning `req:`
acceptance root, when it differs, is shown as "root a-N"; the closure table in
[`plans/phase-a/plan.md`](../plans/phase-a/plan.md#requirement-closure-integration-req-roots) is authoritative.

| ID | Requirement | Sprint |
| --- | --- | --- |
| REQ-SAG-0200 | `resolve_key(id, repo_root)` env → `.env` → keyring | a-2 |
| REQ-SAG-0201 | `ANTHROPIC_KEYCHAIN_SERVICE` optional override | a-2 |
| REQ-SAG-0202 | Repo-root `.env`; process env wins | a-2 |

Missing key → `KEY_MISSING` with fix text from `sc-agent-types`.

## General log (agt-9eh, ADR-SAG-0013 amendment 2026-11-09)

| ID | Requirement | Sprint | Proof |
| --- | --- | --- | --- |
| REQ-SAG-0633 | `resolve_key_logged` and `resolve_key_with_logged` log the credential source (`env`, `dotenv` or `keyring`) and key id on success and the typed id on failure; never the value | agt-9eh | `sc-agent-secrets` unit tests (`resolve::tests`) |
