//! `sc-ai` binary. JSON on stdout, typed errors on stderr, exit 2 on failure.
//!
//! Argv is parsed by [`cli::parse`] (clap). A partial multi-backend failure
//! prints the envelope on stdout, the first failure on stderr, and exits 2.

use std::io::{self, Write};
use std::process::ExitCode;

use mimalloc::MiMalloc;
use sc_agent_types::{JevError, ScAgentErrorResponse};
use sc_ai::execute;

use crate::cli::Parsed;

mod cli;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> ExitCode {
    let command = match cli::parse(std::env::args_os()) {
        Ok(Parsed::Print(text)) => return print_stdout(text.as_bytes()),
        Ok(Parsed::Run(command)) => command,
        Err(error) => return fail(&error),
    };
    match execute(command, &mut io::stdin().lock()) {
        Ok(output) => {
            let printed = print_stdout(output.stdout());
            match output.failure() {
                Some(failure) => {
                    emit(failure);
                    ExitCode::from(2)
                }
                None => printed,
            }
        }
        Err(error) => fail(&error),
    }
}

fn print_stdout(bytes: &[u8]) -> ExitCode {
    match io::stdout().write_all(bytes) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(&JevError::io_failed("stdout", &error)),
    }
}

fn fail(error: &JevError) -> ExitCode {
    emit(&ScAgentErrorResponse::from_jev_error(error));
    ExitCode::from(2)
}

/// One `ScAgentErrorResponse` line on stderr (NFR-SAG-0001).
fn emit(body: &ScAgentErrorResponse) {
    let line = serde_json::to_string(body).unwrap_or_else(|_| {
        serde_json::to_string(&ScAgentErrorResponse::from_jev_error(
            &JevError::operational_io(
                "stderr encoding failed. Retry the command; the error text could not be written as JSON.",
            ),
        ))
        .expect("ScAgentErrorResponse from JevError must serialize")
    });
    // Best effort: a closed stderr leaves nowhere to report the failure.
    let _ = writeln!(io::stderr(), "{line}");
}
