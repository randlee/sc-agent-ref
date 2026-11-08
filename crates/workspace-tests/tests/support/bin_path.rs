//! Builds a workspace binary and returns its path (ADR-SAG-0028).
//!
//! Stable Cargo has no `CARGO_BIN_EXE_*` across packages and no artifact
//! dependencies, so the helper runs
//! `cargo build --release -p <pkg> --features cli --target-dir <isolated> --message-format=json`
//! and reads the executable from the `compiler-artifact` message.
//!
//! `<isolated>` is `$CARGO_TARGET_DIR/workspace-tests-bin`, or
//! `<workspace root>/target/workspace-tests-bin` when `CARGO_TARGET_DIR` is
//! unset. It is never the directory `cargo test` uplifts a binary into: that
//! binary may be built with `test-util` on, and sharing a path would make the
//! parity golden and `test_util_off_binary` depend on test order (RBQA-F103 (b)). The
//! helper therefore runs release-shaped binaries only. The directory is fixed
//! rather than a tempdir per test so binaries are not rebuilt from scratch by
//! every test. The build runs through the bounded runner of the including
//! target (`mod deadline;`, `tests/support/deadline.rs`) with its own cold-build
//! deadline.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

/// Longest a cold release build of a binary (mimalloc's C build, tree-sitter
/// grammars) may take. Well above the 60 s runner default, bounded so a stuck
/// build cannot hang CI (FTQ-001).
const BIN_BUILD_DEADLINE: Duration = Duration::from_secs(20 * 60);

/// Serializes builds: two targets in one process may ask for the same package.
static BUILD_LOCK: Mutex<()> = Mutex::new(());

/// The workspace root: `crates/workspace-tests` is two levels down, and
/// `cargo test` runs each target with the package directory as cwd.
///
/// Built from `CARGO_MANIFEST_DIR` by dropping two components, never
/// canonicalized: on Windows `canonicalize` yields a verbatim `\\?\D:\...`
/// path, which mimalloc's `cl.exe` build cannot use as a `--target-dir`.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/workspace-tests is two levels below the workspace root")
        .to_path_buf()
}

/// Absolute and never verbatim (see [`workspace_root`]).
fn isolated_target_dir() -> PathBuf {
    let base = match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => workspace_root().join(dir),
        None => workspace_root().join("target"),
    };
    base.join("workspace-tests-bin")
}

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: cargo build of the binary under test, via the CARGO the test run uses"
)]
fn cargo() -> Command {
    Command::new(env!("CARGO"))
}

/// Builds `package` in release mode with `--features cli` and returns the
/// path of its executable named `bin`.
///
/// # Panics
///
/// When the build fails or reports no executable for `bin`.
pub fn bin_path(package: &str, bin: &str) -> PathBuf {
    let _guard = BUILD_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut build = cargo();
    build
        .current_dir(workspace_root())
        .args([
            "build",
            "--locked",
            "--release",
            "-p",
            package,
            "--features",
            "cli",
        ])
        .arg("--target-dir")
        .arg(isolated_target_dir())
        .arg("--message-format=json");
    let output = crate::deadline::spawn_with_limit(&mut build, None, BIN_BUILD_DEADLINE);
    assert!(
        output.status.success(),
        "cargo build -p {package} failed (a held Cargo file lock shows as \"Blocking waiting for file lock\"):\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf8 cargo output")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter(|message| message["target"]["name"] == bin)
        .filter(|message| {
            message["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
        })
        .filter_map(|message| message["executable"].as_str().map(PathBuf::from))
        .next_back()
        .unwrap_or_else(|| panic!("no executable for {package}/{bin} in cargo's output"))
}
