# Design: Required-Projection Derivation and Optional-Backfill Coverage

## Purpose

This document resolves the open Phase 2 item from
[design-typed-write.md](./design-typed-write.md):

> Add coverage for required-projection derivation and optional-backfill
> accounting.

The FTS projection derivation and optional backfill handoff paths exist but
lack edge-case coverage. This document defines the test matrix, identifies gaps
in the current derivation logic, and specifies the implementation work needed
to close them.

## Current State

### Required FTS Projection Derivation

`resolve_fts_rows()` runs in the writer thread before `BEGIN IMMEDIATE`. For
each `ChunkInsert`, it resolves the parent node's `kind`:

1. **Fast path:** If the node was co-submitted in the same `WriteRequest`, the
   kind is looked up from `prepared.node_kinds` (a `HashMap<String, String>`
   built during `prepare_write`).

2. **DB path:** Otherwise, the writer queries:
   ```sql
   SELECT kind FROM nodes
   WHERE logical_id = ?1 AND superseded_at IS NULL
   ```

3. **Validation:** If neither path resolves a kind, the write fails with
   `EngineError::InvalidWrite`.

Each resolved chunk produces an `FtsProjectionRow`:

```rust
struct FtsProjectionRow {
    chunk_id: String,
    node_logical_id: String,
    kind: String,
    text_content: String,
}
```

These rows are inserted atomically inside the same transaction as the canonical
writes.

### Optional Backfill

`OptionalProjectionTask` is a caller-provided struct:

```rust
pub struct OptionalProjectionTask {
    pub target: ProjectionTarget,  // Fts, Vec, or All
    pub payload: String,
}
```

Tasks are passed through from `WriteRequest` to `PreparedWrite` unchanged. The
writer does not execute them. `WriteReceipt.optional_backfill_count` reports
how many were submitted.

The intent is that a separate backfill service (not yet implemented) will
process these tasks asynchronously. The write path only accounts for them.

### Existing Test Coverage

| Test | What it covers |
|---|---|
| `writer_fts_rows_are_written_to_database` | FTS row insertion for co-submitted node+chunk |
| `writer_accepts_chunk_for_pre_existing_node` | DB-path kind resolution for chunks referencing nodes from a prior write |
| `writer_rejects_chunk_for_completely_unknown_node` | Validation: chunk with no node anywhere |
| `traversal_query_returns_connected_node_via_typed_writes` | FTS round-trip through query execution |

### What Is Missing

1. **No test for FTS row content correctness.** Existing tests verify FTS rows
   exist but do not assert that `kind` and `text_content` match expected
   values.

2. **No test for multiple chunks per node.** All current tests submit one chunk
   per node. The derivation loop is untested for the N-chunks-per-node case.

3. **No test for mixed fast-path and DB-path resolution.** A single write
   containing a new node with chunks AND chunks for a pre-existing node
   exercises both paths simultaneously. This is untested.

4. **No test for optional backfill accounting.** No test verifies that
   `WriteReceipt.optional_backfill_count` correctly reflects the submitted
   tasks.

5. **No test for empty chunk text.** A chunk with `text_content: ""` produces
   an FTS row with empty content. This is arguably incorrect: empty FTS rows
   waste index space and match nothing. The engine should either skip them or
   reject them.

6. **No test for duplicate chunk IDs across requests.** Two writes submitting
   chunks with the same `id` should produce a SQLite constraint error. This
   path is untested.

7. **No coverage for FTS derivation when node kind changes.** If a node is
   replaced (upsert) with a different `kind`, the FTS row derived from a new
   chunk should use the new kind. This interacts with the supersession path.

## Design Decisions

### Empty Chunk Text

**Decision:** `prepare_write` rejects chunks with empty `text_content` as
`EngineError::InvalidWrite`.

Rationale: FTS rows with empty content are useless. Catching this at validation
time is cheaper and clearer than letting an empty row enter the index.

### Duplicate Chunk IDs

**Decision:** No engine-level pre-check. SQLite's `UNIQUE` constraint on
`chunks.id` handles this. The resulting `rusqlite::Error` is surfaced as
`EngineError::Sqlite`.

Rationale: Checking for cross-request duplicate chunk IDs would require a DB
query during `prepare_write`. The SQLite constraint already provides the right
behavior. Adding a pre-check duplicates the enforcement without benefit.

### Optional Backfill Pass-Through

**Decision:** The current pass-through behavior is correct for v1. The write
path should not attempt to execute optional backfills. It should accurately
count them and surface the count in the receipt.

When the vector projection capability gate is real (per
[setup-sqlite-vec-capability.md](./setup-sqlite-vec-capability.md)), the
backfill path will evolve. For now, accurate accounting is sufficient.

## Implementation Plan

### Task 1: FTS Content Correctness Tests

Verify that derived FTS rows have the correct `kind` and `text_content`.

Tests:
1. `fts_row_has_correct_kind_from_co_submitted_node` — insert node with
   `kind: "Meeting"` and chunk, assert FTS row `kind = "Meeting"`.
2. `fts_row_has_correct_text_content` — insert chunk with known text, assert
   FTS row `text_content` matches exactly.
3. `fts_row_has_correct_kind_from_pre_existing_node` — write node in request 1,
   write chunk in request 2, assert FTS row kind matches the node's kind.

Files: `crates/fathomdb-engine/src/writer/mod.rs` (tests module)

### Task 2: Multiple Chunks Per Node

Test the N-chunks case and the mixed-resolution case.

Tests:
1. `fts_derives_rows_for_multiple_chunks_per_node` — insert one node and three
   chunks, assert three FTS rows exist with correct chunk IDs.
2. `fts_resolves_mixed_fast_and_db_paths` — in one write request, insert a new
   node with a chunk AND a chunk for a pre-existing node. Assert both FTS rows
   exist with correct kinds.

Files: `crates/fathomdb-engine/src/writer/mod.rs` (tests module)

### Task 3: Empty Chunk Text Rejection

Add validation in `prepare_write`.

Tests:
1. `prepare_write_rejects_empty_chunk_text` — submit chunk with
   `text_content: ""`, assert `InvalidWrite`.

Files: `crates/fathomdb-engine/src/writer/mod.rs`

### Task 4: Optional Backfill Accounting

Test that the receipt accurately reports backfill counts.

Tests:
1. `receipt_reports_zero_backfills_when_none_submitted` — write without
   optional backfills, assert `optional_backfill_count == 0`.
2. `receipt_reports_correct_backfill_count` — submit 3 optional backfill
   tasks, assert `optional_backfill_count == 3`.
3. `backfill_tasks_are_not_executed_during_write` — submit a backfill task
   targeting FTS, verify that no extra FTS rows were created beyond the
   required derivation.

Files: `crates/fathomdb-engine/src/writer/mod.rs` (tests module)

### Task 5: FTS Derivation After Node Kind Change

Test the interaction between supersession and FTS derivation.

Tests:
1. `fts_row_uses_new_kind_after_node_replace` — insert node as `kind: "Note"`,
   replace with `kind: "Meeting"` and new chunk using `ChunkPolicy::Replace`,
   assert FTS row has `kind = "Meeting"`.

This test depends on the `ChunkPolicy` implementation from
[design-detailed-supersession.md](./design-detailed-supersession.md). It can
be added in the same PR as the supersession work or deferred until that work
lands.

Files: `crates/fathomdb-engine/src/writer/mod.rs` (tests module)

## Test Summary

| # | Test name | Gap addressed |
|---|---|---|
| 1 | `fts_row_has_correct_kind_from_co_submitted_node` | Content correctness |
| 2 | `fts_row_has_correct_text_content` | Content correctness |
| 3 | `fts_row_has_correct_kind_from_pre_existing_node` | Content correctness |
| 4 | `fts_derives_rows_for_multiple_chunks_per_node` | Multi-chunk |
| 5 | `fts_resolves_mixed_fast_and_db_paths` | Mixed resolution |
| 6 | `prepare_write_rejects_empty_chunk_text` | Validation |
| 7 | `receipt_reports_zero_backfills_when_none_submitted` | Backfill accounting |
| 8 | `receipt_reports_correct_backfill_count` | Backfill accounting |
| 9 | `backfill_tasks_are_not_executed_during_write` | Backfill semantics |
| 10 | `fts_row_uses_new_kind_after_node_replace` | Supersession interaction |

## Done When

- FTS derivation content correctness is tested (kind, text_content)
- Multiple-chunks-per-node and mixed-resolution paths are tested
- Empty chunk text is rejected at validation time
- Optional backfill count is tested for accuracy
- All existing tests continue to pass
