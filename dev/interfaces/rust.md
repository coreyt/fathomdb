---
title: Rust Public Interface
date: 2026-05-12
target_release: 0.6.0
desc: Public Rust surface (traits, functions, types, errors) for 0.6.0
blast_radius: src/rust/crates/fathomdb; design/engine.md; design/bindings.md; design/errors.md; design/lifecycle.md
status: locked
---

# Rust Interface

This file owns Rust-visible symbol spelling and result shape. Cross-binding
parity rules remain owned by `design/bindings.md`.

## Support posture

The Rust facade is stable public Rust contract in 0.6.0 and is the
ground-truth source for engine-side type names. The Python/TypeScript governed
SDK surface parity set is tested by AC-074 (which supersedes AC-057a's
five-verb cap). Under the signed Q5 = BIND-RUST
(`ADR-0.8.0-supersede-five-verb-surface-cap`) the Rust facade is **also** bound
by AC-074; its positive-allowlist/parity pin **landed at reserved-gap Slice 27**
(see § Governed-surface contract below). Rust keeps the facade shape below
unless a successor ADR expands it.

## Governed-surface contract (AC-074, Q5 = BIND-RUST — landed Slice 27; method-level + feature-gated by Slice 27 fix-1)

This file **owns** the governed Rust-facade surface. The `fathomdb` facade is a
**different consumer contract** than the Python/TypeScript 5-verb + `read.*`
SDK: the Rust application verbs are methods on `Engine` (`open`/`write`/
`search`/`close`), and the facade's public surface is a set of re-exported
*types*, not free verbs. So the Rust allowlist is **not** the Py/TS verb set; it
is the **typed governed application surface** this file owns. Three load-bearing
properties hold (asserted by `src/rust/crates/fathomdb/tests/governed_surface.rs`,
which binds AC-074 — not a new AC id):

- **P1 — positive allowlist (`GOVERNED_SURFACE_ALLOWLIST`, 29 types):** the
  facade re-exports exactly the curated governed application surface — the
  original 17: `Engine`, `OpenedEngine`, `OpenReport`, `WriteReceipt`,
  `SearchResult`, `PreparedWrite`, `EngineError`, `EngineOpenError`, the open-path
  diagnostics (`CorruptionDetail`, `CorruptionKind`, `CorruptionLocator`,
  `OpenStage`, `RecoveryHint`), the retrieval soft-fallback shapes (`SoftFallback`,
  `SoftFallbackBranch`), and the instrumentation handles (`CounterSnapshot`,
  `Subscription`) — plus the additive groups: Slice 20 (G5/G6) graph-traversal
  types (`TraversalDirection`, `NodeRecord`, `SearchExpandResult`, `SearchFilter`),
  Slice 35 (G4) filter-grammar types (`Predicate`, `ScalarValue`, `ComparisonOp`),
  Slice 15 (G11) BYO-LLM ingest types (`ExtractDocument`,
  `IngestWithExtractorReceipt`), and 0.8.8 Slice 5 (EXP-OBS) explain-sidecar types
  (`Explanation`, `QueryTrace`, `PerHitExplain`). Each resolves through the facade
  at compile time (`type_name::<…>()`). The recovery /
  integrity / dump operator-seam report types in § "Recovery / operator seam
  re-exports" are deliberately **excluded** from this allowlist — they are
  CLI-only ergonomic symbols (the Rust analogue of "recovery is CLI-only, not an
  SDK verb"), not governed application surface.

- **P2 — parity-in-intent (NOT membership-identity):** the Rust governed surface
  is posture-consistent with the Py/TS governed surface (a governed allowlist,
  recovery-denylist-absent, typed / no-raw-SQL) but is a different consumer
  contract — a type set, not a verb set — so it is **not** asserted
  membership-equal to the Py/TS verb allowlist. The one genuinely shared element
  is the recovery denylist, declared once in
  `src/conformance/governed-surface-allowlist.json` (`recovery_denylist`); the
  Rust test pins the same five names.

- **P3 — recovery-denylist absence:** no governed-surface symbol *is* a recovery
  verb in `{recover, restore, repair, fix, rebuild}` (exact, case-insensitive —
  not substring, so the typed `RecoveryHint` hint is correctly not flagged). The
  **canonical** denylist enforcement remains the **byte-frozen**
  `tests/no_recovery_surface.rs`; `governed_surface.rs` adds the *positive*
  allowlist half + an allowlist-scope denylist check.

Rust has no runtime symbol-table introspection (no `dir(module)`), so — exactly
like `no_recovery_surface.rs` / `reexports.rs` — the type-level pin is a
compile-time resolves-check plus this source-inspection-documented contract. See
`dev/design/slice-27-rust-allowlist-design.md`.

### Method-level boundary: default surface vs the `operator` feature (Slice 27 fix-1)

The Slice 27 type-only audit missed that the facade re-exports the engine's
`Engine` **wholesale**, so its inherent **methods** — including
`rebuild_projections`/`rebuild_vec0` (recovery-denylist names) and the
debug-only raw-SQL `execute_for_test` — were reachable. Per the signed Option B
(codex [P1], HITL 2026-06-05) the **operator/recovery seam is feature-gated**:

- **Default `fathomdb` facade (operator OFF)** — the governed runtime surface:
  the 29 governed types + the application methods `Engine::open`/`write`/`search`/
  `search_explained`/`close` (+ the engine-attached instrumentation/control methods). It exposes
  **no method whose name is in `{recover, restore, repair, fix, rebuild}`** and
  **no raw-SQL method**. This is enforced at the **method** level by
  `compile_fail` doctests in `fathomdb/src/lib.rs`
  (`governed_surface_method_absence_proof`, default build;
  `release_surface_raw_sql_absence_proof`, release build) — the only mechanism
  that can assert a method does *not* resolve.
- **`operator` feature (ON — `fathomdb-cli` enables it)** — un-gates the 12
  operator/recovery methods (`rebuild_*`, `excise_source`, `dump_*`,
  `trace_source_ref`, `truncate_wal`, `verify_embedder`, `check_integrity`,
  `safe_export`, `recompute_mean`) + the 20 operator-seam re-exports below. The
  CLI (`fathomdb recover`/`doctor`) is the operator substrate. **Gating, not
  deletion**: engine behavior is byte-identical with the feature on.

So the recovery-denylist + no-raw-SQL guarantees hold at the **method** level on
the default governed surface, while the CLI retains the seam via the feature.
See `dev/design/slice-27-fix1-operator-gate-design.md`.

## Public surface

Rust exposes:

- `Engine::open(...) -> Result<OpenedEngine, EngineOpenError>`
- `Engine::write(...) -> Result<WriteReceipt, EngineError>`
- `Engine::search(...) -> Result<SearchResult, EngineError>`
- `Engine::search_explained(...) -> Result<SearchResult, EngineError>` — 0.8.8
  EXP-OBS: same retrieval as `search_reranked`, additionally returning the opt-in
  `Explanation` sidecar (`SearchResult.explanation`); default paths are unchanged.
- `Engine::close(...) -> Result<(), EngineError>`

`OpenedEngine` contains:

- `engine`
- `report`

`report` is the `OpenReport` owned by `design/engine.md`.

## Engine-attached instrumentation / control methods

These are public instance methods, not extra top-level SDK verbs:

- `Engine::drain(timeout_ms: u64) -> Result<(), EngineError>`
- `Engine::counters() -> CounterSnapshot`
- `Engine::set_profiling(enabled: bool) -> Result<(), EngineError>`
- `Engine::set_slow_threshold_ms(value: u64) -> Result<(), EngineError>`
- `Engine::subscribe(&self, subscriber: Arc<dyn lifecycle::Subscriber>) -> Subscription`

`drain` is a bounded completion surface for post-commit projection work. It
returns `Ok(())` when the engine-owned background projection queue reaches a
quiescent state before `timeout_ms`, and returns a typed runtime error when the
timeout elapses first.

`subscribe` owns host-subscriber attachment and may carry heartbeat-cadence
options. The payload semantics remain owned by `design/lifecycle.md` and
`design/migrations.md`.

## Companion embedder contract

The Rust workspace also exposes the semver-stable companion crate
`fathomdb-embedder-api` for engine-owned embedder dispatch:

- `Embedder`
- `EmbedderIdentity { name, revision, dimension }`
- `EmbedderError`

## Caller-visible data shapes

- `WriteReceipt` has exactly one public field: `cursor`
- `SearchResult` exposes `projection_cursor`, which names the terminal
  projection-visible point for the search snapshot
- hybrid fallback, when present, exposes a typed branch enum whose values are
  owned by `design/retrieval.md`
- counter/profile/stress payload shapes are owned by `design/lifecycle.md`

## Errors

Rust exposes typed open/runtime errors without message parsing:

- `EngineOpenError`
- `EngineError`

Canonical leaf mapping lives in `design/errors.md`. This file adopts those
types without renaming them.

## Recovery / operator seam re-exports

The `fathomdb` facade re-exports the following recovery and reporting types
from `fathomdb-engine` so that `fathomdb-cli` (the only public consumer of
these types) compiles against the public Rust surface, not engine internals.
These are CLI-only ergonomic types; they are NOT exposed as runtime SDK
verbs (recovery remains CLI-only — see Non-presence below). **Since Slice 27
fix-1 these 20 re-exports — and the `Engine` methods that produce them — are
gated behind the `operator` cargo feature** (which `fathomdb-cli` enables), so
they are absent from the default facade surface (see § Method-level boundary).

Re-exported types (canonical spellings, locked 2026-05-12; extended
2026-05-15):

- `CheckIntegrityOpts`
- `IntegrityReport`
- `SafeExportArtifact`
- `TraceReport`
- `TraceEvent`
- `RebuildReport`
- `RebuildKind`
- `ExciseReport`
- `VerifyEmbedderReport`
- `VerifyEmbedderStatus`
- `DumpSchemaReport`
- `SchemaObject`
- `DumpRowCountsReport`
- `TableRowCount`
- `DumpProfileReport`
- `TruncateWalReport`
- `TruncateWalStatus`

Engine methods backing these types are owned by `design/recovery.md` and
listed in `dev/plans/0.6.0-implementation.md` (Phase 10a + Phase 10b-A).
`PurgeLogicalIdReport` and `RestoreLogicalIdReport` were originally
forward-referenced for Phase 10b-B; both verbs are deferred to 0.8.0
(originally 0.7.x per ADR-0.6.0-cli-scope 2026-05-16 amendment;
re-targeted to 0.8.0 per HITL 2026-05-24 — see `dev/roadmap/0.8.0.md`
and the deferral note in `design/recovery.md § Logical-id purge and
restore`). When 0.8.0 re-opens the scope these types land here per
the same re-export rule.

## Non-presence

The Rust runtime surface does not expose recovery verbs. Recovery remains CLI
only per `design/recovery.md` and `design/bindings.md`. The re-exported
recovery types above are present as compile-time symbols for `fathomdb-cli`;
the runtime `Engine` does NOT gain corresponding SDK methods.
