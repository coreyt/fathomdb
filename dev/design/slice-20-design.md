# Slice 20 — Design Memo: G5/G6 Graph Traversal

**Baseline SHA:** e94fc1af6263f5a97cd7a097daf0b866bd84cf81
**Date:** 2026-06-13

---

## 1. BFS CTE SQL

### 1.1 Anchor: ADR Conflict Resolution

Two ADRs disagree on the traversal filter:

- `ADR-0.8.0-graph-traversal-scope.md` D-G2: `superseded_at IS NULL` only (G11 valid-time deferred)
- `ADR-0.8.1-graph-substrate-g11-migration.md` §5.2: filter includes `t_invalid IS NULL OR t_invalid > now`

The traversal scope ADR explicitly says it is **revisable** by the 0.8.1 graph slices without a formal ADR re-open. The G11 migration ADR (written later, signed 2026-06-13) explicitly specifies Slice 20's filter includes `t_invalid`. The Slice 20 prompt contract confirms the filter with `t_invalid`. Decision: implement with `t_invalid IS NULL OR t_invalid > now` per the prompt and the later ADR.

### 1.2 Visited-set cycle guard

Port from v0.5.6: the `visited` column is a comma-joined string of visited `logical_id` values. The cycle guard is: `instr(t.visited, printf(',%s,', {next_id})) = 0`. The initial seed includes the root to prevent the root from appearing in the expansion.

### 1.3 Direction parameterization

Three separate SQL strings are built at runtime based on `TraversalDirection` enum (Outgoing/Incoming/Both). SQL cannot safely parameterize a conditional JOIN condition, so we branch on direction.

### 1.4 The BFS CTE (Outgoing direction, as example)

```sql
WITH RECURSIVE
  traversal(logical_id, depth, visited) AS (
    -- anchor: seed the root node's direct neighbors at depth 1
    SELECT
      e.to_id,
      1,
      printf(',%s,%s,', ?1, e.to_id)
    FROM canonical_edges e
    WHERE e.from_id = ?1
      AND e.superseded_at IS NULL
      AND (e.t_invalid IS NULL OR e.t_invalid > ?2)
      AND e.to_id != ?1
    UNION ALL
    -- recursive: expand each frontier node
    SELECT
      e.to_id,
      t.depth + 1,
      t.visited || e.to_id || ','
    FROM traversal t
    JOIN canonical_edges e ON e.from_id = t.logical_id
    WHERE t.depth < ?3
      AND e.superseded_at IS NULL
      AND (e.t_invalid IS NULL OR e.t_invalid > ?2)
      AND instr(t.visited, printf(',%s,', e.to_id)) = 0
  )
SELECT DISTINCT n.logical_id, n.kind, n.body, n.write_cursor
FROM traversal tr
JOIN canonical_nodes n ON n.logical_id = tr.logical_id
WHERE n.superseded_at IS NULL
LIMIT ?4
```

Parameters: `?1` = root_logical_id, `?2` = now (datetime ISO-8601 string), `?3` = max_depth (≤ 3), `?4` = hard cap (50).

**Note on DISTINCT in recursive term:** SQLite does NOT support `DISTINCT` inside the recursive term of a CTE. The visited-string approach eliminates cycles at the recursive step, and the final `SELECT DISTINCT` deduplicates nodes that appear at multiple depths.

**Note on anchor visited init:** The visited string is initialized as `',' || root || ',' || neighbor || ','` so both the root and the first neighbor are marked as visited in the initial row.

**"now" implementation:** Use `datetime('now')` passed as a bind parameter (fetched on the Rust side as an ISO-8601 string via `SELECT datetime('now')` in the same read transaction, or passed as the literal string `datetime('now')` inlined in SQL). We pass a pre-fetched ISO-8601 now-string as `?2` to avoid SQL injection.

### 1.5 Incoming and Both directions

- **Incoming:** `e.to_id = <node_id>`, next = `e.from_id`
- **Both:** `(e.from_id = <node_id> OR e.to_id = <node_id>)`, next = `CASE WHEN e.from_id = <node_id> THEN e.to_id ELSE e.from_id END`

---

## 2. How G6 (search_expand) composes G1 + G5

```text
search_expand(query, filter, depth):
  1. search_filtered(query, filter) -> SearchResult (G1+G9 hybrid)
  2. For each search hit: resolve logical_id via canonical_nodes WHERE write_cursor = hit.id
  3. Collect unique root logical_ids
  4. For each root: graph_neighbors(logical_id, depth, Both) -> Vec<NodeRecord>
  5. Merge and deduplicate
```

Implementation note: step 2 resolves the `write_cursor` (the `SearchHit.id`) back to a `logical_id`. The engine uses a DEFERRED reader tx for this. We use `read_get_by_cursor` or a direct SELECT.

---

## 3. Result deduplication/merging strategy for G6

`SearchExpandResult` fields:

- `search_hits: Vec<SearchHit>` — original RRF-scored results (with scores)
- `expanded: Vec<(NodeRecord, u32)>` — (node, hop_count) for nodes NOT in search hits
- `all_logical_ids: Vec<String>` — union of search hit logical_ids + expanded logical_ids

Deduplication rule: a node appearing in both search hits and expansion appears ONLY in `search_hits`. Search score takes priority. The `expanded` vec contains only nodes not already in `search_hits`.

---

## 4. Hard cap and depth > 3 rejection

- **depth > 3 at SDK**: `graph_neighbors(depth=4)` returns `Err(EngineError::InvalidArgument { msg })` immediately before any SQL.
- **hard cap 50**: `LIMIT 50` in the CTE result — the SQL hard cap is independent of depth rejection.
- Engine-level hard cap: a `depth > 50` call also returns `InvalidArgument` (defense-in-depth per ADR D-G1).
- `InvalidArgument` is a new variant added to `EngineError`.

---

## 5. Python/TS API shapes

### Python

```python
# Module-level functions (pattern: read_get, read_collection)
def graph_neighbors(
    engine: Engine,
    logical_id: str,
    depth: int,
    direction: str,  # "outgoing" | "incoming" | "both"
) -> list[NodeRecord]: ...

def search_expand(
    engine: Engine,
    query: str,
    depth: int,
    source_type: str | None = None,
    kind: str | None = None,
    created_after: int | None = None,
    status: str | None = None,
) -> SearchExpandResult: ...

class SearchExpandResult:
    search_hits: list[SearchHit]
    expanded: list[ExpandedNode]  # (node, hop_count)
    all_logical_ids: list[str]

class ExpandedNode:
    node: NodeRecord
    hop_count: int
```

### TypeScript

```typescript
interface SearchExpandResult {
  searchHits: Array<SearchHit>
  expanded: Array<ExpandedNode>
  allLogicalIds: Array<string>
}

interface ExpandedNode {
  node: NodeRecord
  hopCount: number
}

// Top-level functions
declare function graphNeighbors(engine: Engine, logicalId: string, depth: number, direction: 'outgoing' | 'incoming' | 'both'): Promise<Array<NodeRecord>>
declare function searchExpand(engine: Engine, query: string, depth: number, filter?: SearchFilterInput): Promise<SearchExpandResult>
```

---

## 6. EXPLAIN gate strategy

**Test**: `explain_plan_uses_indexes` in `tests/slice20_graph_traversal.rs`.

**Approach**: Run `EXPLAIN QUERY PLAN <BFS CTE SQL>` via rusqlite. The plan returns multiple rows with columns `(id INTEGER, parent INTEGER, notused INTEGER, detail TEXT)`. Collect all `detail` strings, assert:

1. At least one row contains `"USING INDEX"` referencing `canonical_edges`.
2. No row contains `"SCAN canonical_edges"` without `"USING INDEX"`.

The `canonical_edges(from_id)` index enables the anchor join; the `canonical_edges(to_id)` index enables incoming direction joins. Both are verified by the EXPLAIN gate.

**Note on reader pool**: The graph traversal is implemented via a new `ReaderRequest::GraphNeighbors` variant dispatched through the existing reader worker pool (same DEFERRED-tx read isolation as GetById/ReadCollection). The EXPLAIN gate test uses the internal `fn graph_neighbors_in_tx` helper directly to inspect the SQL.

---

## 7. Implementation order

Per ADR D-G4 and the prompt: build G6 (`search_expand`) before standalone G5 (`graph_neighbors`). G5 is the lower-level primitive G6 already exercises; after G6 is built, standalone G5 is promoted as the already-built expand step.

In practice, the engine-layer `fn graph_neighbors_in_tx` is implemented first (since G6 calls it), then `Engine::graph_neighbors` (the public verb) and `Engine::search_expand` are both implemented on top.
