# sc-agent

`sc-ai` is the JSON-first CLI for Jev check parity (`run`, `dry-run`, `ask`, `stream`, and `models`).

## Install

The 0.1.0 channels, as listed in [`CHANGELOG.md`](CHANGELOG.md). Each works
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

From source, which works at any time:

```bash
just install-sc-ai
# or, without just:
cargo install --path crates/sc-ai --locked
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

## Stability

All crates are 0.x. What is and is not promised for the Jev wire types, the
session-log record, the subjects schema and the Python `sc_jev_py` API is in
[`docs/stability.md`](docs/stability.md).

## Session logs

The default log root is `<repo-root>/.sc-ai`. Add `.sc-ai` to your project's
`.gitignore`: session logs and session files live there and are not meant to be
committed. `--log-dir <path>` overrides the log root.
When that flag is absent, a non-empty `SC_LOG_ROOT` environment variable is
the log root.

The v2 file sink writes `<log-root>/logs/sc-ai.log.jsonl`. Session files
default to `<repo-root>/.sc-ai/sessions` (`--sessions-dir` overrides that
directory). Session ids are ULIDs (26 characters); a UUID is refused.
Each send (`judge` or `ask`, `ok` or `failed`) writes one line whose `fields`
object is one session-log row: the session id, `repo` (the main repository), `project`,
`backend`, `model_id`, `payload_sha256`, `questions_sha256`, `request_sha256` and the typed
answers. Logs do not record API keys, prompt text or request bodies.
`--project <TEXT>` (on `jev run` and `jev ask`) labels that call's rows; the text is stored as given and is
not scrubbed, so put no secret in it.

### General log

A second, separate log records what an operator needs to diagnose a run:
`<log-root>/logs/sc-ai-general.log.jsonl`. It holds start and end of
operations, backend selection and the resolved model, path resolution, the
credential source (a name such as `env`, never the value), busy retries and
waits, extraction and git steps, session store actions, flush failures, and
failures by typed error id. It never records API keys, prompt text, session
state or provider bodies. A dry run opens neither log. `sc-ai` has no logger
flag: it writes both default logs under `--log-dir`. Rust and Python callers
can inject their own session and general loggers (`ComposeInputs::session_log`
and `general_log`; Python `run_check(session_log=, general_log=)`), each
falling back to the default.

## Commands

```bash
sc-ai jev run --repo-root . --check check.toml --backend jev
sc-ai jev dry-run --repo-root . --check check.toml
sc-ai jev models
sc-ai jev ask --session-id <ULID> --text "why?" --project my-project
sc-ai jev stream --repo-root . --concurrency 8 < requests.jsonl > answers.jsonl
```

`--backend` takes `jev`, `haiku` or `luna` (each the latest model) or a known
model id of one of them such as `claude-haiku-5-5` (that backend pinned to that
model); `backends=` in Python and `backends` in `check.toml` take the same.
New models are added without recompiling in `~/.sc-agent/models.toml`
(`[[model]]` rows with `id`, `alias`, `released`); `jev models` and
`sc_jev_py.models()` list the resolved models and whether each came from the
crate or the file. A local-only test fails when a provider lists a model newer
than the crate table (`cargo test -p sc-agent-types --test model_freshness`).

`jev run` answers one request per process. `jev stream` answers many through
one process: JSONL in (stdin or `--input FILE`), one JSONL answer per request on
stdout, in completion order, not input order. Each input line is
`{"id":"q-1","request":{...}}` (the `--request-json` document) and its answer
line echoes the id: `{"id":"q-1","ok":true,"envelope":{...}}`, or
`{"id":"q-1","ok":false,"error":{...}}` with the stderr error body above.
`ok` is false whenever `error` is present, and a request in which only some
backends failed has both `envelope` (the answers that succeeded; read it even
when `ok` is false) and `error`. `id` is `null` only when no usable id could be
read from the line. A failed request does not stop the stream. Requests may keep arriving while
answers come back, so an early answer can become a follow-up request. The
stream ends when the input is closed, the queue is empty and the workers are
idle. `--concurrency N` (1 to 64, default 8) sets the worker threads; the queue
is bounded, so a slow reader slows the input. The exit code is 0, or 3 when
every failure is retryable, else 2. The default of 8 was tested only against
scripted and loopback backends, not live providers. Python has the same
capability: `sc_jev_py.run_many` and `sc_jev_py.DecisionStream`.

`jev models` and `jev stream` have their own outputs: `models` prints an
envelope whose `data` is `{"models_file","models_file_present","models"}` and
`stream` prints one JSONL answer per request (above), not the envelope
below.

`jev ask` needs a saved session: only `haiku` and `luna` runs save one. A
`jev` run returns `session_id` as `null` and has nothing to ask about. The
CLI's log root is `<repo>/.sc-ai` and Python's is `~/.sc-agent/log` (it also
reads `SC_AGENT_LOG_DIR`; the CLI does not), so a CLI `ask` does not find a
Python session unless both are given the same sessions directory
([`docs/stability.md`](docs/stability.md)).

For `run`, `dry-run` and `ask`, success writes one JSON object to stdout:

```json
{"version":"1","operation":"jev.run","data":{}}
```

`data` includes the Jev answer fields and `session_id`. `data.readback`
echoes the repo root, `check.toml`, and `--llm-system-prompt-file` path that
the command consumed. Failure exits 2, or 3 for a retryable error, and writes one JSON line to stderr:
`{"error","kind","message","suggested_action","retryable","schema_version"}`
plus `retry_after_seconds`, `field` and `details` when the error has them.
`error` is a stable id, `kind` its broad class, `message` what failed,
`suggested_action` what to do next; `schema_version` versions that error
shape and is `1`. Field list and per-id `details`: [`docs/stability.md`](docs/stability.md). When any backend fails, stdout carries the
envelope (the successful answers, if any, and each failed backend's error),
stderr carries the first failure, and the exit code is 2, or 3 when every
failed backend's error is retryable (retry only the failed backends).

## Development

CI jobs and the local command for each one: [`docs/development/ci.md`](docs/development/ci.md).
Release dry-run: [`docs/development/sc-publish.md`](docs/development/sc-publish.md).
