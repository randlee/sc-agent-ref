# Phase-end fix-review findings: rounds not previously posted

Round 1 is in the findings report on this PR. These rounds ran on the fix branches and should have been posted here as each finished. Fixes merged through #37 and #38. Final report for those rounds: https://github.com/randlee/sc-agent/pull/38#issuecomment-6726548622

## Round 2, at f4f3543
| ID | Severity | Finding | State |
|---|---|---|---|
| RBP-F001 | Important | `NoulProbabilities.yes` raw public `f64` with an unenforced range | Closed, c35d9e2 |
| RBP-F002 | Important | `SubjectsProducer`, `SubjectsSource` public fields; raw `mode` | Closed, 96e6970 |
| RBP-F003 | Minor | `Rev::new` public and unvalidated | Closed, 96e6970 |
| RBP-F004 | Minor | `RowContext` public fields, not `#[non_exhaustive]` | Closed, 96e6970 |
| REQ-QA-001 | Important | `docs/sc-jev-host/requirements.md:12` cites a test that doesn't exist | Closed, 1503350 |
| REQ-QA-002 | Blocking | `project-plan.md` and Phase A close notes contradict the landed A.1 plan | Closed, c35d9e2 |
| REQ-QA-003 | Important | Row 207 publish order stale | Closed, c35d9e2 |
| REQ-QA-004 | Important | Sprint `requirements:` lists follow plan rule rather than reviewer rule | Closed by repository exception |
| REQ-QA-005 | Minor | ADR-SAG-0025 index marker | Closed, 1503350 |
| REQ-QA-006 | Blocking | No execution evidence | Closed by rust-qa at bd00bc3 and CI |
| QA-001 | Minor | `--request-json <file>` untested; CI coverage skipped the cli target | Closed, 1503350 |

## Round 3, at 9d06700
| ID | Severity | Finding | State |
|---|---|---|---|
| RBP-F101 | Important | `QuestionAnswer` raw `f64`, duplicate range check | Closed, c35d9e2 |
| RBP-F102 | Important | `ExtractInput` public fields, late validation | Closed, c35d9e2 |
| RBP-F103 | Important | `InputSpec` closed; hand-built `CheckSpec` skips name validation | Closed, c35d9e2 |
| RBP-F104 | Minor | `JevAnswer` variants not `#[non_exhaustive]` | Closed, c35d9e2 |
| RBP-F105 | Minor | Dead error path in `ask` | Closed, c35d9e2 |
| RBP-F106 | Minor | `SubjectsProducer::new` positional strings | Closed, c35d9e2 |
| REQ-QA-003 | Important | Row 207 cited the wrong ADR | Closed, c35d9e2 |
| REQ-QA-007 | Important | Committed docs cite uncommitted `PHASE-A-REVIEW.md`; truncated sentence | Closed, c35d9e2, repository exception |
| FTQ-001 | Minor | Fixed drain budget could report a finished child as failed | Closed, 903ede0 |

## Round 4, at c35d9e2
| ID | Severity | Finding | State |
|---|---|---|---|
| RBP-F201 | Important | `ExtractInput` required values as `Option` | Closed, bd00bc3 |
| RBP-F202 | Important | `InputSpec` no-op setters; command rules checked only at run time | Closed, bd00bc3 |
| RBP-F203 to F211 | Minor | Message-text error matching, CRLF digest docs, builder and trait consistency | Closed, bd00bc3 |
| REQ-QA-R3-001 | Minor | r-1 sprint note stale | Closed, 903ede0 |
| QA-001 | Minor | Dry-run false failure with a shared `build-dir` | Closed, bd00bc3 |

## Round 5, at bd00bc3
| ID | Severity | Finding | State |
|---|---|---|---|
| FTQ-001-A | Minor | Raising the budget masked the race | Closed, 903ede0 |
| REQ-QA-R4-001, R4-002 | Minor | Sprint notes stale | Closed, 903ede0 |

## Round 6, at 903ede0
| ID | Severity | Finding | State |
|---|---|---|---|
| FTQ-002 | Important | New pipe test depended on a 200 ms process race | Closed, ddfb698 |

Advisory, tracked rather than a finding at the time: public `Deserialize` on `JevAnswer` and `NoulProbabilities`, bead `agt-dxm`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
