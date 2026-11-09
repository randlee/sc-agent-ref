# Phase-end review at 741f8ce: findings so far

Rust quality is still running and posts separately. CI runs independently.

| Review | Result |
|---|---|
| Architecture | **FAIL**: 1 Blocking |
| Requirements (per-id audit by reading) | PASS with 2 Minor |
| HTTP client hardening | 2 Minor |
| Flaky tests | PASS |
| Best practices (closure-only) | PASS |
| Rust quality | Running |

## Findings

| ID | Severity | Finding | Location |
|---|---|---|---|
| ARCH-001 | Blocking | `+crt-static` for `x86_64-pc-windows-msvc` changes how the release binaries and the `sc-jev-py` wheel link, with no ADR recording it. Needs an ADR-SAG-0023 amendment: scope, a kit `RUSTFLAGS` as a re-qualification trigger, and Windows CI as the wheel's acceptance check | `.cargo/config.toml:5` |
| RSH-001 | Minor | Credential header value is a plain `String`, not marked sensitive, so a `ureq` debug or trace log could expose it | `crates/sc-agent-http/src/ureq_transport.rs:133` |
| RSH-002 | Minor | `Timeouts::new` and `BusyPolicy::new` accept any value; the limits exist only in `sc-jev` | `crates/sc-agent-http/src/transport.rs:77` |
| REQ-QA-PE-001 | Minor | The r-4 publish-order list doesn't mark itself as superseded by r-3b and r-7 | `docs/architecture.md:826-827` |
| REQ-QA-PE-002 | Minor | Nothing in this repo tests that the rendered Scoop manifest has both shims | `docs/requirements.md:162` |

## Also going in the fix batch

From a comparison with plan-anchor `d104c2d`, where `jev-api.md` is the wire source of truth under decision D9. Not yet checked against the live API.

| Gap | Issue | Location |
|---|---|---|
| Model alias | Rejects a reply whose model differs from the request, such as `jev-latest` answered by `jev-1.13.0` | `crates/sc-decision-schema/src/jev/v1.rs:892` |
| Probability sum | Tolerance `1e-6` against plan-anchor's 0.01, though Jev rounds | `v1.rs:59-61` |
| HTTP errors | 401, 422 and token-limit statuses all become `NETWORK_ERROR` | `crates/sc-jev/src/client/mod.rs:217-222` |
| Question loading | Tuned descriptions, document order and `roles` are dropped | `crates/sc-jev-check/src/payload/questions.rs:61-72` |

Filed separately rather than rushed: `state` as an object (`{anchor, item}`), which changes canonical bytes and fixtures.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
