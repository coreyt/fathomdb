# Design: Grouped Read Expansion Batching

## Purpose

Address the verified finding that grouped reads execute an N+1 query
pattern (M-4): 1 root query plus `roots.len() * expansions.len()`
expansion queries.

---

## Current State

`crates/fathomdb-engine/src/coordinator.rs:243-282`

`execute_compiled_grouped_read()` runs:
1. One root query via `execute_compiled_read(&compiled.root)`.
2. For each expansion slot, for each root, one expansion query via
   `read_expansion_nodes()`.

For 10 roots with 3 expansion slots, this is 31 queries. Each expansion
query is bounded by `hard_limit`, so total row count is bounded, but the
per-query overhead (prepare, execute, fetch) dominates for large root
sets.

---

## Design

### Batch expansion queries per slot

Replace the root-by-root inner loop with a single query per expansion
slot that uses `IN (...)` over root IDs:

```sql
SELECT ... FROM nodes
WHERE source_logical_id IN (?1, ?2, ?3, ...)
  AND superseded_at IS NULL
  AND edge_label = ?
ORDER BY source_logical_id, created_at DESC
LIMIT ?
```

This reduces the query count from `1 + roots * expansions` to
`1 + expansions`.

### Challenges

**1. Dynamic parameter count.**

SQLite has a `SQLITE_MAX_VARIABLE_NUMBER` limit (default 999, can be
compiled up to 32766). For typical grouped reads with < 100 roots, this
is not a concern. For safety, fall back to the per-root pattern when
`roots.len()` exceeds a threshold (e.g. 200).

**2. Per-root hard limit.**

The current code applies `hard_limit` per root per expansion. A batched
query with a global `LIMIT` would cap the total, not per-root. To
preserve per-root semantics, the batched query must use a window function
or post-fetch grouping:

```sql
SELECT * FROM (
    SELECT *, ROW_NUMBER() OVER (PARTITION BY source_logical_id ORDER BY created_at DESC) as rn
    FROM nodes
    WHERE source_logical_id IN (...)
      AND superseded_at IS NULL
      AND edge_label = ?
) WHERE rn <= ?hard_limit
```

SQLite supports `ROW_NUMBER() OVER (PARTITION BY ...)` since 3.25.0.
The project requires 3.41.0+, so this is available.

**3. Result grouping.**

The batched results must be split back into per-root groups for the
`GroupedReadResults` structure. This is a simple post-fetch partition by
`source_logical_id`.

### Phasing

This is a pure performance optimization with no semantic change. It can
be implemented after the reader connection pool (M-1) since the pool
provides more impactful concurrency improvement with less code change.

---

## Test Plan

- Grouped read with 10 roots and 3 expansions produces identical results
  before and after batching.
- Grouped read with > 200 roots falls back to per-root queries.
- Per-root `hard_limit` is respected in the batched path.
- Window function query plan is efficient (verify with `EXPLAIN QUERY PLAN`).
