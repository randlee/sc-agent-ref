# sc-jev-host

Composition-root support for the `sc-ai` command-line tool and the `sc-jev-py`
Python wheel: the `sc-observability` session-log and general-log adapters, the log and session
path policy, and the `run`/`ask` wiring over `sc-jev-check`.

This is an **internal support crate**. It is published to crates.io only so
that `sc-ai` can be installed, and `sc-ai` and `sc-jev-py` pin it with an
exact `=` requirement under the ADR-SAG-0023 lockstep rule. It makes **no
semver promise of its own**: pin the exact version, or call
`sc_jev_check::{prepare, run}` directly.

Its signatures name types owned by `sc-jev-check` (`CheckSource`,
`RunResponse`, `AskResponse`, `JevError` via `sc-agent-types`). A consumer
depends on `sc-jev-check` and `sc-agent-types` directly, at the versions this
release pins.

Stability: all crates are 0.x; see the [stability statement](https://github.com/randlee/sc-agent/blob/develop/docs/stability.md).

Part of the [sc-agent](https://github.com/randlee/sc-agent) workspace. See the
repository `docs/sc-jev-host/` for architecture and requirements.
Licensed under MIT.
