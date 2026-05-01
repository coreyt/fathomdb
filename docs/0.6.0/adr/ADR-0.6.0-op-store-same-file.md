---
title: ADR-0.6.0-op-store-same-file
date: 2026-04-25
target_release: 0.6.0
desc: Operational store lives in the same sqlite file as primary entities; no dual-store
blast_radius: design/engine.md operational store section; dev/design-add-operational-store-feature.md (folded); docs/concepts/operational-store.md (folded); design-operational-payload-schema-validation.md (folded); design-operational-secondary-indexes.md (folded)
status: accepted
---

# ADR-0.6.0 — Op-store in same sqlite file

**Status:** accepted (HITL 2026-04-25, decision-recording)

## Context

0.5.x carried four design documents proposing an "operational store" — a
logical store for application state distinct from primary entities
(nodes, edges, chunks). Critic-A F8 flagged the cluster as four docs
riding one undecided architectural ADR; the question was whether 0.6.0
should:

(a) ship op-store as a separate logical store / file,
(b) drop op-store from 0.6.0 core entirely, or
(c) defer to Phase 2.

## Decision

**No dual-store. FathomDB operational-store needs live in the same
sqlite file as primary entities. Clients keep their own storage for
whatever else they need.**

### What lives in op-store (in scope)

High-churn operational data that belongs inside the embedded database
but does not belong in the primary graph surface:

- connector health (per-minute updates)
- scheduler cursors, poller `last_check` / `last_result`
- queue state
- debounce / heartbeat records
- tool usage counters
- singleton current-state blobs
- `intake_log` lifecycle tracking
- ephemeral "currently running" state

### What does NOT live in op-store

- `scheduled_tasks` — durable definitions with relationships → graph-native
- `notifications` — user-visible, benefit from search/edges → graph-native
- domain entities (goals, meetings, plans, knowledge objects) → graph-native
- arbitrary application SQL tables → client storage, not op-store

The op-store is **not** a back door for application domain schema.

The op-store also does **not** store derived performance summaries or
benchmark rollups. If operators need counts, rates, or timing
aggregates from op-store data, those are computed at query time from
authoritative rows.

### Tables

Three tables, named with the `operational_*` prefix (per OPS-1):

- `operational_collections` — collection registry. Columns:
  `name PK, kind, schema_json, retention_json, format_version,
  created_at`. `kind` ∈ {`append_only_log`, `latest_state`}.
- `operational_mutations` — authoritative append-only rows for
  `append_only_log` collections. Columns:
  `id PK, collection_name FK, record_key, op_kind, payload_json,
  source_ref, created_at`. `op_kind` ∈ {`append`}.
- `operational_state` — authoritative current-state rows for
  `latest_state` collections. Columns:
  `collection_name FK, record_key, payload_json, source_ref,
  created_at, updated_at`; primary key = `(collection_name, record_key)`.

### Two collection kinds

- **`append_only_log`** — every write appends one authoritative row
  to `operational_mutations`; reads stream those rows directly.
- **`latest_state`** — every write upserts one authoritative row in
  `operational_state`; reads come directly from `operational_state`.
  There is no derived / rebuildable companion table for op-store data
  in 0.6.0.

0.6.0 deliberately does **not** add first-class op-store verbs for
`put`, `delete`, or `increment`, and it does not model collection
disable / soft-retire lifecycle. Clients encode state transitions in
their stored payloads and collection choice. If a future operator
workflow needs explicit mutation verbs or collection-disable
semantics, that reopens this ADR.

The four folded docs (op-store feature, op-store concept, payload schema
validation, secondary indexes) all describe primitives that survive as
**sections of the same file's logical surface** — they are not a
separate store, separate database, or separate file.

## Options considered

**A. Same sqlite file (chosen).** Pros: single-file invariant preserved
(matches `dev/notes/0.6.0-rewrite-proposal.md` Essentials §17); single
backup target; transactional consistency between primary entities and
op-store rows; one schema migration story. Cons: schema namespace must
keep op-store tables distinct from primary tables (already the case in
the 0.5.x folded design).

**B. Separate sqlite file alongside primary file.** Pros: cleaner
isolation. Cons: breaks single-file invariant; doubles backup/restore
surface; cross-file transactions are not real → consistency story
weakens; `safe_export` becomes a manifest of two files.

**C. Drop op-store from 0.6.0; clients persist application state
themselves.** Pros: smallest engine surface. Cons: every agentic client
re-implements the same primitives (run/step/action provenance, opt-in
schema validation, bounded secondary indexes); the rewrite proposal's
"thin-plus" thesis includes operational primitives because they are
load-bearing for agentic workflows.

## Consequences

- `dev/design-add-operational-store-feature.md` folds into
  `design/engine.md` (operational-store section) — same-file constraint.
- `docs/concepts/operational-store.md` folds same place.
- `dev/design-operational-payload-schema-validation.md` folds as
  engine-design input (opt-in payload validation contract).
- `dev/design-operational-secondary-indexes.md` folds as engine-design
  input (bounded secondary-index contract).
- Clients are free to keep their own state outside the FathomDB file.
  FathomDB does not document, depend on, or reach into client storage.
- **Single-writer-thread inheritance (OPS-3).** Op-store writes share the
  one Engine writer thread per ADR-0.6.0-single-writer-thread. No
  separate writer lane; no separate transactional fence. Op-store rows
  written from the same `WriteTx` as the primary-entity rows that
  triggered them commit atomically.
- **Txn boundary convention (OPS-4 sketch).** When a single client
  operation produces "primary entity write + step row + op-store row,"
  all three commit in one transaction on the writer thread. Specific
  transactional API shape lives in `design/engine.md`; the invariant
  is that no client-visible "wrote node but not its op-store row" state
  is observable.
- **Schema namespacing (OPS-1).** Op-store tables use the
  `operational_*` table-name prefix (folded-design convention).
  CI rejects any op-store table without the prefix. Migration
  ordering: op-store tables created in the same schema-migration
  step as the primary tables they reference. Tracked as FU-OPS1.
- **safe_export coverage + redaction (OPS-2 followup).** `safe_export`
  must enumerate op-store rows. Op-store JSON payloads may contain
  operator-supplied secrets; the redaction policy (operator-supplied
  redaction list vs default-redact-all-strings vs schema-driven) is
  open. Tracked as FU-OPS2.
- **Op-store payload typing (X-2 cross-cite).** Per
  ADR-0.6.0-typed-write-boundary, `OpStoreInsert { kind, payload:
  serde_json::Value, schema_id: Option<...> }` is the typed carrier
  shape. The `Value` is structural, not raw SQL. Schema validation
  against `schema_id` lives in the JSON-Schema policy (FU-M5).

## Citations

- HITL decision 2026-04-25 (critic-A F8 cluster resolution).
- `dev/notes/0.6.0-rewrite-proposal.md` Essentials §17 (single-file
  invariant).
- Folded inputs: dev/design-add-operational-store-feature.md;
  docs/concepts/operational-store.md;
  dev/design-operational-payload-schema-validation.md;
  dev/design-operational-secondary-indexes.md.
