# Setup: sqlite-vec Capability

## Purpose

This document tracks the concrete work needed to turn vector support from a
placeholder into an explicit runtime capability.

This is a companion to:

- [design-read-execution.md](./design-read-execution.md)
- [design-typed-write.md](./design-typed-write.md)

## Current Repository State (updated Phase 3)

The Phase 3 deliverables are implemented:

- `sqlite-vec = "0.1"` added as an optional workspace dep; `load_extension` feature enabled in rusqlite
- `crates/fathomdb-engine`: `[features] sqlite-vec = ["dep:sqlite-vec", "fathomdb-schema/sqlite-vec"]`
- `crates/fathomdb-schema`: `[features] sqlite-vec = ["dep:sqlite-vec"]`
- `open_connection_with_vec()` in `sqlite.rs` registers `sqlite3_vec_init` via `sqlite3_auto_extension`
- `ensure_vector_profile()` in `bootstrap.rs` has `#[cfg(feature = "sqlite-vec")]` real impl and non-feature stub
- `BootstrapReport.vector_profile_enabled` queries `vector_profiles WHERE enabled = 1`
- `EngineOptions.vector_dimension: Option<usize>` threads through `Engine::open` → `EngineRuntime::open` → `ExecutionCoordinator::open`
- `ExecutionCoordinator.vector_enabled()` returns true iff feature is on and a profile was bootstrapped
- `VecInsert { chunk_id, embedding: Vec<f32> }` added; wired through `WriteRequest`, `PreparedWrite`, and `apply_write` (cfg-gated INSERT)
- `vec_nodes_active` virtual table naming: `profile="default"`, `table_name="vec_nodes_active"`

**Decisions made:**
- Loading strategy: `sqlite3_auto_extension` (global, idempotent) not per-connection `load_extension` from disk
- Table naming: `vec_nodes_active` as the active profile table name for v1
- Dimension source: `EngineOptions.vector_dimension` → stored in `vector_profiles` table

**Done when** criteria met:
- vector availability is reported explicitly ✓ (`vector_enabled()`)
- the engine can tell whether vector search is runnable ✓ (`CapabilityMissing` error)
- the active profile metadata has one concrete v1 shape ✓ (`vec_nodes_active`, `embedding float[N]`)
- vector-disabled behavior is deterministic in tests ✓ (`vec_insert_noop_without_feature`, `capability_gate_reports_false_without_feature`)

## Deliverables

1. Capability detection during schema/bootstrap.
2. A clear enable/disable model for vector profiles.
3. One active vector table naming scheme for v1.
4. Runtime errors that distinguish:
   - vector requested but extension unavailable
   - vector profile defined but disabled
   - vector profile enabled but missing projection rows
5. Tests that prove the engine degrades explicitly rather than silently.

## Decisions To Make

### 1. Loading Strategy

Choose one v1 model:

- rely on bundled/linked capability from the environment
- or support explicit extension loading at startup

The repo should not pretend both are solved yet.

### 2. Table Naming

The docs already imply versioned tables. The setup work should pick one concrete
v1 naming shape, for example:

- `vec_nodes_v1`
- plus a view or metadata pointer for the active profile

### 3. Dimension Source Of Truth

The profile metadata should define:

- embedding model or profile name
- dimension
- enabled/disabled flag
- physical table name

That metadata should be enough to validate runtime assumptions.

## Implementation Sequence

1. Extend schema bootstrap to detect vector capability and report it.
2. Define one vector profile metadata contract in `fathomdb-schema`.
3. Add admin/setup helpers to create the active vector table when capability is
   available.
4. Gate vector execution and vector write preparation on that capability report.
5. Add tests for both paths:
   - capability absent
   - capability present or mocked

## Done When

- vector availability is reported explicitly
- the engine can tell whether vector search is runnable
- the active profile metadata has one concrete v1 shape
- vector-disabled behavior is deterministic in tests
