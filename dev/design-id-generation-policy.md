# Design: v1 ID-Generation Policy

## Purpose

This document resolves the open Phase 2 item from
[design-typed-write.md](./design-typed-write.md):

> Decide the v1 ID-generation policy for typed writes.

The engine needs a clear contract for who generates `row_id` and `logical_id`
values, what format they take, and whether the engine ever assigns them
automatically.

## Current State

All typed insert structs require callers to provide IDs:

```rust
pub struct NodeInsert {
    pub row_id: String,       // unique per physical row
    pub logical_id: String,   // stable across versions
    ...
}
```

`EdgeInsert` follows the same pattern. `ChunkInsert` has `id: String`.
`RunInsert`, `StepInsert`, and `ActionInsert` have `id: String`.

There is no engine-provided ID generation. Callers must supply all IDs.

The partial unique index `idx_nodes_active_logical_id` enforces at most one
active row per `logical_id`. The `row_id` column has a plain `UNIQUE`
constraint. Duplicate IDs produce a SQLite constraint error at write time.

## Decision: Callers Own IDs

The v1 policy is **caller-owned IDs**. The engine does not generate, assign, or
sequence IDs as part of the write path.

### Rationale

1. **Agent callers already have stable identifiers.** Conversation IDs, document
   hashes, run UUIDs, and similar external identifiers are the natural
   `logical_id` for most agent use cases. Forcing callers to map their IDs to
   engine-generated IDs adds complexity without benefit.

2. **Idempotent replay.** Caller-owned IDs enable deterministic replay: the same
   input produces the same IDs. Engine-generated IDs break this property because
   a replayed write would get different IDs each time.

3. **Engine stays stateless for ID sequencing.** No global counter, no clock
   dependency for ID generation, no coordination between concurrent callers.

4. **Supersession relies on `logical_id` stability.** The `upsert` mechanism
   supersedes the active row by `logical_id`. If the engine generated
   `logical_id` values, callers would need to query the engine to learn what ID
   was assigned before they could issue a replace — an unnecessary round-trip.

### What This Means for Callers

- Callers **must** provide `row_id` and `logical_id` on every `NodeInsert` and
  `EdgeInsert`.
- Callers **must** provide `id` on every `ChunkInsert`, `RunInsert`,
  `StepInsert`, and `ActionInsert`.
- The engine rejects writes with empty or missing IDs at the `prepare_write`
  validation stage, not at SQLite execution time.
- Duplicate `row_id` or `id` values produce an `EngineError::InvalidWrite`.
- Duplicate active `logical_id` values (insert without upsert when an active
  row already exists) produce a SQLite constraint error surfaced as
  `EngineError::Sqlite`.

## Engine-Provided ID Utility

The engine provides an **optional** standalone helper for callers that do not
have their own ID scheme:

```rust
/// Generate a new identifier suitable for use as a row_id, logical_id, or
/// chunk/run/step/action id.
///
/// Format: 26-character ULID (Universally Unique Lexicographically Sortable
/// Identifier). ULIDs are timestamp-prefixed, so IDs generated close in time
/// sort together. They are case-insensitive and URL-safe.
///
/// This function is not part of the write path. Callers that already have
/// stable identifiers are not required to use it.
pub fn new_id() -> String { ... }
```

### Why ULID

- Lexicographic sort order matches insertion order, which is useful for
  time-ordered queries over `runs`, `steps`, and `actions`.
- 128-bit entropy is sufficient to avoid collisions without coordination.
- String representation is compact (26 characters) and human-readable.
- No external service or shared state required.

### Dependency

Add the `ulid` crate to `fathomdb-engine`. It is small, well-maintained, and
has no transitive dependencies beyond `rand`.

## Validation Rules

`prepare_write()` enforces these rules before any write reaches the writer
thread:

| Field | Rule | Error |
|---|---|---|
| `NodeInsert.row_id` | Non-empty, unique within request | `InvalidWrite` |
| `NodeInsert.logical_id` | Non-empty | `InvalidWrite` |
| `EdgeInsert.row_id` | Non-empty, unique within request | `InvalidWrite` |
| `EdgeInsert.logical_id` | Non-empty | `InvalidWrite` |
| `ChunkInsert.id` | Non-empty, unique within request | `InvalidWrite` |
| `RunInsert.id` | Non-empty, unique within request | `InvalidWrite` |
| `StepInsert.id` | Non-empty, unique within request | `InvalidWrite` |
| `ActionInsert.id` | Non-empty, unique within request | `InvalidWrite` |

Cross-request uniqueness is enforced by SQLite's `UNIQUE` constraints and
surfaces as `EngineError::Sqlite`.

## Implementation Plan

### Task 1: Add `prepare_write` Validation

Add non-empty and intra-request uniqueness checks for all ID fields in
`prepare_write()`. Return `EngineError::InvalidWrite` with a descriptive
message on violation.

Tests:
1. `prepare_write_rejects_empty_node_row_id`
2. `prepare_write_rejects_empty_node_logical_id`
3. `prepare_write_rejects_duplicate_row_ids_in_request`
4. `prepare_write_rejects_empty_chunk_id`

Files: `crates/fathomdb-engine/src/writer/mod.rs`

### Task 2: Add `new_id()` Utility

Add `fathomdb-engine/src/ids.rs` with a `new_id()` function that returns a ULID
string. Re-export from `fathomdb/src/lib.rs`.

Tests:
1. `new_id_returns_nonempty_string`
2. `new_id_returns_unique_values` — two calls return different strings
3. `new_id_is_26_characters` — ULID format check
4. `new_id_is_valid_for_node_insert` — use as `row_id`, assert write succeeds

Files: `crates/fathomdb-engine/src/ids.rs` (new),
`crates/fathomdb-engine/src/lib.rs`, `crates/fathomdb/src/lib.rs`

## Done When

- `prepare_write` rejects empty and duplicate IDs before the writer thread
- `new_id()` is available as a public utility
- All tests pass
- No write-path code generates IDs automatically
