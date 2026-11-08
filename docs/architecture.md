# sc-agent architecture

**Status:** Active (Phase A implemented on `integrate/phase-a`)  
**Owner:** sc-agent maintainers

## System shape

sc-agent combines **agent orchestration assets** (prompts, skills, policy) with a
**Rust workspace** under `crates/` for agent launchers and shared libraries.

| Layer | Location | Role |
| --- | --- | --- |
| Agents | `.claude/agents/`, `.cursor/agents/`, `.codex/agents/` | Role prompts |
| Skills | `.claude/skills/`, `.cursor/skills/` | Orchestration, anchoring, publishing |
| Policy | `.claude/project/quality-policy.md` | Reviewers, validation, layout |
| Rust workspace | `crates/*`, root `Cargo.toml` | Libraries + `sc-ai` CLI |
| Boundaries | `boundaries/*` | sc-lint module separation (from sprint a-1) |
| Beads | `.beads/` | Issue tracking |
| Plan anchor | `.sc/plan-anchor/` | Plan tracker settings |

## Phase A crate graph

Direct workspace dependencies, taken from each `crates/*/Cargo.toml`
(`[dependencies]` only; dev-dependencies omitted). The A.1 graph below adds
features and labels.

```
sc-ai (binary) ─────┐
sc-jev-py (PyO3) ───┴── sc-jev-host (sc-observability =1.5.0 adapter, path policy, run/ask)
                          └── sc-jev-check (facade: check.toml, extractors, run_check, sessions)
                                ├── sc-jev ────────────┐
                                ├── sc-jev-backends ───┴── sc-agent-http (ureq transport)
                                ├── sc-agent-secrets
                                ├── sc-decision-schema
                                └── sc-agent-types (observability::v2 port; no sc-observability* dep)

sc-treesitter (library + binary) → sc-agent-types
```

`sc-ai` and `sc-jev-py` also depend directly on `sc-jev-check`,
`sc-decision-schema` and `sc-agent-types`; `sc-jev`, `sc-jev-backends` and
`sc-jev-host` depend directly on `sc-decision-schema` and `sc-agent-types`;
`sc-agent-http` and `sc-agent-secrets` depend on `sc-agent-types`. The graph
shows each of those edges once, at the lowest node that carries it.

Every workspace crate depends on `sc-agent-types`; `sc-decision-schema`
sits directly above it (r-7). Since r-1 no crate depends on
`sc-observability-types` (ADR-SAG-0016 r-1 amendment). **`sc-jev-host` is the
only crate that links full `sc-observability`** (`crates/sc-jev-host/Cargo.toml`;
ADR-SAG-0013 and ADR-SAG-0010 r-3b amendments). `sc-ai` and `sc-jev-py` are
thin composition roots over `sc-jev-host` and have no direct `sc-observability`
dependency.

`sc-jev` and `sc-jev-backends` do not depend on each other. Both reach HTTP
only through `sc-agent-http`.

**`sc-treesitter`** is the `sc/subjects/1` producer (ADR-SAG-0015). It depends
on `sc-agent-types` and links no judge, secrets or HTTP crate. No workspace
crate depends on it; `sc-jev-check` reaches it only by running its binary
through a `command` input. The crate ships a **second released binary**, `sc-treesitter`,
alongside `sc-ai`. Direct dependencies of every crate are listed in its
`boundaries/<crate>/*.toml` manifest ([`boundaries/README.md`](../boundaries/README.md)).

## Crate naming convention

- `sc-jev-*` is the Jev decision family: crates that send TypeSafe Jev API
  requests or answer Jev-schema questions, plus the consumers built only on
  them. Haiku and Luna count as Jev because they answer Jev-schema questions,
  even though their wire is provider HTTP (ADR-SAG-0007, ADR-SAG-0019). Today:
  `sc-jev`, `sc-jev-backends`, `sc-jev-check`, `sc-jev-host` and `sc-jev-py`
  (the PyO3 consumer of `sc-jev-host`).
- Each other decision API gets its own API-specific crates: `sc-decider*`,
  `sc-laya*`, `sc-kev*`.
- `sc-decision-*` (for example `sc-decision-schema`) is the API-agnostic layer
  across all decision APIs. Its shared types are API-agnostic; it also hosts
  one versioned module per API (`jev::v1` from r-5; `decider::`, `laya::`,
  `kev::` reserved), so each API's wire types live in one place.

## Phase A.1 crate graph

**Status:** Active (Phase A.1) — landed in r-3b.

```text
sc-ai (bin; feature cli: clap + mimalloc) ─┐
sc-jev-py (PyO3) ──────────────────────────┴── sc-jev-host (published, = pin; obs, PathPolicy, run_from/ask_from)
                                                 ├── sc-observability =1.5.0
                                                 └── sc-jev-check (prepare/run, RunResponse + Envelope, discover_repo_root, InputSpec)
                                                       ├── sc-jev ──────────┐
                                                       ├── sc-jev-backends ─┴── sc-agent-http (ureq; sc-agent-types features = [secret])
                                                       ├── sc-agent-secrets
                                                       └── sc-agent-types (default secret/obs; git enabled by sc-jev-check; no observability dep)
sc-treesitter (bin; feature cli: clap + mimalloc + sc-agent-types/git) → sc-agent-types (default-features = false)
crates/workspace-tests (publish = false; dev-deps only; drives built binaries; ADR-SAG-0028)
```

The Phase A graph above is the starting point; r-3b moved the observability
adapter, the path policy and the `run`/`ask` wiring out of `sc-ai` and
`sc-jev-py` into `sc-jev-host` (ADR-SAG-0026 host half). `sc-jev-host` is the
only crate that links `sc-observability`. `workspace-tests` links no
`pyo3` or `tree-sitter`; it runs `sc-ai` and `sc-treesitter` as built
binaries. `sc-decision-schema` (r-7) sits directly above `sc-agent-types`
and below `sc-jev`, `sc-jev-backends`, `sc-jev-check`, `sc-jev-host`, `sc-ai` and
`sc-jev-py`; it links no HTTP, secret, git or logging crate.

**Jev API:** All TypeSafe Jev HTTP lives in **`sc-jev`** (REQ-SAG-0110).  
**Trait boundary:** `JudgeBackend` and the decision types live
in `sc-decision-schema`; ids, errors and the backend and model catalogs
live in `sc-agent-types` (ADR-SAG-0030). LLM comparison backends implement
the trait in `sc-jev-backends`. Local judge (decider-4b / cyprid) adds a backend later
without changing callers.

## ADR index

ADR sections below keep their original order. This table lists them in id
order. One rule fills the last column (plan.md conventions): a new ADR lists
every older ADR whose Phase A.1 amendment subsection it drives; an older ADR
lists every A.1 subsection it receives, as the driving ADR or as "A.1
subsection" when only a D decision drives it. Each A.1 entry carries an
`(Active, r-N)` marker, where `r-N` records the sprint that landed it (before
landing the marker read `(Proposed, r-N)`; a landing sprint flipped only its
own row's Status cell and its own marker text, and added or removed no entry).

| ADR | Title | Status | Amends / amended by |
| --- | --- | --- | --- |
| ADR-SAG-0001 | Error enum is CLI authority | Active | Amended by ADR-SAG-0027 (Active, r-1) |
| ADR-SAG-0002 | Canonical judge payload bytes | Active | A.1 subsection (Active, r-5) |
| ADR-SAG-0003 | Secret resolution | Active | Amended by ADR-SAG-0020; ADR-SAG-0024 (Active, r-2) |
| ADR-SAG-0004 | One check, one request | Active | Amended by ADR-SAG-0025 (Active, r-2b) |
| ADR-SAG-0005 | Facade owns payload assembly | Active | Amended by ADR-SAG-0024 (Active, r-2); A.1 subsections (Active, r-4), (Active, r-5); ADR-SAG-0030 (Active, r-7) |
| ADR-SAG-0006 | Transport injection | Active | Amended by ADR-SAG-0026 (Active, r-3) |
| ADR-SAG-0007 | API-direct LLM backends | Active | Amended by ADR-SAG-0026 (Active, r-3) |
| ADR-SAG-0008 | Sessions carry prior answers to later backends | Active | A.1 subsection (Active, r-1); amended by ADR-SAG-0025 (Active, r-2b); ADR-SAG-0031 (Active, r-6) |
| ADR-SAG-0009 | Local backend registers without caller changes | Active | Amended by ADR-SAG-0022; ADR-SAG-0027 (Active, r-1); ADR-SAG-0024 (Active, r-2); ADR-SAG-0030 (Active, r-7) |
| ADR-SAG-0010 | CLI thin adapter | Active | Amended by ADR-SAG-0026 (Active, r-3b) |
| ADR-SAG-0011 | Python via sc-jev-py wheel | Active | Amended by ADR-SAG-0026 (Active, r-3b) |
| ADR-SAG-0012 | sc-jev owns Jev API | Active | Amended by ADR-SAG-0030 (Active, r-5) |
| ADR-SAG-0013 | Session JSONL via sc-observability v2 | Active | Amended by ADR-SAG-0021; ADR-SAG-0026 (Active, r-3b); ADR-SAG-0031 (Active, r-6) |
| ADR-SAG-0014 | CLI success body | Active | A.1 subsection (Active, r-1); amended by ADR-SAG-0024 (Active, r-2) |
| ADR-SAG-0015 | Subjects artifact outside the judge | Active | — |
| ADR-SAG-0016 | v2 shim in sc-agent-types; adapter in sc-jev-host | Active | Amended by ADR-SAG-0023 (Active, r-4); ADR-SAG-0027 (Active, r-1); ADR-SAG-0026 (Active, r-3b); ADR-SAG-0030 (Active, r-7); ADR-SAG-0031 (Active, r-6) |
| ADR-SAG-0017 | First release surfaces | Active | Amended by ADR-SAG-0023 (Active, r-4); ADR-SAG-0026 (Active, r-3), (Active, r-3b); ADR-SAG-0030 (Active, r-7) |
| ADR-SAG-0018 | sc-publish kit owns release surface | Active | — |
| ADR-SAG-0019 | LLM wire envelope vs canonical payload | Active | A.1 subsection (Active, r-1); amended by ADR-SAG-0024 (Active, r-2); ADR-SAG-0025 (Active, r-2b); ADR-SAG-0030 (Active, r-7) |
| ADR-SAG-0020 | Secret injection and redaction | Active | Amends ADR-SAG-0003; amended by ADR-SAG-0029; A.1 subsection (Active, r-1); ADR-SAG-0026 (Active, r-3); A.1 subsection (Active, r-3b) |
| ADR-SAG-0021 | Log and session path precedence | Active | Amends ADR-SAG-0013 |
| ADR-SAG-0022 | Scripted backends are a test-util feature | Active | Amends ADR-SAG-0009; amended by ADR-SAG-0029; A.1 subsection (Active, r-1); ADR-SAG-0024 (Active, r-2); ADR-SAG-0025 (Active, r-2b); ADR-SAG-0030 (Active, r-7) |
| ADR-SAG-0023 | Dependency pins, allocator, released binaries | Active (r-4; host pin r-3b) | Amends ADR-SAG-0016, ADR-SAG-0017 |
| ADR-SAG-0024 | Engine entry point and response contract | Active (r-2) | Amends ADR-SAG-0003, ADR-SAG-0005, ADR-SAG-0009, ADR-SAG-0014, ADR-SAG-0019, ADR-SAG-0022; supersedes the ADR-SAG-0014 A.1 r-1 interim subsection |
| ADR-SAG-0025 | Multi-backend semantics and equal-input attestation | Active (r-2b) | Amends ADR-SAG-0004, ADR-SAG-0008, ADR-SAG-0019, ADR-SAG-0022 |
| ADR-SAG-0026 | Shared HTTP crate and host crate | Active (HTTP half, r-3; host half, r-3b) | Amends ADR-SAG-0006, ADR-SAG-0007, ADR-SAG-0020, ADR-SAG-0017 (HTTP half); ADR-SAG-0010, ADR-SAG-0011, ADR-SAG-0013, ADR-SAG-0016, ADR-SAG-0017 (host half); amended by ADR-SAG-0030 (Active, r-7); ADR-SAG-0031 (Active, r-6) |
| ADR-SAG-0027 | Error taxonomy and extensibility | Active (r-1) | Amends ADR-SAG-0001, ADR-SAG-0009, ADR-SAG-0016; amended by ADR-SAG-0030 (Active, r-7); ADR-SAG-0031 (Active, r-6) |
| ADR-SAG-0028 | workspace-tests crate | Active (r-3b) | Amended by ADR-SAG-0030 (Active, r-7) |
| ADR-SAG-0029 | Phase A hardening amendments | Active (a-10) | Amends ADR-SAG-0020 and ADR-SAG-0022; A.1 supersedes named sentences of (a) (Active, r-1), (Active, r-3) and of (b) (Active, r-1), (Active, r-2) |
| ADR-SAG-0030 | Decision schema crate and model catalog | Active (r-7); jev::v1 (Active, r-5) | Amends ADR-SAG-0005, ADR-SAG-0009, ADR-SAG-0012, ADR-SAG-0016, ADR-SAG-0017, ADR-SAG-0019, ADR-SAG-0022, ADR-SAG-0026, ADR-SAG-0027, ADR-SAG-0028 |
| ADR-SAG-0031 | Session log storage schema | Active (r-7 storage schema, r-6 wiring) | Amends ADR-SAG-0008, ADR-SAG-0013, ADR-SAG-0016, ADR-SAG-0026, ADR-SAG-0027 (wiring) |

## ADR-SAG-0001 — Error enum is CLI authority

**Status:** Active

All user-visible error ids come from `JevError` in `sc-agent-types`.
CLI and PyO3 map variants to JSON / exceptions; no ad-hoc strings.

## ADR-SAG-0002 — Canonical judge payload bytes

**Status:** Active

The **canonical judge payload** is sorted-key compact JSON (UTF-8): the
questions + state the backends must judge. SHA-256 of **those** bytes is
`prompt_sha256` for equal-input and the cross-language fixture (sprint a-1).

### Amendment (Phase A.1, r-5)

**Status:** Active (Phase A.1) — landed in r-5.

- (Active, r-5) A question has two new fields, omitted from the bytes
  when they hold their default: `kind` (`QuestionKind::{Choice, Score,
  Noul}`, default `Choice`, not written for a choice question) and
  `descriptions` (one per choice; absent means each description is the
  choice label). `choice` has 2 to 255 choices, `score` 2 to 10 in level
  order, `noul` none; any other shape is `INPUT_INVALID`, checked by one
  validating constructor that deserialization also runs. A payload without these fields
  serializes byte-identically to the Phase A form, so `prompt_sha256` and
  the cross-language fixture are unchanged and the schema version stays
  `canonical.v1`. There is one payload type (`CanonicalJudgePayload`).

## ADR-SAG-0005 — Facade owns payload assembly

**Status:** Active

`sc-jev-check` is the **only** crate that runs extractors and builds
`PayloadBundle`. `run::build_request` is the public assembly entry for a
check file. `run_check` uses that same assembly. Backend crates consume
`JudgeRequest` and must not re-canonicalize. Composition roots call the
facade and do not call `PayloadBuilder`. Pre-send refusal for config/input
errors happens before any backend call (ADR-SAG-0004).

### Amendment (Phase A.1, r-4)

**Status:** Active (Phase A.1) — landed in r-4.

- (Active, r-4) `sc-jev-check` runs the four consumer extractor kinds
  (`file`, `table-column`, `subjects`, `command`); it is not the only
  producer of subjects (ADR-SAG-0015).

### Amendment (Phase A.1, r-2)

**Status:** Active (Phase A.1, landed in r-2).

- (Active, r-2) The public assembly entry point is `prepare`
  (ADR-SAG-0024). The sentences "`run::build_request` is the public assembly
  entry" and "`run_check` uses that same assembly" above are superseded.

### Amendment (Phase A.1, r-5)

**Status:** Active (Phase A.1) — landed in r-5.

- (Active, r-5) `CanonicalJudgePayload` is the only payload schema. Both backend families
  validate by deserializing it. The Jev HTTP body is the documented flat
  object `{model, state, questions}` built by serde from the canonical
  payload (no byte-splice); `prompt_sha256` stays on canonical bytes.
  ADR-SAG-0019 is unchanged: the documented Jev body is the "API shape the
  evaluator expects" for Jev, and Haiku/Luna still embed the same canonical
  bytes.

### Amendment (Phase A.1, r-7)

**Status:** Active (Phase A.1, r-7).

- (Active, r-7) `PayloadBundle`, `CanonicalJudgePayload` and `JudgeRequest`
  are `sc-decision-schema` types (ADR-SAG-0030). `sc-jev-check` is still the
  only crate that builds `PayloadBundle`, and backend crates still consume
  `JudgeRequest` without re-canonicalizing.

## ADR-SAG-0008 — Sessions carry prior answers to later backends

**Status:** Active

When multiple backends run in one invocation, **session state** is how a second
opinion receives prior answers. TypeSafe Jev has no rationale field on the wire;
Haiku/Luna session `ask` paths reuse stored context. Session files are UUID paths
with mode `0600` (sprint a-6).

### Amendment (Phase A.1, r-1 — interim shapes)

**Status:** Active (Phase A.1, landed in r-1). Bullet 1, except its last
sentence, is superseded by the r-2b amendment below and ADR-SAG-0025 (Active,
r-2b); bullet 2 (session document v2) stays in force.

- (Superseded by ADR-SAG-0025, r-2b; last sentence in force) ~~The run
  engine is unchanged in behaviour: later LLM backends still receive
  earlier answers as session context, and the record now says so
  (`EqualInputRecord.mode = ChainContext`).~~ Every backend gets the same
  `Arc<PayloadBundle>`.
- (Active, r-1) Session files are **document version 2**. A file with a
  missing or older version (Phase A, v1) or a newer one loads as
  `SessionDocumentInvalid` (`INPUT_INVALID`) with its own recovery text; v1
  sessions cannot be resumed. On load, every stored outcome is checked
  against the stored payload (`JudgeOutcome::check_against`).

### Amendment (Phase A.1, r-2b)

**Status:** Active (Phase A.1) — landed in r-2b.

- (Active, r-2b) Multi-backend runs are **independent by default**: no backend receives
  another's answers. `MultiBackendMode::ChainContext` passes prior successful
  LLM answers to later LLM backends through the system prompt, and the mode is
  recorded on `EqualInputRecord.mode`. Sessions continue to carry context for
  `ask`. See ADR-SAG-0025.

### Amendment (Phase A.1, r-6)

**Status:** Active (Phase A.1, r-6).

- (Active, r-6) The sentence "Session files are UUID paths" is
  superseded: session ids, session file names, the CLI `--session-id` value
  and Python session ids are **ULIDs** (ADR-SAG-0031); a UUID id is
  `INPUT_INVALID`. Mode `0600` is unchanged.
- (Active, r-6) The r-1 interim sentence "Session files are document
  version 2" is superseded: the session document is **version 3** (adds
  `decision_schema`, ADR-SAG-0030). Version 2 and older load as
  `SessionDocumentInvalid` (`INPUT_INVALID`) with "re-run the check"
  recovery. The rest of that bullet stays in force.

- (Active, r-6) Retention: opening a `FileSessionStore` removes session
  documents past the TTL plus one TTL of grace (version 1 and 2 documents
  included), so `SESSION_EXPIRED` stays reachable for one more TTL before a
  document reads as `SESSION_NOT_FOUND`.

## ADR-SAG-0009 — Local backend registers without caller changes

**Status:** Active

Future local judges (decider-4b / cyprid) implement `JudgeBackend` and register
in `sc-jev-check`’s backend registry. Callers keep `run_check(..., backends: &[BackendId])`;
no CLI or PyO3 API change when a local backend lands.

Amended by ADR-SAG-0022.

### Amendment (Phase A.1, r-1)

**Status:** Active (Phase A.1, landed in r-1).

- (Active, r-1) `BackendId` is `#[non_exhaustive]`. Static backend facts
  (`family`, `key_id`, `requires_llm_context`, `supports_session_ask`,
  `request_limit_bytes`) live on `BackendDescriptor` rows in `BACKENDS`;
  `JudgeBackend::capabilities()` is a provided method derived from the
  descriptor.
- (Active, r-1) Backend dispatch is a **test-and-review convention**, not a
  lint: the production factory is a table
  `&[(BackendId, fn(ResolvedApiKey) -> Arc<dyn JudgeBackend>)]` in
  `sc-jev-check::registry`, checked against `BACKENDS` by
  `test_every_backend_has_factory`. Everywhere else, code branches on
  descriptor facts (`family`, `requires_llm_context`,
  `supports_session_ask`, `request_limit_bytes`), never on `BackendId`
  equality or a `match`; review enforces it (no clippy lint expresses it).
- (Active, r-1) "Open" means additive: adding a built-in backend is a
  minor `sc-agent-types` change (variant + `BACKENDS` row) plus one factory
  table row, with no caller signature change (checklist in ADR-SAG-0027). It
  is not a runtime-open registry: `BackendRegistry::register` installs a
  double or scripted backend for an existing id and cannot add an id.
- (Active, r-1) `JudgeBackend` is intentionally open (not sealed); every
  method added after A.1 has a default body.

### Amendment (Phase A.1, r-2)

**Status:** Active (Phase A.1, landed in r-2).

- (Active, r-2) The sentence "Callers keep `run_check(..., backends:
  &[BackendId])`" is superseded: callers use `prepare`/`run` (ADR-SAG-0024)
  with `RunConfig::backends(BackendSet)`. `run_check` is deleted. A new
  backend still needs no CLI or PyO3 signature change.

### Amendment (Phase A.1, r-7)

**Status:** Active (Phase A.1, r-7).

- (Active, r-7) `JudgeBackend` and `BackendCapabilities` live in
  `sc-decision-schema` with the types their signatures name (ADR-SAG-0030);
  `capabilities()` is still derived from the `sc-agent-types` descriptor.
  `BackendId` names **how** a backend is called (Jev API, Anthropic API,
  OpenAI API); `ModelId` (model catalog in `sc-agent-types`) names **which**
  model. `BackendDescriptor` gains `default_model_alias`.
- (Active, r-7) `JudgeBackend::model_id()` is a provided method returning
  the descriptor's `default_model()` (no fallback); backends with a
  configured model override it, and a backend outside the catalog must
  implement it. `judge` takes the per-send `&SendTrace` (ADR-SAG-0031).

## ADR-SAG-0022 — Scripted backends are a test-util feature

**Status:** Active

Amends ADR-SAG-0009. Amended by ADR-SAG-0029.

Registration without a caller-signature change stays ADR-SAG-0009. This
decision records the test-only scripted judge. It is not a production backend
and not a second `run_check` policy.

- `sc-jev-check` Cargo feature `test-util` exposes
  `registry::register_scripted_backends`. It builds a `BackendRegistry` of
  in-process `JudgeBackend`s. Each judge returns the supplied answer object.
  It does not open a socket and does not store an API key. Callers pass that
  registry to `RunOptions::registry`. `run_check`'s parameters stay unchanged.
  A repeated backend id is `JevError::registry_invalid` (`CONFIG_INVALID`).
  An LLM backend id still requires `JudgeRequest.llm` before it returns
  (ADR-SAG-0019). A build that leaves the feature off does not export the
  function. This crate's own `cargo test` compiles the library with
  `cfg(test)`, so those tests see the function without enabling the feature.
  Every other crate, including `sc-jev-py`, sees it only by enabling
  `sc-jev-check/test-util`.
- `sc-jev-py` Cargo feature `test-util` enables `sc-jev-check/test-util` and
  adds keyword-only `scripted_answers` to `run_check`. That argument forwards
  to `register_scripted_backends`. This crate does not implement
  `JudgeBackend`. When the feature is off, production `run_check` has no
  `scripted_answers` parameter and does not register a judge. A value that is
  not a JSON object of question ids is `INPUT_INVALID` (ADR-SAG-0001). Omit
  the argument to use the HTTP backends inside `sc-jev-check`. `cargo test`
  of this crate enables `sc-jev-check/test-util` through its dev-dependency
  and compiles the Python signature with `cfg(test)`.
  *Superseded by ADR-SAG-0029 (b):* `run_check` has one signature with no
  `scripted_answers` parameter in any build. Scripted injection is the
  separate function `sc_jev_py._testing.run_check_scripted`, registered only
  under feature `test-util`.
- Default features of both crates leave `test-util` off. The publish wheel
  does not pass `--features test-util`. Sprint a-8 pytest may enable it so
  session `ask` runs without a network send. Enabling it does not change
  production registration.
  *Superseded in part by ADR-SAG-0029 (b):* the a-8 pytest calls
  `sc_jev_py._testing.run_check_scripted`, not a `run_check` argument.

### Amendment (Phase A.1, r-1)

**Status:** Active (Phase A.1, landed in r-1).

- (Active, r-1) `registry::ScriptedJudge` and `registry::RequestLog` are
  deleted; their callers use `sc_agent_types::test_util::MockJudgeBackend`
  (`requests()`, `validations()`, `with_provider_meta`, `with_failure`,
  `refusing`). `register_scripted_backends` keeps its signature and the
  string-shorthand normalisation, and builds `MockJudgeBackend`s; the
  `sc-jev-check` feature becomes `test-util = ["sc-agent-types/test-util"]`.
  Supersedes the ADR-SAG-0029 (b) sentence "`sc-jev-check` feature
  `test-util` makes `registry::ScriptedJudge`, `registry::RequestLog`, and
  `registry::register_scripted_backends` public": only
  `register_scripted_backends` stays.
- (Active, r-1) The sentence above "An LLM backend id still requires
  `JudgeRequest.llm` before it returns" still holds and is not superseded:
  `MockJudgeBackend` enforces it from the descriptor
  (`requires_llm_context()`), so `validate_for_send` (counted) and `judge`
  return `INPUT_INVALID` for an LLM id with no LLM context and record
  nothing.

### Amendment (Phase A.1, r-2)

**Status:** Active (Phase A.1, landed in r-2).

- (Active, r-2) The scripted registry is passed via `RunConfig::registry`
  (ADR-SAG-0024), not `RunOptions::registry`; the sentences "not a second
  `run_check` policy" and "`run_check`'s parameters stay unchanged" now read
  against `prepare`/`run`. `register_scripted_backends` and its `test-util`
  gating are unchanged. Correction: an integration test target does not
  compile the library with `cfg(test)`, so "this crate's own `cargo test` …
  sees the function without enabling the feature" holds only for unit
  tests; `sc-jev-check`'s integration targets enable `test-util` through
  `[[test]] required-features = ["test-util"]` and `--features test-util`
  on their validation lines (no self dev-dependency; `boundaries/README.md`
  "Dev-dependency and self edges"). For that one
  sentence this supersedes ADR-SAG-0029 (b)'s "Everything else in
  ADR-SAG-0022 stands"; 0029 (b)'s `run_check` signature and
  `_testing.run_check_scripted` rules are unchanged.

### Amendment (Phase A.1, r-2b)

**Status:** Active (Phase A.1) — landed in r-2b.

- (Active, r-2b) The scripted backend reports, as
  `JudgeOutcome.input_sha256`, the digest of the bundle it received, so
  equal-input attestation (ADR-SAG-0025) treats it like a real backend;
  tests that need a mismatch use `MockJudgeBackend::with_reported_digest`.
- (Active, r-2b) `register_scripted_backends` accepts a second JSON form,
  `{"backends": {<backend id>: <answers> | {"failure": {"error", "kind"?, "stage"?}}}}`,
  so one backend can fail (`stage` `judge`: `with_failure`; `validate`:
  `refusing`) while others answer; the a-10 shared form (question id →
  answer, with the ADR-SAG-0029 (b) string shorthand) is unchanged. The
  `sc-ai` hidden `--scripted-answers` file and
  `sc_jev_py._testing.run_check_scripted(scripted_answers=...)` take the
  same JSON (r-3b). Schema in sprint r-2b D5.

### Amendment (Phase A.1, r-7)

**Status:** Active (Phase A.1, r-7).

- (Active, r-7) `MockJudgeBackend` moves from `sc_agent_types::test_util`
  to `sc_decision_schema::test_util` (non-default `test-util` feature of
  `sc-decision-schema`); `sc-jev-check`'s `test-util` feature enables it and
  `register_scripted_backends` is built over it as before (ADR-SAG-0030).

## ADR-SAG-0019 — LLM wire envelope vs canonical payload

**Status:** Active

**Jev (TypeSafe)** sends the canonical payload in the API shape the evaluator
expects (including model selection on the wire).

**Haiku / Luna** send API-direct HTTP that is **not** byte-identical to Jev’s
body. They **must** embed the **same canonical payload bytes** in the user/judge
message (per README parity). A **system prompt** (evaluator instructions, JSON
schema hints) is allowed and **may change** over time without changing
`prompt_sha256`, as long as the embedded canonical blob is unchanged.

**System prompt injection (not hardcoded in Rust):** production crates must not
embed evaluator prose as string literals. Resolution **precedence** (first hit wins):

1. **`sc-ai jev --llm-system-prompt-file`** or `RunOptions.system_prompt_override`
   (working-tree read at invoke time).
2. Else **`check.toml`** `[llm_judge] system_prompt_file` at the same `rev` as
   other check inputs (`git show` when `rev` is not `WORKTREE`).
3. Else packaged default from **`crates/sc-jev-check/assets/jev.toml`** → asset path
   resolved via **`include_str!` at build time** into the wheel/binary (REQ-SAG-0503:
   prose lives in the asset file, not as Rust string literals).

Haiku and Luna require resolved text in `JudgeRequest.llm`. **Jev-only** runs set
`llm: None`; a missing prompt file is **not** an error when no LLM backend is selected.
When an LLM backend is selected and the resolved file is missing → `CONFIG_INVALID`
(or `PROMPT_FILE_MISSING` if added to the README contract in a-1).

`RunOptions` fields (facade): `repo_root`, `dry_run`, `system_prompt_override`,
`session_log: SharedSessionLog`.

Token/count checks apply to the full message the provider receives; equal-input
still records whether every backend saw the same canonical bytes.
*Amended by the r-1 interim amendment and the r-2b amendment below: `equal`
is derived from per-backend attestations computed from backend-reported
digests.*

### Amendment (Phase A.1, r-1 — interim shapes)

**Status:** Active (Phase A.1, landed in r-1). The 'Until r-2b …
`equal = false`' sentences of bullet 1 and the 'r-1 records `ChainContext`'
sentence of bullet 2 are superseded by the r-2b amendment below and
ADR-SAG-0025 (Active, r-2b); the rest stays in force.

- (Active, r-1; middle sentence superseded by ADR-SAG-0025, r-2b) The
  sentence "equal-input still records whether every backend saw the same
  canonical bytes" now reads: equal-input records, per backend, an
  `Attestation` (`Matched`, `Mismatched`, `Unattested`, `Failed`,
  `NotSent`), and `equal` is **derived**: true only when every backend is
  `Matched`. ~~Until r-2b lands backend-side hashing, no backend reports
  `JudgeOutcome.input_sha256`, so the engine records `Unattested` for every
  backend it sent to and **a live run reports `equal = false`**.~~ The
  engine still gives every backend the one shared bundle.
- (Active, r-1) `JudgeRequest` carries `Arc<PayloadBundle>` (one bundle per
  run) and a `CancelToken`; `JudgeRequest.llm` is unchanged.
  `MultiBackendMode` (`Independent` default, `ChainContext`) is recorded on
  `EqualInputRecord.mode`; ~~r-1 records `ChainContext`, its actual
  behaviour~~ (superseded by ADR-SAG-0025 bullet 1: the engine records the
  mode the caller chose, `Independent` by default).

### Amendment (Phase A.1, r-2)

**Status:** Active (Phase A.1, landed in r-2).

- (Active, r-2) The `RunOptions` field list above is replaced by
  `RunConfig` (ADR-SAG-0024). The prompt precedence is unchanged.

### Amendment (Phase A.1, r-2b)

**Status:** Active (Phase A.1) — landed in r-2b.

- (Active, r-2b) Equal-input records, per backend, an `Attestation`
  computed from `JudgeOutcome.input_sha256`: the digest of the canonical blob
  that backend recovered from its final serialized wire body. `equal` is true
  only if every backend's attestation is `Matched`. See ADR-SAG-0025.

### Amendment (Phase A.1, r-7)

**Status:** Active (Phase A.1, r-7).

- (Active, r-7) Haiku and Luna use the same decision schema as Jev: the
  input is the canonical payload bytes embedded unchanged (this ADR), and the
  model's JSON output is parsed as `sc_decision_schema::DecisionAnswers`, the
  answers map `JudgeOutcome` carries for Jev (ADR-SAG-0030). Provider
  envelopes stay in `sc-jev-backends`.

## ADR-SAG-0003 — Secret resolution

**Status:** Active

`sc-agent-secrets` resolves keys: **process env → repo-root `.env` → keyring**.
**Only `sc-jev-check`** calls `resolve_key`. `JevClient` and LLM backends receive
the resolved secret (and transport) at **construction**; they do **not** depend on
`sc-agent-secrets`. `JudgeRequest` carries neither keys nor `repo_root`.

Amended by ADR-SAG-0020.

### Amendment (Phase A.1, r-2)

**Status:** Active (Phase A.1, landed in r-2).

- (Active, r-2) "Repo root" for `.env` lookup is
  `sc_jev_check::run::discover_repo_root`: the git toplevel of the start
  directory, or the start directory (absolutized) only outside git (git
  finds no work tree and no ancestor holds `.git`) or when git is not
  installed; any other git failure is an error, so a subdirectory is never
  silently taken for the root. Both `sc-ai`
  and `sc-jev-py` use it. Behaviour change: `sc-jev-py` called from a
  subdirectory now reads the toplevel `.env` instead of the subdirectory's.

## ADR-SAG-0020 — Secret injection and redaction

**Status:** Active

Amends ADR-SAG-0003. Amended by ADR-SAG-0029.

Resolution order stays ADR-SAG-0003. This decision records the extension
points and the secret-bearing return type.

- `EnvSource` and `KeyringSource` are open injection traits. `resolve_key_with`
  takes them. Production `resolve_key` uses `ProcessEnv` and `PlatformKeyring`.
- Cargo feature `test-util` exposes the in-memory doubles `MapEnv` and
  `MapKeyring`. Production builds leave the feature off. The doubles are not
  a second resolution policy.
  *Superseded by ADR-SAG-0029 (a):* `sc-agent-secrets` has no `test-util`
  feature. `MapEnv` and `MapKeyring` are `#[cfg(test)]` crate-private code
  and are not exported.
- `resolve_key` and `resolve_key_with` return `ResolvedApiKey`. `Debug` and
  `Display` redact the secret. `as_str` is the only read, and only the
  transport that sends the key may call it.
  *Amended by ADR-SAG-0029 (a):* `ResolvedApiKey::try_new` is the one
  construction rule set, and `clippy.toml` `disallowed-methods` enforces the
  `as_str` rule.
- An unreadable repo-root `.env` or keyring is `CONFIG_INVALID`
  (`JevError::credential_store_invalid`). The cause is an I/O kind or a
  keyring variant label. It never includes secret bytes, passwords, or the
  raw platform error text. The fix hint names that cause and the recovery
  step. A miss stays `KEY_MISSING` (ADR-SAG-0001).
- This crate writes no log lines. Secrets in CLI output and logs remain
  NFR-SAG-0002 and ADR-SAG-0013.

### Amendment (Phase A.1, r-1)

**Status:** Active (Phase A.1, landed in r-1).

- (Active, r-1) `ResolvedApiKey::try_new` is the only constructor in every
  build; the unchecked `new` is removed, and test keys use `try_new`; the
  secret is held as `Arc<str>`. Backend constructors (`JevClient::new`,
  Haiku, Luna, and their `with_default_transport`) are infallible.
- (Active, r-1) Supersedes these ADR-SAG-0029 (a) sentences: "The
  unchecked `ResolvedApiKey::new` exists only under `cfg(test)` or the
  `sc-agent-types/test-util` feature, for test doubles" (it no longer
  exists), and, for the two sending transports, "carry
  `#[allow(clippy::disallowed_methods, reason = ...)]`" (from r-1 they carry
  `#[expect(..)]`; r-3 deletes both files).

### Amendment (Phase A.1, r-3)

**Status:** Active (Phase A.1) — landed in r-3.

- (Active, r-3) A secret travels only as a `SecretHeader` holding
  `ResolvedApiKey` inside `sc_agent_http::HttpRequest`. The single read
  site is a **lint-enforced workspace convention, not a type seal**:
  `ResolvedApiKey::as_str` is listed in `clippy.toml`
  `disallowed-methods`; its one call site,
  `crates/sc-agent-http/src/ureq_transport.rs`, carries
  `#[expect(clippy::disallowed_methods, reason = ...)]`; workspace lints
  deny `allow_attributes` and `allow_attributes_without_reason`; every
  crate has `[lints] workspace = true`, checked locally by r-3's
  validation; and an `rg` check finds `disallowed_methods` in no file
  under `crates/` other than that call site. Crates outside the
  workspace are not constrained. Redaction is applied once, in
  `send_with_busy_retry`, to every error it returns, including errors a
  `Transport` impl returns; a caller of `Transport::send` directly
  bypasses it.
- (Active, r-3) The `Transport` trait is open; test transports see
  `SecretHeader::redacted()` / `matches()` only.
- (Active, r-3) Supersedes the ADR-SAG-0029 (a) sentence naming
  `crates/sc-jev/src/client/https.rs` and
  `crates/sc-jev-backends/src/transport.rs` as "the only production
  readers": r-3 deletes both, and the one reader is
  `crates/sc-agent-http/src/ureq_transport.rs`. That `sc-agent-secrets` has
  no Cargo features is ADR-SAG-0029 (a) and is not restated here.

### Amendment (Phase A.1, r-3b)

**Status:** Active (Phase A.1) — landed in r-3b.

- (Active, r-3b) `workspace-tests --test source_scans` is the
  cross-crate proof of the ADR-SAG-0020 r-3 read-site convention:
  `test_disallowed_methods_expect_sites` fails if an `expect` naming
  `clippy::disallowed_methods` (item- or crate-level, also inside
  `cfg_attr`) appears in any file of any target (`src`, `tests`,
  `benches`, `examples`, `build.rs`) other than
  `crates/sc-agent-http/src/ureq_transport.rs` and the process-spawn sites
  ADR-SAG-0028 lists, and a spawn-site file must not contain
  `ResolvedApiKey`; `test_every_crate_inherits_workspace_lints` fails if a
  crate stops inheriting the workspace lints or adds a per-crate
  `[lints.*]` override. From r-3b this replaces r-3's local `rg` check.

## ADR-SAG-0004 — One check, one request

**Status:** Active

No splitting oversized inputs. Pre-send byte/token validation on every backend
via `validate_for_send` before `judge`.

### Amendment (Phase A.1, r-2b)

**Status:** Active (Phase A.1) — landed in r-2b.

- (Active, r-2b) "Before `judge`" applies to the request actually sent. In
  `MultiBackendMode::ChainContext`, each augmented request is re-validated
  immediately before its own send; a refusal there marks that backend
  `Failed` (`leak = true`) and sends nothing for it. The base request is still
  validated on every backend before the first send (ADR-SAG-0025).

## ADR-SAG-0006 — Transport injection

**Status:** Active

Jev HTTP client lives in `sc-jev`; LLM HTTP in `sc-jev-backends`. Transports are
**injected at backend construction** (not passed into `JudgeBackend::judge`).
Both use test doubles for unit tests.

### Amendment (Phase A.1, r-3)

**Status:** Active (Phase A.1) — landed in r-3.

- (Active, r-3) HTTP transport, retry, caps and redaction live in `sc-agent-http`
  (ADR-SAG-0026). `sc-jev` and `sc-jev-backends` are wire-format crates that
  take an injected `sc_agent_http::Transport` at construction. Injection at
  construction (not per `judge`) is unchanged.

## ADR-SAG-0007 — API-direct LLM backends

**Status:** Active

Haiku and Luna judges use **provider HTTP only** in Phase A: no vendor SDK,
agent harness, or tool-use loop. Wire format and token counting are implemented
in `sc-jev-backends` against documented REST endpoints. **HTTP timeouts:** connect
10s, request 120s (configurable on backend builder for tests); busy retry per
REQ-SAG-0505. *Superseded in part by the r-3 amendment below:* timeouts are
configured on the injected `UreqTransport`, not on a backend builder.

### Amendment (Phase A.1, r-3)

**Status:** Active (Phase A.1) — landed in r-3. Numbers decided by the
owner (2026-11-06).

- (Active, r-3) One busy-retry policy for every backend, in
  `sc_agent_http::BusyPolicy`: statuses 429/502/529 only; at most 4 sends;
  a valid Retry-After (integer or decimal seconds) is honoured, clamped to
  0–30 s. A valid Retry-After is 1–32 bytes of ASCII digits with an
  optional `.` and ASCII-digit fraction; anything else (HTTP-date, sign,
  exponent, `inf`, `NaN`, empty) is malformed. When Retry-After is absent
  **or malformed** (treated the same), the wait backs off exponentially from
  1 s (1, 2, 4, 8 … capped at 30 s); with the default 4 sends at most three
  waits occur (1, 2, 4 s), so 8 s and the cap apply only to a non-default
  policy. Every wait adds up to 250 ms jitter. A 90 s budget from the first send governs
  starting new attempts: no attempt starts after it, and every wait is capped
  to the remaining budget; an in-flight attempt keeps its 120 s request
  timeout (covering the body read). A `CancelToken` is checked before each
  attempt and during sliced waits (≤ 100 ms); a cancel during an in-flight
  request is observed when it returns, so worst-case cancel latency is the
  120 s request timeout. A transport error or timeout after the body was
  sent is never retried. A 502 retry can double-charge a provider
  that processed the request.
- (Active, r-3) No network-connectivity detection and no indefinite wait on
  network loss: connect and DNS failures return `NETWORK_ERROR`
  (`NetworkErrorKind::Connect`, `retryable() == true`) at once, and the caller
  (CLI, Python, future MCP) decides whether to retry.
- (Active, r-3) HTTP timeouts (connect 10 s, request 120 s covering the
  body read) are construction state of the injected transport,
  `UreqTransport::new(Timeouts)`; `with_default_transport` uses
  `Timeouts::default()`, and a test or caller that needs other values
  passes `new(key, UreqTransport::new(Timeouts::new(..)))`. The backend
  builder `with_timeouts` is removed. This supersedes the base sentence
  "configurable on backend builder for tests".

## ADR-SAG-0010 — CLI thin adapter

**Status:** Active

`sc-ai` parses argv and prints envelopes; no network code in the binary crate.
Depends on **`sc-jev-check`** + full `sc-observability`, not on `sc-jev` directly.

### Amendment (Phase A.1, r-3b)

**Status:** Active (Phase A.1) — landed in r-3b.

- (Active, r-3b) `sc-ai` depends on `sc-jev-host` (observability adapter, path policy)
  and `sc-jev-check`. Argv parsing uses clap derive behind the default `cli`
  feature. There is still no network code in the binary crate. It no longer
  depends on `sc-observability` directly (ADR-SAG-0013 amendment).

## ADR-SAG-0011 — Python via sc-jev-py wheel

**Status:** Active

Python apps use the **`sc-jev-py`** PyPI extension (PyO3 → `sc-jev-check`). Optional
thin wrappers (e.g. atm-bd `packages/jev-check`) must not reimplement Jev HTTP
or canonicalization.

### Amendment (Phase A.1, r-3b)

**Status:** Active (Phase A.1) — landed in r-3b.

- (Active, r-3b) The wheel's path is PyO3 → `sc-jev-host` →
  `sc-jev-check`. `sc-jev-py` keeps only argument conversion, the GIL release
  and exception mapping; path policy, logging adapter and run wiring are
  `sc-jev-host`'s (`PathPolicy::python()`).

## ADR-SAG-0018 — sc-publish kit owns release surface

**Status:** Active

Release manifests, channel contracts, and publish workflows are **installed**
from [sc-publish](https://github.com/randlee/sc-publish) via
`release/sc-publish-consumer-input.json`. Edit consumer input + re-run
`install.py`; do not hand-edit rendered TOML under `release/`.

## ADR-SAG-0017 — First release surfaces

**Status:** Active

Phase A first release ships **five publish channels** from one tag via sc-publish:
**crates.io**, **PyPI** (`sc-jev-py`), **Homebrew**, **Scoop**, and **WinGet**
(`sc-ai` binary), plus GitHub Release assets. Sprint **a-9** closes
REQ-SAG-0130, REQ-SAG-0131, REQ-SAG-0132, and REQ-SAG-0803 (kit install,
preflight CI, and publish dry-run).

### Amendment (Phase A.1, r-4)

**Status:** Active (Phase A.1) — landed in r-4.

- (Active, r-4) Two released binaries: `sc-ai` and `sc-treesitter`
  (ADR-SAG-0023). Per-channel coverage, matching
  `release/sc-publish-consumer-input.json` and the rendered
  `release/publish-artifacts.toml`:

  | Channel | sc-ai | sc-treesitter |
  | --- | --- | --- |
  | GitHub Release archive | yes | yes |
  | crates.io (`cargo install`) | yes | yes |
  | Homebrew | yes | yes (same formula) |
  | Scoop | shim | in archive, no shim |
  | WinGet | alias | in package, no alias |

  Scoop: `[channels.scoop].binary` is one string at the pinned kit, so
  sc-agent's consumer input still has a single Scoop binary. The upstream
  change landed in sc-publish as https://github.com/randlee/sc-publish/pull/176 (optional
  `[channels.scoop].binaries`). The owner directed a PR instead of an issue.
  sc-agent gets the second shim only after it re-pins the kit and sets
  `binaries` (not part of this amendment). Bead `agt-scoop-multi-shim`.

  WinGet: command aliases are not an upstream issue. The first manifest is
  submitted by hand and must list a `PortableCommandAlias` for each of
  `sc-ai` and `sc-treesitter` (bead `agt-sft`; the related upstream PR
  randlee/sc-publish#182 is open). The Windows invocation path for
  `sc-treesitter` is in `docs/development/sc-publish.md`. REQ-SAG-0133.

### Amendment (Phase A.1, r-3)

**Status:** Active (Phase A.1) — landed in r-3.

- (Active, r-3) Published library crates gain `sc-agent-http`. crates.io
  publish order: `sc-agent-types`, `sc-agent-secrets`, `sc-agent-http`,
  `sc-jev`, `sc-jev-backends`, `sc-jev-check`, `sc-treesitter`, `sc-ai`.

### Amendment (Phase A.1, r-3b)

**Status:** Active (Phase A.1) — landed in r-3b.

- (Active, r-3b) Published library crates gain `sc-jev-host`
  (internal support, owner decision 2026-11-06), published directly after
  `sc-jev-check`. `workspace-tests` is `publish = false`. No crate is
  published to crates.io until r-5 (D9) lands (plan.md decision (c)).

### Amendment (Phase A.1, r-7)

**Status:** Active (Phase A.1, r-7).

- (Active, r-7) Published library crates gain `sc-decision-schema`,
  published second, directly after `sc-agent-types` (its only workspace
  dependency). crates.io order: `sc-agent-types`, `sc-decision-schema`,
  `sc-agent-secrets`, `sc-agent-http`, `sc-jev`, `sc-jev-backends`,
  `sc-jev-check`, `sc-jev-host`, `sc-treesitter`, `sc-ai`. No crate is
  published to crates.io until r-5 (D9) lands (unchanged).

## ADR-SAG-0015 — Subjects artifact outside the judge

**Status:** Active

**Boundary (one line):** extraction produces the **subjects artifact outside the
judge**; the judge consumes it and **links no parser**.

Replaces delimited comment regions and in-crate tree-sitter (third review,
Rand 2026-11-02). **Normative artifact:** [`docs/sc-agent-types/subjects-schema.md`](sc-agent-types/subjects-schema.md) (`sc/subjects/1`). **Producer:** [`sc-treesitter`](sc-treesitter/architecture.md) (pinned static grammars, query assets, `matches` mode). **Consumer:** [`docs/sc-jev-check/extraction.md`](sc-jev-check/extraction.md) — kinds `file`, `table-column`, `subjects`, `command` only.

Delimited regions, in-facade tree-sitter, and Python `python-branches` extractor are **out of scope** for Phase A.

LLM system prompt loading: **ADR-SAG-0019** (`[llm_judge] system_prompt_file` in
check.toml; default asset under `sc-jev-check`).

## ADR-SAG-0013 — Session JSONL via sc-observability v2

**Status:** Active

Jev session/prompt logging uses **`sc-observability` v2** (`LoggerBuilder`,
file/console sinks). **`sc-jev-check`** emits `LogEvent`s through
**`SharedSessionLog`** (see ADR-SAG-0016). Composition roots **`sc-ai`** and
**`sc-jev-py`** link full `sc-observability` and adapt to `SharedSessionLog`.
**`sc-jev`** does not log and does not depend on observability crates. No bespoke
JSONL writers. Secrets never appear in log lines. Path precedence for the two
composition roots is ADR-SAG-0021.

### Amendment (Phase A.1, r-3b)

**Status:** Active (Phase A.1) — landed in r-3b.

- (Active, r-3b) The sentence "Composition roots `sc-ai` and `sc-jev-py`
  link full `sc-observability`" is superseded: `sc-jev-host` is the only
  crate that links `sc-observability` (`sc-lint.toml` ownership record) and
  holds the one `SessionLog` adapter; both roots use it.

### Amendment (Phase A.1, r-6)

**Status:** Active (Phase A.1, r-6).

- (Active, r-6) Each send's log line carries one ADR-SAG-0031 row as the
  v2 envelope's `fields{}` object; "No bespoke JSONL writers" stands. The
  row's outcome key is `send_outcome`; the envelope's own `outcome` stays
  unset. "Secrets never appear in log lines" stands; the row's one path is
  `repo`.

- (Active, r-6) Envelope fields: `correlation_id` is the session id and
  `request_id` is the answer id; the level is `Warn` when `send_outcome` is
  `failed`, otherwise `Info`. `fields{}` is unchanged.

## ADR-SAG-0021 — Log and session path precedence

**Status:** Active

Amends ADR-SAG-0013. The v2 file sink and the ban on bespoke JSONL writers
stay ADR-SAG-0013. This decision records where each composition root puts
that sink and its session files. First match wins. An empty flag, argument,
or environment value is absent.

**`sc-ai` (CLI):**

1. Log root: `--log-dir`, then `SC_LOG_ROOT`, then `<repo-root>/.sc-ai`.
2. Session directory: `--sessions-dir`, then `<repo-root>/.sc-ai/sessions`.
3. JSONL path: `<log-root>/logs/sc-ai.log.jsonl`.

**`sc-jev-py` (Python):**

1. Log root: `log_dir`, then `SC_AGENT_LOG_DIR`, then `SC_LOG_ROOT`, then
   `~/.sc-agent/log` (`%USERPROFILE%\.sc-agent\log` when `HOME` is unset; the
   process temp directory plus `sc-agent/log` when both are unset).
2. Session directory: `session_dir`, then `SC_AGENT_SESSION_DIR`, then
   `{log_root}/sessions`.
3. JSONL path: `{log_root}/logs/sc-jev-py.log.jsonl`.

`resolve_log_root` and `resolve_session_dir` are the Python contract for that
order. The two roots do not share a default directory. Secrets never appear
in either log (ADR-SAG-0013).

## ADR-SAG-0016 — v2 shim in sc-agent-types; adapter in sc-jev-host

**Status:** Active

Do **not** publish a separate observability crate. For the **first release or two**,
**`sc-agent-types::observability::v2`** holds the injectable `SessionLog` port
(aligned with registry `v2::Logger`). Temporary dependency:
`sc-observability-types = "1.5.0"` on **`sc-agent-types` only**
(*superseded by the r-1 amendment below: the dependency is removed*); full
`sc-observability = "1.5.0"` on composition roots **`sc-ai`** and **`sc-jev-py`**.
Libraries take only **`SharedSessionLog`**. When upstream v2 ships, **delete
the module** and drop the types dependency if possible (bead:
`agt-obs-v2-shim-removal`).

### Amendment (Phase A.1, r-4)

**Status:** Active (Phase A.1) — landed in r-4.

- (Active, r-4) The pin is exact: `sc-observability = "=1.5.0"`, a
  deliberate lockstep per ADR-SAG-0023. The version text `"1.5.0"` above is
  superseded by `=1.5.0` (exact).

### Amendment (Phase A.1, r-1)

**Status:** Active (Phase A.1, landed in r-1).

- (Active, r-1) `sc-agent-types` no longer depends on
  `sc-observability-types`, and the root workspace entry is deleted (no
  user remains). `observability::v2::SessionLog::emit` returns the local
  `LogRejected`. The `observability` module is behind the default-on `obs`
  feature (ADR-SAG-0027). REQ-SAG-0120/0121 are amended to match.

### Amendment (Phase A.1, r-3b)

**Status:** Active (Phase A.1) — landed in r-3b.

- (Active, r-3b) "Do not publish a separate observability crate" and "no
  extra crate" are superseded for `sc-jev-host`: the `sc-observability`
  adapter lives once in that crate, which is published to crates.io as
  internal support, `=`-pinned by `sc-ai` and `sc-jev-py`, with no semver
  promise of its own (owner decision 2026-11-06; ADR-SAG-0026). The port
  stays in `sc-agent-types`.
- (Active, r-3b) This ADR is retitled "v2 shim in sc-agent-types; adapter
  in sc-jev-host" (heading and ADR index title cell), since "(no extra
  crate)" no longer holds.

### Amendment (Phase A.1, r-7)

**Status:** Active (Phase A.1, r-7).

- (Active, r-7) The `SessionLog` port stays in `sc-agent-types`. The
  session-log record types and SQLite DDL are data in
  `sc_decision_schema::session_log` (ADR-SAG-0031 "Storage schema"), and the
  `judge` / `payload` modules leave `sc-agent-types` (ADR-SAG-0030).

### Amendment (Phase A.1, r-6)

**Status:** Active (Phase A.1, r-6).

- (Active, r-6) `observability::v2::LogEvent` carries one row as a JSON
  object: `LogEvent::new(session_id, operation, fields)` with `fields()`; the
  `backend()` / `prompt_sha256()` accessors are removed (the values are in
  `fields`). The port does not name the row type (ADR-SAG-0031 "Wiring").

- (Active, r-6) The adapter maps the row onto the v2 envelope: `request_id` is
  the row's `answer_id` and `correlation_id` is the session id (both ULIDs,
  built through serde because `sc-observability-types` is not a direct
  dependency), and the level follows `send_outcome` (ADR-SAG-0013 r-6).

## ADR-SAG-0014 — CLI success body

**Status:** Active

On success, stdout is the versioned sc-ai-cli envelope
`{ "version", "operation", "data" }` where **`data`** holds the **verbatim Jev
response JSON** for the operation plus **`session_id`** (REQ-SAG-0710). LLM
backends normalize to the same answer shape before printing.
*Amended by the r-1 interim amendment below: `answers` is written once from
the typed answers; other per-answer keys stay in `provider_meta`.*

Amended by ADR-SAG-0024 (Active, r-2).

### Amendment (Phase A.1, r-1 — interim shapes)

**Status:** Active (Phase A.1, landed in r-1). Superseded by ADR-SAG-0024
(r-2) when that lands.

- (Active, r-1) The data that `data` is built from is typed. For a Jev
  primary outcome, the response fields other than `answers` are copied from
  `provider_meta` verbatim, and `answers` is written **once**, from the typed
  answers (`choice`, `confidence`, `probabilities`; byte-identical to the Jev
  answer objects for those keys). A per-answer key outside those three
  survives only in `provider_meta` (plan CPR-203 option (a)). For an LLM
  primary outcome, `answers`, `model` and `usage` are as before.
- (Active, r-1) Each `outcomes[]` entry is a `JudgeOutcome`: `answers`
  (question id → `{choice, confidence?, probabilities?}`), `backend`,
  `input_sha256` (null until r-2b backend hashing) and `provider_meta`. The
  Phase A `probabilities` field is removed (it was derived from `answers`).
- (Active, r-1) `equal_input` is `{prompt_sha256, mode, backends: [{backend,
  attestation: {status}}], leak, equal, producer}`; `commands` is removed
  and `equal` is derived (ADR-SAG-0019 r-1 amendment). REQ-SAG-0710 holds
  with these shapes.

### Amendment (Phase A.1, r-2)

**Status:** Active (Phase A.1, landed in r-2).

- (Active, r-2) `data` is the single serialization of
  `sc_jev_check::run::RunResponse` (ADR-SAG-0024), printed unchanged by every
  consumer. The **primary backend** is the first backend in `BackendSet`
  order whose status is `Ok`; `primary_backend` names it. If it is Jev, its
  response fields other than `answers` are copied to the top level verbatim
  and `answers` is written once, from the typed answers (`choice`,
  `confidence`, `probabilities`, byte-identical to the Jev answer objects for
  those keys), so no key appears twice (the sentence above still holds for
  that case); if it is an LLM backend (Jev failed or
  was not selected), its normalized `answers`, `model` and `usage` are at the
  top level and no Jev answer is. Then `session_id` (null unless
  `session_saved`), `session_saved`, `prompt_sha256`, `payload_bytes`,
  `results` (per backend: `ok` + outcome, or `failed` + the
  `ScAgentErrorResponse` derived from its error), `equal_input`, dry-run
  `payload`/`limits`, and last `warnings` (array of strings, omitted when
  empty). `warnings` replaces the Phase A `data.warning` key; the
  composition crate adds the log-flush warning through
  `RunResponse::with_warning` (r-3b). The `ask` body is the follow-up's
  answer fields by the same primary rule, then `session_id`,
  `prompt_sha256` and `warnings` (the CR-014 flush warning, through
  `AskResponse::with_warning`). The `{version, operation, data}`
  wrapper is `sc_jev_check::run::Envelope`; `readback` is the only key a
  consumer adds (`sc-ai`). REQ-SAG-0710 is amended to match.

## ADR-SAG-0012 — sc-jev owns Jev API

**Status:** Active

Every TypeSafe Jev HTTP operation is implemented in `sc-jev`. Other crates
must not send Jev API requests. Phase A ships the check/judge slice; remaining
endpoints extend `sc-jev` in place.

### Amendment (Phase A.1, r-5)

**Status:** Active (Phase A.1) — landed in r-5.

- (Active, r-5) The Jev request and response shapes are versioned data
  types in `sc_decision_schema::jev` (one serde struct per API call,
  ADR-SAG-0030). `sc-jev` still performs every Jev HTTP operation: the
  endpoint, sending the body, status mapping, busy retry and the call site of
  the `input_sha256` recovery. The `jev::v1` data types own parsing and the
  pure response-to-`DecisionAnswers` mapping (`CheckResponse::into_answers`),
  which has no HTTP and no I/O. No other crate sends Jev requests.

## ADR-SAG-0029 — Phase A hardening amendments

**Status:** Active

Amends ADR-SAG-0020 and ADR-SAG-0022. Recorded by the Phase A phase-end
hardening sprint (a-10, PR 20). The earlier ADR text stays in place; the
sentences this decision replaces carry a *Superseded by ADR-SAG-0029*
pointer.

**(a) Secrets (amends ADR-SAG-0020).**

- `sc-agent-secrets` has no Cargo features. The in-memory doubles `MapEnv`
  and `MapKeyring` are `#[cfg(test)]` code inside the crate
  (`crates/sc-agent-secrets/src/resolve.rs`) and are not exported. Other
  crates inject their own `EnvSource` / `KeyringSource` implementations
  through `resolve_key_with`.
- `ResolvedApiKey::try_new(BackendKeyId, String)` is the single construction
  rule set: it trims the value, maps an empty value to `KEY_MISSING`, and
  maps interior whitespace or control characters to
  `JevError::credential_store_invalid` (`CONFIG_INVALID`). Every resolution
  path in `sc-agent-secrets` goes through it. The unchecked
  `ResolvedApiKey::new` exists only under `cfg(test)` or the
  `sc-agent-types/test-util` feature, for test doubles.
- `clippy.toml` `disallowed-methods` lists `ResolvedApiKey::as_str`. The two
  sending transports (`crates/sc-jev/src/client/https.rs` and
  `crates/sc-jev-backends/src/transport.rs`) are the only production readers
  and carry `#[allow(clippy::disallowed_methods, reason = ...)]`.

**(b) Scripted backends in `sc-jev-py` (amends ADR-SAG-0022).**

- `sc_jev_py.run_check` has one signature in every build. It never takes
  `scripted_answers`.
- Scripted injection is `sc_jev_py._testing.run_check_scripted(check_path, *,
  scripted_answers, ...)`. The `_testing` submodule is registered only when
  the `sc-jev-py` feature `test-util` is on. The publish wheel leaves it off,
  so the wheel has no `_testing` submodule.
- `sc-jev-check` feature `test-util` makes `registry::ScriptedJudge`,
  `registry::RequestLog`, and `registry::register_scripted_backends` public.
  The string shorthand (`"yes"` for `{"choice": "yes"}`) is normalised inside
  `register_scripted_backends`, so every caller gets the same rule.
- Everything else in ADR-SAG-0022 stands: no production registration, no
  socket, no API key, default features off.

**(c) Error-code dispositions (upholds ADR-SAG-0001, REQ-SAG-0101,
REQ-SAG-0800).** Phase A hardening moved some failures to a different
existing id. Every id below is in
`tests/fixtures/jev/README-contract/errors.json` and is a `JevError`
variant; no id was added or removed, and `JevError::code()` still matches
the snapshot (test `codes_match_readme_contract_snapshot`).

| Surface | Failure | Before | After |
| --- | --- | --- | --- |
| `sc_jev_py.run_check` / `_testing.run_check_scripted` | unknown backend name | `CONFIG_INVALID` | `INPUT_INVALID` (`BackendId::from_str`, same as `sc-ai` and `check.toml`) |
| same | kebab-case alias (`anthropic-haiku`, `openai-luna`) | accepted | `INPUT_INVALID` (one documented spelling) |
| any crate | `serde_json` encode / decode failure (`JevError::encode_failed`) | mixed (`INPUT_INVALID`, `EXTRACTION_FAILED`) or raw text | `INPUT_INVALID`, detail keeps only category and line/column |
| any crate | file or process I/O failure (`JevError::io_failed`) | mixed (`INPUT_MISSING` for any read error, `EXTRACTION_FAILED` for git spawn) | `INPUT_INVALID` with the `io::ErrorKind`; a missing file stays `INPUT_MISSING` |
| `sc-treesitter` CLI | missing positional source path | `INPUT_MISSING` | `INPUT_INVALID` (an argument error, not a missing file) |
| `sc-treesitter` library | span or line number does not fit `u32` | `INPUT_TOO_LARGE` | `INPUT_INVALID` naming the field and value |
| `sc-treesitter` CLI | revision starting with `-`, path outside the repo or with a forbidden component, unresolvable revision, non-blob object | mixed | `INPUT_INVALID` (absent path at a revision stays `INPUT_MISSING`) |
| `sc-treesitter` CLI | blob or worktree file over 8 MiB | read unbounded | `INPUT_TOO_LARGE` |

Python exceptions carry the same `code` as the Rust `JevError`
(REQ-SAG-0800); the change is which existing id a failure maps to.

**Deferred.** ADR-SAG-0016 still names `"1.5.0"` while the workspace pins
`"=1.5.0"` (arch-qa ARCH-004). The version-form text is left to the D8
amendment (`PHASE-A-REVIEW.md` section 4).

Phase A.1 amendments that replace a 0029 sentence name it: ADR-SAG-0020 r-1 and
r-3, ADR-SAG-0022 r-1 and r-2 (plan.md "Plan PR route").

## Rust API patterns (Phase A)

**Status:** Active

- **Digest newtypes:** `Sha256Digest` (lowercase hex, 64 chars) for
  `PayloadBundle.sha256`, `EqualInputRecord.prompt_sha256`, and log fields — not
  unconstrained `String` (RBP-F002).
- **Domain newtypes:** `RepoRoot`, `Rev`, `SessionId`, `BackendKeyId` in
  `sc-agent-types` (frozen in a-1); secrets APIs take `&RepoRoot` not raw paths
  where the contract requires repo context (RBP-F003).
- **Resolved secrets:** `ResolvedApiKey` in `sc-agent-types`, returned by
  `sc-agent-secrets::resolve_key` (ADR-SAG-0020). `Debug` and `Display` redact.
  `as_str` is the only read of the secret, and only the sending transport calls it.
  `try_new` is the only production constructor; `clippy.toml` enforces the
  `as_str` rule (ADR-SAG-0029).
- **Extractor sealing:** `Extractor` is `pub trait Extractor: private::Sealed` —
  only the four Phase A kinds implement it; external crates cannot add kinds
  without a new ADR (RBP-F004).
- **Backend dispatch:** `sc-jev-check` holds `Vec<Arc<dyn JudgeBackend>>` (or
  equivalent registry) for multi-backend runs (RBP-F005).

### Amendment (Phase A.1, r-2)

**Status:** Active (Phase A.1, landed in r-2).

- (Active, r-2) The "Extractor sealing" bullet (RBP-F004) is withdrawn: the
  `Extractor` trait is removed and `extract_input` matches the four
  `config::InputSpec` kinds directly. A new kind is an `InputSpec` variant
  plus an ADR.

## Roadmap — API agents vs CLI wrappers

| Approach | Status |
| --- | --- |
| API-direct Anthropic / OpenAI (Haiku/Luna in `sc-jev-check` flow; future `sc-ai` agents) | **Wanted** — Phase A and Phase B (standalone) |
| Subprocess JSON bridges to claude-code, codex-cli, cursor-cli | **Out of scope for Phase A and B**; a later agent kind (ADR-SAG-0030 scope context) |

Future agent crates sit beside `sc-jev` and depend on `sc-agent-types` +
`sc-agent-secrets`; they do not duplicate Jev HTTP or payload rules from `sc-jev` when running
comparison flows.

## Related documents

- [`requirements.md`](requirements.md) — REQ-SAG ids
- [`design.md`](design.md) — goals and non-goals
- [`framework-charter.md`](framework-charter.md) — multi-consumer Rust posture
- [`plans/phase-a/plan.md`](plans/phase-a/plan.md) — Phase A sprints and boundaries
- Per-crate: `docs/sc-agent-types/`, `docs/sc-jev-check/` (+ [`extraction.md`](sc-jev-check/extraction.md)), `docs/sc-jev/`, etc.


## ADR-SAG-0023 — Dependency pins, allocator, released binaries

**Status:** Active (Phase A.1) — landed in r-4; its r-3b subsection landed in r-3b.

- (Active, r-4) Library crates declare caret ranges. Exact `=` pins are
  allowed only for deliberate lockstep. The list below, between the markers,
  is the **only** copy: `.github/scripts/check-exact-pins.sh` reads it and
  fails on any `=` requirement in any `Cargo.toml` whose crate is not on it.
  NFR-SAG-0004. Reasons: `sc-observability` — v2 shim alignment;
  `tree-sitter`, `tree-sitter-python`, `tree-sitter-rust` — output
  determinism; `sc-jev-host` — lockstep internal support (owner
  decision 2026-11-06), in force from r-3b (subsection below; before the
  crate exists no `Cargo.toml` names it). `sc-observability-types` is not on
  the list: r-1 removed its only user and its root entry. The script reads
  the first word of each line as the crate name; the rest of the line is a
  note.

  <!-- lockstep-pins:start -->
  sc-observability
  tree-sitter
  tree-sitter-python
  tree-sitter-rust
  sc-jev-host
  <!-- lockstep-pins:end -->

- (Active, r-4) Global allocators and argv parsers are linked only by
  binaries: `mimalloc` and `clap` are optional behind each binary crate's
  default `cli` feature, and the `[[bin]]` targets use
  `required-features = ["cli"]`. Libraries build with
  `--no-default-features` without mimalloc (REQ-SAG-0134) or clap
  (REQ-SAG-0135).
- (Active, r-4) Released binaries are `sc-ai` and `sc-treesitter`.
  Per-channel coverage is ADR-SAG-0017's r-4 amendment.

### Amendment (Phase A.1, r-3b)

**Status:** Active (Phase A.1) — landed in r-3b.

- (Active, r-3b) The intra-workspace `sc-jev-host` dependency of
  `sc-ai` and `sc-jev-py` is `=`-pinned (it is already on the list above);
  `check-exact-pins.sh` re-runs green with the pin present.

## ADR-SAG-0024 — Engine entry point and response contract

**Status:** Active (Phase A.1, landed in r-2). Amends ADR-SAG-0003,
ADR-SAG-0005, ADR-SAG-0009, ADR-SAG-0014, ADR-SAG-0019, ADR-SAG-0022.

- (Active, r-2) `sc-jev-check`'s only run API is
  `prepare(CheckSource, RunConfig) -> PreparedRun` then
  `run(PreparedRun) -> RunResponse`, plus `discover_repo_root(&Path)`.
  `PreparedRun` owns its config. `build_request`, `run_check`, `run_request`
  and `prepare_caller_request` are deleted (no shims; nothing published).
- (Active, r-2) Backend selection: `RunConfig::backends` if set, else the
  source document's `backends`, else `[jev]`. `prepare` and dry-run read only
  static `BackendDescriptor` facts and construct no backend.
- (Active, r-2) `RunResponse` (private fields) is the one result type
  every consumer (CLI, PyO3, future MCP) prints, and its single `Serialize`
  is the ADR-SAG-0014 `data` body (ADR-SAG-0014 r-2 amendment): the primary
  backend (first `Ok` in `BackendSet` order, possibly an LLM backend)
  contributes its answer fields at the top level, named by
  `primary_backend` (for Jev: its response fields except `answers`, then
  `answers` once from the typed answers), followed by `session_id` (null unless `session_saved`),
  `session_saved`, `prompt_sha256`, `payload_bytes`, `results` (per backend
  `ok` + outcome or `failed` + `ScAgentErrorResponse`), `equal_input` and
  `warnings` (omitted when empty; `RunResponse::with_warning` is how a
  composition root adds one). A `BackendFailure` stores only the `JevError`;
  its serialized error object is derived from it at serialize time. The
  `{version, operation, data}` wrapper is `sc_jev_check::run::Envelope`.
  `RunResponse::first_failure()` / `is_partial()` decide exit codes and
  exceptions for every consumer. REQ-SAG-0620, REQ-SAG-0710 (amended).
- (Active, r-2) `equal_input` is the serialized r-1 `EqualInputRecord`
  (r-1 interim amendment of ADR-SAG-0008/0014/0019): r-2 pins its
  serialized shape in every `RunResponse` sample and adds no field. A
  dry run sends nothing, so every backend is `NotSent` and `equal` is
  false. `leak = any Failed`, the derivation of every other `Attestation`
  value and the session-save-failure behaviour (`session_saved = false`,
  one `warnings` entry, answers kept) are ADR-SAG-0025 (r-2b); until
  r-2b the samples that show them (`jev_failed_haiku_ok.json`,
  `session_save_failed.json`) are built with `RunResponse::test_builder`
  and are shape pins only. `RunConfig` has no `mode` setter until r-2b
  (ADR-SAG-0025).
- (Active, r-2) Dry-run reports `payload_bytes` and, per selected backend,
  `request_limit_bytes` and `over_limit`; it does not fail on an over-limit
  payload. Each backend owns its byte cap and enforces it in
  `validate_for_send`; the engine does not pre-cap. REQ-SAG-0314.
- (Active, r-2) `FileSessionStore` is stateless (`dir`, `ttl`, `clock`
  fn pointer) and never sends: a follow-up is
  `run::ask(&FileSessionStore, SessionId, follow_up, RunConfig) ->
  AskResponse`, which takes the backend from `RunConfig::registry` (else
  `BackendRegistry::from_env` for the session's backend), sends through the
  same send step as `run` (validate, judge, one log event), and does not
  rewrite the file. No lock: `save` writes a `create_new` tmp named with pid + random,
  fsyncs, renames, fsyncs the parent, removes the tmp on error; last writer
  wins; readers are protected by digest and id re-verification; tmp files
  older than 1 h are swept by `FileSessionStore::new`. The session is written once at the end of `run`, only
  when a backend with `supports_session_ask` returned `Ok`. REQ-SAG-0607.
- (Active, r-2) Repo-root discovery is the git toplevel for both consumers
  (ADR-SAG-0003 r-2 amendment).
- (Active, r-2) Landing details (shapes the bullets above imply, fixed at
  r-2): `check.toml` takes a top-level `backends = [...]` list parsed as a
  `BackendSet` (empty, repeated or unknown ids are `CONFIG_INVALID`); a
  caller request's `backends` repeat is `INPUT_INVALID`. Each backend is
  sent its own request: the shared `Arc<PayloadBundle>`, with LLM context
  only for a backend whose descriptor `requires_llm_context`. A live run
  that selects an `ask`-capable backend needs `RunConfig::sessions_dir`,
  checked (`CONFIG_INVALID`) before any send; session files are
  `<id>.json`. `primary_backend` is omitted when no backend returned `Ok`
  (a dry run), and a Jev `provider_meta` key that the body writes itself is
  not copied to the top level, so no key appears twice; the full
  `provider_meta` stays under `results[].outcome`. `Envelope::ask(&AskResponse)`
  carries the outcome's answer fields by the same primary rule, then
  `session_id`, `prompt_sha256` and `warnings` (omitted when empty;
  `AskResponse::with_warning` carries the CR-014 log-flush warning). The
  envelope is built infallibly from the typed body and written in the
  documented key order, then `readback`; the version (`1`) and operation
  names (`jev.run`, `jev.dry-run`, `jev.ask`) have one owner,
  `sc_jev_check::run`. `CheckSource::CheckFile` holds a `RepoRelativePath`
  built by `CheckSource::check_file(&RepoRoot, &Path)`, the one check-path
  normaliser. A `command` input may name its `producer` for
  `ProducerError` (default: the stem of `command[0]`). The public surface
  is the facade in `boundaries/sc-jev-check/jev-check.toml`; payload
  assembly and extraction are internal, so `prepare` is the only way to
  build a payload (ADR-SAG-0005). A failed session write is fail-fast in
  r-2; the unsaved-session shape is `test_builder`-only until ADR-SAG-0025.
  The production
  registry is `BackendRegistry::from_env(&RepoRoot, &BackendSet)` or
  `from_resolver(&BackendSet, Fn(BackendKeyId) -> Result<ResolvedApiKey,
  JevError>)`; `RunConfig::registry(Arc<BackendRegistry>)` is the
  no-secrets path (EXT-009).

## ADR-SAG-0025 — Multi-backend semantics and equal-input attestation

**Status:** Active. Amends ADR-SAG-0004, ADR-SAG-0008, ADR-SAG-0019,
ADR-SAG-0022. The engine bullets landed in r-2b and the REQ-SAG-0604
consumer-output bullet in r-3b.

- (Active, r-2b) The default mode is `Independent`; `ChainContext` is
  explicit (`RunConfig::mode`) and recorded on `EqualInputRecord.mode`.
  REQ-SAG-0605 (engine part).
- (Active, r-2b) The base request is validated on every backend before any
  send; each `ChainContext`-augmented request is re-validated before its own
  send, and a refusal marks that backend `Failed`. Each backend yields
  `BackendStatus::Ok | Failed(BackendFailure)` (the `JevError`; its
  `ScAgentErrorResponse` is derived when serialized); a failure
  does not discard other answers; `leak = any Failed`; if all fail, `run`
  returns `Err`. `judge` is called once per backend; only `BusyPolicy`
  retries; a session-save failure keeps the answers with
  `session_saved = false` and one `warnings` entry. REQ-SAG-0604 (engine part).
- (Active, r-3b) REQ-SAG-0604 consumer output: on a partial result
  (`RunResponse::is_partial()`) `sc-ai` prints the envelope with the
  successful answers on stdout, the first failure's
  `ScAgentErrorResponse` on stderr, and exits 2; `sc-jev-py` raises
  `JevError` with a `.response` attribute holding the envelope `data`.
  All-failed stays a plain error.
- (Active, r-2b) `equal` is derived from per-backend `Attestation`
  (`Matched`, `Mismatched`, `Unattested`, `Failed`, `NotSent`). Each backend
  reports `input_sha256`: the digest of the canonical blob recovered from its
  final serialized wire body. The engine trusts the reported digest. This
  re-encoding is attestation-only and never feeds the wire body, so it does
  not breach ADR-SAG-0005's "backends must not re-canonicalize". REQ-SAG-0606.
  From r-2b every backend that sends sets `input_sha256`; the ADR-SAG-0014
  r-1 interim clause 'null until r-2b backend hashing' no longer holds
  (`input_sha256` is null only for a backend that was not sent, failed,
  or reported no digest — `Unattested`).

## ADR-SAG-0026 — Shared HTTP crate and host crate

**Status:** Active (HTTP half landed in r-3; host half landed in r-3b)

HTTP half amends ADR-SAG-0006, 0007,
0017 and 0020; host half amends ADR-SAG-0010, 0011, 0013, 0016 and 0017.

### HTTP half

**Status:** Active (Phase A.1) — landed in r-3.

- (Active, r-3) `sc-agent-http` (published) owns `HttpRequest`
  (`body: Arc<[u8]>`) / `HttpResponse`, `Endpoint`, `SecretHeader`,
  `Transport`, `UreqTransport` (one `Agent` built in `new`, 8 MiB streaming
  response cap; private module `ureq_transport`), `BusyPolicy` /
  `send_with_busy_retry` (with `CancelToken`) and redaction of every error it
  returns, and is the single secret-read site (ADR-SAG-0020 r-3 amendment).
  It depends only on `sc-agent-types` (`default-features = false`,
  `features = ["secret"]`) and `ureq`; no `ureq` type is public. Its
  non-default `test-util` feature (test hooks for clock, sleeper and jitter;
  loopback-only `http` endpoint; test transports) is enabled only by
  dev-dependencies. Phase B provider agents reuse it. REQ-SAG-0402.
- (Active, r-3) ADR-SAG-0012 still holds: `sc-jev` owns every TypeSafe
  Jev API operation (endpoints, wire body, response parsing);
  `sc-agent-http` only carries bytes.

### Host half

**Status:** Active (Phase A.1) — landed in r-3b.

- (Active, r-3b) `sc-jev-host` owns the `sc-observability` adapter,
  `PathPolicy` (one data type; `cli()` and `python()` presets encode
  ADR-SAG-0021 per root, unchanged), `ComposeInputs`, `run_from` and
  `ask_from`. **Decided (owner, 2026-11-06):** it is published to crates.io
  as an internal-support crate, `=`-pinned from `sc-ai` and `sc-jev-py`, with
  no semver promise of its own, because `sc-ai` is published and cannot
  depend on an unpublished crate. Publish order and the published-crate set
  are ADR-SAG-0017's r-3b amendment. `ComposeInputs` has private fields,
  `new(cwd, policy)` and one setter per optional field. `run_from` and
  `ask_from` signatures name types owned by `sc-jev-check` and
  `sc-agent-types` (`CheckSource`, `RunResponse`, `AskResponse`, `JevError`;
  `ask_from` returns `AskResponse`, whose `outcome()` is the `JudgeOutcome`); compose does not re-export them, so a consumer (a future MCP
  server included) depends on those crates directly at the versions the
  compose release pins, and the `cargo tree -d` gate keeps one version of
  each in the `sc-ai` and `sc-jev-py` graphs.
- (Active, r-3b) Environment ownership: API-key environment and `.env`
  reading belongs to `sc-jev-check` (`BackendRegistry::from_env` via
  `sc-agent-secrets`); log and session path environment variables belong to
  `PathPolicy` in `sc-jev-host`. No other crate reads either set. The one
  other environment read in a library `src/` is `GIT_CEILING_DIRECTORIES` in
  `crates/sc-jev-check/src/run/mod.rs` (`enclosing_git_entry`, repo-root
  discovery): a git setting, neither a log, session nor API-key variable, and
  allowed by name in `test_env_reads_only_in_owners`.

### Amendment (Phase A.1, r-7)

**Status:** Active (Phase A.1, r-7).

- (Active, r-7) Host-half type note: `JudgeOutcome` is owned by
  `sc-decision-schema` (ADR-SAG-0030), not `sc-agent-types`. Compose does
  not re-export it.
- (Active, r-7) HTTP half: `send_with_busy_retry`, `send_and_parse` and
  `HttpClient::send_and_parse` take a required `SendTrace` parameter, so no
  backend can send without one. The trace belongs to one send: the engine
  creates it and passes it to `JudgeBackend::judge(req, &trace)`, never on
  the cloneable `JudgeRequest`; `sc-jev` and `sc-jev-backends` pass
  `trace.clone()` on. `HttpRequest`'s body is text (`Arc<str>`); the
  `Transport` trait (ADR-SAG-0006) is otherwise unchanged. Recording into
  the trace landed in r-6 (see the r-6 amendment below).

### Amendment (Phase A.1, r-6)

**Status:** Active (Phase A.1, r-6).

- (Active, r-6) HTTP half: `send_with_busy_retry` records the SHA-256 of
  the request body in the request's `SendTrace` immediately before the first
  send (ADR-SAG-0031). Hashing the bytes it carries is not Jev semantics, so
  ADR-SAG-0012 still holds; headers are not hashed.
- (Active, r-6) Host half: `ComposeInputs::project` and the resolved
  `repo` (`sc_jev_check::discover_main_repo_root`) reach the engine as log
  context. `PathPolicy` and ADR-SAG-0021 are unchanged.

## ADR-SAG-0027 — Error taxonomy and extensibility

**Status:** Active (Phase A.1, landed in r-1). Amends ADR-SAG-0001.

- (Active, r-1) `JevError`, `BackendId` and `BackendKeyId` are
  `#[non_exhaustive]`, and struct-like `JevError` variants are
  `#[non_exhaustive]` so downstream crates use constructors; every
  struct-like variant has a public constructor. Recovery
  categories are explicit variants (`CredentialStoreInvalid`,
  `RegistryInvalid`, `SessionStoreIo`, `Operational { area }`,
  `SessionDocumentInvalid`, `EncodeFailed`, `IoFailed`). Producer variants are
  neutral (`LanguageUnavailable { producer, language }`); producer stderr is
  re-raised as `ProducerError { producer: String, code: ErrorCode, message:
  String }`, whose `code()` is the carried id; every `producer` field is a
  `String`. No string-prefix parsing. REQ-SAG-0106.
- (Active, r-1) `code()` returns `ErrorCode`, a `#[non_exhaustive]` enum
  with `as_str()`; its ids are unchanged. `ErrorCode::ALL` is a hand-written
  `const` table (no `build.rs`) that a test compares with
  `errors.json`. Every variant maps to exactly one existing id; an id may
  have several variants (REQ-SAG-0101 amended). The variant → id table is in
  sprint r-1 and pinned by a golden test. A new stable code requires
  changing the README `errors.json` contract shared with Python. Revisit
  when Phase B needs an `INTERNAL` class.
- (Active, r-1) `retryable()` is true only for `NetworkError` kinds Busy,
  Timeout, Connect.
- (Active, r-1) `ScAgentErrorResponse::from_jev_error` (name unchanged)
  fills fields by one rule, in the fixed key order `error`, `message`,
  `schema_version`, `retryable`, `details`: `schema_version` is always `1`
  (versions this error shape only, independent of the stdout envelope
  `version`; bumped only by a breaking change of this shape); `retryable` is
  present, as `true`, only when `retryable()` is true; `details` is present
  only for `ProducerError`, as `{"producer": <name>}`, a closed object.
  Deserialization (for producer stderr) accepts `schema_version` absent or
  `1`, denies unknown fields at the top level and inside `details`, and
  treats anything else as not an error response; so adding any key, top
  level or in `details`, is a breaking change and bumps `schema_version`.
  Every stderr line gains `"schema_version":1`; NFR-SAG-0001 and
  REQ-SAG-0701 are amended and a golden stderr test pins the bytes.
- (Active, r-1) **Consumer rule.** Agents and CLI/MCP clients may read
  `schema_version` from every stderr line. A value other than `1` means
  "unknown error shape": fall back to the exit code and the raw line. A bump
  is a breaking change to be announced in `CHANGELOG.md` (nothing has been
  released yet, so that file has only an unreleased 0.1.0 entry).
- (Active, r-1) `Error::source()` is `None` for every redacted-cause
  variant (`CredentialStoreInvalid`, `NetworkError`, `EncodeFailed`,
  `IoFailed`, `SessionStoreIo`); no A.1 variant carries a non-redacted source.
- (Active, r-1) Serde spelling of backend ids matches `Display`
  (`openai_luna`, `openai`), with a one-release read alias for
  `open_ai_luna` / `open_ai` in config and caller-request input. Whether and
  when the alias is removed is the owner's decision, tracked by bead
  `agt-backend-serde-alias-removal`; nothing has been released, so no release
  has shipped the alias, and a removal would be announced in `CHANGELOG.md`.
- (Active, r-1) Module seams are cargo features of `sc-agent-types`:
  `judge`, `secret`, `obs` (default-on), `git` and `test-util`
  (non-default); `error`, `envelope`, `ids`, `schema`, `cancel` are always
  on. `secret` and `judge` are independent; `git` needs only the core.
  `sc-jev-check` (r-2) and `sc-treesitter`'s `cli` feature (r-3c) are the
  only enablers of `git`. `cargo hack --feature-powerset` checks every
  combination. `sc-treesitter` uses `default-features = false` (r-3c).
  The stable public surface is the crate-root re-exports plus the public
  modules `schema` (`schema::subjects`), `observability::v2`, `git` and
  `test_util`; `error`, `ids`, `judge`, `payload`, `secret`, `cancel`, and
  the `catalog` / `code` modules are private. The catalog
  (`BackendDescriptor`, `BACKENDS`, `BackendFamily`) and the error-code
  enums (`ErrorCode`, `NetworkErrorKind`, `OperationalArea`) sit in their
  own private modules because sc-boundary 0.8 reports a type cycle inside
  one module (SCB-CYCLE-001); callers reach them only through the crate
  root.
- (Active, r-1) **Adding a built-in backend** (checklist): (1) a minor
  `sc-agent-types` release with the `BackendId` variant, its `BACKENDS` row
  (facts, aliases, limit) and a `BackendKeyId` variant if needed; (2) the
  backend type with `with_default_transport`; (3) one row in
  `sc-jev-check::registry`'s factory table; (4) the catalog and
  factory-coverage tests pass unchanged. Dispatch on ids is a
  test-and-review convention (ADR-SAG-0009 r-1 amendment);
  `BackendRegistry::register` is for doubles and scripted backends of
  existing ids only. `BackendId` deserializes through `FromStr`, so the
  descriptor's `aliases()` is the one alias source.
- (Active, r-1) **Test seams** are non-default, additive `test-util` cargo
  features of the crate that owns the hook, enabled only by
  dev-dependencies of other crates, never by a normal dependency edge. A
  crate's own integration targets that need the hook declare
  `[[test]] required-features = ["test-util"]` and their validation lines
  pass `--features test-util`: the r-1 D0 spike showed sc-boundary flags a
  path-only self dev-dependency (`SCB-MANIFEST-002`) and a versioned one
  cannot be packaged (`boundaries/README.md` "Dev-dependency and self
  edges"); `workspace-tests --test test_util_off` and a `cargo tree`
  gate prove it. The one exception is the `sc-jev-py` pytest wheel, built
  with `maturin --features extension-module,test-util` (ADR-SAG-0029 (b));
  the publish wheel has no `_testing` submodule. No acceptance test in
  `tests/` relies on `cfg(test)`.

### Amendment (Phase A.1, r-7)

**Status:** Active (Phase A.1, r-7).

- (Active, r-7) Layering: `sc-agent-types` holds the types every agent kind
  shares (ids, errors, the backend and model catalogs, secrets, git, cancel,
  the observability port, `schema::subjects`); `sc-decision-schema` sits
  above it and holds the decision types (ADR-SAG-0030). `QuestionId` and
  `ChoiceId` move there. The `judge` and `test-util` features of
  `sc-agent-types` are deleted (`default = ["secret", "obs"]`); the
  module-seam sentence's `judge` and `test-util` and the stable-surface
  entries `judge`, `payload` and `test_util` are superseded.
- (Active, r-7) Adding a built-in backend (checklist) also sets its
  descriptor's `default_model_alias` and, for a new model, adds the
  `ModelId` variant and its `MODELS` row; the factory-table row of step 3
  takes `&ModelAliases`.
- (Active, r-7) The core (always-on) surface gains `SendTrace` and
  `ProjectLabel` (ADR-SAG-0031).
- (Active, r-7) **Git deadline contract.** Every git subprocess started by
  `sc_agent_types::git` has a fixed wall-clock deadline (`GIT_DEADLINE`,
  60 s, crate-private, not configurable). At the deadline the child is killed
  and reaped and the call returns the new variant
  `JevError::GitTimeout { seconds }`. It maps to the existing
  `EXTRACTION_TIMEOUT` id (an id may have several variants; no new stable
  code, `errors.json` is unchanged), with `Display` "git did not finish
  within Ns" and a hint about a stalled git process, a lock file or a slow
  filesystem. `ExtractionTimeout` keeps the `command` extractor's
  `timeout_seconds` meaning. `JevError` is `#[non_exhaustive]`, so the
  variant is additive.

### Amendment (Phase A.1, r-6)

**Status:** Active (Phase A.1, r-6).

- (Active, r-6) Core dependencies change from `uuid` and `sha2` to `ulid`
  and `sc-sha` (caret, ADR-SAG-0023). `observability::v2::LogEvent`'s
  constructor and accessors change (ADR-SAG-0016 r-6 amendment), a
  stable-surface change made before any publish (ADR-SAG-0031).

## ADR-SAG-0028 — workspace-tests crate

**Status:** Active (Phase A.1) — landed in r-3b.

- (Active, r-3b) **Purpose.** `crates/workspace-tests` (`publish = false`)
  holds proofs that span crates and must not live inside a published crate:
  the Rust/Python cross-implementation digest (`jev_cross_impl`), the real
  `sc-treesitter` → `check.toml` producer run, CLI/Python output parity,
  git-read parity, source scans (allow attributes, `disallowed_methods`
  expect sites for the secret read and process spawns, lint-mechanism
  presence, lints inheritance, environment-read owners) and the
  `test-util`-off proof (graph query and a release-built `sc-ai`). Its
  `src/lib.rs` is empty; everything is under `tests/`; its documentation is
  one `README.md` paragraph per target (no `docs/workspace-tests/`).
  REQ-SAG-0109.
- (Active, r-3b) **Dev-only edges.** It has no normal dependencies. Its
  dev-dependencies are exactly `sc-agent-types` (`features = ["git"]`),
  `sc-jev-check`, `sc-jev-host`, `serde_json`, `tempfile`.
  `sc-boundary` counts dev-only workspace edges (r-1's D0 spike, outcome 4,
  `boundaries/README.md` "Dev-dependency and self edges"): the
  `workspace-tests` manifest lists `sc-agent-types`, `sc-jev-check` and
  `sc-jev-host` in its dependency allowlist, and each of those three lists
  `workspace-tests` in `allowed_dependents`. There is no
  `cargo-tree-gates.sh` dev-set row. The third-party dev-dependencies
  (`serde_json`, `tempfile`) are not checked by `sc-boundary`.
- (Active, r-3b) **Built-binary contract.** `sc-ai` and `sc-treesitter`
  are never linked: a test builds them with
  `cargo build --release -p <pkg> --features cli --target-dir <isolated> --message-format=json`
  (via `env!("CARGO")`) and runs the executable named in the
  `compiler-artifact` message (helper `tests/support/bin_path.rs`). The
  isolated target directory never holds the `cargo test` build of `sc-ai`,
  which is built with `test-util` on (`--features test-util`, or Cargo
  feature unification in a workspace test run; there is no self
  dev-dependency; `boundaries/README.md`, section Dev-dependency and self
  edges). Stable Cargo only: no
  `CARGO_BIN_EXE_*` across packages, no artifact dependencies.
- (Active, r-3b) **Forbidden edges.** `pyo3`, `tree-sitter`,
  `sc-treesitter`, `sc-jev-py`, `sc-ai`, `sc-agent-http`, `sc-jev`,
  `sc-jev-backends` (manifest `forbidden_edges`; `cargo tree -p
  workspace-tests -e normal,dev` shows neither `pyo3` nor `tree-sitter`). A proof
  that needs one of them goes through a built binary or into that crate's own
  tests.

### Amendment (Phase A.1, r-7)

**Status:** Active (Phase A.1, r-7).

- (Active, r-7) **Dev-only edges.** The "exactly" dev-dependency list of the
  r-3b bullet is replaced: `workspace-tests` dev-dependencies are
  `sc-agent-types` (`features = ["git"]`), `sc-decision-schema`
  (`features = ["test-util"]`), `sc-jev-check`, `sc-jev-host`, `serde_json`,
  `tempfile`. The new `sc-decision-schema` dev edge is counted by
  `sc-boundary` like the others: the `workspace-tests` manifest lists
  `sc-decision-schema` in its dependency allowlist and the
  `sc-decision-schema` manifest lists `workspace-tests` in
  `allowed_dependents`. Its source scans, `jev_cross_impl` and the other
  targets import the moved decision types from `sc_decision_schema`.

## ADR-SAG-0030 — Decision schema crate and model catalog

**Status:** Active (Phase A.1, r-7); the "Jev API structs" bullet and the
ADR-SAG-0012 subsection landed in r-5. Amends
ADR-SAG-0005, ADR-SAG-0009, ADR-SAG-0016, ADR-SAG-0017, ADR-SAG-0019,
ADR-SAG-0022, ADR-SAG-0026, ADR-SAG-0027 and ADR-SAG-0028 (an r-7
subsection in each) and ADR-SAG-0012 (an r-5 subsection).
Owner decisions 2026-11-07.

- (Active, r-7) **Crate.** `sc-decision-schema` (published) is
  **data-only**: serde types, schema versions and DDL, plus the
  `JudgeBackend` trait whose signatures name them (a trait cannot sit below
  the types it names). Its dependencies are `sc-agent-types`
  (`default-features = false`), `serde` and `serde_json`; it has no HTTP,
  git, process, file or logging dependency and no I/O. Its non-default
  `test-util` feature holds `MockJudgeBackend`.
- (Active, r-7) **Layering and one home.** `sc-agent-types` is below
  `sc-decision-schema` and holds the types every agent kind shares: ids,
  errors, the backend and model catalogs, secrets, git, cancel, the
  observability port and `schema::subjects`. The decision types move (not
  copy) to `sc-decision-schema`: `QuestionId`, `ChoiceId`, `Question`,
  `ChoiceAnswer`, `CanonicalJudgePayload`, `PayloadBundle`,
  `canonical_json_bytes`, `QuestionAnswer`, `JudgeOutcome`, `JudgeRequest`,
  `LlmWireContext`, `JudgeBackend`, `BackendCapabilities`, the attestation
  types and `MultiBackendMode`. `sc-agent-types` re-exports none of them.
- (Active, r-7) **Versioning.** `DecisionSchemaId { family, version }`,
  written `"canonical.v1"`. Readers accept every known id and refuse any
  other (`INPUT_INVALID` for input documents, `RESPONSE_INVALID` for backend
  output). Additive optional fields keep the version; a removed, renamed or
  retyped field, or a changed meaning, bumps it. **The version lives outside
  the hashed bytes**: it is never a key of `CanonicalJudgePayload` or part of
  `PayloadBundle::bytes()`, so `prompt_sha256` and the Rust/Python
  cross-implementation fixture are byte-identical. It is carried by the
  sc-agent-owned wrappers (the session-log row, ADR-SAG-0031, and the session
  document, ADR-SAG-0008 r-6) and is not sent to a vendor whose schema does
  not define it.
- (Active, r-5) **Jev API structs.** `sc_decision_schema::jev::v1` has one
  serde request and one response type per Jev API call known today (check:
  `CheckRequest`, `CheckResponse`). `sc-jev` builds the body with
  `CheckRequest` and parses replies with `CheckResponse`; the byte-splice is
  deleted. They encode the documented Jev API (D9, owner decision (c)): the
  flat body `{model, state, questions}`, every question type (`choice`,
  `score`, `noul`), every `state` form (string, object, array) and the
  `{model, answers, usage}` reply.
- (Active, r-7) **LLM I/O on the same schema.** Haiku/Luna input is the
  canonical payload bytes embedded unchanged (ADR-SAG-0019); their output is
  `DecisionAnswers`, the same answers type as `jev::v1::CheckResponse`.
- (Active, r-7) **Model-family variants.** `schema_for(ModelFamily)` and
  one module of plain functions per `SchemaFamily` (`variants::canonical`:
  `input`, the identity on the payload, and `answers`); no trait until a
  second variant exists. Dispatch is static and keyed by `SchemaFamily`:
  `llm::v1::parse_output` holds the one `match`. A future local decision
  model with a slightly different schema adds a `SchemaFamily` variant, a
  module and one arm in this crate; callers do not change.
- (Active, r-7; binding as a reservation, owner 2026-11-07) **Per-API modules and crate naming.** `sc-decision-schema` keeps one
  versioned module per decision API: `jev::v1` lands in r-5 (see "Jev API
  structs"); `decider::` and `laya::` are reserved for the first post-MVP
  crates (decider first, then laya) and `kev::` later. Its shared types stay
  API-agnostic, so those crates plug in without changing the jev crates. Crate
  families follow "Crate naming convention": `sc-jev-*`, `sc-decider*`,
  `sc-laya*`, `sc-kev*` (per API) and `sc-decision-*` (API-agnostic). No crate
  is renamed and no module is built before its sprint.
- (Active, r-7) **Model catalog** (`sc-agent-types`). `ModelId` is an open
  enum: known variants that track recent releases plus `Other(UnknownModel)`, so a
  model released after the build still parses, is sent verbatim and is
  logged. `Display`, `FromStr` and serde use the model string. The unknown
  form (`UnknownModel`) has no public constructor, so `Other` never holds a
  known model string and one model always logs one way. `MODELS` is
  the in-crate table; `ModelAliases::builtin().latest("jev" | "haiku" |
  "luna" | …)` returns the newest model for an alias, and
  `with_override` (from `check.toml` `[models]`) replaces it. Backends hold
  their configured `ModelId`; the Jev model pin moves from `sc-jev`'s
  `jev.toml` to `MODELS`, so each model string has one source. `BackendId` = how we call; `ModelId` = which
  model; the log's `model_id` is the `ModelId` string.
- (Recorded) **Scope context (recorded, not built).** sc-agent will launch
  decision models, Claude and Codex through their SDKs/APIs directly, and
  CLI agents (claude-code, codex-cli, cursor-cli, …) through a wrapper; more
  SDK and CLI agent kinds land in `sc-agent-types` over time. The catalog
  and the layering are model- and agent-agnostic so this is not precluded.
  REQ-SAG-0900/0901 define the API-direct agents; CLI wrappers are
  additional future agent kinds. No wrapper is built in A.1.

## ADR-SAG-0031 — Session log storage schema

**Status:** Active (r-7 storage schema, r-6 wiring). The wiring subsection
amends ADR-SAG-0008, ADR-SAG-0013, ADR-SAG-0016, ADR-SAG-0026 and
ADR-SAG-0027 (an r-6 subsection in each). Owner decisions 2026-11-07.

### Storage schema

**Status:** Active (Phase A.1, r-7).

- (Active, r-7) The session-log storage contract is a **SQLite schema**,
  authoritative: `sessions` (by session ULID: `session_created_at`),
  `questions` (by `prompt_sha256`: `question_ids`, `first_seen_at`) and
  `answers` (one per send, by ULID `answer_id`, with `decision_schema`,
  `repo`, `project` and the rest; indexed by `(prompt_sha256, model_id)`,
  `request_sha256` and `(repo, project)`). Values that can differ between
  two sends of one session or prompt live on `answers`, so the loader's
  `INSERT OR IGNORE` on the other two tables loses nothing. The DDL is
  `crates/sc-decision-schema/src/session_log/schema.sql`
  (`SESSION_LOG_DDL`; `docs/sc-decision-schema/session-log-schema.md` points
  to it); `schema_version` starts
  at 1. Adding, renaming or retyping a column, or changing its nullability,
  bumps `schema_version` with a migration note.
- (Active, r-7) **Projection.** The MVP store is JSONL; the projection of
  one send is the `fields{}` object of one sc-observability v2 log line
  (ADR-SAG-0013). It has exactly the schema's column keys: nullable columns
  present as `null`, `question_ids` a JSON array, `answers_json` a JSON
  object. The outcome key is `send_outcome` (`ok` | `failed`), so it never
  collides with the envelope's `outcome`. `ts` is the row's time; the
  envelope `timestamp` is write time. A loader reads `fields{}` into the
  tables (`INSERT OR IGNORE` for `sessions` and `questions`), so the
  projection is lossless. This supersedes the r-6 draft's "flat top-level,
  no nesting" wording, which never landed.
- (Active, r-7) **Nullability.** `request_sha256` is NULL when the send
  failed before a body was built; `model_id` is never NULL; `project` is NULL
  when absent; `error_code` is set iff `send_outcome = failed`;
  `answers_json` is set iff `send_outcome = ok`.
- (Active, r-7) **Content.** No secret, payload text, prompt text or raw
  response body in any column (NFR-SAG-0002); `answers_json` holds only
  typed answers. `repo` is the only path column. `project` is caller text,
  stored as given and not scrubbed, and validated by `ProjectLabel` (the row
  type holds a `ProjectLabel`, so the loader and the CLI accept the same
  text).

### Wiring

**Status:** Active (Phase A.1, r-6).

- (Active, r-6) Every `judge` and `ask` send, `Ok` or `Failed`, emits
  exactly one row; a backend refused before send (`NotSent`) emits none; a
  session-save failure emits none of its own. The row goes through
  `SharedSessionLog` as `LogEvent` fields and the `sc-jev-host` v2
  adapter writes it as `fields{}` verbatim (no bespoke writer).
- (Active, r-6) Session ids are ULIDs (`ulid`, core dependency of
  `sc-agent-types`); the session document is version 3 (ADR-SAG-0008 r-6).
- (Active, r-6) `request_sha256` is the SHA-256 of exactly the bytes
  sent, the future response-cache key. `sc-agent-http` records it at the
  send site (`send_with_busy_retry`, used by every backend) into the
  per-send `SendTrace` the engine passes to `judge` and the backend passes
  as the exchange's required `trace` argument (seam from r-7, ADR-SAG-0026
  r-7); one value per send, retries included; NULL when no body was
  built. `prompt_sha256` stays the canonical-payload digest
  (ADR-SAG-0002/0025).
- (Active, r-6) The row's `model_id` is `JudgeBackend::model_id()`
  (ADR-SAG-0009 r-7 amendment), never NULL.
- (Active, r-6) `repo` is `sc_jev_check::discover_main_repo_root`, started from the explicit repo root when one is set, else from the working directory (a dry run skips git): a
  linked worktree resolves to its main repository; a separate or bare git
  dir falls back to the current toplevel; outside git it is the calling
  folder, absolutized. It uses `sc_agent_types::git::discover_common_dir`,
  inside the existing `git.rs` spawn site, so the `git` feature enablers
  (ADR-SAG-0027) and the ADR-SAG-0028 expect-site list are unchanged.
  ADR-SAG-0021 paths are unchanged: sessions and logs stay under the
  worktree's own root. `project` is `ProjectLabel` (≤ 256 bytes, no control
  characters), from `sc-ai --project` or Python `project=`.
- (Active, r-6) SHA-256 is computed with `sc-sha`
  (`calculate_hash(HashInput::TextFileBytes { .. })`, strict UTF-8, CRLF and
  bare CR normalized to LF). Request bodies and canonical payloads are
  compact JSON with no raw CR, so their digest equals SHA-256 of the bytes
  sent; other text (a tree-sitter query source) hashes in its LF-normalized
  form; no
  workspace manifest lists `sha2` or `uuid`.
- (Active, r-6) Log-write failure policy: a rejected or failed log row
  never fails the run; `SessionLog::emit` is best effort and a flush failure
  is a `data.warnings` entry (plan.md owner decisions 2026-11-07, item 9).
- (Active, r-6) On the error path (`run` or `ask` returned an error) the log is
  still flushed; a flush failure there is appended once to the error detail
  and the error code is unchanged, because no response exists to carry a
  `data.warnings` entry.
