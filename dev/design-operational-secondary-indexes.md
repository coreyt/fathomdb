# Design: Additional Secondary Indexes For Operational Collections

## Purpose

Define how FathomDB can add collection-declared secondary indexes beyond the
current filtered-read support without exposing arbitrary SQL index management
or breaking the existing operational collection contract.

The current system already supports bounded filtered reads through declared
`filter_fields_json` and engine-maintained extracted values. This design covers
the next step: higher-volume or more latency-sensitive operational read
workloads that need more than the baseline single-field access path.

## Decision Summary

- Keep `filter_fields_json` as the baseline filtered-read contract.
- Add a separate `secondary_indexes_json` collection field for additional
  indexing; do not overload `filter_fields_json`.
- Secondary indexes must be explicit, bounded, and collection-declared.
- The engine should maintain index entries transactionally; callers must not
  provide raw SQL or direct SQLite index definitions.
- The implemented v1 slice supports three named index kinds and a maximum of
  three indexed fields.
- Rebuild/repair support is required because these are derived access
  structures, not canonical state.
- The first query-planning slice routes append-only reads through
  `append_only_field_time` when the request shape matches; latest-state index
  kinds are already maintained and rebuildable, but broader planner usage can
  expand later without changing the contract.

## Goals

- Improve operational read latency for workloads that exceed the baseline
  filtered-read path.
- Preserve the current bounded query model.
- Keep index definitions portable across export, recovery, bootstrap, and
  rebuild flows.
- Avoid engine-owned schema explosion from one-off application indexes.

## Non-Goals

- Arbitrary SQL DDL from callers.
- User-defined SQLite expressions.
- Unlimited composite indexes.
- Full cost-based planning over arbitrary payload JSON.
- Replacing the existing filtered-read contract.

## Why A Separate Contract Field

`filter_fields_json` answers "which payload fields are readable through the
bounded filtered-read API?" Secondary indexing answers a different question:
"which declared access paths deserve stronger materialized support?"

Those are related but not identical concerns. Some fields may be filterable
without needing an additional index, and some compound access paths may require
an index definition that cannot be expressed as a simple filter-field list.

The collection metadata should therefore gain a separate field, for example:

```sql
ALTER TABLE operational_collections
    ADD COLUMN secondary_indexes_json TEXT NOT NULL DEFAULT '[]';
```

## Supported Index Kinds

The implemented v1 slice supports three index kinds.

### 1. `append_only_field_time`

For `append_only_log` collections.

Use case:

- "all audit rows for actor X ordered by timestamp desc"
- "all events for status Y between timestamps A and B"

Contract shape:

```json
{
  "name": "actor_ts",
  "kind": "append_only_field_time",
  "field": "actor",
  "value_type": "string",
  "time_field": "ts"
}
```

### 2. `latest_state_field`

For `latest_state` collections.

Use case:

- "all current connector_health rows with status = degraded"

Contract shape:

```json
{
  "name": "status_current",
  "kind": "latest_state_field",
  "field": "status",
  "value_type": "string"
}
```

### 3. `latest_state_composite`

For `latest_state` collections, up to three declared fields with left-prefix
matching semantics.

Use case:

- "all current rows for tenant = T and category = C ordered by updated_at desc"

Contract shape:

```json
{
  "name": "tenant_category",
  "kind": "latest_state_composite",
  "fields": [
    {"name": "tenant", "value_type": "string"},
    {"name": "category", "value_type": "string"}
  ]
}
```

## Storage Shape

The design should use a derived engine-maintained table rather than generating
per-collection ad hoc SQLite DDL from raw JSON payloads.

Example shape:

```sql
CREATE TABLE operational_secondary_index_entries (
    collection_name TEXT NOT NULL,
    index_name TEXT NOT NULL,
    subject_kind TEXT NOT NULL,      -- 'mutation' or 'current'
    mutation_id TEXT,
    record_key TEXT,
    sort_timestamp INTEGER,
    sort_integer INTEGER,
    slot1_text TEXT,
    slot1_integer INTEGER,
    slot2_text TEXT,
    slot2_integer INTEGER,
    slot3_text TEXT,
    slot3_integer INTEGER,
    PRIMARY KEY (collection_name, index_name, subject_kind, mutation_id, record_key)
);
```

The engine would then create a bounded set of SQLite indexes over this derived
table, for example by slot and sort dimension. This keeps schema surface stable
while still supporting multiple declared access paths.

## Write-Time Maintenance

Secondary index entries should be maintained transactionally alongside:

- `operational_mutations`
- `operational_current`
- `operational_filter_values`

Affected operations:

- `Append`
- `Put`
- `Delete`
- collection disable/repair flows that mutate current state
- compaction/purge/excise/recovery flows that remove or rebuild canonical rows

If the canonical write rolls back, the index maintenance must also roll back.

## Query Planning

The read path should stay bounded and explicit:

- try a declared secondary index only when the request shape matches it
- otherwise fall back to the existing filtered-read path
- never expose raw SQL predicates to callers

Index selection should be deterministic and rule-based:

- exact match on index kind and left-prefix fields
- optional trailing range on the declared sort/timestamp field
- clear fallback when no declared index matches

Current implementation note:

- append-only reads can use `append_only_field_time` directly today
- latest-state index kinds are maintained transactionally and included in
  rebuild flows, but they are not yet selected by a distinct public read API

## Admin Surface

The design should add:

1. `update_operational_collection_secondary_indexes(collection, secondary_indexes_json)`
2. `rebuild_operational_secondary_indexes(collection)`
3. index status/health visibility in collection-describe/admin reporting

Index rebuild is required because these entries are derived state and must be
recoverable after export, recovery, or detected drift.

## Evolution Rules

- Secondary indexes are explicit opt-ins, never inferred from workload history.
- Contract updates are per collection.
- Existing collections can gain new indexes through an explicit update path.
- New indexes may require a rebuild before they are considered active.
- Unknown index kinds or unsupported field combinations must fail clearly.

## Verification

Implementation should add requirement-level tests for:

- registration/update rejects malformed `secondary_indexes_json`
- update/rebuild round-trips preserve the collection contract exactly
- append-only index maintenance across `Append`
- latest-state index maintenance across `Put` and `Delete`
- excise/compact/purge/rebuild flows keep derived index state consistent
- read path uses declared index when available and falls back cleanly when not
- safe export / recover / bootstrap preserve contracts and rebuildability

## Risks And Tradeoffs

### Risks Accepted By This Design

- The engine still does not support arbitrary application-specific SQL
  indexing.
- Some workloads may still need future index kinds if they exceed the initial
  bounded model.

### Risks Reduced By This Design

- High-volume filtered reads gain stronger latency support without giving up
  recoverability.
- Index contract remains durable and explicit instead of living in operator
  folklore.
- Export and recovery can treat index entries as derived state with a clear
  rebuild path.

## Bottom Line

Additional secondary indexes now ship as collection-declared
`secondary_indexes_json` contracts backed by engine-maintained derived entries
and deterministic rebuild support. The design improves specific bounded access
paths without turning operational collections into a general SQL indexing
surface.
