# Design: Structured Node Full-Text Projections

> **Status: Implemented** (SchemaVersion 15). The feature shipped as described
> below. Where the implementation diverged from the original design proposal,
> the divergence is marked with **[Divergence]** and explained inline. For
> consumer-facing usage guidance, see
> [docs/guides/property-fts.md](../docs/guides/property-fts.md).

## Purpose

Define how FathomDB should implement
[schema-declared full-text projections over structured node properties](./schema-declared-full-text-projections-over-structured-node-properties.md)
without turning structured entities into synthetic client-authored chunks.

The goal is to give structured node kinds a first-class engine-managed full-text
search path while preserving the current boundaries:

- canonical state remains nodes, edges, chunks, runs, steps, and actions
- derived search state remains rebuildable
- `text_search(...)` remains the user-facing query primitive
- explicit chunks remain the right model for real document ingestion

## Decision Summary

- Add a narrow engine-owned contract table for kind-level text projection
  definitions. Do not introduce a general node-schema engine.
- **[Divergence]** Add a *separate* FTS5 virtual table (`fts_node_properties`)
  for structured-node projections rather than broadening `fts_nodes`. The
  existing `fts_nodes` table remains chunk-only and unchanged. This is cleaner
  for rebuild, avoids migrating existing FTS data, and keeps the two origins
  fully independent.
- Keep `text_search(...)` as the only user-facing query operator for both
  chunk-backed and property-backed FTS.
- **[Divergence]** Instead of adding a second query operator, compile
  `text_search(...)` to a UNION over `fts_nodes` and `fts_node_properties`.
- Maintain structured-node FTS rows transactionally on node insert, upsert,
  retire, excise, and admin rebuild.
- Treat structured-node FTS rows as active-state derived data keyed by
  `node_logical_id`, not as version-history artifacts.

## Goals

- Eliminate client-side scans for structured entity search workloads.
- Let clients write only canonical structured nodes, not synthetic chunks.
- Preserve rebuild, export, repair, and recovery discipline for derived search
  state.
- Reuse the existing FTS query surface instead of adding a second text-search
  API.
- Keep the implementation narrow enough for a first slice.

## Non-Goals

- A general node schema-validation engine.
- Arbitrary runtime property search over undeclared JSON paths.
- Per-field weighting in v1.
- Highlighting, snippets, field match reporting, or field-scoped query syntax.
- History-aware search over superseded structured-node versions.
- Replacing chunks for document ingestion.

## Current State

Prior to this feature, `text_search(...)` was the only FTS query path:

- drives from `fts_nodes`
- `fts_nodes` is populated only from `chunks`
- structured node properties were invisible to full-text search

## Design Overview

Implement the feature in two layers:

1. **Kind-level contract**
   - declare which JSON property paths contribute searchable text for a node
     kind

2. **Derived FTS document maintenance**
   - populate a *separate* FTS index (`fts_node_properties`) whose source is
     structured-node text projections
   - the existing `fts_nodes` index remains chunk-only

## Contract Storage

FathomDB does not currently have a general schema registry for node kinds. The
implementation adds a narrow contract table rather than trying to generalize
`schema_json` from the operational store.

### **[Divergence]** Table: `fts_property_schemas` (was `node_kind_text_projections`)

The implemented table name is `fts_property_schemas`, not
`node_kind_text_projections`. The name aligns with the `fts_` prefix convention
used by the other FTS tables and makes the purpose immediately clear.

```sql
CREATE TABLE IF NOT EXISTS fts_property_schemas (
    kind TEXT PRIMARY KEY,
    property_paths_json TEXT NOT NULL,
    separator TEXT NOT NULL DEFAULT ' ',
    format_version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
```

Contract rules:

- `kind` names a node kind
- `property_paths_json` is an ordered JSON array of JSON paths (each must start
  with `$.`)
- `separator` controls how extracted values are concatenated (default: space)
- only simple dot-notation paths are supported in v1, such as `$.title` and
  `$.payload.summary_text`
- malformed paths and duplicates are rejected at registration time
- an empty path list is rejected
- **[Divergence]** No `updated_at` or `disabled_at` columns. Updates are
  idempotent upserts via `register_fts_property_schema`. Removal is a separate
  `remove_fts_property_schema` operation that deletes the row (it does NOT
  delete FTS rows; a rebuild is required).

Example stored payload:

```json
["$.title", "$.description", "$.rationale"]
```

## FTS Storage Shape

### **[Divergence]** Separate `fts_node_properties` table (not a modified `fts_nodes`)

The original design proposed broadening `fts_nodes` with `origin` and
`origin_id` columns. The implementation instead creates a completely separate
FTS5 virtual table:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS fts_node_properties USING fts5(
    node_logical_id UNINDEXED,
    kind UNINDEXED,
    text_content
);
```

**Why a separate table instead of broadening `fts_nodes`:**

- Avoids migrating the existing `fts_nodes` virtual table, which would require
  dropping and recreating it (FTS5 virtual tables cannot be ALTERed)
- Keeps chunk-origin and property-origin data fully independent for rebuild
- Simplifies delete logic: property FTS rows are always deleted by
  `node_logical_id` without needing an `origin` discriminator
- No risk of accidentally deleting chunk rows when maintaining property rows or
  vice versa

The existing `fts_nodes` table remains unchanged and continues to serve
chunk-backed `text_search(...)`.

Semantics:

- at most one active `fts_node_properties` row exists per active logical ID
- chunk-origin rows remain in `fts_nodes`, one-per-chunk

## Query Planning Changes

### `text_search(...)` — transparently covers both chunk and property FTS

The user-facing query API is unchanged: `text_search(...)` remains the only FTS
query operator. The query compiler's `FtsNodes` driving table branch now always
emits a UNION of `fts_nodes` and `fts_node_properties`, so property-derived
text is transparently searchable alongside chunk-derived text.

```sql
base_candidates AS (
    SELECT DISTINCT logical_id FROM (
        SELECT src.logical_id
        FROM fts_nodes f
        JOIN chunks c ON c.id = f.chunk_id
        JOIN nodes src ON src.logical_id = c.node_logical_id
            AND src.superseded_at IS NULL
        WHERE fts_nodes MATCH ?1 AND src.kind = ?2
        UNION
        SELECT fp.node_logical_id AS logical_id
        FROM fts_node_properties fp
        JOIN nodes src ON src.logical_id = fp.node_logical_id
            AND src.superseded_at IS NULL
        WHERE fts_node_properties MATCH ?3 AND fp.kind = ?4
    )
    LIMIT {base_limit}
)
```

No separate `PropertyTextSearch` query step exists. Clients do not need to know
whether a kind's text comes from chunks or property projections.

## Write-Time Maintenance

Structured-node projection rows are derived state. The engine maintains them
inside the same IMMEDIATE transaction as the canonical node mutation.

### Preparation stage

`resolve_property_fts_rows` loads all schemas once per request from the DB,
extracts property paths from co-submitted nodes using `extract_json_path`,
and concatenates with the configured separator.

`extract_json_path` supports simple dot-notation paths like `$.name`,
`$.address.city`.

Normalization rules for v1:

- include strings directly
- stringify numbers and booleans
- flatten arrays of scalars in order
- ignore objects, nested arrays, and non-scalar array elements
- skip missing and null values
- preserve declared path order first, then preserve flattened element order
- preserve explicit empty strings if present
- apply the configured separator between every extracted value

Empty-text rule:

- if no values remain after normalization, do not insert a property FTS row

### Transaction stage

Inside the write transaction:

1. **Node insert without upsert**
   - insert one `fts_node_properties` row if the kind has a registered schema

2. **Node upsert**
   - **[Divergence]** property FTS rows are ALWAYS deleted on upsert (not just
     on `ChunkPolicy::Replace`), then re-inserted if non-empty

3. **Node retire**
   - delete any `fts_node_properties` row for that `logical_id`
   - chunk FTS rows in `fts_nodes` are deleted separately as before

4. **Node excise**
   - **[Divergence]** full property FTS rebuild is performed within the same
     transaction (not just a row-level delete/insert)

5. **Chunk policy replace**
   - continues to delete only chunk-origin rows in `fts_nodes`
   - does not affect `fts_node_properties` rows (naturally isolated by separate
     table)

Because property FTS rows live in a separate table, the delete statements are
simple and cannot accidentally affect chunk rows:

```sql
DELETE FROM fts_node_properties WHERE node_logical_id = ?1;
```

## Rebuild And Repair

Structured-node FTS state is fully rebuildable from:

- active canonical nodes
- `fts_property_schemas` contracts

### Full FTS rebuild

`rebuild_property_fts` in `projection.rs`:

1. DELETE all rows from `fts_node_properties`
2. Load all property schemas
3. For each active node matching a schema kind, extract paths and INSERT

This is integrated into `rebuild_projections(ProjectionTarget::Fts)` and
`rebuild_projections(ProjectionTarget::All)`.

### Missing-projection rebuild

`rebuild_missing_property_fts_in_tx` fills gaps for nodes that have zero
property FTS rows but should have them based on their kind's schema.

### Repair notes and diagnostics

The repair report continues to return `ProjectionTarget::Fts`.

## Admin Surface

The implemented admin API has four operations:

1. `register_fts_property_schema(kind, property_paths, separator)`
   — idempotent upsert with validation (paths must start with `$.`, no
   duplicates, non-empty list)
2. `describe_fts_property_schema(kind)` → `Option<FtsPropertySchemaRecord>`
3. `list_fts_property_schemas()` → `Vec<FtsPropertySchemaRecord>`
4. `remove_fts_property_schema(kind)` — deletes the schema row; does NOT delete
   existing FTS rows (requires explicit rebuild)

**[Divergence]** from original design:

- No separate `update` operation; `register` is an idempotent upsert
- No `disable` operation; `remove` deletes the row entirely
- `register` does not implicitly rewrite existing FTS rows; callers must run
  `rebuild_projections(fts)` or equivalent

## Schema Migration

This feature ships in **SchemaVersion(15)** with an idempotent `ensure` helper.

### Migration actions

1. Create `fts_property_schemas` table
2. Create `fts_node_properties` FTS5 virtual table

**[Divergence]** The existing `fts_nodes` table is NOT modified. No migration of
existing FTS data is required.

## Cross-Surface Parity

The four admin operations are exposed across all SDK surfaces:

| Surface | Methods |
|---|---|
| Rust Engine facade | 4 methods |
| NAPI (Node.js) | 4 `#[napi]` methods accepting JSON |
| PyO3 (Python FFI) | 4 methods with `py.allow_threads()` |
| Python SDK (`_admin.py`) | 4 high-level methods with `run_with_feedback` |
| Python types (`_types.py`) | `FtsPropertySchemaRecord` dataclass |
| TypeScript `native.ts` | 4 method signatures |
| TypeScript `admin.ts` | 4 high-level methods |
| TypeScript `types.ts` | `FtsPropertySchemaRecord` type + `fromWire` |
| Admin bridge | 4 bridge commands |

## Export, Recovery, And Integrity

The `fts_property_schemas` table is canonical metadata and must be included in:

- safe export
- recovery import
- bootstrap checks

The `fts_node_properties` rows remain derived state and follow the same rules as
existing FTS projection data:

- export may include them, but recovery correctness must not depend on them
- full rebuild from canonical state and contracts must be sufficient
- logical restore must reestablish property FTS visibility for the restored
  active node before the operation is considered correct
- admin integrity and semantic checks should eventually cover property FTS drift
  explicitly, not only chunk-backed FTS drift

## WAL, Backups, Consistency, And Stress Effects

### WAL And Write Amplification

Structured-node projection maintenance adds additional FTS writes for configured
kinds:

- node insert may add one property FTS row
- node upsert deletes any prior property FTS row and inserts one new row
- node retire deletes any property FTS row

That increases WAL volume for projection-heavy workloads. This is an accepted
tradeoff, but it remains bounded:

- at most one property FTS row per active logical ID
- no synthetic chunk explosion for structured records
- all projection mutations stay in the same transaction as canonical node writes

### Backup And Safe Export

- `fts_property_schemas` is canonical metadata and must be exported
- `fts_node_properties` remains derived and rebuildable
- restore correctness must not depend on `fts_node_properties` contents
- exported databases may contain `fts_node_properties`, but replay and recovery
  procedures must remain correct if those rows are discarded and rebuilt

### Transactional Consistency

- if the canonical node write commits, the property FTS update also commits
- if the transaction rolls back, neither canonical nor derived changes remain
- property FTS rows are ALWAYS deleted on upsert (not conditional on chunk
  policy)
- node retire removes property FTS rows in the same transaction
- excise triggers a full property FTS rebuild within the same transaction
- rebuild and repair are idempotent and safe to rerun after interruption

### Stress And Performance Testing

Add stress and benchmark cases for:

- repeated upserts of the same logical IDs for projection-enabled kinds
- mixed workloads with chunk-backed document kinds and structured-projection
  kinds in the same database
- large batches of writes touching many configured kinds
- search latency under write load for `text_search(...)` against
  structured-only kinds (property FTS via UNION)
- FTS rebuild latency and WAL impact on databases with many configured
  structured kinds
- backup/export plus rebuild roundtrips
- logical restore of projection-enabled kinds after repeated retire/excise
  cycles

## Verification

Implementation should add black-box tests for:

- register projection contract rejects malformed path syntax beyond the `$.`
  prefix check
- structured-node FTS row is created on node insert for configured kind
- structured-node FTS row is replaced on node upsert
- structured-node FTS row is deleted on node retire
- structured-node FTS extraction flattens scalar arrays and ignores objects and
  nested arrays
- chunk replace does not affect property FTS rows (separate table)
- `text_search(...)` finds a structured-only node via property FTS (no chunk
  needed)
- `text_search(...)` still returns chunk-backed document nodes
- `text_search(...)` returns results from both chunks and properties in one query
  (UNION)
- full FTS rebuild restores property FTS rows
- missing-projection rebuild restores a deleted property FTS row
- logical restore reestablishes property FTS visibility for a restored node
- safe export preserves `fts_property_schemas` and does not require trusting
  `fts_node_properties`
- export/recovery preserves contracts and rebuildability

## Risks And Tradeoffs

### Risks Accepted

- write cost increases for configured kinds because projection text must be
  derived and indexed
- the UNION query adds a second FTS MATCH scan per `text_search(...)` call, even
  when no property schemas are registered (the second half returns zero rows)

### Risks Reduced

- clients stop doing repeated broad reads and local substring scans
- structured search logic becomes centralized and testable in the engine
- rebuild and repair continue to work because structured-node text remains
  derived state
- separate table avoids any risk of corrupting or migrating existing chunk FTS
  data

## Summary of Divergences from Original Proposal

| Aspect | Original Proposal | Implemented |
|---|---|---|
| Contract table name | `node_kind_text_projections` | `fts_property_schemas` |
| Contract columns | `paths_json`, `updated_at`, `disabled_at` | `property_paths_json`, `separator`, no `updated_at`/`disabled_at` |
| FTS storage | Broadened `fts_nodes` with `origin`/`origin_id` | Separate `fts_node_properties` table |
| Query surface | `text_search` covers both origins via broadened `fts_nodes` | `text_search` covers both via UNION of `fts_nodes` + `fts_node_properties` |
| Admin API | register/update/disable/describe/list (5 ops) | register/describe/list/remove (4 ops, register is idempotent upsert) |
| Upsert delete behavior | Delete only on matching origin | Always delete property FTS rows on upsert |
| Excise behavior | Not specified | Full property FTS rebuild in same transaction |
| Schema migration | Modify `fts_nodes` | No change to `fts_nodes`; new tables only |

## Bottom Line

FathomDB implements structured-node full-text projections as a contract-backed
extension of the existing projection system, using a separate
`fts_node_properties` FTS5 table rather than modifying the existing `fts_nodes`
table. The engine stores the canonical node once, derives a rebuildable FTS
document from declared properties in `fts_property_schemas`, and makes that text
transparently searchable through the existing `text_search(...)` operator. The
query compiler always produces a UNION across both FTS tables, so clients do not
need to know whether a kind's searchable text comes from chunks or properties.
