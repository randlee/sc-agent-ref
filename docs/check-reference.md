# check.toml and request reference

The files below are the tested examples in
[`tests/fixtures/jev/quickstart`](../tests/fixtures/jev/quickstart); a test
runs them through `sc-ai jev dry-run` and fails when this page stops showing
them verbatim. Unknown keys are refused everywhere (`deny_unknown_fields`).

## check.toml

```toml
[questions]
path = "questions.json"

[inputs.note]
kind = "file"
path = "notes/readme.txt"
```

- `[questions]` is required and is a table with a repo-relative `path`.
  `questions = "questions.json"` fails with `expected struct RawPath`.
- `[state]` (optional) is the same table, naming a `plan-anchoring/state/1`
  file of earlier answers.
- `[llm_judge] system_prompt_file` (optional) replaces the packaged system
  prompt.
- `backends` (optional list: `jev`, `haiku`, `luna` or a model id) and
  `[models]` (alias to model id) pick backends.
- `[inputs.<name>]` fills `{name}` in a question's `instructions`. `kind` is
  `file` (`path`), `table-column` (`path`, `column`), `subjects` or `command`
  (`command = [argv...]`, run without a shell; see
  [`sc-jev-check/extraction.md`](sc-jev-check/extraction.md)).

`jev dry-run` runs `command` inputs too, so a `check.toml` from a pull request
is code that runs in CI. Review it before running it, or avoid `command`.

## Question file for check.toml (`plan-anchoring/questions/1`)

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

A question is `type` (`choice`), `instructions` and `criteria` (choice id to
its description). Top-level `gate`, `notes` and `roles` are accepted and not
sent. Any other key, such as `role` on a question, is refused.

## Request document (`--request-json`, `request_json=`)

A different format, used instead of `check.toml`, not with it. Questions are an
array with `id`, `prompt` and `choices`; `state` and `backends` are optional.
Do not put `system_prompt` in it.

```json
{
  "questions": [
    {"id": "relevant", "prompt": "Does the note apply?", "choices": ["no", "yes"]}
  ],
  "state": [],
  "backends": ["jev"]
}
```

```bash
sc-ai jev dry-run --repo-root . --request-json request.json
sc-ai jev run --repo-root . --request-json - < request.json
```

## Python

`sc_jev_py` runs the same engine and the same key lookup (process environment,
repo-root `.env`, keyring) in process:

```python
import sc_jev_py

result = sc_jev_py.run_check("check.toml", dry_run=True, repo_root=".")
print(result.payload_sha256)
result = sc_jev_py.run_check(
    request_json=open("request.json").read(), backends=["jev"], dry_run=True, repo_root="."
)
```

Failures raise `sc_jev_py.JevError` with the CLI error fields (`code`,
`message`, `suggested_action`, `retryable`). Python writes logs under
`~/.sc-agent/log`, not the CLI's `<repo>/.sc-ai`; see
[`stability.md`](stability.md) before using `ask` across the two.
