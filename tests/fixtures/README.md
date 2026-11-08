# Test fixtures

Two fixtures let another Jev implementation (a Python client, say) prove it
builds the same bytes and digests as this workspace. Both are checked by the
Rust tests on every CI run, so they stay correct.

## 1. Canonical payload bytes: `jev/cross_impl`

The canonical judge payload (ADR-SAG-0002) is JSON with sorted object keys,
compact separators (`,` and `:`), UTF-8, and non-ASCII characters left as
UTF-8 (not `\uXXXX`). Control characters are escaped; `/` is not.
`payload_sha256` is the SHA-256 (lowercase hex) of those bytes. (It was named `prompt_sha256` before the phase-end rename; the digest values are unchanged.)

| File | Role |
| --- | --- |
| `jev/cross_impl/logical.json` | Input document, with hard escape and sort cases (non-BMP and `U+FFFF` keys, control characters, DEL) |
| `jev/cross_impl/canonicalize.py` | Python reference: reads `logical.json`, writes the canonical bytes to stdout |
| `jev/cross_impl/reference.bytes` | The expected canonical bytes, exactly |
| `jev/cross_impl/reference.sha256` | The expected digest of `reference.bytes` |

To prove conformance, canonicalize `logical.json` with your implementation and
require:

1. Your bytes equal `reference.bytes` byte for byte.
2. The SHA-256 of your bytes equals the line in `reference.sha256`.

Check the reference itself (from the repository root):

```bash
python3 tests/fixtures/jev/cross_impl/canonicalize.py > /tmp/out.bytes
cmp /tmp/out.bytes tests/fixtures/jev/cross_impl/reference.bytes && echo same
python3 -c "import hashlib; print(hashlib.sha256(open('tests/fixtures/jev/cross_impl/reference.bytes','rb').read()).hexdigest())"
cat tests/fixtures/jev/cross_impl/reference.sha256
```

The last two commands print the same digest
(`e608bcbd1dcfb360315c8e889f34311ce67d645806896b329d56dcff5f8db3a7`). The Rust
side of this check is `cargo test -p workspace-tests --test jev_cross_impl`.
Do not run `canonicalize.py --write` unless the owner decided the canonical
form changes: it rewrites both reference files.

## 2. Subjects to payload: `cross-impl/branches`

A producer-to-consumer fixture: a Python source file, the `sc/subjects/1`
document `sc-treesitter` emits for it, and the extract arguments.

| File | Role |
| --- | --- |
| `cross-impl/branches/hook.py` | Source read at `HEAD` |
| `cross-impl/branches/subjects.json` | Expected `sc/subjects/1` document, byte for byte |
| `cross-impl/branches/request.json` | Expected producer argv and capture names |

To prove a consumer conforms, build the check payload from `subjects.json` and
compare digests. For a question `branches` with instructions
`Which branch applies?\n\n{branches}` and choices `no`, `yes`:

1. Join the `text` of every subject in `subjects.json` with `\n`.
2. Build `{"questions":[{"id":"branches","prompt":"Which branch applies?\n\n<joined>","choices":["no","yes"]}],"state":[]}`.
   Choices are sorted.
3. Canonicalize it as in section 1. The SHA-256 must be
   `d3b5e61a488d62ae83aa662bf96faa776dc708e7d9365ea94e23def14d7090c3`
   (204 bytes).

The reference implementation produces the same digest as `payload_sha256` of
`sc-ai jev dry-run` over a `check.toml` with `[inputs.branches]`
`kind = "subjects"`, `path = "subjects.json"`. The Rust side is
`cargo test -p sc-jev-check --test cross_impl_branches`.

A producer implementation conforms when, run on `hook.py` with the arguments in
`request.json`, it emits `subjects.json` byte for byte. The compiled-in query is
pinned by `producer.query_sha256` in `subjects.json`: the SHA-256 (lowercase
hex) of the query text after CRLF and bare CR are normalized to LF, which is
the SHA-256 of the file bytes since the query contains no CR. The rules for line
numbers, byte spans, ids and ordering are in
[`docs/sc-agent-types/subjects-schema.md`](../../docs/sc-agent-types/subjects-schema.md).

## 3. Question and request digests: `questions_sha256`, `request_sha256`

`questions_sha256` identifies a question set, so runs of the same questions
against different state can be compared. It is the SHA-256 (lowercase hex,
computed with `sc-sha`) of the question set alone. It
appears in `sc-ai jev run` and `dry-run` output (`data.questions_sha256`, right
after `payload_sha256`), on the run result (`RunResponse::questions_sha256`) and
in Python (`CheckResult.questions_sha256`, `build_request(...)["questions_sha256"]`).
sc-agent builds no cache. The public functions are
`sc_decision_schema::jev::v1::questions_sha256` and
`PayloadBundle::questions_sha256`.

The bytes hashed are the form plan-anchor's `questions_digest` uses
(`json.dumps(questions, sort_keys=True, separators=(",", ":"),
ensure_ascii=False)` encoded as UTF-8), over this object:

- One key per question id; the value is `{"type", "instructions", "criteria"}`.
- `type` is `choice`, `score` or `noul`; `instructions` is the question prompt
  exactly as in the payload (after input substitution).
- `criteria` maps each choice to its description: the tuned description when the
  question has one, else the choice label itself. A `noul` question has no
  `criteria` key.
- Keys are sorted at every level (by Unicode code point, as Python sorts), with
  no whitespace; non-ASCII characters are written as UTF-8, not `\uXXXX`;
  control characters are escaped (`\n`, `\u001f`).

The digest does not cover `state` or the model, so it is not `payload_sha256`.
Descriptions are covered: tuning a description changes the digest. The
question order in the payload does not matter. Example (checked by
`cargo test -p sc-decision-schema --test jev -- test_questions_sha256_is_pinned_to_the_documented_bytes`):

```text
{"plain":{"criteria":{"no":"no","yes":"yes"},"instructions":"Plain?","type":"choice"},"relevant":{"criteria":{"no":"It does not.","yes":"It applies."},"instructions":"Does it apply? é\n","type":"choice"},"urgent":{"instructions":"Urgent?","type":"noul"}}
```

SHA-256: `ad8e0a8cebca33763fd388d96ce884066babd5aa09372aba4d55a40d0c1da8b7`.

Differences from plan-anchor to know: plan-anchor hashes the gate questions
without its extra questions and keys by its own ids; sc-agent hashes the payload's
questions. A question set whose criteria equal the choice labels hashes the
same as one with no descriptions.

Question sets loaded from `plan-anchoring/questions/1` keep their `criteria`
values as descriptions. The pinned `cross-impl/branches` digest above uses a
question set whose criteria equal their keys (no descriptions), so its bytes did
not change.

### `request_sha256`

`request_sha256` identifies an identical request, so a consumer can substitute a
cached response. It is an object keyed by backend id (`jev`, `anthropic_haiku`,
`openai_luna`) in `data.request_sha256`, `RunResponse::request_sha256()`,
`CheckResult.request_sha256` and `build_request(...)["request_sha256"]`. Each
value is the SHA-256 (lowercase hex, `sc-sha`) of the exact UTF-8 body of the
first HTTP request that backend is sent, for the model it runs with:

- Jev: the body `{"model":...,"state":...,"questions":...}`. `jev` is sent as
  `jev-latest`.
- Anthropic Haiku and OpenAI Luna: the token-count body, which is sent before
  the judge message and is the one the session log records. It holds the model,
  the canonical payload and the LLM system prompt.

It is computed before sending, with no API key, and equals the
`request_sha256` the session-log row records for the same send (tests:
`cargo test -p sc-jev-check --test request_digest`). It changes when a
question, the state, the model or the LLM system prompt changes. `payload_sha256`
covers none of the model or the system prompt, so it cannot key a cache.

A backend is left out of the object when its bytes cannot be known before a
send or nothing would be sent: an LLM backend after another backend in
`chain_context` mode (its system prompt carries the earlier answers), or an LLM
backend given a question it refuses (`noul`). The size limit is not applied: a
dry run reports the digest of an over-limit body next to its `over_limit` flag.
