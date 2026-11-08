//! `jev` command dispatch over [`sc_jev_host`].

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sc_agent_types::{BackendSet, JevError, RepoRoot, Rev, ScAgentErrorResponse, SessionId};
use sc_decision_schema::MultiBackendMode;
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::{CallerRequest, CheckSource, Envelope};
use sc_jev_host::compose::{ComposeInputs, ResolvedPaths, ask_from, explicit_repo_root, run_from};
use sc_jev_host::paths::PathPolicy;
use serde::Serialize;

use crate::envelope;

/// One parsed `sc-ai jev` command.
#[derive(Debug)]
pub enum Command {
    /// `jev run` (live) or `jev dry-run`.
    Run(RunCommand),
    /// `jev ask`.
    Ask(AskCommand),
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
    /// `--backend`, in order.
    pub backends: Vec<sc_agent_types::BackendId>,
    /// `--request-json`: a path, or `-` for stdin.
    pub request_json: Option<String>,
    /// `--llm-system-prompt-file`.
    pub prompt_file: Option<PathBuf>,
    /// `--sessions-dir`.
    pub sessions_dir: Option<PathBuf>,
    /// `--log-dir`.
    pub log_dir: Option<PathBuf>,
    /// `--chain-context`: later backends see earlier answers.
    pub chain_context: bool,
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
    /// `--sessions-dir`.
    pub sessions_dir: Option<PathBuf>,
    /// `--log-dir`.
    pub log_dir: Option<PathBuf>,
    /// Backends used instead of `BackendRegistry::from_env` (test seam).
    pub registry: Option<Arc<BackendRegistry>>,
}

/// What one command prints.
#[derive(Debug)]
pub struct Output {
    stdout: Vec<u8>,
    failure: Option<ScAgentErrorResponse>,
}

impl Output {
    /// The stdout bytes: the versioned envelope plus a newline.
    #[must_use]
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// The first failed backend's error, present only for a partial
    /// response. The binary prints it as one stderr line and exits 2.
    #[must_use]
    pub fn failure(&self) -> Option<&ScAgentErrorResponse> {
        self.failure.as_ref()
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
    }
}

/// Inputs shared by `run` and `ask`: the working directory, the explicit
/// repo root and log and session directories.
fn base_inputs(
    repo_root: Option<&Path>,
    log_dir: Option<PathBuf>,
    sessions_dir: Option<PathBuf>,
) -> Result<ComposeInputs, JevError> {
    let explicit = repo_root.map(explicit_repo_root).transpose()?;
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        // An explicit repo root never needs the working directory.
        Err(_) if explicit.is_some() => PathBuf::from("."),
        Err(error) => {
            return Err(JevError::operational_config(format!(
                "current directory is unavailable ({}). Pass --repo-root.",
                error.kind()
            )));
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
    Ok(inputs)
}

fn dispatch_run(command: RunCommand, stdin: &mut dyn Read) -> Result<Output, JevError> {
    let mut inputs = base_inputs(
        command.repo_root.as_deref(),
        command.log_dir.clone(),
        command.sessions_dir.clone(),
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
    let resolved = inputs.resolve()?;
    let repo = &resolved.repo_root;
    let source = if let Some(request_path) = &command.request_json {
        let bytes = request_bytes(repo, request_path, stdin)?;
        CheckSource::CallerRequest(CallerRequest::from_json(&bytes)?)
    } else {
        let check_path = command
            .check
            .clone()
            .unwrap_or_else(|| PathBuf::from("check.toml"));
        CheckSource::check_file(repo, &check_path)?
    };
    let readback = run_readback(&command, &resolved);
    // The root is resolved once; `run_from` does not discover it again.
    let response = run_from(source, inputs.repo_root(resolved.repo_root))?;
    let failure = if response.is_partial() {
        response.first_failure().map(|failure| failure.response())
    } else {
        None
    };
    Ok(Output {
        stdout: envelope::to_bytes(&Envelope::run(&response).with_readback(&readback)?)?,
        failure,
    })
}

fn dispatch_ask(command: AskCommand) -> Result<Output, JevError> {
    let mut inputs = base_inputs(
        command.repo_root.as_deref(),
        command.log_dir.clone(),
        command.sessions_dir.clone(),
    )?;
    if let Some(registry) = &command.registry {
        inputs = inputs.registry(Arc::clone(registry));
    }
    let resolved = inputs.resolve()?;
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
        if error.kind() == std::io::ErrorKind::NotFound {
            JevError::input_missing(path.display().to_string())
        } else {
            JevError::io_failed(format!("request JSON {}", path.display()), &error)
        }
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
