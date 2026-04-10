# Schema-Declared Full-Text Projections Over Structured Node Properties

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

## User-Facing API Surface

At a high level, the feature should appear in schema or kind configuration, not
in ad hoc client search code.

Example schema shape:

```yaml
kinds:
  WMGoal:
    properties: ...
    text_projection:
      paths:
        - $.title
        - $.description
        - $.rationale

  WMKnowledgeObject:
    properties: ...
    text_projection:
      paths:
        - $.title
        - $.canonical_key
        - $.knowledge_type
        - $.payload.summary_text
```

High-level query surface:

- clients continue to write normal nodes only
- clients query with normal text search against the kind
- no synthetic chunk creation is required

Example:

```python
db.nodes("WMKnowledgeObject").text_search("oauth token rotation", limit=25)
```

Recommended API position:

- `text_search(...)` remains the primary query operator
- `property_text_search(...)` becomes unnecessary and should eventually be
  removed or deprecated
- the engine decides whether a kind is backed by chunk FTS, structured-node
  FTS, or both

## Write-Time Behavior

On node insert or update for a kind with `text_projection` configured,
FathomDB should:

- extract the configured JSON paths from the node's `properties`
- normalize the extracted values into projection text
- update the internal full-text projection atomically with the node write
- mark the projection as superseded when the node version is superseded
- support admin rebuild and backfill from canonical node rows

Normalization rules for v1 should be simple and deterministic:

- include scalar string values
- include scalar non-string values by stringifying them
- include arrays of scalars by flattening them
- ignore objects unless a path points to a scalar or array leaf
- skip missing or null values

The projection is engine-owned derived state, similar in spirit to an index,
not client-authored content.

## Query-Time Behavior

When a query targets a kind with a structured full-text projection:

- `text_search(...)` should drive from the projection-backed FTS index
- candidate node IDs should come from the projection index, not from scanning
  `nodes`
- filters and traversals should compose the same way they do for existing
  search-backed plans
- results should resolve directly to active node versions of that kind

Expected behavior:

- search execution path is FTS-first
- ranking is based on projection text match score
- kind restriction is intrinsic to the projection or applied at the indexed
  candidate stage
- no client-side serialization or substring matching is needed

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

They solve different problems and should remain separate concepts.

## Why This Solves the Client-Side Scan Problem

It moves search responsibility into the engine where it belongs.

Instead of:

- fetching 500 nodes
- serializing JSON in Python
- performing substring checks
- repeating across kinds

the client does:

- issue a text query against a kind
- let FathomDB hit an FTS-backed projection
- receive only matched nodes

That eliminates broad reads, repeated JSON materialization, and duplicated
per-client search logic. It also gives FathomDB a real search execution path
rather than forcing application-side table scans.

## Constraints, Tradeoffs, and Open Questions

Constraints:

- projection text is derived, so write cost increases
- schema must explicitly identify searchable fields
- only declared fields are searchable through this mechanism
- backup and recovery must treat projection rows as rebuildable derived state,
  while preserving projection contracts as canonical metadata
- write-time projection maintenance increases WAL traffic and should be measured
  under sustained upsert workloads

Tradeoffs:

- better read performance in exchange for additional write and index maintenance
- simpler client behavior in exchange for more engine responsibility
- kind-specific schema configuration instead of arbitrary runtime property
  search

Open design questions:

- Should per-path weights be supported, or deferred?
- Should the projection store field boundaries, or only concatenated text?
- How should arrays and nested objects be normalized beyond simple scalar
  flattening?
- Should language or tokenizer configuration be global, per database, or per
  kind?
- Should the engine expose which field matched, or only return matched nodes?
- Can a kind use both chunk-backed and structured-property-backed text search,
  and if so how are results merged?
- How should projection schema changes trigger backfill and migration?
- Should projection failures block writes, or degrade and surface repair state?
- What stress and benchmark coverage is required to validate WAL growth,
  checkpoint behavior, rebuild latency, and backup/restore correctness for
  projection-heavy workloads?

## Recommended Minimal First Version

Ship a narrow, opinionated v1.

Scope:

- one projection config per node kind
- projection defined as ordered JSON paths
- engine-maintained node-level FTS projection
- `text_search(...)` uses that projection automatically for configured kinds
- admin rebuild and backfill support

Behavior:

- extract strings, numbers, booleans, and arrays of scalars
- concatenate normalized values into a single internal search document per node
  version
- update projection synchronously on node write and update
- resolve hits to active node rows
- keep existing chunk FTS behavior unchanged for document kinds
- preserve transactional consistency so canonical node writes and required text
  projections commit or roll back together
- preserve safe export and recovery by making contracts canonical and projection
  rows rebuildable

Do not include in v1:

- per-field weighting
- field-scoped query syntax
- highlighting or snippets
- stemming or language customization per kind
- complex object flattening policies
- merged ranking across chunk and structured projections

Recommended product position:

- treat current `property_text_search` as an interim scan-based capability
- define Structured Node Full-Text Projections as the durable engine feature
- steer clients to schema declaration plus ordinary `text_search(...)`

## Bottom Line

FathomDB should add a first-class capability for schema-declared full-text
projections over structured node properties. That gives structured node kinds
the same engine-owned search discipline that chunks already provide for
documents, without forcing clients to invent synthetic chunk records or run
search in application code.
