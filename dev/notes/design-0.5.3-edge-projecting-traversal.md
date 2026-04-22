# Design: Edge-Projecting Traversal (0.5.3 sole item)

**Release:** 0.5.3
**Scope item:** Item 1 (sole item) from `dev/notes/0.5.3-scope.md` (to be re-cut)
**Breaking:** Yes (intentional) — removes `NodeRow.edge_properties` in favor of first-class `EdgeRow`
**Status:** Locked 2026-04-20 (Memex + FathomDB agreement)

---

## Problem

FathomDB's traversal surface returns visited nodes only. Edge identity
(`logical_id`, `source_logical_id`, `target_logical_id`, `kind`) is
never exposed to callers. 0.5.1 added `NodeRow.edge_properties:
Option<String>` as a partial patch — the traversed edge's properties
JSON rides along on the destination node — but the edge row itself has
no surface.

Two consumers are blocked:

1. **Memex wm2**
   (`dev/2026-04-20-wm2-meeting-implementation-design.md`): typed
   edges (`updates_goal`, `creates_task`, `assigned_to`,
   `blocked_by`, ...) with rich per-edge metadata. wm2 hot paths
   need edge metadata AND endpoint node data in one row (e.g. edge
   risk class + target goal status). Memex currently materializes a
   synthetic "shadow node" per edge to work around the gap
   (`src/memex/fathom_store.py:1188`, `:1248`). Doubles writes,
   invents identity, does not scale.

2. **Cypher 0.6.0** (`dev/pathway-to-basic-cypher-2026-04-17.md`):
   `RETURN r` and `RETURN r.prop` require a first-class `EdgeRow`
   result type. 0.6.0 roadmap lists `EdgeRow` as a 0.5.1 prerequisite;
   only the `edge_properties` shortcut shipped, so `RETURN r` is still
   stubbed as `UnsupportedCypherFeature`.

---

## Current state (anchored to HEAD)

| Location | What exists |
|---|---|
| `crates/fathomdb-schema/src/bootstrap.rs:30` | `edges` table: `row_id`, `logical_id`, `source_logical_id`, `target_logical_id`, `kind`, `properties`, `created_at`, `superseded_at`, `source_ref`, `confidence`. |
| `crates/fathomdb-query/src/ast.rs:18-35` | `ExpansionSlot { slot, direction, label, max_depth, filter, edge_filter }` (0.5.1). |
| `crates/fathomdb-query/src/ast.rs:149-172` | `EdgePropertyEq`, `EdgePropertyCompare` (0.5.1). |
| `crates/fathomdb-engine/src/coordinator.rs:373-389` | `NodeRow` with `edge_properties: Option<String>` (0.5.1 shortcut — to be removed). |
| `crates/fathomdb-engine/src/coordinator.rs:463-477` | `ExpansionRootRows` / `ExpansionSlotRows` — no edge data. |
| `crates/fathomdb-engine/src/coordinator.rs:2281-2319` | Traversal CTE joins `edges e`, selects only `e.properties`. |
| `crates/fathomdb/src/ffi_types.rs:~1700-1870` | `FfiNodeRow` JSON wire; no `FfiEdgeRow`. |
| `python/fathomdb/_query.py:254` / `:858` | `.expand()` builder entry point. |
| `python/fathomdb/_types.py:321-344` / `:569-611` | `NodeRow` with `edge_properties`; `ExpansionRootRows` / `ExpansionSlotRows` / `GroupedQueryRows`. |
| `typescript/packages/fathomdb/src/types.ts:106-353` | Parallel TypeScript surfaces. |

---

## Goal

- Add first-class `EdgeRow` with full edge identity + kind +
  properties + provenance fields.
- Add `.traverse_edges(...)` sibling to `.expand(...)` in the
  builder, returning `Vec<(EdgeRow, NodeRow)>` tuples per root:
  each tuple pairs a traversed edge with its endpoint node
  (target on OUT, source on IN).
- Add `EdgeExpansionSlot` AST struct parallel to `ExpansionSlot`,
  carried on a separate `QueryAst.edge_expansions` vec. Slot-struct
  type is the discriminator; no enum tag.
- Extend `GroupedQueryRows` with an additive
  `edge_expansions: Vec<EdgeExpansionSlotRows>` field, enabling
  grouped queries that mix node- and edge-expansions in one
  round-trip.
- Remove `NodeRow.edge_properties`. Breaking. Intentional.

Locked choices (Memex + FathomDB agreement 2026-04-20):

- **Return shape:** `Vec<(EdgeRow, NodeRow)>` tuples, not parallel
  arrays or edges-only. wm2 hot paths need both in one row.
- **AST distinction:** distinct `EdgeExpansionSlot` struct on its own
  `edge_expansions` vec, not a shape flag on existing `ExpansionSlot`
  and no `ExpansionKind` tag (Memex confirmed 2026-04-20: builder-only
  access, no generic iteration use case).
- **Composition:** sibling expansion (grouped queries may mix node
  and edge slots), not terminal-only.
- **FFI:** new entry point + new wire row type; additive wire.
- **Breaking removal:** confirmed.

---

## Design

### 1. `EdgeRow` struct (`crates/fathomdb-engine/src/coordinator.rs`)

```rust
/// A single edge row surfaced during edge-projecting traversal.
///
/// Columns are sourced directly from the `edges` table; identity
/// fields (`source_logical_id`, `target_logical_id`) are absolute
/// (tail/head as stored), not re-oriented to traversal direction.
///
/// Multi-hop semantics: for `max_depth > 1`, each emitted tuple
/// reflects the final-hop edge leading to the emitted endpoint node.
/// Full path enumeration is out of scope for 0.5.3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeRow {
    /// Physical row ID from the `edges` table.
    pub row_id: String,
    /// Logical ID of the edge.
    pub logical_id: String,
    /// Logical ID of the edge source (tail).
    pub source_logical_id: String,
    /// Logical ID of the edge target (head).
    pub target_logical_id: String,
    /// Edge kind (label).
    pub kind: String,
    /// JSON-encoded edge properties.
    pub properties: String,
    /// Optional source reference for provenance tracking.
    pub source_ref: Option<String>,
    /// Optional confidence score attached to the edge.
    pub confidence: Option<f64>,
}
```

Deferred columns: `created_at`, `superseded_at`. Temporal API is a
separate release surface; nodes + edges get it together later.

### 2. `NodeRow.edge_properties` removal (breaking)

```rust
pub struct NodeRow {
    pub row_id: String,
    pub logical_id: String,
    pub kind: String,
    pub properties: String,
    pub content_ref: Option<String>,
    pub last_accessed_at: Option<i64>,
    // edge_properties REMOVED — callers read edge.properties from
    // the companion EdgeRow in the (EdgeRow, NodeRow) tuple.
}
```

`0.5.x` is pre-1.0. 0.5.1 → 0.5.2 → 0.5.3 breaking-per-release
pattern is already established. Memex is the only known external
consumer and confirms no wm2 reliance on `edge_properties` post-landing.

### 3. AST variant (`crates/fathomdb-query/src/ast.rs`)

```rust
/// An edge-projecting expansion slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeExpansionSlot {
    pub slot: String,
    pub direction: TraverseDirection,
    pub label: String,
    pub max_depth: usize,
    /// Optional endpoint-node filter (applied to the target-side node
    /// in OUT, or source-side in IN). Reuses the `Predicate` enum.
    pub endpoint_filter: Option<Predicate>,
    /// Optional edge-property filter (reuses EdgePropertyEq /
    /// EdgePropertyCompare from 0.5.1).
    pub edge_filter: Option<Predicate>,
}

pub struct QueryAst {
    pub root_kind: String,
    pub steps: Vec<QueryStep>,
    pub expansions: Vec<ExpansionSlot>,
    pub edge_expansions: Vec<EdgeExpansionSlot>, // NEW
    pub final_limit: Option<usize>,
}
```

Vec membership is the discriminator: `QueryAst.expansions` for node,
`QueryAst.edge_expansions` for edge. `ExpansionSlot` remains unchanged
(zero risk to 0.5.1 callers of `.expand()`); `EdgeExpansionSlot` is
additive. Memex confirmed no need for a unified-list discriminator
enum (2026-04-20); dropped for simplicity.

### 4. Result types (`crates/fathomdb-engine/src/coordinator.rs`)

```rust
/// Expansion results for a single root node within an edge-projecting
/// expansion slot. `pairs[i]` is the i-th (edge, endpoint) tuple
/// reached from the root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeExpansionRootRows {
    pub root_logical_id: String,
    pub pairs: Vec<(EdgeRow, NodeRow)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeExpansionSlotRows {
    pub slot: String,
    pub roots: Vec<EdgeExpansionRootRows>,
}

pub struct GroupedQueryRows {
    pub roots: Vec<NodeRow>,
    pub expansions: Vec<ExpansionSlotRows>,
    pub edge_expansions: Vec<EdgeExpansionSlotRows>, // NEW
    pub was_degraded: bool,
}
```

Existing node-expansion shapes (`ExpansionRootRows`,
`ExpansionSlotRows`) unchanged. Memex + future callers invoking only
`.expand()` see identical result shapes to 0.5.2.

### 5. Edge-expand CTE (new, parallel to `coordinator.rs:2281`)

**Implementation strategy: two SQL builders** (decided 2026-04-20;
Memex indifferent, FathomDB picks Option A for upgrade cleanliness).

The existing node-expand SQL builder at `coordinator.rs:2281` stays
byte-for-byte unchanged. All 0.5.2 `.expand(...)` call sites hit the
identical `shape_hash` post-upgrade — plan cache survives.

A new parallel builder emits edge-expand SQL. Same scaffold
(recursive `traversed` CTE seeded from `root_ids`, `numbered` outer
with `ROW_NUMBER()` per root), with these deltas:

```sql
traversed(
    root_id, logical_id, depth, visited, emitted,
    edge_row_id, edge_logical_id, edge_source_logical_id,
    edge_target_logical_id, edge_kind, edge_properties,
    edge_source_ref, edge_confidence
) AS (
    SELECT rid, rid, 0, printf(',%s,', rid), 0,
           NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL
    FROM root_ids
    UNION ALL
    SELECT t.root_id, {next_logical_id}, t.depth + 1,
           t.visited || {next_logical_id} || ',',
           t.emitted + 1,
           e.row_id, e.logical_id, e.source_logical_id,
           e.target_logical_id, e.kind, e.properties,
           e.source_ref, e.confidence
    FROM traversed t
    JOIN edges e ON {join_condition}
        AND e.kind = ?{edge_kind_param}
        AND e.superseded_at IS NULL{edge_filter_sql}
    WHERE t.depth < {max_depth}
      AND t.emitted < {hard_limit}
      AND instr(t.visited, printf(',%s,', {next_logical_id})) = 0
),
numbered AS (
    SELECT t.root_id, n.row_id, n.logical_id, n.kind, n.properties,
           n.content_ref, am.last_accessed_at,
           ROW_NUMBER() OVER (PARTITION BY t.root_id
                              ORDER BY n.logical_id) AS rn,
           t.edge_row_id, t.edge_logical_id,
           t.edge_source_logical_id, t.edge_target_logical_id,
           t.edge_kind, t.edge_properties,
           t.edge_source_ref, t.edge_confidence
    FROM traversed t
    JOIN nodes n ON n.logical_id = t.logical_id
        AND n.superseded_at IS NULL
    LEFT JOIN node_access_metadata am ON am.logical_id = n.logical_id
    WHERE t.depth > 0{endpoint_filter_sql}
)
SELECT root_id, row_id, logical_id, kind, properties, content_ref,
       last_accessed_at,
       edge_row_id, edge_logical_id, edge_source_logical_id,
       edge_target_logical_id, edge_kind, edge_properties,
       edge_source_ref, edge_confidence
FROM numbered
WHERE rn <= {hard_limit}
ORDER BY root_id, logical_id
```

Row construction emits `(EdgeRow, NodeRow)` tuple per row.

`WHERE t.depth > 0` excludes root seeds, so no NULL-edge row leaks
into the outer SELECT. Every emitted tuple has a non-NULL edge.

`edges.properties` is `BLOB NOT NULL` (bootstrap.rs:36). BLOB→String
decode reuses the existing path that already handles
`nodes.properties` (also BLOB-backed); no new conversion surface.

Plan cache: edge-expand gets its own `shape_hash` family (new SQL
text). Node-expand `shape_hash` unchanged. No cache invalidation for
existing callers.

Code cost: ~150 LOC near-duplicate SQL scaffolding in the new helper.
Accepted in exchange for zero plan-cache churn on 0.5.2 → 0.5.3
upgrade.

### 6. Compile path (`crates/fathomdb-engine/src/coordinator.rs`)

`execute_compiled_grouped_read` gains a second loop after the
existing node-expansion loop:

```rust
for edge_expansion in &compiled.edge_expansions {
    let slot_rows = if roots.is_empty() {
        EdgeExpansionSlotRows { slot: edge_expansion.slot.clone(),
                                roots: Vec::new() }
    } else {
        self.execute_edge_expansion_for_roots(
            edge_expansion,
            &roots,
        )?
    };
    edge_expansions.push(slot_rows);
}
```

New helper `execute_edge_expansion_for_roots` mirrors existing
node-expansion helper (`execute_expansion_for_roots`) with these
changes:
- Outer SELECT includes edge columns.
- Row construction emits `(EdgeRow, NodeRow)` tuples.
- `per_root: HashMap<String, Vec<(EdgeRow, NodeRow)>>`.
- Returns `Vec<EdgeExpansionRootRows>`.

`validate_fused_filter_for_edge_label` reused for fused
endpoint-filter predicates against the edge label's target set.

### 7. FFI wire (`crates/fathomdb/src/ffi_types.rs`)

New `FfiEdgeRow`:

```json
{
    "row_id": "er-...",
    "logical_id": "el-...",
    "source_logical_id": "...",
    "target_logical_id": "...",
    "kind": "assigned_to",
    "properties": "{...}",
    "source_ref": null,
    "confidence": null
}
```

New named struct `FfiEdgeExpansionPair { edge: FfiEdgeRow, endpoint:
FfiNodeRow }` — NOT a serde-of-tuple. Default `(A, B)` serialization
is a JSON array `[a, b]`; we want an object so both SDKs decode
unambiguously. Explicit named struct guarantees wire shape.

New `FfiEdgeExpansionRootRows` / `FfiEdgeExpansionSlotRows`:

```json
{
    "slot": "provenance",
    "roots": [
        {
            "root_logical_id": "meeting-abc",
            "pairs": [
                {"edge": {...FfiEdgeRow...},
                 "endpoint": {...FfiNodeRow...}}
            ]
        }
    ]
}
```

`GroupedQueryRows` wire grows an additive `edge_expansions` array.
Clients that don't request edge expansions see an empty list; no
wire-version bump. Existing FFI entry points `compile_grouped_ast` /
`execute_grouped_ast` (`crates/fathomdb/src/python.rs:166`, `:203`;
`crates/fathomdb/src/node.rs:118`, `:159`) accept the whole
`QueryAst` as JSON — the new `edge_expansions` field rides the
existing wire additively. No new entry point.

### 8. Builder API

**Rust (`crates/fathomdb-query/src/builder.rs`):**

```rust
impl SearchBuilder {
    pub fn traverse_edges(
        self,
        slot: impl Into<String>,
        direction: TraverseDirection,
        label: impl Into<String>,
        max_depth: usize,
    ) -> EdgeExpansionBuilder { ... }
}

impl EdgeExpansionBuilder {
    pub fn edge_filter(self, predicate: Predicate) -> Self { ... }
    pub fn endpoint_filter(self, predicate: Predicate) -> Self { ... }
    pub fn done(self) -> SearchBuilder { ... }
}
```

**Python (`python/fathomdb/_query.py`):**

```python
def traverse_edges(
    self,
    *,
    slot: str,
    direction: TraverseDirection | str,
    label: str,
    max_depth: int,
    edge_filter: dict | None = None,
    endpoint_filter: dict | None = None,
) -> "Query":
    """Register an edge-projecting expansion slot.

    Emits (EdgeRow, NodeRow) tuples per root on execution. The
    endpoint node is the target on OUT traversal, source on IN.
    Slot name must be unique across both node- and edge-expansions
    within the same query.
    """
```

**TypeScript:**

```typescript
traverseEdges(opts: {
    slot: string;
    direction: "in" | "out";
    label: string;
    maxDepth: number;
    edgeFilter?: Predicate;
    endpointFilter?: Predicate;
}): Query
```

### 9. Python result types (`python/fathomdb/_types.py`)

```python
@dataclass(frozen=True)
class EdgeRow:
    row_id: str
    logical_id: str
    source_logical_id: str
    target_logical_id: str
    kind: str
    properties: str  # JSON-encoded; callers parse with json.loads
    source_ref: str | None = None
    confidence: float | None = None

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "EdgeRow":
        ...


@dataclass(frozen=True)
class EdgeExpansionRootRows:
    root_logical_id: str
    pairs: list[tuple[EdgeRow, NodeRow]]


@dataclass(frozen=True)
class EdgeExpansionSlotRows:
    slot: str
    roots: list[EdgeExpansionRootRows]


@dataclass(frozen=True)
class GroupedQueryRows:
    roots: list[NodeRow]
    expansions: list[ExpansionSlotRows]
    edge_expansions: list[EdgeExpansionSlotRows]  # NEW
```

`NodeRow.edge_properties` field deleted. Export `EdgeRow`,
`EdgeExpansionRootRows`, `EdgeExpansionSlotRows` from
`fathomdb/__init__.py`.

Iteration example (Memex hot path):

```python
for slot in rows.edge_expansions:
    for root in slot.roots:
        for edge, endpoint in root.pairs:
            if json.loads(edge.properties)["risk_class"] == "high":
                consider(endpoint)
```

### 10. TypeScript result types

```typescript
export type EdgeRow = {
  rowId: string;
  logicalId: string;
  sourceLogicalId: string;
  targetLogicalId: string;
  kind: string;
  properties: string; // JSON-encoded; callers parse with JSON.parse
  sourceRef: string | null;
  confidence: number | null;
};

export type EdgeExpansionRootRows = {
  rootLogicalId: string;
  pairs: Array<{ edge: EdgeRow; endpoint: NodeRow }>;
};

export type EdgeExpansionSlotRows = {
  slot: string;
  roots: EdgeExpansionRootRows[];
};

export type GroupedQueryRows = {
  roots: NodeRow[];
  expansions: ExpansionSlotRows[];
  edgeExpansions: EdgeExpansionSlotRows[]; // NEW
};
```

TypeScript tuple shape uses `{edge, endpoint}` objects instead of
tuple arrays — idiomatic for TS consumers and survives JSON round-trip
without index-key ambiguity. Python's `tuple[EdgeRow, NodeRow]`
decodes from the same wire `{"edge": ..., "endpoint": ...}` dict.

Drop `edgeProperties` from `NodeRow`. Export all new types from
`packages/fathomdb/src/index.ts`.

### 11. Cypher 0.6.0 alignment

Translator rules after 0.5.3:

- `MATCH (a:K)-[r:TYPE]->(b) WHERE a.logical_id=$id RETURN r` →
  grouped query with one `EdgeExpansionSlot`, projection = edges
  only from the `(edge, endpoint)` tuple.
- `RETURN n, r` → grouped query with one `EdgeExpansionSlot`,
  projection = full tuples.
- `RETURN b` (endpoint) → `EdgeExpansionSlot` with projection = nodes
  from tuple, or `ExpansionSlot` (existing `.expand()`) — translator
  picks whichever yields the simpler plan.
- `RETURN r.prop` → `json_extract(edge.properties, '$.prop')` applied
  at projection. Translator emits a named column.
- `WHERE r.prop = $v` → already wired via `EdgePropertyEq` in
  `edge_filter` (0.5.1). Reused unchanged.

`dev/notes/0.6.0-roadmap.md` and
`dev/pathway-to-basic-cypher-2026-04-17.md` update when 0.5.3 ships:
strike `EdgeRow` from the blocking list; flip `RETURN r` /
`RETURN r.prop` statuses to "unblocked".

---

## TDD approach

Red-green-refactor per `feedback_tdd`. Pack order:

1. **Pack A — `EdgeRow` struct + `FfiEdgeRow` serde.**
   - Red: unit test round-trips `EdgeRow` ↔ `FfiEdgeRow` JSON;
     asserts all eight fields.
   - Green: struct + `FfiEdgeRow` + `From` impl.

2. **Pack B — AST + builder scaffold.**
   - Red: `crates/fathomdb-query/tests/edge_expansion_ast.rs`
     compiles an empty `EdgeExpansionSlot` through the builder;
     asserts AST shape.
   - Green: add `EdgeExpansionSlot`, `QueryAst.edge_expansions`,
     `SearchBuilder::traverse_edges`.

3. **Pack C — CTE edit + coordinator execution (engine).**
   - Red: `crates/fathomdb/tests/expansion_edges.rs`: write two
     nodes + edge with `source_ref` + `confidence`, run grouped
     query with `edge_expansions = [one slot]`, assert
     `GroupedQueryRows.edge_expansions[0].roots[0].pairs[0]` has
     both `EdgeRow` (all eight fields) and `NodeRow` matching
     inserts.
   - Red: multi-hop test (`max_depth=2`) → final-hop edge in each
     pair.
   - Red: edge_filter still works under edge-expansion.
   - Red: endpoint_filter narrows pairs correctly.
   - Red: mixed query with both `expansions` (node) and
     `edge_expansions` (edge) returns both populated correctly.
   - Green: CTE edit, new `execute_edge_expansion_for_roots`, wire
     through `execute_compiled_grouped_read`.

4. **Pack D — `NodeRow.edge_properties` removal.**
   - Red: existing 0.5.1 tests reading `node.edge_properties` fail
     to compile.
   - Green: delete field. Rewrite each affected 0.5.1 edge-filter
     test (e.g. `expansion_edge_filter.rs`) to assert the equivalent
     condition via the edge-expansion pair's `EdgeRow.properties`.
     Delete a test only if it becomes fully redundant with a Pack C
     test — never delete to shed the compile error.
   - CHANGELOG Breaking section written before merge.

5. **Pack E — Python binding.**
   - Red: `python/tests/test_bindings.py`: `.traverse_edges()`
     returns `GroupedQueryRows` with `edge_expansions[0].roots[0]
     .pairs` list of `(EdgeRow, NodeRow)` tuples.
   - Red: `EdgeRow.from_wire` round-trip test.
   - Red: cold-import `from fathomdb import EdgeRow` works.
   - Green: add dataclasses, wire decoder, builder method, exports.

6. **Pack F — TypeScript binding.**
   - Red: `typescript/apps/sdk-harness` or unit test: expand result
     carries `edgeExpansions` with typed pairs.
   - Red: cold-import coverage parallel to existing.
   - Green: add types, wire decoder, builder method, exports.

7. **Pack G — Docs + roadmap update.**
   - `docs/reference/query.md`: document
     `.traverse_edges()` + `GroupedQueryRows.edge_expansions`.
   - `docs/guides/querying.md`: example reading edge metadata +
     endpoint node from an edge-expansion.
   - `dev/notes/0.6.0-roadmap.md`: strike `EdgeRow` from blocking
     list; flip `RETURN r` / `RETURN r.prop` to "unblocked".
   - `dev/pathway-to-basic-cypher-2026-04-17.md`: same update.

Each pack = separate commit, orchestrated per
`feedback_orchestrate_releases`: implementer in worktree, code-reviewer
on diff, main thread plans and verifies only.

---

## Out of scope for 0.5.3

- **`(source_node, edge, target_node)` triples** (walk both endpoints
  in one row). Memex nice-to-have #2; current tuple shape covers the
  common OUT-walk case where source = the root. Reserve for 0.7.x if
  a demand signal materializes.
- **Edge-property FTS**. Memex nice-to-have #3. New per-edge-kind FTS
  surface = schema + rebuild actor + conformance. Own release.
- **Full edge-path enumeration for multi-hop** (`pairs` as
  `Vec<Vec<(EdgeRow, NodeRow)>>` describing the full path). Breaks
  tuple simplicity; defer. Document final-hop semantics clearly in
  `EdgeExpansionSlot` doc comment.
- **Temporal fields** (`created_at`, `superseded_at` on `EdgeRow`).
  Time-travel API is cross-cutting; nodes + edges together in a
  dedicated release.
- **Expression indexes on `edges.properties`**. Deferred in the
  2026-04-17 investigation; still deferred. Narrowing via
  `(source_logical_id, kind, superseded_at)` is sufficient for current
  workloads.
- **Parallel-array result shape** (option (c) from Memex Q1
  discussion). Only wins if fan-in dedup becomes measurable; wm2 query
  sizes don't justify the caller-side complexity today.
- **Edge-only return variant** (drop the endpoint node). Callers
  project the `.0` of the tuple client-side.

---

## Acceptance

- `cargo nextest run --workspace` green with new
  `expansion_edges.rs` suite (Pack C).
- `pytest python/tests/` green including new `EdgeRow` +
  `traverse_edges` coverage and the cold-import smoke test.
- `npm --prefix typescript test` green including new type + cold-import
  coverage.
- `scripts/preflight.sh --release` green (clippy tracing/python,
  nextest tracing).
- `scripts/preflight-CI.sh` green for full CI parity before tag.
- Memex's `fathom_store.py` can be rewritten post-landing to drop the
  shadow-node pattern; depends only on `fathomdb==0.5.3`.
- CHANGELOG.md Breaking section names `NodeRow.edge_properties`
  removal and migration path (`rows.edge_expansions[...].pairs`).
- 0.6.0 roadmap + Cypher pathway docs: `EdgeRow` struck from blocking
  list; `RETURN r` / `RETURN r.prop` statuses flipped.

---

## Risks

| Risk | Mitigation |
|---|---|
| CTE edit regresses hot path. | Existing grouped-query nextest suite + new `expansion_edges.rs` cover. Plan-cache `shape_hash` invalidates automatically on SQL text change. |
| Multi-hop "final-hop edge only" surprises callers. | Struct + slot doc comments + `docs/guides/querying.md` example + CHANGELOG note. |
| Slot-name collision between `.expand()` and `.traverse_edges()` within one query. | Validate at builder time: union of expansion slot names must be unique; raise `BuilderValidationError::DuplicateSlot` on conflict. Test. |
| Older wire consumers see new `edge_expansions` array. | Additive field. `from_wire` in Python + TS tolerates missing key → empty list during the `0.5.2 → 0.5.3` SDK/engine skew window. Remove tolerance at 0.6.0. |
| Breaking removal of `NodeRow.edge_properties` surprises integrators. | Pre-1.0 semver license; breaking-per-release precedent in 0.5.1 / 0.5.2; Memex (only known external consumer of the field) pre-agreed. CHANGELOG calls it out loudly. |
| Endpoint filter reuses node `Predicate` enum; some variants don't make sense on target nodes (e.g. `ContentRefNotNull` in a traversal endpoint). | Same constraint already exists for `ExpansionSlot.filter`; reuse the same validation path. No new surface. |
| Python tuple vs TS object shape divergence on the wire. | Wire is `{"edge": ..., "endpoint": ...}` dict in both directions (guaranteed by named `FfiEdgeExpansionPair` struct, not serde-of-tuple). Python decodes to `tuple[EdgeRow, NodeRow]` for ergonomic `for edge, endpoint in pairs:`; TS decodes to `{edge, endpoint}` object. Round-trip test in each SDK. |

---

## References

- Memex feature request: 2026-04-20 (this session).
- Memex Q1/Q2 resolution: 2026-04-20 (this session; locked Option B +
  tuples + sibling expansion).
- `dev/notes/investigation-edge-properties-20260417.md` — 0.5.1
  scoping decisions and indexing analysis.
- `dev/notes/design-0.5.1-edge-property-filter.md` — 0.5.1 `edge_filter`
  + `NodeRow.edge_properties` design (this doc supersedes the latter).
- `dev/pathway-to-basic-cypher-2026-04-17.md` — Cypher 0.6.0 AST,
  `RETURN r` / `RETURN r.prop` dependency on `EdgeRow`.
- `dev/notes/0.6.0-roadmap.md` — blocking dependency table, updates
  when 0.5.3 ships.
