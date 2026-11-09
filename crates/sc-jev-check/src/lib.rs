//! Jev check engine: `check.toml`, extraction, the canonical payload,
//! backends, sessions, and the one run API.
//!
//! Run a check with [`run::prepare`] then [`run::run`]; the result is one
//! [`run::RunResponse`] whose serialization every consumer prints
//! (ADR-SAG-0024). The run writes a session log row per send and, through
//! [`run::RunConfig::general_log`], a general log of operator diagnostics
//! (ADR-SAG-0013 amendment 2026-11-09). Module roles and boundaries are described in
//! `docs/sc-jev-check/architecture.md`.

// Public modules hold only the facade (`boundaries/sc-jev-check/jev-check.toml`,
// sprint r-2 D2.9). Payload assembly, extraction and the equal-input helpers
// are internal: `run::prepare` is the only way to build a payload
// (ADR-SAG-0005, ADR-SAG-0024).
pub mod config;
pub(crate) mod equal_input;
pub(crate) mod extract;
pub mod models_file;
pub(crate) mod payload;
pub mod registry;
pub mod run;
pub mod session;

/// Shared bounded subprocess runner for unit tests (FTQ-008).
#[cfg(test)]
#[path = "../../../tests/support/deadline.rs"]
mod deadline;

/// Shared git isolation for unit-test fixtures (FTQ-001).
#[cfg(test)]
#[path = "../../../tests/support/git_env.rs"]
mod git_env;
