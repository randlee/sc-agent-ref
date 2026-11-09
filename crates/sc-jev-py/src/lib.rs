//! Python bindings for the Jev check facade.
//!
//! `sc-jev-py` is a thin PyO3 adapter over `sc-jev-host`
//! (ADR-SAG-0011, ADR-SAG-0026), which calls `sc-jev-check` in-process and
//! owns the session logger and the path policy. Payload bytes and Jev HTTP
//! stay in the Rust crates. This crate does not
//! depend on `sc-jev`.
//!
//! # Two logs
//!
//! `run_check` writes two separate `sc-observability` logs: the **session
//! log** below (one row per judge send) and a **general log** of operator
//! diagnostics at `{log_root}/logs/sc-jev-py-general.log.jsonl` (start and
//! end of operations, backend and model, path resolution, credential source
//! as a name never a value, retries and busy waits, extraction and git steps,
//! session store actions, failures by typed id). Neither carries API keys,
//! prompt text, state or provider bodies. Both have sc-agent defaults and
//! both can be replaced per call with `session_log=` and `general_log=`
//! callables. The default general log is the shared host logger of the
//! `logging` module: Rust writes it with no GIL and Python reaches the same
//! logger through `sc_observability.get_host_logger()` after
//! `install_host_logger()`.
//!
//! # Session log
//!
//! Log and session paths follow ADR-SAG-0021. Log root resolution, first
//! match wins:
//!
//! 1. The `log_dir` argument.
//! 2. `SC_AGENT_LOG_DIR`.
//! 3. `SC_LOG_ROOT` (the `sc-observability` convention).
//! 4. `~/.sc-agent/log` (`%USERPROFILE%\.sc-agent\log` when `HOME` is unset;
//!    the process temp directory plus `sc-agent/log` when both are unset).
//!
//! The active file is `{log_root}/logs/sc-jev-py.log.jsonl`. One JSONL line is
//! written per judge send and per session `ask`, `Failed` sends included. The
//! line's `fields` object is one session-log row (ADR-SAG-0031): `answer_id`,
//! `session_id` (a ULID), `repo` (the main repository), `project` (the
//! `project=` text of that call, or `null`), `prompt_sha256`, `request_sha256`,
//! `backend`, `model_id`, `operation`, `send_outcome`, `error_code` and the
//! typed `answers_json`. It carries no API key, prompt text or request body;
//! `project` is caller text and is not scrubbed. Session directory resolution, first match wins:
//!
//! 1. The `session_dir` argument.
//! 2. `SC_AGENT_SESSION_DIR`.
//! 3. `{log_root}/sessions`.
//!
//! # Examples
//!
//! ```text
//! import sc_jev_py as jev
//! result = jev.run_check("check.toml", backends=["jev"], dry_run=False)
//! ```
//!
//! The wheel ships type stubs (`sc_jev_py.pyi`, installed as `__init__.pyi`)
//! and a `py.typed` marker.

#![warn(missing_docs)]

mod error;
mod logging;
mod run;

use pyo3::prelude::*;
use pyo3::types::PyModuleMethods;

use crate::error::JevError;

/// Registers the Python entry points.
///
/// Log and session paths follow ADR-SAG-0021. Log root resolution, first
/// match wins: the `log_dir` argument, `SC_AGENT_LOG_DIR`, `SC_LOG_ROOT`, then
/// `~/.sc-agent/log` (`%USERPROFILE%\.sc-agent\log` when `HOME` is unset; the
/// process temp directory plus `sc-agent/log` when both are unset). The JSONL
/// file is `{log_root}/logs/sc-jev-py.log.jsonl`. One line is appended per
/// judge send and per session `ask`. Each line's `fields` object is one
/// session-log row (`session_id`, `repo`, `project`, `prompt_sha256`,
/// `request_sha256`, `backend`, `model_id`, `operation`, `send_outcome`, and
/// the typed answers); lines do not contain API keys or prompt text.
///
/// Session directory resolution, first match wins: the `session_dir`
/// argument, `SC_AGENT_SESSION_DIR`, then `{log_root}/sessions`.
///
/// `build_request` calls `sc_jev_check::run::prepare`. `run_check` calls
/// `sc_jev_check::run::prepare` then `run` (ADR-SAG-0024). Without
/// `repo_root`, both use `discover_repo_root` of the current directory (the
/// git toplevel). This module does not run extractors,
/// reimplement payload canonicalization or Jev HTTP, or implement
/// `JudgeBackend` (ADR-SAG-0005, ADR-SAG-0009). `run_check` has one signature
/// in every build. Feature `test-util` adds the `_testing` submodule, whose
/// `run_check_scripted` forwards scripted answers into `sc-jev-check`
/// (ADR-SAG-0022). Live runs resolve keys inside `sc-jev-check`.
#[pymodule]
fn sc_jev_py(module: &Bound<'_, PyModule>) -> PyResult<()> {
    logging::register_native(module.py())?;
    module.add_class::<run::CheckResult>()?;
    module.add_class::<run::Session>()?;
    module.add_function(wrap_pyfunction!(run::build_request, module)?)?;
    module.add_function(wrap_pyfunction!(run::run_check, module)?)?;
    module.add_function(wrap_pyfunction!(run::canonical_bytes, module)?)?;
    module.add_function(wrap_pyfunction!(run::canonical_sha256, module)?)?;
    module.add_function(wrap_pyfunction!(run::default_log_root, module)?)?;
    module.add_function(wrap_pyfunction!(run::resolve_log_root, module)?)?;
    module.add_function(wrap_pyfunction!(run::resolve_session_dir, module)?)?;
    module.add_function(wrap_pyfunction!(run::log_file, module)?)?;
    module.add_function(wrap_pyfunction!(run::general_log_file, module)?)?;
    module.add_function(wrap_pyfunction!(run::install_host_logger, module)?)?;
    let jev_error = module.py().get_type::<JevError>();
    // `response` is the run body of a partial failure; absent otherwise.
    jev_error.setattr("response", module.py().None())?;
    module.add("JevError", jev_error)?;
    #[cfg(feature = "test-util")]
    {
        let testing = PyModule::new(module.py(), "_testing")?;
        testing.add_function(wrap_pyfunction!(
            run::testing::run_check_scripted,
            &testing
        )?)?;
        module.add_submodule(&testing)?;
    }
    Ok(())
}
