//! Local freshness check of the `MODELS` table (agt-akp, randlee/sc-agent#48).
//!
//! Fails when a provider lists a newer model in a tracked line than the
//! crate's table, so the crate must be upgraded. It compares the crate table
//! only: `~/.sc-agent/models.toml` does not silence it (that file is the
//! stopgap, this test is the reminder).
//!
//! Never runs in CI: the `CI` environment variable is set there, and each
//! test prints that reason and returns. A missing CLI or key skips with a
//! printed reason (run with `--nocapture` to read it). A CLI that is present
//! but whose output no longer parses FAILS with a message saying which
//! format changed: agents maintain these parsers.
//!
//! Sources, none needing an API key except Jev's:
//!
//! - OpenAI lines luna, terra, sol, astra: `codex debug models` (JSON; reads
//!   `slug`, `visibility`, `supported_in_api`, `upgrade`; hidden entries are
//!   ignored; the slug must match `gpt-N(.M)-<line>`). `codex debug` is not a
//!   stable interface (randlee/sc-agent#48).
//! - Anthropic lines haiku, sonnet, opus, fable: `claude -p hi --model
//!   <alias> --output-format stream-json --verbose`, reading `model` from the
//!   `system` `init` event. The process is killed right after that event, so
//!   no model call is made.
//! - Jev: `GET https://api.typesafe.ai/v1/models` with `TYPESAFE_API_KEY`
//!   (through `curl`, the key on stdin, never argv).
//!
//! Run: `cargo test -p sc-agent-types --test model_freshness -- --nocapture`.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use sc_agent_types::ModelAliases;

#[path = "../../../tests/support/deadline.rs"]
#[expect(
    dead_code,
    reason = "the shared runner is included whole; this target uses only spawn_with_limit"
)]
mod deadline;

const CODEX_LINES: [&str; 4] = ["luna", "terra", "sol", "astra"];
const CLAUDE_LINES: [&str; 4] = ["haiku", "sonnet", "opus", "fable"];
const CLI_DEADLINE: Duration = Duration::from_secs(60);
const JEV_MODELS_URL: &str = "https://api.typesafe.ai/v1/models";

/// A model version: `[6, 1]` for `gpt-6.1-sol`, `[5, 5]` for `claude-opus-5-5`.
type Version = Vec<u32>;

/// The reason this run must not talk to a provider, if any.
fn skip_reason() -> Option<&'static str> {
    std::env::var_os("CI").map(|_| "the CI environment variable is set; this test never runs in CI")
}

fn skip(why: &str) {
    println!("model_freshness: skipped: {why}");
}

fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate: PathBuf = dir.join(program);
        candidate.is_file() || candidate.with_extension("exe").is_file()
    })
}

/// The version after `prefix` in `id`, taking dotted (`6.1`) or dashed (`5-5`)
/// groups of at most two digits, and requiring `suffix` to follow exactly
/// (`-sol`, or nothing). A date suffix (`-20260901`) is not a version group.
fn version_of(id: &str, prefix: &str, suffix: &str) -> Option<Version> {
    let rest = id.strip_prefix(prefix)?;
    let rest = rest.strip_suffix(suffix).or_else(|| {
        // Claude ids may carry a context marker or a date after the version.
        suffix.is_empty().then_some(rest)
    })?;
    let mut version = Vec::new();
    for group in rest.split(['.', '-']) {
        // `1[1m]` is version group 1 with a context marker after it.
        let digits: String = group.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() || digits.len() > 2 || version.len() >= 2 {
            break;
        }
        version.push(digits.parse().ok()?);
        if digits.len() != group.len() {
            break;
        }
    }
    (!version.is_empty()).then_some(version)
}

fn crate_model(alias: &str) -> String {
    ModelAliases::builtin()
        .latest(alias)
        .unwrap_or_else(|| panic!("the crate has no alias {alias}"))
        .to_string()
}

fn show(version: &[u32]) -> String {
    version
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: the local freshness test runs the provider CLIs"
)]
fn command(program: &str) -> Command {
    Command::new(program)
}

#[test]
fn openai_models_are_not_newer_than_the_crate_table() {
    if let Some(why) = skip_reason() {
        return skip(why);
    }
    if !on_path("codex") {
        return skip("the codex CLI is not on PATH");
    }
    let output = deadline::spawn_with_limit(
        command("codex").args(["debug", "models"]),
        None,
        CLI_DEADLINE,
    );
    assert!(
        output.status.success(),
        "`codex debug models` failed ({}): update this test for the new CLI (randlee/sc-agent#48): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let listed = codex_versions(&output.stdout).unwrap_or_else(|why| {
        panic!(
            "`codex debug models` output no longer parses ({why}); update codex_versions in \
             crates/sc-agent-types/tests/model_freshness.rs (randlee/sc-agent#48)"
        )
    });
    let mut newer = Vec::new();
    for line in CODEX_LINES {
        let have = version_of(&crate_model(line), "gpt-", &format!("-{line}"))
            .unwrap_or_else(|| panic!("crate model for {line} is not gpt-N(.M)-{line}"));
        for (slug, version) in listed.iter().filter(|(slug, _)| slug.ends_with(line)) {
            if version > &have {
                newer.push(format!(
                    "{slug} (crate has {line} {}): upgrade the crate",
                    show(&have)
                ));
            }
        }
    }
    assert!(
        newer.is_empty(),
        "OpenAI lists newer models:\n{}",
        newer.join("\n")
    );
}

/// The visible, API-supported, not-superseded `gpt-N(.M)-<line>` slugs of
/// `codex debug models`, with their versions.
fn codex_versions(stdout: &[u8]) -> Result<Vec<(String, Version)>, String> {
    let json: serde_json::Value =
        serde_json::from_slice(stdout).map_err(|error| format!("not JSON: {error}"))?;
    let models = json
        .get("models")
        .and_then(serde_json::Value::as_array)
        .ok_or("no `models` array")?;
    let mut found = Vec::new();
    for model in models {
        let slug = model
            .get("slug")
            .and_then(serde_json::Value::as_str)
            .ok_or("a model has no `slug`")?;
        let visibility = model
            .get("visibility")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{slug}: no `visibility`"))?;
        let in_api = model
            .get("supported_in_api")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| format!("{slug}: no `supported_in_api`"))?;
        let superseded = model
            .get("upgrade")
            .is_some_and(|upgrade| !upgrade.is_null());
        if visibility != "list" || !in_api || superseded {
            continue;
        }
        for line in CODEX_LINES {
            if let Some(version) = version_of(slug, "gpt-", &format!("-{line}")) {
                found.push((slug.to_owned(), version));
            }
        }
    }
    if found.is_empty() {
        return Err("no gpt-N(.M)-<line> model is listed".to_owned());
    }
    Ok(found)
}

#[test]
fn anthropic_models_are_not_newer_than_the_crate_table() {
    if let Some(why) = skip_reason() {
        return skip(why);
    }
    if !on_path("claude") {
        return skip("the claude CLI is not on PATH");
    }
    let mut newer = Vec::new();
    for line in CLAUDE_LINES {
        let answered = claude_init_model(line);
        let prefix = format!("claude-{line}-");
        let listed = version_of(&answered, &prefix, "").unwrap_or_else(|| {
            panic!(
                "claude --model {line} reported model {answered:?}, not {prefix}N-M; update \
                 claude_init_model in crates/sc-agent-types/tests/model_freshness.rs \
                 (randlee/sc-agent#48)"
            )
        });
        let have = version_of(&crate_model(line), &prefix, "")
            .unwrap_or_else(|| panic!("crate model for {line} is not {prefix}N-M"));
        if listed > have {
            newer.push(format!(
                "claude --model {line} answers {answered} (crate has {line} {}): upgrade the crate",
                show(&have)
            ));
        }
    }
    assert!(
        newer.is_empty(),
        "Anthropic lists newer models:\n{}",
        newer.join("\n")
    );
}

/// Starts `claude -p hi --model <alias> --output-format stream-json
/// --verbose`, returns the `model` of the `system` `init` event and kills
/// the process at once, before any model call.
#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: the local freshness test runs the claude CLI"
)]
fn claude_init_model(alias: &str) -> String {
    let mut child = Command::new("claude")
        .args([
            "-p",
            "hi",
            "--model",
            alias,
            "--output-format",
            "stream-json",
            "--verbose",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("cannot start claude: {error}"));
    let stdout = child.stdout.take().expect("piped stdout");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut outcome = Err(format!(
        "claude --model {alias} printed no `system` `init` event"
    ));
    while let Ok(line) = receiver.recv_timeout(CLI_DEADLINE) {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let is_init = event["type"] == "system" && event["subtype"] == "init";
        if is_init {
            outcome = event["model"].as_str().map(str::to_owned).ok_or_else(|| {
                format!("claude --model {alias}: the init event has no string `model`")
            });
            break;
        }
    }
    // Kill first (no model call), then reap; both are prompt after a kill.
    let _ = child.kill();
    let _ = child.wait();
    outcome.unwrap_or_else(|why| {
        panic!(
            "{why}; update claude_init_model in crates/sc-agent-types/tests/model_freshness.rs \
             (randlee/sc-agent#48)"
        )
    })
}

#[test]
fn jev_models_are_not_newer_than_the_crate_table() {
    if let Some(why) = skip_reason() {
        return skip(why);
    }
    let Some(key) = std::env::var("TYPESAFE_API_KEY")
        .ok()
        .filter(|key| !key.is_empty())
    else {
        return skip("TYPESAFE_API_KEY is not set");
    };
    if key.contains(['"', '\n', '\\']) {
        return skip("TYPESAFE_API_KEY has characters this test will not pass to curl");
    }
    if !on_path("curl") {
        return skip("curl is not on PATH");
    }
    // The key goes through curl's config on stdin, never argv.
    let config = format!("header = \"Authorization: Bearer {key}\"\nurl = \"{JEV_MODELS_URL}\"\n");
    let output = deadline::spawn_with_limit(
        command("curl").args([
            "--silent",
            "--show-error",
            "--max-time",
            "30",
            "--write-out",
            "\n%{http_code}",
            "--config",
            "-",
        ]),
        Some(config.as_bytes()),
        CLI_DEADLINE,
    );
    if !output.status.success() {
        return skip(&format!(
            "the Jev models request failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let (body, status) = text.rsplit_once('\n').unwrap_or(("", ""));
    if status.trim() != "200" {
        return skip(&format!("GET /v1/models answered HTTP {}", status.trim()));
    }
    let models: serde_json::Value = serde_json::from_str(body).unwrap_or_else(|error| {
        panic!(
            "GET {JEV_MODELS_URL} body is not JSON ({error}); update the Jev check in \
             crates/sc-agent-types/tests/model_freshness.rs (randlee/sc-agent#48)"
        )
    });
    let released = jev_latest_release(&models).unwrap_or_else(|| {
        panic!(
            "GET {JEV_MODELS_URL} lists no `jev-latest` with a `release_date`; update the Jev \
             check in crates/sc-agent-types/tests/model_freshness.rs (randlee/sc-agent#48)"
        )
    });
    let crate_row = ModelAliases::builtin()
        .resolved()
        .into_iter()
        .find(|row| row.alias() == "jev")
        .expect("crate has the jev alias");
    assert!(
        released.as_str() <= crate_row.released(),
        "Jev's jev-latest was released {released}, after the crate's {} ({}): upgrade the crate",
        crate_row.model(),
        crate_row.released()
    );
    // The list may also name versions (`jev-N.N.N`); none may beat the crate.
    let have = version_of(&crate_model("jev"), "jev-", "").expect("crate jev model is jev-N.N.N");
    for version in jev_versions(body) {
        assert!(
            version <= have,
            "Jev lists jev-{} (crate has jev-{}): upgrade the crate",
            show(&version),
            show(&have)
        );
    }
    let _ = std::io::stdout().flush();
}

/// The `YYYY-MM-DD` of the `jev-latest` entry's `release_date`.
/// `GET /v1/models` answers `{"models":[{"name","description","release_date"}]}`.
fn jev_latest_release(models: &serde_json::Value) -> Option<String> {
    let entry = models
        .get("models")?
        .as_array()?
        .iter()
        .find(|entry| entry["name"] == "jev-latest")?;
    let date = entry["release_date"].as_str()?.get(..10)?;
    Some(date.to_owned())
}

/// Every `jev-N.N.N` token in `body`, as `[N, N, N]`. The body's JSON shape is
/// not relied on: the ids are found wherever they are.
fn jev_versions(body: &str) -> Vec<Version> {
    body.match_indices("jev-")
        .filter_map(|(at, _)| {
            let rest = &body[at + 4..];
            let end = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(rest.len());
            let parts: Vec<u32> = rest[..end]
                .split('.')
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            (parts.len() == 3).then_some(parts)
        })
        .collect()
}

#[test]
fn the_parsers_read_the_known_shapes() {
    assert_eq!(version_of("gpt-6.1-sol", "gpt-", "-sol"), Some(vec![6, 1]));
    assert_eq!(version_of("gpt-6-luna", "gpt-", "-luna"), Some(vec![6]));
    assert_eq!(version_of("gpt-6.1-sol-mini", "gpt-", "-sol"), None);
    assert_eq!(
        version_of("claude-opus-5-5", "claude-opus-", ""),
        Some(vec![5, 5])
    );
    assert_eq!(
        version_of("claude-opus-5-5-20260901", "claude-opus-", ""),
        Some(vec![5, 5])
    );
    assert_eq!(
        version_of("claude-fable-5-1[1m]", "claude-fable-", ""),
        Some(vec![5, 1])
    );
    assert!(vec![6, 1] > vec![6]);
    let jev: serde_json::Value = serde_json::from_str(
        r#"{"models":[{"name":"jev-preview","release_date":"2026-10-14T20:14:00+00:00"},{"name":"jev-latest","release_date":"2026-10-22T02:00:00+00:00"}]}"#,
    )
    .expect("json");
    assert_eq!(jev_latest_release(&jev).as_deref(), Some("2026-10-22"));
    assert!(vec![5, 10] > vec![5, 9]);
    assert_eq!(
        jev_versions(r#"{"data":[{"id":"jev-1.14.0"},{"id":"jev-1.9.2"},{"id":"jev-latest"}]}"#),
        vec![vec![1, 14, 0], vec![1, 9, 2]]
    );
    let codex = br#"{"models":[
        {"slug":"gpt-6.1-sol","visibility":"list","supported_in_api":true,"upgrade":null},
        {"slug":"gpt-7-sol","visibility":"hide","supported_in_api":true,"upgrade":null},
        {"slug":"gpt-8-sol","visibility":"list","supported_in_api":true,"upgrade":{"model":"x"}},
        {"slug":"gpt-reserve","visibility":"hide","supported_in_api":true,"upgrade":null}]}"#;
    assert_eq!(
        codex_versions(codex).expect("parses"),
        vec![("gpt-6.1-sol".to_owned(), vec![6, 1])]
    );
    assert!(codex_versions(b"not json").is_err());
    assert!(codex_versions(br#"{"models":[{"slug":"gpt-6-sol"}]}"#).is_err());
}
