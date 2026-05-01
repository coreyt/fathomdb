---
title: Op-Store Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: Operational collections, authoritative row semantics, and transactional behavior with primary writes
blast_radius: op-store tables; PreparedWrite::OpStore; REQ-053; REQ-057..REQ-059; AC-060b; AC-061..AC-063
status: draft
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
