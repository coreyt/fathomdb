# Design: Repair And Provenance Primitives

## Purpose

This document scopes the next design pass for the **admin, repair, and
provenance** layer.

The repo already has an admin surface, but most operations are still simplified.
The next step is to define which repair primitives become durable v1 contracts
and which remain deferred.

Companion docs:

- [setup-admin-bridge.md](./setup-admin-bridge.md)
- [setup-round-trip-fixtures.md](./setup-round-trip-fixtures.md)

## Layer Boundary

This layer covers the operator-facing and repair-facing runtime surface:

- integrity checks
- projection rebuild
- missing-projection repair
- trace by provenance
- excise by provenance
- safe export

This layer does not own:

- query compilation
- normal write ingestion
- SDK language bindings
- remote orchestration workflows

## Current Repository State

The current scaffold provides useful shape but shallow semantics:

- `check_integrity()` runs physical and foreign-key checks and detects missing
  FTS rows
- `rebuild_projections()` supports FTS and defers vector rebuild
- `trace_source()` returns row counts only
- `excise_source()` supersedes rows by `source_ref` but does not restore prior
  active versions
- the Rust admin bridge already exposes JSON-over-stdio commands

This is enough to prove the admin split, but not enough to support the repair
model described in the architecture docs.

## Design Goals

1. Keep canonical state authoritative and projections disposable.
2. Make provenance good enough for surgical repair.
3. Prefer deterministic rebuild and excision primitives over broad ad hoc SQL.
4. Keep Go as orchestration and UX around Rust-owned repair semantics.
5. Make every mutation-capable repair path auditable.

## Proposed Primitive Set

### 1. Integrity Primitives

The v1 integrity surface should expose checks for:

- `PRAGMA integrity_check`
- `PRAGMA foreign_key_check`
- missing required FTS rows
- missing optional vector rows when vector profiles are enabled
- active-row uniqueness by `logical_id`

The last check matters for append-oriented repair semantics.

### 2. Trace Primitives

`trace_source()` should grow from counts into structured lineage output.

Minimum useful v1 trace output:

- `source_ref`
- matching node rows
- matching edge rows
- matching action rows
- affected `logical_id`s
- timestamps or creation ordering

That gives excision and debugging a stable inspection surface.

### 3. Excision Primitives

`excise_source()` should eventually do more than supersede rows:

1. identify rows emitted by the bad `source_ref`
2. supersede those physical rows
3. find affected `logical_id`s
4. restore the most recent prior active version where one exists
5. repair required projections

This should stay a Rust-owned semantic operation, not a Go-side SQL script
generator in the first pass.

### 4. Rebuild Primitives

Projection rebuild should stay deterministic and canonical-driven:

- rebuild FTS from `chunks <- nodes`
- rebuild vector projections from enabled profiles once available
- rebuild only active state unless an admin mode later requires more

`rebuild_missing_projections()` should remain the startup and repair mechanism
for interrupted optional backfills.

### 5. Export Primitive

`safe_export()` should become more explicit about consistency rules:

- whether it checkpoints WAL
- whether it requires writer quiescence
- what metadata is included alongside the copy

The current file copy is a start, not the full contract.

## Key Design Questions

1. Is `source_ref` enough as the primary repair key in v1, or do we also need
   first-class run/step/action lookup helpers?
2. Should `excise_source()` rebuild projections immediately, or return a repair
   plan the caller executes in stages?
3. How much temporal rollback belongs in v1 versus staying deferred after
   `source_ref` excision is solid?
4. Should export produce only a copied `.sqlite` file, or also a small metadata
   manifest with schema version and protocol version?
5. Which invariants should fail `check_integrity()` versus surface as warnings?

## First Implementation Slice

1. Expand integrity checks with active-row uniqueness validation.
2. Replace count-only `TraceReport` with structured lineage detail.
3. Add a real excision routine that restores prior active versions when
   possible.
4. Re-run required projection repair after excision.
5. Add black-box tests that prove:
   - trace exposes affected rows
   - excision supersedes the bad version
   - the prior version becomes active again

## Definition Of Done For This Design Pass

This layer is scaffolded enough when:

- provenance-backed trace is useful for debugging
- excision semantics match append-oriented versioning
- integrity checks cover active-row invariants
- repair mutations remain Rust-owned
- the admin bridge can expose these primitives without inventing new semantics
