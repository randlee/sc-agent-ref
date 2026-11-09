# sc-agent

`sc-ai` is the JSON-first CLI for Jev check parity (`run`, `dry-run`, `ask`, and `stream`).

## Install

From source (works today):

```bash
just install-sc-ai
```

The same install without `just`:

```bash
cargo install --path crates/sc-ai --locked
```

Available after the first release (nothing has been published yet; see
[`CHANGELOG.md`](CHANGELOG.md)):

```bash
# crates.io
cargo install sc-ai --locked

# Homebrew (installs sc-ai and sc-treesitter)
brew install randlee/tap/sc-ai
```

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
