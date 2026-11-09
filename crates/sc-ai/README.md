# sc-ai

The `sc-ai` command-line tool for Jev check parity: `run`, `dry-run`, and `ask`.

Session logs and session files default to `<repo-root>/.sc-ai`; add `.sc-ai` to your project's `.gitignore`. A second log, `logs/sc-ai-general.log.jsonl` beside the session log, records operator diagnostics (no keys, prompt text or provider bodies). The CLI keeps these default loggers and `--log-dir`; it has no logger flag.

Install it with `cargo install sc-ai --locked`, or from the Homebrew, Scoop, and WinGet channels.

Part of the [sc-agent](https://github.com/randlee/sc-agent) workspace. See the
repository `docs/` for architecture and requirements. Licensed under MIT.
