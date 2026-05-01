---
title: Public Surfaces — Option 2
date: 2026-05-01
target_release: 0.6.0
desc: Enumerated public-facing surfaces (Rust, Python, TypeScript, CLI, observability, machine-readable)
status: living
---

# Public Surfaces

This file enumerates the outward-facing surfaces first, before any
subsystem decomposition. Each entry lists the audience, the entry points
or verbs visible at that surface, the machine-readable contracts the
surface exposes, the canonical owning doc(s), the related core
components, and the inconsistencies / missing precision visible at the
surface boundary today.

## S-1: Rust public API

- **Primary audience.** Rust embedders consuming the `fathomdb` facade
  crate; integrators building higher-level adapters.
- **Entry points / verbs.** Five-verb SDK surface (idiomatic Rust
  spelling): `Engine::open`, `admin::configure`, `write`, `search`,
  `close` (per REQ-053; `design/bindings.md` § 1). `Engine::write`
  accepts `&[PreparedWrite]` per ADR-0.6.0-prepared-write-shape
  (`design/engine.md` § Batch submission semantics; `design/bindings.md`
  § 4). Engine-config knobs reachable through `EngineConfig` (named
  set: `embedder_pool_size`, `scheduler_runtime_threads`;
  `design/engine.md` § EngineConfig ownership; non-exhaustive in the
  corpus).
- **Machine-readable contracts exposed.**
  - `WriteReceipt { cursor: c_w, ... }` returned on write commit
    (architecture.md § 3 step 2); cursor field shape per REQ-055.
  - `Search { results, projection_cursor, soft_fallback }` returned by
    read path (architecture.md § 4 step 5; REQ-029 / AC-031).
  - Error roots `EngineError` and `EngineOpenError` with structured
    `CorruptionDetail` for the corruption-on-open path
    (`design/errors.md` § Top-level types + § Corruption detail owner).
  - `ProjectionStatus` enum `{Pending, Failed, UpToDate}` (AC-010).
- **Canonical owning docs.** `interfaces/rust.md` (status: `not-started`).
  Cross-doc owners while interface stub is empty: `design/engine.md`,
  `design/bindings.md`, `design/errors.md`.
- **Related core components.** runtime / writer / reader / errors /
  bindings facade.
- **Notable inconsistencies / missing precision.** `interfaces/rust.md`
  is a stub: no signatures, no error cases per symbol, no stability
  posture. `EngineConfig` field set incomplete in `design/engine.md`
  ("includes runtime controls such as ..."). The exact field set of
  `AdminSchemaWrite` is owned in `design/engine.md` and "interface docs"
  but neither location has it (`design/engine.md` § PreparedWrite::AdminSchema
  provenance: "still owned here and by the interface docs").

## S-2: Python public API

- **Primary audience.** Python application authors consuming the
  `fathomdb` PyO3 cdylib package.
- **Entry points / verbs.** Five-verb SDK surface in idiomatic Python
  casing, mirroring Rust shape; sync surface per
  ADR-0.6.0-python-api-shape (Path 1) (`design/bindings.md` § 2). No
  recovery / repair / restore / fix / rebuild / doctor names allowed
  (`design/bindings.md` § 1; AC-041; AC-057a).
- **Machine-readable contracts exposed.** Idiomatic Python materializations
  of Rust surface: search results as `list[dict]` / dataclass; op-store
  payloads as Python dict; vector input as `numpy.ndarray[float32]` or
  `list[float]` (`design/bindings.md` § 4).
- **Canonical owning docs.** `interfaces/python.md` (status:
  `not-started`). Cross-doc owners: `design/bindings.md`,
  `design/errors.md`.
- **Related core components.** bindings facade (Python adapter); runtime
  (engine boundary).
- **Notable inconsistencies / missing precision.** `interfaces/python.md`
  is a stub: no concrete class names, no exception hierarchy diagram, no
  `Embedder` protocol shape. The Python exception class for every
  variant in the error taxonomy is named-but-not-spelled in the corpus.
  Subscriber-registration call signature is named-but-not-spelled.
  Drain verb name is named-but-not-spelled (REQ-030 cross-cite to
  binding-interface ADRs; ADR-0.6.0-async-surface). Profile-record
  transport API call is named-but-not-spelled (AC-005a: "documented API
  call enables per-statement profiling").

## S-3: TypeScript public API

- **Primary audience.** TypeScript / Node.js application authors
  consuming the `fathomdb` napi-rs cdylib package.
- **Entry points / verbs.** Five-verb SDK surface in idiomatic TS
  casing (camelCase); Promise surface per ADR-0.6.0-typescript-api-shape
  (Path 2; `design/bindings.md` § 2). napi-rs `ThreadsafeFunction`
  pool sized at `num_cpus::get()`; pool-sizing knob surfaceable but
  not part of the canonical engine-config set
  (`design/bindings.md` § 6).
- **Machine-readable contracts exposed.** Promise-wrapped equivalents
  of the Rust shape; vector input as `Float32Array` (zerocopy per
  ADR-0.6.0-zerocopy-blob); op-store payload as plain object
  (`design/bindings.md` § 4).
- **Canonical owning docs.** `interfaces/typescript.md` (status:
  `not-started`). Cross-doc owners: `design/bindings.md`,
  `design/errors.md`.
- **Related core components.** bindings facade (TS adapter); napi-rs
  Rust thread pool (architecture.md § 6).
- **Notable inconsistencies / missing precision.** `interfaces/typescript.md`
  is a stub: no concrete class names, no `EngineError` /
  per-variant `instanceof` class names, no subscriber-callback
  signature, no `Embedder` interface shape. The async invariants A–D
  manifest "differently per language" but the TS-side manifestation is
  not enumerated. Whether `EngineError` has one or two roots in TS is
  noted as binding choice ("TS may keep two roots";
  `design/bindings.md` § 3) but not resolved.

## S-4: CLI (operator binary)

- **Primary audience.** Operators; humans + CI scripts; not application
  callers.
- **Entry points / verbs.**
  - `fathomdb recover --accept-data-loss [--truncate-wal]
    [--rebuild-vec0] [--rebuild-projections] [--excise-source <id>]
    [--purge-logical-id <id>] [--restore-logical-id <id>]`
    (`interfaces/cli.md` § Recover root; `design/recovery.md` § Two-root
    CLI split; AC-058).
  - `fathomdb doctor <verb>` where verb ∈
    `{check-integrity, safe-export, verify-embedder, trace,
    dump-schema, dump-row-counts, dump-profile}`
    (`interfaces/cli.md` § Doctor verbs; AC-040a/b).
  - Doctor-only invocation flags: `--quick`, `--full`, `--round-trip`,
    `--pretty` (`design/recovery.md` § Doctor-only flags).
- **Machine-readable contracts exposed.**
  - `--json` is normative on every verb (`interfaces/cli.md` § Output
    posture; `design/recovery.md` § Machine-readable output).
  - `doctor check-integrity --json`: single JSON object with top-level
    keys `physical`, `logical`, `semantic` (AC-043a); per-finding fields
    `code`, `stage`, `locator`, `doc_anchor`, `detail` (AC-043c;
    `design/recovery.md` § check-integrity schema owner).
  - `doctor check-integrity --full` may emit doctor-only finding code
    `E_CORRUPT_INTEGRITY_CHECK` (`interfaces/cli.md`; `design/errors.md`
    § Doctor-only finding codes).
  - `recover --json`: progress stream + terminal summary
    (`interfaces/cli.md`; `design/recovery.md` § Machine-readable
    output). Other `doctor dump-*`, `safe-export`, `verify-embedder`,
    `trace` JSON shapes "remain owned here as the draft fills in"
    (`design/recovery.md` § Machine-readable output).
  - Exit-code classes: `doctor-check-*` = 0 / 65 / 70 / 71;
    `doctor-export-*` = 0 / 66 / 71; `recover-*` = 0 / 64 / 70 / 71
    (`interfaces/cli.md` § Doctor verbs + § Recover root).
- **Canonical owning docs.** `interfaces/cli.md` (status: `draft`;
  owns flag spelling, root paths, exit-code classes); `design/recovery.md`
  (canonical verb table, JSON shapes, recovery-hint anchors).
- **Related core components.** recovery; bindings facade (CLI);
  release.
- **Notable inconsistencies / missing precision.** Doctor JSON shapes
  beyond `check-integrity` are explicitly TBD. `--rebuild-projections`
  is the canonical 0.6.0 "regenerate" workflow but the corpus uses
  "regenerate" as a workflow name across `interfaces/cli.md` § Recover
  root note, `design/recovery.md` § Two-root CLI split, and REQ-059;
  there is no separate `fathomdb regenerate` command — risk of operator
  confusion. CLI does not mirror the SDK five-verb surface
  (`design/bindings.md` § 1) — boundary is explicit but invites
  operator expectation drift.

## S-5: Subscriber / observability surface

- **Primary audience.** Operators; SREs; debugging / triage; capacity
  planners.
- **Entry points / verbs.**
  - Subscriber registration via host-language idiomatic helper:
    Python: `logging`-backed adapter helper; TypeScript: per-event
    callback; CLI: console subscriber (human mode) or JSON emission per
    verb (machine mode) (`design/bindings.md` § 8).
  - Counter-snapshot pull surface (`design/lifecycle.md` § Counter
    snapshot; AC-004a/b/c).
  - Per-statement profile-record surface, runtime-toggleable
    (`design/lifecycle.md` § Per-statement profiling; AC-005a/b).
- **Machine-readable contracts exposed.**
  - Lifecycle phase enum: `{Started, Slow, Heartbeat, Finished, Failed}`
    (`design/lifecycle.md` § Phase enum; AC-001 + AC-008).
  - Diagnostic source/category: `source ∈ {Engine, SqliteInternal}`;
    engine categories `{writer, search, admin, error}`; SQLite-internal
    categories `{corruption, recovery, io}`
    (`design/lifecycle.md` § Host-routed diagnostics; AC-003a/b/c/d;
    AC-006).
  - Counter snapshot keys: `queries`, `writes`, `write_rows`,
    `errors_by_code`, `admin_ops`, `cache_hit`, `cache_miss`
    (`design/lifecycle.md` § Public key set; AC-004a).
  - Profile record fields: `wall_clock_ms`, `step_count`, `cache_delta`
    (`design/lifecycle.md` § Public record shape; AC-005b).
  - Stress-failure context fields: `thread_group_id`, `op_kind`,
    `last_error_chain`, `projection_state` (`design/lifecycle.md`
    § Stress-failure context; AC-009).
  - Migration per-step event fields: `step_id`, `duration_ms`, plus
    `failed: true` on failure (AC-046b/c; `design/migrations.md` —
    payload shape ownership delegated; file is a one-paragraph stub).
  - Engine event payload "appears under a stable `fathomdb` payload key
    in the host record" (`design/bindings.md` § 8) — wire-stable across
    bindings.
- **Canonical owning docs.** `design/lifecycle.md` (phase, counters,
  profiling, stress context, diagnostic routing); `design/bindings.md`
  § 8 (subscriber attachment protocol per binding); `design/migrations.md`
  (migration step payload — currently stub); `design/errors.md`
  (error-event variant routing).
- **Related core components.** lifecycle; bindings facade; migrations;
  errors.
- **Notable inconsistencies / missing precision.** No interface file
  spells the per-binding subscriber-registration call signature.
  `design/migrations.md` is a one-paragraph stub: AC-046b/c assert
  fields (`step_id`, `duration_ms`, `failed`) but the payload
  schema's authoritative definition does not exist yet. The
  "non-phase envelope for `open`, `write`, `search`, admin, recovery,
  or migration-originated events remains owned by the producing
  surface or binding contract" (`design/lifecycle.md` § Public event
  contract) — multiple owners, none yet enumerated.

## S-6: Machine-readable output surface (cross-cutting)

This surface aggregates the typed payloads that callers and operators
can deserialize without parsing prose. Its members are produced by
several other surfaces but share a coherent "machine-readable contract"
audience.

- **Primary audience.** CI scripts; operator tooling; bindings test
  suites; client libraries dispatching on typed shapes.
- **Entry points / verbs.** N/A; this is a payload-class enumeration.
- **Machine-readable contracts exposed.**
  - CLI `--json` per verb (S-4).
  - `EngineOpenError::Corruption(CorruptionDetail)` with
    `kind: CorruptionKind`, `stage: OpenStage`, `locator:
    CorruptionLocator`, `recovery_hint: RecoveryHint { code,
    doc_anchor }` (`design/errors.md` § Engine.open corruption table;
    AC-035b).
  - `RecoveryHint.code` stable string set: `E_CORRUPT_WAL_REPLAY`,
    `E_CORRUPT_HEADER`, `E_CORRUPT_SCHEMA`,
    `E_CORRUPT_EMBEDDER_IDENTITY` (open-path); doctor-only
    `E_CORRUPT_INTEGRITY_CHECK` (`design/errors.md`).
  - Counter snapshot, profile record, stress-failure context (S-5).
  - Lifecycle phase enum + diagnostic source/category enums (S-5).
  - `WriteReceipt { cursor: c_w, ... }` (architecture.md § 3 step 2;
    REQ-055 / AC-059b).
  - `Search { ..., projection_cursor: c_r, soft_fallback: Option<...> }`
    (architecture.md § 4 step 5; REQ-029 / AC-031).
  - `ProjectionStatus` enum (AC-010).
  - Op-store collection metadata schema: `name`, `kind`, `schema_json`,
    `retention_json`, `format_version`, `created_at` (AC-062).
- **Canonical owning docs.** Distributed: `design/errors.md`
  (corruption + variant routing); `design/lifecycle.md` (counters /
  profiles / stress / phase / category); `design/recovery.md` (CLI JSON
  shapes); `interfaces/cli.md` (CLI flag spelling); `design/op-store.md`
  (collection metadata — file not in this option's read set, status
  unknown but `acceptance.md` AC-062 fixes the columns).
- **Related core components.** errors; lifecycle; recovery; op_store;
  projection.
- **Notable inconsistencies / missing precision.** `interfaces/wire.md`
  is a `not-started` stub. The wire-stable `fathomdb` payload key
  (`design/bindings.md` § 8) is named in protocol but not enumerated
  per event family. `WriteReceipt` field set beyond `cursor` is
  notational ("...") in the architecture diagram. The four `OpenStage`
  values include only the corruption-emitting subset; `LockAcquisition`
  is explicitly NOT a corruption stage (AC-035b) — but no doc lists
  the full `OpenStage` enum, leaving the enum's complete shape implicit.
