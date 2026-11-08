//! Argv-only git helpers (feature `git`, RBQA-F006).
//!
//! Every call runs `git --no-pager -C <dir> ...` with `GIT_TERMINAL_PROMPT=0`
//! and `LC_ALL=C`, passes revisions after `--end-of-options`, and classifies
//! failures by exit status and follow-up probes, never by parsing localized
//! messages. Blob bytes come from `git cat-file blob <sha>`, which writes the
//! stored bytes without textconv or filters, through a reader bounded by
//! the caller's byte limit. [`hash_blob`] computes the blob id of bytes the
//! caller already holds (`hash-object --stdin --no-filters`), so a working
//! tree read and its id always agree.
//!
//! The user's git config is honoured (for example `safe.directory`); the
//! commands are read-only plumbing that run no hooks and do not sign.
//! Environment overrides that would redirect git to another repository
//! (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`,
//! `GIT_ALTERNATE_OBJECT_DIRECTORIES`) are removed.
//!
//! Every git subprocess has a wall-clock deadline, `GIT_DEADLINE` (60 s, a
//! fixed default). The child is polled with `try_wait`; at the deadline it is
//! killed and reaped and the call returns the typed
//! `JevError::GitTimeout` (id `EXTRACTION_TIMEOUT`, ADR-SAG-0027 r-7), the same pattern as the
//! `sc-jev-check` command extractor. Output is also bounded by `max_bytes`,
//! and prompts are off (`GIT_TERMINAL_PROMPT=0`).

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::JevError;
use crate::ids::{BlobSha, RepoRelativePath, RepoRoot, Rev};

/// Largest source input the git readers' callers accept: 8 MiB (RSH-007).
///
/// Pass it as `max_bytes` to [`read_at_rev`]; working-tree readers apply the
/// same cap before [`hash_blob`]. `sc-jev-check` uses this constant for its
/// worktree and revision reads.
pub const MAX_INPUT_BYTES: u64 = 8 * 1024 * 1024;

/// Wall-clock limit of one git subprocess (a fixed default, not configurable).
pub(crate) const GIT_DEADLINE: Duration = Duration::from_secs(60);

/// How often a running git child is polled.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Bound on killing a child and reaping it, on the failure path only.
///
/// It applies after the deadline fired or `try_wait` failed, never to a child
/// that exited by itself. Killing normally reaps at once; the bound only keeps
/// a child stuck in the kernel from blocking the caller forever.
const REAP_BUDGET: Duration = Duration::from_secs(10);

/// Bytes of git's stderr kept for an error detail.
const STDERR_DETAIL_BYTES: u64 = 4096;

/// Bytes of `hash-object` stdout read: a 64-digit id plus a newline fits.
const OBJECT_ID_OUTPUT_BYTES: u64 = 128;

/// One blob read from git: its object id and bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitBlob {
    sha: BlobSha,
    bytes: Vec<u8>,
}

impl GitBlob {
    /// The blob's object id.
    #[must_use]
    pub fn sha(&self) -> &BlobSha {
        &self.sha
    }

    /// The blob bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the bytes, consuming the blob.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: the one argv-only `git` command every revision read goes through (ADR-SAG-0028)"
)]
fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-pager")
        .arg("-C")
        .arg(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        // `-C dir` must decide the repository; inherited overrides would
        // point git at another one.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .stdin(Stdio::null());
    command
}

fn run(command: Command) -> Result<Output, JevError> {
    run_with_deadline(command, GIT_DEADLINE)
}

/// Runs `command` with piped output within `limit`; kills and reaps it at the
/// deadline (`GitTimeout`).
fn run_with_deadline(mut command: Command, limit: Duration) -> Result<Output, JevError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| JevError::io_failed("git", &error))?;
    let started = Instant::now();
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let status = wait_bounded(&mut child, started, limit)?;
    // The pipes are waited for against the one deadline of the call. The
    // child exited, so its own pipe ends are closed and EOF is immediate; the
    // bound fires only when something else still holds a pipe (a process the
    // child left behind, such as a git helper or daemon). The deadline can
    // already be past here, because it is observed once per `POLL_INTERVAL`
    // and the child may have exited within that tick. The floor of one tick
    // gives the reader threads that same granularity to hand over output that
    // is already in the pipe, so a finished child is not reported as failed
    // for being noticed late.
    let pipes = (started + limit).max(Instant::now() + POLL_INTERVAL);
    Ok(Output {
        status,
        stdout: collect(stdout, pipes)?,
        stderr: collect(stderr, pipes)?,
    })
}

/// Reads a pipe to its end on its own thread.
fn drain(mut pipe: impl Read + Send + 'static) -> Receiver<io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = pipe.read_to_end(&mut bytes).map(|_| bytes);
        let _ = tx.send(result);
    });
    rx
}

/// The drained bytes, waiting at most until `deadline`; a missing pipe is empty.
fn collect(
    rx: Option<Receiver<io::Result<Vec<u8>>>>,
    deadline: Instant,
) -> Result<Vec<u8>, JevError> {
    let Some(rx) = rx else {
        return Ok(Vec::new());
    };
    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(error)) => Err(JevError::io_failed("git output", &error)),
        Err(_) => Err(JevError::io_failed(
            "git output",
            &io::Error::new(io::ErrorKind::TimedOut, "pipe did not close"),
        )),
    }
}

/// Polls `child` until it exits or `limit` passes since `started`; at the
/// deadline the child is killed and reaped and the call is `GitTimeout`.
fn wait_bounded(
    child: &mut Child,
    started: Instant,
    limit: Duration,
) -> Result<ExitStatus, JevError> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() >= limit => {
                kill_and_reap(child);
                return Err(JevError::git_timeout(limit.as_secs()));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                kill_and_reap(child);
                return Err(JevError::io_failed("git", &error));
            }
        }
    }
}

/// Kills `child` and waits for it for at most [`REAP_BUDGET`] (failure path).
fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let deadline = Instant::now() + REAP_BUDGET;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(None) => thread::sleep(POLL_INTERVAL),
            _ => return,
        }
    }
}

/// Finds the git toplevel that contains `start`.
///
/// Returns `Ok(None)` when `start` is not inside a git work tree.
///
/// # Errors
///
/// `INPUT_INVALID` when git cannot be started or prints a toplevel that is
/// not UTF-8.
pub fn discover_toplevel(start: &Path) -> Result<Option<RepoRoot>, JevError> {
    let mut command = git(start);
    command.args(["rev-parse", "--show-toplevel"]);
    let output = run(command)?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = std::str::from_utf8(&output.stdout)
        .map_err(|_| JevError::input_invalid("git printed a toplevel path that is not UTF-8"))?;
    let path = text.trim_end_matches(['\n', '\r']);
    if path.is_empty() {
        return Ok(None);
    }
    Ok(Some(RepoRoot::new(path)))
}

/// Finds the git common directory of the repository that contains `start`.
///
/// Runs `git rev-parse --path-format=absolute --git-common-dir`. For a
/// linked worktree this is the main repository's git directory, shared by
/// every worktree; for an ordinary checkout it is its own `.git`. Returns
/// `Ok(None)` when `start` is not inside a git repository.
///
/// # Errors
///
/// `INPUT_INVALID` when git cannot be started or prints a path that is not
/// UTF-8.
pub fn discover_common_dir(start: &Path) -> Result<Option<PathBuf>, JevError> {
    let mut command = git(start);
    command.args(["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    let output = run(command)?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = std::str::from_utf8(&output.stdout)
        .map_err(|_| JevError::input_invalid("git printed a common dir path that is not UTF-8"))?;
    let path = text.trim_end_matches(['\n', '\r']);
    if path.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(path)))
}

/// Reads `path` at `rev` from the repository at `root`.
///
/// Resolves `{rev}:{path}` with `rev-parse --verify` to an object id, then
/// reads it with `cat-file blob`, which rejects trees. At most `max_bytes`
/// bytes are read.
///
/// # Errors
///
/// - `INPUT_INVALID` when `rev` is [`Rev::WORKTREE`], starts with `-`, does
///   not resolve, or `path` names something other than a file;
/// - `INPUT_MISSING` when `rev` resolves but has no `path`;
/// - `INPUT_TOO_LARGE` when the blob is larger than `max_bytes`;
/// - `INPUT_INVALID` (`io_failed`) when git cannot be started or read.
pub fn read_at_rev(
    root: &RepoRoot,
    rev: &Rev,
    path: &RepoRelativePath,
    max_bytes: u64,
) -> Result<GitBlob, JevError> {
    let rev_text = rev.as_str();
    if rev.is_worktree() || rev_text.starts_with('-') || rev_text.is_empty() {
        return Err(JevError::input_invalid(format!(
            "revision {rev_text} is not a git revision that can be read"
        )));
    }
    let object = format!("{rev_text}:{}", path.as_str());
    let mut resolve = git(root.as_path());
    resolve.args([
        "rev-parse",
        "--verify",
        "--quiet",
        "--end-of-options",
        &object,
    ]);
    let resolved = run(resolve)?;
    if !resolved.status.success() {
        return Err(classify_missing(root, rev, path)?);
    }
    let sha_text = std::str::from_utf8(&resolved.stdout)
        .map(str::trim)
        .unwrap_or_default();
    let sha = BlobSha::try_from(sha_text).map_err(|_| {
        JevError::extraction_failed("git rev-parse returned an unexpected object id")
    })?;

    let mut kind = git(root.as_path());
    kind.args(["cat-file", "-t", sha.as_str()]);
    let kind = run(kind)?;
    if !kind.status.success() || kind.stdout.trim_ascii() != b"blob" {
        return Err(JevError::input_invalid(format!(
            "{path} at {rev} is not a file",
            path = path.as_str()
        )));
    }
    let mut size = git(root.as_path());
    size.args(["cat-file", "-s", sha.as_str()]);
    let size = run(size)?;
    let actual_bytes = std::str::from_utf8(&size.stdout)
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok());
    if let Some(actual_bytes) = actual_bytes
        && actual_bytes > max_bytes
    {
        return Err(JevError::input_too_large(max_bytes, actual_bytes));
    }

    let mut read = git(root.as_path());
    read.args(["cat-file", "blob", sha.as_str()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = read
        .spawn()
        .map_err(|error| JevError::io_failed("git", &error))?;
    let started = Instant::now();
    let limit = max_bytes.saturating_add(1);
    // The read runs on its own thread so a hung git cannot block this one.
    let reader = child.stdout.take().map(|stdout| {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout.take(limit).read_to_end(&mut bytes).map(|_| bytes);
            let _ = tx.send(result);
        });
        rx
    });
    let read_result = match reader {
        Some(rx) => match rx.recv_timeout(GIT_DEADLINE) {
            Ok(result) => Some(result),
            Err(_) => {
                kill_and_reap(&mut child);
                return Err(JevError::git_timeout(GIT_DEADLINE.as_secs()));
            }
        },
        None => None,
    };
    let bytes = match &read_result {
        Some(Ok(bytes)) => bytes.clone(),
        _ => Vec::new(),
    };
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        kill_and_reap(&mut child);
        return Err(JevError::input_too_large(
            max_bytes,
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        ));
    }
    let exit = wait_bounded(&mut child, started, GIT_DEADLINE)?;
    match read_result {
        Some(Ok(_)) if exit.success() => Ok(GitBlob { sha, bytes }),
        Some(Err(error)) => Err(JevError::io_failed("git cat-file output", &error)),
        _ => Err(JevError::input_invalid(format!(
            "{path} at {rev} is not a file",
            path = path.as_str()
        ))),
    }
}

/// Returns the blob id of `bytes` in the repository at `root`.
///
/// Runs `git hash-object -t blob --stdin --no-filters` and writes `bytes`
/// to its stdin, so git never sees a filesystem path (Windows verbatim
/// `\\?\` paths cannot reach it), clean filters (`autocrlf`, attributes) do
/// not apply, and the id is that of exactly these bytes. The repository's
/// object format (SHA-1 or SHA-256) decides the id length. Nothing is
/// written to the object database. Callers bound `bytes` (for example by
/// [`MAX_INPUT_BYTES`]); the output read is bounded here.
///
/// # Errors
///
/// - `INPUT_INVALID` (`io_failed`) when git cannot be started or its pipes
///   fail;
/// - `EXTRACTION_FAILED` when git exits non-zero (the detail carries the
///   start of git's stderr) or prints something other than an object id.
pub fn hash_blob(root: &RepoRoot, bytes: &[u8]) -> Result<BlobSha, JevError> {
    let mut command = git(root.as_path());
    command
        .args(["hash-object", "-t", "blob", "--stdin", "--no-filters"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| JevError::io_failed("git", &error))?;
    let (stdin, stdout, stderr) =
        match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
            (Some(stdin), Some(stdout), Some(stderr)) => (stdin, stdout, stderr),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(JevError::extraction_failed(
                    "git hash-object pipes are unavailable",
                ));
            }
        };
    // Each pipe has its own thread, so a full stdout or stderr buffer cannot
    // stall the stdin write (or the reverse). All waits share one deadline.
    let started = Instant::now();
    let input = bytes.to_vec();
    let (written_tx, written_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut stdin = stdin;
        let _ = written_tx.send(stdin.write_all(&input));
    });
    let errors = spawn_bounded_read(stderr, STDERR_DETAIL_BYTES);
    let id = spawn_bounded_read(stdout, OBJECT_ID_OUTPUT_BYTES);
    let expired = || Err::<(), JevError>(JevError::git_timeout(GIT_DEADLINE.as_secs()));
    let left = || GIT_DEADLINE.saturating_sub(started.elapsed());
    let outcome = (|| {
        let written = written_rx
            .recv_timeout(left())
            .or_else(|_| expired().map(|()| Ok(())))?;
        let id = id
            .recv_timeout(left())
            .or_else(|_| expired().map(|()| Ok(Vec::new())))?;
        let detail = errors
            .recv_timeout(left())
            .or_else(|_| expired().map(|()| Ok(Vec::new())))?;
        Ok::<_, JevError>((written, id, detail))
    })();
    let (written, id, detail) = match outcome {
        Ok(parts) => parts,
        Err(error) => {
            kill_and_reap(&mut child);
            return Err(error);
        }
    };
    let exit = wait_bounded(&mut child, started, GIT_DEADLINE)?;
    let id = id.map_err(|error| JevError::io_failed("git hash-object output", &error))?;
    if !exit.success() || written.is_err() {
        let detail = detail.unwrap_or_default();
        let reason = String::from_utf8_lossy(detail.trim_ascii());
        let reason = reason.lines().next().unwrap_or("no stderr");
        return Err(JevError::extraction_failed(format!(
            "git hash-object failed: {reason}"
        )));
    }
    std::str::from_utf8(id.trim_ascii())
        .ok()
        .and_then(|text| BlobSha::try_from(text).ok())
        .ok_or_else(|| {
            JevError::extraction_failed("git hash-object returned an unexpected object id")
        })
}

/// [`bounded_read`] on its own thread.
fn spawn_bounded_read(
    reader: impl Read + Send + 'static,
    limit: u64,
) -> Receiver<io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(bounded_read(reader, limit));
    });
    rx
}

/// Reads up to `limit` bytes of `reader`, then drains the rest so the
/// writer never blocks on a full pipe.
fn bounded_read(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut reader = reader;
    let mut kept = Vec::new();
    (&mut reader).take(limit).read_to_end(&mut kept)?;
    io::copy(&mut reader, &mut io::sink())?;
    Ok(kept)
}

/// `INPUT_MISSING` when `rev` has a tree (the path is absent), otherwise
/// `INPUT_INVALID` (the revision does not resolve).
fn classify_missing(
    root: &RepoRoot,
    rev: &Rev,
    path: &RepoRelativePath,
) -> Result<JevError, JevError> {
    let tree = format!("{}^{{tree}}", rev.as_str());
    let mut probe = git(root.as_path());
    probe.args(["cat-file", "-e", "--end-of-options", &tree]);
    let exists = run(probe)?;
    Ok(if exists.status.success() {
        JevError::input_missing(path.as_str())
    } else {
        JevError::input_invalid(format!("revision {rev} was not found"))
    })
}

/// The guard inside `read_at_rev` behind `Rev::parse`. A `Rev` that git would
/// read as an option cannot be built through the public API, so this test
/// builds one with the crate-private `Rev::unchecked`.
#[cfg(all(test, feature = "git"))]
mod guard_tests {
    use super::*;

    #[test]
    fn test_read_at_rev_refuses_an_option_shaped_revision_without_running_git() {
        let dir = tempfile::tempdir().expect("temp");
        let target = dir.path().join("FILE");
        let rev = Rev::unchecked(format!("--output={}", target.display()));
        let root = RepoRoot::new(dir.path());
        let path = RepoRelativePath::try_from("src/a.py").expect("path");
        let error = read_at_rev(&root, &rev, &path, 1024).expect_err("option");
        assert_eq!(error.code(), "INPUT_INVALID");
        assert!(!target.exists(), "no file is created");
        let empty = read_at_rev(&root, &Rev::unchecked(""), &path, 1024).expect_err("empty");
        assert_eq!(empty.code(), "INPUT_INVALID");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[expect(
        clippy::disallowed_methods,
        reason = "process spawn: a stand-in for git in the deadline tests"
    )]
    fn child(program: &str, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command.args(args);
        command
    }

    fn hung_child() -> Command {
        child("sleep", &["30"])
    }

    /// A child that outlives the deadline is killed and reaped, and the call
    /// is a typed `EXTRACTION_TIMEOUT`, long before the child would exit.
    #[test]
    fn test_hung_git_is_killed_at_the_deadline() {
        let error =
            run_with_deadline(hung_child(), Duration::from_millis(200)).expect_err("deadline");
        assert_eq!(error.code(), "EXTRACTION_TIMEOUT");
        assert!(matches!(error, JevError::GitTimeout { .. }), "{error:?}");
    }

    #[test]
    fn test_fast_child_completes_within_the_deadline() {
        let output = run_with_deadline(child("true", &[]), GIT_DEADLINE).expect("runs");
        assert!(output.status.success());
    }

    /// A pipe that never reaches EOF (its writer stays open) ends the wait
    /// with the typed `pipe did not close` error. The test holds the writer
    /// itself, so no process is involved and nothing races the deadline.
    #[test]
    fn test_pipe_held_open_is_a_typed_error() {
        let (reader, writer) = std::os::unix::net::UnixStream::pair().expect("socket pair");
        let rx = drain(reader);
        let error =
            collect(Some(rx), Instant::now() + Duration::from_millis(50)).expect_err("held pipe");
        // `pipe did not close` is an `io::ErrorKind::TimedOut` on `git output`;
        // `JevError` keeps the kind and drops the OS message.
        assert!(
            matches!(
                &error,
                JevError::IoFailed { what, kind }
                    if what == "git output" && *kind == io::ErrorKind::TimedOut
            ),
            "{error:?}"
        );
        // The writer is open for the whole assert.
        drop(writer);
    }

    /// A pipe whose writer wrote and closed hands over its bytes even when the
    /// deadline is short, as long as it has not passed.
    #[test]
    fn test_closed_pipe_yields_its_bytes_before_the_deadline() {
        use std::io::Write;
        let (reader, mut writer) = std::os::unix::net::UnixStream::pair().expect("socket pair");
        let rx = drain(reader);
        writer.write_all(b"hello").expect("write");
        drop(writer);
        let bytes = collect(Some(rx), Instant::now() + Duration::from_secs(30)).expect("bytes");
        assert_eq!(bytes, b"hello");
    }
}
