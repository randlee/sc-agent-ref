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
| REQ-SAG-0101 | `JevError`: one variant per id in `tests/fixtures/jev/README-contract/errors.json`; each has `code()`, `message()` and `suggested_action()` (was `fix_hint()`, removed by the agt-2a3 amendment) |
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
| REQ-SAG-0303 | Question-set schema `plan-anchoring/questions/1`; `criteria` values load as question descriptions |

## Phase A — Jev API in sc-jev (sprint a-4)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0400 | Injectable transport; the Jev check/judge body is built once per call and every busy retry resends it unchanged |
| REQ-SAG-0401 | Busy retry policy (429/502/529 + Retry-After) |

## Phase A — LLM backends (sprint a-5)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0500 | Haiku API-direct; injectable system prompt; canonical payload embedded; `payload_sha256` on canonical bytes only |
| REQ-SAG-0501 | Luna API-direct + input token count path (feature 8); fake-transport proof in CI; live probe optional when `OPENAI_API_KEY` present |
| REQ-SAG-0502 | LLM context-length refusal matches Jev semantics |
| REQ-SAG-0503 | LLM evaluator system prompt from config/file at runtime; no production prompt prose hardcoded in Rust |
| REQ-SAG-0504 | LLM `stop_reason` / `status`: truncated at max_tokens → `RESPONSE_INVALID` (tighten #4) |
| REQ-SAG-0505 | Haiku/Luna busy retry 429/502/529 + Retry-After like Jev (tighten #5) |

## Phase A — orchestration (sprint a-6)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0802 | `sc-jev-py` publishes six `cp310-abi3` wheels (`abi3-py310`, the floor the embedded sc-observability runtime sets; `requires-python` stays `>=3.11`; Linux, macOS, Windows on x86_64 and arm64); Windows MSVC targets link the CRT statically (ADR-SAG-0023, phase-end amendment); the `wheel` CI job builds the release-shape wheel on `windows-2022` and `windows-11-arm`, installs it in a fresh venv and imports `sc_jev_py` without `_testing`, and checks the wheel tag is `cp310-abi3` — (Active, phase-end; tag amended agt-9eh) |
| REQ-SAG-0133 | A repo-owned check (`scripts/check-scoop-manifest.py`, run by `scripts/sc-publish-dry-run.sh checks`) renders the Scoop manifest at the pin and fails unless both binaries are shimmed — (Active, phase-end) |
| REQ-SAG-0600 | Session store: UUID, mode 0600, expiry from `created_at` |
| REQ-SAG-0601 | `run_check` / dry-run library API |
| REQ-SAG-0602 | Equal-input record on every backend run. Its `payload_sha256` is the backend-neutral digest of the canonical payload (questions and state): the proof that every backend judged the same input. It excludes the LLM system prompt and the model, so it cannot key a response cache (`request_sha256` does, REQ-SAG-0621) |
| REQ-SAG-0603 | Multiple backends in one run (feature 3) |
| REQ-SAG-0610 | Every Jev/LLM prompt send emits `LogEvent` via `SharedSessionLog` (v2 path) |
| REQ-SAG-0611 | Log records include session correlation id, backend, payload digest, no secrets |

## Phase A — CLI (sprint a-7)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0700 | `sc-ai jev` subcommands with versioned JSON envelope |
| REQ-SAG-0701 | stderr `{"error","message"}`, exit 2 *(reshaped; exit 2 or 3: see the agt-2a3 amendment rows REQ-SAG-0701 (error shape, and exit codes) below)* |
| REQ-SAG-0702 | Readback pairs per sc-ai-cli |
| REQ-SAG-0703 | Documented install path for `sc-ai` binary |
| REQ-SAG-0710 | Blocking CLI: stdout = versioned envelope (ADR-SAG-0014) with Jev answer JSON in `data` + `session_id`; stderr errors; exit 2 *(or 3: see the agt-2a3 REQ-SAG-0701 exit-codes row below)* |
| NFR-SAG-0002 | Secrets never appear in CLI output or logs |
| NFR-SAG-0003 | Session JSONL retained on disk (path configurable; default documented) |
| REQ-SAG-0711 | `sc-ai` wires the logger through `sc_jev_host::obs` into `SharedSessionLog` for `sc-jev-check` (amended in r-3b; was `v2::LoggerBuilder` + a local adapter) |

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

## Phase A.1 — refinement (landed)

Each row and amendment carries an `(Active, r-N)` marker; `r-N` records the
sprint that landed it (before landing, the marker read `(Proposed, r-N)`). When two sprints of one wave edit adjacent
rows, the second to merge resolves the textual conflict, keeping both rows
verbatim. Plan: [`plans/phase-a-refine/plan.md`](plans/phase-a-refine/plan.md).

| ID | Requirement |
| --- | --- |
| REQ-SAG-0105 | Open backend set: `#[non_exhaustive]` ids; static facts on `BackendDescriptor` rows in one `BACKENDS` catalog; `JudgeBackend::capabilities()` derived from the descriptor; production dispatch is one factory table checked by `test_every_backend_has_factory`; elsewhere code branches on descriptor facts, never on id equality (test-and-review convention, ADR-SAG-0009) — (Active, r-1) |
| REQ-SAG-0106 | Explicit `JevError` variants with unchanged `code()` ids, producer-neutral variants, `ProducerError`, `retryable()`, no detail-prefix parsing — (Active, r-1) |
| REQ-SAG-0107 | `sc-agent-types` has no `sc-observability*` dependency; session log port uses local `LogRejected`; module seams are cargo features (`judge`, `secret`, `obs` default; `git`, `test-util` non-default; `cancel` core), every combination builds — (Active, r-1) |
| REQ-SAG-0108 | `SubjectsDocument` is the one validated subjects type (private fields; validated on deserialize and by `SubjectsBuilder::build`); it and the id-assigning `SubjectsBuilder` live in `sc-agent-types`; producer and consumer share them — (Active, r-1) |
| REQ-SAG-0109 | `crates/workspace-tests` (`publish = false`, no pyo3/tree-sitter in its graph) hosts cross-crate proofs incl. `jev_cross_impl`, a real `sc-treesitter` → `check.toml` e2e test, CLI/Python parity and git-read parity — (Active, r-3b) |
| REQ-SAG-0133 | Both released binaries (`sc-ai`, `sc-treesitter`) documented with per-channel coverage — (Active, r-4; Scoop multi-binary support landed upstream as https://github.com/randlee/sc-publish/pull/176 (owner decision: a PR instead of an issue; optional `[channels.scoop].binaries`); sc-agent has re-pinned the kit (sc-publish `34dab3f`) and sets `binaries`, so Scoop shims both `sc-ai` and `sc-treesitter`, bead `agt-scoop-multi-shim` resolved; WinGet command aliases are not an upstream issue, the kit pins Komac 2.16.0 and the first hand-submitted manifest must list both binaries with aliases, bead `agt-sft`) |
| REQ-SAG-0134 | CI proves `mimalloc` is absent from `--no-default-features` library builds of `sc-ai` and `sc-treesitter` — (Active, r-4) |
| REQ-SAG-0135 | CI proves `clap` is absent from `--no-default-features` library builds of `sc-ai` and `sc-treesitter` (`cargo tree` gates) — (Active, r-3b) |
| NFR-SAG-0004 | Library deps use caret ranges; `=` pins only for lockstep listed in ADR-SAG-0023, checked by `check-exact-pins.sh` — (Active, r-4) |
| REQ-SAG-0312 | `command` input `path` field (`RepoRelativePath`) settable in `check.toml`, substituted for `{path}` — (Active, r-2) |
| REQ-SAG-0313 | Command producer stderr `ScAgentErrorResponse` re-raised with its code (`ProducerError`), named by the input's optional `producer` (default: the stem of `command[0]`, which for an interpreter-run script is the interpreter) — (Active, r-2) |
| REQ-SAG-0314 | Request byte cap has one owner per backend (descriptor limit), enforced in `validate_for_send` before any send; dry-run reports `payload_bytes` and per-backend `over_limit` without failing — (Active, r-2) |
| REQ-SAG-0402 | One shared HTTP layer (`sc-agent-http`) for all backend families — (Active, r-3) |
| REQ-SAG-0403 | Both backends validate `CanonicalJudgePayload` by deserializing; the Jev body is the documented flat `{model, state, questions}` object (no byte-splice) — (Active, r-5) |
| REQ-SAG-0604 | Per-backend results; a failure keeps other answers; `leak` = any failed; consumers print the envelope on stdout and the first failure on stderr and exit 2 / raise with `.response` — (Active, r-3b; exit code 2 or 3 and the all-failed case superseded by the agt-2a3 amendment below) |
| REQ-SAG-0605 | Multi-backend runs independent by default; `chain_context` explicit (CLI flag, Python kwarg) and recorded — (Active, r-3b) |
| REQ-SAG-0606 | `equal` computed from backend-reported digests of the canonical blob recovered from each final wire body, compared with the run's `payload_sha256` (same questions and state, any backend, system prompt and model excluded) — (Active, r-2b) |
| REQ-SAG-0607 | Stateless `FileSessionStore` with a written no-lock concurrency model; session written once per run, only when an `ask`-capable backend returned `Ok` — (Active, r-2) |
| REQ-SAG-0622 | Python parity with the CLI `--request-json`: `run_check(request_json=...)` and `build_request(request_json=...)`, mutually exclusive with `check_path` — (Active, phase-end digests) |
| REQ-SAG-0620 | One engine API (`prepare` → `run`) and one `RunResponse` with one serialization printed by every consumer — (Active, r-2) |
| REQ-SAG-0621 | Every run result carries `questions_sha256` and per-backend `request_sha256` beside `payload_sha256`. `questions_sha256` identifies a question set, so runs of the same questions against different state can be compared. `payload_sha256` identifies questions plus state, backend-neutral: the equal-input proof that every backend judged the same input; it excludes the LLM system prompt and the model. `request_sha256` identifies an identical request: the sc-sha SHA-256 of the exact first body that backend is sent for its model, computed before sending without a key, equal to the session-log `request_sha256` of the same send, so a consumer can substitute a cached response; it is absent for a backend whose bytes depend on an earlier answer (an LLM backend after another in `chain_context`) or that refuses the questions. **On a reorder of the questions**: `questions_sha256` does not change (a set has no order, and `choice` options sort); `payload_sha256` and `request_sha256` change (the payload array and the Jev body keep order). **`score` level order is meaning**, so reversing the levels changes `questions_sha256`; this is the one departure from plan-anchor's `questions_digest` form (which has `choice` only): a `score` question's `criteria` is the ordered array `[[level, description], ...]`. sc-agent builds no cache; a key made with `jev` (sent as `jev-latest`) can go stale, so cache on pinned versions. Printed by `sc-ai` and Python additively; bytes in `tests/fixtures/README.md` — (Active) |
| REQ-SAG-0712 | `sc-ai` and `sc-treesitter` parse argv with clap derive behind the `cli` feature; error contract unchanged — (Active, r-3b) |
| REQ-SAG-0140 | `sc-decision-schema` is a published, data-only crate above `sc-agent-types`: dependencies `sc-agent-types` (core only), `serde`, `serde_json`; no HTTP, git, process, file or logging dependency and no I/O — (Active, r-7) |
| REQ-SAG-0141 | Each decision type has one home: the question/answer payload, judge contract and attestation types move (not copy) from `sc-agent-types` to `sc-decision-schema`, with no re-export — (Active, r-7) |
| REQ-SAG-0142 | The decision schema is versioned (`DecisionSchemaId`, e.g. `canonical.v1`); readers accept known versions and refuse others; the version lives outside the hashed canonical bytes, so `payload_sha256` and the cross-impl fixture are unchanged — (Active, r-7) |
| REQ-SAG-0143 | One serde request and response struct per Jev API call known today (`jev::v1` check) in `sc-decision-schema`; `sc-jev` builds the body with `CheckRequest` and parses replies with `CheckResponse`, with no byte-splice; it encodes the documented Jev API: flat `{model, state, questions}`, question types `choice`, `score` and `noul`, string/object/array `state`, and the `{model, answers, usage}` reply; a reply naming a different model than the request is accepted and the answered model is kept (`JudgeOutcome::answered_model`) — (Active, r-5) |
| REQ-SAG-0144 | Haiku/Luna input and output use the same schema: canonical bytes in, `DecisionAnswers` out (the Jev answers type) — (Active, r-7) |
| REQ-SAG-0145 | Model-family schema variants: `schema_for(ModelFamily)` with static dispatch keyed by `SchemaFamily` (no trait until a second variant); the canonical variant is the identity on the payload — (Active, r-7) |
| REQ-SAG-0146 | The session-log storage contract is an authoritative SQLite DDL with `schema_version`; the JSONL projection is the `fields{}` object of one v2 log line with exactly the column keys (`send_outcome`, nullable `request_sha256`, non-null `model_id`); a loader reads it into the tables losslessly; from `schema_version` 2 the rows carry `questions_sha256` (a question set, any state) beside `payload_sha256` (questions and state) and `request_sha256` (exact request bytes, for cached-response substitution) — (Active, r-7) |
| REQ-SAG-0147 | Model catalog in `sc-agent-types`: open `ModelId` enum (known variants plus an unknown-model form that only parsing can build), so a model released after the build parses, is sent verbatim and is logged; `jev` (latest) and any supported version such as `jev-1.13.0` are accepted, and `jev` may break on an unforeseen API change, which is expected and fixed by upgrading sc-agent — (Active, r-7) |
| REQ-SAG-0149 | Backends are `jev`, `haiku`, `luna` (each the latest model of its line); the old names and read aliases are removed; `--backend`, `backends=` and `check.toml` also take a known model id of one of them (that backend pinned to it); `sonnet`, `opus`, `fable`, `terra`, `sol`, `astra` are defined in `MODELS` and `ModelId` but are `INPUT_INVALID` (valid names listed) as a backend — (Active, agt-akp) |
| REQ-SAG-0150 | `~/.sc-agent/models.toml` `[[model]]` rows (`id`, `alias`, `released`) extend existing aliases without recompiling; a short name resolves to the newest `released` across crate and file; a malformed file, or an id of another family than its alias (by prefix), is `CONFIG_INVALID`; `check.toml` `[models]` then a pin win; `jev` sends the latest (`jev-latest`) and only an override or pin changes it; Rust and Python behave the same — (Active, agt-akp) |
| REQ-SAG-0151 | `sc-ai jev models` and `sc_jev_py.models()` list the resolved models and their source (`crate` or `file`); the resolved model and its source are logged through the general log, never at `error` — (Active, agt-akp) |
| REQ-SAG-0152 | A local-only freshness test fails when a provider lists a newer model in a tracked line than `MODELS`; it never runs when `CI` is set and skips with a printed reason without a CLI or key — (Active, agt-akp) |
| REQ-SAG-0148 | `ModelAliases::latest(alias)` returns the newest model for `jev`, `haiku`, `luna`, … from an in-crate table, overridable by `check.toml` `[models]`; backends send their configured model; `jev` is sent on the wire as `jev-latest` (`jev::v1::wire_model`, unverified against the live API) — (Active, r-7) |
| REQ-SAG-0612 | `SessionId` is a ULID (26-char Crockford base32) in documents, file names, CLI and Python; a UUID id (CLI) or a v2 session document is `INPUT_INVALID` (Python only receives ULID ids and has no UUID input path) — (Active, r-6) |
| REQ-SAG-0613 | Each send logs `request_sha256`: SHA-256 of the exact request body sent (headers excluded), recorded by `sc-agent-http` at the send site; `null` when the send failed before a body was built; future response-cache key — (Active, r-6) |
| REQ-SAG-0614 | Each row logs `repo`: the main repository root (a worktree resolves to it), else the calling folder; log and session paths are unchanged (ADR-SAG-0021) — (Active, r-6) |
| REQ-SAG-0615 | Each row logs `project`: optional caller text (`sc-ai --project`, Python `project=`), ≤ 256 bytes, no control characters, stored unscrubbed — (Active, r-6) |
| REQ-SAG-0616 | Each row logs `model_id`: the `ModelId` string from `JudgeBackend::model_id()` (catalog default, backend's configured model), never null — (Active, r-6) |
| REQ-SAG-0617 | Every `judge`/`ask` send, `Ok` or `Failed`, writes exactly one row in the v2 `fields{}` object (NotSent none); a rejected or failed log write never fails the run (best effort; flush failure is a `data.warnings` entry) — (Active, r-6) |
| REQ-SAG-0618 | SHA-256 digests are computed with `sc-sha` (`calculate_hash`, strict UTF-8, CR normalized to LF; for request bodies and canonical payloads, compact JSON with no raw CR, the digest is SHA-256 of the bytes sent; other text such as query sources is identified in its LF-normalized form); no workspace manifest lists `sha2` or `uuid` — (Active, r-6) |

### Phase A.1 amendments to existing rows

| ID | Amendment |
| --- | --- |
| REQ-SAG-0101 | "One variant per id" becomes "every variant maps to exactly one id in `errors.json`; an id may have several variants" — (Active, r-1) |
| REQ-SAG-0103 | The a-1 frozen contract is unfrozen once by r-1 (types break) and re-frozen at r-1 merge; later changes are additive — (Active, r-1) |
| REQ-SAG-0120 | The `observability::v2::SessionLog` port stays in `sc-agent-types` but no longer depends on `sc-observability-types`: `emit` returns the local `LogRejected`; the module is behind the default `obs` feature — (Active, r-1) |
| REQ-SAG-0121 | Noop and test-util doubles stay; "trim deps" is done for `sc-observability-types` (dependency and root entry removed); module deletion still waits on upstream v2 — (Active, r-1) |
| NFR-SAG-0001 | *(Superseded by the agt-2a3 amendment below.)* `ScAgentErrorResponse` is `{error, message}` plus `schema_version` (always `1`), `retryable` (only when `true`) and `details` (only for producer errors), in that key order; readers accept `schema_version` absent or `1` and deny unknown keys, so any added key (top level or in `details`) is breaking and bumps `schema_version` — (Active, r-1) |
| REQ-SAG-0701 | stderr is one `ScAgentErrorResponse` line per NFR-SAG-0001 (amended): `{error, message, schema_version, ...}`, exit 2 — (Active, r-1; superseded by the agt-2a3 amendment below) |
| REQ-SAG-0601 | `prepare`/`run` library API; dry-run via `RunConfig::dry_run` (replaces the Phase A `run_check` / dry-run API) — (Active, r-2) |
| REQ-SAG-0710 | `data` is the `RunResponse` serialization (ADR-SAG-0014 r-2 amendment): the primary backend is the first `Ok` backend in `BackendSet` order and its answers are at the top level (the verbatim Jev response when Jev is primary); plus `session_id` (null unless `session_saved`), `results`, `equal_input`, `warnings` — (Active, r-2) |
| REQ-SAG-0505 | Busy retry (owner decision 2026-11-06, amended 2026-11-08): 429/502/529 only; 5 sends in total (4 delays: 1, 2, 4, 8 s, 15 s before jitter); no policy or backend may exceed 5 sends; valid Retry-After honoured, clamped 0–30 s; Retry-After grammar ASCII digits with optional fraction, ≤ 32 bytes, else malformed; absent or malformed Retry-After → exponential backoff 1, 2, 4, 8 … s capped at 30 s; ≤ 250 ms jitter; 90 s budget for starting attempts (in-flight attempt keeps its request timeout); transport deadlines are bounded in the shared layer: request 1 ms–120 s, connect 1 ms–10 s; timeouts and other network errors, including connect/DNS failures, are not retried — `NETWORK_ERROR`, `retryable() == true`, caller decides — (Active, r-3; amended 2026-11-08) |
| REQ-SAG-0130 | Consumer input publishes `sc-agent-http` (publish order per ADR-SAG-0017, r-3 amendment as amended by r-7, `docs/architecture.md`, and `docs/development/sc-publish.md`) — (Active, r-3) |
| REQ-SAG-0130 | Consumer input publishes `sc-jev-host` (internal support, directly after `sc-jev-check`; owner decision 2026-11-06); no crates.io publish until r-5 (D9) lands — (Active, r-3b) |
| REQ-SAG-0711 | `sc-ai` wires the logger through `sc_jev_host::obs` — (Active, r-3b) |
| REQ-SAG-0103 | The frozen `JudgeBackend` / `JudgeRequest` contract and its contract tests move to `sc-decision-schema` (ADR-SAG-0030), unchanged except that `judge` takes the per-send `&SendTrace` and `model_id()` is added; later changes stay additive — (Active, r-7) |
| REQ-SAG-0107 | `sc-agent-types` loses the `judge` and `test-util` features with the moved modules; features are `secret`, `obs` (default) and `git` (non-default); every combination builds — (Active, r-7) |
| REQ-SAG-0130 | Consumer input publishes `sc-decision-schema` (publish order 2, directly after `sc-agent-types`) — (Active, r-7) |
| REQ-SAG-0600 | Session store: session id is a **ULID** (was UUID) in file names and documents; mode 0600, expiry from `created_at` — (Active, r-6) |
| REQ-SAG-0611 | Every log line's `fields{}` object is one ADR-SAG-0031 session-log row (session ULID, `repo`, `project`, `model_id`, `payload_sha256`, `questions_sha256`, nullable `request_sha256`, `send_outcome`, typed answers or `error_code`), on every line; never secrets, payload text or raw response bodies — (Active, r-6) |

## Amendments: error contract (bead agt-2a3, ADR-SAG-0027 and ADR-SAG-0020 amendments)

Made before the first release; nothing is kept for compatibility.

| ID | Amendment |
| --- | --- |
| REQ-SAG-0101 | `errors.json` gains `AUTH_FAILED`, `API_REQUEST_INVALID`, `QUOTA_EXHAUSTED`, `PROVIDER_BUSY`, `API_ERROR` and `IO_FAILED`; every variant still maps to exactly one id. `fix_hint()` is replaced by `message()` (what failed) and `suggested_action()` (what to do) — (Active, agt-2a3) |
| REQ-SAG-0106 | `retryable()` is true for `NETWORK_ERROR` kinds `timeout` and `connect`, `PROVIDER_BUSY`, `API_ERROR` from status 500 and above, and a producer's error as the producer says; `Operational`, `OperationalArea` and `SessionStoreIo` are removed; `ProducerError` carries the producer's `retryable`, `retry_after_seconds`, `field`, `details` and `suggested_action` — (Active, agt-2a3) |
| NFR-SAG-0001 | `ScAgentErrorResponse` is `{error, kind, message, suggested_action, retryable, retry_after_seconds?, field?, details?, schema_version}` in that key order; `retryable` is always present; `message` never holds the fix; `schema_version` stays `1` because nothing was published (from the first release, removing or renaming a key bumps it; an added key does not, because readers ignore unknown keys); readers require only `error` and `message` — (Active, agt-2a3; supersedes the r-1 field rule) |
| REQ-SAG-0701 | stderr is one `ScAgentErrorResponse` line per NFR-SAG-0001 (agt-2a3 shape), exit 2, or 3 when every failed backend's error is retryable; a command-line usage error is `INPUT_INVALID` naming the argument in `field`, never `CONFIG_INVALID` — (Active, agt-2a3) |
| REQ-SAG-0505 | Busy exhaustion is `PROVIDER_BUSY` with `cause` (`sends`, `budget`), `attempts` and `retry_after_seconds`; a busy status whose body marks quota is returned unretried and becomes `QUOTA_EXHAUSTED`; retry timing is unchanged — (Active, agt-2a3) |
| REQ-SAG-0604 | An all-failed run returns the response like a partial one (no answers, one typed error per backend, each with `details.backend`); `sc-ai` prints the envelope and the first failure, and exits 3 only if every failed backend's error is retryable, otherwise 2; `sc-jev-py` raises with `.response` — (Active, agt-2a3; ADR-SAG-0025 amendment) |
| REQ-SAG-0701 | Exit codes: 0 success, 2 a failure that is not retryable, 3 a retryable failure (`sc-treesitter`: its error's `retryable`; `sc-ai`: 3 only if **every** failed backend's error is retryable, whatever the order of the backends), for `sc-ai` and `sc-treesitter` (`docs/stability.md`) — (Active, agt-2a3) |
| REQ-SAG-0167 | The `sc-ai` binary has a `test-util`-only `--loopback-port` flag that aims the real HTTP backends at a loopback server; the published binary rejects it — (Active, agt-2a3; ADR-SAG-0022 test seam) |
| NFR-SAG-0002 | Provider error text (status, type, code, message, parameter, request id, or the raw body when it is not error JSON) may appear in an error, bounded to 1024 bytes per field and single-line, and always passes through `redact`; prompt text and model output are never added — (Active, agt-2a3; ADR-SAG-0020 amendment) |

Jev out of credits (ERR-QA-6, closed): live replies on 2026-11-08 show an
unfunded organization gets HTTP 402 with
`{"detail":{"error_type":"billing_error","message":...}}`, so it is
`QUOTA_EXHAUSTED`, not retried, with Jev's message and type kept.

**Owner decision (bead agt-2a3):** errors return as much or more detail than
Jev returns, with a discriminated id and a message on every path; out of
tokens (`QUOTA_EXHAUSTED`), busy (`PROVIDER_BUSY`) and API error
(`API_ERROR`) are distinct ids handled differently inside sc-agent; the
Python exception is never less capable than the CLI body (REQ-SAG-0165;
proof: `crates/sc-jev-py/src/error.rs` `exception_fields_equal_the_cli_body`
compares every attribute with the serialized CLI body for 16 error shapes,
and `tests/fixtures/jev-python/tests/test_error_fields.py`). Nothing was
released, so nothing is kept for compatibility.

New requirements:

| ID | Requirement |
| --- | --- |
| REQ-SAG-0160 | A non-200 reply from Jev, Haiku or Luna is classified once, in `sc_agent_http::api_failure`: 401 and 403 `AUTH_FAILED`; 402 or a quota body `QUOTA_EXHAUSTED`; a too-long-input body `INPUT_TOO_LARGE`; 400 and 422 `API_REQUEST_INVALID`; everything else `API_ERROR`. Each keeps the provider's status, type, code, message, parameter and request id — (Active, agt-2a3) |
| REQ-SAG-0161 | A 429 (or 502, 529) whose body marks quota, credit or spend-limit exhaustion (documented Anthropic and OpenAI shapes) is not busy and is not retried; an unknown 429 body stays busy — (Active, agt-2a3) |
| REQ-SAG-0162 | `IO_FAILED` carries `what` and the snake_case `io::ErrorKind` as `details.what` and `details.io_kind`; `docs/stability.md` no longer tells consumers to parse `message` to tell a bad input from a failed read — (Active, agt-2a3) |
| REQ-SAG-0163 | One id, `INPUT_TOO_LARGE`, covers the byte cap, the token cap and a provider's own refusal, with `limit`, `actual` and `unit` (`bytes`, `tokens`) in `details` — (Active, agt-2a3) |
| REQ-SAG-0164 | `RESPONSE_INVALID` has a `cause` (`truncated`, `too_large`, `not_json`, `schema`), a distinct `suggested_action` per cause, and `retryable` false for all, with no action that says to retry unchanged — (Active, agt-2a3) |
| REQ-SAG-0165 | The Python `JevError` carries the same fields as the CLI body (`code` for `error`, `kind`, `message`, `suggested_action`, `retryable`, `retry_after_seconds`, `field`, `details`, `schema_version`), produced from the same serialization — (Active, agt-2a3) |
| REQ-SAG-0166 | A session-log flush failure that happens as an error leaves is `details.log_flush_failure` on every variant, not text in the message — (Active, agt-2a3) |

## Amendments: general log (bead agt-9eh, ADR-SAG-0013 amendment 2026-11-09)

The session log was the only log. These rows add the general log and the
injection that was missing from the first design. Per-crate proofs are in each
crate's requirements file.

| ID | Requirement |
| --- | --- |
| REQ-SAG-0630 | Two logs, two files, both written with `sc-observability`: the session log (`<service>.log.jsonl`) and the general log (`<service>-general.log.jsonl`); no `tracing` or `log` dependency, no bespoke writer — (Active, agt-9eh) |
| REQ-SAG-0631 | The general-log port (`GeneralLog`, `SharedGeneralLog`, `NoopGeneralLog`) sits beside `SessionLog` in `sc-agent-types`, open and `Send + Sync`; libraries take the logger as a value, with no global — (Active, agt-9eh) |
| REQ-SAG-0632 | Each log is injectable (`RunConfig::general_log`, `ComposeInputs::{session_log, general_log}`, Python `session_log=` and `general_log=`) and falls back to the default sc-agent sets; the CLI injects nothing and keeps the defaults and `--log-dir`; Python is never less capable than the CLI — (Active, agt-9eh) |
| REQ-SAG-0633 | The general log records start and end of operations, backend selection and resolved model, config and path resolution, credential source (never the value), busy retries and waits, extraction and git steps, session store actions, flush failures, and failures by typed error id — (Active, agt-9eh) |
| REQ-SAG-0634 | No general-log event carries an API key, prompt text, session state or a provider body; errors are logged by `ErrorCode` only (NFR-SAG-0002) — (Active, agt-9eh) |
| REQ-SAG-0635 | A logger that rejects, raises or cannot flush changes no result and fails no run; a dry run opens no default log — (Active, agt-9eh) |
| REQ-SAG-0638 | Every general-log event of a run carries one per-run `run_id`; `send.done` and `run.done` carry `duration_ms` and the `session_id`; a failure event carries the fixed `stage`, the `backend` and `session_id` when known and the typed `error_code`, never provider or prompt text — (Active, agt-9eh) |
| REQ-SAG-0639 | A failure before the run (root discovery, `--repo-root`, `--request-json`, the check source) is logged by typed id at the resolved or an early-known log root; a path with no known root is named in the ADR-SAG-0013 silent-paths table — (Active, agt-9eh) |
| REQ-SAG-0640 | Logs name an endpoint only as scheme, host and path (`Endpoint::log_form`); userinfo, port, query and fragment never reach a log — (Active, agt-9eh) |
| REQ-SAG-0636 | The Python wheel links the `sc-observability` Python runtime in its own binary and registers it, so Rust writes general-log rows with no GIL (the default path takes no GIL; an injected callable takes it once per row; no queue) and `sc_observability.get_host_logger()` returns the same logger; Rust and Python rows land in one log — (Active, agt-9eh) |
| REQ-SAG-0637 | `pyo3` is pinned `=0.29.2` and `sc-observability-py`, `sc-observability-binding-runtime` and `sc-observability-dto` `=1.5.0`, all on the ADR-SAG-0023 lockstep list — (Active, agt-9eh) |

## Amendments: streaming decision calls (bead agt-1aq, ADR-SAG-0032)

| ID | Requirement |
| --- | --- |
| REQ-SAG-0720 | A bounded queue with N worker threads (no async runtime) runs `prepare` then `run` per request around the unchanged engine, with one shared session logger and one shared backend client per backend selection — (Active, agt-1aq) |
| REQ-SAG-0721 | `sc-ai jev stream` reads JSONL `{"id","request"}` lines (stdin or `--input`) and writes one `{"id","ok","envelope"?,"error"?}` line per request, in completion order; every answer echoes the caller's id — (Active, agt-1aq) |
| REQ-SAG-0722 | A failed request is a typed `ScAgentErrorResponse` on that request's line and the stream continues; the exit code is 0, or 3 when every failure is retryable, else 2 (`docs/stability.md`) — (Active, agt-1aq) |
| REQ-SAG-0723 | A stream is finished when its input is closed, the queue is empty and the workers are idle; requests may keep arriving while answers return — (Active, agt-1aq) |
| REQ-SAG-0724 | Concurrency is one global setting (`--concurrency`, `concurrency=`), 1 to 64, default 8; the default was tested only against scripted and loopback backends — (Active, agt-1aq) |
| REQ-SAG-0725 | `sc-ai --help` documents the stream wire format, ordering, failure and exit rules (pinned by `help_notes_describe_the_stream`) — (Active, agt-1aq) |
| REQ-SAG-0726 | `sc_jev_py.run_many` and `sc_jev_py.DecisionStream` give Python the same capability plus per-request `submit`, timeouts and `cancel`, releasing the GIL while workers run — (Active, agt-1aq) |
| REQ-SAG-0728 | A stream logs through the general log: its own `run_id` with worker and request counts at start and end, and one run, with its own `run_id`, per request (start, end with duration, failure by typed id, stage and backend); injected `session_log` and `general_log` are honoured; no prompt text, state or secret — (Active, agt-1aq) |
| REQ-SAG-0729 | The request's cancel token governs a send: a cancel ends a busy-retry wait within 100 ms and nothing more is sent; every way out of a stream logs `stream.end` and flushes within 2 s; Python streams take `session_log=`/`general_log=` and share the host logger — (Active, agt-1aq) |
| REQ-SAG-0730 | A Python stream at full rate keeps every general-log row (a full runtime queue is waited out, bounded; a row dropped after the bound is counted in the summary and a Warn row; Python rows on `get_host_logger()` are not starved) — (Active, agt-1aq) |
| REQ-SAG-0731 | `sc_jev_py.ask(session_id, follow_up, ...)` resumes a session stored by any earlier process, with the same options as `sc-ai jev ask` (`--project`, `--repo-root`, `--sessions-dir`, `--log-dir`, `--models-file`), through the same `ask_from`, so Python is never less capable than the CLI — (Active, phase-end) |
| REQ-SAG-0727 | Memory is bounded (a slow consumer applies backpressure; lines over 1 MiB are refused) and `cancel` stops new work and honours the cancel token — (Active, agt-1aq) |
