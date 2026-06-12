# ADR-0.8.1 — Graph Substrate G11 Migration: edge enrichment + projectability

> **Status:** ACCEPTED — pending HITL sign-off (Slice-0 gate).
> **Activates:** H3 reservation from ADR-0.8.0-graph-model-and-edge-addressing.md (HITL-signed 2026-06-05).
> **Schema:** SCHEMA_VERSION 13 → 14 (step-14 additive ALTER TABLE canonical_edges).
> **Implements at:** Slice 15 (keystone). Gates Slices 20 + 30.

---

## 1. Context

### 1.1 The H3 reservation

`ADR-0.8.0-graph-model-and-edge-addressing.md` §5 ("What to RESERVE NOW") carried a
**prose reservation** (H3), HITL-signed 2026-06-05:

> Reserve edge-enrichment columns (`body`/`text`, `valid_at`/`invalid_at`, `confidence`) as
> additive-now in the substrate ADR's data model — a prose reservation, not a column.

H3 was signed as a prose-only reservation because adding the columns in 0.8.0 would have been
premature — the schema change is reserved-additive pending the 0.8.1 BYO-LLM ingest API that
writes the columns. This ADR **activates** H3: it makes the prose reservation concrete by
specifying the exact schema step, column names, and the Slice 15 implementation contract.

### 1.2 Current canonical_edges schema (SCHEMA_VERSION = 13)

At SCHEMA_VERSION 13 (after step-13 adds the `operational_mutations(collection_name,id)` index),
`canonical_edges` has the following columns:

```
write_cursor INTEGER NOT NULL
kind         TEXT NOT NULL
from_id      TEXT NOT NULL
to_id        TEXT NOT NULL
source_id    TEXT
logical_id   TEXT
superseded_at INTEGER
```

**What is missing for the Graphiti-shaped memory ontology (the three "cracks" from the graph-model ADR §4.2):**

1. Edges have **no `body`** — a fact-edge's text and embedding have nowhere to live
2. Edges have **no valid-time/confidence** — point-in-time recall and confidence-weighted
   retrieval are not expressible
3. **No per-fact semantic search** — only node `body` is projected to vector/FTS; an edge
   cannot be embedded today

G11 closes all three cracks with additive nullable columns and an edge projectability seam.

---

## 2. Decision

### 2.1 Column name authority

The canonical FathomDB column names for the valid-time pair are **`t_valid`** and **`t_invalid`**.

The graph-model ADR (§5) notes: *"The valid-time pair `valid_at`/`invalid_at` used here for
Graphiti alignment is the **same** reserved bi-temporal pair the substrate ADR names canonically
as `t_valid`/`t_invalid` (Decision 1); the substrate ADR is the schema-contract authority for
the column names."*

Graphiti's `valid_at`/`invalid_at` are **aliases for the consumer's understanding only** — they
map to `t_valid`/`t_invalid` at the ingest boundary. The BYO-LLM protocol (`fathomdb.extract.v1`)
uses the same `t_valid`/`t_invalid` names in the extract response `edges[]` entries, so no
name translation is needed at the ingest mapping layer.

### 2.2 The four G11 columns

| Column | Type | Semantics | Nullable |
|--------|------|-----------|----------|
| `body` | `TEXT` | The fact/relationship text — the `body` from the BYO-LLM extract response `edges[].body` | YES — NULL for pre-G11 rows |
| `t_valid` | `TEXT` | Event valid-time: ISO-8601 timestamp when the fact *became true* (event time, not ingestion time). NULL = "still valid" or "unknown" | YES |
| `t_invalid` | `TEXT` | Event invalid-time: ISO-8601 timestamp when the fact *stopped being true*. NULL = "still valid" | YES |
| `confidence` | `REAL` | Calibrated extraction confidence ∈ [0.0, 1.0]. NULL for pre-G11 rows and when the harness does not provide a confidence score | YES |

All four columns are **nullable**. Pre-G11 rows read NULL for all four columns — no existing
query breaks, no data migration is required, and no accretion marker is needed (additive
`ALTER TABLE ADD COLUMN` is per the migration policy for additive column additions).

### 2.3 Schema step-14 — the exact SQL

```sql
-- step-14 (G11 edge enrichment; SCHEMA_VERSION 13→14)
ALTER TABLE canonical_edges ADD COLUMN body TEXT;
ALTER TABLE canonical_edges ADD COLUMN t_valid TEXT;
ALTER TABLE canonical_edges ADD COLUMN t_invalid TEXT;
ALTER TABLE canonical_edges ADD COLUMN confidence REAL;
```

Four separate `ALTER TABLE` statements. `SCHEMA_VERSION` bumps **13 → 14** when step-14 is
applied. This is an **additive-only, accretion-exempt** migration step: all columns are nullable,
all pre-existing rows read NULL, no index changes, no data backfill required.

---

## 3. Edge projectability

### 3.1 Why edge bodies must be searchable

The graph-model ADR R8 requirement: "A fact (corpus relationship or memory fact-edge) should be
embeddable/FTS-able, not only traversable." The graph arm (R3, Slice 30) generates candidates by
projecting edge `body` text into FTS + vector, then fusing as a third RRF arm. Without
projectability, the graph arm cannot contribute candidates to retrieval.

### 3.2 FTS5 projection

Edge `body` text is projected into **`search_index`** (FTS5) with `source_type = 'edge_fact'`
as the partition discriminant. This makes fact-edges semantically searchable via the existing FTS
infrastructure, with the `source_type` column available for partition-aware queries (e.g. retrieve
only edge facts, or combine edge facts with node bodies in a single RRF fusion).

### 3.3 Vector projection

Edge `body` text is projected into **`vector_default`** (sqlite-vec 1-bit embedding) with the
same `source_type = 'edge_fact'` partition. This enables approximate nearest-neighbor semantic
search over fact-edge text via the same KNN infrastructure FathomDB uses for node bodies.

### 3.4 Projection seam contract

The projection seam must accept an edge source (not only a node source). Slice 15 implements
both projections as part of the BYO-LLM ingest path: when an `edges[]` entry arrives with a
non-null `body`, the engine writes the `canonical_edges` row **and** inserts corresponding rows
into `search_index` and `vector_default` with `source_type = 'edge_fact'`.

### 3.5 Source discriminant

`source_type = 'edge_fact'` is the partition discriminant for edge-body entries in both indexes.
This distinguishes edge-body entries from node-body entries (`source_type = 'node'`) in filtered
KNN queries and FTS partition scans.

---

## 4. Invalidate-not-accumulate contract

### 4.1 Semantics

When a new BYO-LLM ingest supersedes a prior fact-edge (same `from_id`, `to_id`, `kind`/relation,
overlapping temporal scope), the ingest API:

1. **Sets `superseded_at`** on the prior active row (tombstone) — does NOT delete or update the
   `body` or any other column; history is preserved
2. **Inserts the new enriched row** as the active fact-edge (with fresh `write_cursor`, full G11
   columns populated)

The prior row remains queryable for historical/point-in-time queries. Only the new row has
`superseded_at IS NULL` (active).

### 4.2 Invalidation scope

The invalidation check is on `(from_id, to_id, kind)` — the same tuple the hybrid upsert ADR
(H2, future) would use for natural-key ergonomics. For temporal disambiguation:

- When `t_invalid` is known from the extract response (the harness signals "this fact stopped
  being true"), the engine sets `t_invalid` on the **new row** (not on the prior row; the prior
  row's `t_invalid` was already NULL = "still valid at the time of prior ingest")
- When `t_invalid` is NULL (still valid), the prior row is tombstoned via `superseded_at`
- The temporal filter at query time: `superseded_at IS NULL AND (t_invalid IS NULL OR t_invalid > now)`

### 4.3 Provenance

The `source_id` column (pre-G11, already present) carries the `source_doc_id` from the extract
response. The G11 `confidence` column carries the harness's calibrated confidence. Both are
preserved on all rows (active and superseded) as provenance.

---

## 5. What this gates

### 5.1 Slice 15 (keystone) implements

- Step-14 migration (`ALTER TABLE canonical_edges ADD COLUMN …` × 4)
- `SCHEMA_VERSION` bump 13 → 14
- BYO-LLM ingest API: spawn + handshake + extract dispatch + entities→nodes + edges→enriched
  `canonical_edges` + invalidate-not-accumulate bookkeeping
- FTS5 projection of edge `body` with `source_type = 'edge_fact'`
- Vector (1-bit) projection of edge `body` with `source_type = 'edge_fact'`
- Conformance fixture (engine-side golden-input → expected-output)

### 5.2 Slices 20 + 30 depend on this

- **Slice 20** (G5/G6 graph traversal): the traversal filter `t_invalid IS NULL OR t_invalid > now`
  requires the `t_invalid` column to exist; the `kind`-filtered BFS uses `canonical_edges(from_id)/
  (to_id)` — the indexes already exist from step-12/13; no new index is needed here
- **Slice 30** (R3 graph arm): projects edge bodies as a third RRF arm; requires both the
  `body` column (to project) and the FTS+vector projections (to retrieve candidates)

**These slices MUST NOT open until Slice 15 is merged.**

---

## 6. Falsifiable acceptance bar (Slice 15 tests)

1. **Columns present**: `PRAGMA table_info(canonical_edges)` shows `body`, `t_valid`, `t_invalid`,
   `confidence` after step-14
2. **Legacy rows NULL-safe**: `SELECT * FROM canonical_edges WHERE superseded_at IS NULL` succeeds
   on a pre-G11 DB after applying step-14 (all four new columns read NULL for pre-existing rows)
3. **SCHEMA_VERSION**: `PRAGMA user_version` returns 14 after step-14
4. **Additive safety**: no existing test that reads `canonical_edges` breaks after step-14
5. **Edge FTS searchable**: an edge with `body = "Alice owns the project"` is retrievable via
   `search_index MATCH 'project'` with `source_type = 'edge_fact'`
6. **Edge vector searchable**: the same edge body produces a vector entry in `vector_default`
   with `source_type = 'edge_fact'` and is returned by a KNN query
7. **Invalidate-not-accumulate**: ingesting a superseding fact-edge tombstones the prior
   active edge (`superseded_at` set, `body`/`t_valid` preserved on the prior row) and inserts
   the new enriched row as the single active row for `(from_id, to_id, kind)`

---

## 7. Explicitly deferred / not in this ADR or Slice 15

- **Hybrid `(from_id, to_id, kind)` upsert ergonomics** (H2) — future write-API ADR; opaque-id
  remains the 0.8.1 addressing model
- **Traversable provenance edges + episode tier** (H4) — `source_id` scalar carries provenance
  in 0.8.1; traversable provenance is a future ADR
- **Reified fact-nodes** (H6) — the documented n-ary escape hatch; adopt on n-ary demand
- **G4↔G10 filter unification** (reserved-gap 37, Slice 35) — grammar work, not schema

---

## 8. References

- H3 reservation: `dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md` §5 "What to RESERVE NOW"
- Column name authority: `dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md` Option 2A
  (:135-159,177-196) — bi-temporal column shape certified additive; `t_valid`/`t_invalid` are
  the canonical names
- Current schema (step-13): `src/rust/crates/fathomdb-schema/src/lib.rs`
- v0.5.6 portability proof: `dev/profiling/v05-lineage.md:14-48` (v0.5.6 carried `confidence REAL`
  on edges — fact-on-edge enrichment is proven portable in this codebase)
- BYO-LLM extract response (column-to-field mapping): `dev/adr/ADR-0.8.1-byo-llm-extraction-protocol.md` §3.4
- Graph traversal scope: `dev/adr/ADR-0.8.0-graph-traversal-scope.md` (valid-time filter
  `t_invalid IS NULL OR t_invalid > now` at traversal time, Slice 20)
- IR-C graph arm (R3): `dev/plans/runs/IR-C-roadmap.md` §R3 (edge body FTS+vector projection,
  third RRF arm)
- 0.8.1 slice contracts: `dev/plans/0.8.1-implementation.md` (Slice 15 keystone, Slice 20, Slice 30)
