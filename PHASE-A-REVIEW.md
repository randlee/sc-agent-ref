# Phase A — critical review (integrate/phase-a @ d4a0111)

**Generated:** 2026-11-06. Untracked handoff document, like `PHASE-A-STATUS.md`. Do not commit.

**What was reviewed.** All ten Phase A sprints merged on `integrate/phase-a` at
`d4a0111d2df5c818222e9c7388e206f1d4e9bc20` (~14k LOC, 8 crates). Twelve reviewers ran
against that head: the standard six-agent phase-ending QA set (req-qa, arch-qa, rust-qa,
rust-best-practices, rust-service-hardening, flaky-test-qa) plus six deep reviewers
(extensibility-qa, ruthless-boundary-qa, a bug-hunting rust-code-reviewer that reproduced
its findings against built binaries, and three slice-by-slice architecture critiques).
Raw fenced JSON for every reviewer is under
`.cursor/phase-a-orchestration/phase-end-qa/` and `phase-end-deep/`; the enforcer report is
`phase-end-qa/findings-report.md`.

## 1. Verdict in one paragraph

The low-level engineering is good and should not be touched: secret hygiene, stable error
ids, transport hardening (https-only, no redirects, caps, timeouts), refuse-before-send,
atomic 0600 session files, argv-only subprocess control, canonical-JSON parity with Python,
deterministic subject ids, and test-util gating that provably stays out of release builds.
The problems are in the layers that grew sprint by sprint on top of that core: the engine
has four overlapping entry points, the session store carries hidden mutable state, two
"guarantees" are hollow (the equal-input record can only ever say `equal: true`, and the
documented `{path}` command placeholder cannot be used from `check.toml`), the two
consumers (sc-ai, sc-jev-py) re-implement orchestration and already disagree, and the
backend identity is a closed enum despite docs saying the registry is open. There are also
a handful of real correctness and security bugs (git option injection through `--rev`,
worktree reads following symlinks out of the repo, subprocesses inheriting API keys,
`--rev` reading a different file than WORKTREE when the repo root is a subdirectory).
All of those are routine fixes. Phase A is complete; it is not yet a foundation Phase B
should build on unchanged.

## 2. Gate result

| Gate | Result |
| --- | --- |
| Phase-ending QA (6 reviewers, enforcer) | **FAIL** — 5 Blocking, 21 Important, 21 Minor (47 ids); 28 routine, 19 architectural |
| Deep review (6 reviewers) | 2 Blocking + 1 Critical (boundary manifests), ~45 Important, ~50 Minor across EXT/RBQA/CR/ARCHC/ARCHE/ARCHP |
| `just lint`, clippy, fmt, 194 tests ×2, publish dry-run, wheel | all green |
| Coverage (llvm-cov) | 82.5% line overall; sc-jev 65%, sc-jev-py 60%; both HTTPS transports under 35% |
| CI | **none** runs tests, clippy, pytest or coverage; only the sc-publish dry-run workflow runs on PRs |

The five phase-QA blockers are all documentation and evidence gaps, not code: three sprint
docs still say `in_progress`; REQ-SAG-0610 / NFR-SAG-0002 / NFR-SAG-0003 are claimed by two
sprints; a-8 deferred the wheel+pytest CI proof to a-9 and a-9 never delivered it; the
plan.md acceptance command set has no CI or recorded evidence; and the pin/allocator/second
binary decisions have no ADR.

## 3. What is being fixed now (routine, no decision needed)

Dispatched to rust-developer on `sprint/a-10-phase-end-hardening` (code, three waves) and
`sprint/a-10-docs-ci-boundaries` (docs, boundaries, CI). Each item maps to reviewer ids.

**Security / correctness (first).**
- Git option injection via `--rev` in sc-treesitter: `--end-of-options`, reject leading
  `-`, resolve to blob sha then `cat-file blob` (atomic bytes+sha, rejects trees), accept
  64-hex, locale-independent error classification, same path rules as sc-jev-check.
  (CR-005, RBQA-F006, ARCHP-004, RBP-F003)
- Worktree reads: canonicalize + repo-prefix check or refuse symlinks; size cap on
  `fs::read`, `git show`, and the system-prompt override. (CR-006, RSH-002, RSH-003, ARCHE-024)
- Subprocess env: `env_clear()` + allowlist for command extractors; `GIT_TERMINAL_PROMPT=0`
  on git calls. (RSH-001)
- `git show {rev}:./{relative}` so a subdirectory repo root reads the same file as WORKTREE;
  absolutize `RepoRoot` at construction; `cat-file -t` blob check and proper error
  classification. (CR-002, CR-007, CR-008, ARCHE-015)
- Single-pass placeholder substitution so extracted text cannot be re-expanded. (CR-003)
- sc-ai `--check` normalised lexically, not via worktree `canonicalize`; reject `--rev` with
  `--request-json`; `--flag=-value` accepted; `--help` only in command position.
  (CR-004, CR-016, ARCHE-023, ARCHE-018 bug part)
- Remove `PACKAGED-PROMPT-TOKEN-a3` from the production system prompt. (ARCHE-008)
- `.env` parser strips unquoted trailing `# comment`. (ARCHC-015)
- Session store: unique tmp names, cleanup on error, fsync parent, dir mode 0700, documented
  single-writer assumption. (CR-015, RSH-004)
- Log flush failure after a successful run is a warning, not exit 2. (CR-014)
- One `ResolvedApiKey::try_new` rule set; redact once via a method covering every String
  field. (ARCHC-014, RBP-F010, ARCHC-025)
- Session-log flush, `strong_count` race, one logger per log root, GIL released in
  `build_request`. (ARCHP-013, ARCHP-015, ARCHP-016)

**Duplication / dead logic.**
- `impl FromStr for BackendId` + `BackendId::key_id()`; one `validate_question_set` /
  `validate_state`; one `Rev::parse`; delete the three divergent copies and the hyphen
  alias drift. (RBP-F004, ARCHE-010, ARCHP-017, CR-011, ARCHC-022)
- Align Retry-After parsing, busy list and exhausted-retry wording across sc-jev and
  sc-jev-backends (interim until decision D5). (QA-003, ARCHC-013)
- Delete `VendorPin` / `PackagedConfig.vendors` / `[vendors.*]`; parse packaged jev.toml once;
  drop `BuiltPayload.prompt`. (ARCHE-009, RBQA-F004 routine part)
- Remove duplicate prepare, unreachable branches, no-op statements, double validation inside
  judge. (ARCHC-007, ARCHC-012, ARCHE-014)
- Haiku/Luna wrappers via one generic/macro; remove Arc layer. (EXT-008, ARCHC-024)
- One ureq Agent per transport with connect timeout; versioned User-Agent. (RSH-006, ARCHC-020)
- Table-column subject span = cell span; reject multiple producers explicitly. (ARCHE-020)
- Subjects extractor no longer reads process stdin for `-`. (ARCHE-021)
- Dry-run no longer creates `.sc-ai/`, sessions dir or a logger thread. (ARCHE-019)
- `tempfile` everywhere; `exact.rs` moved to `tests/`; consolidate test doubles on the
  existing shared ones. (QA-007, RBP-F015, ARCHE-016, ARCHP-021)
- sc-treesitter: one static grammar table, cfg-gated per-language tests, `cargo hack`
  feature matrix, dead defensive checks removed, `is_missing` counted as parse error,
  `--version`, `#[non_exhaustive]` on published structs, duplicate `.scm` fixture removed.
  (ARCHP-007, ARCHP-008, ARCHP-010, ARCHP-011, ARCHP-012, QA-006)
- sc-jev-py: single `run_check` signature (scripted injection via a test-util-only module),
  `pub(crate)` pyfunctions, cdylib only, `#[expect(.., reason)]`, pathlib accepted,
  `#[pyclass(frozen)]`, `.pyi` + `py.typed`. (EXT-003, ARCHP-018, ARCHP-019, QA-008)
- Public-surface narrowing (`pub(crate)`), private `api`/`client` modules, facade strings
  updated. (ARCHE-017, RBQA-F011, ARCHC-017)
- Hand-built `ConfigInvalid { detail }` literals replaced by the typed constructors;
  `encode_failed` helper so serde causes are not dropped. (RBP-F002, RBP-F015)
- Cross-impl fixture extended with escaping/ordering edge cases. (CR-013)
- rustdoc warnings fixed; `RUSTDOCFLAGS=-D warnings`; workspace `unsafe_code = forbid`;
  clippy `disallowed-methods` for `ResolvedApiKey::as_str`. (QA-005, RBQA-F012, RBQA-F008)
- Transport loopback tests to lift sc-jev / sc-jev-backends / sc-jev-py coverage. (QA-002)
- Pure `default_log_root_from(home, tmp)`; subprocess test helper with deadline; git config
  isolation in tests. (FTQ-002, FTQ-003)

**Docs, boundaries, CI.**
- Consumer-owned CI workflow: `just lint`, `just test`, clippy, doc, coverage, `maturin
  develop` + pytest, sc-treesitter feature matrix, on ubuntu/macos/windows.
  (QA-001, FTQ-001, REQ-QA-003, REQ-QA-004)
- Boundary manifests: list every direct dependency (option B), add forbidden edges for all
  eight crates, populate or remove the empty `[enforcement]` stanzas. (RBQA-F001, RBQA-F002,
  ARCH-003, ARCH-004)
- Sprint docs a-1/a-7/a-8 → `complete`; one owning sprint per REQ root; a-1 validation line
  and module layout corrected; plan.md / project-plan.md phase status; REQ-SAG-0005 in the
  closure list; cite the NFR-SAG-0002 tests; Luna live-count bead carried forward.
  (REQ-QA-001, -002, -005, -006, -007, -008, -009)
- architecture.md: add sc-treesitter to the crate graph, status Active, ADR index table.
  (ARCH-005)
- docs/sc-jev-backends module table; subjects-schema grammar version; helper-capture rule
  spelled one way (`_` prefix, no `@`). (ARCHC-017, ARCHP-012, ARCHP-001 routine part)

## 4. Decisions needed before any architectural change

Each decision lists the reviewers' options and my recommendation. Nothing below has been
started.

**D1. How open is the backend set?** (EXT-001 blocking, ARCHE-005, ARCHC-005, RBP-F004)
`BackendId`, `BackendKeyId` and `JevError` are closed enums with no `#[non_exhaustive]`;
per-backend facts are scattered over five exhaustive matches and string literals, so
ADR-SAG-0009's "registers without caller changes" is false beyond the `run_check` signature.
- Options: (a) `#[non_exhaustive]` + `JudgeBackend::capabilities()` + one `BACKENDS`
  catalog; (b) keep closed, lockstep versioning, amend ADR-SAG-0009 to say so; (c) open
  string newtype with runtime registry.
- Recommendation: **(a)**. One break now (pre-1.0), engine reads capabilities not ids,
  future backends are minor bumps. Do before the first crates.io publish.

**D2. Where does shared consumer logic live?** (EXT-002 blocking, ARCHE-003, ARCHE-011,
ARCHE-012, RBQA-F005, ARCHP-014, RBP-F013, QA-004)
sc-ai and sc-jev-py each own repo discovery, path defaults, backend parsing, the
request/check split, response shaping and the observability adapter; the two already
disagree (git toplevel vs cwd changes which `.env` is read). An MCP server would be a
third copy. The engine has four entry points.
- Options: (a) one two-step engine API (`CheckSource` → `prepare` → `run`) plus a
  `RunResponse` type in sc-jev-check plus `discover_repo_root` in the engine, and a small
  unpublished `sc-agent-compose` crate for the obs adapter and path policy (amends
  ADR-SAG-0016); (b) same engine changes, accept the duplicated adapter until the v2 shim
  is removed; (c) leave consumers as they are and document sc-jev-py as the "raw" surface.
- Recommendation: **(a)**, because MCP is on the Phase B list and would otherwise copy
  `merge_jev_response`. Keep ADR-SAG-0021 path precedence per root.

**D3. Multi-backend semantics.** (ARCHE-002, ARCHE-013, ARCHE-001/CR-009)
Today the second LLM backend receives the first's answers in its system prompt
(ADR-SAG-0008), the run still reports `equal: true`, and if any backend fails mid-run the
caller gets only an error and loses paid-for answers. `leak` is unreachable.
- Options: independent vs chained (or an explicit recorded mode); fail-fast vs per-backend
  `Result`s; equal-input computed from a backend-reported wire digest vs documented as a
  constant.
- Recommendation: **independent by default with an explicit `chain_context` mode recorded
  on the attestation; per-backend results with `leak = any failed`; equal-input from the
  digest each backend reports.** Amend ADR-SAG-0008 and ADR-SAG-0019 accordingly.

**D4. Session store shape.** (EXT-006, ARCHE-004, RBP-F005, RBP-F008)
`SessionStore` is declared open but every caller uses the concrete type and inherent
methods; the store holds an `Arc<RwLock<BackendRegistry>>` that callers must install before
`ask` works; the engine reloads the session from disk per backend.
- Options: (a) stateless `FileSessionStore { dir, ttl, clock }` with `save/load/ask(..,
  &BackendRegistry)`, delete the trait and `install_registry`, write once at end of run and
  only when an LLM backend took part; (b) make the trait the real port with
  `Arc<dyn SessionStore>`; (c) document the ordering requirement.
- Recommendation: **(a)**. Smallest honest shape; a trait can return when a second store
  exists.

**D5. One HTTP layer or two.** (EXT-005, RBQA-F003, RBP-F006, ARCHC-001, QA-003)
sc-jev and sc-jev-backends carry parallel transport traits, request types, retry loops,
caps, redaction and two ADR-SAG-0020 sealing models; they already drifted.
- Options: (a) new `sc-agent-http` crate both depend on; (b) shared transport behind a
  feature in sc-agent-types; (c) align behaviour only and keep two copies.
- Recommendation: **(a)**. Phase B's provider agents need exactly this layer. The interim
  alignment (Retry-After, busy list, wording) is already in the routine track.

**D6. Error type design.** (RBP-F001, ARCHC-004, ARCHE-022, EXT-007, ARCHP-003, RBP-F002)
Recovery categories are magic string prefixes inside `detail` parsed back out by
`fix_hint`; no `source`; producer variants are named after sc-treesitter; no `retryable()`.
- Options: (a) explicit variants (`SessionStoreIo`, `RegistryInvalid`, `Operational`,
  producer-neutral `LanguageUnavailable { producer, language }`) all mapping to the existing
  stable `code()` ids, `#[non_exhaustive]`, `retryable()`; (b) keep variants, add a
  `recovery` field; (c) status quo with private prefixes.
- Recommendation: **(a)**. Wire ids stay stable; Rust consumers stop parsing strings.
  Pairs with D1 (same crate, same break).

**D7. Scope of sc-agent-types.** (RBQA-F009, ARCHC-008, ARCHC-021, ARCHP-001, ARCHP-020)
The types crate bundles errors, ids, secret, judge port, subjects schema, `InputSpec`
(check.toml schema only sc-jev-check parses), `EqualInputRecord`, and the observability v2
port whose only production use is re-exporting a third-party `EventError`; every crate
links the exactly pinned `sc-observability-types` even if it never logs.
- Options: drop the dep via a local `LogRejected` error now; split the crate; add a
  `schema` module; subjects validator + id-assigning builder in types so producer and
  consumer share one implementation; a `publish = false` workspace-tests crate for
  cross-crate proofs (jev_cross_impl, the missing end-to-end producer test).
- Recommendation: **local `LogRejected` now (drops the dep from every crate); keep one types
  crate with a `schema` module; subjects validate+builder in types; create
  `crates/workspace-tests`.** Do not split the crate yet.

**D8. Dependency pins, allocator, second binary → one new ADR.** (ARCH-001 blocking,
ARCH-002, RBP-F001 from a-9, ARCHP-005/006, RBQA-F007)
Library crates carry `=` pins; sc-treesitter's library links mimalloc; sc-treesitter ships
as a second released binary that Homebrew installs but Scoop/WinGet do not; ADR-SAG-0016
says `1.5.0` while the workspace says `=1.5.0`.
- Recommendation: **caret ranges in library crates, exact pins only where lockstep is
  deliberate (sc-observability) and documented; mimalloc behind a default-on `cli` feature
  with `required-features` on the bin; amend ADR-SAG-0017 to name both binaries and the
  per-channel coverage (Scoop single-shim is a kit limitation to file upstream); amend
  ADR-SAG-0016's version text.** All reviewers agreed on the mimalloc gating.

**D9. Jev wire body vs canonical bytes.** (ARCHC-002, ARCHC-003)
sc-jev accepts two question shapes and its own tests use the one production never sends;
sc-jev-backends rejects that shape. The `"model"` byte-splice breaks the moment a payload
key sorts before `model` (`context`, `inputs`, `metadata`).
- Recommendation: **confirm with the TypeSafe API which shape it accepts; make
  `CanonicalJudgePayload` the only schema both backends validate by deserializing; build the
  Jev wire body as a JSON envelope around the canonical object (digest stays on canonical
  bytes) and amend ADR-SAG-0005.** This is the one item I would not start without the
  vendor answer.

**D10. JudgeBackend trait shape and outcome typing.** (ARCHC-006, ARCHC-011, EXT-011,
RBQA-F010)
Blocking trait; validate and judge both re-parse the raw request (~4 passes per backend);
`JudgeOutcome.probabilities` duplicates `answers`; `answers`/`provider_meta` are untyped.
- Recommendation: **keep the sync trait and declare it blocking; drop the redundant
  re-validation (routine); defer prepare/send split and typed `JudgeMeta` to the first
  Phase B agent that needs them.** Removing `probabilities` touches a frozen contract, so it
  rides with D6's break if you take it.

**D11. The command-producer contract.** (CR-001, ARCHE-006, ARCHP-002)
`{path}` cannot be set from check.toml and producer stderr errors are discarded, so the
only proven producer integration is a committed `subjects.json`.
- Recommendation: **add `path: Option<String>` to `InputSpec::Command`, have the command
  extractor parse a single `ScAgentErrorResponse` stderr line and re-raise its code, and
  add a workspace end-to-end test running the real sc-treesitter binary through
  check.toml.**

**D12. CLI argument parsing.** (ARCHE-018, EXT-010, RBP-F012, ARCHP-011)
Two hand-rolled parsers (~370 lines) with three parallel flag tables each.
- Recommendation: **adopt clap derive in both binaries.** The edge-case bugs are being fixed
  now regardless.

**D13. Small ADR wording amendments.** ADR-SAG-0005 narrowed to "runs the four consumer
extractor kinds" (ARCH-006); remove the decorative sealed `Extractor` trait and amend the
RBP-F004 note (ARCHE-007, RBP-F009, EXT-004); single owner of the request byte cap
(RBQA-F004: today 200 000 in sc-jev-check vs 96 000 in both backends, so a payload between
them passes pre-flight and is refused downstream). Recommendation: accept all three.

## 5. What reviewers said is solid

Keep as is: secret resolution and redaction (every keyring error variant tested for
leaks); `JevError::code()` pinned to `errors.json` with uniqueness; canonical encoding
robust to `preserve_order`; validation before any send in both backend families; argv-only
subprocess control with caps, timeouts and bounded reap; atomic session writes with digest
re-verification; prompt/payload separation enforced in one encoder; subjects contract typed
in sc-agent-types with golden-byte fixtures consumed by producer, engine and Python;
sc-treesitter library does no I/O and links no judge crate; test-util doubles provably
absent from release builds; sc-ai has no HTTP dependency and one envelope per stream.

## 6. Suggested order

1. Routine tracks land (in progress) → QA-1/QA-2 on the two a-10 PRs → merge to integrate.
2. You decide D1–D13 (D8, D9, D11 have the highest blast radius if deferred).
3. One architectural sprint per cluster: {D1, D6, D10-break} (types crate break);
   {D2, D4, D11} (engine shape); {D5, D7} (crate layout); {D3} (semantics + ADR-0008/0019);
   {D8, D13} (ADRs + manifests); {D9} after the vendor answer; {D12} any time.
4. Re-run the phase-ending gate; then merge integrate → develop.

## 7. Decisions (accepted 2026-11-06)

Owner accepted every recommendation in section 4 (D1–D13) as written. Sequencing: the two
a-10 routine tracks land first; then one planning pass writes the ADR amendments and sprint
docs per cluster and goes through plan review; then implementation sprints with the normal
QA loop. D9 stays blocked until the TypeSafe API's accepted payload shape (array `id`/`choices`
vs object `criteria`) is confirmed.

**Owner confirmation (2026-11-06):** `sc-agent-compose` IS published to crates.io as an
internal-support crate, `=`-pinned from `sc-ai` and `sc-jev-py`, with no semver promise of its
own. This supersedes the "unpublished" wording in D2 (a published `sc-ai` cannot depend on an
unpublished crate).

**Owner confirmation (2026-11-06, retry policy):** when `Retry-After` is absent or malformed,
back off exponentially from ~1 s (capped at 30 s) with jitter; honour a valid header clamped to
0–30 s; 4 sends; 90 s budget for starting new attempts plus the in-flight 120 s request timeout.
No network-connectivity detection or indefinite wait: connect failures surface as
`NETWORK_ERROR` with `retryable() == true` and the caller decides.
