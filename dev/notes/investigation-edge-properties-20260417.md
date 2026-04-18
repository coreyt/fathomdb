# Edge Property Investigation — 2026-04-17

## Trigger

Memex analysis: provenance links are written as both a `WMProvenanceLink` node and an edge (with variable `kind` = relationship string). `_load_provenance` queries the node rather than traversing the edge. Question: is FathomDB's edge support sufficient for Memex to use edges for provenance, and should FathomDB improve this?

## Findings

### Schema

`edges` table (defined in `crates/fathomdb-schema/src/bootstrap.rs:30`):

```sql
CREATE TABLE IF NOT EXISTS edges (
    row_id TEXT PRIMARY KEY,
    logical_id TEXT NOT NULL,
    source_logical_id TEXT NOT NULL,
    target_logical_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    properties BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    superseded_at INTEGER,
    source_ref TEXT,
    confidence REAL
);
```

Indexes: `(source_logical_id, kind, superseded_at)`, `(target_logical_id, kind, superseded_at)`, `source_ref`. No expression indexes on `properties`.

### Write side

`EdgeInsert` (`writer.rs:72`) has `properties: String`. The coordinator (`coordinator.rs:4517`) writes it to the `BLOB` column. Rusqlite binds `String` as SQLite `TEXT`, so `json_extract()` works correctly despite `BLOB` column affinity. Write side is functional.

### Read side — nodes (complete)

`NodeRow.properties: String` is included in all query result types: `QueryRows`, `ExpansionRootRows`, `ExpansionSlotRows`, `GroupedQueryRows`. Filter predicates `JsonPathEq` and `JsonPathCompare` exist in `compile_expansion_filter` (`coordinator.rs:43`) and are applied as `AND json_extract(n.properties, ?{p}) = ?{}` fragments on destination nodes during expand.

### Read side — edges (absent)

The traversal CTE (`coordinator.rs:2068`) joins on `edges` to find the next hop but selects only destination node columns (`n.row_id, n.logical_id, n.kind, n.properties, ...`). Edge `properties` is never selected, never returned, never filtered on. No `EdgeRow` result type exists. No edge property predicate support in `compile_expansion_filter` or the expand API.

### Indexing assessment

SQLite's `json_extract()` on a non-indexed column requires a scan of candidate rows. For edge traversal, candidates are already narrowed by the `(source_logical_id, kind, superseded_at)` index before any JSON extraction runs — typically a small result set. Expression indexes (`CREATE INDEX … ON edges(json_extract(properties, '$.key'))`) would help only for high-degree nodes with many edges of the same kind, which is not a current observed pattern. Deferred.

## Memex applicability

| Data | Edge? | Expand-eligible? |
|---|---|---|
| WMAction → WMObservation | Yes, `EDGE_HAS_OBSERVATION` | Yes — N traversals collapse to one `execute_grouped()` |
| WMObservation → WMAction | Yes, same edge IN direction | Yes — currently used per-hit, collapsible |
| Provenance links | Yes, but `kind` = relationship string (variable) | Partial — variable kind prevents single expand; needs fixed edge kind + property filter, or one expand per known kind |

Immediate win: collapse N per-hit `WMObservation → WMAction` traversals into one `execute_grouped()`. No FathomDB changes needed.

Provenance fix: refactor to use a fixed edge kind `HAS_PROVENANCE_LINK`, store the relationship type in `edge.properties`. Requires edge property filter support in FathomDB (not yet available). Alternatively: one expand per known relationship kind if the set is bounded (~5 values).

## Conclusions

1. Edge property write side is correct and functional.
2. Edge property read/filter side is entirely absent — this is the gap.
3. Node property query surface is complete and can serve as the implementation template.
4. Adding edge property filter in traversal is well-scoped: apply the `compile_expansion_filter` pattern to the `JOIN edges` condition in the traversal CTE.
5. Expression indexes deferred — the existing traversal indexes make JSON extraction on candidates practical without them.
6. Tracked in `dev/notes/0.5.1-scope.md`.
