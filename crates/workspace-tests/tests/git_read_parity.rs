//! `sc_agent_types::git::read_at_rev` is the one revision reader
//! (RBQA-F006): its bytes match `git cat-file blob`, the blob id the
//! `sc-treesitter` binary reports, and what `sc-jev-check` puts in a payload;
//! and an option-looking `--rev` creates no file.

#[path = "../../../tests/support/deadline.rs"]
mod deadline;
#[path = "../../../tests/support/git_env.rs"]
mod git_env;
mod support;

use std::path::Path;
use std::process::{Command, Output};

use deadline::spawn_with_deadline;
use sc_agent_types::git::{MAX_INPUT_BYTES, read_at_rev};
use sc_agent_types::{BackendId, BackendSet, RepoRelativePath, RepoRoot, Rev};
use sc_jev_check::run::CheckSource;
use sc_jev_host::compose::{ComposeInputs, run_from};
use sc_jev_host::paths::PathPolicy;
use support::bin_path::bin_path;
use tempfile::TempDir;

const SOURCE: &str = "def f(x):\n    match x:\n        case 1:\n            return Refusal(x)\n";

fn git(cwd: &Path, args: &[&str]) -> Output {
    let output = spawn_with_deadline(git_env::git_command(cwd).args(args), None);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn repo() -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix("workspace-tests-git-")
        .tempdir()
        .expect("temp");
    std::fs::create_dir(dir.path().join("src")).expect("src");
    std::fs::write(dir.path().join("src/a.py"), SOURCE).expect("a.py");
    std::fs::write(
        dir.path().join("questions.json"),
        r#"{"schema":"plan-anchoring/questions/1","version":1,"questions":{"relevant":{"type":"choice","instructions":"Source:\n{live}","criteria":{"no":"No.","yes":"Yes."}}}}"#,
    )
    .expect("questions");
    std::fs::write(
        dir.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n\n[inputs.live]\nkind = \"file\"\npath = \"src/a.py\"\n",
    )
    .expect("check");
    git(dir.path(), git_env::SHA1_INIT);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "one"]);
    // The worktree copy differs from HEAD, so a reader that ignored the
    // revision would be caught.
    std::fs::write(dir.path().join("src/a.py"), "changed = True\n").expect("dirty");
    dir
}

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: runs the built sc-treesitter binary, the unit under test"
)]
fn sc_treesitter(binary: &Path) -> Command {
    Command::new(binary)
}

fn extract(dir: &Path, rev: &str) -> Output {
    let binary = bin_path("sc-treesitter", "sc-treesitter");
    let mut command = sc_treesitter(&binary);
    git_env::isolate_git(&mut command)
        .args(["extract", "--language", "python", "--query", "branches"])
        .arg(format!("--rev={rev}"))
        .arg("src/a.py")
        .current_dir(dir);
    spawn_with_deadline(&mut command, None)
}

#[test]
fn test_read_at_rev_matches_git_cat_file() {
    let dir = repo();
    let root = RepoRoot::new(dir.path());
    let path = RepoRelativePath::try_from("src/a.py").expect("path");
    let blob = read_at_rev(
        &root,
        &Rev::parse("HEAD").expect("rev"),
        &path,
        MAX_INPUT_BYTES,
    )
    .expect("read_at_rev");
    let expected = git(dir.path(), &["cat-file", "blob", "HEAD:src/a.py"]).stdout;
    assert_eq!(blob.bytes(), expected.as_slice());
    assert_eq!(blob.bytes(), SOURCE.as_bytes());
    let id = git(dir.path(), &["rev-parse", "HEAD:src/a.py"]).stdout;
    assert_eq!(blob.sha().as_str(), String::from_utf8_lossy(&id).trim());

    // The sc-treesitter binary reads the same blob.
    let output = extract(dir.path(), "HEAD");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).expect("subjects");
    assert_eq!(document["source"]["blob_sha"], blob.sha().as_str());
}

/// `sc-jev-check` reads a `file` input at a revision through the same reader.
#[test]
fn test_jev_check_reads_the_same_bytes_at_rev() {
    let dir = repo();
    let inputs = ComposeInputs::new(dir.path().to_path_buf(), PathPolicy::cli())
        .repo_root(RepoRoot::new(dir.path()))
        .rev(Rev::parse("HEAD").expect("rev"))
        .backends(BackendSet::try_from(vec![BackendId::Jev]).expect("set"))
        .dry_run(true);
    let source = CheckSource::check_file(&RepoRoot::new(dir.path()), Path::new("check.toml"))
        .expect("source");
    let response = run_from(source, inputs).expect("dry run at HEAD");
    let payload = response.payload().expect("dry-run payload").to_string();
    assert!(payload.contains("Refusal(x)"), "{payload}");
    assert!(!payload.contains("changed = True"), "{payload}");
}

/// An option-looking `--rev` fails in argument parsing: git never runs, so no
/// file appears.
#[test]
fn test_rev_option_injection_creates_no_file() {
    let dir = repo();
    let target = dir.path().join("pwned.txt");
    let output = extract(dir.path(), &format!("--output={}", target.display()));
    assert_eq!(output.status.code(), Some(2));
    assert!(!target.exists(), "--output=FILE as --rev created a file");
}
