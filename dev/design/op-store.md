---
title: Op-Store Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: Operational collections, authoritative row semantics, and transactional behavior with primary writes
blast_radius: op-store tables; PreparedWrite::OpStore; REQ-053; REQ-057..REQ-059; AC-060b; AC-061..AC-063
status: locked
---

# Op-Store Design

This file owns the operational collection model, `append_only_log` /
`latest_state` behavior, the authoritative op-store table shapes, and the
writer transaction rules for op-store rows committed alongside primary writes.

## 0.6.0 narrowing

0.6.0 keeps op-store as core embedded-database surface, but narrows and
clarifies the earlier draft:

- `operational_mutations.op_kind` is a single-value enum: `append`.
- `operational_collections` does not carry `disabled_at`; 0.6.0 has no
  collection-disable workflow.
- `operational_current` is removed. Op-store data is stored only in
  authoritative regular tables.

Authoritative table split:

- `append_only_log` collections store rows in `operational_mutations`.
- `latest_state` collections store rows in `operational_state`, keyed by
  `(collection_name, record_key)`.

Operational store data includes current operational state such as connector
health, scheduler cursors, queue state, heartbeats, counters, singleton state
blobs, and currently-running markers, plus append-only operational event
streams such as lifecycle/failure logs. Derived performance summaries are not
stored in op-store tables; they are calculated at query time.

## Public surface provenance

Op-store is core 0.6.0 product surface, not an internal implementation detail.
Its public anchor points are:

- `PreparedWrite::OpStore(OpStoreInsert)` in the accepted write carrier.
- authoritative on-disk `operational_*` tables in the same SQLite file
  (ADR-0.6.0-op-store-same-file).
- operator-visible workflows that depend on durable op-store rows, including
  projection failure recording.

The subsystem therefore needs direct REQ/AC traceability rather than ADR-only
implication.

## Collection registry

`operational_collections` is the registry of declared operational collections.
Each row names:

- `name`
- `kind`
- `schema_json`
- `retention_json`
- `format_version`
- `created_at`

  0.6.0 lifecycle posture:

- collections are declared and then preserved as named registry entries
- `kind` is fixed for the life of a collection
- no rename, disable, soft-retire, or alternate "current table" lifecycle
  exists in 0.6.0
- `disabled_at` is intentionally absent from the accepted schema

If a future release needs collection retirement or kind changes, that is new
surface and reopens the op-store ADR.

## Collection kinds

### `append_only_log`

`append_only_log` collections are durable event streams.

- Each accepted write appends exactly one authoritative row to
  `operational_mutations`.
- Rows are never updated in place.
- `op_kind` is fixed to `append` in 0.6.0.
- Historical rows remain the source of truth for this collection kind.

`projection_failures` is the canonical 0.6.0 example of an
`append_only_log` operational collection.

### `latest_state`

`latest_state` collections are authoritative current-state maps.

- Each accepted write upserts exactly one row in `operational_state`, keyed by
  `(collection_name, record_key)`.
- The row in `operational_state` is the authoritative latest state.
- There is no derived `operational_current` table and no background rebuild
  step for current-state reads.

This means "latest" is a storage contract, not a query-time rollup over an
append log.

## Write contract

An op-store write may be committed by itself or in the same batch as primary
entity writes. In both cases:

- the op-store row participates in the same single writer-thread transaction as
  the rest of the accepted batch
- unknown collection names, collection-kind misuse, or registry violations fail
  the submission before partial commit and surface as `OpStoreError`
- JSON payload checks against a registered `schema_id` run save-time,
  pre-commit, and fail as `SchemaValidationError`

When a caller submits "primary write + op-store row" together, the 0.6.0
contract is atomic visibility: callers never observe the primary write without
its same-batch op-store row.

## Read-back contract (`read.collection` / `read.mutations` — Slice 30 / G3)

The governed `read.collection` / `read.mutations` verbs read an
`append_only_log` collection's appended rows back over `operational_mutations`.
The contract:

- the read runs on the **ReaderWorkerPool DEFERRED-tx snapshot path** (never the
  writer `connection.lock()`), preserving single-writer isolation (REQ-018);
- rows are returned strictly **`ORDER BY id`** (the autoincrement PK), so order
  is the append order;
- a **mandatory `limit`** caps each page — there is no public path that issues an
  unbounded SELECT — and the engine clamps the effective SQL `LIMIT` to a ~1M cap
  (`READ_COLLECTION_MAX_LIMIT`). `limit == 0` returns an empty page without
  issuing a SELECT; a `limit` above the cap is clamped, never an unbounded scan;
- an optional **`after_id` cursor** (`WHERE id > ?`) paginates: each next page
  resumes strictly after the previous page's last `id`, with no boundary overlap.
  A negative `after_id` is normalized to the start of the log; an `after_id` past
  the last id (and an unknown / unregistered collection) yields an empty page;
- each returned `OpStoreRow` carries `{ id, collection, record_key, op_kind,
  payload (the stored payload_json), schema_id, write_cursor }`.

**Index-driven pagination (Slice 33 / G3 / F4-READ).** The read-back SELECT is
`WHERE collection_name = ?1 AND id > ?2 ORDER BY id LIMIT ?3`. The step-13
additive index `operational_mutations(collection_name, id)`
(`operational_mutations_collection_id_idx`, `SCHEMA_VERSION 13`) makes this
**index-driven**: `EXPLAIN QUERY PLAN` reports `SEARCH operational_mutations USING
INDEX operational_mutations_collection_id_idx (collection_name=? AND id>?)` — no
`SCAN`, no `USE TEMP B-TREE FOR ORDER BY`, and not the pre-step-13 id-PK walk
(`SEARCH … USING INTEGER PRIMARY KEY (rowid>?)`). The leading `collection_name`
equality fixes the index prefix and the trailing `id` serves **both** the
`after_id` cursor range and `ORDER BY id`, so paginating one collection inside a
genuine ~1M-row multi-collection log is **O(page)**, not O(rows-scanned). The
EXPLAIN gate is pinned in `tests/pr_g3_read_collection.rs`. (This retires the
earlier "cursor/limit hardening under a genuine ~1M-row log is a reserved
follow-on" note.)

`read.mutations` is a mutation-log-oriented alias surface over the identical
read-back. Both verbs land in **lockstep** across the Python and TypeScript SDKs.
The read-back is **read-only and typed** — no raw-SQL or filter-DSL surface.

## Projection-failure ownership

The durable `projection_failures` collection belongs to this subsystem because
it is stored as authoritative op-store data, even though projection scheduling
semantics are owned by `design/projections.md`.

Op-store owns only the storage-side facts:

- failed batches are recorded in a durable `append_only_log` collection
- those rows survive restart like any other op-store rows
- regeneration reads canonical state; it does not treat op-store failure rows as
  a derived queue

`design/projections.md` owns when a failure row is emitted and when the
regenerate workflow is required.

### `projection_failures` payload floor

The durable `projection_failures` collection uses the standard
`operational_mutations` row envelope plus a collection-specific payload whose
public minimum fields are:

- `write_cursor`
- `failure_code`
- `recorded_at`

These fields identify which committed write batch failed projection work, which
stable failure code was recorded, and when the durable audit row was appended.

`design/projections.md` continues to own why the row is emitted and why
`recover --rebuild-projections` is the repair path. This file owns only the
durable row schema floor.
