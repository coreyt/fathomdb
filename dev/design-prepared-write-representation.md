# Design: PreparedWrite Representation

## Purpose

This document resolves the open Phase 2 item from
[design-typed-write.md](./design-typed-write.md):

> Decide whether `PreparedWrite` stays typed to execution time or compiles down
> to SQL plus binds.

The question is whether the writer thread receives typed Rust structs and
generates SQL at execution time, or whether the preparation stage compiles
everything to SQL text and bind lists before handing off to the writer.

## Current State

`PreparedWrite` holds fully typed structs:

```rust
struct PreparedWrite {
    label: String,
    nodes: Vec<NodeInsert>,
    edges: Vec<EdgeInsert>,
    chunks: Vec<ChunkInsert>,
    runs: Vec<RunInsert>,
    steps: Vec<StepInsert>,
    actions: Vec<ActionInsert>,
    node_kinds: HashMap<String, String>,
    required_fts_rows: Vec<FtsProjectionRow>,
    optional_backfills: Vec<OptionalProjectionTask>,
}
```

The writer thread's `apply_write()` function iterates over each typed struct
and constructs SQL inline:

```rust
tx.execute(
    "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
     VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)",
    params![node.row_id, node.logical_id, node.kind, node.properties, node.source_ref],
)?;
```

SQL strings are hardcoded in `apply_write`. There is no shape hashing, no
statement caching, and no intermediate compiled representation for writes.

## Options

### Option A: Stay Typed to Execution Time (Current Approach)

`PreparedWrite` continues to hold typed structs. `apply_write` continues to
construct SQL from struct fields at execution time.

**Advantages:**
- Simplest: no new abstraction layer.
- Debugging: typed structs are inspectable; SQL-plus-binds lists are opaque.
- Supersession logic is easier to express: conditional branching (upsert flag,
  chunk policy) maps directly to Rust control flow.
- Provenance warnings are easy to derive: iterate structs, check fields.
- Future schema changes require updating SQL in one place (`apply_write`), not
  in a separate compilation stage and a separate execution stage.

**Disadvantages:**
- SQL strings are rebuilt on every write (no statement reuse across requests).
- No shape hash means the writer cannot use prepared statement caching.

### Option B: Compile to SQL Plus Binds

`prepare_write()` produces a `Vec<(String, Vec<BindValue>)>` — a list of SQL
statements with their bind parameters. The writer thread executes them
sequentially without knowing their structure.

**Advantages:**
- Writer thread is pure execution: iterate list, execute each statement.
- Statement caching becomes possible if shapes are hashed.
- Cleaner separation between "what to write" and "how to execute."

**Disadvantages:**
- Loses type information before execution. Provenance warnings, backfill
  accounting, and debug logging must be computed during compilation, not
  during execution.
- Conditional logic (upsert, chunk policy, retire) must be resolved during
  compilation. The compiled form becomes a flat list of SQL statements where
  the intent is no longer visible.
- The write path has no shape diversity: every node insert uses the same SQL
  template, every edge insert uses the same SQL template. Statement caching
  gains almost nothing because the templates are trivially few.
- Adds a compilation stage that the read path already owns. Mixing two
  different compilation models (read compiler in `fathomdb-query`, write
  compiler in `fathomdb-engine`) increases cognitive load.
- Harder to test: verifying that compilation produces correct SQL requires
  snapshot testing against SQL text; verifying typed structs only requires
  assertion on field values.

## Decision: Stay Typed (Option A)

`PreparedWrite` remains typed to execution time. SQL construction stays in
`apply_write`.

### Rationale

1. **Write SQL is structurally fixed.** The read path benefits from compilation
   because query shapes vary: different filters, traversals, and driving tables
   produce structurally different SQL. Write SQL does not vary structurally.
   Every node insert uses the same INSERT template. Statement caching for
   writes provides no measurable benefit.

2. **Conditional logic is clearer in Rust.** The supersession design introduces
   branching: upsert flag, chunk policy, retire operations. These are natural
   `if`/`match` branches in `apply_write`. Flattening them to a pre-compiled
   SQL list obscures the control flow and makes the code harder to review.

3. **Provenance and diagnostics need type access.** The write receipt includes
   `provenance_warnings` derived by iterating typed structs. Moving this to a
   compilation stage is possible but adds no value: the same work happens, just
   in a different function.

4. **Simplicity.** Adding a write compilation layer doubles the surface area of
   the write path without solving a real performance problem. The single-writer
   discipline already serializes writes, so execution speed is bounded by
   SQLite transaction throughput, not by SQL string construction.

### Prepared Statement Reuse Within `apply_write`

The one concrete optimization that a compiled form would enable is prepared
statement reuse within a single transaction: prepare the INSERT template once,
bind and execute N times.

This can be achieved without changing the `PreparedWrite` representation by
using `rusqlite::Statement` directly inside `apply_write`:

```rust
let mut node_stmt = tx.prepare_cached(
    "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
     VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)"
)?;
for node in &prepared.nodes {
    node_stmt.execute(params![...])?;
}
```

This is a localized optimization that keeps `PreparedWrite` typed while
avoiding redundant SQL parsing for large batch writes.

## Implementation Plan

### Task 1: Use `prepare_cached` in `apply_write`

Replace `tx.execute(SQL, params)` calls in `apply_write` with
`tx.prepare_cached(SQL)` followed by `stmt.execute(params)` for each insert
type that processes multiple rows (nodes, edges, chunks, FTS rows).

This is a performance-only change. No behavior changes. No new types.

Tests:
1. Existing tests continue to pass (no behavioral change).
2. `writer_batch_insert_multiple_nodes` — insert 100 nodes in one request,
   assert all rows exist. This validates that prepare_cached works correctly
   across a loop.

Files: `crates/fathomdb-engine/src/writer.rs`

### Task 2: Document the Decision

Update the Phase 2 checklist in `design-typed-write.md` to mark this item
resolved and reference this document.

## Done When

- `apply_write` uses `prepare_cached` for repeated insert patterns
- No new types or compilation stages are introduced
- Existing tests pass without modification
- The batch insert test validates statement reuse correctness
