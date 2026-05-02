---
title: Components — Option 2
date: 2026-05-01
target_release: 0.6.0
desc: Minimum component set viewed through the public-surface lens
status: living
---

# Components

For each in-scope component this file lists its purpose, the public
surfaces it feeds or owns, the public contracts it consumes, the
internal interfaces it uses to support public behavior, and the
non-ownership boundaries that protect the public-surface contract.

The minimum component set is `engine`, `lifecycle`, `bindings`,
`recovery`, `migrations`, Python SDK, TypeScript SDK, plus the public
surfaces themselves (Rust API, Python API, TypeScript API, CLI,
subscriber/observability, machine-readable output) — those public
surfaces are enumerated separately in `public-surfaces.md`. This file
covers the subsystem-side of the relationship.

## C-1: engine

- **Purpose.** Owns runtime open/close, writer/reader split, batch
  submission semantics, cursor contract, and `EngineConfig` knob
  set ownership (`design/engine.md`).
- **Public surfaces fed or owned.**
  - Rust API (S-1): `Engine.open`, `Engine.write`, `Engine.search`,
    `Engine.close`; `WriteReceipt { cursor: c_w, ... }`;
    `projection_cursor` on read tx; `EngineConfig` shape.
  - Machine-readable output (S-6): `EngineOpenError` corruption
    surface (joint with `errors`); cursor values.
- **Public contracts consumed.**
  - `PreparedWrite` enum from ADR-0.6.0-prepared-write-shape (typed
    write boundary).
  - Embedder identity invariants from
    ADR-0.6.0-vector-identity-embedder-owned.
  - JSON-Schema validation cadence from ADR-0.6.0-json-schema-policy
    (op-store payload checked save-time, pre-commit).
- **Internal interfaces used.** Writer thread mpsc; reader connection
  pool; embedder dispatch pool; scheduler post-commit dispatch
  (architecture.md § 3 + § 6).
- **Non-ownership boundaries.**
  - Does not own migration step payload schema (`design/migrations.md`).
  - Does not own per-binding signatures (`interfaces/{rust,python,ts}.md`).
  - Does not own CLI verb table (`interfaces/cli.md` /
    `design/recovery.md`).
  - Does not own counter / profile / phase shapes
    (`design/lifecycle.md`).
  - Does not own variant→class mapping
    (`design/errors.md` + `design/bindings.md`).

## C-2: lifecycle

- **Purpose.** Owns the response-cycle phase enum, slow / heartbeat
  semantics, host-subscriber routing, counter snapshot shape, profile
  record shape, and stress-failure context schema
  (`design/lifecycle.md`).
- **Public surfaces fed or owned.**
  - Subscriber / observability (S-5): every typed payload listed
    there originates here except migration step events
    (`design/migrations.md`) and CLI machine-readable verb output
    (`design/recovery.md`).
  - Machine-readable output (S-6): phase enum, source/category enums,
    counter snapshot key set, profile record fields, stress-failure
    fields.
- **Public contracts consumed.**
  - Subscriber-attachment protocol from `design/bindings.md` § 8
    (cross-language transport).
  - Engine event payload routing through host subscriber (binding
    adapter responsibility).
- **Internal interfaces used.** Engine emission sites; SQLite-internal
  event capture (re-routed through host subscriber, never to a private
  sink).
- **Non-ownership boundaries.**
  - Does not own `Engine.open` / `Engine.close` lifetime
    (`design/engine.md`).
  - Does not own migration step payload (`design/migrations.md`).
  - Does not own subscriber registration call signature per binding
    (`design/bindings.md` + `interfaces/{python,ts}.md`).
  - Does not own CLI verb-owned JSON shape (`design/recovery.md`).
  - Does not own diagnostic event message text (anti-requirement: not
    a public contract).

## C-3: bindings

- **Purpose.** Cross-language binding strategy: surface-set parity,
  error-mapping protocol, async dispatch model, marshalling protocol,
  embedder identity invariant, lock contract uniformity, logging-
  subscriber protocol, build/packaging strategy, recovery-surface
  unreachability (`design/bindings.md`).
- **Public surfaces fed or owned.**
  - Python API (S-2) — protocol-level only; symbol-level owned by
    `interfaces/python.md` (stub).
  - TypeScript API (S-3) — protocol-level only; symbol-level owned by
    `interfaces/typescript.md` (stub).
  - Machine-readable output (S-6): error class hierarchy invariants,
    typed-attribute contract, vector input encoding boundary.
  - Subscriber / observability (S-5): per-binding subscriber-attachment
    protocol; engine-event payload wire-stability across bindings.
- **Public contracts consumed.**
  - `EngineError` / `EngineOpenError` variant set from
    `design/errors.md`.
  - `PreparedWrite` shape from
    ADR-0.6.0-prepared-write-shape.
  - Engine-config knob symmetry rule (`design/engine.md`'s knob set is
    the input).
  - Lock mechanism contract from
    ADR-0.6.0-database-lock-mechanism.
- **Internal interfaces used.** PyO3 boundary (Python); napi-rs +
  ThreadsafeFunction (TS); CLI argv parser; engine `&[PreparedWrite]`
  mpsc submission.
- **Non-ownership boundaries.**
  - Does not own per-language symbol names or kwargs spelling
    (`interfaces/{python,ts}.md`).
  - Does not own variant→class mapping matrix (`design/errors.md`).
  - Does not own CI smoke gate (`design/release.md`).
  - Does not own developer ergonomics (e.g. "do not manually `cargo
    build && cp`"; `design/bindings.md` § 9).
  - Does not commit any new error class, dispatch model, or
    `BindingsConfig` struct (`design/bindings.md` § 14).

## C-4: recovery

- **Purpose.** Owns the operator surface for corruption inspection,
  export, and recovery; canonical CLI verb table; check-integrity JSON
  schema; recovery-hint anchor → workflow mapping
  (`design/recovery.md`).
- **Public surfaces fed or owned.**
  - CLI (S-4): every recover sub-flag and doctor verb.
  - Machine-readable output (S-6): `doctor check-integrity` JSON
    object (`physical`, `logical`, `semantic` keys); finding records
    with `code`, `stage`, `locator`, `doc_anchor`, `detail`;
    `recover` progress stream + summary.
- **Public contracts consumed.**
  - `EngineOpenError::Corruption(CorruptionDetail)` payload from
    `design/errors.md` (recovery-hint codes route to recovery
    workflow anchors).
  - SDK non-presence claim from `design/bindings.md` § 10
    (recovery is unreachable from runtime SDK).
- **Internal interfaces used.** SQLite physical recovery primitives;
  `safe_export` SHA-256 manifest writer; canonical row replay for
  projection rebuild (architecture.md § 2).
- **Non-ownership boundaries.**
  - Does not own corruption variant table (`design/errors.md`).
  - Does not own CLI flag spelling or exit-code classes (those are
    `interfaces/cli.md`).
  - Does not provide an SDK surface; runtime SDK contains no
    recovery verb (REQ-037 / REQ-054 / AC-035d / AC-041).
  - Does not own migration execution (`design/migrations.md`); doctor
    verbs do not run migrations.

## C-5: migrations

- **Purpose.** Owns the migration loop that runs during `Engine.open`,
  per-step event contract, and accretion-guard rules cited by REQ-042 /
  REQ-045 (`design/migrations.md`).
- **Public surfaces fed or owned.**
  - Subscriber / observability (S-5): per-step migration event with
    `step_id`, `duration_ms`, and `failed: true` on failure
    (AC-046b/c).
  - Machine-readable output (S-6): structured migration event payload;
    `MigrationFailed` typed exception.
- **Public contracts consumed.**
  - Lifecycle phase routing (`design/lifecycle.md`).
  - `Engine.open` step ordering (`design/engine.md` § Open path).
  - Accretion-guard linter (`design/release.md` per architecture.md;
    AC-049).
- **Internal interfaces used.** `fathomdb-schema` migration definitions;
  SQLite `PRAGMA user_version`; structured tracing emission to host
  subscriber.
- **Non-ownership boundaries.**
  - Does not own subscriber routing transport (`design/bindings.md`
    § 8 + `design/lifecycle.md`).
  - Does not own corruption-on-open detection (`design/engine.md` +
    `design/errors.md`).
  - Does not appear as a doctor verb (`design/recovery.md`).

## C-6: Python SDK (binding)

- **Purpose.** PyO3 cdylib package `fathomdb` under `src/python/`; binds the
  five-verb SDK surface in idiomatic Python (sync, snake_case)
  (`design/bindings.md` § 1 + § 2; architecture.md § 1).
- **Public surfaces fed or owned.** Python API (S-2). Subscriber
  attachment for Python (`design/bindings.md` § 8).
- **Public contracts consumed.**
  - Cross-binding parity, error-mapping protocol, marshalling
    protocol, lock contract, embedder identity invariant from
    `design/bindings.md`.
  - `EngineError` / `EngineOpenError` variant set from
    `design/errors.md`.
  - Build path: `pip install -e src/python/` (memory
    `feedback_python_native_build`; `design/bindings.md` § 9).
- **Internal interfaces used.** PyO3 type marshalling;
  `Python::allow_threads` to release GIL on engine calls; NumPy
  `ndarray[float32]` zerocopy cast for vector input
  (`design/bindings.md` § 4).
- **Non-ownership boundaries.**
  - Does not own engine semantics (`design/engine.md`).
  - Does not pre-validate JSON Schema (single source of truth is
    engine; `design/bindings.md` § 4).
  - Does not auto-rebuild on identity mismatch
    (`design/bindings.md` § 5).
  - Does not maintain in-process registry of held DB paths
    (`design/bindings.md` § 7).

## C-7: TypeScript SDK (binding)

- **Purpose.** napi-rs cdylib package `fathomdb` under `src/ts/`; binds the
  five-verb SDK surface in idiomatic TS (Promise, camelCase)
  (`design/bindings.md` § 1 + § 2; architecture.md § 1).
- **Public surfaces fed or owned.** TypeScript API (S-3). Subscriber
  attachment for TypeScript (`design/bindings.md` § 8).
- **Public contracts consumed.** Same set as Python SDK plus the
  napi-rs `ThreadsafeFunction` thread-pool sizing decision (Path 2;
  ADR-0.6.0-async-surface).
- **Internal interfaces used.** napi-rs `ThreadsafeFunction`;
  Rust-owned thread pool sized `num_cpus::get()`; `Float32Array`
  zerocopy cast for vector input (`design/bindings.md` § 4).
- **Non-ownership boundaries.** Same shape as Python SDK plus:
  - TS pool-sizing knob is a binding-runtime mechanic, not a
    canonical engine-config knob (`design/bindings.md` § 6) — does
    not create a Python-parity obligation.
  - Does not promise `EngineError` is a single root in TS
    (`design/bindings.md` § 3 leaves "TS may keep two roots" open).
