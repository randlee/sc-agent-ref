//! Source scans (ADR-SAG-0020 r-3 read-site convention, ADR-SAG-0026
//! environment ownership). They read files and link nothing.
//!
//! **Needles are built from fragments** so this file never contains a
//! literal it hunts, and the `rg` gate needs no exclusion for it. Prose here
//! says "allow attribute", never the literal.
//!
//! Attributes are read whole: each outer or inner attribute from its `#` to
//! the matching `]`, across lines, with whitespace removed, before any match.
//! Comments and string contents are skipped while finding the end, so a
//! bracket in a `reason` text cannot end an attribute early. Every scan covers
//! every target under `crates/**/*.rs` (`src`, `tests`, `benches`,
//! `examples`, `build.rs`).

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .canonicalize()
        .expect("workspace root")
}

/// Every `.rs` file under `crates/` and `tests/support/`, as a workspace-relative `/` path and its
/// text, skipping build output.
fn rust_files() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name != "target") {
                    walk(&path, out);
                }
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let root = root();
    let mut paths = Vec::new();
    walk(&root.join("crates"), &mut paths);
    // The shared test helpers included by `#[path]` into many crates' tests
    // (the bounded runner, git isolation) are scanned under the test-file rule.
    walk(&root.join("tests/support"), &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "no Rust files found under crates/");
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .expect("under root")
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&path).expect("utf8 source");
            (relative, text)
        })
        .collect()
}

/// One attribute: its text without whitespace (strings intact) and the same
/// text with string contents removed.
struct Attribute {
    /// `true` for an inner attribute (`#![..]`), which covers a whole crate or
    /// module.
    inner: bool,
    /// Whitespace removed.
    compact: String,
    /// Whitespace removed and every string literal emptied to `""`.
    code: String,
}

/// Skips a string literal starting at `chars[i] == '"'`; returns the index
/// after the closing quote.
fn skip_string(chars: &[char], mut i: usize) -> usize {
    i += 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

/// Skips a raw string starting at `r`; returns `None` when it is not one.
fn skip_raw_string(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut hashes = 0;
    while chars.get(i) == Some(&'#') {
        hashes += 1;
        i += 1;
    }
    if chars.get(i) != Some(&'"') {
        return None;
    }
    i += 1;
    while i < chars.len() {
        if chars[i] == '"' && (0..hashes).all(|n| chars.get(i + 1 + n) == Some(&'#')) {
            return Some(i + 1 + hashes);
        }
        i += 1;
    }
    Some(i)
}

/// Skips a comment starting at `chars[i]`; returns the index after it.
fn skip_comment(chars: &[char], i: usize) -> Option<usize> {
    if chars[i] != '/' {
        return None;
    }
    match chars.get(i + 1) {
        Some('/') => Some(
            chars[i..]
                .iter()
                .position(|&c| c == '\n')
                .map_or(chars.len(), |n| i + n),
        ),
        Some('*') => {
            let mut depth = 0;
            let mut j = i;
            while j < chars.len() {
                if chars[j] == '/' && chars.get(j + 1) == Some(&'*') {
                    depth += 1;
                    j += 2;
                } else if chars[j] == '*' && chars.get(j + 1) == Some(&'/') {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        return Some(j);
                    }
                } else {
                    j += 1;
                }
            }
            Some(chars.len())
        }
        _ => None,
    }
}

/// Skips a char literal starting at `'`; `None` for a lifetime.
fn skip_char(chars: &[char], i: usize) -> Option<usize> {
    if chars.get(i + 1) == Some(&'\\') {
        let close = chars[i + 2..].iter().position(|&c| c == '\'')?;
        return Some(i + 3 + close);
    }
    (chars.get(i + 2) == Some(&'\'')).then_some(i + 3)
}

/// Every attribute in `source`, outside comments and string literals.
fn attributes(source: &str) -> Vec<Attribute> {
    let chars: Vec<char> = source.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some(next) = skip_comment(&chars, i) {
            i = next;
            continue;
        }
        match chars[i] {
            '"' => i = skip_string(&chars, i),
            'r' if matches!(chars.get(i + 1), Some('"' | '#')) => {
                i = skip_raw_string(&chars, i).unwrap_or(i + 1);
            }
            '\'' => i = skip_char(&chars, i).unwrap_or(i + 1),
            '#' => {
                let inner = chars.get(i + 1) == Some(&'!');
                let open = i + 1 + usize::from(inner);
                if chars.get(open) != Some(&'[') {
                    i += 1;
                    continue;
                }
                let (end, attribute) = read_attribute(&chars, open, inner);
                found.push(attribute);
                i = end;
            }
            _ => i += 1,
        }
    }
    found
}

/// Reads the attribute whose `[` is at `open`; returns the index after the
/// matching `]`.
fn read_attribute(chars: &[char], open: usize, inner: bool) -> (usize, Attribute) {
    let mut compact = String::new();
    let mut code = String::new();
    let mut depth = 0;
    let mut i = open;
    while i < chars.len() {
        if let Some(next) = skip_comment(chars, i) {
            i = next;
            continue;
        }
        let c = chars[i];
        match c {
            '"' => {
                let end = skip_string(chars, i);
                compact.extend(&chars[i..end]);
                code.push_str("\"\"");
                i = end;
                continue;
            }
            '[' => depth += 1,
            ']' => depth -= 1,
            _ => {}
        }
        if !c.is_whitespace() {
            compact.push(c);
            code.push(c);
        }
        i += 1;
        if depth == 0 {
            break;
        }
    }
    (
        i,
        Attribute {
            inner,
            compact,
            code,
        },
    )
}

/// The comma-separated items of every `name(...)` call in `code`, where
/// `name` is a lint-level word such as the expect word.
fn call_items(code: &str, name: &str) -> Vec<Vec<String>> {
    let mut calls = Vec::new();
    let mut from = 0;
    while let Some(offset) = code[from..].find(name) {
        let start = from + offset + name.len();
        from = start;
        let mut depth = 1;
        let mut items = vec![String::new()];
        for c in code[start..].chars() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                ',' if depth == 1 => {
                    items.push(String::new());
                    continue;
                }
                _ => {}
            }
            items.last_mut().expect("item").push(c);
        }
        calls.push(items);
    }
    calls
}

/// The word that opens an expect attribute, built from fragments.
fn expect_word() -> String {
    ["exp", "ect("].concat()
}

fn allow_word() -> String {
    ["al", "low("].concat()
}

fn disallowed_methods_lint() -> String {
    ["clippy::disallowed", "_methods"].concat()
}

/// ADR-SAG-0020: no attribute is an allow attribute, including inside a
/// conditional attribute; no expect names the blanket lint groups.
#[test]
fn test_no_allow_attributes() {
    let allow = allow_word();
    let expect = expect_word();
    let blanket = ["warnings", "clippy::all", "clippy::restriction"];
    let mut failures = Vec::new();
    for (path, text) in rust_files() {
        for attribute in attributes(&text) {
            if attribute.code.contains(&allow) {
                failures.push(format!("{path}: {}", attribute.compact));
            }
            for items in call_items(&attribute.code, &expect) {
                if items.iter().any(|item| blanket.contains(&item.as_str())) {
                    failures.push(format!("{path}: blanket expect {}", attribute.compact));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "forbidden attributes:\n{}",
        failures.join("\n")
    );
}

/// Files whose production code may carry an expect for the disallowed-method
/// lint: the secret read site and the process-spawn sites ADR-SAG-0028 lists.
const EXPECT_SITES: &[&str] = &[
    // The `ResolvedApiKey::as_str` read (ADR-SAG-0020).
    "crates/sc-agent-http/src/ureq_transport.rs",
    // The argv-only `git` spawn.
    "crates/sc-agent-types/src/git.rs",
    // The extractor spawns the configured argv.
    "crates/sc-jev-check/src/extract/command.rs",
    // The `#[cfg(test)]` `test_support::git_command` helper builds fixture
    // repos with an isolated config (a-10, CPR-301 option 2).
    "crates/sc-jev-check/src/extract/mod.rs",
];

fn is_test_file(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.get(2) == Some(&"tests")
        || path.ends_with("/tests.rs")
        || path.starts_with("tests/support/")
}

/// ADR-SAG-0020 r-3 / ADR-SAG-0028: an expect naming the disallowed-method
/// lint appears only at the listed sites, or in a test file whose item-level
/// expect has a reason naming a process spawn; a spawn-site test file never
/// contains the secret type, so an expect cannot cover a secret read.
#[test]
fn test_disallowed_methods_expect_sites() {
    let lint = disallowed_methods_lint();
    let secret = ["Resolved", "ApiKey"].concat();
    let mut failures = Vec::new();
    let mut seen_sites = Vec::new();
    for (path, text) in rust_files() {
        let hits: Vec<Attribute> = attributes(&text)
            .into_iter()
            .filter(|attribute| attribute.code.contains(&lint))
            .filter(|attribute| {
                call_items(&attribute.code, &expect_word())
                    .iter()
                    .flatten()
                    .any(|item| item == &lint)
            })
            .collect();
        if hits.is_empty() {
            continue;
        }
        if EXPECT_SITES.contains(&path.as_str()) {
            seen_sites.push(path);
            continue;
        }
        if !is_test_file(&path) {
            failures.push(format!("{path}: expect outside the listed sites"));
            continue;
        }
        if text.contains(&secret) {
            failures.push(format!("{path}: a spawn-site test file mentions {secret}"));
        }
        for attribute in hits {
            if attribute.inner {
                failures.push(format!("{path}: crate-level expect {}", attribute.compact));
            }
            if !attribute.compact.to_ascii_lowercase().contains("spawn") {
                failures.push(format!(
                    "{path}: reason does not name a process spawn: {}",
                    attribute.compact
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // The scan sees the real read site: a lexer that missed it would pass
    // everything above vacuously.
    assert!(
        seen_sites.contains(&"crates/sc-agent-http/src/ureq_transport.rs".to_owned()),
        "the ureq_transport expect was not found; sites seen: {seen_sites:?}"
    );
}

/// The text of `file` under the workspace root.
fn read(file: &str) -> String {
    std::fs::read_to_string(root().join(file)).unwrap_or_else(|error| panic!("{file}: {error}"))
}

/// Lines of the `[section]` table in a TOML `text` (up to the next header).
fn section<'a>(text: &'a str, header: &str) -> Vec<&'a str> {
    text.lines()
        .skip_while(|line| line.trim() != header)
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .collect()
}

/// RBQA-F102 (b), (c); RBQA-F103 (c): the mechanisms that make the scans
/// above and clippy effective are present.
#[test]
fn test_lint_mechanism_present() {
    let clippy = read("clippy.toml");
    for path in [
        "sc_agent_types::ResolvedApiKey::as_str",
        "std::process::Command::new",
    ] {
        assert!(
            clippy
                .lines()
                .any(|line| line.contains("path =") && line.contains(path)),
            "clippy.toml must list {path} under disallowed-methods"
        );
    }

    let manifest = read("Cargo.toml");
    let lints = section(&manifest, "[workspace.lints.clippy]");
    for level in [
        "allow_attributes = \"deny\"",
        "allow_attributes_without_reason = \"deny\"",
    ] {
        assert!(
            lints.iter().any(|line| line.trim() == level),
            "[workspace.lints.clippy] must set {level}"
        );
    }
    let workspace = section(&manifest, "[workspace]");
    assert!(
        workspace
            .iter()
            .any(|line| line.trim() == "resolver = \"2\""),
        "[workspace] must set resolver = \"2\" (the dev-only test-util seam depends on it)"
    );
    let members: Vec<&str> = workspace
        .iter()
        .map(|line| line.trim())
        .skip_while(|line| !line.starts_with("members"))
        .skip(1)
        .take_while(|line| *line != "]")
        .collect();
    assert!(!members.is_empty(), "[workspace] members not found");
    for member in members {
        let name = member.trim_end_matches(',').trim_matches('"');
        assert!(
            name.strip_prefix("crates/")
                .is_some_and(|rest| !rest.is_empty() && !rest.contains('/')),
            "every member is crates/<name>: {member}"
        );
    }

    // The clippy command that `just lint` runs.
    let lint = read("sc-lint.toml");
    let command = lint
        .split("[[tool.sc-lint.lint]]")
        .find(|block| block.contains("name = \"clippy\""))
        .and_then(|block| block.lines().find(|line| line.starts_with("command")))
        .expect("sc-lint.toml has a clippy lint command");
    for needle in [
        "\"--workspace\"",
        "\"--all-targets\"",
        "\"-D\"",
        "\"warnings\"",
    ] {
        assert!(
            command.contains(needle),
            "clippy command lacks {needle}: {command}"
        );
    }
}

/// Every crate inherits the workspace lints and adds no `[lints.*]` table.
#[test]
fn test_every_crate_inherits_workspace_lints() {
    let mut checked = 0;
    for entry in std::fs::read_dir(root().join("crates")).expect("crates dir") {
        let dir = entry.expect("entry").path();
        let manifest = dir.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let name = dir
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(&manifest).expect("manifest");
        let lints = section(&text, "[lints]");
        assert!(
            lints.iter().any(|line| line.trim() == "workspace = true"),
            "crates/{name}/Cargo.toml must set [lints] workspace = true"
        );
        assert!(
            !text.lines().any(|line| line.trim().starts_with("[lints.")),
            "crates/{name}/Cargo.toml must not add a [lints.*] table"
        );
        checked += 1;
    }
    assert!(checked >= 10, "found only {checked} crates");
}

/// Files under `crates/*/src/**` that may read the process environment: the
/// key owner, the child-environment allowlist, the log and session path
/// owner, and git's ceiling-directory rule for repo-root discovery (a git
/// setting, not a log, session or API-key variable).
const ENV_OWNERS: &[&str] = &[
    "crates/sc-jev-check/src/extract/sanitize.rs",
    "crates/sc-jev-check/src/extract/command.rs",
    "crates/sc-jev-host/src/paths.rs",
    "crates/sc-jev-check/src/run/mod.rs",
];

/// ADR-SAG-0026 environment ownership. A text scan: an aliased import evades
/// it, and review covers that.
#[test]
fn test_env_reads_only_in_owners() {
    let needles: Vec<String> = ["var(", "var_os(", "vars(", "vars_os("]
        .iter()
        .map(|call| format!("env::{call}"))
        .collect();
    let mut failures = Vec::new();
    for (path, text) in rust_files() {
        let in_src = path.split('/').nth(2) == Some("src");
        if !in_src || is_test_file(&path) {
            continue;
        }
        let owner =
            path.starts_with("crates/sc-agent-secrets/src/") || ENV_OWNERS.contains(&path.as_str());
        if owner {
            continue;
        }
        let code: String = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for needle in &needles {
            if code.contains(needle.as_str()) {
                failures.push(format!("{path}: reads the environment ({needle})"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The attribute reader sees the forms the `rg` gate misses: spaced,
/// multi-line, inside a conditional attribute, and with a bracket inside a
/// string; and it ignores comments and string contents.
#[test]
fn test_attribute_reader_sees_spaced_and_multiline_forms() {
    let allow = ["al", "low"].concat();
    let expect = ["exp", "ect"].concat();
    let sample = format!(
        "#[ {allow} (dead_code) ]\nfn a() {{}}\n#[cfg_attr(\n    test,\n    {allow}(unused),\n)]\nfn b() {{}}\n#[{expect}(warnings, reason = \"a ] bracket\")]\nfn c() {{}}\n// #[{allow}(in_comment)]\nconst S: &str = \"#[{allow}(in_string)]\";\n"
    );
    let found = attributes(&sample);
    assert_eq!(
        found.len(),
        3,
        "{:?}",
        found.iter().map(|a| &a.compact).collect::<Vec<_>>()
    );
    let needle = allow_word();
    assert!(found[0].code.contains(&needle), "{}", found[0].code);
    assert!(found[1].code.contains(&needle), "{}", found[1].code);
    let calls = call_items(&found[2].code, &expect_word());
    assert!(calls[0].iter().any(|item| item == "warnings"), "{calls:?}");
}
