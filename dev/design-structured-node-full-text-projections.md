# Design: Structured Node Full-Text Projections

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
- Broaden the FTS projection model from "chunk rows only" to "engine-managed FTS
  documents" that may come from chunks or structured node-property projections.
- Keep `text_search(...)` as the primary query operator. Query planning should
  not require clients to choose between chunk-backed and structured-property
  search.
- Keep `property_text_search(...)` as a compatibility fallback for now, but do
  not extend it. The durable path is projection-backed `text_search(...)`.
- Maintain structured-node FTS rows transactionally on node insert, upsert,
  retire, and admin rebuild.
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

Today the repository has two relevant search paths:

1. `text_search(...)`
   - drives from `fts_nodes`
   - `fts_nodes` is populated only from `chunks`
   - the compiled SQL still joins through `chunks`

2. `property_text_search(...)`
   - is modeled as "tokenized text search over JSON-encoded node properties"
   - is planned off the `nodes` driving table
   - it does not have an engine-maintained projection contract
   - it preserves the broad-scan shape that the new feature is meant to avoid

This means structured kinds without chunks still lack a true engine-managed FTS
path.

## Design Overview

Implement the feature in two layers:

1. **Kind-level contract**
   - declare which JSON property paths contribute searchable text for a node
     kind

2. **Derived FTS document maintenance**
   - populate the FTS index with rows whose source is either:
     - an explicit chunk
     - a structured-node text projection

This lets `text_search(...)` remain a single user-facing primitive while the
engine decides what text exists for a kind.

## Contract Storage

FathomDB does not currently have a general schema registry for node kinds. The
implementation should therefore add a narrow contract table rather than trying
to generalize `schema_json` from the operational store.

### New table: `node_kind_text_projections`

```sql
CREATE TABLE IF NOT EXISTS node_kind_text_projections (
    kind TEXT PRIMARY KEY,
    paths_json TEXT NOT NULL,
    format_version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    disabled_at INTEGER
);
```

Contract rules:

- `kind` names a node kind
- `paths_json` is an ordered JSON array of JSON paths
- malformed contract payloads and invalid JSON-path syntax are rejected at
  registration time
- an empty path list is rejected
- disabled contracts remain visible for audit and migration but do not generate
  new structured FTS rows

Example stored payload:

```json
["$.title", "$.description", "$.rationale"]
```

Rationale:

- narrow, explicit, and rebuildable
- avoids inventing a broad node-kind schema surface
- fits the existing "contract table plus derived state" pattern used elsewhere

## FTS Storage Shape

The current `fts_nodes` table is chunk-centric. The feature needs FTS rows whose
source may be a chunk or a structured node projection. The cleanest path is to
retain one engine-owned FTS table but broaden its semantics from "chunk rows"
to "FTS documents."

### Revised `fts_nodes` shape

Replace the current virtual table with:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS fts_nodes USING fts5(
    origin UNINDEXED,          -- 'chunk' | 'node_projection'
    origin_id UNINDEXED,       -- chunk.id or node.logical_id
    node_logical_id UNINDEXED,
    kind UNINDEXED,
    text_content
);
```

Semantics:

- `origin='chunk'`
  - `origin_id = chunks.id`
  - `node_logical_id` is the parent node logical ID
- `origin='node_projection'`
  - `origin_id = nodes.logical_id`
  - `node_logical_id = nodes.logical_id`
  - `text_content` is derived from declared property paths

Important v1 rule:

- at most one active `node_projection` FTS row exists per active logical ID
- chunk-origin rows remain one-per-chunk

Why keep one FTS table:

- `text_search(...)` stays simple
- the compiler does not need database metadata to choose between chunk-backed
  and structured-node-backed FTS
- kinds that have both chunks and structured-node projections naturally search
  across both sources, with final node de-duplication already handled by
  `SELECT DISTINCT`

## Query Planning Changes

### Keep `text_search(...)` stable

The user-facing API should remain:

```python
db.nodes("WMKnowledgeObject").text_search("oauth token rotation", limit=25)
```

### Compiler change

The FTS driving query should no longer join through `chunks`. It should resolve
nodes directly from the FTS row's `node_logical_id`.

Current shape:

```sql
FROM fts_nodes f
JOIN chunks c ON c.id = f.chunk_id
JOIN nodes src ON src.logical_id = c.node_logical_id AND src.superseded_at IS NULL
```

Target shape:

```sql
FROM fts_nodes f
JOIN nodes src ON src.logical_id = f.node_logical_id AND src.superseded_at IS NULL
WHERE fts_nodes MATCH ?1
  AND src.kind = ?2
```

This change is required even for chunk-backed rows because the FTS table now
already carries the node logical ID.

### `property_text_search(...)`

Decision:

- keep it for backward compatibility in v1
- do not optimize or extend it
- document it as a scan-based compatibility feature
- migrate structured workloads to projection-backed `text_search(...)`

This avoids a risky API removal while making the intended engine path clear.

## Write-Time Maintenance

Structured-node projection rows are derived state. The engine should maintain
them inside the same transaction as the canonical node mutation.

### Preparation stage

Extend `prepare_write()` with a new derivation step:

- resolve active text-projection contracts for kinds mentioned in
  `request.nodes`
- for each inserted or upserted node whose kind has an active contract:
  - parse `properties`
  - extract declared paths
  - normalize values into one text blob
  - emit a prepared structured FTS row

Suggested internal type:

```rust
struct StructuredFtsProjectionRow {
    node_logical_id: String,
    kind: String,
    text_content: String,
}
```

Normalization rules for v1:

- include strings directly
- stringify numbers and booleans
- flatten arrays of scalars in order
- ignore objects unless the declared path resolves to a scalar or scalar array
- skip missing and null values
- preserve configured path order in the concatenated output

Empty-text rule:

- if all declared paths resolve to empty or missing content, do not insert a
  `node_projection` FTS row

This matches the current "derived state should not contain useless empty rows"
discipline already applied to chunks.

### Transaction stage

Inside `apply_write(...)`, add structured FTS maintenance rules:

1. **Node insert without upsert**
   - insert one `node_projection` FTS row if the kind has an active contract

2. **Node upsert**
   - delete existing `node_projection` FTS row for that `logical_id`
   - insert the newly prepared row if non-empty

3. **Node retire**
   - delete any `node_projection` FTS row for that `logical_id`

4. **Chunk policy replace**
   - continues to delete only chunk-origin rows for that `logical_id`
   - must not delete structured-node projection rows

This requires the writer to distinguish deletion by `origin`.

Suggested delete statements:

```sql
DELETE FROM fts_nodes
WHERE origin = 'node_projection' AND node_logical_id = ?1;

DELETE FROM fts_nodes
WHERE origin = 'chunk' AND node_logical_id = ?1;
```

## Rebuild And Repair

Structured-node FTS state must be fully rebuildable from:

- active canonical nodes
- active chunk rows
- `node_kind_text_projections` contracts

### Full FTS rebuild

Update `ProjectionService::rebuild_projections(ProjectionTarget::Fts)` to:

1. start `IMMEDIATE` transaction
2. delete all rows from `fts_nodes`
3. repopulate chunk-origin rows
4. repopulate structured-node projection rows
5. commit

Implementation note:

- chunk-origin rows can still be rebuilt with one SQL `INSERT ... SELECT`
- structured-node projection rows should be rebuilt via Rust helper logic using
  the same extraction code path as write preparation

Do not try to express arbitrary path extraction solely in SQLite JSON1 for v1.
The Rust helper is easier to reason about, easier to test, and guarantees the
same normalization behavior between write-time derivation and rebuild.

### Missing-projection rebuild

Extend `rebuild_missing_projections()` so it can insert missing rows for both
origins:

- missing chunk-origin row:
  - active chunk exists
  - no `fts_nodes` row with `origin='chunk'` and matching `origin_id`

- missing node-projection row:
  - active node exists for a kind with active contract
  - derived projection text is non-empty
  - no `fts_nodes` row with `origin='node_projection'` and matching `origin_id`

### Repair notes and diagnostics

The repair report can continue to return `ProjectionTarget::Fts`.
The report should not split chunk vs structured counts in v1, but admin
diagnostics should eventually expose:

- number of configured node-kind contracts
- number of active structured-node FTS rows
- per-kind counts during inspection/debugging

## Admin Surface

Add a narrow admin API for text projection contracts.

Suggested operations:

1. `register_node_kind_text_projection(kind, paths_json)`
2. `update_node_kind_text_projection(kind, paths_json)`
3. `disable_node_kind_text_projection(kind)`
4. `describe_node_kind_text_projection(kind)`
5. `list_node_kind_text_projections()`

Behavior rules:

- registration validates JSON-path syntax and rejects duplicates
- update is explicit; no silent overwrite
- register/update does not implicitly rewrite existing FTS rows
- callers must run `rebuild_projections(fts)` or an equivalent targeted rebuild
  before expecting historic nodes of that kind to be searchable through the new
  contract

This mirrors the operational-store pattern where contract updates and rebuilds
are separate, explicit steps.

## Schema Migration

This feature needs one schema migration.

### Migration actions

1. add `node_kind_text_projections`
2. replace the `fts_nodes` virtual table definition with the broadened schema
3. repopulate `fts_nodes` from existing chunk rows during migration or rely on
   the standard rebuild path immediately after bootstrap

Recommended migration discipline:

- migrate the virtual table shape
- leave population to the existing rebuild path rather than performing a large
  data copy inside bootstrap

That keeps migration logic smaller and preserves the existing projection-repair
model.

## Export, Recovery, And Integrity

The new contract table is canonical metadata and must be included in:

- safe export
- recovery import
- bootstrap checks

The structured-node FTS rows remain derived state and should follow the same
rules as existing FTS projection data:

- export may include them, but recovery correctness must not depend on them
- full rebuild from canonical state and contracts must be sufficient

Integrity/admin checks should eventually be aware of contract drift scenarios:

- contract exists but projected rows are absent
- projected rows exist for a disabled contract
- duplicate active `node_projection` rows for one logical ID

These checks can follow after the core write/query/rebuild slice.

## WAL, Backups, Consistency, And Stress Effects

This feature adds more derived-state work to the canonical write and repair
paths. The design therefore needs explicit discipline for secondary effects, not
just query semantics.

### WAL And Write Amplification

Structured-node projection maintenance adds additional FTS writes for configured
kinds:

- node insert may add one `node_projection` FTS row
- node upsert may delete one prior `node_projection` row and insert one new row
- node retire may delete one `node_projection` row

That increases WAL volume for projection-heavy workloads. This is an accepted
tradeoff, but it should remain bounded:

- at most one structured-node FTS row per active logical ID
- no synthetic chunk explosion for structured records
- all projection mutations stay in the same transaction as canonical node
  writes, so WAL growth is proportional to a bounded number of extra rows per
  affected node

This feature should not change the engine's single-writer model or checkpoint
discipline. It does increase per-write WAL traffic, so benchmark and stress
coverage should explicitly measure whether large upsert workloads produce
unacceptable WAL growth or degraded checkpoint behavior.

### Backup And Safe Export

The backup/export model should stay unchanged:

- `node_kind_text_projections` is canonical metadata and must be exported
- `fts_nodes` remains derived and rebuildable
- restore correctness must not depend on `fts_nodes` contents being present or
  perfectly synchronized

This means backup/restore testing must cover both cases:

- backup includes `fts_nodes` and restore preserves searchable behavior
- `fts_nodes` is missing or stale after recovery and `rebuild_projections(fts)`
  restores correct search behavior from canonical state plus contracts

### Transactional Consistency

The engine must preserve the same atomicity rule it already applies to required
projections:

- if the canonical node write commits, the required structured-node FTS update
  must also commit
- if the transaction rolls back, neither canonical nor derived changes may
  remain visible

Required consistency properties:

- no active node should become searchable through stale superseded projection
  text after an upsert
- `ChunkPolicy::Replace` must not accidentally remove structured-node
  projection rows
- node retire must remove structured-node projection rows in the same
  transaction
- rebuild and repair must be idempotent and safe to rerun after interruption

### Stress And Performance Testing

This feature needs explicit workload coverage in addition to unit tests.

Add stress and benchmark cases for:

- repeated upserts of the same logical IDs for projection-enabled kinds
- mixed workloads with chunk-backed document kinds and structured-projection
  kinds in the same database
- large batches of writes touching many configured kinds
- search latency under write load for `text_search(...)` against structured-only
  kinds
- FTS rebuild latency and WAL impact on databases with many configured
  structured kinds
- backup/export plus rebuild roundtrips on databases with both chunk and
  structured FTS origins

Success criteria should include:

- no client-side scan path needed for structured retrieval
- no projection drift after stress runs
- no WAL/checkpoint regressions beyond accepted write amplification
- rebuild restores searchability deterministically after projection loss

These tests belong alongside the existing performance and client-workload
coverage, not only in narrow writer unit tests.

## API And SDK Surface

### Query API

No new query method is required for v1.

- keep `text_search(query, limit)` as the main text retrieval primitive
- keep `property_text_search(query, limit)` as compatibility-only

### Write API

No new write shape is required. Clients continue to write ordinary nodes.

### Admin/metadata API

Expose the contract registration methods through:

- Rust facade
- Python `engine.admin`
- TypeScript `engine.admin`

The SDKs should treat `paths_json` as a structured value where convenient, but
the engine contract should remain simple and JSON-serializable.

## Rollout Plan

### Phase 1: Storage And Rebuild Backbone

- add `node_kind_text_projections`
- migrate `fts_nodes` to the broadened document shape
- update FTS rebuild to repopulate chunk-origin rows in the new shape
- add structured-node rebuild from contracts

### Phase 2: Transactional Write Maintenance

- extend `prepare_write()` with structured-text derivation
- update writer FTS maintenance to insert/delete by `origin`
- add retire/upsert coverage

### Phase 3: Query Compiler Alignment

- remove the `chunks` join from the FTS driving SQL
- verify `text_search(...)` works for:
  - chunk-only kinds
  - structured-only kinds
  - kinds with both sources

### Phase 4: Admin Surface And Deprecation Messaging

- add register/update/describe/list admin APIs
- document `property_text_search(...)` as compatibility-only
- add docs showing structured-node search through `text_search(...)`

## Verification

Implementation should add black-box tests for:

- register projection contract rejects malformed path JSON
- structured-node FTS row is created on node insert for configured kind
- structured-node FTS row is replaced on node upsert
- structured-node FTS row is deleted on node retire
- chunk replace deletes only chunk-origin rows, not node-projection rows
- `text_search(...)` returns a structured-only node without any chunk
- `text_search(...)` still returns chunk-backed document nodes
- `text_search(...)` over a kind with both sources de-duplicates node results
- full FTS rebuild restores both chunk-origin and structured-node rows
- missing-projection rebuild restores a deleted structured-node row
- export/recovery preserves contracts and rebuildability

## Risks And Tradeoffs

### Risks Accepted

- write cost increases for configured kinds because projection text must be
  derived and indexed
- `fts_nodes` semantics become broader than the old chunk-only assumption
- kinds with both chunk and structured sources may produce denser candidate sets

### Risks Reduced

- clients stop doing repeated broad reads and local substring scans
- structured search logic becomes centralized and testable in the engine
- rebuild and repair continue to work because structured-node text remains
  derived state

## Open Questions

1. Should contract updates automatically trigger a targeted rebuild, or should
   rebuild remain explicit?
2. Should v1 add a tiny helper to validate that declared JSON paths resolve to
   scalar leaves on sample data, or is syntax validation enough?
3. Should future ranking favor chunk-origin or node-projection origin when both
   match the same node?
4. Should admin diagnostics expose per-origin row counts from day one?
5. Should `property_text_search(...)` eventually compile to `text_search(...)`
   for configured kinds, or simply be removed after migration?

## Recommended Minimal First Version

Ship the narrowest useful slice:

- one contract table: `node_kind_text_projections`
- broadened `fts_nodes` with `origin` and `origin_id`
- transactional structured-node FTS maintenance on node write and retire
- FTS rebuild support for both chunk and structured origins
- `text_search(...)` updated to resolve directly from `fts_nodes.node_logical_id`
- no per-field weighting, no field match reporting, no new user query API

## Bottom Line

FathomDB should implement structured-node full-text projections as a
contract-backed extension of the existing projection system, not as client-side
synthetic chunking. The engine stores the canonical node once, derives a
rebuildable FTS document from declared properties, and exposes that text
through the existing `text_search(...)` path.
