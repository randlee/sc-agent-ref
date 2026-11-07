# sc-agent requirements

**ID range:** REQ-SAG-0001+  
**Status:** Approved  
**Owner:** sc-agent maintainers

## Product purpose

Rust crates plus orchestration assets for launching Synaptic Canvas agents.
Phase A delivers **`sc-jev`** (full Jev API home; check/judge slice first +
Haiku/Luna comparison) and
the **`sc-ai`** CLI. Behavioural contract for errors and parity rows:
**in-repo snapshot** [`tests/fixtures/jev/README-contract/errors.json`](../tests/fixtures/jev/README-contract/errors.json)
(aligned with atm-core jev-check README; update snapshot when README changes).

## Bootstrap (done)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0001 | Agent assets under `.claude/`, `.cursor/`, skills as documented |
| REQ-SAG-0002 | `.claude/project/quality-policy.md` is the QA policy source |
| REQ-SAG-0003 | sc-ai team in `.atm.toml`; roster in `docs/development/developer-roster.md` |
| REQ-SAG-0004 | plan-anchor under `.sc/plan-anchor/` |
| REQ-SAG-0005 | Rust workspace follows `docs/architecture.md` and per-crate requirements |

## Phase A — sc-jev (north star)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0110 | **`sc-jev` implements 100% of TypeSafe Jev HTTP API** over product lifetime; Phase A closes check/judge parity only |

## Phase A — foundation (sprint a-1)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0100 | Root Cargo workspace, `justfile`, sc-lint, standard lint/test targets |
| REQ-SAG-0101 | `JevError`: one variant per id in `tests/fixtures/jev/README-contract/errors.json`; each has `code()` and `fix_hint()` |
| REQ-SAG-0102 | Cross-implementation payload fixture: Rust digest matches committed Python reference bytes |
| REQ-SAG-0103 | Frozen `JudgeBackend` / `JudgeRequest` contract + contract tests in `sc-agent-types` |
| NFR-SAG-0001 | CLI error JSON shape (`ScAgentErrorResponse`) defined in `sc-agent-types` |
| REQ-SAG-0120 | Temporary `sc-agent-types::observability::v2` SessionLog shim (not a separate crate) |
| REQ-SAG-0121 | Noop + test-util doubles; delete shim + trim deps after upstream v2 release |

## Phase A — secrets (sprint a-2)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0200 | Resolve API keys **process env → repo-root `.env` → keyring** (see REQ-SAG-0202) |
| REQ-SAG-0201 | Optional `ANTHROPIC_KEYCHAIN_SERVICE` override |
| REQ-SAG-0202 | Repo-root `.env` third; precedence **process env → `.env` → keyring** |

## Phase A — subjects schema (sprint a-1)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0104 | Artifact schema **`sc/subjects/1`** in `sc-agent-types` + golden fixture |

## Phase A — sc-treesitter (sprint a-11)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0310 | Producer crate/binary; pinned grammars; query assets; emits `sc/subjects/1` |

## Phase A — check build (sprint a-3)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0300 | Extractors: `file`, `table-column`, `subjects`, `command` (no parser deps) |
| REQ-SAG-0311 | `subjects` input kind reads validated `sc/subjects/1` |
| REQ-SAG-0305 | `command` stdout must be subjects schema; argv/timeout contract |
| REQ-SAG-0309 | `table-column`: unescaped pipe splitting only (tighten #9) |
| REQ-SAG-0301 | Refuse send on `CONFIG_INVALID`, `INPUT_*` errors |
| REQ-SAG-0302 | Git repo-root resolution for relative paths |
| REQ-SAG-0303 | Question-set schema `plan-anchoring/questions/1` |

## Phase A — Jev API in sc-jev (sprint a-4)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0400 | Injectable transport; byte-exact Jev check/judge body |
| REQ-SAG-0401 | Busy retry policy (429/502/529 + Retry-After) |

## Phase A — LLM backends (sprint a-5)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0500 | Haiku API-direct; injectable system prompt; canonical payload embedded; `prompt_sha256` on canonical bytes only |
| REQ-SAG-0501 | Luna API-direct + input token count path (feature 8); fake-transport proof in CI; live probe optional when `OPENAI_API_KEY` present |
| REQ-SAG-0502 | LLM context-length refusal matches Jev semantics |
| REQ-SAG-0503 | LLM evaluator system prompt from config/file at runtime; no production prompt prose hardcoded in Rust |
| REQ-SAG-0504 | LLM `stop_reason` / `status`: truncated at max_tokens → `RESPONSE_INVALID` (tighten #4) |
| REQ-SAG-0505 | Haiku/Luna busy retry 429/502/529 + Retry-After like Jev (tighten #5) |

## Phase A — orchestration (sprint a-6)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0600 | Session store: UUID, mode 0600, expiry from `created_at` |
| REQ-SAG-0601 | `run_check` / dry-run library API |
| REQ-SAG-0602 | Equal-input record on every backend run |
| REQ-SAG-0603 | Multiple backends in one run (feature 3) |
| REQ-SAG-0610 | Every Jev/LLM prompt send emits `LogEvent` via `SharedSessionLog` (v2 path) |
| REQ-SAG-0611 | Log records include session correlation id, backend, payload digest, no secrets |

## Phase A — CLI (sprint a-7)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0700 | `sc-ai jev` subcommands with versioned JSON envelope |
| REQ-SAG-0701 | stderr `{"error","message"}`, exit 2 |
| REQ-SAG-0702 | Readback pairs per sc-ai-cli |
| REQ-SAG-0703 | Documented install path for `sc-ai` binary |
| REQ-SAG-0710 | Blocking CLI: stdout = versioned envelope (ADR-SAG-0014) with Jev answer JSON in `data` + `session_id`; stderr errors; exit 2 |
| NFR-SAG-0002 | Secrets never appear in CLI output or logs |
| NFR-SAG-0003 | Session JSONL retained on disk (path configurable; default documented) |
| REQ-SAG-0711 | `sc-ai` wires `v2::LoggerBuilder` + adapter into `SharedSessionLog` for `sc-jev-check` |

## Phase A — sc-jev-py + first release (sprint a-8)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0800 | PyO3 exports **`sc-jev-check`** (`run_check`, sessions); Python errors match `JevError` codes; no direct `sc-jev` dependency |
| REQ-SAG-0801 | Cross-impl pytest harness in repo |
| REQ-SAG-0802 | maturin wheel build; `pip install` from built artifact |

## Phase A — sc-publish kit (sprint a-9)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0130 | sc-publish consumer input enables **crates.io**, **pypi**, **homebrew**, **scoop**, **winget** + `sc-ai` release binaries |
| REQ-SAG-0131 | `docs/development/sc-publish.md`; do not hand-edit rendered publish manifests |
| REQ-SAG-0132 | Consumer-owned **`sc-publish dry-run`** workflow green on every PR: `install.py --dry-run` drift check, manifest validation, version lockstep, consumer preflight, `cargo publish --dry-run`, `maturin build`; it runs the same script as the local recipe. The kit's Release Preflight is `workflow_dispatch`-only and is run by the publisher at release time |
| REQ-SAG-0803 | **First release:** all five channels via sc-publish — **a-9** installs kit + consumer input + preflight + publish dry-run in CI |

## Phase B (draft — not scheduled)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0900 | `sc-ai` exposes API-direct Claude agent (Anthropic HTTP, no claude-code subprocess) |
| REQ-SAG-0901 | `sc-ai` exposes API-direct Codex/OpenAI agent (HTTP, no codex-cli subprocess) |
| REQ-SAG-0902 | Comparison flows may reuse `sc-jev` payload / equal-input rules where applicable |

## Traceability

Sprint docs under `docs/plans/phase-a/` list REQ ids in YAML frontmatter.
Phase plan: [`docs/plans/phase-a/plan.md`](plans/phase-a/plan.md).

## Phase A.1 — refinement (proposed)

Each row and amendment carries a marker `(Proposed, r-N)` that its owning
sprint flips to `(Active, r-N)`. When two sprints of one wave edit adjacent
rows, the second to merge resolves the textual conflict, keeping both rows
verbatim. Plan: [`plans/phase-a-refine/plan.md`](plans/phase-a-refine/plan.md).

| ID | Requirement |
| --- | --- |
| REQ-SAG-0105 | Open backend set: `#[non_exhaustive]` ids; static facts on `BackendDescriptor` rows in one `BACKENDS` catalog; `JudgeBackend::capabilities()` derived from the descriptor; production dispatch is one factory table checked by `test_every_backend_has_factory`; elsewhere code branches on descriptor facts, never on id equality (test-and-review convention, ADR-SAG-0009) — (Active, r-1) |
| REQ-SAG-0106 | Explicit `JevError` variants with unchanged `code()` ids, producer-neutral variants, `ProducerError`, `retryable()`, no detail-prefix parsing — (Active, r-1) |
| REQ-SAG-0107 | `sc-agent-types` has no `sc-observability*` dependency; session log port uses local `LogRejected`; module seams are cargo features (`judge`, `secret`, `obs` default; `git`, `test-util` non-default; `cancel` core), every combination builds — (Active, r-1) |
| REQ-SAG-0108 | `SubjectsDocument` is the one validated subjects type (private fields; validated on deserialize and by `SubjectsBuilder::build`); it and the id-assigning `SubjectsBuilder` live in `sc-agent-types`; producer and consumer share them — (Active, r-1) |
| REQ-SAG-0109 | `crates/workspace-tests` (`publish = false`, no pyo3/tree-sitter in its graph) hosts cross-crate proofs incl. `jev_cross_impl`, a real `sc-treesitter` → `check.toml` e2e test, CLI/Python parity and git-read parity — (Proposed, r-3b) |
| REQ-SAG-0133 | Both released binaries (`sc-ai`, `sc-treesitter`) documented with per-channel coverage — (Active, r-4; upstream sc-publish issue URL pending — owner deferral recorded in sprint-r-4 Implementation notes, bead `agt-scoop-multi-shim`) |
| REQ-SAG-0134 | CI proves `mimalloc` is absent from `--no-default-features` library builds of `sc-ai` and `sc-treesitter` — (Active, r-4) |
| REQ-SAG-0135 | CI proves `clap` is absent from `--no-default-features` library builds of `sc-ai` and `sc-treesitter` (`cargo tree` gates) — (Proposed, r-3b) |
| NFR-SAG-0004 | Library deps use caret ranges; `=` pins only for lockstep listed in ADR-SAG-0023, checked by `check-exact-pins.sh` — (Active, r-4) |
| REQ-SAG-0312 | `command` input `path` field (`RepoRelativePath`) settable in `check.toml`, substituted for `{path}` — (Active, r-2) |
| REQ-SAG-0313 | Command producer stderr `ScAgentErrorResponse` re-raised with its code (`ProducerError`), named by the input's optional `producer` (default: the stem of `command[0]`, which for an interpreter-run script is the interpreter) — (Active, r-2) |
| REQ-SAG-0314 | Request byte cap has one owner per backend (descriptor limit), enforced in `validate_for_send` before any send; dry-run reports `payload_bytes` and per-backend `over_limit` without failing — (Active, r-2) |
| REQ-SAG-0402 | One shared HTTP layer (`sc-agent-http`) for all backend families — (Active, r-3) |
| REQ-SAG-0403 | Both backends validate `CanonicalJudgePayload` by deserializing; the Jev body is the documented flat `{model, state, questions}` object (no byte-splice) — (Proposed, r-5) |
| REQ-SAG-0604 | Per-backend results; a failure keeps other answers; `leak` = any failed; consumers print the envelope on stdout and the first failure on stderr and exit 2 / raise with `.response` — (Proposed, r-3b) |
| REQ-SAG-0605 | Multi-backend runs independent by default; `chain_context` explicit (CLI flag, Python kwarg) and recorded — (Proposed, r-3b) |
| REQ-SAG-0606 | `equal` computed from backend-reported digests of the canonical blob recovered from each final wire body — (Proposed, r-2b) |
| REQ-SAG-0607 | Stateless `FileSessionStore` with a written no-lock concurrency model; session written once per run, only when an `ask`-capable backend returned `Ok` — (Active, r-2) |
| REQ-SAG-0620 | One engine API (`prepare` → `run`) and one `RunResponse` with one serialization printed by every consumer — (Active, r-2) |
| REQ-SAG-0712 | `sc-ai` and `sc-treesitter` parse argv with clap derive behind the `cli` feature; error contract unchanged — (Proposed, r-3b) |
| REQ-SAG-0140 | `sc-decision-schema` is a published, data-only crate above `sc-agent-types`: dependencies `sc-agent-types` (core only), `serde`, `serde_json`; no HTTP, git, process, file or logging dependency and no I/O — (Proposed, r-7) |
| REQ-SAG-0141 | Each decision type has one home: the question/answer payload, judge contract and attestation types move (not copy) from `sc-agent-types` to `sc-decision-schema`, with no re-export — (Proposed, r-7) |
| REQ-SAG-0142 | The decision schema is versioned (`DecisionSchemaId`, e.g. `canonical.v1`); readers accept known versions and refuse others; the version lives outside the hashed canonical bytes, so `prompt_sha256` and the cross-impl fixture are unchanged — (Proposed, r-7) |
| REQ-SAG-0143 | One serde request and response struct per Jev API call known today (`jev::v1` check) in `sc-decision-schema`; `sc-jev` builds the body with `CheckRequest` and parses replies with `CheckResponse`, with no byte-splice; it encodes the documented Jev API: flat `{model, state, questions}`, question types `choice`, `score` and `noul`, string/object/array `state`, and the `{model, answers, usage}` reply — (Proposed, r-5) |
| REQ-SAG-0144 | Haiku/Luna input and output use the same schema: canonical bytes in, `DecisionAnswers` out (the Jev answers type) — (Proposed, r-7) |
| REQ-SAG-0145 | Model-family schema variants: `schema_for(ModelFamily)` with static dispatch keyed by `SchemaFamily` (no trait until a second variant); the canonical variant is the identity on the payload — (Proposed, r-7) |
| REQ-SAG-0146 | The session-log storage contract is an authoritative SQLite DDL with `schema_version`; the JSONL projection is the `fields{}` object of one v2 log line with exactly the column keys (`send_outcome`, nullable `request_sha256`, non-null `model_id`); a loader reads it into the tables losslessly — (Proposed, r-7) |
| REQ-SAG-0147 | Model catalog in `sc-agent-types`: open `ModelId` enum (known variants plus an unknown-model form that only parsing can build), so a model released after the build parses, is sent verbatim and is logged — (Proposed, r-7) |
| REQ-SAG-0148 | `ModelAliases::latest(alias)` returns the newest model for `jev`, `haiku`, `luna`, … from an in-crate table, overridable by `check.toml` `[models]`; backends send their configured model — (Proposed, r-7) |
| REQ-SAG-0612 | `SessionId` is a ULID (26-char Crockford base32) in documents, file names, CLI and Python; a UUID id or a v2 session document is `INPUT_INVALID` — (Proposed, r-6) |
| REQ-SAG-0613 | Each send logs `request_sha256`: SHA-256 of the exact request body sent (headers excluded), recorded by `sc-agent-http` at the send site; `null` when the send failed before a body was built; future response-cache key — (Proposed, r-6) |
| REQ-SAG-0614 | Each row logs `repo`: the main repository root (a worktree resolves to it), else the calling folder; log and session paths are unchanged (ADR-SAG-0021) — (Proposed, r-6) |
| REQ-SAG-0615 | Each row logs `project`: optional caller text (`sc-ai --project`, Python `project=`), ≤ 256 bytes, no control characters, stored unscrubbed — (Proposed, r-6) |
| REQ-SAG-0616 | Each row logs `model_id`: the `ModelId` string from `JudgeBackend::model_id()` (catalog default, backend's configured model), never null — (Proposed, r-6) |
| REQ-SAG-0617 | Every `judge`/`ask` send, `Ok` or `Failed`, writes exactly one row in the v2 `fields{}` object (NotSent none); a rejected or failed log write never fails the run (best effort; flush failure is a `data.warnings` entry) — (Proposed, r-6) |
| REQ-SAG-0618 | SHA-256 digests are computed with `sc-sha` (`calculate_hash`, strict UTF-8, CR normalized to LF; for request bodies and canonical payloads, compact JSON with no raw CR, the digest is SHA-256 of the bytes sent; other text such as query sources is identified in its LF-normalized form); no workspace manifest lists `sha2` or `uuid` — (Proposed, r-6) |

### Phase A.1 amendments to existing rows

| ID | Amendment |
| --- | --- |
| REQ-SAG-0101 | "One variant per id" becomes "every variant maps to exactly one id in `errors.json`; an id may have several variants" — (Active, r-1) |
| REQ-SAG-0103 | The a-1 frozen contract is unfrozen once by r-1 (types break) and re-frozen at r-1 merge; later changes are additive — (Active, r-1) |
| REQ-SAG-0120 | The `observability::v2::SessionLog` port stays in `sc-agent-types` but no longer depends on `sc-observability-types`: `emit` returns the local `LogRejected`; the module is behind the default `obs` feature — (Active, r-1) |
| REQ-SAG-0121 | Noop and test-util doubles stay; "trim deps" is done for `sc-observability-types` (dependency and root entry removed); module deletion still waits on upstream v2 — (Active, r-1) |
| NFR-SAG-0001 | `ScAgentErrorResponse` is `{error, message}` plus `schema_version` (always `1`), `retryable` (only when `true`) and `details` (only for producer errors), in that key order; readers accept `schema_version` absent or `1` and deny unknown keys, so any added key (top level or in `details`) is breaking and bumps `schema_version` — (Active, r-1) |
| REQ-SAG-0701 | stderr is one `ScAgentErrorResponse` line per NFR-SAG-0001 (amended): `{error, message, schema_version, ...}`, exit 2 — (Active, r-1) |
| REQ-SAG-0601 | `prepare`/`run` library API; dry-run via `RunConfig::dry_run` (replaces the Phase A `run_check` / dry-run API) — (Active, r-2) |
| REQ-SAG-0710 | `data` is the `RunResponse` serialization (ADR-SAG-0014 r-2 amendment): the primary backend is the first `Ok` backend in `BackendSet` order and its answers are at the top level (the verbatim Jev response when Jev is primary); plus `session_id` (null unless `session_saved`), `results`, `equal_input`, `warnings` — (Active, r-2) |
| REQ-SAG-0505 | Busy retry (owner decision 2026-11-06): 429/502/529 only; 4 sends; valid Retry-After honoured, clamped 0–30 s; Retry-After grammar ASCII digits with optional fraction, ≤ 32 bytes, else malformed; absent or malformed Retry-After → exponential backoff 1, 2, 4, 8 … s capped at 30 s (the default 4 sends give at most three waits: 1, 2, 4 s); ≤ 250 ms jitter; 90 s budget for starting attempts (in-flight attempt keeps its 120 s timeout); connect/DNS failures are not retried — `NETWORK_ERROR`, `retryable() == true`, caller decides — (Active, r-3) |
| REQ-SAG-0130 | Consumer input publishes `sc-agent-http` (publish order 3) — (Active, r-3) |
| REQ-SAG-0130 | Consumer input publishes `sc-agent-compose` (internal support, directly after `sc-jev-check`; owner decision 2026-11-06); no crates.io publish until r-5 (D9) lands — (Proposed, r-3b) |
| REQ-SAG-0711 | `sc-ai` wires the logger through `sc_agent_compose::obs` — (Proposed, r-3b) |
| REQ-SAG-0103 | The frozen `JudgeBackend` / `JudgeRequest` contract and its contract tests move to `sc-decision-schema` (ADR-SAG-0030), unchanged except that `judge` takes the per-send `&SendTrace` and `model_id()` is added; later changes stay additive — (Proposed, r-7) |
| REQ-SAG-0107 | `sc-agent-types` loses the `judge` and `test-util` features with the moved modules; features are `secret`, `obs` (default) and `git` (non-default); every combination builds — (Proposed, r-7) |
| REQ-SAG-0130 | Consumer input publishes `sc-decision-schema` (publish order 2, directly after `sc-agent-types`) — (Proposed, r-7) |
| REQ-SAG-0600 | Session store: session id is a **ULID** (was UUID) in file names and documents; mode 0600, expiry from `created_at` — (Proposed, r-6) |
| REQ-SAG-0611 | Every log line's `fields{}` object is one ADR-SAG-0031 session-log row (session ULID, `repo`, `project`, `model_id`, `prompt_sha256`, nullable `request_sha256`, `send_outcome`, typed answers or `error_code`), on every line; never secrets, payload text or raw response bodies — (Proposed, r-6) |
