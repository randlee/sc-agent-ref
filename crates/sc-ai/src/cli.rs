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
use sc_agent_types::{BackendChoice, JevError, ProjectLabel, Rev, SessionId};
use sc_ai::{AskCommand, Command, ModelsCommand, RunCommand, StreamCommand};
use sc_jev_host::stream::Concurrency;

/// Usage notes appended to `--help`.
const NOTES: &str = "\
For run, dry-run and ask, success writes one JSON envelope to stdout: {\"version\",\"operation\",\"data\"}.
Failure writes one JSON line to stderr and exits 2, or 3 for a retryable error. Its keys are error (a stable
id), kind, message (what failed), suggested_action, retryable (always present),
retry_after_seconds, field and details (the last three only when the error has
them) and schema_version (1). When any backend fails, stdout carries the envelope
(the successful answers, if any, and each failed backend's error), stderr carries
the first failure, and the exit code is 2, or 3 when every failed backend's error
is retryable (retry only the failed backends).

--check is a repo-relative check.toml (default check.toml).
--request-json is a caller-built questions document, or - for stdin.
--backend is jev, haiku, or luna (each means the latest model of that line),
or a known model id of one of them, such as claude-haiku-5-5 (that backend
pinned to that model). The default is jev. Other models (sonnet, opus, fable,
terra, sol, astra) are defined but not selectable yet.
jev models lists the resolved models and where each came from (crate or file).
New models are added without recompiling in ~/.sc-agent/models.toml, as
[[model]] rows with id, alias and released (YYYY-MM-DD); a short name resolves
to the most recent released. --models-file reads another file. check.toml
[models] still wins over the file. A malformed file is CONFIG_INVALID.
--llm-system-prompt-file overrides the LLM system prompt. It changes the
provider wire, not payload_sha256.
--rev defaults to WORKTREE.
dry-run builds the payload without sending, but it still runs every `command`
input of check.toml (an argv, no shell), so a check.toml from an untrusted
change is code execution; review it first.
ask needs a session id from a haiku or luna run. A jev run saves no session
(session_id is null). sc-ai and the Python wheel use different default log and
session roots (see docs/stability.md); pass --sessions-dir to share sessions.
--project labels this call's session-log rows (at most 256 bytes of UTF-8, no
control characters). The text is stored as given and is not scrubbed, so put
no secret in it. The default is none.
--chain-context appends earlier backends' answers to later LLM backends'
prompts; the default is independent.

jev stream answers many requests through one process (no process per query):
JSONL in (--input FILE, or stdin), JSONL out on stdout, one line per request,
in completion order, not input order. Each input line is
{\"id\":\"<your id>\",\"request\":<the --request-json document>} and its answer
line echoes the id: {\"id\",\"ok\",\"envelope\"?,\"error\"?}. envelope is the jev run
envelope; error is the stderr body above. ok is false whenever error is present.
A request in which some backends failed has both envelope (the answers that
succeeded; read it even when ok is false) and error (the first failure). A
failed request (a bad line, a failed backend) is an error on its own line and
the stream continues. id is null only when no usable id could be read from the
line; otherwise it is echoed, even if the rest of the line is invalid.
--concurrency N (1 to 64, default 8) sets the worker threads; the queue is bounded, so a slow reader slows the input.
A closed output pipe (for example | head -1) is noticed at the next write, so
when nothing finishes for a long time (every worker in a long Retry-After
wait) the process keeps going until an answer is written or the retry budget
(about 90 s) ends; then it cancels the rest. The
stream ends when the input is closed, the queue is empty and the workers are
idle. Exit 0 when no request failed; else 3 if every failure is retryable, else
2. Per-request errors go on stdout lines, not stderr. Ids are not checked for
uniqueness; --backend, if given, wins over a request's own backends.

Default log root: <repo-root>/.sc-ai
Override with --log-dir or SC_LOG_ROOT.
Session JSONL: <log-root>/logs/sc-ai.log.jsonl (one row per send)
Session files: <repo-root>/.sc-ai/sessions (override with --sessions-dir).

data is the run response: the primary backend's answers at the top level (the
verbatim Jev response when Jev answered), primary_backend, session_id,
payload_sha256, questions_sha256 (SHA-256 of the question set alone),
request_sha256 (per backend, the SHA-256 of the exact first request body,
computed before sending, for cached-response substitution), payload_bytes,
results, and equal_input. Dry-run adds payload
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
    about = "Jev check client: run, dry-run, ask, stream, and models.",
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
    /// List the resolved models and where each came from.
    Models(ModelsArgs),
    /// Build the payload and report limits without sending (runs check.toml `command` inputs).
    DryRun(RunArgs),
    /// Send a follow-up to a saved haiku or luna session (a jev run saves none).
    Ask(AskArgs),
    /// Answer many requests (JSONL in, JSONL out) through one process.
    Stream(StreamArgs),
}

/// Arguments of `jev models`.
#[derive(Debug, Args)]
struct ModelsArgs {
    /// Models file (default: ~/.sc-agent/models.toml).
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    models_file: Option<PathBuf>,
}

/// Arguments of `jev stream`.
#[derive(Debug, Args)]
struct StreamArgs {
    /// Repository root (default: git toplevel of the current directory).
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    repo_root: Option<PathBuf>,
    /// Backend (short name or known model id) for every request; repeat for several. Wins over a
    /// request's own `backends`, as in `jev run`.
    #[arg(long = "backend", value_name = "ID", allow_hyphen_values = true, value_parser = parse_backend)]
    backends: Vec<BackendChoice>,
    /// JSONL requests: a path, or - for stdin (the default).
    #[arg(long, value_name = "PATH|-", allow_hyphen_values = true, value_parser = non_empty_text)]
    input: Option<String>,
    /// Number of worker threads, 1 to 64 (default: 8).
    #[arg(long, value_name = "N", allow_hyphen_values = true, value_parser = parse_concurrency)]
    concurrency: Option<Concurrency>,
    /// Prompt file that overrides the LLM system prompt.
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    llm_system_prompt_file: Option<PathBuf>,
    /// Directory for session files.
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    sessions_dir: Option<PathBuf>,
    /// Log root (default: SC_LOG_ROOT, then <repo-root>/.sc-ai).
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    log_dir: Option<PathBuf>,
    /// Models file (default: ~/.sc-agent/models.toml).
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    models_file: Option<PathBuf>,
    /// Let later backends see earlier answers (default: independent).
    #[arg(long)]
    chain_context: bool,
    /// Project label for the log rows (not scrubbed; no secrets).
    #[arg(long, value_name = "TEXT", allow_hyphen_values = true, value_parser = parse_project)]
    project: Option<ProjectLabel>,
    /// Scripted backend answers (test builds only).
    #[cfg(feature = "test-util")]
    #[arg(long, value_name = "FILE", hide = true, value_parser = non_empty_path)]
    scripted_answers: Option<PathBuf>,
    /// Send over real HTTP to `http://127.0.0.1:<PORT>` (test builds only).
    #[cfg(feature = "test-util")]
    #[arg(long, value_name = "PORT", hide = true)]
    loopback_port: Option<u16>,
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
    /// Backend: jev, haiku or luna (latest), or a known model id such as
    /// claude-haiku-5-5; repeat for several.
    #[arg(long = "backend", value_name = "ID", allow_hyphen_values = true, value_parser = parse_backend)]
    backends: Vec<BackendChoice>,
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
    /// Models file (default: ~/.sc-agent/models.toml).
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    models_file: Option<PathBuf>,
    /// Let later backends see earlier answers (default: independent).
    #[arg(long)]
    chain_context: bool,
    /// Project label for this call's log rows (not scrubbed; no secrets).
    #[arg(long, value_name = "TEXT", allow_hyphen_values = true, value_parser = parse_project)]
    project: Option<ProjectLabel>,
    /// Scripted backend answers (test builds only).
    #[cfg(feature = "test-util")]
    #[arg(long, value_name = "FILE", hide = true, value_parser = non_empty_path)]
    scripted_answers: Option<PathBuf>,
    /// Send over real HTTP to `http://127.0.0.1:<PORT>` instead of the
    /// providers (test builds only; keys still come from the environment).
    #[cfg(feature = "test-util")]
    #[arg(long, value_name = "PORT", hide = true)]
    loopback_port: Option<u16>,
}

/// Arguments of `jev ask`.
#[derive(Debug, Args)]
struct AskArgs {
    /// Session id (a ULID) from a `jev run` envelope.
    #[arg(long, value_name = "ULID", allow_hyphen_values = true, value_parser = parse_session_id)]
    session_id: SessionId,
    /// Project label for this call's log rows (not scrubbed; no secrets).
    #[arg(long, value_name = "TEXT", allow_hyphen_values = true, value_parser = parse_project)]
    project: Option<ProjectLabel>,
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
    /// Models file (default: ~/.sc-agent/models.toml).
    #[arg(long, value_name = "PATH", allow_hyphen_values = true, value_parser = non_empty_path)]
    models_file: Option<PathBuf>,
    /// Scripted backend answers (test builds only).
    #[cfg(feature = "test-util")]
    #[arg(long, value_name = "FILE", hide = true, value_parser = non_empty_path)]
    scripted_answers: Option<PathBuf>,
}

/// Outcome of [`parse`].
#[derive(Debug)]
pub(crate) enum Parsed {
    /// Run this command.
    ///
    /// Boxed so the enum stays small on Windows, where `Command` is much
    /// larger than `Print`'s `String` (`clippy::large_enum_variant`).
    Run(Box<Command>),
    /// Serve a JSONL stream (`jev stream`).
    Stream(Box<StreamCommand>),
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
        Ok(Cli {
            command: Group::Jev(JevCmd::Stream(args)),
        }) => stream_command(args).map(|command| Parsed::Stream(Box::new(command))),
        Ok(cli) => into_command(cli).map(|command| Parsed::Run(Box::new(command))),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Ok(Parsed::Print(error.render().to_string()))
        }
        Err(error) => Err(value_parser_error(&error)
            .map(|parsed| with_blamed_flag(parsed, &error))
            .unwrap_or_else(|| usage_error(&error))),
    }
}

fn into_command(cli: Cli) -> Result<Command, JevError> {
    let Group::Jev(command) = cli.command;
    Ok(match command {
        JevCmd::Stream(_) => unreachable!("parse routes `jev stream` to stream_command"),
        JevCmd::Models(args) => Command::Models(ModelsCommand {
            models_file: args.models_file,
        }),
        JevCmd::Run(args) => Command::Run(run_command(args, false)?),
        JevCmd::DryRun(args) => Command::Run(run_command(args, true)?),
        JevCmd::Ask(args) => Command::Ask(AskCommand {
            repo_root: args.repo_root,
            session_id: args.session_id,
            text: args.text,
            project: args.project,
            sessions_dir: args.sessions_dir,
            log_dir: args.log_dir,
            models_file: args.models_file,
            #[cfg(feature = "test-util")]
            registry: scripted_registry(args.scripted_answers.as_deref())?,
            #[cfg(not(feature = "test-util"))]
            registry: None,
        }),
    })
}

fn stream_command(args: StreamArgs) -> Result<StreamCommand, JevError> {
    #[cfg(feature = "test-util")]
    let registry = stream_registry(&args)?;
    Ok(StreamCommand {
        repo_root: args.repo_root,
        backends: args.backends,
        input: args.input,
        prompt_file: args.llm_system_prompt_file,
        sessions_dir: args.sessions_dir,
        log_dir: args.log_dir,
        models_file: args.models_file,
        chain_context: args.chain_context,
        project: args.project,
        concurrency: args.concurrency.unwrap_or_default(),
        #[cfg(feature = "test-util")]
        registry,
        #[cfg(not(feature = "test-util"))]
        registry: None,
    })
}

/// The test registry of a `stream`: as [`run_registry`].
#[cfg(feature = "test-util")]
fn stream_registry(
    args: &StreamArgs,
) -> Result<Option<std::sync::Arc<sc_jev_check::registry::BackendRegistry>>, JevError> {
    if let Some(registry) = scripted_registry(args.scripted_answers.as_deref())? {
        return Ok(Some(registry));
    }
    let Some(port) = args.loopback_port else {
        return Ok(None);
    };
    let ids = if args.backends.is_empty() {
        vec![sc_agent_types::BackendId::Jev.into()]
    } else {
        args.backends.clone()
    };
    let backends = sc_agent_types::BackendSet::try_from(ids)?;
    let root = args.repo_root.clone().unwrap_or_else(|| PathBuf::from("."));
    sc_jev_check::registry::BackendRegistry::loopback_from_env(
        &backends,
        &sc_agent_types::RepoRoot::new(root),
        port,
        3,
    )
    .map(|registry| Some(std::sync::Arc::new(registry)))
}

fn run_command(args: RunArgs, dry_run: bool) -> Result<RunCommand, JevError> {
    #[cfg(feature = "test-util")]
    let registry = run_registry(&args)?;
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
        models_file: args.models_file,
        chain_context: args.chain_context,
        project: args.project,
        #[cfg(feature = "test-util")]
        registry,
        #[cfg(not(feature = "test-util"))]
        registry: None,
    })
}

/// The test registry of a `run`: `--scripted-answers` wins; otherwise
/// `--loopback-port` builds real HTTP backends aimed at the loopback server.
#[cfg(feature = "test-util")]
fn run_registry(
    args: &RunArgs,
) -> Result<Option<std::sync::Arc<sc_jev_check::registry::BackendRegistry>>, JevError> {
    if let Some(registry) = scripted_registry(args.scripted_answers.as_deref())? {
        return Ok(Some(registry));
    }
    let Some(port) = args.loopback_port else {
        return Ok(None);
    };
    let ids = if args.backends.is_empty() {
        vec![sc_agent_types::BackendId::Jev.into()]
    } else {
        args.backends.clone()
    };
    let backends = sc_agent_types::BackendSet::try_from(ids)?;
    let root = args.repo_root.clone().unwrap_or_else(|| PathBuf::from("."));
    sc_jev_check::registry::BackendRegistry::loopback_from_env(
        &backends,
        &sc_agent_types::RepoRoot::new(root),
        port,
        1,
    )
    .map(|registry| Some(std::sync::Arc::new(registry)))
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
        JevError::read_failed_with(
            file.display().to_string(),
            format!("scripted answers {}", file.display()),
            "Pass an existing file as --scripted-answers.",
            &error,
        )
    })?;
    let answers: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
        JevError::input_invalid(format!("scripted answers are not JSON: {error}"))
    })?;
    let ids: Vec<sc_agent_types::BackendId> = match answers
        .as_object()
        .filter(|map| map.len() == 1)
        .and_then(|map| map.get("backends"))
        .and_then(serde_json::Value::as_object)
    {
        Some(by_backend) => by_backend
            .keys()
            .map(|name| name.parse::<sc_agent_types::BackendId>())
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

/// A value parser's own error, with the flag clap blames as its `field` when
/// the parser did not name one (an empty value does not know its flag).
fn with_blamed_flag(parsed: JevError, error: &clap::Error) -> JevError {
    match error
        .get(clap::error::ContextKind::InvalidArg)
        .map(ToString::to_string)
        .and_then(|argument| JevError::usage_field(&argument))
    {
        Some(flag) => parsed.with_field(flag),
        None => parsed,
    }
}

/// `INPUT_INVALID` for a command-line usage error, naming the argument clap
/// blames when it names one.
///
/// Usage errors are `INPUT_INVALID`, never `CONFIG_INVALID`: the caller's
/// command line is input, and no configuration source is wrong.
fn usage_error(error: &clap::Error) -> JevError {
    let detail = message(error);
    let field = error
        .get(clap::error::ContextKind::InvalidArg)
        .map(ToString::to_string)
        .and_then(|argument| JevError::usage_field(&argument));
    match field {
        Some(field) => JevError::input_invalid_field(field, detail),
        None => JevError::input_invalid(detail),
    }
}

/// One-line, colour-free description of a clap error.
fn message(error: &clap::Error) -> String {
    if error.kind() == ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        || error.kind() == ErrorKind::MissingSubcommand
    {
        return "a command is required: jev run, jev dry-run, jev ask or jev stream (see --help)"
            .to_owned();
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
    Rev::parse(value).map_err(|error| error.with_field("--rev"))
}

fn parse_concurrency(value: &str) -> Result<Concurrency, JevError> {
    let workers: usize = value.parse().map_err(|_| {
        JevError::input_invalid_field("--concurrency", "--concurrency must be a whole number")
    })?;
    Concurrency::new(workers).map_err(|_| {
        JevError::input_invalid_field(
            "--concurrency",
            format!("--concurrency must be between 1 and {}", Concurrency::MAX),
        )
    })
}

fn parse_backend(value: &str) -> Result<BackendChoice, JevError> {
    value
        .parse()
        .map_err(|error: JevError| error.with_field("--backend"))
}

/// A session id is a ULID since Phase A.1; anything else, a UUID included,
/// is one `INPUT_INVALID` line whose fix hint says how to start over.
fn parse_session_id(value: &str) -> Result<SessionId, JevError> {
    value.parse().map_err(|_| {
        // Caller text echoed in an error is cut short (it can be any length).
        let shown: String = value.chars().take(64).collect();
        let cut = if shown.len() < value.len() { "..." } else { "" };
        JevError::input_invalid_field(
            "--session-id",
            format!(
                "session id {shown:?}{cut} is not a ULID; session ids are ULIDs since Phase A.1; start a new session with `sc-ai jev run`"
            ),
        )
    })
}

fn parse_project(value: &str) -> Result<ProjectLabel, JevError> {
    ProjectLabel::new(value).map_err(|error| error.with_field("--project"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    /// A UUID (or any non-ULID) session id names ULIDs in its fix hint.
    #[test]
    fn session_id_hint_names_ulids() {
        for bad in ["3f2b8a9e-6c1d-4e7a-9b25-0d4c1a7e5f10", "nope", ""] {
            let error = parse_session_id(bad).expect_err("not a ULID");
            assert_eq!(error.code(), "INPUT_INVALID");
            let message = error.message();
            assert!(message.contains("ULID"), "{message}");
            assert!(message.contains("sc-ai jev run"), "{message}");
            assert_eq!(error.field(), Some("--session-id"));
            assert!(error.message().len() < 400);
        }
        assert!(parse_session_id("01JC4Z7A5B0000000000000001").is_ok());
    }

    /// The `--help` notes list the amended stderr keys (REQ-SAG-0701).
    #[test]
    fn help_notes_list_the_error_body_keys() {
        let Ok(Parsed::Print(text)) = parse(["sc-ai", "--help"]) else {
            panic!("--help must print");
        };
        for key in [
            "error (a stable",
            "kind,",
            "message (what failed)",
            "suggested_action",
            "retryable (always present)",
            "retry_after_seconds",
            "field and details",
            "schema_version (1)",
            "exits 2, or 3 for a retryable error",
            "or 3 when every failed backend's error",
            "retry only the failed backends",
        ] {
            assert!(text.contains(key), "{key}: {text}");
        }
        assert!(!text.contains("{\"schema_version\",\"error\",\"message\"}"));
    }

    /// The `--help` notes pin the stream wire format and exit rule
    /// (REQ-SAG-0725).
    #[test]
    fn help_notes_describe_the_stream() {
        let Ok(Parsed::Print(text)) = parse(["sc-ai", "--help"]) else {
            panic!("--help must print");
        };
        for key in [
            "jev stream answers many requests through one process",
            "{\"id\":\"<your id>\",\"request\":",
            "in completion order, not input order",
            "echoes the id",
            "ok is false whenever error is present.",
            "has both envelope (the answers that",
            "succeeded; read it even when ok is false) and error (the first failure)",
            "id is null only when no usable id could be read from the",
            "otherwise it is echoed, even if the rest of the line is invalid.",
            "failed request (a bad line, a failed",
            "the stream continues",
            "--concurrency N (1 to 64, default 8)",
            "the queue is bounded",
            "A closed output pipe (for example | head -1) is noticed at the next write",
            "the process keeps going until an answer is written or the retry budget",
            "the input is closed, the queue is empty and the workers are",
            "Exit 0 when no request failed; else 3 if every failure is retryable, else",
            "Ids are not checked for",
        ] {
            assert!(text.contains(key), "{key}: {text}");
        }
    }

    /// `--concurrency` is a whole number from 1 to 64; the default is 8.
    #[test]
    fn stream_concurrency_is_validated() {
        let run = |extra: &[&str]| {
            let mut argv = vec!["sc-ai", "jev", "stream"];
            argv.extend_from_slice(extra);
            parse(argv)
        };
        let Ok(Parsed::Stream(command)) = run(&[]) else {
            panic!("stream parses");
        };
        assert_eq!(command.concurrency.get(), 8);
        let Ok(Parsed::Stream(command)) = run(&["--concurrency", "3"]) else {
            panic!("stream parses");
        };
        assert_eq!(command.concurrency.get(), 3);
        for bad in ["0", "65", "x", "-1"] {
            let error = run(&["--concurrency", bad]).expect_err(bad);
            assert_eq!(error.code(), "INPUT_INVALID", "{bad}");
            assert_eq!(error.field(), Some("--concurrency"), "{bad}");
        }
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
