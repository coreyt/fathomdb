# 07 — Graph: edges + recursive-CTE traversal

**Component:** `canonical_edges(write_cursor, kind, from_id, to_id, source_id)`
(rewrite-era table, migration `002`) plus the `WITH RECURSIVE` traversal that
0.8.0's `neighbors()` (G5) and retrieve-and-expand (G6) will build on top of it.

**Three artifacts, do not conflate them** (all git-verified against the v0.5.0 tag):

1. **`fathom_nodes` / `fathom_edges` / `fathom_chunks`** — these literal names were
   **never a shipped table** (`git log -S "CREATE TABLE fathom_nodes"` across all
   tags = empty). They exist only as a **rejection tripwire**: `reject_legacy_shape`
   (`lib.rs:4529`) refuses to open any DB carrying them. The
   `tests/fixtures/v05_shape.sql` `fathom_nodes(id,kind,body)` is a **synthetic
   stub** that exercises that tripwire — NOT the real v0.5.0 schema. Never profile
   or design against `fathom_*`.
2. **The real v0.5.x graph layer — tables `nodes` / `edges` / `chunks`** — this
   absolutely existed and was graph-shaped (`git show v0.5.0:crates/fathomdb-schema/src/bootstrap.rs`):
   `nodes(logical_id, kind, properties, superseded_at, confidence)`,
   `edges(logical_id, source_logical_id, target_logical_id, kind, properties,
   superseded_at, confidence)` with dual-endpoint indexes, bitemporal
   `superseded_at`, query-side `traverse()` / `expand()` / `TraverseDirection`
   (`fathomdb-query/src/builder.rs:103,374`), dangling-edge detection
   (`admin.rs:864,1073`), and `restore_logical_id` + `restore_validated_edges`
   (`admin.rs:2785,4553`). **The 0.6.0 rewrite deleted this layer.**
3. **`canonical_edges`** — the rewrite-era append-only table (migration `002`) that
   0.8.0's G5/G8/G11 build on.

So G5/G8/G11 are **net-new code on `canonical_edges` that conceptually revives a
capability v0.5.x already had** (and the rewrite dropped) — not a rename, not code
reuse, not `fathom_*`. **v0.5.x is a working reference implementation**: model G8's
dangling-endpoint check on `admin.rs` dangling-edge detection, and G5/restore
semantics on `restore_validated_edges`. See `dev/profiling/v05-lineage.md`.

## Why it matters

Graph traversal is the differentiating retrieval mode (Pillar 3/4 in
`0.8.0-agent-memory-fit.md`) and the one with **no implementation yet** — so
profiling here is about establishing the *baseline that G5's design depends on*,
not measuring existing behavior. The existing `SLOW_CTE` fixture in
`lifecycle_observability.rs` is a synthetic counter, **dev-host-dependent**, and
explicitly NOT a graph benchmark — do not cite it as one.

## Ingest path — what to measure

- **Edge INSERT cost** — plain INSERT today; `from_id`/`to_id` are opaque strings,
  **never joined or validated** (no referential check until G8), and there is
  **no `from_id`/`to_id` index** (only `source_id`). So edge ingest is currently
  cheap; baseline it before G5/G8 add index maintenance + G8's per-edge EXISTS
  probe.

## Retrieval path — what to measure (the important part)

This is the G5 design input. In a **scratch copy** (never touch the real schema):

- **Index presence is decisive.** Time the `WITH RECURSIVE` neighbors walk
  **with and without** an index on `from_id` (and `to_id` for undirected/reverse
  walks). Without it, every hop is a full edge-table scan — "destroys
  performance" is the literature consensus and the whole reason G5 folds these
  indexes into G0's migration.
- **Depth scaling.** Time depth 1 / 2 / 3 separately. Branching factor compounds:
  depth d at branching b visits ~b^d nodes. Report the visited-node count, not
  just ms — a depth-3 walk on a hub node can explode.
- **`EXPLAIN QUERY PLAN`** for each depth — confirm the recursive part uses the
  index (`SEARCH … USING INDEX`) vs a `SCAN`. This is the proof G5 needs.
- **Cycle / revisit cost.** Multiple paths to a node revisit it; without a
  visited-set the CTE does redundant work. Profile a cyclic fixture and measure
  the blowup; this decides whether G5 needs path-tracking
  (`MAX_WALK_DEPTH` / `MAX_NEIGHBORS` clamps are the bounded-primitive answer).
- **Hot-node / fan-out distribution** — a few high-degree nodes dominate
  traversal cost. Report the degree distribution of the test graph so numbers are
  interpretable.

## G6 retrieve-and-expand note

G6 = run hybrid search, then per-hit bounded edge walk inside the **same
DEFERRED snapshot transaction** (`read_search_in_tx`, before `tx.commit`).
Profile the marginal cost of expanding K hits by depth-1 neighborhood on top of a
plain search — that marginal is G6's whole cost (its prerequisites G0/G1/G4/G5 are
where the real work is).

## Sharp edges

- Recursive CTEs are correct but unbounded by default — a real `neighbors()` MUST
  clamp depth and count; profile at the clamp values, not unbounded.
- Profile `canonical_edges`, never `fathom_edges`. But the real v0.5.x `nodes`/
  `edges` layer (git history) is a working reference for G5/G8 traversal +
  dangling-edge validation — read it before designing, don't reinvent.

## Scaling expectation

With the endpoint index: milliseconds at small depth, fine to ~100K nodes; "you
will feel it" at ~500K nodes / depth 6 (literature). Without the index: O(edges)
per hop — unusable. The index is not optional; the profiler exists to quantify
how much it buys and where depth makes it fall over.
