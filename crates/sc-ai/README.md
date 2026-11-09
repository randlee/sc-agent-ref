# sc-ai

The `sc-ai` command-line tool for Jev check parity: `run`, `dry-run`, `ask`, `stream` (many requests through one process, JSONL in and out) and `models`.

Session logs and session files default to `<repo-root>/.sc-ai`; add `.sc-ai` to your project's `.gitignore`. A second log, `logs/sc-ai-general.log.jsonl` beside the session log, records operator diagnostics (no keys, prompt text or provider bodies). The CLI keeps these default loggers and `--log-dir`; it has no logger flag.

## Install

The 0.1.0 channels, as listed in the [changelog](https://github.com/randlee/sc-agent/blob/develop/CHANGELOG.md). Each works
once its 0.1.0 release has been published there:

```bash
# crates.io
cargo install sc-ai --locked

# Homebrew (installs sc-ai and sc-treesitter)
brew install randlee/tap/sc-ai

# Scoop (Windows; installs sc-ai and sc-treesitter)
scoop bucket add randlee https://github.com/randlee/scoop-bucket
scoop install sc-ai

# WinGet (Windows; sc-ai only)
winget install randlee.sc-ai

# PyPI: the Python wheel (sc_jev_py), not the CLI
pip install sc-jev-py
```

## Quick start

Build prerequisites for `cargo install`: Rust 1.94.1 or newer (the workspace
`rust-version`) and a C compiler (`mimalloc` and `ring` compile C).
`cargo install sc-ai` installs only `sc-ai`, which has no tree-sitter
grammars. For `sc-treesitter`, whose grammars compile C too, run
`cargo install sc-treesitter --locked` or use Homebrew.

In a git repository, write a `check.toml`, the `questions.json` it names, and
the file it reads:

```toml
[questions]
path = "questions.json"

[inputs.note]
kind = "file"
path = "notes/readme.txt"
```

```json
{
  "schema": "plan-anchoring/questions/1",
  "version": 1,
  "questions": {
    "relevant": {
      "type": "choice",
      "instructions": "Does the note apply?\n\n{note}",
      "criteria": {"no": "The note does not apply.", "yes": "The note applies."}
    }
  }
}
```

Build the request without sending it, then send it:

```bash
sc-ai jev dry-run --repo-root . --check check.toml
sc-ai jev run --repo-root . --check check.toml --backend jev
```

`dry-run` needs no key, but it runs any `command` input in `check.toml`, so a
`check.toml` from a pull request is code execution in CI: review it first.
`run` needs the backend's key: `TYPESAFE_API_KEY` for `jev`,
`ANTHROPIC_API_KEY` for `haiku`, `OPENAI_API_KEY` for `luna`. `sc-ai` reads it
from the process environment, then `.env` in the repo root (skipped when the
variable is set in the environment, even empty), then the platform keyring
(service = the variable name, account = your OS user). With no keystore, such
as a headless Linux container, set the variable or use `.env`. If no source
has a key, `run` fails with `KEY_MISSING`, whose `suggested_action` names the
variable. Only the process environment can set `ANTHROPIC_KEYCHAIN_SERVICE`
(an Anthropic keyring service override); a repo `.env` cannot.

The `check.toml` fields, both question file formats (the `plan-anchoring`
file above and the `--request-json` document with `choices` and `prompt`), a
worked `--request-json` example and the same flow in Python (`run_check`) are
in [`docs/check-reference.md`](https://github.com/randlee/sc-agent/blob/develop/docs/check-reference.md).
A TLS-inspecting proxy is not supported: the HTTP client trusts only the
bundled web PKI roots.

`jev ask` needs a saved session: only `haiku` and `luna` runs save one; a `jev` run has `session_id` null. See the [repository README](https://github.com/randlee/sc-agent#readme) for `stream`, `models`, output shapes and session logs.

Part of the [sc-agent](https://github.com/randlee/sc-agent) workspace. See the
repository `docs/` for architecture and requirements. Licensed under MIT.
