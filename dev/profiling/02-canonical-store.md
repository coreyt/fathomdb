# 02 — Canonical store (`canonical_nodes` / `canonical_edges`)

**Component:** the durable ground-truth tables.
`canonical_nodes(write_cursor, kind, body, source_id)` and
`canonical_edges(write_cursor, kind, from_id, to_id, source_id)` (migrations
`002_canonical.sql` + `008_source_id.sql`). Today indexed only on `source_id`.

## Why it matters

This is the source of truth every projection derives from and every read
hydrates from. On retrieval, the vector and text branches return rowids/cursors
and then **fetch `body` from `canonical_nodes`** — so canonical-store read cost
is inside the query budget even though it's "just a lookup."

## Ingest path — what to measure

- **Plain INSERT cost** per node/edge row (cheap, B-tree append on rowid).
- **`source_id` index maintenance** — the only secondary index today; its
  upkeep is the only non-trivial per-row write cost. Measure with vs without
  provenance set.
- **Row-count growth vs file growth** — snapshot `SELECT count(*)` per table at
  each N checkpoint and correlate with on-disk size (page growth feeds the page
  cache and WAL — see `11-sqlite-pragmas.md`).
- **No `kind` index today** — G4/G5 will add `canonical_nodes(kind)` and
  `canonical_edges(from_id/to_id)`. Baseline the index-free INSERT now so the
  added index-maintenance cost is attributable when those land.

## Retrieval path — what to measure

- **Body-fetch by cursor** — `SELECT body FROM canonical_nodes WHERE write_cursor
  = ?` once per hit, inside `read_search_in_tx`. With `final_limit = 10` this is
  ~10 point lookups; small, but measure it as its own stage (it's separable from
  the vec0/FTS5 scan).
- **The hit→record gap** — there is no `logical_id` / by-id read at 0.7.2
  (G0/G2). Profiling the body-fetch now gives the baseline for G2's `get(id)` and
  G1's structured-hit hydration.

## Sharp edges

- `body` is the embedded + FTS-indexed text *and* the JSON property bag (no
  separate `properties` column). Large bodies inflate FTS5 segment size
  (`03-fts5.md`) and embed time (`05-embedder.md`) — body size is a hidden axis;
  record body-length distribution alongside row counts.
- `from_id`/`to_id` are opaque strings, never joined/validated at 0.7.2 — edge
  INSERT cost today excludes any referential check (G8 adds it).

## Scaling expectation

INSERT and point-lookup are ~O(log N) on the rowid/index B-trees — rarely the
bottleneck. This component's profiling value is mostly as the **denominator**
(rows per layer) for normalizing the expensive layers, and as the G0/G2/G4/G5
baseline.
