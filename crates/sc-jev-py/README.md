# sc-jev-py

PyO3 bindings over `sc-jev-check`, shipped to PyPI as a maturin wheel.

This crate has `publish = false` and never goes to crates.io. Install the wheel with `pip install sc-jev-py`.

Logging: `run_check` writes a session log and a separate general log under the
log root; pass `session_log=` and `general_log=` callables to replace either.
The default general log is the `sc-observability` logger linked into this wheel:
Rust writes it with no GIL, and after `sc_jev_py.install_host_logger()` the
`sc-observability` package's `get_host_logger()` returns the same logger
(`pip install sc-jev-py[observability]`; import `sc_jev_py` first).

Stability: all crates are 0.x; see the [stability statement](https://github.com/randlee/sc-agent/blob/develop/docs/stability.md).

Part of the [sc-agent](https://github.com/randlee/sc-agent) workspace. See the
repository `docs/` for architecture and requirements. Licensed under MIT.
