# sc-agent-secrets — architecture

**Boundary:** `agent-secrets`  
**Depends on:** `sc-agent-types`

## API

```rust
use sc_agent_types::{BackendKeyId, JevError, RepoRoot};

pub fn resolve_key(id: BackendKeyId, repo_root: &RepoRoot) -> Result<ResolvedApiKey, JevError>;

pub fn resolve_key_with(
    id: BackendKeyId,
    repo_root: &RepoRoot,
    env: &impl EnvSource,
    keyring: &impl KeyringSource,
    account: impl AsRef<str>,
) -> Result<ResolvedApiKey, JevError>;
```

Callers pass the same git toplevel the facade uses for extractors; this crate
does **not** run `git`. Precedence: **process environment → repo-root `.env` →
keyring** (ADR-SAG-0003). `resolve_key` uses the process environment, the OS
user as the keyring account, and the platform keyring (`keyring` crate:
Keychain, Credential Manager, or Secret Service). `resolve_key_with` is the
injectable entry. `EnvSource` and `KeyringSource` are the open injection
traits (ADR-SAG-0020). The in-memory `MapEnv` and `MapKeyring` doubles are
crate-private test code; the crate has no `test-util` feature.

Every source's value is sealed by `ResolvedApiKey::try_new`, which is the one
normalization rule set: trim; empty is `KEY_MISSING`; interior whitespace or a
control character is `CONFIG_INVALID` with a credential-store hint. A blank
value counts as absent, so the next source is tried.

The keyring service is the environment variable name. Anthropic lookups honor
`ANTHROPIC_KEYCHAIN_SERVICE` from the process environment only: a repo-root
`.env` is repository content and must not choose which keychain item is sent
to Anthropic as the key.

`ResolvedApiKey` lives in `sc-agent-types` so a sending transport can accept
it without depending on this crate. `Debug` and `Display` redact the secret.
`as_str` is the only read, and only that transport may call it. This crate
emits no log lines (NFR-SAG-0002, ADR-SAG-0013).
A platform keyring read has a 15 s deadline (`KEYRING_READ_TIMEOUT`; ADR-SAG-0020
agt-2a3 amendment, "Keyring read deadline"): a keystore
that waits for a prompt it cannot show (a locked macOS Keychain in a headless
run) returns `IO_FAILED` with `io_kind` `other` (not retryable: a retry blocks again
until the keystore is reachable) and an action that points at the process
environment and `.env`, instead of hanging. The blocked read stays on its own
detached thread; it holds no state, and later reads in the same process return
the same error at once (so several backends cost one wait and strand at most
one thread).

Unreadable `.env` or keyring failures are `IO_FAILED` (a runtime I/O failure,
not a configuration error) with a sanitized cause in `what`, the I/O class in
`details.io_kind` and a recovery action, never secret bytes. A miss is `KEY_MISSING`
(ADR-SAG-0001).

## Repo-root `.env` dialect

The parser is a small, documented subset of the common dotenv format. It never
writes the process environment.

- One `KEY=value` per line, optionally prefixed by `export `. Keys match
  `[A-Za-z_][A-Za-z0-9_]*`; other lines, blank lines, and lines starting with
  `#` are skipped. A later line for the same key wins. A UTF-8 BOM is ignored.
- **Unquoted** values are trimmed. A `#` preceded by whitespace starts a
  comment to the end of the line: `KEY=v # prod` is `v`. A `#` with no
  whitespace before it is part of the value: `KEY=v#1` is `v#1`.
- **`'single'`** quotes keep the text verbatim: `KEY='a # b'` is `a # b`.
- **`"double"`** quotes unescape `\n`, `\r`, `\t`, and `\<char>`:
  `KEY="v # not comment"` is `v # not comment`.
- A quoted value may be followed by whitespace and a `#` comment. Any other
  text after the closing quote makes the whole value unquoted text.
- No variable expansion, no command substitution, no multi-line values.

## General log (agt-9eh)

`resolve_key_logged` and `resolve_key_with_logged` report the winning source
(`env`, `dotenv` or `keyring`) and the key id at `debug`, and the typed error id
at `warn` on a miss. The value is never logged. `resolve_key` and
`resolve_key_with` are unchanged and log nothing.
