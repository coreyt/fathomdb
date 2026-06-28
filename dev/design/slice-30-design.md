# Slice 30 (0.8.1) — R3 Graph-Retrieval Arm Design Memo

> NOTE: The previous `slice-30-design.md` covered the 0.8.0 Slice 30 (G2/G3 read.get/read.collection).
> That work is CLOSED and on main. This file now covers the 0.8.1 Slice 30 (R3 graph-retrieval arm).
> The 0.8.0 G2/G3 design is preserved in git history.

**Author:** Slice 30 implementer agent
**Date:** 2026-06-13
**Status:** self-reviewed, approved for implementation
**References:** `dev/plans/0.8.1-implementation.md` §Slice 30; `dev/plans/prompts/0.8.1-SLICE-30-PREP.md`;
`dev/plans/runs/IR-C-roadmap.md` §R3/C3/C7

---

## Design Questions (§3.0) — Answers

### Q1 — Third-arm integration into `fuse_rrf`

**Decision: Approach B** — new function `fuse_three_arms` with `fuse_rrf` delegating.

```rust
pub fn fuse_three_arms(
    vector_hits: Vec<SearchHit>,
    text_hits: Vec<SearchHit>,
    graph_hits: Vec<SearchHit>,
) -> Vec<SearchHit> {
    // full 3-arm RRF implementation
}

pub fn fuse_rrf(vector_hits: Vec<SearchHit>, text_hits: Vec<SearchHit>) -> Vec<SearchHit> {
    // delegate to fuse_three_arms with empty graph arm
    fuse_three_arms(vector_hits, text_hits, vec![])
}
```

**Rationale:** All existing `pr_g9_rrf_fusion.rs` unit tests call `fuse_rrf(v, t)` — they remain GREEN with zero modification since `fuse_rrf` is unchanged at the call site. `fuse_three_arms` is the new function that the production path calls when `use_graph_arm=true`. When `use_graph_arm=false`, the production path continues calling `fuse_rrf(v, t)` -> `fuse_three_arms(v, t, vec![])` -> byte-identical output.

**Backward compatibility:** with `use_graph_arm=false` (default), the production pipeline calls `fuse_rrf(vector_results, text_results)` unchanged, producing byte-identical output. The only change is `fuse_rrf` now delegates internally; the output contract is preserved by the identity `fuse_three_arms(v, t, vec![]) == fuse_rrf(v, t)`.

### Q2 — Graph-arm weight constant

```rust
pub const RRF_WEIGHT_GRAPH: f64 = 1.0;
```

**Justification:** Conservative starting value, equal to `RRF_WEIGHT_VECTOR`. Without R2 per-class delta data we cannot calibrate the graph arm weight. 1.0 is the minimum non-zero contribution; text already dominates at 3.0. The graph arm's purpose is to surface additional *reachable* nodes, not to override the primary signals. A value of 1.0 means the best graph-arm hit (position 0 in the sorted candidate list) gets `1.0/(30+1) = 0.032` — comparable to a rank-1 vector hit.

### Q3 — Graph arm candidate generation

**Seed count N = 10**: BFS is seeded from the top-min(10, len) hits of the initial 2-arm fused result
(before the graph arm is added). Nodes with `branch == SoftFallbackBranch::TextEdge` are filtered out
before seeding (edge-body hits do not have a node write_cursor in `canonical_nodes`).

**Seed filter:** Only hits whose `write_cursor` resolves to an active node in `canonical_nodes`
(verified via SQL) are used as BFS roots. TextEdge hits are skipped.

**Graph arm pool:** Only NEWLY-reached nodes (NOT in the seed set) are added to the graph arm
candidate pool. Seed nodes are already ranked in the vector/text arms — adding them would create
double-counting without new signal.

**Temporal filter:** The BFS excludes:

- Edges with `t_invalid IS NOT NULL AND datetime(t_invalid) <= datetime('now')` (invalidated)
- Edges with `superseded_at IS NOT NULL` (superseded)

The existing `build_bfs_with_depth_sql()` CTE already applies this filter. The graph arm reuses
this SQL directly.

**BFS parameters:** depth <= 3, cap 50 (same as `graph_neighbors`).

### Q4 — Scoring formula for graph-arm candidates

**Formula:** `decay = 1.0 / (1.0 + hop_count as f64)`

| hop_count | decay score |
|-----------|-------------|
| 1         | 0.5         |
| 2         | 0.333       |
| 3         | 0.25        |

**Synthesized penalty applied before sorting** (see Q7): multiply decay by 0.3 for `kind="unknown"`.

**Sorted order:** Candidates are sorted by decay score DESC, then by body ASC (deterministic tiebreak).
This sorted order is passed as `graph_hits: Vec<SearchHit>` to `fuse_three_arms`.

**RRF compatibility:** In `fuse_three_arms`, the graph arm uses position-based RRF (same as
vector/text arms):

- Position 0 -> `RRF_WEIGHT_GRAPH / (RRF_K + 1) = 0.032`
- Position 1 -> `RRF_WEIGHT_GRAPH / (RRF_K + 2) = 0.031`

These scores are in the same range as vector/text RRF contributions. The decay-based sorting ensures
close-hop nodes rank higher within the arm.

**Why not raw hop count as score?** Hop counts (1, 2, 3) are not in the same range as RRF scores
(~0.01-0.10). The decay function maps them to [0.25, 0.5] which is used to determine ORDER within
the arm, not the final accumulated score.

### Q5 — `SoftFallbackBranch` for graph-arm hits

**Decision:** Add `SoftFallbackBranch::GraphArm` (new variant).

```rust
pub enum SoftFallbackBranch {
    Vector,
    Text,
    TextEdge,   // G11 Slice 15
    GraphArm,   // R3 Slice 30
}
```

**Match arms to update:**

1. `fathomdb-napi/src/lib.rs` — two match arms converting to string: add
   `SoftFallbackBranch::GraphArm => "graph_arm"` (x2)
2. `fathomdb-py/src/lib.rs` — two match arms: add
   `SoftFallbackBranch::GraphArm => "graph_arm"` (x2)
3. `fathomdb-engine/src/lib.rs` — the `search_expand_in_tx` match on `hit.branch`

**Facade re-export:** `SoftFallbackBranch` is already re-exported in `fathomdb/src/lib.rs` via
`pub use fathomdb_engine::{..., SoftFallbackBranch, ...}`. Adding the new variant is automatically
included.

**TS SDK:** `SoftFallbackBranch = "vector" | "text" | "text_edge"` -> add `"graph_arm"`. Update
the branch check in `search()`.

**Python SDK:** The `SoftFallbackBranch` literal type in `fathomdb/types.py`; add `"graph_arm"`.

### Q6 — `temporal_fallback` handling

**Decision: DEFERRED — SCHEMA GATE FIRED.**

At BFS traversal time, `canonical_edges` rows do not carry a `temporal_fallback` flag. The flag
exists only in the ELPS extraction result as `warnings[].kind = "temporal_fallback"` and is not
persisted to any DB column by the current ingest pipeline (Slice 15).

**Detection options evaluated:**

- **Sentinel `t_valid` detection**: The fallback `t_valid` equals the document's `created_at`. We
  do not store `created_at` in `canonical_edges`, so there is no sentinel to compare against.
- **Schema change**: Add `temporal_fallback BOOLEAN NOT NULL DEFAULT 0` to `canonical_edges`. This
  requires a `SCHEMA_VERSION` bump (14 -> 15) and HITL approval. BLOCKED per §3.0.R schema gate.
- **Auxiliary table**: A `canonical_edge_metadata(rowid, temporal_fallback)` table could hold the
  flag. However, this would require schema migration machinery equivalent to a schema bump.

**Test `graph_arm_temporal_fallback_excluded_or_downweighted`:** Written as an `#[ignore]` test
documenting the intended behavior. It becomes a real RED/GREEN test once the schema gate is
resolved.

### Q7 — `synthesized` node penalty

**Decision: Option (b) — `kind = "unknown"` heuristic.**

In the ELPS protocol, synthesized dangling-endpoint nodes always have `type: "unknown"` (see QD
case d6: `Grace{type:"unknown", synthesized:true}`). The ingest pipeline stores this type as the
node's `kind` in `canonical_nodes`. Therefore, `kind = "unknown"` is a reliable proxy for
`synthesized = true` in the current schema.

**Penalty:** Multiply BFS-decay score by **0.3** (<=0.5 per spec, conservative).

```text
decay_final = decay_base * (if kind == "unknown" { 0.3 } else { 1.0 })
```

A synthesized node at hop 1 gets `0.5 * 0.3 = 0.15`, while a regular node at hop 3 gets `0.25`.
The synthesized node still appears but ranks BELOW non-synthesized hop-3 nodes.

**No schema change required.**

### Q8 — `capped` warning surface

**Decision: DEFERRED.**

The `capped` warning (edge truncation at extraction time) is per-document and is not persisted to
the DB by the current ingest pipeline. Without a schema change (adding `source_capped BOOLEAN` to
`canonical_edges` or an auxiliary table), the graph arm cannot know at query time which edges came
from capped documents.

Status: The cap information is currently discarded at ingest time. Deferred to reserved-gap
31-34.

### Q9 — `use_graph_arm` flag wiring

**Call path:**

```text
Engine::search_reranked(query, filter, rerank_depth, use_graph_arm: bool)
  -> Engine::search_inner(query, filter, rerank_depth, use_graph_arm: bool)
       -> ReaderRequest::Search { ..., use_graph_arm: bool, ... }
            -> reader worker: read_search_in_tx(..., use_graph_arm: bool, ...)
                 -> if use_graph_arm { build_graph_arm_candidates(&tx, &initial_hits) }
                 -> fuse_three_arms(vector_results, text_results, graph_hits)
```

**Struct changes:**

- `ReaderRequest::Search`: add `use_graph_arm: bool` field
- `read_search_in_tx`: add `use_graph_arm: bool` parameter (11th parameter total)

**Default behavior:** `Engine::search()` -> `Engine::search_filtered()` ->
`Engine::search_reranked(query, filter, 0, false)`. Default is always `false` -> graph arm is
empty -> `fuse_rrf(v, t)` path (byte-identical to pre-Slice-30).

**Bindings:**

- PyO3 (`fathomdb-py`): add `use_graph_arm: bool = false` to `search` signature
- NAPI (`fathomdb-napi`): add `use_graph_arm: Option<bool>` to `search` signature
- Python wrapper: add `use_graph_arm: bool = False` as keyword-only arg; validate
  `isinstance(use_graph_arm, bool)`; raise `TypeError` if not bool
- TypeScript wrapper: add `useGraphArm?: boolean`; validate `typeof useGraphArm !== 'boolean'`;
  throw `TypeError`

---

## Self-Review (§3.0.R)

- **Footprint**: No network call, no subprocess, no GPU runtime in any code path (graph arm is
  pure SQLite BFS). PASS.

- **Backward compatibility**: With `use_graph_arm=false` (default), `read_search_in_tx` skips
  graph arm generation entirely. `fuse_rrf(v, t)` is called unchanged. PASS.

- **Temporal filter correctness**: The BFS SQL uses `build_bfs_with_depth_sql()` which already
  contains `e.superseded_at IS NULL AND (e.t_invalid IS NULL OR datetime(e.t_invalid) > datetime('now'))`.
  Invalidated and superseded edges are excluded. PASS.

- **RRF determinism**: Graph arm candidates are sorted by (decay_score DESC, body ASC) before
  being passed to `fuse_three_arms`. Sort is stable and deterministic. `fuse_three_arms` uses the
  same accumulation and sort as `fuse_rrf`. PASS.

- **`temporal_fallback` coverage**: SCHEMA GATE FIRED. Test `graph_arm_temporal_fallback_excluded_or_downweighted`
  is `#[ignore]`. This is explicitly deferred per §7.

- **Governed surface**: `SoftFallbackBranch::GraphArm` added; `SoftFallbackBranch` is already in
  facade re-export; all match arms updated. PASS.

- **Schema changes gated on HITL**: `temporal_fallback` storage requires schema bump (14 -> 15).
  SCHEMA GATE FIRED — escalated to orchestrator in `output.json.blockers_encountered`. `synthesized`
  detection uses `kind = "unknown"` (no schema change). `capped` warning deferred. No schema bump
  in this slice.

---

## Summary

| Item | Decision | Status |
|------|----------|--------|
| `fuse_rrf` extension | Approach B (new `fuse_three_arms`, `fuse_rrf` delegates) | IMPLEMENT |
| `RRF_WEIGHT_GRAPH` | 1.0 (conservative, no R2 data) | IMPLEMENT |
| Seed count | N = 10 | IMPLEMENT |
| BFS depth/cap | <=3 hops, cap 50 (existing constraint) | IMPLEMENT |
| Scoring formula | `decay = 1.0 / (1.0 + hop_count)`, sorted for position-RRF | IMPLEMENT |
| `SoftFallbackBranch::GraphArm` | New variant | IMPLEMENT |
| temporal_fallback | DEFERRED — schema gate (needs new column on `canonical_edges`) | BLOCKED |
| synthesized detection | `kind = "unknown"` heuristic, penalty 0.3 | IMPLEMENT |
| capped warning | DEFERRED — not persisted at ingest time | DEFERRED |
| `use_graph_arm` wiring | Full call-stack threading | IMPLEMENT |
| Schema version bump | NO (no schema change in this slice) | N/A |
