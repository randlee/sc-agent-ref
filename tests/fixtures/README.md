# Test fixtures

Two fixtures let another Jev implementation (a Python client, say) prove it
builds the same bytes and digests as this workspace. Both are checked by the
Rust tests on every CI run, so they stay correct.

## 1. Canonical payload bytes: `jev/cross_impl`

The canonical judge payload (ADR-SAG-0002) is JSON with sorted object keys,
compact separators (`,` and `:`), UTF-8, and non-ASCII characters left as
UTF-8 (not `\uXXXX`). Control characters are escaped; `/` is not.
`prompt_sha256` is the SHA-256 (lowercase hex) of those bytes.

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

The reference implementation produces the same digest as `prompt_sha256` of
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
