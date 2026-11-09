# sc-jev-backends — architecture

**Boundary:** `jev-backends`  
**Depends on:** `sc-agent-types`, `sc-agent-http` (r-3), `serde_json` (API key resolved by **`sc-jev-check`** and passed at backend construction)

**HTTP policy:** `with_default_transport` builds `sc_agent_http::UreqTransport`
with connect 10 s and request 120 s (ADR-SAG-0007); busy retry is
`sc_agent_http::BusyPolicy` (REQ-SAG-0505 as amended in r-3). Other timeouts
are a transport choice at construction: `new(key, UreqTransport::new(Timeouts::new(..)))`.

Non-Jev **comparison** backends only. TypeSafe Jev HTTP lives in **`sc-jev`**.

## Modules

| Module | Role |
| --- | --- |
| `backend` | `AnthropicHaikuBackend` (Haiku) and `OpenAiLunaBackend` (Luna), API-direct; each implements `JudgeBackend` from `sc-agent-types` |
| `wire` | Provider request and reply shapes (Haiku `system`, Luna `instructions`; not the Jev wire document) |
| `validate` | Pre-send checks on the canonical payload and injected prompt: the payload deserialized as `CanonicalJudgePayload` (question-kind rules run there: `choice` 2 to 255 choices, `score` 2 to 10; a `noul` question is refused before any send), byte cap on the assembled body and the token cap, both `INPUT_TOO_LARGE` with `limit`, `actual` and `unit` (REQ-SAG-0163) |
| `backend` (HTTP part) | Builds each `sc_agent_http::HttpRequest` (endpoint, headers, `SecretHeader`) and sends it with `send_with_busy_retry`; maps every non-200 reply with `sc_agent_http::api_failure` (`AUTH_FAILED`, `QUOTA_EXHAUSTED`, `INPUT_TOO_LARGE`, `API_REQUEST_INVALID` or `API_ERROR`), keeping the provider's status, type, message and request id, bounded to 1024 bytes per field and redacted (ADR-SAG-0020 amendment) |

All modules are private; the crate root re-exports the public items.

## LLM wire envelope

Backends accept **caller-supplied** evaluator system prompt text plus canonical
payload bytes (ADR-SAG-0019). This crate does not read `check.toml` and does not
ship production prompt prose in source — only wiring, validation, and HTTP.

## Transport

From r-3 the transport is `sc_agent_http::Transport` (ADR-SAG-0026, HTTP
half). Both backends are generic, `AnthropicHaikuBackend<T: Transport =
UreqTransport>` and `OpenAiLunaBackend<T>`; `new(key, transport)`,
`with_default_transport(key)` (r-1 signature), `with_busy_policy(policy)` and
`with_cancel(token)`. The former `BackendTransport`, `RetrySleeper`,
`with_retry_sleeper`, `with_timeouts` and the `DEFAULT_*_TIMEOUT` constants
are gone; tests use `sc_agent_http::test_util` and
`BusyPolicy::with_test_hooks`.

No CLI, no PyO3, no `check.toml`, no Jev API endpoints.

## Model (r-7, REQ-SAG-0147/0148)

The `HAIKU_MODEL` / `LUNA_MODEL` constants are gone. Each backend holds a
configured `ModelId`: the catalog default (`BackendDescriptor::default_model`)
unless `with_model` sets one (the `sc-jev-check` factory passes
`ModelAliases::latest(alias)`, so `check.toml` `[models]` reaches here). The
backend sends `model.to_string()` verbatim, even for a model string this build
does not know, and `JudgeBackend::model_id()` returns it. Reply text is parsed
by `sc_decision_schema::llm::v1::parse_output`; provider envelopes stay here.
