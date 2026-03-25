# Design: Read Result Diagnostics

## Purpose

This document resolves the open Phase 2 item from
[design-read-execution.md](./design-read-execution.md):

> Decide whether read results should include diagnostics beyond decoded rows.

The question is whether `QueryRows` should carry metadata about the query
execution — SQL text, bind count, driving table, cache hit/miss, timing — or
remain a pure data container.

## Current State

`QueryRows` is a minimal data container:

```rust
pub struct QueryRows {
    pub nodes: Vec<NodeRow>,
}
```

`NodeRow` carries the four decoded columns:

```rust
pub struct NodeRow {
    pub row_id: String,
    pub logical_id: String,
    pub kind: String,
    pub properties: String,
}
```

The `CompiledQuery` that produces a `QueryRows` already carries metadata:

```rust
pub struct CompiledQuery {
    pub sql: String,
    pub binds: Vec<BindValue>,
    pub shape_hash: ShapeHash,
    pub driving_table: DrivingTable,
    pub hints: ExecutionHints,
}
```

But none of this metadata is exposed through the read result. A caller that
wants to know what SQL was executed must inspect the `CompiledQuery` before
calling `execute_compiled_read`.

## Options

### Option A: Keep QueryRows Pure

`QueryRows` remains a data-only container. Callers that need diagnostics
inspect the `CompiledQuery` they passed in.

### Option B: Add Diagnostics to QueryRows

`QueryRows` gains a `diagnostics` field:

```rust
pub struct QueryRows {
    pub nodes: Vec<NodeRow>,
    pub diagnostics: QueryDiagnostics,
}

pub struct QueryDiagnostics {
    pub sql: String,
    pub bind_count: usize,
    pub driving_table: DrivingTable,
    pub shape_hash: ShapeHash,
    pub cache_hit: bool,
    pub row_count: usize,
    pub execution_time_us: u64,
}
```

### Option C: Return a Separate Diagnostics Struct Alongside QueryRows

`execute_compiled_read` returns a tuple or a wrapper:

```rust
pub fn execute_compiled_read(
    &self, compiled: &CompiledQuery
) -> Result<(QueryRows, QueryDiagnostics), EngineError>
```

## Decision: Option A — Keep QueryRows Pure, Add Opt-In Diagnostics Method

`QueryRows` stays data-only. A separate method provides diagnostics for
callers that need them.

### Rationale

1. **Most callers don't need diagnostics.** Agent code that queries fathomdb
   wants decoded rows. Forcing every caller to destructure diagnostics or
   carry an unused field adds noise.

2. **Pre-execution diagnostics are already available.** `CompiledQuery`
   exposes `sql`, `binds`, `shape_hash`, `driving_table`, and `hints`. A
   caller that wants to log the query plan already has this information
   before calling `execute_compiled_read`. There is no need to echo it back
   in the result.

3. **Post-execution diagnostics are narrow.** The only information that
   `execute_compiled_read` knows and the caller does not is:
   - Whether the SQL was a cache hit (new prepare vs. reused statement)
   - Row count (but the caller can count `nodes.len()`)
   - Execution wall time (but the caller can time the call externally)

   These are observability concerns, not data concerns. They belong in a
   tracing/metrics layer, not in the return type.

4. **Future SDK bindings benefit from a simple return type.** When Python and
   TypeScript SDKs bind to the Rust facade, a simple `QueryRows` is easier to
   marshal than a struct-with-diagnostics. Keeping diagnostics separate avoids
   baking observability into the FFI contract.

### Opt-In Diagnostics: `explain_compiled_read`

For debugging, testing, and operator tooling, add a separate method that
returns diagnostics without executing the query:

```rust
/// Return the execution plan for a compiled query without executing it.
///
/// This is useful for debugging, testing shape-hash caching, and operator
/// diagnostics. It does not open a transaction or touch the database beyond
/// checking the statement cache.
pub fn explain_compiled_read(
    &self,
    compiled: &CompiledQuery,
) -> QueryPlan {
    QueryPlan {
        sql: compiled.sql.clone(),
        bind_count: compiled.binds.len(),
        driving_table: compiled.driving_table.clone(),
        shape_hash: compiled.shape_hash.clone(),
        cache_hit: self.shape_sql_cache.contains_key(&compiled.shape_hash),
    }
}
```

```rust
pub struct QueryPlan {
    pub sql: String,
    pub bind_count: usize,
    pub driving_table: DrivingTable,
    pub shape_hash: ShapeHash,
    pub cache_hit: bool,
}
```

This is a read-only introspection method. It does not execute SQL. It is
useful for:
- Test assertions about query compilation
- Operator-facing `EXPLAIN`-style output in the Go CLI
- SDK-level debug logging

### Execution Timing

Execution timing is explicitly **not** added to `QueryRows` or `QueryPlan`.

Callers that need timing should use external instrumentation:

```rust
let start = Instant::now();
let rows = coordinator.execute_compiled_read(&compiled)?;
let elapsed = start.elapsed();
```

If the engine later adds a tracing layer (e.g., `tracing` crate spans), timing
will be emitted as structured events, not embedded in return types.

## Implementation Plan

### Task 1: Add `QueryPlan` and `explain_compiled_read`

Add the `QueryPlan` struct and the `explain_compiled_read` method to
`ExecutionCoordinator`.

Tests:
1. `explain_returns_correct_sql` — compile a text search query, call
   `explain_compiled_read`, assert `sql` matches `compiled.sql`.
2. `explain_returns_correct_driving_table` — compile a text search query,
   assert `driving_table` is `FtsNodes`.
3. `explain_reports_cache_miss_then_hit` — call `explain_compiled_read`
   before and after `execute_compiled_read`. First call should report
   `cache_hit: false`, second should report `cache_hit: true`.
4. `explain_does_not_execute_query` — call `explain_compiled_read` on an
   empty database with no matching rows. Assert it returns a `QueryPlan`
   without error (proving it did not attempt to execute the SQL).

Files: `crates/fathomdb-engine/src/coordinator.rs`,
`crates/fathomdb-engine/src/lib.rs`,
`crates/fathomdb/src/lib.rs`

### Task 2: Re-export from Public Facade

Re-export `QueryPlan` and `explain_compiled_read` from `crates/fathomdb/src/lib.rs`.

Files: `crates/fathomdb/src/lib.rs`

## Done When

- `QueryRows` remains unchanged — no diagnostics fields
- `explain_compiled_read` returns a `QueryPlan` with SQL, bind count, driving
  table, shape hash, and cache-hit flag
- All tests pass
- The decision is documented and the Phase 2 checklist in
  `design-read-execution.md` is updated
