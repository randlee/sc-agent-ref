//! `sc-ai` binary. JSON on stdout, typed errors on stderr, exit 2 on failure (3 when the error is retryable, or when every failed backend's is).
//!
//! Argv is parsed by [`cli::parse`] (clap). A partial multi-backend failure
//! prints the envelope on stdout, the first failure on stderr, and exits 2, or 3 when
//! every failed backend's error is retryable (retry only the failed backends).
//! An all-failed run does the same, with no answers. `jev stream` instead
//! writes one JSON line per request as each completes and exits by the whole
//! stream's failures.

use std::io::{self, Write};
use std::process::ExitCode;

use mimalloc::MiMalloc;
use sc_agent_types::{JevError, ScAgentErrorResponse};
use sc_ai::{execute, serve_stream};

use crate::cli::Parsed;

mod cli;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> ExitCode {
    let command = match cli::parse(std::env::args_os()) {
        Ok(Parsed::Print(text)) => return print_stdout(text.as_bytes()),
        Ok(Parsed::Stream(command)) => return stream(*command),
        Ok(Parsed::Run(command)) => *command,
        Err(error) => return fail(&error),
    };
    match execute(command, &mut io::stdin().lock()) {
        Ok(output) => {
            let printed = print_stdout(output.stdout());
            let outcome = match output.failure() {
                Some(failure) => {
                    emit(failure);
                    ExitCode::from(output.exit_code())
                }
                None => printed,
            };
            for warning in output.warnings() {
                warn(warning);
            }
            outcome
        }
        Err(error) => fail(&error),
    }
}

/// `jev stream`: answers are written as they complete. Per-request failures
/// are on their lines; the exit code applies the stability rule to the whole
/// stream. A stream-level error (unreadable input, closed stdout) is one
/// stderr line, as for every other command.
fn stream(command: sc_ai::StreamCommand) -> ExitCode {
    let stdin = Box::new(io::BufReader::new(io::stdin()));
    match serve_stream(command, stdin, &mut io::stdout().lock()) {
        Ok(summary) => {
            if let Some(warning) = summary.log_flush_warning() {
                warn(warning);
            }
            ExitCode::from(summary.exit_code())
        }
        Err(error) => fail(&error),
    }
}

/// A warning on a run that succeeded: one stderr line,
/// `{"warning": "<text>", "schema_version": 1}`, with the exit code
/// unchanged. The text is the envelope's `data.warnings` entry. It has no
/// `error` key, so it is never mistaken for a failure.
fn warn(warning: &str) {
    let line = serde_json::json!({ "warning": warning, "schema_version": 1 });
    // Best effort: a closed stderr leaves nowhere to report the warning.
    let _ = writeln!(io::stderr(), "{line}");
}

fn print_stdout(bytes: &[u8]) -> ExitCode {
    match io::stdout().write_all(bytes) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(&JevError::io_failed("stdout", &error)),
    }
}

fn fail(error: &JevError) -> ExitCode {
    let body = ScAgentErrorResponse::from_jev_error(error);
    emit(&body);
    ExitCode::from(body.exit_code())
}

/// One `ScAgentErrorResponse` line on stderr (NFR-SAG-0001).
fn emit(body: &ScAgentErrorResponse) {
    let line = serde_json::to_string(body).unwrap_or_else(|_| {
        serde_json::to_string(&ScAgentErrorResponse::from_jev_error(
            &JevError::io_failed_with(
                "stderr (the error text could not be encoded as JSON)",
                io::ErrorKind::InvalidData,
                Some("Retry the command.".to_owned()),
            ),
        ))
        .expect("ScAgentErrorResponse from JevError must serialize")
    });
    // Best effort: a closed stderr leaves nowhere to report the failure.
    let _ = writeln!(io::stderr(), "{line}");
}
