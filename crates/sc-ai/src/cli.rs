//! Command-line grammar of the `sc-ai` binary (clap derive, REQ-SAG-0712).
//!
//! [`parse`] is the only entry point. `--help` and `--version` print to
//! stdout and exit 0; every other parse failure is a [`JevError`] with code
//! `INPUT_INVALID` (ADR-SAG-0029 (c)). Value flags accept values that start
//! with `-` (`allow_hyphen_values`), so `--text -1` is text and `--rev -p`
//! reaches [`Rev::parse`], which rejects it before any git call.
//!
//! Under the `test-util` feature only, the hidden `--scripted-answers <file>`
//! flag injects `sc-jev-check`'s scripted backends (the r-2b D5 JSON). The
//! published binary has no such flag.

use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand};
use sc_agent_types::{BackendId, JevError, Rev, SessionId};
use sc_ai::{AskCommand, Command, RunCommand};

/// Usage notes appended to `--help`.
const NOTES: &str = "\
Success writes one JSON envelope to stdout: {\"version\",\"operation\",\"data\"}.
Failure writes one {\"schema_version\",\"error\",\"message\"} line to stderr and
exits 2. When some, not all, backends fail, stdout carries the envelope with the
successful answers, stderr carries the first failure, and the exit code is 2.

--check is a repo-relative check.toml (default check.toml).
--request-json is a caller-built questions document, or - for stdin.
--backend is jev, anthropic_haiku, or openai_luna. The default is jev.
--llm-system-prompt-file overrides the LLM system prompt. It changes the
provider wire, not prompt_sha256.
--rev defaults to WORKTREE.
--chain-context appends earlier backends' answers to later LLM backends'
prompts; the default is independent.

Default log root: <repo-root>/.sc-ai
Override with --log-dir or SC_LOG_ROOT.
Session JSONL: <log-root>/logs/sc-ai.log.jsonl
Session files: <repo-root>/.sc-ai/sessions (override with --sessions-dir).

data is the run response: the primary backend's answers at the top level (the
verbatim Jev response when Jev answered), primary_backend, session_id,
prompt_sha256, payload_bytes, results, and equal_input. Dry-run adds payload
and per-backend limits, and does not resolve API keys. Without --repo-root the
repo root is the git toplevel of the current directory, or the current
directory outside git.";

// `name` and `bin_name` are fixed so usage and `--version` text never depend
// on argv[0] (Windows runs `sc-ai.exe`). A doc comment here would replace
// `about` in `--help`.
#[derive(Debug, Parser)]
#[command(
    name = "sc-ai",
    bin_name = "sc-ai",
    version,
    about = "Jev check client: run, dry-run, and ask.",
    after_help = NOTES
)]
struct Cli {
    /// The command group.
    #[command(subcommand)]
    command: Group,
}

#[derive(Debug, Subcommand)]
enum Group {
    /// Jev check commands.
    #[command(subcommand)]
    Jev(JevCmd),
}

#[derive(Debug, Subcommand)]
enum JevCmd {
    /// Run a check against the selected backends.
    Run(RunArgs),
    /// Build the payload and report limits without sending.
    DryRun(RunArgs),
    /// Send a follow-up to a saved session.
    Ask(AskArgs),
}

/// Arguments of `jev run` and `jev dry-run`.
#[derive(Debug, Args)]
struct RunArgs {
    /// Repository root (default: git toplevel of the current directory).
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    repo_root: Option<PathBuf>,
    /// Repo-relative check.toml.
    #[arg(
        long,
        value_name = "PATH",
        allow_hyphen_values = true,
        value_parser = non_empty_path,
        conflicts_with = "request_json"
    )]
    check: Option<PathBuf>,
    /// Git revision to read inputs at, or WORKTREE.
    #[arg(
        long,
        value_name = "REV",
        allow_hyphen_values = true,
        value_parser = parse_rev,
        conflicts_with = "request_json"
    )]
    rev: Option<Rev>,
    /// Backend id; repeat for several (jev, anthropic_haiku, openai_luna).
    #[arg(long = "backend", value_name = "ID", allow_hyphen_values = true, value_parser = parse_backend)]
    backends: Vec<BackendId>,
    /// Caller-built questions document: a path, or - for stdin.
    #[arg(long, value_name = "PATH|-", allow_hyphen_values = true, value_parser = non_empty_text)]
    request_json: Option<String>,
    /// Prompt file that overrides the LLM system prompt.
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    llm_system_prompt_file: Option<PathBuf>,
    /// Directory for session files.
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    sessions_dir: Option<PathBuf>,
    /// Log root (default: SC_LOG_ROOT, then <repo-root>/.sc-ai).
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    log_dir: Option<PathBuf>,
    /// Let later backends see earlier answers (default: independent).
    #[arg(long)]
    chain_context: bool,
    /// Scripted backend answers (test builds only).
    #[cfg(feature = "test-util")]
    #[arg(long, value_name = "FILE", hide = true, value_parser = non_empty_path)]
    scripted_answers: Option<PathBuf>,
}

/// Arguments of `jev ask`.
#[derive(Debug, Args)]
struct AskArgs {
    /// Session id from a `jev run` envelope.
    #[arg(long, value_name = "UUID", allow_hyphen_values = true, value_parser = parse_session_id)]
    session_id: SessionId,
    /// Follow-up text.
    #[arg(long, value_name = "TEXT", allow_hyphen_values = true, value_parser = non_empty_text)]
    text: String,
    /// Repository root (default: git toplevel of the current directory).
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    repo_root: Option<PathBuf>,
    /// Directory for session files.
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    sessions_dir: Option<PathBuf>,
    /// Log root (default: SC_LOG_ROOT, then <repo-root>/.sc-ai).
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    log_dir: Option<PathBuf>,
    /// Scripted backend answers (test builds only).
    #[cfg(feature = "test-util")]
    #[arg(long, value_name = "FILE", hide = true, value_parser = non_empty_path)]
    scripted_answers: Option<PathBuf>,
}

/// Outcome of [`parse`].
#[derive(Debug)]
pub(crate) enum Parsed {
    /// Run this command.
    Run(Command),
    /// Print this text to stdout and exit 0 (`--help`, `--version`).
    Print(String),
}

/// Parses `args` (including the program name, as from [`std::env::args_os`]).
///
/// # Errors
///
/// [`JevError::InputInvalid`] for every clap error other than the help and
/// version displays; a value parser's own [`JevError`] is returned as is.
/// A `--scripted-answers` file that cannot be read or parsed is an error of
/// the scripted-backends registry.
pub(crate) fn parse<I, T>(args: I) -> Result<Parsed, JevError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match Cli::try_parse_from(args) {
        Ok(cli) => into_command(cli).map(Parsed::Run),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Ok(Parsed::Print(error.render().to_string()))
        }
        Err(error) => {
            Err(value_parser_error(&error)
                .unwrap_or_else(|| JevError::input_invalid(message(&error))))
        }
    }
}

fn into_command(cli: Cli) -> Result<Command, JevError> {
    let Group::Jev(command) = cli.command;
    Ok(match command {
        JevCmd::Run(args) => Command::Run(run_command(args, false)?),
        JevCmd::DryRun(args) => Command::Run(run_command(args, true)?),
        JevCmd::Ask(args) => Command::Ask(AskCommand {
            repo_root: args.repo_root,
            session_id: args.session_id,
            text: args.text,
            sessions_dir: args.sessions_dir,
            log_dir: args.log_dir,
            #[cfg(feature = "test-util")]
            registry: scripted_registry(args.scripted_answers.as_deref())?,
            #[cfg(not(feature = "test-util"))]
            registry: None,
        }),
    })
}

fn run_command(args: RunArgs, dry_run: bool) -> Result<RunCommand, JevError> {
    Ok(RunCommand {
        dry_run,
        repo_root: args.repo_root,
        check: args.check,
        rev: args.rev,
        backends: args.backends,
        request_json: args.request_json,
        prompt_file: args.llm_system_prompt_file,
        sessions_dir: args.sessions_dir,
        log_dir: args.log_dir,
        chain_context: args.chain_context,
        #[cfg(feature = "test-util")]
        registry: scripted_registry(args.scripted_answers.as_deref())?,
        #[cfg(not(feature = "test-util"))]
        registry: None,
    })
}

/// The scripted registry for `--scripted-answers FILE` (ADR-SAG-0022, r-2b
/// D5): the file is passed unchanged to `register_scripted_backends`. The
/// per-backend form names the backends it scripts; the shared form scripts
/// every built-in backend.
#[cfg(feature = "test-util")]
fn scripted_registry(
    file: Option<&std::path::Path>,
) -> Result<Option<std::sync::Arc<sc_jev_check::registry::BackendRegistry>>, JevError> {
    let Some(file) = file else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(file).map_err(|error| {
        JevError::io_failed(format!("scripted answers {}", file.display()), &error)
    })?;
    let answers: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
        JevError::input_invalid(format!("scripted answers are not JSON: {error}"))
    })?;
    let ids: Vec<BackendId> = match answers
        .as_object()
        .filter(|map| map.len() == 1)
        .and_then(|map| map.get("backends"))
        .and_then(serde_json::Value::as_object)
    {
        Some(by_backend) => by_backend
            .keys()
            .map(|name| name.parse::<BackendId>())
            .collect::<Result<_, _>>()?,
        None => sc_agent_types::BACKENDS
            .iter()
            .map(sc_agent_types::BackendDescriptor::id)
            .collect(),
    };
    sc_jev_check::registry::register_scripted_backends(&ids, answers)
        .map(|registry| Some(std::sync::Arc::new(registry)))
}

/// The [`JevError`] a value parser returned, so its own code and message
/// reach stderr unwrapped.
fn value_parser_error(error: &clap::Error) -> Option<JevError> {
    std::error::Error::source(error)?
        .downcast_ref::<JevError>()
        .cloned()
}

/// One-line, colour-free description of a clap error.
fn message(error: &clap::Error) -> String {
    if error.kind() == ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        || error.kind() == ErrorKind::MissingSubcommand
    {
        return "a command is required: jev run, jev dry-run or jev ask (see --help)".to_owned();
    }
    let rendered = error.render().to_string();
    let first = rendered.lines().next().unwrap_or_default();
    first.strip_prefix("error: ").unwrap_or(first).to_owned()
}

fn non_empty_text(value: &str) -> Result<String, JevError> {
    if value.is_empty() {
        Err(JevError::input_invalid(
            "a flag value must not be empty. See sc-ai jev --help.",
        ))
    } else {
        Ok(value.to_owned())
    }
}

fn non_empty_path(value: &str) -> Result<PathBuf, JevError> {
    non_empty_text(value).map(PathBuf::from)
}

fn parse_rev(value: &str) -> Result<Rev, JevError> {
    Rev::parse(value)
}

fn parse_backend(value: &str) -> Result<BackendId, JevError> {
    value.parse()
}

fn parse_session_id(value: &str) -> Result<SessionId, JevError> {
    value.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    /// Usage text names `sc-ai`, never argv\[0\] such as `sc-ai.exe`.
    #[test]
    fn usage_ignores_argv0() {
        for argv0 in ["sc-ai.exe", "/opt/bin/other-name"] {
            let Ok(Parsed::Print(text)) = parse([argv0, "jev", "run", "--help"]) else {
                panic!("{argv0}: jev run --help must print");
            };
            assert!(text.contains("Usage: sc-ai jev run "), "{argv0}: {text}");
        }
    }
}
