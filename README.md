# sc-agent

`sc-ai` is the JSON-first CLI for Jev check parity (`run`, `dry-run`, and `ask`).

## Install

```bash
just install-sc-ai
```

The same install without `just`:

```bash
cargo install --path crates/sc-ai --locked
```

## Session logs

The default log root is `<repo-root>/.sc-ai`. `--log-dir <path>` overrides it.
When that flag is absent, a non-empty `SC_LOG_ROOT` environment variable is
the log root.

The v2 file sink writes `<log-root>/logs/sc-ai.log.jsonl`. Session files
default to `<repo-root>/.sc-ai/sessions` (`--sessions-dir` overrides that
directory). Session ids are ULIDs (26 characters); a UUID is refused.
Each send (`judge` or `ask`, `ok` or `failed`) writes one line whose `fields`
object is one session-log row: the session id, `repo` (the main repository), `project`,
`backend`, `model_id`, `prompt_sha256`, `request_sha256` and the typed
answers. Logs do not record API keys, prompt text or request bodies.
`--project <TEXT>` (on `jev run` and `jev ask`) labels that call's rows; the text is stored as given and is
not scrubbed, so put no secret in it.

## Commands

```bash
sc-ai jev run --repo-root . --check check.toml --backend jev
sc-ai jev dry-run --repo-root . --check check.toml
sc-ai jev ask --session-id <ULID> --text "why?" --project my-project
```

Success writes one JSON object to stdout:

```json
{"version":"1","operation":"jev.run","data":{}}
```

`data` includes the Jev answer fields and `session_id`. `data.readback`
echoes the repo root, `check.toml`, and `--llm-system-prompt-file` path that
the command consumed. Failure exits 2 and writes `{"error","message"}` to
stderr. `error` is a `JevError` code.

## Development

CI jobs and the local command for each one: [`docs/development/ci.md`](docs/development/ci.md).
Release dry-run: [`docs/development/sc-publish.md`](docs/development/sc-publish.md).
