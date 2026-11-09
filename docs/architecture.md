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
sc-jev-py (PyO3) ───┴── sc-jev-host (sc-observability =1.5.0 adapters for the session and general logs, path policy, run/ask)
                          └── sc-jev-check (facade: check.toml, extractors, run_check, sessions)
                                ├── sc-jev ────────────┐
                                ├── sc-jev-backends ───┴── sc-agent-http (ureq transport)
                                ├── sc-agent-secrets
                                ├── sc-decision-schema
                                └── sc-agent-types (observability::v2 ports: SessionLog, GeneralLog; no sc-observability* dep)

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
only direct dependent of full `sc-observability`** (`crates/sc-jev-host/Cargo.toml`;
ADR-SAG-0013 and ADR-SAG-0010 r-3b amendments). `sc-ai` and `sc-jev-py` are
thin composition roots over `sc-jev-host` and have no direct `sc-observability`
dependency. **`sc-jev-py` alone also links the `sc-observability` Python
runtime** (`sc-observability-py` as an rlib, with `sc-observability-binding-runtime`
and `sc-observability-dto`) so Rust writes the general log with no GIL and
Python reaches the same logger (ADR-SAG-0013 amendment 2026-11-09).

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
                                                       ├── sc-jev-backends ─┴── sc-agent-http (ureq; sc-agent-types features = [secret, obs])
                                                       ├── sc-agent-secrets
                                                       └── sc-agent-types (default secret/obs; git enabled by sc-jev-check; no observability dep)
sc-treesitter (bin; feature cli: clap + mimalloc + sc-agent-types/git) → sc-agent-types (default-features = false)
crates/workspace-tests (publish = false; dev-deps only; drives built binaries; ADR-SAG-0028)
```

The Phase A graph above is the starting point; r-3b moved the observability
adapter, the path policy and the `run`/`ask` wiring out of `sc-ai` and
`sc-jev-py` into `sc-jev-host` (ADR-SAG-0026 host half). `sc-jev-host` is the
only direct dependent of `sc-observability`. `workspace-tests` links no
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
| ADR-SAG-0007 | API-direct LLM backends | Active (r-3; retry numbers phase-end) | Amended by ADR-SAG-0026 (Active, r-3) |
| ADR-SAG-0008 | Sessions carry prior answers to later backends | Active | A.1 subsection (Active, r-1); amended by ADR-SAG-0025 (Active, r-2b); ADR-SAG-0031 (Active, r-6) |
| ADR-SAG-0009 | Local backend registers without caller changes | Active | Amended by ADR-SAG-0022; ADR-SAG-0027 (Active, r-1); ADR-SAG-0024 (Active, r-2); ADR-SAG-0030 (Active, r-7) |
| ADR-SAG-0010 | CLI thin adapter | Active | Amended by ADR-SAG-0026 (Active, r-3b) |
| ADR-SAG-0011 | Python via sc-jev-py wheel | Active | Amended by ADR-SAG-0026 (Active, r-3b) |
| ADR-SAG-0012 | sc-jev owns Jev API | Active | Amended by ADR-SAG-0030 (Active, r-5) |
| ADR-SAG-0013 | Session and general logs via sc-observability v2 | Active | Amended by ADR-SAG-0021; ADR-SAG-0026 (Active, r-3b); ADR-SAG-0031 (Active, r-6); general log (Active, 2026-11-09, agt-9eh) |
| ADR-SAG-0014 | CLI success body | Active | A.1 subsection (Active, r-1); amended by ADR-SAG-0024 (Active, r-2) |
| ADR-SAG-0015 | Subjects artifact outside the judge | Active | — |
| ADR-SAG-0016 | v2 shim in sc-agent-types; adapter in sc-jev-host | Active | Amended by ADR-SAG-0023 (Active, r-4); ADR-SAG-0027 (Active, r-1); ADR-SAG-0026 (Active, r-3b); ADR-SAG-0030 (Active, r-7); ADR-SAG-0031 (Active, r-6) |
| ADR-SAG-0017 | First release surfaces | Active | Amended by ADR-SAG-0023 (Active, r-4); ADR-SAG-0026 (Active, r-3), (Active, r-3b); ADR-SAG-0030 (Active, r-7) |
| ADR-SAG-0018 | sc-publish kit owns release surface | Active | — |
| ADR-SAG-0019 | LLM wire envelope vs canonical payload | Active | A.1 subsection (Active, r-1); amended by ADR-SAG-0024 (Active, r-2); ADR-SAG-0025 (Active, r-2b); ADR-SAG-0030 (Active, r-7) |
| ADR-SAG-0020 | Secret injection and redaction | Active | Amends ADR-SAG-0003; amended by ADR-SAG-0029; A.1 subsection (Active, r-1); ADR-SAG-0026 (Active, r-3); A.1 subsection (Active, r-3b); agt-2a3 provider error detail (Active) |
| ADR-SAG-0021 | Log and session path precedence | Active | Amends ADR-SAG-0013 |
| ADR-SAG-0022 | Scripted backends are a test-util feature | Active | Amends ADR-SAG-0009; amended by ADR-SAG-0029; A.1 subsection (Active, r-1); ADR-SAG-0024 (Active, r-2); ADR-SAG-0025 (Active, r-2b); ADR-SAG-0030 (Active, r-7); agt-2a3 loopback test seam (Active) |
| ADR-SAG-0023 | Dependency pins, allocator, released binaries | Active (r-4; host pin r-3b; Windows CRT phase-end) | Amends ADR-SAG-0016, ADR-SAG-0017 |
| ADR-SAG-0024 | Engine entry point and response contract | Active (r-2) | Amends ADR-SAG-0003, ADR-SAG-0005, ADR-SAG-0009, ADR-SAG-0014, ADR-SAG-0019, ADR-SAG-0022; supersedes the ADR-SAG-0014 A.1 r-1 interim subsection |
| ADR-SAG-0025 | Multi-backend semantics and equal-input attestation | Active (r-2b) | Amends ADR-SAG-0004, ADR-SAG-0008, ADR-SAG-0019, ADR-SAG-0022; amended by agt-2a3 (all-failed runs keep every error) (Active) |
| ADR-SAG-0026 | Shared HTTP crate and host crate | Active (HTTP half, r-3, amended 2026-11-09 agt-9eh: `secret` and `obs`; host half, r-3b) | Amends ADR-SAG-0006, ADR-SAG-0007, ADR-SAG-0020, ADR-SAG-0017 (HTTP half); ADR-SAG-0010, ADR-SAG-0011, ADR-SAG-0013, ADR-SAG-0016, ADR-SAG-0017 (host half); amended by ADR-SAG-0030 (Active, r-7); ADR-SAG-0031 (Active, r-6); agt-2a3 provider failure classification (Active); ADR-SAG-0032 (Active, agt-1aq: `sc-jev-host` gains `stream` and a direct `serde` edge) |
| ADR-SAG-0027 | Error taxonomy and extensibility | Active (r-1) | Amends ADR-SAG-0001, ADR-SAG-0009, ADR-SAG-0016; amended by ADR-SAG-0030 (Active, r-7); ADR-SAG-0031 (Active, r-6); agt-2a3 error contract (Active) |
| ADR-SAG-0028 | workspace-tests crate | Active (r-3b) | Amended by ADR-SAG-0030 (Active, r-7) |
| ADR-SAG-0029 | Phase A hardening amendments | Active (a-10) | Amends ADR-SAG-0020 and ADR-SAG-0022; A.1 supersedes named sentences of (a) (Active, r-1), (Active, r-3) and of (b) (Active, r-1), (Active, r-2) |
| ADR-SAG-0030 | Decision schema crate and model catalog | Active (r-7); jev::v1 (Active, r-5) | Amends ADR-SAG-0005, ADR-SAG-0009, ADR-SAG-0012, ADR-SAG-0016, ADR-SAG-0017, ADR-SAG-0019, ADR-SAG-0022, ADR-SAG-0026, ADR-SAG-0027, ADR-SAG-0028 |
| ADR-SAG-0031 | Session log storage schema | Active (r-7 storage schema, r-6 wiring) | Amends ADR-SAG-0008, ADR-SAG-0013, ADR-SAG-0016, ADR-SAG-0026, ADR-SAG-0027 (wiring) |
| ADR-SAG-0032 | Streaming decision calls | Active (agt-1aq) | Amends ADR-SAG-0014 (one envelope per run), ADR-SAG-0026 (host crate gains `stream` and a direct `serde` edge); uses ADR-SAG-0013, ADR-SAG-0024, ADR-SAG-0027 |

## ADR-SAG-0001 — Error enum is CLI authority

**Status:** Active

All user-visible error ids come from `JevError` in `sc-agent-types`.
CLI and PyO3 map variants to JSON / exceptions; no ad-hoc strings.

## ADR-SAG-0002 — Canonical judge payload bytes

**Status:** Active

The **canonical judge payload** is sorted-key compact JSON (UTF-8): the
questions + state the backends must judge. SHA-256 of **those** bytes is
`payload_sha256` for equal-input and the cross-language fixture (sprint a-1).

### Amendment (Phase A.1, r-5)

**Status:** Active (Phase A.1) — landed in r-5.

- (Active, r-5) A question has two new fields, omitted from the bytes
  when they hold their default: `kind` (`QuestionKind::{Choice, Score,
  Noul}`, default `Choice`, not written for a choice question) and
  `descriptions` (one per choice; absent means each description is the
  choice label). `choice` has 2 to 255 choices, `score` 2 to 10 in level
  order, `noul` none; any other shape is `INPUT_INVALID`, checked by one
  validating constructor that deserialization also runs. A payload without these fields
  serializes byte-identically to the Phase A form, so `payload_sha256` and
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
  payload (no byte-splice); `payload_sha256` stays on canonical bytes.
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

### Amendment (agt-2a3, loopback test seam)

**Status:** Active (bead `agt-2a3`, before the first release).

- (Active, agt-2a3) The hidden `sc-ai jev run --loopback-port <PORT>` flag is
  a second test seam next to `--scripted-answers`. It builds the **real HTTP**
  backends (`BackendRegistry::loopback_from_env`) aimed at
  `http://127.0.0.1:<PORT>`, one send each, so a binary test can drive a
  mapped 401, a busy reply and a redacted key through the whole path.
- (Active, agt-2a3) **Limits.** Only under the `test-util` cargo feature;
  keys still come from the environment and `.env` like a real run (nothing is
  faked); the only endpoint it can build is `Endpoint::loopback_http`, which
  is `127.0.0.1` over plain HTTP and cannot name another host. The published
  binary has neither the flag nor the code:
  `workspace-tests --test test_util_off_binary`
  (`test_release_binary_rejects_loopback_port`) builds the release binary and
  asserts the flag is refused, and the dev-only graph check
  (`test_util_off`) keeps `test-util` out of the normal graph.
- (Active, agt-2a3) **Gated items** (all `#[cfg(feature = "test-util")]`):
  `sc_jev::JevClient::loopback` (and the crate-private
  `JevClientConfig::with_loopback_port`),
  `sc_jev_backends::{AnthropicHaikuBackend, OpenAiLunaBackend}::loopback`,
  `sc_jev_check::registry::BackendRegistry::loopback_from_env`, and the
  `--loopback-port` argument in the `sc-ai` binary. The `test-util` features of
  `sc-jev`, `sc-jev-backends` and `sc-jev-check` enable `sc-agent-http`'s
  `test-util` (`Endpoint::loopback_http`, `UreqTransport::loopback_for_tests`).

## ADR-SAG-0019 — LLM wire envelope vs canonical payload

**Status:** Active

**Jev (TypeSafe)** sends the canonical payload in the API shape the evaluator
expects (including model selection on the wire).

**Haiku / Luna** send API-direct HTTP that is **not** byte-identical to Jev’s
body. They **must** embed the **same canonical payload bytes** in the user/judge
message (per README parity). A **system prompt** (evaluator instructions, JSON
schema hints) is allowed and **may change** over time without changing
`payload_sha256`, as long as the embedded canonical blob is unchanged.

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
  step. A miss stays `KEY_MISSING` (ADR-SAG-0001). *Amended (agt-2a3): an
  unreadable store is `IO_FAILED`; `CONFIG_INVALID` is kept for a credential
  whose content is unusable.*
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

### Amendment (agt-2a3, provider error detail)

**Status:** Active (bead `agt-2a3`, before the first release).

- (Active, agt-2a3) **Provider error text is allowed in errors.** The rule
  that a failing reply's body is never echoed (status only) is replaced.
  An error built from a non-200 reply from Jev, Haiku or Luna carries the
  provider's status, error type, error code, message, blamed parameter and
  request id, falling back to the raw body text when the body is not a
  recognised error JSON. Prompt text and model output stay out: they can
  appear only if the provider's own error text repeats them.
- (Active, agt-2a3) **Scrubbing.** Provider text is made single-line
  (control characters become spaces) and bounded to **1024 bytes per field**,
  cut at a character boundary and ending in `...`
  (`sc_agent_types::bound_provider_text`, `PROVIDER_TEXT_MAX_BYTES`), when the
  error is built; the request's key is removed from the provider text
  *before* the cut (`ResolvedApiKey::redact_text`), so a key straddling the
  1024-byte boundary leaves no fragment (ERR-QA-3). The error then leaves
  through `redact`, once, in
  `send_and_parse` (r-3 amendment), so every occurrence of the request's key,
  in any provider field or in the raw fallback, becomes `[redacted]`.
  `ResolvedApiKey::redact_error` covers the provider fields and `details`
  strings (`JevError::map_detail` reaches them; a test proves it). The raw
  fallback scrubs the whole body, then bounds it. Session
  logs still store only the `error_code`, never the text (NFR-SAG-0002,
  ADR-SAG-0031).
- (Active, agt-2a3) **Keyring read deadline.** A platform keyring read
  (`PlatformKeyring::password`) runs on its own thread with a fixed 15 s
  deadline (`KEYRING_READ_TIMEOUT`, not configurable). A keystore that waits
  for a prompt it cannot show (a locked macOS Keychain in a headless run)
  gives `IO_FAILED` with `io_kind` `other` (not retryable: the blocked
  read does not clear until the user unlocks the keystore, so a retry
  blocks again) instead of hanging. A read that has been stuck past the
  deadline makes every later read in the process return the same error at
  once, so several backends cost one wait and at most one thread is
  stranded. The blocked thread is detached and left to finish; it holds no
  state and reads only the keystore, so abandoning it is safe and the
  process can still exit. A reader thread that cannot start or that panics
  is a non-retryable `IO_FAILED` (`other`). The deadline applies to the
  keyring only; the environment and `.env` sources are unchanged. The Python
  entry points resolve keys inside `py.detach`, so the GIL is released
  during the wait.

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

### Amendment (agt-1aq, stream cancel)

- (Active) **The judge request's `CancelToken` governs the send.** A cancel
  set on `JudgeRequest::cancel()` (the `RunConfig::cancel` token, and a
  stream's own) ends a busy-retry wait within the 100 ms wait slice and stops
  any further send, the same as the client's own token; either one cancelling
  is enough. `HttpClient::send_and_parse_for` takes the request's token and
  `JevClient` and the LLM backends pass it. A request already on the wire is
  not interrupted. Proof: `sc-ai` `stream_cancel_ends_a_real_busy_wait_and_sends_nothing_more`
  (a real `Retry-After: 30` wait over loopback HTTP). `UreqTransport` keeps up to
  64 idle connections per host (one constant), so a stream at its maximum
  concurrency reuses connections.

### Amendment (Phase A.1, r-3)

**Status:** Active (Phase A.1) — landed in r-3. Numbers decided by the
owner (2026-11-06). The send count and default waits below are what r-3
shipped; the phase-end amendment below supersedes them with 5 sends and waits
of 1, 2, 4, 8 s.

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

### Amendment (Phase A.1, phase-end, 2026-11-08)

**Status:** Active (Phase A.1). Owner decision (2026-11-08).

Amends the r-3 busy-retry numbers above; the rest of r-3 stands.

- `BusyPolicy` makes at most 5 sends in total, and no policy may exceed 5.
  With the default 5 sends, four waits occur when Retry-After is absent or
  malformed, of 1, 2, 4 and 8 s (15 s total before jitter). The 30 s cap applies only to an
  honoured Retry-After. The 90 s budget stays as a ceiling for slow or
  Retry-After-heavy sequences.
- The shared layer bounds the transport deadlines for every backend: request
  1 ms–120 s, connect 1 ms–10 s.

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
  | Scoop | shim | shim |
  | WinGet | alias | in package, no alias |

  Scoop: the upstream change landed in sc-publish as
  https://github.com/randlee/sc-publish/pull/176 (optional
  `[channels.scoop].binaries`; the owner directed a PR instead of an issue).
  sc-agent has re-pinned the kit (sc-publish `34dab3f`) and sets
  `binaries = ["sc-ai", "sc-treesitter"]`, so Scoop shims both. Bead
  `agt-scoop-multi-shim` is resolved.

  WinGet: command aliases are not an upstream issue. The first manifest is
  submitted by hand and must list a `PortableCommandAlias` for each of
  `sc-ai` and `sc-treesitter` (bead `agt-sft`). The kit pins Komac 2.16.0 for
  later updates (the related upstream PR randlee/sc-publish#182 was open when
  this was written). The Windows invocation path for
  `sc-treesitter` is in `docs/development/sc-publish.md`. REQ-SAG-0133.

### Amendment (Phase A.1, r-3)

**Status:** Active (Phase A.1) — landed in r-3.

- (Active, r-3) Published library crates gain `sc-agent-http`. crates.io
  publish order: `sc-agent-types`, `sc-agent-secrets`, `sc-agent-http`,
  `sc-jev`, `sc-jev-backends`, `sc-jev-check`, `sc-treesitter`, `sc-ai`.
  **Superseded** for the order by the r-3b amendment (`sc-jev-host` joins
  directly after `sc-jev-check`) and the r-7 amendment (`sc-decision-schema`
  joins at order 2); the authoritative list is the r-7 amendment and
  `release/sc-publish-consumer-input.json`. Only the addition of
  `sc-agent-http` stands.

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

## ADR-SAG-0013 — Session and general logs via sc-observability v2

**Status:** Active

Jev session/prompt logging uses **`sc-observability` v2** (`LoggerBuilder`,
file/console sinks). **`sc-jev-check`** emits `LogEvent`s through
**`SharedSessionLog`** (see ADR-SAG-0016). Composition roots **`sc-ai`** and
**`sc-jev-py`** link full `sc-observability` and adapt to `SharedSessionLog`.
**`sc-jev`** does not log and does not depend on observability crates *(superseded 2026-11-09, agt-9eh: `sc-jev` logs only through the injected `GeneralLog` port and still has no observability dependency; see the amendment below)*. No bespoke
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

### Amendment (2026-11-09, agt-9eh): the general log

**Status:** Active. The accepted text above is unchanged; this subsection adds
what it did not say. Owner direction (2026-11-08): the two logs are
"instantiate and use". The gap was that general logging was not designed before
code was written, so the code carried only the session log. This amendment adds
the general log and requires the logger to be injected everywhere an operator
needs to see what happened.

- (Active, 2026-11-09) **Two logs, two files.** The *session log* is the
  ADR-SAG-0031 row per judge send (`<log-root>/logs/<service>.log.jsonl`). The
  *general log* is operator diagnostics
  (`<log-root>/logs/<service>-general.log.jsonl`): start and end of operations,
  backend selection and resolved model, config and path resolution, credential
  source (a name, never a value), busy retries and waits, extraction and git
  steps, session store actions, flush failures, and failures by typed error id.
  Both are written with `sc-observability`; there is no bespoke writer and no
  `tracing` or `log` dependency.
- (Active, 2026-11-09) **`sc-jev`.** The sentence "`sc-jev` does not log" above
  is amended: `sc-jev` logs only through the injected `GeneralLog` port
  (`JevClient::with_general_log`) and still has no observability dependency.
- (Active, 2026-11-09) **Correlation and failure detail.** Every event of one
  run carries the same `run_id` (a ULID; `CorrelatedGeneralLog`, applied once per
  run by `sc-jev-host`), so runs sharing a file can be told apart. `send.done` and
  `run.done` carry `duration_ms`; `run.done` and the send events carry the
  `session_id`. A failure event (`run.failed`, `ask.failed`, `prepare.failed`,
  `operation.failed`) carries the fixed `stage` name, the `backend` and
  `session_id` when known, `duration_ms` and the typed `error_code`. A stage
  is a fixed string, never provider or prompt text.
- (Active, 2026-11-09) **Durability.** When `run_check` returns, its default general-log rows are on disk, including with several threads: the runtime allows one flush at a time and refuses a second, so the wheel serializes its flushes and retries a refusal (a failed flush is a Warn row `general_log.flush_failed`). A fresh-process test that exits at once checks it with two and three concurrent runs (20 repeats each, where the loss showed) and eight (5 repeats). The wheel job also builds on macOS. `.cargo/config.toml` sets `-undefined dynamic_lookup` for both darwin targets because maturin 1.9.4 (what the kit's release workflows install, inside the allowed `>=1.8,<2`) does not link `sc-observability-py`'s cdylib without it; maturin 1.16 does, so no kit repin is needed. The flag applies to every darwin build, `sc-ai` included, where a missing symbol would link and then crash at run time. CI job `darwin-link` closes that gap: `.github/scripts/check-undefined-symbols.sh` fails if `nm -um` lists any `(dynamically looked up)` symbol in the release `sc-ai` or `sc-treesitter`, and its `--self-test` proves it fails a binary with a missing extern.
- (Active, 2026-11-09) **Failures before the run.** Root discovery in
  `prepare_from` and `ask_from`, and in `sc-ai` the repo root, `--request-json`
  and the check source, log `operation.failed` by typed id
  (`compose::log_failure`), at the resolved log root or, when none yet, at a root
  known without a repository (`--log-dir`, `SC_LOG_ROOT`, or a home default).
- (Active, 2026-11-09) **Silent paths.** These log nothing, on purpose:

  | Path | Why |
  | --- | --- |
  | A failure before any log root is known (CLI default root is repo-relative, no `--log-dir`, repo root not found) | There is nowhere to write; opening one in an unknown directory would create files. The error still reaches stderr. |
  | A dry run | ARCHE-019: a dry run opens no log or directory. Injected loggers still receive events. |
  | `sc-treesitter` (library and binary) | A stdout-JSON producer with no operation an operator diagnoses; `sc-jev-check` logs the extraction that runs it. |
  | `sc-agent-types::git` and `sc-decision-schema` | Pure helpers with no logger by design (core crates link no port consumer); the caller logs the git failure (`git.main_repo_failed`, test `a_git_failure_is_logged_by_its_caller_and_the_run_still_succeeds`). |
  | A failure of the general log itself (open, rejection, flush) | There is no log left to report to, and logging never changes a result. |
- (Active, 2026-11-09) **Port.** `sc_agent_types::observability::v2` gains the
  open `GeneralLog: Send + Sync` trait (`fn log(&self, GeneralEvent) ->
  Result<(), LogRejected>`), `SharedGeneralLog = Arc<dyn GeneralLog>` and
  `NoopGeneralLog`, beside `SessionLog`. A `GeneralEvent` is a level, a stable
  `target` and `action`, a message and structured fields. Libraries receive the
  logger as a value (`RunConfig::general_log`, `BusyPolicy::with_general_log`,
  `HttpClient::with_general_log`, the backends' `with_general_log`,
  `resolve_key_logged`, the `*_logged` registry constructors); there is no
  global logger and no library crate links `sc-observability`.
- (Active, 2026-11-09) **Injection and defaults.** `ComposeInputs::session_log`
  and `ComposeInputs::general_log` each replace one default; unset, the host
  opens sc-agent's default under the resolved log root
  (`ObservabilityAdapter`, `GeneralLogAdapter`, the latter at `debug` and
  above). A dry run opens no default log (ARCHE-019). The default general log
  is best effort: if it cannot start the call carries on with none; the session
  log keeps its contract. An injected log's flush is the caller's.
- (Active, 2026-11-09) **CLI.** `sc-ai` injects nothing: it keeps the default
  loggers and `--log-dir`. Injection is a Rust and Python API concern.
- (Active, 2026-11-09) **Never leaks, never changes a result.** No event carries
  an API key, prompt text, session state or a provider body; errors are logged by
  `ErrorCode` only. A logger that rejects, raises or cannot flush changes no
  result and fails no run. A session-log flush failure keeps its CR-014
  warning and is also written to the general log.
- (Active, 2026-11-09) **Python.** `sc-jev-py` links `sc-observability-py` (lib
  `_native`) with `sc-observability-binding-runtime`, registers the module
  inside its own binary (`wrap_pymodule!`), seeds
  `sys.modules["sc_observability._native"]`, installs a core backend with
  `install_host_logger`, and keeps it in a static. Rust writes each row with
  `try_log` inside `py.detach`, so **no row takes the GIL**; Python gets the
  same queue and file from `sc_observability.get_host_logger()`. `run_check`
  also takes `session_log=` and `general_log=` callables (the GIL is taken per
  row for those only; one that raises is ignored). Python may be more flexible
  than the CLI, never less. The default path takes no GIL; an injected
  callable takes it once per row, which is the cost of an arbitrary Python
  logger, and there is no queue. One process has one host logger, installed by the
  first default general log or by `install_host_logger(log_dir=None)`. `pyo3`
  is pinned `=0.29.2` (the version `sc-observability-py` pins), and the three
  `sc-observability-*` runtime crates join the lockstep list. The wheel's
  stable-ABI floor is the lower of the two requests, `abi3-py310` (the
  runtime asks for 310, this crate formerly asked for 311, and Cargo unions
  the features), so the built wheel is tagged `cp310-abi3`, read from the
  wheel. `Cargo.toml`, REQ-SAG-0802, `docs/development/sc-publish.md` and the
  `wheel` CI job (which fails on any other tag) all state that floor.
  `requires-python` stays `>=3.11`, the versions the tests cover.

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
  `backend()` / `payload_sha256()` accessors are removed (the values are in
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
- (Active, r-1) `equal_input` is `{payload_sha256, mode, backends: [{backend,
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
  `session_saved`), `session_saved`, `payload_sha256`, `payload_bytes`,
  `results` (per backend: `ok` + outcome, or `failed` + the
  `ScAgentErrorResponse` derived from its error), `equal_input`, dry-run
  `payload`/`limits`, and last `warnings` (array of strings, omitted when
  empty). `warnings` replaces the Phase A `data.warning` key; the
  composition crate adds the log-flush warning through
  `RunResponse::with_warning` (r-3b). The `ask` body is the follow-up's
  answer fields by the same primary rule, then `session_id`,
  `payload_sha256` and `warnings` (the CR-014 flush warning, through
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
the snapshot (test `codes_match_readme_contract_snapshot`). *(Superseded by the
agt-2a3 amendment to ADR-SAG-0027: six ids were added to `errors.json`, and
the table's row for file or process I/O now reads `IO_FAILED`.)*

| Surface | Failure | Before | After |
| --- | --- | --- | --- |
| `sc_jev_py.run_check` / `_testing.run_check_scripted` | unknown backend name | `CONFIG_INVALID` | `INPUT_INVALID` (`BackendId::from_str`, same as `sc-ai` and `check.toml`) |
| same | kebab-case alias (`anthropic-haiku`, `openai-luna`) | accepted | `INPUT_INVALID` (one documented spelling) |
| any crate | `serde_json` encode / decode failure (`JevError::encode_failed`) | mixed (`INPUT_INVALID`, `EXTRACTION_FAILED`) or raw text | `INPUT_INVALID`, detail keeps only category and line/column |
| any crate | file or process I/O failure (`JevError::io_failed`) | mixed (`INPUT_MISSING` for any read error, `EXTRACTION_FAILED` for git spawn) | `INPUT_INVALID` with the `io::ErrorKind`; a missing file stays `INPUT_MISSING` *(Superseded by agt-2a3: `IO_FAILED` with `details.what` and `details.io_kind`; a missing file stays `INPUT_MISSING`.)* |
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
  `PayloadBundle.sha256`, `EqualInputRecord.payload_sha256`, and log fields — not
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
  determinism; `pyo3`, `sc-observability-py`,
  `sc-observability-binding-runtime` and `sc-observability-dto` — the Python
  wheel embeds the `sc-observability` Python runtime in its own binary, and
  that runtime pins `pyo3 =0.29.2`, so both ride one exact version (agt-9eh,
  ADR-SAG-0013 amendment 2026-11-09); `sc-jev-host` — lockstep internal support (owner
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
  pyo3
  sc-observability-py
  sc-observability-binding-runtime
  sc-observability-dto
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

### Amendment (Phase A.1, phase-end)

**Status:** Active (Phase A.1) — landed in the phase-end release layer.

- (Active, phase-end) Windows MSVC targets link the C runtime statically
  for released binaries and for the `sc-jev-py` wheel, so they run without
  the Visual C++ redistributable (sc-lint failed with `STATUS_DLL_NOT_FOUND`,
  `0xC0000135`, without it). The flag
  `-C target-feature=+crt-static` lives in the repository
  `.cargo/config.toml` under `[target.x86_64-pc-windows-msvc]` and
  `[target.aarch64-pc-windows-msvc]`; Linux and macOS builds are unaffected.
  Cargo configuration cannot be scoped to one package, so the wheel built
  from the repository root gets it too.
- (Active, phase-end) **Re-qualification trigger.** `RUSTFLAGS` replaces
  `target.*.rustflags` entirely. A sc-publish pin that sets `RUSTFLAGS` in a
  build step silently drops the flag; any pin bump must re-check this and
  re-qualify the Windows binaries and wheel before it merges. A kit-side
  override for the wheel step is the fix if a statically linked CRT in the
  Python extension proves a problem; it is not a repo edit.
- (Active, phase-end) The Windows legs (`windows-2022`, `windows-11-arm`) of
  the `wheel` job in `.github/workflows/sc-agent-ci.yml` are the wheel's
  acceptance check: they build the release-shape wheel, install it in a fresh
  virtual environment and import `sc_jev_py` without `_testing`. The wheel
  matrix is six wheels (Linux, macOS and Windows on x86_64 and arm64,
  `cp310-abi3`; amended agt-9eh from `abi3-py311`), declared in `release/sc-publish-consumer-input.json`
  (REQ-SAG-0802). Operational notes: `docs/development/sc-publish.md`.

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
  `session_saved`, `payload_sha256`, `payload_bytes`, `results` (per backend
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
  `session_id`, `payload_sha256` and `warnings` (omitted when empty;
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

### Amendment (phase-end digests, 2026-11-08)

**Status:** Active (phase-end digests).

- (Active, phase-end digests) `PreparedRun::request_sha256()` returns, per
  selected backend, the SHA-256 of the exact first request body that backend
  is sent for the model it runs with, and `RunResponse::request_sha256()`
  carries the same list (`data.request_sha256`, an object keyed by backend
  id). It is computed by `prepare`/`run` before the first send, with no API
  key and no send, through two key-free entry points:
  `sc_jev::request_sha256(model, payload)` (the Jev body) and
  `sc_jev_backends::request_sha256(id, model, request)` (the LLM backends'
  first body, the token-count body, which holds the model, the payload and
  the system prompt). It is the digest `SendTrace` records, so it equals the
  session-log row's `request_sha256` for the same send (ADR-SAG-0031). The
  per-backend request is the one the engine would build (`request_for`), so
  an LLM backend's system prompt is included and Jev receives none.
- (Active, phase-end digests) The digest is absent for a backend whose bytes
  cannot be known before a send: an LLM backend after another backend in
  `chain_context` mode (its system prompt carries the earlier answers), or an
  LLM backend that refuses the questions (such as `noul`), where nothing would
  be sent. The size limit is not applied, so a dry run reports an over-limit
  body's digest next to its `over_limit` flag.
- (Active, phase-end digests) `RunResponse::questions_sha256()` is the
  digest of the question set alone (ADR-SAG-0031 and REQ-SAG-0621).
  `payload_sha256` is the renamed `prompt_sha256`. `build_request()` and
  `CheckResult` in Python carry the same three values, and Python accepts the
  CLI's `--request-json` document as `request_json=` (REQ-SAG-0622). The keys
  are additive in the `data` body.

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
  returns `Err` *(superseded by the agt-2a3 amendment (all-failed runs keep
  every error): `run` returns the response)*. `judge` is called once per backend; only `BusyPolicy`
  retries; a session-save failure keeps the answers with
  `session_saved = false` and one `warnings` entry. REQ-SAG-0604 (engine part).
- (Active, r-3b) REQ-SAG-0604 consumer output: on a partial result
  (`RunResponse::is_partial()`) `sc-ai` prints the envelope with the
  successful answers on stdout, the first failure's
  `ScAgentErrorResponse` on stderr, and exits 2 *(2, or 3 when every failed
  backend's error is retryable: agt-2a3 amendment)*; `sc-jev-py` raises
  `JevError` with a `.response` attribute holding the envelope `data`.
  All-failed stays a plain error. *(Superseded by the agt-2a3 amendment
  below: an all-failed run returns the same response.)*
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
- (Active, phase-end digests, 2026-11-08) The run's digest of the canonical
  payload, questions and state, was named `prompt_sha256` when the bullet
  above was accepted; it is `payload_sha256` now (renamed everywhere, no
  alias; see ADR-SAG-0031 for the session-log columns). It is the
  backend-neutral equal-input proof: every backend judged the same input. It
  excludes the LLM system prompt and the model, so it cannot key a response
  cache. Two other digests have other jobs and do not replace it:
  `questions_sha256` identifies a question set (same questions, any state),
  and per-backend `request_sha256` identifies an identical request (the exact
  first body for that backend and model, computed before sending), for
  cached-response substitution (REQ-SAG-0621).

### Amendment (agt-2a3, all-failed runs keep every error)

**Status:** Active (bead `agt-2a3`, before the first release).

- (Active, agt-2a3) A run in which every selected backend failed is no
  longer a plain error that keeps only the first backend's failure. `run`
  returns the `RunResponse` with no primary answers, `session_saved: false`
  and one typed failure per backend in `results`, exactly like a partial run
  (`is_partial()` is true). `sc-ai` prints that envelope on stdout (no
  `answers` or `primary_backend` keys), the first failure on stderr, and
  exits 2, or 3 when every failed backend's error is retryable (ADR-SAG-0027 agt-2a3
  amendment). `sc-jev-py` raises `JevError` for the first failure with
  `.response` set to the same body. A run that selected no backend stays an
  `INPUT_INVALID` error. This supersedes "All-failed stays a plain error" in
  the (Active, r-3b) consumer-output bullet above and the (Active, r-2b)
  engine bullet "if all fail, `run` returns `Err`". REQ-SAG-0604 is amended
  to match.
- (Active, agt-2a3) Each stored backend failure names its backend in
  `details.backend` (the descriptor name), so a failure read on its own,
  for instance the stderr line, says which backend it came from.

## ADR-SAG-0026 — Shared HTTP crate and host crate

**Status:** Active (HTTP half landed in r-3; host half landed in r-3b)

HTTP half amends ADR-SAG-0006, 0007,
0017 and 0020; host half amends ADR-SAG-0010, 0011, 0013, 0016 and 0017.

### HTTP half

**Status:** Active (Phase A.1) — landed in r-3.

- (Active, r-3) `sc-agent-http` (published) owns `HttpRequest`
  (`body: Arc<str>`) / `HttpResponse`, `Endpoint`, `SecretHeader`,
  `Transport`, `UreqTransport` (one `Agent` built in `new`, 8 MiB streaming
  response cap; private module `ureq_transport`), `BusyPolicy` /
  `send_with_busy_retry` (with `CancelToken`) and redaction of every error it
  returns, and is the single secret-read site (ADR-SAG-0020 r-3 amendment).
  It depends only on `sc-agent-types` (`default-features = false`,
  `features = ["secret"]`) and `ureq` *(and `serde_json`, for reading a
  failing reply's error JSON only: agt-2a3 amendment below)*; no `ureq` type is public. Its
  non-default `test-util` feature (test hooks for clock, sleeper and jitter;
  loopback-only `http` endpoint; test transports) is enabled only by
  dev-dependencies. Phase B provider agents reuse it. REQ-SAG-0402.
- (Active, r-3) ADR-SAG-0012 still holds: `sc-jev` owns every TypeSafe
  Jev API operation (endpoints, wire body, response parsing);
  `sc-agent-http` only carries bytes.

- (Active, 2026-11-09, agt-9eh) The "`features = [\"secret\"]`" clause above is
  amended: `sc-agent-http` takes `sc-agent-types` features `secret` **and
  `obs`**. `obs` is for the `GeneralLog` port only (`BusyPolicy::with_general_log`,
  `HttpClient::with_general_log`); it adds no third-party edge and no
  `sc-observability*` dependency. The `cargo tree` gate that held the edge to
  `secret` now holds it to `secret` and `obs` and still forbids `git`, `default`
  and `test-util`. Logs name an endpoint only through `Endpoint::log_form`
  (scheme, host and path; no userinfo, port, query or fragment).

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
  allowed by name in `test_env_reads_only_in_owners`. The extractor child
  environment allowlist and the secret-value scan also read the whole
  environment (`std::env::vars_os()` in `apply_env_allowlist` and
  `secret_values`, `crates/sc-jev-check/src/extract/sanitize.rs`); that file
  is in the scan's owner list, and the scan's needles include `vars_os(`, so
  an unlisted file reading the environment that way fails the test.

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

### Amendment (agt-2a3, provider failure classification)

**Status:** Active (bead `agt-2a3`, before the first release). Amends the
HTTP half of ADR-SAG-0026.

- (Active, agt-2a3) `sc-agent-http` adds `serde_json` to its allowed
  dependencies (`boundaries/sc-agent-http/agent-http.toml`; no other new
  edge). `sc_agent_http::failure` is the one place that reads a provider's
  error JSON: it maps a failing HTTP status and body to an error id
  (`api_failure`) and answers the busy retry's quota question
  (`is_quota_reply`). It reads only the error fields of a non-success reply
  (type, code, message, parameter, request id, `Retry-After`), never a
  success body, a request, or a Jev answer. The shapes it reads: the
  Anthropic and OpenAI `{"error": {type, code, message, param, details}}`
  and `request_id`; for Jev (undocumented here) `{"detail": "..."}`,
  `{"detail": [{"loc", "msg"}]}`, `{"detail": {"error_type", "message"}}`
  (seen in live replies, 2026-11-08), `{"message": "..."}`, a top-level
  `code`, and, for any other body, the whole body, scrubbed then bounded, as raw text.
  For Haiku and Luna "the input is too long" is recognised only from
  structured markers, status 413 and Anthropic's "prompt is too long"
  message (unverified wording); only Jev's message text is matched for
  `max_tokens`.
- (Active, agt-2a3) This squares with "the crate only carries bytes" and
  ADR-SAG-0012: Jev API semantics (the check wire body, the reply parser,
  every Jev operation) stay in `sc-jev`; this crate still owns no wire
  document and no answer type. The failure classifier is generic transport
  error handling shared by all three backends, so it lives next to the retry
  loop that needs the same quota test, instead of being copied into `sc-jev`
  and `sc-jev-backends`. Bounds and redaction are ADR-SAG-0020's
  agt-2a3 amendment; ids and retryability are ADR-SAG-0027's.

## ADR-SAG-0027 — Error taxonomy and extensibility

**Status:** Active (Phase A.1, landed in r-1). Amends ADR-SAG-0001.

- (Active, r-1) `JevError`, `BackendId` and `BackendKeyId` are
  `#[non_exhaustive]`, and struct-like `JevError` variants are
  `#[non_exhaustive]` so downstream crates use constructors; every
  struct-like variant has a public constructor. Recovery
  categories are explicit variants (`CredentialStoreInvalid`,
  `RegistryInvalid`, `SessionStoreIo`, `Operational { area }`,
  `SessionDocumentInvalid`, `EncodeFailed`, `IoFailed`; *the variant list is
  replaced by the agt-2a3 amendment (Variants)*). Producer variants are
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
  when Phase B needs an `INTERNAL` class. *"Its ids are unchanged" is
  superseded: the agt-2a3 amendment adds six ids.*
- (Active, r-1) `retryable()` is true only for `NetworkError` kinds Busy,
  Timeout, Connect. *Superseded by the agt-2a3 amendment (Retryability).*
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
  *(Superseded by the agt-2a3 amendment: readers ignore unknown keys and an
  added key does not bump the version.)*
  Every stderr line gains `"schema_version":1`; NFR-SAG-0001 and
  REQ-SAG-0701 are amended and a golden stderr test pins the bytes.
  *The key order and the `retryable`, `details` and message rules are
  superseded by the agt-2a3 amendment (Envelope).*
- (Active, r-1) **Consumer rule.** Agents and CLI/MCP clients may read
  `schema_version` from every stderr line. A value other than `1` means
  "unknown error shape": fall back to the exit code and the raw line. A bump
  is a breaking change to be announced in `CHANGELOG.md` (nothing has been
  released yet, so that file has only an unreleased 0.1.0 entry).
- (Active, r-1) `Error::source()` is `None` for every redacted-cause
  variant (`CredentialStoreInvalid`, `NetworkError`, `EncodeFailed`,
  `IoFailed`, `SessionStoreIo`); no A.1 variant carries a non-redacted source.
  *`SessionStoreIo` no longer exists and the list is `CredentialStoreInvalid`,
  `NetworkError`, `EncodeFailed`, `IoFailed` (agt-2a3 amendment).*
- (Active, r-1) Serde spelling of backend ids matches `Display`
  (`openai_luna`, `openai`), with a one-release read alias for
  `open_ai_luna` / `open_ai` in config and caller-request input. Whether and
  when the alias is removed is the owner's decision, tracked by bead
  `agt-ho2`; nothing has been released, so no release
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
  enums (`ErrorCode`, `NetworkErrorKind`, `OperationalArea`; *`OperationalArea`
  is removed and `ErrorCategory`, `BusyCause`, `ResponseInvalidCause` and
  `LimitUnit` are added by the agt-2a3 amendment*) sit in their
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

### Amendment (agt-2a3, error contract)

**Status:** Active (bead `agt-2a3`, before the first release). Owner rules:
the error body returns at least the detail the Jev service returns, with a
discriminated id and a message on every path; out of tokens, busy and API
error are distinct ids handled differently inside sc-agent; nothing has been
released, so there are no compatibility shims. This subsection supersedes
the (Active, r-1) bullets on `retryable()` and on the field rule, and the
(Active, r-1) variant list, as marked.

- (Active, agt-2a3) **New ids.** `errors.json` gains `AUTH_FAILED` (401, 403),
  `API_REQUEST_INVALID` (400, 422), `QUOTA_EXHAUSTED` (out of tokens, credit
  or spend allowance), `PROVIDER_BUSY` (rate limit or overload after the busy
  retry), `API_ERROR` (every other failing provider status, 5xx included) and
  `IO_FAILED` (a file, process, stream or dependency failed, with `what` and
  the `io::ErrorKind`). `NetworkErrorKind` is now `timeout`, `connect`, `tls`,
  `cancelled`: a reply with a failing status is never a network error, so
  `busy`, `status` and `body_too_large` are removed. `INPUT_TOO_LARGE` has
  one shape for the byte cap, the token cap and a provider's own refusal:
  `limit`, `actual`, `unit` (`bytes` or `tokens`). `RESPONSE_INVALID` has a
  `cause` (`truncated`, `too_large`, `not_json`, `schema`) and its action
  differs per cause.
- (Active, agt-2a3) **Variants.** `Operational { area }`, `OperationalArea`,
  `SessionStoreIo` and `BusyExhaustion` are removed. The session store, a
  credential store that cannot be opened (`.env`, keyring) and every
  process, git, log, stdin, stdout and working-directory failure are
  `IoFailed`. `CredentialStoreInvalid` keeps only a credential whose content
  is unusable (`CONFIG_INVALID`). `ConfigInvalid` and `IoFailed` take an
  optional explicit recovery. `InputInvalid` takes an optional `field`.
  `ProducerError` carries the producer's `suggested_action`, `retryable`,
  `retry_after_seconds`, `field` and `details`, so re-raising a producer's
  error drops nothing. The wrapper variant `Annotated { error, flush_failure,
  backend }` (id, message, action and retryability are the inner error's)
  carries `details.backend` (the failing backend's descriptor name, set on
  every stored backend failure) and `details.log_flush_failure` (a
  session-log flush failure that happens while an error leaves), so neither
  is text appended to a detail. (It replaces the first-draft name
  `LogFlushFailed`.) `NetworkErrorKind` gains `other` (an unclassified
  transport error, not retryable). This replaces the r-1 variant list.
- (Active, agt-2a3) **Which id (the rule).** `INPUT_INVALID` is what the
  caller handed in being malformed or breaking a rule, command-line usage
  errors included. `CONFIG_INVALID` is a configuration source being wrong
  (`check.toml`, `jev.toml`, the backend registry, a credential's content, log
  setup a flag selects). `INPUT_MISSING` is a named path that does not exist
  (the OS said not found), whichever code path reads it
  (`JevError::read_failed` is the one helper). `IO_FAILED` is the operating
  system or a dependency failing during the work (a producer that cannot
  be spawned or waited on, a git that fails, a reader that stops). The
  `EXTRACTION_*` ids stay for the extraction pipeline's own results (empty,
  invalid output, a producer's exit). The `JevError` module documentation
  states the rule; call sites follow it.
- (Active, agt-2a3) **Retryability.** `retryable()` is true for
  `NETWORK_ERROR` of kind `timeout` or `connect`, for `PROVIDER_BUSY`, for
  `API_ERROR` with status 500 or above (500, 503 and 504 included), and for a
  producer's error whose producer said so. It is advice to the caller, not a
  promise that sc-agent resends: only busy statuses (429, 502, 529) are
  resent automatically. A 5xx is flagged retryable because the provider
  failed on its side and a later attempt can get past it (Anthropic's own
  guidance for 500 is to retry with backoff, and the `plan-anchor` Jev client
  treats every 5xx as transient); it is not resent automatically because a
  lost or failed reply can mean the provider already processed and billed the
  request. `QUOTA_EXHAUSTED`, `AUTH_FAILED`, `API_REQUEST_INVALID`,
  `RESPONSE_INVALID` and `INPUT_TOO_LARGE` are not retryable: the same
  request fails the same way. `IO_FAILED` is retryable for `interrupted`
  and `timed_out` only; both `EXTRACTION_TIMEOUT` variants (a stalled git,
  a `command` extractor over its limit) are not, with an action naming the
  likely cause of each. Every non-retryable action says so; a test
  checks that no non-retryable action tells the caller to retry unchanged.
- (Active, agt-2a3) **Quota is not busy.** A 429 (or 502, 529) whose body
  says the account is out of tokens, credit or spend allowance is returned at
  once by the busy retry (one send, no wait) and becomes `QUOTA_EXHAUSTED`.
  The recognized markers are the documented ones: Anthropic's tier spend cap
  (429 `rate_limit_error` with `error.details.error_code ==
  "enforced_spend_limit_reached"`, no `retry-after`), a customer spend limit
  (400 `invalid_request_error`, message starting `You have reached your
  specified`), 402 `billing_error`; OpenAI's `insufficient_quota` and
  `credit_balance_exhausted` codes; Jev's 402 with `detail.error_type ==
  "billing_error"` (seen in live replies, 2026-11-08, unfunded organization,
  checked after authentication and before body validation). A marker in
  `detail.error_type` counts the same at any status, so a Jev 429 saying
  `billing_error` is quota too. An unknown 429 body stays busy. The
  hook is `sc_agent_http::failure` (`is_quota_reply`), called by the retry
  loop; retry timing is untouched.
- (Active, agt-2a3) **Busy exhausted.** `PROVIDER_BUSY` carries
  `retry_after_seconds` (the last valid `Retry-After`, rounded up, capped at
  one day; absent when the provider sent none or an HTTP-date), `cause`
  (`sends` or `budget`) and `attempts`, as machine fields, so no caller
  matches a text suffix.
- (Active, agt-2a3) **Envelope.** `ScAgentErrorResponse` keys, in order:
  `error`, `kind`, `message`, `suggested_action`, `retryable`,
  `retry_after_seconds`, `field`, `details`, `schema_version`. `retryable` is
  always present, `false` included. `message` is `JevError::message()` (what
  failed and why) and never holds the fix; `suggested_action` is
  `JevError::suggested_action()`. `fix_hint()` is removed. `kind` is
  `validation`, `not_found`, `invalid_state`, `dependency`, `permission`,
  `timeout` or `capacity`. `details` is an object (a reader ignores unknown keys) whose keys are
  listed in `docs/stability.md`. Reading ignores unknown keys, at the top
  level and in `details`, so a producer's new key does not make its line
  unparseable (ERR-QA-16), and accepts `schema_version` absent or `1`; only `error` and `message` are
  required on read. This replaces the r-1 field rule.
- (Active, agt-2a3) **`schema_version` stays `1`.** Nothing has been
  published, so no consumer has read an earlier shape; from the first published release on,
  removing or renaming a key or changing its meaning bumps the version, while
  an added key does not because readers ignore unknown keys
  (`docs/stability.md` "Versioning").
- (Active, agt-2a3) **Python.** The `sc_jev_py.JevError` exception carries
  the CLI body's fields as attributes (`code` is the body's `error`), built
  from the serialized body, so CLI and Python cannot drift (ADR-SAG-0011).
- (Active, agt-2a3) `Error::source()` is `None` for every redacted-cause
  variant (`CredentialStoreInvalid`, `NetworkError`, `EncodeFailed`,
  `IoFailed`); `SessionStoreIo` no longer exists.

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
  `PayloadBundle::bytes()`, so `payload_sha256` and the Rust/Python
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

### Amendment (phase-end digests, 2026-11-08)

**Status:** Active (phase-end digests).

- (Active, phase-end digests) **One mapping point for the wire model.**
  `sc_decision_schema::jev::v1::wire_model` maps a `ModelId` to the string
  sent to Jev; `CheckRequest::from_canonical` is the only caller. `jev` (the
  latest) is sent as `jev-latest`, the value plan-anchor's `jev-api.md`
  documents (decision D9: that document is the wire source of truth). This
  mapping is **not yet tested against the live API**; nothing confirms the API
  accepts the bare `jev`. A reply may name a different model than the request
  (`jev` is answered by a concrete version such as `jev-1.13.0`): that is not
  an error, `CheckResponse::into_answers` no longer compares the models, and
  the answered model is read with `JudgeOutcome::answered_model()` (from the
  reply's `model` in `provider_meta`) and printed as the output's `model`. A
  reply with no model is still `RESPONSE_INVALID`.
- (Active, phase-end digests) **`ModelId::Jev` is a floating variant with no
  `MODELS` row.** It names the latest Jev model, not one release, so it has no
  release date and must not become the `jev` alias default
  (`ModelAliases::latest("jev")` stays the newest pinned row, `jev-1.13.0`);
  `ModelId::family()` answers `TypeSafeJev` for it directly. It may break on
  an API change this build does not know; that is expected and the remedy is
  upgrading sc-agent. Cache keys made with `jev` can go stale when Jev moves
  (docs/stability.md).
- (Active, phase-end digests) **Sum tolerance.** `CheckResponse::into_answers`
  accepts a `choice` or `score` answer whose probabilities sum to 1 within
  0.01 per option (plan-anchor's `PROBABILITY_ROUNDING`), because Jev rounds
  what it sends. The check validates only: values pass through unrounded
  (`f64` throughout, sc-agent never rounds), and the range check (finite, in
  `[0, 1]`) is unchanged. This supersedes the earlier 1e-6 bound.

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
  *(Superseded in part by the phase-end digests bullet below, schema_version
  2: the table list (`questions` is now `question_sets` plus `payloads`), the
  index on `(prompt_sha256, model_id)` (now two indexes on
  `questions_sha256` and `payload_sha256`; the column is `payload_sha256`),
  "the other two tables", and "`schema_version` starts at 1" (it is 2 now).
  The rest stands.)*
- (Active, phase-end digests; `schema_version` 2, 2026-11-08) **Three
  digests and a question-set key.** Version 1 keyed `questions` by the
  payload digest, which includes the state, so the same questions asked
  against different state were unrelated rows. Version 2 has `sessions`,
  `question_sets` (by `questions_sha256`: `first_seen_at`), `payloads` (by
  `payload_sha256`: its `questions_sha256`, `question_ids` in payload order,
  `first_seen_at`) and `answers` (with `questions_sha256` and
  `payload_sha256`, referencing `payloads` by the composite
  `(payload_sha256, questions_sha256)`, so a send cannot name a payload under
  another question set), indexed by `(questions_sha256, model_id)`,
  `(payload_sha256, model_id)`, `request_sha256` and `(repo, project)`; the
  loader's `INSERT OR IGNORE` covers `sessions`, `question_sets` and
  `payloads`. `question_ids` order lives on `payloads`, not `question_sets`,
  because `questions_sha256` does not depend on question order, so the order
  is not fixed by the question set; it is fixed by the payload digest.
  Each digest has one job: `questions_sha256` identifies a question set (same
  questions, any state; independent of question and `choice` option order,
  dependent on `score` level order) so runs can be compared across states;
  `payload_sha256` (renamed from `prompt_sha256`) identifies questions plus
  state, is backend-neutral, is the equal-input proof, and excludes the LLM
  system prompt and the model; `request_sha256` identifies the exact request
  bytes of one send for one backend and model, for cached-response
  substitution. Version 1 rows are refused (`INPUT_INVALID`); nothing was
  released at version 1. The principle of the old key still holds: every
  deduplicated table is keyed by the digest that fixes its other columns.
- (Active, r-7) **Projection.** The MVP store is JSONL; the projection of
  one send is the `fields{}` object of one sc-observability v2 log line
  (ADR-SAG-0013). It has exactly the schema's column keys: nullable columns
  present as `null`, `question_ids` a JSON array, `answers_json` a JSON
  object. The outcome key is `send_outcome` (`ok` | `failed`), so it never
  collides with the envelope's `outcome`. `ts` is the row's time; the
  envelope `timestamp` is write time. A loader reads `fields{}` into the
  tables (`INSERT OR IGNORE` for `sessions` and `questions`), so the
  projection is lossless. This supersedes the r-6 draft's "flat top-level,
  no nesting" wording, which never landed. *(Superseded in part: since
  schema_version 2 the `INSERT OR IGNORE` tables are `sessions`,
  `question_sets` and `payloads`, not `sessions` and `questions`.)*
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
  built. `payload_sha256` stays the canonical-payload digest
  (ADR-SAG-0002/0025). From the phase-end digests, `request_sha256` is also
  computed before sending (no key, no send) for each backend whose first body
  is known in advance, and equals the value the row records.
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

## ADR-SAG-0032 — Streaming decision calls

**Status:** Active (agt-1aq). Owner decision 2026-11-08: streaming is a small
change, not a redesign. One `sc-ai` instance answers thousands of decision
queries, so callers do not spawn a process per query.

- (Active) **A bounded queue with N worker threads around the existing
  per-request work.** `sc_jev_host::stream::DecisionStream` runs
  `sc_jev_check::run::{prepare, run}` for one caller-built request per job. No
  async runtime. The engine, `JudgeBackend`, `Transport` and `SessionLog` are
  unchanged; the host wraps them. The job queue and the answer queue each hold
  `2 x N` items, so at most `5 x N` requests are in the process at once
  (queued, running, answered and unread). A slow reader stops the workers, then
  fills the job queue, then blocks the submitter: backpressure reaches the
  producer (the stdin pipe, or Python `submit`). An input line is capped at
  1 MiB.
- (Active) **Same capability from every adapter.** One input is a JSONL file
  (`--input`), a stdio stream (`sc-ai jev stream`), or a Python call
  (`run_many`, `DecisionStream`). Requests can keep arriving while earlier
  answers come back, so a caller may feed an early decision back in as a
  follow-up query on the same stream.
- (Active) **Wire format.** Input line:
  `{"id": "<caller id>", "request": <the --request-json document>}`. Output
  line, one per request, in completion order (not input order):
  `{"id", "ok", "envelope"?, "error"?}`. `envelope` is the existing `jev run`
  envelope without the path readback; `error` is the existing
  `ScAgentErrorResponse`. `ok` is false whenever `error` is present. A failed
  request has only `error`; a request with some failed backends has both
  `envelope` and `error` (as `jev run` prints stdout and stderr), so a client
  reads `envelope` whenever it is present. `id` is echoed whenever a usable id
  could be read from the line, even if the rest is invalid; it is `null` only
  when none could. Ids are 1 to 256 bytes with no control character and are
  not checked for uniqueness. `sc-jev-host` gains a direct `serde` dependency
  for the wire structs (and the `serde_json` `raw_value` feature, so the
  request document is passed through byte for byte); it adds no new crate to
  the lockfile.
- (Active) **Per-request failure never stops the stream.** The failure is a
  typed error on that request's line. **Exit code**, the existing rule applied
  across the stream: 0 when no request failed; 3 when every failure is
  retryable; otherwise 2. A stream-level failure (unreadable input, closed
  stdout) is one stderr `ScAgentErrorResponse` line and its own exit code.
- (Active) **Finished** means: input closed, queue empty, workers idle. The
  last worker out closes the answer queue, so reading answers ends exactly
  then.
- (Active) **Concurrency** is one global setting, `--concurrency` /
  `concurrency=`, 1 to 64, default 8. The default is a hardcoded owner
  suggestion. The mechanism is tested with scripted and loopback backends
  only; **the default was not tested against live providers.**
- (Active) **Thread safety.** `JevClient` has no interior mutability and its
  transport is `Send + Sync` (`JudgeBackend: Send + Sync` makes this a compile
  fact; a loopback HTTP test holds four requests in flight over one shared
  client). The stream builds **one registry, so one client, per backend
  selection** and shares it across workers; there is no client per worker.
- (Active) **Cancellation** stops new work: `cancel` closes the input, cancels
  the stream's own `CancelToken` (`start` replaces any token on the inputs, so
  cancelling is only through the stream), so a busy-retry wait ends within
  100 ms, and answers each request still queued with a `NETWORK_ERROR` "cancelled" line.
  A request already sent is not interrupted. The CLI cancels on a write
  failure; it installs no signal handler (SIGINT ends the process, as for
  `jev run`).
- (Active) **Logging.** All workers share one session logger (one writer;
  concurrent writing is `sc-observability`'s) and one general logger. An
  injected `ComposeInputs::session_log` or `general_log` is used (its flush is
  the caller's); otherwise the defaults under the log root are opened once and
  flushed by `finish`. The stream has its own `run_id` (on `stream.start` and
  `stream.end`, with worker counts, answered and failed counts and the
  duration). **Each request is a run with its own `run_id`**, stamped on every
  event of that request, the engine's included, together with the stream's id
  and the caller's `request_id`; `stream.request.start`, `.done` (with
  `duration_ms`), `.failed` (typed `error_code`, coarse `stage`) and
  `.backend_failed` (backend id and `error_code`) are the rows. Never a prompt,
  a question, state or a secret; a request id is caller text, so it must
  carry no secret. All of it goes through `stream/log.rs`.
- (Active) **Leaving a stream flushes, within a bound.** Every way out
  other than `finish` (`abandon`, a drop, a Python `with` exit, an interrupted
  `run_many`, `Batch` drop) cancels, waits at most **2 s** (`ABANDON_WAIT`) for
  the workers, then writes `stream.end` and flushes the default session and
  general logs, once (a repeated `finish` writes no second `stream.end`). A
  request still on the wire after the bound can lose its rows; no exit waits
  longer than the bound. Python `DecisionStream` and `run_many` take
  `session_log=` and `general_log=` like `run_check` and default to the shared
  host logger, flushed on every exit, so `get_host_logger()` shares its writer.
  A leading UTF-8 byte order mark on the input is ignored.
- (Active) **Lifecycle edges.** `finish` closes the input and drops unread
  answers instead of waiting for a reader, so it never hangs on them. An
  unread answer holds capacity: a caller that submits more than `capacity`
  requests from the thread that should read the answers waits forever unless it
  reads from another thread or submits with a timeout. A worker's exit is
  accounted by a drop guard, and the path for a job that is not run is guarded
  too, so every accepted job gets exactly one answer even if logging panics
  there. The Python batch driver is `sc_jev_host::stream::Batch`; the PyO3
  crate only converts arguments, detaches the GIL and maps errors (ADR-SAG-0011).
- (Active) **Out of scope.** Resume, sha256 lookup and any result cache (a
  general concern for all calls); a shared rate limiter (each request keeps its
  own busy retry); a new session-log writer.
