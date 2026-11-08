//! Composition-root support for `sc-ai` and `sc-jev-py`.
//!
//! Both roots are thin adapters (argv, PyO3) over this crate, so the logic
//! that would otherwise be written twice lives once (ADR-SAG-0026 compose
//! half; a future MCP server is a third adapter):
//!
//! - [`obs`]: the `sc-observability` v2 logger adapted to the
//!   [`SessionLog`](sc_agent_types::observability::v2::SessionLog) port.
//! - [`paths`]: [`PathPolicy`](paths::PathPolicy), the log-root and
//!   session-directory precedence of ADR-SAG-0021.
//! - [`compose`]: [`run_from`](compose::run_from) and
//!   [`ask_from`](compose::ask_from) over
//!   [`ComposeInputs`](compose::ComposeInputs).
//!
//! # Stability
//!
//! This is an **internal support crate**. It is published only so `sc-ai` can
//! be installed, both consumers pin it with an exact `=` requirement
//! (ADR-SAG-0023), and it makes **no semver promise of its own**. External
//! consumers with their own composition needs call
//! `sc_jev_check::run::{prepare, run}` directly.
//!
//! # Types owned by other crates
//!
//! [`compose::run_from`] and [`compose::ask_from`] name `CheckSource`,
//! `RunResponse` and `AskResponse` (owned by `sc-jev-check`) and `JevError`
//! (owned by `sc-agent-types`). This crate does not re-export them, so there
//! is one path per item; a consumer depends on `sc-jev-check` and
//! `sc-agent-types` directly, at the versions this release pins.
//!
//! # Environment ownership
//!
//! Log and session path environment variables are read only by
//! [`paths::PathPolicy`]. API-key environment and `.env` reading stay in
//! `sc-jev-check` (`BackendRegistry::from_env`).

#![warn(missing_docs)]

pub mod compose;
pub mod obs;
pub mod paths;
