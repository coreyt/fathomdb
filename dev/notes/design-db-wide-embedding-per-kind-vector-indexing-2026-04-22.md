# Design: Database-Wide Embedding With Per-Kind Vector Indexing

**Date:** 2026-04-22
**Status:** Draft
**Motivation:** Memex vector projection wiring
**Decision:** One database-wide embedding engine; per-kind vector indexing and
text source configuration; FathomDB-owned async/incremental vector projection.

---

## Problem

Memex wants vector search to behave like a managed FathomDB projection, similar
to FTS:

1. Memex writes canonical data.
2. FathomDB persists the canonical node/chunk state.
3. If a kind is configured for FTS, FathomDB updates the FTS projection.
4. If a kind is configured for vector indexing, FathomDB updates the vector
   projection.

Today vector projection does not behave that way. Opening an engine with
`embedder="builtin"` enables the read-time query embedder, but ordinary writes
do not automatically populate per-kind `vec_<kind>` tables. Python
`configure_vec()` records a global `('*', 'vec')` profile, while
`get_vec_profile(kind)` and per-kind vector search expect kind-specific vector
projection state. A fresh Memex store can ingest `KnowledgeItem` data
successfully and still see per-kind vector search degrade because the
`vec_knowledgeitem` table has not been materialized.

The product contract should not require Memex to know how to create vector
tables, generate embeddings, or insert raw vectors.

---

## Decision

FathomDB should use:

- one database-wide embedding engine and identity;
- per-kind vector indexing enablement;
- per-kind vector text source/schema;
- FathomDB-managed async and incremental vector projection maintenance.

Per-kind sqlite-vec tables may remain the physical storage layout. Per-kind
embedding engines are not part of this design.

---

## Goals

- Make vector projection a first-class managed projection, not a caller-managed
  side channel.
- Keep one active vector embedding identity for the whole database.
- Let applications opt individual kinds into vector indexing.
- Let applications define what canonical text should be embedded for each kind.
- Ensure new writes are not starved behind large full-database rebuilds.
- Make natural-language semantic search APIs unambiguous.
- Preserve existing low-level vector storage primitives internally where useful.

## Non-Goals

- Supporting different embedding engines per kind.
- Supporting comparable vector ranking across multiple embedding models.
- Requiring Memex to submit pre-embedded vectors.
- Doing expensive embedding work inside the canonical write transaction.
- Removing per-kind vector tables as an internal storage detail.
- Changing the FTS tokenizer model.

---

## Current State

### What already exists

FathomDB already has several building blocks:

- `EmbedderChoice` on engine open selects the read-time query embedder.
- `QueryEmbedder` exposes `embed_query`, `identity`, and `max_tokens`.
- `BatchEmbedder` exists for write-time/bulk regeneration.
- `regenerate_vector_embeddings` can embed existing chunks and write to a
  per-kind vector table.
- `regenerate_vector_embeddings_in_process` can batch-embed in process.
- `SchemaManager::ensure_vec_kind_profile` creates per-kind vec tables and
  projection profile rows.
- The writer can currently store caller-supplied `VecInsert` rows into the
  correct per-kind vector table.
- FTS already has automatic writer-side projection maintenance and async rebuild
  machinery.

### What is different for FTS

FTS projection is local, deterministic, and cheap enough to maintain
synchronously for normal writes:

- chunk FTS rows are derived from canonical chunks;
- property FTS rows are derived from configured property schemas;
- registration can trigger eager or async rebuild;
- async rebuilds use durable state plus staging/double-write behavior.

Vector projection is different:

- embedding can be expensive;
- embedding may require model load or external resources;
- embedding can fail independently from the canonical write;
- embedding identity must match between write-time and query-time;
- full historical rebuilds can be long-running.

Therefore vector projection should borrow the FTS lifecycle model, but not copy
the synchronous write-transaction execution model.

---

## Data Model

### Database-wide embedding identity

Add or formalize one database-wide vector embedding profile. Conceptually:

```text
vector_embedding_profile
  profile_name
  model_identity
  model_version
  dimensions
  normalization_policy
  max_tokens
  active_at
  created_at
```

Only one profile is active for a database at a time.

Changing the database-wide embedding engine is an explicit administrative
operation. It invalidates or schedules regeneration for all vector-enabled
kinds.

### Per-kind vector indexing schema

Add a per-kind vector indexing config. Conceptually:

```text
vector_index_schemas
  kind
  enabled
  source_mode
  source_config_json
  chunking_policy
  preprocessing_policy
  created_at
  updated_at
```

`source_mode` should start conservative:

- `chunks`: embed chunk text for chunks attached to nodes of this kind.

Future modes can mirror FTS property schemas:

- `properties`: embed selected JSON property paths.
- `composed`: embed selected properties plus chunk text.
- `recursive_properties`: embed scalar leaves under configured paths.

The first Memex-facing path should be `chunks`, because Memex already writes
knowledge text as canonical chunk content.

### Physical vec tables

Keep per-kind vector tables as storage:

```text
vec_<kind_table_name>
  chunk_id TEXT PRIMARY KEY
  embedding float[N]
```

All per-kind vector tables in a database use the same database-wide embedding
identity and dimensions.

The per-kind table remains an implementation detail. The public identity
contract is database-wide.

---

## Admin API

### Configure the database embedding engine

Python shape:

```python
db.admin.configure_embedding("builtin")
```

or:

```python
db.admin.configure_embedding(BuiltinEmbeddingEngine())
```

Responsibilities:

- resolve and validate the embedding engine;
- persist the database-wide embedding identity;
- compare against the existing active identity;
- if identity changes, mark all vector-enabled kinds as needing regeneration;
- do not silently mix old vectors with a new embedding identity.

### Configure vector indexing for a kind

Python shape:

```python
db.admin.configure_vec("KnowledgeItem", source="chunks")
```

Responsibilities:

- persist that `KnowledgeItem` is vector-indexed;
- create or ensure the internal per-kind vector table;
- schedule a background backfill for existing canonical chunks of that kind;
- cause future writes for that kind to enqueue high-priority incremental vector
  work.

The kind-level setting controls whether and how a kind is vector-indexed. It
does not select a separate embedding model.

### Status and repair

Expose vector projection status per kind:

```python
db.admin.get_vec_index_status("KnowledgeItem")
```

Useful fields:

```text
kind
enabled
state
pending_incremental_rows
pending_backfill_rows
last_error
last_completed_at
embedding_identity
```

Projection repair should be able to:

- enqueue missing vector work for vector-enabled kinds;
- remove stale vector rows for retired/superseded canonical state;
- schedule full regeneration after embedding identity changes.

---

## Write Path

Canonical writes must remain authoritative.

For a write that inserts or updates a node/chunk:

1. Validate and persist canonical node/chunk state.
2. Maintain synchronous projections that are cheap and deterministic, such as
   chunk FTS.
3. If the node kind has vector indexing enabled, enqueue vector projection work
   after the canonical write commits.

Embedding should not run inside the canonical write transaction by default.

The vector work item should capture enough canonical identity to be safe:

```text
work_id
kind
node_logical_id
chunk_id or source row id
canonical content hash/version
priority
embedding_profile_id
attempt_count
created_at
```

Before applying an embedding result, FathomDB must verify that the canonical
state still matches the work item. If the chunk was superseded, retired, or
changed, the embedding result is discarded or requeued against the new state.

---

## Queueing And Scheduling

Vector projection needs at least two classes of work:

- **incremental writes:** new or updated canonical data;
- **backfill/regeneration:** historical rows, identity changes, repair.

New incremental work must not be queued behind a large batch rebuild. Suggested
scheduling rule:

```text
always prefer incremental work over backfill work,
with occasional backfill slices to avoid starvation
```

For example:

- process all currently ready incremental work first, up to a bounded batch;
- process one bounded backfill slice;
- re-check incremental work.

This keeps recently written Memex memories semantically searchable promptly even
while a large historical vector rebuild is running.

The queue should be durable. If the process exits after canonical write commit
but before embedding completes, pending vector work must survive restart.

---

## Rebuild Lifecycle

Full vector rebuild should use the same embedding engine and identity as
incremental write projection.

Rebuild lifecycle:

1. Admin config or repair marks a kind as needing backfill/regeneration.
2. FathomDB enqueues or records durable backfill state for active canonical
   rows of that kind.
3. The vector projection worker embeds in batches where possible.
4. Apply only results whose canonical state still matches the work item.
5. Track progress and errors per kind.

Changing the database-wide embedding identity should not partially mix old and
new vectors.

Recommended initial strategy:

1. Mark every vector-enabled kind as stale/degraded.
2. Stop returning vector hits for stale kinds whose table identity does not
   match the active database-wide embedding identity.
3. Delete and repopulate rows in the existing per-kind vector table.
4. Mark the kind current only after fresh rows have been generated and applied.

This is intentionally simpler than replacement-table swap. FTS can preserve
reader continuity during some async property-index rebuilds because old and new
FTS rows are still queried with the same local tokenizer semantics. Vector
identity changes are different: old vectors cannot be queried correctly with
the new embedding engine. Online replacement-table swap can be added later, but
it should not be required for the first managed-vector implementation.

---

## Query API

Natural-language semantic search should accept natural-language text:

```python
db.nodes("KnowledgeItem").semantic_search("Acme", limit=5)
```

The API should:

- require a configured database-wide embedding engine;
- require the target kind to have vector indexing enabled;
- embed the query with the same database-wide engine used for write projection;
- search the kind's vector table;
- return vector scores and degradation metadata.

Unified `search()` may use semantic search as one branch, but the branch
semantics should be explicit in docs:

```python
db.nodes("KnowledgeItem").search("Acme", limit=5)
```

If a raw-vector query path remains, it should be clearly named and treated as
internal, debug, or admin-oriented:

```python
db.nodes("KnowledgeItem").raw_vector_search([0.1, 0.2, ...], limit=5)
```

`vector_search("Acme")` should not remain ambiguous.

---

## Raw Vector Inserts

Memex should not use raw vector insertion.

FathomDB may keep the existing `VecInsert` machinery internally while migrating
the vector projection implementation. If any public raw-vector insert API
remains, it should be explicitly marked as unsafe/admin/import-oriented and
should validate against the active database-wide embedding identity:

- dimensions must match;
- target kind must be vector-enabled;
- inserted vectors must be associated with the active embedding profile;
- docs must state that normal applications should configure an embedding engine
  instead.

The Memex integration path is:

```text
configure embedding engine
configure vector indexing for KnowledgeItem
write canonical KnowledgeItem data
let FathomDB maintain vector projection
```

---

## Failure And Degradation Semantics

Canonical writes should not fail solely because vector embedding is temporarily
unavailable.

Examples:

- model weights are not downloaded yet;
- external embedding provider is rate-limited;
- embedding worker crashes;
- a full rebuild is in progress.

In these cases:

- canonical writes still commit;
- vector work remains pending or failed with retry metadata;
- semantic search returns available vector hits plus clear degradation metadata,
  or degrades to empty vector results;
- unified search can still use FTS/fallback branches.

Hard failures should occur for configuration errors:

- no database-wide embedding engine configured;
- target kind is not vector-enabled;
- vector table dimension does not match active embedding identity;
- attempting to switch embedding identity without accepting rebuild impact.

---

## Migration From Current Behavior

Current behavior should migrate toward this design in stages.

### Stage 1: clarify profile semantics

- Introduce database-wide embedding profile APIs.
- Stop treating Python `configure_vec()` as global vector identity plus
  per-kind readiness.
- Make per-kind `get_vec_profile(kind)` semantics explicit or replace it with
  `get_vec_index_status(kind)`.

### Stage 2: per-kind vector indexing config

- Add `configure_vec(kind, source="chunks")`.
- Ensure `configure_vec` creates/ensures the per-kind vector table.
- Schedule a backfill rather than requiring Memex to call regeneration directly.

### Stage 3: incremental vector projection

- Add durable vector projection work state.
- Enqueue high-priority work after canonical writes for vector-enabled kinds.
- Add a worker that embeds and applies results outside the write transaction.

### Stage 4: query API cleanup

- Add `semantic_search(text, limit)`.
- Deprecate ambiguous natural-language use of `vector_search(text)`.
- Keep raw-vector search under an explicit raw/debug/admin name if needed.

### Stage 5: public raw vector insert policy

- Remove `vec_inserts` from normal application-facing docs.
- Either make raw vector inserts internal, or require explicit unsafe/admin
  naming and strict identity validation.

---

## Acceptance Criteria

- A fresh Memex store can configure one database-wide embedding engine.
- Memex can configure `KnowledgeItem` for vector indexing with chunk text as the
  source.
- Memex can ingest a `KnowledgeItem` without supplying raw vectors.
- FathomDB eventually populates the internal `vec_knowledgeitem` projection.
- New `KnowledgeItem` writes are prioritized ahead of large historical rebuilds.
- Semantic search over `KnowledgeItem` returns vector-backed hits with non-null
  vector scores after projection catches up.
- If the embedding worker is unavailable, canonical writes still succeed and
  vector search reports clear degradation.
- API docs clearly distinguish natural-language semantic search from raw-vector
  search.

---

## Test Additions

Add a Memex-oriented trip-wire:

1. Open a fresh database with a configured embedding engine.
2. Configure `KnowledgeItem` vector indexing from chunks.
3. Ingest one `KnowledgeItem` with chunk text containing `Acme Corp`.
4. Wait for or explicitly drive vector projection catch-up in test mode.
5. Run natural-language semantic search for `Acme`.
6. Assert:
   - `was_degraded == False`;
   - at least one hit is returned;
   - the hit has a non-null vector score;
   - Memex did not submit any raw vector.

Also add scheduling tests:

- seed a large backfill;
- enqueue a new incremental `KnowledgeItem`;
- assert the incremental work is processed before the backfill drains.

---

## Closed Recommendations

### Projection worker ownership

Recommendation: run the initial vector projection worker inside
`EngineRuntime`, backed by durable database state.

Rationale:

- This matches Memex's expected product contract: opening FathomDB and writing
  data should be enough for configured projections to catch up.
- FTS already uses an engine-owned `RebuildActor` plus durable rebuild state for
  async property FTS rebuilds. Vector projection should reuse that pattern rather
  than require a separate Memex-maintained service.
- Durable state remains the correctness boundary. If the in-process worker is
  not running, pending vector work stays in the database and can be resumed by a
  later engine instance.

Recommended shape:

- add a vector projection actor alongside the FTS rebuild actor;
- use durable queue/state tables rather than in-memory-only queues;
- expose explicit admin/test controls to drain or poll vector projection work;
- keep embedding and vector application outside the canonical write
  transaction.

### Embedding availability validation

Recommendation: `configure_embedding(...)` should validate configuration and
identity, but should not eagerly require model availability by default.

Rationale:

- Built-in models may need first-use download or cache warmup.
- External engines may be temporarily unavailable even though their identity and
  configuration are valid.
- FTS tokenizer configuration can be validated synchronously because tokenizers
  are local SQLite configuration. Embedding engines are heavier and may depend
  on model files, network, credentials, or provider availability.

Recommended shape:

- `configure_embedding(...)` persists the database-wide identity and detects
  rebuild impact.
- `admin.check_embedding()` or `admin.warm_embedding()` can be added for callers
  who want an explicit availability check.
- Projection work records availability failures in vector projection status and
  retries according to policy.
- Canonical writes continue to commit when the embedding engine is temporarily
  unavailable.

### First text source beyond chunks

Recommendation: after `source="chunks"`, the first additional source mode
should be explicit scalar JSON property paths.

Proposed API shape:

```python
db.admin.configure_vec(
    "KnowledgeItem",
    source="properties",
    property_paths=["$.title", "$.body"],
)
```

Rationale:

- This reuses the FTS property-schema mental model.
- Existing FTS path validation and extraction code can inform the implementation.
- Explicit scalar paths are much easier to reason about than recursive or
  composed sources.
- Recursive extraction, weighted fields, and composed chunk-plus-property text
  can be deferred.

The initial Memex requirement remains `source="chunks"`.

### Identity changes

Recommendation: for the first implementation, mark affected vector indexes
stale/degraded and rebuild in place. Do not implement replacement-table swap
yet.

Rationale:

- FTS table swap works because readers can continue querying the old table with
  compatible tokenizer semantics while a new table is built.
- Vector identity changes make old rows incompatible with the new query
  embedder, so continuing to return old vector hits is misleading.
- In-place degraded rebuild is simpler and makes the uncertainty explicit.

Required behavior:

- switching database-wide embedding identity requires explicit rebuild-impact
  acknowledgement when vector-enabled rows exist;
- all affected vector-enabled kinds move to stale/degraded state;
- semantic search does not return rows from stale vector tables;
- the vector worker repopulates the existing per-kind tables with the new
  identity;
- status APIs expose progress and last error per kind.

Replacement-table rebuild can be revisited later for online reindexing, but it
should require a clear query-routing design for old-vs-new identities.

### Semantic search scope

Recommendation: `semantic_search` should require one explicit root kind at
first.

Supported initial shape:

```python
db.nodes("KnowledgeItem").semantic_search("Acme", limit=5)
```

Rationale:

- The storage layout is per kind.
- Per-kind enablement and source schemas mean each kind has its own projection
  readiness and status.
- A single-kind API gives clear errors for "kind is not vector-indexed" and
  clear degradation metadata for "kind is still catching up."
- Cross-kind semantic search can be added later because the database-wide
  embedding identity makes scores comparable, but it still needs fanout,
  per-kind status handling, and merge semantics.

Unified `search()` can call the single-kind semantic branch internally when the
root kind is known and vector indexing is enabled for that kind.
