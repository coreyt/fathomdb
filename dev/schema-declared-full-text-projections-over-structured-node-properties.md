# Schema-Declared Full-Text Projections Over Structured Node Properties

> **Status: Implemented** (SchemaVersion 15). See the
> [design document](./design-structured-node-full-text-projections.md) for
> architectural rationale and divergence notes.

## Problem Statement

FathomDB currently has a gap for structured nodes that need text retrieval but
are not documents. Clients are compensating by fetching broad batches of nodes,
serializing JSON properties in application code, and performing substring and
filter logic outside the engine. That creates repeated client-side scans,
unnecessary row materialization, duplicated search behavior, and poor
performance.

The missing capability is not "client chunking." The missing capability is
engine-managed full-text indexing for schema-declared node properties.

## Feature Name

**Structured Node Full-Text Projections**

Definition phrase:

**schema-declared full-text projections over structured node properties**

## Conceptual Model

A node kind may declare a full-text projection: a list of JSON property paths
whose values contribute to that kind's searchable text surface.

Examples:

- `WMGoal` -> `title`, `description`, `rationale`
- `WMKnowledgeObject` -> `title`, `canonical_key`, `knowledge_type`,
  `payload.summary_text`
- `WMObservation` -> `summary`, `payload.notes`
- `WMEvent` -> `title`, `summary`

FathomDB then:

- treats the node as the canonical stored record
- derives searchable text from the declared property paths
- maintains an internal projection and FTS index for that node kind
- resolves search hits back to the node directly

This is an indexing projection, not a second client-owned record model.

## Feature Contract

### Schema

Two new tables added in SchemaVersion(15) with an idempotent ensure helper:

**Contract table** — `fts_property_schemas`:

```sql
CREATE TABLE IF NOT EXISTS fts_property_schemas (
    kind TEXT PRIMARY KEY,
    property_paths_json TEXT NOT NULL,
    separator TEXT NOT NULL DEFAULT ' ',
    format_version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
```

**FTS index** — `fts_node_properties` (separate from `fts_nodes`):

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS fts_node_properties USING fts5(
    node_logical_id UNINDEXED,
    kind UNINDEXED,
    text_content
);
```

The existing `fts_nodes` table is unchanged and remains chunk-only.

### Admin API

Four operations, exposed across all SDK surfaces:

| Operation | Signature | Behavior |
|---|---|---|
| Register | `register_fts_property_schema(kind, property_paths, separator)` | Idempotent upsert. Validates a non-empty ordered list of simple dot-notation paths beginning with `$.`; rejects duplicates and malformed paths. |
| Describe | `describe_fts_property_schema(kind)` → `Option<FtsPropertySchemaRecord>` | Returns schema for a single kind, or None. |
| List | `list_fts_property_schemas()` → `Vec<FtsPropertySchemaRecord>` | Returns all registered schemas. |
| Remove | `remove_fts_property_schema(kind)` | Deletes the schema row. Does NOT delete FTS rows; requires explicit rebuild. |

### Write-Time Behavior

On node insert or upsert for a kind with a registered schema:

- `resolve_property_fts_rows` loads all schemas once per request from the DB
- Extracts property paths from co-submitted nodes using `extract_json_path`
  (supports simple dot-notation: `$.name`, `$.address.city`)
- Concatenates extracted values with the configured separator
- Property FTS rows are inserted in the same IMMEDIATE transaction as nodes

Normalization rules:

- Include scalar string values directly.
- Stringify numbers and booleans.
- Flatten arrays of scalars in order.
- Skip `null`, missing values, objects, nested arrays, and array elements that
  are not scalar.
- Preserve declared path order first, then preserve element order within any
  flattened scalar array.
- Preserve empty strings if they are explicitly present; they still count as
  extracted values even though they contribute no visible characters.
- Apply the configured separator between every extracted value, regardless of
  whether the values came from different paths or from the same flattened array.
- If no values are extracted after normalization, do not insert a property FTS
  row.

Supported path syntax in v1:

- Only simple dot-notation paths are supported, such as `$.title` and
  `$.payload.summary_text`.
- Paths must start with `$.`.
- Paths must contain one or more non-empty key segments after `$`.
- Array indexing, wildcards, recursive descent, quoted keys, and filter syntax
  are out of scope for v1 and must be rejected at registration time.

Transaction behavior:

- **Insert**: insert one `fts_node_properties` row if kind has a schema
- **Upsert**: ALWAYS delete existing property FTS rows, then re-insert
  (unconditional, not gated on ChunkPolicy)
- **Retire**: delete property FTS rows alongside chunk FTS rows
- **Excise**: full property FTS rebuild within the same transaction

### Query-Time Behavior

The existing `text_search(...)` query operator transparently covers both
chunk-backed and property-backed FTS. No new query API is introduced for this
feature.

The query compiler's `FtsNodes` driving table branch emits a UNION of
`fts_nodes` and `fts_node_properties`, so property-derived text is searchable
alongside chunk-derived text without any user-facing API change.

Example usage:

```python
# Searches both chunk-backed and property-backed FTS transparently
db.nodes("Document").text_search("oauth token rotation", limit=25)
db.nodes("WMGoal").text_search("quarterly revenue", limit=25)
```

### Projection and Rebuild

- `rebuild_property_fts` in `projection.rs`: DELETE all + re-INSERT from schemas
  + active nodes
- `rebuild_missing_property_fts_in_tx`: fills gaps for nodes with zero property
  FTS rows
- Integrated into `rebuild_projections(Fts)` and `rebuild_projections(All)`

### Cross-Surface Parity

All four admin operations are available across:

| Surface | Implementation |
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

## How It Differs From Explicit Chunks and Document Ingestion

Explicit chunks remain the correct model for documents and external content
because they represent real text units derived from content.

Structured node full-text projections are different:

- source of truth is the node's structured properties
- searchable text is derived from declared fields, not authored as chunks
- projection granularity is node-level, not content-fragment-level
- clients do not choose chunk boundaries or dual-write synthetic text records

So:

- use chunks for document and content ingestion
- use structured full-text projections for structured entity records
- use `text_search(...)` to search both chunk-backed and property-backed text
- use admin schema registration to opt a kind into property-backed search

They solve different problems and remain separate concepts with separate FTS
tables.

## Why This Solves the Client-Side Scan Problem

It moves search responsibility into the engine where it belongs.

Instead of:

- fetching 500 nodes
- serializing JSON in Python
- performing substring checks
- repeating across kinds

the client does:

- issue a `text_search(...)` query against a kind
- let FathomDB hit an FTS-backed projection
- receive only matched nodes

That eliminates broad reads, repeated JSON materialization, and duplicated
per-client search logic. It also gives FathomDB a real search execution path
rather than forcing application-side table scans.

## Constraints and Tradeoffs

Constraints:

- projection text is derived, so write cost increases
- schema must explicitly identify searchable fields via `$.`-prefixed paths
- only declared fields are searchable through this mechanism
- backup and recovery must treat `fts_node_properties` rows as rebuildable
  derived state, while preserving `fts_property_schemas` as canonical metadata
- write-time projection maintenance increases WAL traffic and should be measured
  under sustained upsert workloads

Tradeoffs:

- better read performance in exchange for additional write and index maintenance
- simpler client behavior in exchange for more engine responsibility
- kind-specific schema configuration instead of arbitrary runtime property
  search
- separate FTS table avoids migration risk while still preserving transparent
  `text_search(...)` coverage

## Lifecycle, Backup, And Recovery Semantics

The authoritative durability boundary for this feature is:

- `fts_property_schemas` is canonical metadata and must be preserved by
  bootstrap, safe export, and recovery workflows
- `fts_node_properties` is derived active-state projection data and must be
  rebuildable from active nodes plus `fts_property_schemas`

Lifecycle expectations:

- node insert and upsert must commit canonical node state and property FTS
  updates atomically
- node retire must remove property FTS visibility in the same transaction
- source excision must remove or rebuild property FTS atomically so excised
  content is not searchable after commit
- logical restore must reestablish property FTS visibility for the restored
  active node before the operation is considered complete
- rebuild and repair operations must be idempotent and sufficient to recover
  property FTS from canonical state alone

Backup and export expectations:

- safe export should include `fts_property_schemas` because it is canonical
  metadata
- safe export may include `fts_node_properties`, but restore correctness must
  not depend on those rows being present or up to date
- any recovery path that does not trust derived rows must be able to rebuild
  `fts_node_properties` from canonical state without data loss

Diagnostics expectations:

- integrity and semantic health tooling should account for drift in
  `fts_node_properties`, not only `fts_nodes`
- stress and benchmark coverage should include projection-enabled structured
  kinds under sustained upsert and search load

## Open Design Questions (Resolved and Remaining)

Resolved:

- **Per-path weights**: Deferred (not in v1)
- **Field boundaries vs concatenated text**: Concatenated text only, with
  configurable separator
- **Language/tokenizer configuration**: Not in v1
- **Both chunk and property search on same kind**: Supported transparently via
  `text_search(...)`, which unions chunk-backed and property-backed FTS
  candidates
- **Projection schema changes and backfill**: Explicit — register schema, then
  run rebuild
- **Projection failures**: Do not block writes; property FTS is derived state

Remaining:

- Should per-path weights be supported in a future version?
- Should the engine expose which field matched, or only return matched nodes?
- Should future versions add richer path syntax beyond simple dot-notation?
- Should future versions expose field-level match reporting or snippets?

## Bottom Line

FathomDB has a first-class capability for schema-declared full-text projections
over structured node properties. Structured node kinds get the same
engine-owned search discipline that chunks provide for documents, using a
separate `fts_node_properties` FTS5 table backed by `fts_property_schemas`
contracts. Clients write ordinary nodes, register a property schema per kind,
and query with the existing `text_search(...)` operator. The engine derives,
maintains, and rebuilds the FTS projection automatically.
