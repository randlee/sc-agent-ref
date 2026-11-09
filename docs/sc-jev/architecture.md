# sc-jev — architecture

**Boundary:** `jev`  
**Depends on:** `sc-agent-types`, `sc-decision-schema`, `sc-agent-http` (r-3), `serde`, `serde_json`, `toml` (API key resolved by **`sc-jev-check`** and passed at `JevClient` construction)

## North star

**`sc-jev` is the only TypeSafe Jev client.** Every Jev HTTP operation the
product needs is implemented here. No other crate sends Jev API traffic.

Phase A lands the **check/judge** HTTP vertical slice. Additional endpoints add
as modules without new top-level client crates.

## Module layout (Phase A)

| Module | Purpose |
| --- | --- |
| `client` | `JevClient<T: Transport = UreqTransport>`: builds the `HttpRequest` (endpoint, `Content-Type`, `Authorization: Bearer` as a `SecretHeader`) and maps the reply |
| `check` | Check/judge wire body (`sc_decision_schema::jev::v1::CheckRequest` for the pinned model), `input_sha256` recovery by inverting the body, and the reply parser (`CheckResponse`) |
| `config` | `JevClientConfig`, packaged `jev.toml` (host, path, attempts, timeout); the model is a `ModelId` from the `sc-agent-types` catalog, set by the backend factory, not read from `jev.toml` |

From r-3, HTTP transport, busy retry, the 8 MiB response cap and redaction
are `sc-agent-http` (ADR-SAG-0026, ADR-SAG-0006 r-3 amendment). This crate is
a wire-format crate: it still owns every TypeSafe Jev API operation
(ADR-SAG-0012). `JevClient::with_default_transport(key)` keeps the r-1
signature and builds `UreqTransport` with a 10 s connect timeout and the
`jev.toml` request timeout. `with_busy_policy` and `with_cancel` replace the
policy and the cancel token; tests use the `sc-agent-http` `test-util`
doubles through the dev-dependency.

## Non-responsibilities

- `check.toml`, extractors, sessions, `run_check` (**`sc-jev-check`**).
- Haiku/Luna (**`sc-jev-backends`**).
- CLI, PyO3.

## Check flow (Phase A)

1. Caller (facade) builds `JudgeRequest` with canonical `PayloadBundle`.
2. `JevClient` as `JudgeBackend` deserializes the payload bytes (the one
   validator), builds the documented Jev body `{model, state, questions}`
   with `CheckRequest::from_canonical` and POSTs `to_body()`. The body is
   built once; busy retries resend it unchanged. A body over the descriptor's
   `request_limit_bytes` is `INPUT_TOO_LARGE` and is not sent.
3. `input_sha256` is recovered by parsing the final body with `CheckRequest`
   and inverting it to canonical bytes (ADR-SAG-0025); for an unmodified body
   it equals `prompt_sha256`.
4. The reply parses through `CheckResponse` and `into_answers` (model, ids,
   types, criteria, probabilities) to a `JudgeOutcome`; `noul` answers map to
   `yes`/`no` with no confidence. There is no byte-splice.

Comparison backends are registered and invoked only from **`sc-jev-check`**.

## General log (agt-9eh)

`JevClient::with_general_log` logs judge start, end and failure (typed id) with
the model and endpoint, and hands the log to its `HttpClient` for busy-retry
events. Unset, nothing is logged.
