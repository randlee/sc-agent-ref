//! `jev` command dispatch over [`sc_jev_host`].

use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sc_agent_types::observability::v2::NoopGeneralLog;
use sc_agent_types::{
    BackendSet, JevError, ProjectLabel, RepoRoot, Rev, ScAgentErrorResponse, SessionId,
};
use sc_decision_schema::MultiBackendMode;
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::{CallerRequest, CheckSource, Envelope};
use sc_jev_host::compose::{
    ComposeInputs, ResolvedPaths, ask_from, explicit_repo_root, list_models, log_failure, run_from,
};
use sc_jev_host::paths::PathPolicy;
use sc_jev_host::stream::{Concurrency, DecisionStream, StreamSummary, serve_jsonl};
use serde::Serialize;

use crate::envelope;

/// One parsed `sc-ai jev` command.
#[derive(Debug)]
pub enum Command {
    /// `jev run` (live) or `jev dry-run`.
    Run(RunCommand),
    /// `jev ask`.
    Ask(AskCommand),
    /// `jev models`: the resolved models and their source.
    Models(ModelsCommand),
}

/// Arguments of `jev models`.
#[derive(Debug)]
pub struct ModelsCommand {
    /// `--models-file`: read instead of `~/.sc-agent/models.toml`.
    pub models_file: Option<PathBuf>,
}

/// Arguments of `jev run` and `jev dry-run`.
#[derive(Debug)]
pub struct RunCommand {
    /// `--dry-run`: the `dry-run` subcommand.
    pub dry_run: bool,
    /// `--repo-root`.
    pub repo_root: Option<PathBuf>,
    /// `--check`, repo-relative; `check.toml` when absent.
    pub check: Option<PathBuf>,
    /// `--rev`; `WORKTREE` when absent.
    pub rev: Option<Rev>,
    /// `--backend`, in order: a short name or a known model id.
    pub backends: Vec<sc_agent_types::BackendChoice>,
    /// `--request-json`: a path, or `-` for stdin.
    pub request_json: Option<String>,
    /// `--llm-system-prompt-file`.
    pub prompt_file: Option<PathBuf>,
    /// `--sessions-dir`.
    pub sessions_dir: Option<PathBuf>,
    /// `--log-dir`.
    pub log_dir: Option<PathBuf>,
    /// `--models-file`: read instead of `~/.sc-agent/models.toml`.
    pub models_file: Option<PathBuf>,
    /// `--chain-context`: later backends see earlier answers.
    pub chain_context: bool,
    /// `--project`: label for this call's log rows (not scrubbed).
    pub project: Option<ProjectLabel>,
    /// Backends used instead of `BackendRegistry::from_env`. Set only by the
    /// binary's `test-util` seam (`--scripted-answers`).
    pub registry: Option<Arc<BackendRegistry>>,
}

/// Arguments of `jev ask`.
#[derive(Debug)]
pub struct AskCommand {
    /// `--repo-root`.
    pub repo_root: Option<PathBuf>,
    /// `--session-id`.
    pub session_id: SessionId,
    /// `--text`.
    pub text: String,
    /// `--project`: label for this call's log row (not scrubbed).
    pub project: Option<ProjectLabel>,
    /// `--sessions-dir`.
    pub sessions_dir: Option<PathBuf>,
    /// `--log-dir`.
    pub log_dir: Option<PathBuf>,
    /// `--models-file`: read instead of `~/.sc-agent/models.toml`.
    pub models_file: Option<PathBuf>,
    /// Backends used instead of `BackendRegistry::from_env` (test seam).
    pub registry: Option<Arc<BackendRegistry>>,
}

/// Arguments of `jev stream`: many requests through one process
/// (ADR-SAG-0032).
#[derive(Debug)]
pub struct StreamCommand {
    /// `--repo-root`.
    pub repo_root: Option<PathBuf>,
    /// `--backend`; wins over a request's own `backends`, as in `jev run`.
    pub backends: Vec<sc_agent_types::BackendChoice>,
    /// `--input`: a path, or `-` (the default) for stdin.
    pub input: Option<String>,
    /// `--llm-system-prompt-file`.
    pub prompt_file: Option<PathBuf>,
    /// `--sessions-dir`.
    pub sessions_dir: Option<PathBuf>,
    /// `--log-dir`.
    pub log_dir: Option<PathBuf>,
    /// `--models-file`: read instead of `~/.sc-agent/models.toml`.
    pub models_file: Option<PathBuf>,
    /// `--chain-context`.
    pub chain_context: bool,
    /// `--project`.
    pub project: Option<ProjectLabel>,
    /// `--concurrency`: the number of worker threads.
    pub concurrency: Concurrency,
    /// Backends used instead of `BackendRegistry::from_env` (test seam).
    pub registry: Option<Arc<BackendRegistry>>,
}

/// What one command prints.
#[derive(Debug)]
pub struct Output {
    stdout: Vec<u8>,
    failure: Option<ScAgentErrorResponse>,
    warnings: Vec<String>,
    exit_code: u8,
}

impl Output {
    /// The stdout bytes: the versioned envelope plus a newline.
    #[must_use]
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// The first failed backend's error, present only for a partial
    /// or all-failed response. The binary prints it as one stderr line.
    #[must_use]
    pub fn failure(&self) -> Option<&ScAgentErrorResponse> {
        self.failure.as_ref()
    }

    /// The envelope's `data.warnings`: the run succeeded but something worth
    /// knowing happened (the session log was not flushed, or rows could not
    /// be written). The binary prints each as one stderr line; the exit code
    /// is not changed.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The process exit code: 0 with no failure; otherwise 3 only when
    /// **every** failed backend's error is retryable, else 2. It does not
    /// depend on the order of the backends. Exit 3 means "retry the failed
    /// backends": a backend that answered may already have its answer in the
    /// envelope, so a retry should send only the backends that failed.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }
}

/// Runs one `sc-ai jev` command.
///
/// `stdin` is read only for `--request-json -`. Success is the versioned
/// envelope; a partial response also carries the first failure (see
/// [`Output::failure`]).
///
/// # Errors
///
/// Returns [`JevError`] for unreadable paths, config, input, key, session,
/// prompt, or judge failures, including the case where every requested
/// backend failed. A dry-run does not resolve keys.
pub fn execute(command: Command, stdin: &mut dyn Read) -> Result<Output, JevError> {
    match command {
        Command::Run(run) => dispatch_run(run, stdin),
        Command::Ask(ask) => dispatch_ask(ask),
        Command::Models(models) => dispatch_models(&models),
    }
}

/// `jev models`: a read command. The envelope is
/// `{"version","operation":"models","data":{"models_file","models_file_present","models":[...]}}`.
/// It opens no log and sends nothing.
fn dispatch_models(command: &ModelsCommand) -> Result<Output, JevError> {
    let listing = list_models(command.models_file.as_deref(), &NoopGeneralLog)?;
    let mut stdout = serde_json::to_vec(&serde_json::json!({
        "version": sc_jev_check::run::ENVELOPE_VERSION,
        "operation": "models",
        "data": listing,
    }))
    .map_err(|error| JevError::encode_failed("response JSON", &error))?;
    stdout.push(b'\n');
    Ok(Output {
        stdout,
        failure: None,
        warnings: Vec::new(),
        exit_code: 0,
    })
}

/// Inputs shared by `run` and `ask`: the working directory, the explicit
/// repo root and log and session directories.
fn base_inputs(
    dry_run: bool,
    repo_root: Option<&Path>,
    log_dir: Option<PathBuf>,
    sessions_dir: Option<PathBuf>,
    models_file: Option<PathBuf>,
) -> Result<ComposeInputs, JevError> {
    let explicit = match repo_root.map(explicit_repo_root).transpose() {
        Ok(explicit) => explicit,
        Err(error) => {
            // The repo root is not known, so only a log root that needs no
            // repository (`--log-dir`, `SC_LOG_ROOT`) can take the row.
            let early = ComposeInputs::new(PathBuf::from("."), PathPolicy::cli()).dry_run(dry_run);
            let early = match log_dir {
                Some(dir) => early.log_dir(dir),
                None => early,
            };
            log_failure(&early, None, "cli.repo_root", &error);
            return Err(error);
        }
    };
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        // An explicit repo root never needs the working directory.
        Err(_) if explicit.is_some() => PathBuf::from("."),
        Err(error) => {
            return Err(JevError::io_failed_with(
                "current directory",
                error.kind(),
                Some("Pass --repo-root.".to_owned()),
            ));
        }
    };
    let mut inputs = ComposeInputs::new(cwd, PathPolicy::cli());
    if let Some(root) = explicit {
        inputs = inputs.repo_root(root);
    }
    if let Some(dir) = log_dir {
        inputs = inputs.log_dir(dir);
    }
    if let Some(dir) = sessions_dir {
        inputs = inputs.sessions_dir(dir);
    }
    if let Some(path) = models_file {
        inputs = inputs.models_file(path);
    }
    Ok(inputs)
}

fn dispatch_run(command: RunCommand, stdin: &mut dyn Read) -> Result<Output, JevError> {
    let mut inputs = base_inputs(
        command.dry_run,
        command.repo_root.as_deref(),
        command.log_dir.clone(),
        command.sessions_dir.clone(),
        command.models_file.clone(),
    )?
    .dry_run(command.dry_run)
    .mode(if command.chain_context {
        MultiBackendMode::ChainContext
    } else {
        MultiBackendMode::Independent
    });
    if let Some(rev) = command.rev.clone() {
        inputs = inputs.rev(rev);
    }
    if !command.backends.is_empty() {
        inputs = inputs.backends(BackendSet::try_from(command.backends.clone())?);
    }
    if let Some(path) = &command.prompt_file {
        inputs = inputs.system_prompt_override(path.clone());
    }
    if let Some(registry) = &command.registry {
        inputs = inputs.registry(Arc::clone(registry));
    }
    if let Some(project) = &command.project {
        inputs = inputs.project(project.clone());
    }
    let resolved = inputs
        .resolve()
        .inspect_err(|error| log_failure(&inputs, None, "cli.resolve", error))?;
    let repo = &resolved.repo_root;
    let noted = |stage: &'static str| {
        let inputs = &inputs;
        let resolved = &resolved;
        move |error: JevError| {
            log_failure(inputs, Some(resolved), stage, &error);
            error
        }
    };
    let source = if let Some(request_path) = &command.request_json {
        let bytes = request_bytes(repo, request_path, stdin).map_err(noted("cli.request_json"))?;
        CheckSource::CallerRequest(
            CallerRequest::from_json(&bytes).map_err(noted("cli.request_json"))?,
        )
    } else {
        let check_path = command
            .check
            .clone()
            .unwrap_or_else(|| PathBuf::from("check.toml"));
        CheckSource::check_file(repo, &check_path).map_err(noted("cli.check_source"))?
    };
    let readback = run_readback(&command, &resolved);
    // The root is resolved once; `run_from` does not discover it again.
    let response = run_from(source, inputs.repo_root(resolved.repo_root))?;
    let failure = if response.is_partial() {
        response.first_failure().map(|failure| failure.response())
    } else {
        None
    };
    let exit_code = if response.is_partial() {
        if response.all_failures_retryable() {
            sc_agent_types::EXIT_RETRYABLE
        } else {
            sc_agent_types::EXIT_FAILURE
        }
    } else {
        0
    };
    Ok(Output {
        stdout: envelope::to_bytes(&Envelope::run(&response).with_readback(&readback)?)?,
        failure,
        warnings: response.warnings().to_vec(),
        exit_code,
    })
}

fn dispatch_ask(command: AskCommand) -> Result<Output, JevError> {
    let mut inputs = base_inputs(
        false,
        command.repo_root.as_deref(),
        command.log_dir.clone(),
        command.sessions_dir.clone(),
        command.models_file.clone(),
    )?;
    if let Some(registry) = &command.registry {
        inputs = inputs.registry(Arc::clone(registry));
    }
    if let Some(project) = &command.project {
        inputs = inputs.project(project.clone());
    }
    let resolved = inputs
        .resolve()
        .inspect_err(|error| log_failure(&inputs, None, "cli.resolve", error))?;
    let readback = AskReadback {
        repo_root: path_text(resolved.repo_root.as_path()),
        sessions_dir: path_text(&resolved.sessions_dir),
        session_id: command.session_id.to_string(),
    };
    let response = ask_from(
        command.session_id,
        &command.text,
        inputs.repo_root(resolved.repo_root),
    )?;
    Ok(Output {
        stdout: envelope::to_bytes(&Envelope::ask(&response).with_readback(&readback)?)?,
        failure: None,
        warnings: response.warnings().to_vec(),
        exit_code: 0,
    })
}

#[derive(Serialize)]
struct RunReadback {
    check_toml: Option<String>,
    repo_root: String,
    llm_system_prompt_file: Option<String>,
    request_json: Option<String>,
    sessions_dir: String,
}

#[derive(Serialize)]
struct AskReadback {
    repo_root: String,
    sessions_dir: String,
    session_id: String,
}

fn run_readback(command: &RunCommand, resolved: &ResolvedPaths) -> RunReadback {
    let repo = &resolved.repo_root;
    RunReadback {
        check_toml: command
            .check
            .as_ref()
            .map(|path| absolute_text(repo, path))
            .or_else(|| {
                command
                    .request_json
                    .is_none()
                    .then(|| absolute_text(repo, Path::new("check.toml")))
            }),
        repo_root: path_text(repo.as_path()),
        llm_system_prompt_file: command
            .prompt_file
            .as_ref()
            .map(|path| absolute_text(repo, path)),
        request_json: command.request_json.as_ref().map(|path| {
            if path == "-" {
                "-".to_owned()
            } else {
                absolute_text(repo, Path::new(path))
            }
        }),
        sessions_dir: path_text(&resolved.sessions_dir),
    }
}

fn request_bytes(repo: &RepoRoot, spec: &str, stdin: &mut dyn Read) -> Result<Vec<u8>, JevError> {
    if spec == "-" {
        let mut bytes = Vec::new();
        stdin
            .read_to_end(&mut bytes)
            .map_err(|error| JevError::io_failed("stdin", &error))?;
        return Ok(bytes);
    }
    let path = joined_path(repo.as_path(), Path::new(spec));
    std::fs::read(&path).map_err(|error| {
        JevError::read_failed_with(
            path.display().to_string(),
            format!("request JSON {}", path.display()),
            "Pass an existing file as --request-json (or - for stdin).",
            &error,
        )
    })
}

fn joined_path(repo: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}

fn absolute_text(repo: &RepoRoot, path: &Path) -> String {
    path_text(&joined_path(repo.as_path(), path))
}

fn path_text(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

/// Runs `sc-ai jev stream`: JSONL requests in from `--input` (or `stdin`),
/// JSONL answers out to `stdout`, one line per request, in completion order.
///
/// Returns when the input is closed, the queue is empty and the workers are
/// idle. The caller exits with [`StreamSummary::exit_code`]; per-request
/// failures are on their output lines, not an `Err`.
///
/// # Errors
///
/// Returns [`JevError`] for an unreadable input file, root discovery or
/// logger start, or a failure to read the input or write `stdout`.
pub fn serve_stream(
    command: StreamCommand,
    stdin: Box<dyn BufRead + Send>,
    stdout: &mut dyn Write,
) -> Result<StreamSummary, JevError> {
    let mut inputs = base_inputs(
        false,
        command.repo_root.as_deref(),
        command.log_dir.clone(),
        command.sessions_dir.clone(),
        command.models_file.clone(),
    )?
    .mode(if command.chain_context {
        MultiBackendMode::ChainContext
    } else {
        MultiBackendMode::Independent
    });
    if !command.backends.is_empty() {
        inputs = inputs.backends(BackendSet::try_from(command.backends.clone())?);
    }
    if let Some(path) = &command.prompt_file {
        inputs = inputs.system_prompt_override(path.clone());
    }
    if let Some(registry) = &command.registry {
        inputs = inputs.registry(Arc::clone(registry));
    }
    if let Some(project) = &command.project {
        inputs = inputs.project(project.clone());
    }
    let resolved = inputs.resolve()?;
    let input: Box<dyn BufRead + Send> = match command.input.as_deref() {
        None | Some("-") => stdin,
        Some(spec) => {
            let path = joined_path(resolved.repo_root.as_path(), Path::new(spec));
            let file = std::fs::File::open(&path).map_err(|error| {
                JevError::read_failed_with(
                    path.display().to_string(),
                    format!("stream input {}", path.display()),
                    "Pass an existing file as --input (or - for stdin).",
                    &error,
                )
            })?;
            Box::new(std::io::BufReader::new(file))
        }
    };
    let stream = Arc::new(DecisionStream::start(
        inputs.repo_root(resolved.repo_root),
        command.concurrency,
    )?);
    serve_jsonl(&stream, input, stdout)
}
