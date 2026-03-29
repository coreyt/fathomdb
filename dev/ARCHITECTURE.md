# ARCHITECTURE.md

## 1. System Overview

`fathomdb` is intentionally designed as a **graph/vector/FTS shim in front of SQLite**.

That is the technical choice: rather than building a new storage engine, `fathomdb` uses SQLite as the durable local store and adds an opinionated access layer for the kinds of queries local AI agents need. The shim is responsible for:

- mapping agent-world-model data into relational structures
- exposing graph, document, full-text, and vector access through one API
- compiling agent-friendly query builders into optimized SQL
- managing derived search projections and their synchronization
- preserving enough history and provenance to support replay, correction, and rollback

At implementation time, this breaks down into four strata:

1. **Fluent AST builder:** deterministic SDK surface for agents
2. **Query compiler:** converts AST steps into one cohesive SQLite plan
3. **Execution coordinator:** manages connections, WAL mode, prepared statements, and reader/writer discipline
4. **Write/projection pipeline:** handles canonical writes, projection sync, and temporal/provenance metadata

**Core stack**

- **Storage engine:** SQLite
- **Document functions:** SQLite `JSON1` / `JSONB`
- **Full-text index:** `FTS5`
- **Vector index:** `sqlite-vec`
- **Shim language:** Rust
- **SDK surfaces:** Python, TypeScript, and Rust

Rust is the preferred core implementation language because the engine is
compiler-heavy, FFI-heavy, and correctness-sensitive:

- the query planner and AST compiler benefit from Rust's enums, pattern matching, and type system
- the single-writer actor, prepared-statement cache, and projection pipeline benefit from explicit ownership and concurrency control
- SQLite and extension integration fit well with Rust's systems-level interop
- Python and TypeScript bindings can be layered on top of a Rust core without changing the engine boundary

## 2. Canonical Persistence Model

The canonical store remains SQLite, but the schema must represent more than generic documents. The shim should center the database around a graph-friendly backbone, an explicit chunk projection layer, and a small set of typed runtime tables for v1.

### 2.1 Graph-Centric Backbone

The base relational layer captures shared identity, relationships, and common metadata:

```sql
CREATE TABLE nodes (
    row_id TEXT PRIMARY KEY,
    logical_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    properties BLOB NOT NULL,      -- JSONB
    created_at INTEGER NOT NULL,
    superseded_at INTEGER,
    source_ref TEXT,
    confidence REAL
);

CREATE TABLE edges (
    row_id TEXT PRIMARY KEY,
    logical_id TEXT NOT NULL,
    source_logical_id TEXT NOT NULL,
    target_logical_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    properties BLOB NOT NULL,      -- JSONB
    created_at INTEGER NOT NULL,
    superseded_at INTEGER,
    source_ref TEXT,
    confidence REAL
);
```

This gives the system a durable graph substrate for people, projects, meetings, tasks, claims, events, and other world-model entities.

The split between `logical_id` and `row_id` is a core part of the append-only
design:

- `logical_id` identifies the real-world entity
- `row_id` identifies a specific physical version of that entity
- edges point to logical identities rather than a single physical version
- the query compiler always resolves those identities to the currently active row

Canonical entity properties should remain primarily in SQLite JSONB blobs rather
than being aggressively normalized into many physical columns. The goal is to
preserve schema flexibility for user-world entities while reserving explicit
relational columns for system-owned fields such as timestamps, confidence, and
provenance.

### 2.2 Typed Runtime Tables

Generic graph records alone are not enough, but the engine draws a firm
boundary between storage primitives and application domain semantics. The typed
runtime schema is narrow by design and stays narrow:

- `runs`: execution containers (session-level, scheduler-level, or equivalent)
- `steps`: prompt/control/LLM-stage records within a run
- `actions`: concrete tool calls, observations, or emitted outcomes within a step

These three tables are the **ceiling** for engine-owned typed tables. They
exist because `source_ref` needs a typed provenance anchor with referential
integrity, not because the engine has opinions about how agent execution should
be structured. Applications that want a flat event stream use `kind = "event"`.
Applications that want deeper nesting use edges between operation nodes. The
three-table model accommodates both without engine changes.

**The engine does not own domain semantics.** Concepts above this line —
meetings, approvals, scheduling artifacts, intent frames, evaluation records,
knowledge objects, or any other application domain concept — belong in
application code, modeled as generic `nodes` and `edges` with
application-chosen `kind` values. The engine provides graph, versioning,
search, and provenance primitives; the application defines its own world-model
ontology on top.

This is a permanent design principle, not a v1 deferral. Domain-specific typed
tables are not deferred engine features. They are application responsibility.
Applications that need query performance on hot domain-specific paths should use
expression indexes on JSONB properties (§7.2) rather than requesting new engine
tables. That mechanism handles performance needs without growing the engine
schema.

[ARCHITECTURE-deferred-expansion.md](./ARCHITECTURE-deferred-expansion.md)
documents domain patterns that applications can build on the engine's
primitives.

### 2.3 Chunk Projection Layer

Chunked semantic retrieval requires an explicit mapping layer:

```sql
CREATE TABLE chunks (
    id TEXT PRIMARY KEY,
    node_logical_id TEXT NOT NULL,
    text_content TEXT NOT NULL,
    byte_start INTEGER,
    byte_end INTEGER,
    created_at INTEGER NOT NULL
);
```

Vector and full-text search resolve through `chunks`, not directly to `nodes`.

### 2.4 Operational Store

The engine provides a general-purpose operational store alongside the graph
backbone. Operational data is organized into **collections**, each declared with
a `kind` that controls mutation semantics:

- **`append_only_log`:** immutable event streams. Mutations use `Append` only;
  records are never updated in place. Suitable for audit logs, provenance
  streams, and event sourcing.
- **`latest_state`:** key-value current-state tables. Mutations use `Put` and
  `Delete`; the derived current-state view (`operational_current`) always
  reflects the most recent value for each record key.

Each collection is registered in `operational_collections` with its schema,
retention policy, and optional contracts.

**Mutations and ordering.** Every write is recorded in `operational_mutations`
with a monotonically increasing `mutation_order` column (seeded from `rowid`).
Mutations are indexed by `(collection_name, record_key, mutation_order DESC)`
for efficient latest-state resolution and historical replay.

**Current-state derived view.** For `latest_state` collections, the
`operational_current` table maintains a materialized latest-value-per-key view
with a foreign key back to the originating mutation.

**Filter fields and secondary indexes.** Collections may declare
`filter_fields_json` to enable extracted-value filtering on mutations.
Extracted scalar values (string or integer) are stored in
`operational_filter_values` and indexed for efficient filtered reads.
Collections may also declare `secondary_indexes_json`; indexed entries are
stored in `operational_secondary_index_entries` with up to three typed slots per
entry.

**Validation contracts.** Collections may declare `validation_json` to enforce
structural constraints on mutation payloads at write time.

**Retention.** Each collection carries a `retention_json` policy (e.g.
`max_age_seconds`, `max_rows`, or `keep_all`). Retention is executed as an
explicit operator action through two primitives:

- `plan_operational_retention`: evaluates retention policies and reports what
  would be deleted
- `run_operational_retention`: executes the plan, deleting expired mutations and
  recording the run in `operational_retention_runs`

The engine does not run retention automatically; scheduling is the
responsibility of the external operator or orchestration layer.

**Compact and purge.** Two additional primitives operate on `append_only_log`
collections:

- `compact_operational_collection`: identifies and removes compaction candidates
  according to the collection's retention policy
- `purge_operational_collection`: deletes mutations matching purge criteria and
  records the operation

### 2.5 Metadata And Schema Control

The shim owns the internal schema and migrations.

- Agents do not write raw SQL directly.
- Internal schema versions are tracked in `fathom_schema_migrations`.
- Migrations are applied by the shim on startup. Each migration is wrapped in
  an `unchecked_transaction()` and committed individually, so a failure mid-run
  leaves earlier migrations intact.
- **Downgrade protection:** on bootstrap, the engine compares the highest
  applied schema version against its own compiled version. If the database has
  been migrated by a newer engine, bootstrap rejects the open with a
  `VersionMismatch` error rather than silently running against an incompatible
  schema.
- The WAL journal size limit is set to 512 MB
  (`PRAGMA journal_size_limit = 536870912`) to bound disk usage from
  long-running write workloads.
- Provenance, confidence, timestamps, and correction lineage are stored as canonical metadata, not only derived annotations.

For identity and locality:

- canonical row identifiers should be time-sortable rather than random
- the preferred default is ULID stored as text for debugging and operational clarity
- UUIDv7 stored as a 16-byte blob remains an acceptable alternative if lower-level I/O efficiency becomes more important than human readability

## 3. Derived Multi-Modal Projections

The graph/vector/FTS layer is a technical decision in service of the user needs. These projections are derived from canonical SQLite state and managed by the shim.

```sql
CREATE VIRTUAL TABLE fts_nodes USING fts5(
    chunk_id UNINDEXED,
    node_logical_id UNINDEXED,
    kind UNINDEXED,
    text_content
);

CREATE VIRTUAL TABLE vec_nodes USING vec0(
    chunk_id TEXT PRIMARY KEY,
    embedding float[1536]
);
```

The vector projection shown above represents the active embedding profile. In
practice, vector storage should be versioned by embedding model and dimension,
for example `vec_nodes_v1`, `vec_nodes_v2`, rather than altered in place across
model migrations.

### 3.1 Projection Rules

- Text-bearing records are chunked and projected into FTS.
- Embeddable content is chunked and projected into the vector index.
- Graph traversals run over canonical relationships in `edges`.
- Projection rows always retain linkage back to canonical records.
- Vector and FTS candidate sets resolve through `vec_nodes -> chunks -> nodes`.

Projection classes should be treated differently:

- **Required projections:** low-cost lexical/search projections such as FTS and cheap metadata extraction must commit atomically with canonical writes
- **Optional projections:** expensive semantic enrichments such as externally generated embeddings may be queued and backfilled for bulk/background ingestion workloads

### 3.2 Synchronization Strategy

This architecture deliberately avoids SQLite triggers for projection maintenance.

**Decision:** the shim owns multi-write synchronization.

That means a single write path is responsible for:

1. writing canonical rows
2. writing or updating graph relations
3. updating required projections such as FTS
4. updating inline semantic projections when they are available for the current workload
5. enqueuing optional semantic projection work in memory when it is intentionally deferred
6. recording provenance, control artifacts, and outcome telemetry

All of this happens in one coordinated unit of work so failures are visible and repairable.

## 4. Query Layer And API

The API is intentionally agent-friendly. Rather than asking an LLM to invent bespoke SQL or another query language, the shim exposes deterministic SDKs and compiles them into SQL.

```python
results = (
    db.nodes("Meeting")
    .vector_search("stressful work discussion", limit=5)
    .traverse(direction="in", label="ATTENDED")
    .filter(lambda p: p.properties["status"] == "active")
    .select("id", "properties.name")
    .execute()
)
```

### 4.1 Builder And AST Model

- SDK calls build an internal query AST.
- The compiler turns that AST into SQLite queries.
- Agents work with predictable code constructs instead of fragile string synthesis.
- Representative AST steps include vector search, text search, graph traversal, JSON predicates, temporal filters, chunk resolution, and joins against runtime tables.

### 4.2 Query Capabilities

The compiler must support one execution model that can combine:

- graph traversal
- JSON property filtering
- full-text search
- vector similarity
- chunk-to-node resolution
- temporal filtering
- provenance-aware filtering
- joins against runtime tables such as runs, steps, or actions

### 4.3 Compilation Strategy

The compiler should plan queries from the inside out by identifying the narrowest
driving table first.

In practice this means:

- vector or full-text matches usually form the initial candidate set
- chunk rows resolve those candidates back to canonical node identities
- graph traversals join outward from those resolved identities
- canonical node and edge reads happen after candidate reduction
- JSON and relational filters are applied as late as possible over the already reduced set

This planning discipline is what keeps multimodal queries inside SQLite's VM
instead of spilling large intermediate sets into application memory.

The main exception is a highly selective deterministic filter, such as a direct
entity ID or foreign-key equality predicate. In those cases, the relational
constraint should drive the query and FTS/vector search should be evaluated only
inside that reduced scope.

### 4.4 Top-K Pushdown

To avoid N+1 behavior, the compiler should push the narrowest candidate step as deep as possible.

Example:

```sql
SELECT target_node.row_id, target_node.properties
FROM (
    SELECT chunk_id
    FROM vec_nodes
    WHERE embedding MATCH ?
    ORDER BY distance
    LIMIT 5
) v
INNER JOIN chunks c ON c.id = v.chunk_id
INNER JOIN nodes seed_node
    ON c.node_logical_id = seed_node.logical_id
    AND seed_node.superseded_at IS NULL
INNER JOIN edges e
    ON seed_node.logical_id = e.source_logical_id
    AND e.superseded_at IS NULL
INNER JOIN (
    SELECT row_id, logical_id, properties
    FROM nodes
    WHERE superseded_at IS NULL
) target_node
    ON e.target_logical_id = target_node.logical_id
WHERE e.kind = 'ATTENDED';
```

Known-depth traversals compile to joins. Variable-depth traversals compile to recursive CTEs.

Recursive CTE plans must always be bounded. Generated traversal queries should
include:

- a strict depth ceiling
- a hard scalar result limit
- cycle detection over visited IDs

The compiler should never emit an effectively unbounded recursive traversal.

### 4.5 Execution Coordinator

The execution layer should treat SQLite as a bytecode-executing VM and optimize
around that reality.

- enforce `PRAGMA journal_mode = WAL`
- use `PRAGMA synchronous = NORMAL` unless a stricter durability profile is required
- enforce `PRAGMA foreign_keys = ON`
- set `PRAGMA busy_timeout = 5000`
- prefer `PRAGMA temp_store = MEMORY`
- use `PRAGMA mmap_size` aggressively on machines that can afford it
- provide a pool of read-only connections for concurrent readers
- provide exactly one coordinated write connection or equivalent serialized write path
- cache prepared statements keyed by an AST-shape hash rather than raw literal values

**Reader pool.** The `ReadPool` maintains a configurable number of read-only
connections (default `pool_size = 4`). Each connection is wrapped in its own
`Mutex`, so concurrent readers that acquire different slots proceed in parallel
without contention. The shape cache that maps AST-shape hashes to compiled SQL
is bounded at 4096 entries; when the limit is reached, the entire cache is
cleared rather than partially evicted.

The AST-shape hash should include structural constants such as:

- `LIMIT` values
- recursion-depth ceilings
- `IN (...)` list arity

User-provided data such as strings, vectors, and timestamps remain parameterized.
This preserves planner quality where SQLite behaves differently for structural
constants versus bound values.

This keeps read latency low while respecting SQLite's single-writer model.

**Writer thread.** The write architecture uses an explicit in-process writer
thread. All writes flow through a bounded `sync_channel(256)` rather than
relying on `busy_timeout` as the main concurrency-control mechanism.

The writer thread applies several safety measures:

- **Panic recovery:** each call to `resolve_and_apply` is wrapped in
  `catch_unwind`. If the closure panics, the writer issues `ROLLBACK` to clean
  up any open transaction and returns an error to the caller rather than
  poisoning the thread.
- **Reply timeout:** callers wait at most 30 seconds (`recv_timeout`) for the
  writer to reply. If the writer is stuck or overloaded, the caller receives a
  timeout error instead of blocking indefinitely.
- **Per-type write limits:** a single `WriteRequest` is bounded to 10,000
  nodes, 10,000 edges, 50,000 chunks, and 100,000 total items. These limits
  prevent a single request from monopolizing the writer or exhausting memory.

## 5. Write Path And Ingestion

The write path is responsible for more than saving documents.

### 5.1 Canonical Write Pipeline

When the agent writes memory, the shim should be able to:

1. accept the raw source artifact
2. perform heavy pre-flight work such as parsing, chunking, or embedding generation before taking a write lock
3. normalize the artifact into canonical records
4. create or update relevant graph entities and edges
5. write runtime table records (`runs`, `steps`, `actions`) when applicable
6. project searchable text into FTS
7. project chunks into vector storage
8. attach provenance, confidence, timestamps, and correction links

### 5.2 Example Ingestion

```python
db.ingest(
    kind="meeting_transcript",
    content=large_transcript,
    metadata={"meeting_id": "..."}
)
```

The shim may then:

- store the raw artifact
- extract transcript segments and meeting artifacts
- create follow-up tasks or commitments when promotion rules allow
- index relevant text into FTS
- embed chunks for semantic retrieval

### 5.3 Transaction Discipline

Long-running enrichment work cannot happen inside an active SQLite transaction.

The intended write sequence is:

1. **Pre-flight async stage:** parse the source, prepare canonical payloads, and obtain embeddings or other expensive enrichments
2. **`BEGIN IMMEDIATE`:** acquire the write lock only after heavy computation completes
3. **Canonical append/update:** write nodes, edges, and typed semantic side-table rows
4. **Projection sync:** update required projections and any inline semantic projections in the same transaction
5. **Commit:** release the write lock

This preserves atomicity between canonical rows and required search projections
while keeping expensive external computation off the locked path.

For interactive writes, embeddings that are required for immediate agent use
should be generated before `BEGIN IMMEDIATE` and then committed atomically with
canonical rows.

For bulk or background ingestion, canonical rows may commit first and enqueue
optional semantic projection work into an in-memory worker queue. If the process
crashes before that work completes, startup should run a
`rebuild_missing_projections()` pass rather than depend on a durable queue table
inside the same SQLite file.

### 5.4 Control And Evaluation Writes

The same datastore also needs write paths for:

- prompt/control artifacts
- tool and non-tool selected actions
- observations and outcomes
- approvals and review decisions
- evaluation labels and replay metadata

Those records are canonical data, not logging afterthoughts.

## 6. Reversibility And Temporal Semantics

The user need for trust becomes a technical requirement for versioned state.

### 6.1 Versioned Writes

Updates should be append-oriented rather than destructive when possible.

- Existing rows are superseded rather than blindly overwritten.
- Queries can be scoped to a time or session context.
- Corrections preserve lineage instead of erasing prior state.

Default reads should target active state, which in practice means a query shape
equivalent to `superseded_at IS NULL`. Temporal-scoped reads replace that with
`created_at <= ? AND (superseded_at IS NULL OR superseded_at > ?)`.

This is intentionally a unitemporal append-oriented model, not a full bitemporal
database design.

### 6.2 Proposal And Approval Support

The core engine should keep temporal state simple: active, superseded, or
deleted. Proposal or approval semantics for v1 should live in application-layer
JSON properties or SDK models rather than being baked into the engine's core
visibility rules. The deferred engine-level approval model is preserved in
[ARCHITECTURE-deferred-expansion.md](./ARCHITECTURE-deferred-expansion.md).

### 6.3 Explainability And Provenance Joins

Because canonical rows keep source references and typed semantic side tables
store control and action history, the query layer can support explain-style
queries that join a node or edge back to:

- the control artifact that authorized or interpreted it
- the action or observation that created it
- the confidence, timestamp, and review status attached to it

Direct `source_ref` foreign keys on canonical rows are preferred over a separate
general-purpose lineage graph. This keeps explain queries cheap and operationally
simple.

### 6.4 Provenance Lifecycle

Provenance events accumulate over time. The engine provides
`purge_provenance_events` as an explicit operator primitive for managing this
growth:

- Deletes events older than a caller-supplied `before_timestamp`.
- Deletes in batches of 10,000 rows per loop iteration to avoid holding the
  write lock for an unbounded duration.
- Certain event types are preserved by default (`excise` and `purge_logical_id`)
  so that audit-critical records survive routine purges. Callers may override
  the preserved set via `preserve_event_types`.
- Supports `dry_run` mode, which counts matching events without deleting.
- Returns a report including `events_deleted`, `total_after`, and
  `oldest_remaining` timestamp.

Like operational retention, provenance purge is an explicit operator action. The
engine does not schedule or auto-run provenance cleanup.

### 6.5 Integrity And Recovery Model

`fathomdb` should treat recovery as a first-class engine capability, not a
backup-only afterthought. The architecture explicitly plans for three corruption
classes:

- **Physical corruption:** disk, filesystem, or crash-related damage
- **Logical corruption:** derived projection drift or broken virtual-table state
- **Semantic corruption:** bad agent reasoning that poisons the world model

The corresponding recovery primitives are:

- `rebuild_projections(target=[...])` for deterministic reconstruction of FTS
  projections from canonical state and restoration of vector profile capability
  metadata
- `regenerate-vectors` for admin-owned regeneration of vector embeddings from a
  persisted TOML or JSON contract
- `rebuild_missing_projections()` at startup or admin time when optional
  semantic projection work was interrupted
- rollback-by-time-window for broad semantic reversal
- excision-by-`source_ref` for surgical containment of one bad run, step, or
  action

Because canonical state and derived projections are explicitly separated, the
blast radius of logical corruption is bounded. Because canonical rows are
append-oriented and provenance-linked, semantic corruption can be reversed
without restoring a coarse external snapshot.

**Success criterion:** A developer should be able to diagnose and repair a
wrong agent memory in under 15 minutes using only SQLite tooling plus
fathomdb's admin commands. The `trace_source` → `excise_source` →
`rebuild_projections` workflow is designed for exactly this scenario.

**Crash-consistency testing:** The test plan must cover crash scenarios
explicitly: interrupted WAL checkpoints, partial projection sync failures, and
write transactions interrupted mid-transaction. These are distinct from
functional correctness tests and require deliberate injection.

### 6.6 Physical Recovery Protocol

SQLite's built-in recovery tools remain useful, but `fathomdb` should recover
canonical tables only and then rebuild projections. Physical repair should:

1. isolate the database from live agent traffic
2. recover or dump canonical tables such as `nodes`, `edges`, `chunks`, `runs`,
   `steps`, and `actions`
3. rebuild into a fresh database file
4. run projection rebuilds from canonical state

FTS5 shadow data should never be treated as canonical recovery material.

Vector profile metadata is recoverable and should be restored so vector-capable
databases reopen with the correct table shape. Embedding rows written through
optional semantic projection work are not canonical recovery material in v0.1
and may need to be regenerated after physical recovery through the
`regenerate-vectors` admin workflow. The contract that drives regeneration is
persisted in `vector_embedding_contracts` so the application can supply the
model identity, version, normalization policy, chunking policy, preprocessing
policy, and generator command needed to rebuild embeddings deterministically
enough for recovery.

### 6.7 Admin And Repair Surface

The Rust engine should expose repair primitives directly, while a separate Go
admin tool can orchestrate them operationally. Expected admin operations
include:

- projection rebuild
- integrity checks
- safe export / snapshot
- trace by `source_ref`
- excise bad lineage and emit a repair patch
- apply a patch and re-run projection repair

## 7. Operational Decisions

### 7.1 Concurrency

SQLite remains the durability layer, so the shim should assume:

- WAL mode for concurrent readers
- a coordinated writer path for updates
- an in-memory write queue or equivalent serialization strategy to avoid `SQLITE_BUSY`

**Scale-out path:** If write throughput ever becomes a bottleneck, the correct
expansion is sharding by workspace or agent identity (multiple database files),
not multi-writer merge semantics. CRDT-style multi-writer sync requires
relaxing `PRAGMA foreign_keys`, partial unique indexes, and referential
integrity constraints — all of which fathomdb's corruption-class detection
and provenance tracing depend on. Shard-by-workspace preserves the
single-writer model and all integrity guarantees within each shard.

### 7.2 Indexing Strategy

JSON-heavy filters will need help over time.

- The shim can monitor hot query paths.
- Frequently queried JSON paths should default to expression indexes directly on JSON extraction.
- FTS and vector projections remain rebuildable from canonical state.

For example, a hot predicate such as `properties ->> '$.status'` can be indexed
directly:

```sql
CREATE INDEX idx_node_status
ON nodes(properties ->> '$.status');
```

Generated columns remain available when they are operationally useful, but they
should not be the default indexing strategy.

### 7.3 Integrity Checks And Repairability

The operational model should include explicit integrity routines:

- `PRAGMA integrity_check`
- `PRAGMA foreign_key_check`
- projection-shape and projection-presence checks
- startup detection of missing optional semantic backfills

Repairability is part of normal operation, not only disaster response.

**WAL health observability:** WAL size, checkpoint lag, and long-running reader
detection should be formalized as admin-surface signals. A WAL that grows
without checkpointing degrades read performance and risks losing checkpoint
progress. The admin surface should expose these as observable database health
metrics, not require manual `PRAGMA` inspection.

### 7.4 Embedding Pipeline

Embedding work should not block the interactive loop unnecessarily.

- chunking and embedding can run asynchronously
- expensive enrichment should finish before the write transaction begins
- required projections should commit atomically with canonical rows
- backfills and rebuilds should still be visible and repairable if projection state must be regenerated later

### 7.5 Dynamic Model Compatibility

Vector dimensions and embedding providers cannot be hardcoded permanently.

- vector projection tables should be configurable to the active embedding profile
- metadata should record the embedding model and projection version used
- model upgrades should create new versioned vector tables rather than mutating existing ones in place

## 8. Key Trade-Offs

1. **SQLite instead of a custom engine**
   The gain is durability, portability, and zero-ops deployment. The cost is living within SQLite's single-writer constraints.

2. **Shim-managed projections instead of trigger-heavy synchronization**
   The gain is explicit control, debuggability, and better error handling. The cost is a more opinionated write path in the shim.

3. **Typed canonical tables plus graph backbone instead of pure document blobs**
   The gain is cleaner semantics for agent state, control artifacts, and evaluation records. The cost is more schema design and migration work.

4. **Derived FTS/vector projections instead of direct-only canonical reads**
   The gain is high-performance multimodal retrieval. The cost is projection maintenance and rebuild logic.

## 9. Cross-Layer Protocol

The engine is accessed from multiple language runtimes. Each integration path
has its own protocol and safety properties.

### 9.1 Go to Rust: JSON-Over-Stdio Bridge

The `fathomdb-admin-bridge` binary implements a single-shot JSON request/response
protocol over stdin/stdout. The Go admin tool (`fathom-integrity`) serializes a
`BridgeRequest` as JSON, invokes the binary, and deserializes the JSON response.

- Protocol version is negotiated in each request (`protocol_version` field).
- Input is capped at 64 MB (`MAX_BRIDGE_INPUT_BYTES`).
- **Path validation:** database and destination paths must be absolute and must
  not contain `..` components. The bridge rejects requests that violate these
  constraints before opening any database.

### 9.2 Python to Rust: PyO3 In-Process Embedding

The Python SDK binds to the Rust engine via PyO3. The engine runs in-process
with no serialization boundary for read/write operations; Python objects are
converted to Rust types at the FFI layer.

### 9.3 Type Parity

Report-type field parity between Rust structs and their Python representations
is enforced by compile-time tests (`python_types.rs`). These tests catch struct
divergence early rather than allowing silent field drops at the serialization
boundary.

## 10. Architectural Summary

`fathomdb` is not trying to replace SQLite. It is using SQLite as a durable local substrate and adding an opinionated graph/vector/FTS shim for agent workloads. The architecture is aligned with the broader user need by making canonical agent state durable in SQLite while using graph traversal, FTS, and vector search as managed technical access paths over that state.
