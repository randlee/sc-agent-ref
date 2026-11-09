# Plan QA (step 6) fix round 1, plan/phase-a-refine-r6 @ debbf1d

Reviewer JSON is in this directory: req-qa.json, arch-qa.json, ruthless-boundary-qa.json. Fix all 14 ids. Owner ceremony rule: add no gates, receipts or checks. Leave the D9 and sc-sha gates open.

- **ARCH-001..005 (Blocking):** add the missing `adrs` entries, each with a one-line reason.
  - r-6: ADR-SAG-0019 and ADR-SAG-0012.
  - r-7: ADR-SAG-0024, ADR-SAG-0012 and ADR-SAG-0006.
- **REQ-QA-001 (Blocking):** add NFR-SAG-0004 and REQ-SAG-0107 to the r-6 requirements. Give them as regression coverage the existing check-exact-pins.sh and cargo hack feature-powerset commands.
- **REQ-QA-002:** list REQ-SAG-0140 and REQ-SAG-0141 in r-5 as regressions, and REQ-SAG-0103 for Branch B. Use r-7's existing cargo tree and just lint commands.
- **REQ-QA-003:** change the REQ-SAG-0618 command to `--depth 1` (direct deps only). Reword "removed from the workspace" to "no workspace manifest lists them", consistently in REQ-SAG-0618, ADR-SAG-0031 and r-6 D2.
- **REQ-QA-004:** retitle plan.md "Why the critical path is 4" to "Why r-3b follows r-2b".
- **REQ-QA-005:** relabel r-7's REQ-SAG-0613 seam row as boundary:, not req:.
- **REQ-QA-006:** add owner item 5 to r-7's item list, in the plan.md sprint table and the r-7 intro.
- **RBQA-F001:** make the SendTrace a required parameter of the single exchange fn (send_and_parse / send_with_busy_retry) instead of an optional HttpRequest builder field. Each backend passes `request.send_trace().clone()`. Drop the Option branch in r-6 D3. Update the r-7 D8 code sample, the sc-agent-http facade and the ADR-SAG-0006 r-7 note to match. This also supersedes ARCH-005's "optional SendTrace" wording.
- **RBQA-F002:** move ProjectLabel into r-7 (core sc-agent-types). Type RowContext.project as Option<ProjectLabel>, so try_from_fields reuses the one validator. r-6 keeps the CLI, Python and compose wiring. Update the owned paths in both docs.
- **RBQA-F003:** add one sentence in r-7 D7 (and docs/sc-agent-types/architecture.md) saying ModelFamily is the only family schema_for consumes and is never derived from BackendFamily.
