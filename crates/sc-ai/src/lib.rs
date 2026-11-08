//! `sc-ai` command-line adapter over [`sc_jev_host`].
//!
//! The binary parses argv (clap, behind the `cli` feature) into a typed
//! [`Command`] and [`execute`] runs it through
//! [`sc_jev_host::compose`]; the rest is printing a versioned JSON
//! envelope. Request validation, payload assembly, prompt resolution, judge
//! calls, sessions and key resolution stay in `sc-jev-check`; the session
//! logger, path defaults and repo discovery live once in `sc-jev-host`.
//! This crate does not send HTTP.
//!
//! # Session logs
//!
//! The default log root is `<repo-root>/.sc-ai`. `--log-dir` wins, then the
//! `SC_LOG_ROOT` environment variable, then that default (ADR-SAG-0021,
//! [`sc_jev_host::paths::PathPolicy::cli`]). The v2 file sink writes
//! `<log-root>/logs/sc-ai.log.jsonl`. Session files default to
//! `<repo-root>/.sc-ai/sessions`.
//!
//! # Partial failure
//!
//! When some, not all, requested backends fail, [`Output`] carries the
//! envelope with the successful answers on stdout and the first failure's
//! `ScAgentErrorResponse` for stderr; the process exits 2 (REQ-SAG-0604).
//!
//! # Examples
//!
//! The envelope type, its version and its operation names live in
//! `sc_jev_check::run` (one owner, ADR-SAG-0024):
//!
//! ```
//! use sc_jev_check::run::ENVELOPE_VERSION;
//!
//! assert_eq!(ENVELOPE_VERSION, "1");
//! ```

#![warn(missing_docs)]

mod command;
mod envelope;

#[doc(inline)]
pub use command::{AskCommand, Command, Output, RunCommand, execute};
