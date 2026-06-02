# v0.5.x lineage — what the rewrite dropped, what 0.8.0 revives

> Git-verified against the `v0.5.0` tag (2026-06-01). Corrects an earlier error in
> this repo's notes that claimed "no graph-shaped layer ever existed." It did.

## Three artifacts, never conflate them

| Artifact | What it is | Status today |
|---|---|---|
| **`fathom_nodes` / `fathom_edges` / `fathom_chunks`** | Literal names that were **never a shipped table** (`git log -S "CREATE TABLE fathom_nodes"` = empty across all tags). | Rejection tripwire only — `reject_legacy_shape` (`lib.rs:4529`) refuses to open a DB carrying them. `tests/fixtures/v05_shape.sql` is a synthetic stub, NOT the real schema. |
| **`nodes` / `edges` / `chunks`** (the **real** v0.5.x graph layer) | A full document + graph + KV store. Deleted by the 0.6.0 rewrite. | Lives in git history (`v0.5.0`) — a working reference implementation. |
| **`canonical_nodes` / `canonical_edges`** | Rewrite-era append-only tables (migration `002`). | What 0.8.0 G0/G5/G8/G11 build on. |

## What the real v0.5.x layer actually had (verified)

- **Graph schema** (`fathomdb-schema/src/bootstrap.rs`):
  `nodes(logical_id, kind, properties BLOB, superseded_at, confidence)`,
  `edges(logical_id, source_logical_id, target_logical_id, kind, properties,
  superseded_at, confidence)` — dual-endpoint indexes
  (`idx_edges … (source_logical_id, kind, superseded_at)` and the target mirror),
  partial-unique active index, **bitemporal `superseded_at`**, and a **per-fact
  `confidence REAL`** column on both nodes and edges. ~36 tables total in bootstrap
  (vs 15 in 0.7.2).
- **Graph traversal verbs** (`fathomdb-query/src/builder.rs:103,374`):
  `traverse(direction, kind, depth)`, `expand(...)`, `TraverseDirection::{Out,In}`.
- **By-id read in the query builder**: `filter_logical_id_eq` (`builder.rs:120`),
  surfaced through the SDK (napi `filter_logical_id_eq`).
- **Rich JSON-path filter DSL**: typed int/timestamp `gt/gte/lt/lte`, bool,
  fused-secondary-index predicates (`builder.rs` JsonPath* predicates).
- **Grouped / aggregation queries**: `compile_grouped_query`
  (`compile.rs:661`), `execute_compiled_grouped_query`, grouped-query read tests.
- **Referential integrity**: dangling-edge detection (`admin.rs:864,1073` —
  "active edge(s) with missing endpoint node") and
  **`restore_validated_edges`** (`admin.rs:4553`) called from `restore_logical_id`
  (`admin.rs:2785`) — restore-with-endpoint-validation.
- **Operational-collection lifecycle**: register / validate / secondary-indexes /
  retention / compact / trace / read — 15+ `admin.rs` verbs;
  `operational_secondary_index_entries`, `operational_retention_runs`,
  `operational_filter_values` tables.
- **FTS property schemas**: schema-declared full-text projections over node
  properties (`fts_property_schemas`, `fts_node_property_positions`; SchemaVer 15;
  `dev/schema-declared-full-text-projections-over-structured-node-properties.md`).
- **Per-kind FTS/vec profile config + tokenizer presets**
  (`set_fts_profile`, `resolve_tokenizer_preset`).
- **In-process admin/maintenance API** (SDK, not CLI):
  `regenerate_vector_embeddings`, `rebuild_projections`, `safe_export`,
  logical-id restore/purge as SDK calls.

## The 0.6.0 rewrite stance

0.6.0 deliberately stripped all of the above to a **5-verb retrieval engine**
(`open/write/search/close/admin.configure`, AC-057a). That was an intentional
scope reset to get the substrate + perf right first — not a capability the project
forgot it had. See `dev/design/0.8.0-agent-memory-fit.md` §5.

## What 0.8.0 revives vs leaves dropped

**Revived (subset):** identity/supersession (G0), structured search hits +
attribution (G1, restored as the knowledge-store anchor), metadata-filtered KNN
(G10), and — *gated/partial* — graph traversal (G5), dangling-edge validation
(G8), bitemporal edges (G11), op-store read (G3).

**0.8.0 goes BEYOND v0.5.x in one place:** RRF score fusion + rerank (G9).
v0.5.x `fusion.rs` was **filter-fusion** (partitioning `Filter` predicates into
fusable-into-the-search-SQL vs residual `WHERE`), *not* score/rank fusion — it had
no RRF/MMR. Verified: `fusion.rs` header is "Filter-fusion helpers."

**Left dropped at 0.8.0** (v0.5.x had, 0.8.0 will not): graph traversal as a
shipped SDK verb (G5/G6 are explicit ADR non-goals — gated on AC-057a), by-id SDK
read (G2 gated), the rich typed JSON-path filter DSL (0.8.0 commits only a "small
filter grammar"), full operational-collection governance (register/secondary-
index/retention/compact — only G3 read at most), schema-declared FTS property
projections, per-kind FTS/vec tokenizer presets, in-process admin/maintenance API
(CLI-only now), grouped/aggregation queries, and per-fact `confidence`
(G12 importance is design-tier and different).

## How to use this in 0.8.0 work

Per-feature disposition + porting note (full triage:
`dev/design/0.8.0-v05-feature-triage.md`):

| Feature | v0.5.x ref | 0.8.0 disposition | Porting note |
|---|---|---|---|
| Structured hits (SearchHit) | `search.rs` / engine | **ADD (G1)** | Reshape BOTH vector (`lib.rs:3247-3250`) + FTS (`3296-3305`) branches; drop `Eq` on `SearchResult` |
| By-id (`Predicate::LogicalIdEq`) | `builder.rs:120` | **ADD (G2)** after G0 | `ReaderRequest::GetById`; point lookup on `logical_id` |
| Op `read_collection`/`trace` | `admin.rs:1651-1754` | **ADD READ (G3)** | Read-shape oracle only; author new cursor SELECTs; gate SDK on ADR-supersede |
| Dangling-edge / `restore_validated_edges` | `admin.rs:864-875,4553` | **ADD (G8)** after G0 | Flag-and-count default; index-back on `(logical_id,kind)` |
| `superseded_at` + partial-unique-active idx | `bootstrap.rs` | **ADD (G0)** | Port idx shape; RESERVE valid-time cols (v0.5.x has none) |
| `traverse`/`expand` recursive CTE | `builder.rs:103,374`; `compile.rs:~523-560` | **DEFER 0.8.x (G5/G6)** | Re-target `source/target_logical_id` → `from_id/to_id`; SDK depth ≤3 |
| JSON-path `Predicate` CORE | `ast.rs:83`; `builder.rs:167-266` | **DEFER 0.8.x (G4 core)** | Closed enum; EXCLUDE fused + `_unchecked` (`279-354`) |
| Grouped query | `compile.rs:661` | **DEFER 0.8.x — fold into G6** | Not aggregation; retrieve+expand fan-out only |
| `confidence REAL` | `bootstrap.rs` | **DEFER 0.8.x (F9)** | NOT a column; vec0 reshape + decay policy ADR |
| `fts_property_schemas` | `bootstrap.rs` (SchemaVer 15) | **DEFER 0.8.x — RESHAPED** | Use BM25F named columns; drop per-kind tables + position sidecar |
| `set_fts_profile` / per-kind tokenizer presets | `admin.rs:389,477-530` | **DROP (F6)** | Per-kind surface dropped. KEEP the *global* default `porter unicode61 remove_diacritics 2` (zero-surface, not F6) |
| in-process AdminClient | `admin.rs` | **DROP (F7)** | Stays CLI-only (doctor/recover); seams already exist |

**Invariant note:** `search_index`/vec0 are DERIVED projections — recreating the
FTS5 virtual table + rebuild (`lib.rs:58-59`, `rebuild_projections:2635`) is the
EXISTING repair pattern and does NOT violate no-data-migration (which governs
SOURCE records per `ADR-0.6.0-json-schema-policy.md:51`). This is why the F6
global-tokenizer-default upgrade is cleanly doable in 0.8.0.

Read these from git (`git show v0.5.0:<path>`); none of it is in the working tree.
