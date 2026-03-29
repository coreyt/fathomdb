# Remaining Work: FathomDB Support For Memex

## Purpose

This note summarizes what still remains for `fathomdb` to support Memex well,
after the operational-store feature is implemented.

It is intentionally narrower than the broader Memex architecture notes:

- it treats the operational store as implemented substrate, not a proposal
- it focuses on remaining generic `fathomdb` work that still matters for Memex
- it separates true blockers from follow-on improvements and optional breadth

## What Is Already Closed

The operational-store implementation closes the main high-churn operational
state gap that previously blocked a clean answer for:

- `connector_health`
- `auto_ingest_sources`
- `tool_usage_stats`
- `session_context`
- `intake_log`

It also closes the generic substrate need for:

- atomic graph + operational writes in one `WriteRequest`
- rebuildable current-state materialization from canonical mutation history
- admin/integrity/recovery coverage for operational collections

For Memex, this means the earlier "operational-state modeling" gap is no
longer the main remaining issue.

## Remaining Work By Priority

### 1. Generic restore/purge lifecycle APIs

This is the highest-priority remaining substrate gap.

Why it still matters:

- Memex's `forget` workflow needs retire, restore during a grace period, and
  hard purge after the grace period.
- Purge must cascade cleanly across chunks, FTS, vec rows, and directly
  connected edges.
- Purge must remain auditable.

What `fathomdb` still needs:

- `restore_logical_id(...)`
- `purge_logical_id(...)`
- likely source-based variants where appropriate
- clear purge modes such as hard purge with tombstone / audit record
- deterministic interaction with projection rebuilds and recovery tooling

Acceptance bar:

- purge leaves no orphaned edges, chunks, FTS rows, or vec rows
- restore is safe after retire and impossible after hard purge
- purge/restore actions are visible in provenance/admin tooling
- recovery and semantic checks understand the resulting state

Memex impact:

- blocks migration of the user-facing `forget` tool

### 2. Write-bundle builders and request-local reference helpers

The core write transaction is already broad enough. The missing piece is
ergonomics.

Why it still matters:

- Memex Phase 1 migration is write-heavy
- large `WriteRequest`s are verbose and easy to wire incorrectly
- client code needs a simpler way to declare related nodes, edges, chunks, and
  runtime rows that reference each other

What `fathomdb` still needs:

- higher-level Rust builders for multi-object write bundles
- matching Python builders/helpers
- request-local aliases or reference helpers for generated IDs

Acceptance bar:

- common Memex flows can be expressed without manually threading dozens of
  precomputed IDs through one raw request object
- builders lower assembly complexity without weakening atomicity or explicit IDs

Memex impact:

- not a hard substrate blocker, but it is the main productivity blocker for
  efficient dual-write migration code

### 3. Richer read/query result shapes

Current query support is still too node-first for several Memex read paths.

Why it still matters:

- Memex drill-in views want root node plus bounded related context
- retrieval wants search hits plus selected linked metadata
- current usage still requires multi-query assembly for many real views

What `fathomdb` still needs:

- bounded traversal + grouping by edge kind in exposed query surfaces
- richer result shapes than flat node lists
- expanded generic predicates, especially timestamp and numeric comparisons
- search-result enrichment support

Acceptance bar:

- a root node plus bounded 1-2 hop related state can be fetched as one compiled
  query/result
- common read patterns no longer require N follow-up lookups per search hit

Memex impact:

- blocks flipping several read paths from SQLite-side joins to `fathomdb`

## Important Follow-On Work That Is Not A Migration Blocker

### 4. `last_accessed` without full supersession

This remains a real design problem, but it is not a Phase 1 blocker.

Preferred direction from the Memex notes:

- batched touch/update API first
- append-only access events with derived materialization second
- operational-store row as an interim fallback

Why it matters:

- write-on-read via full node supersession creates pure write amplification

Why it is not blocking:

- Memex can defer precise `last_accessed` updates during early migration

### 5. Filtered reads for operational collections

This is now a bounded follow-on issue, not a structural gap.

What remains:

- exact-key and history reads exist
- non-key filtered reads, especially for `audit_log`, are still weak

Likely choices:

- add filtered operational reads / declared filter indexes later
- or keep `audit_log` in application-owned SQLite until those reads exist

Why it matters:

- `audit_log` is the one Memex operational table with meaningful read-side
  pressure beyond exact-key lookups

### 6. Schema validation depth for operational payloads

Current v1 behavior is acceptable for Memex.

What remains:

- optional runtime validation if `schema_json` evolves beyond documentation

Why it matters:

- this is more about future hardening than current Memex migration pressure

### 7. Clean up stale vector-cleanup docs and close remaining lifecycle proofs

The old "retire leaves stale vec rows" characterization is stale.

What remains:

- remove stale docs where they still exist
- add lifecycle-proof coverage for:
  - purge plus vec cleanup
  - excision plus vec cleanup
  - restore/purge interaction with regenerated vectors

Why it matters:

- Memex needs confidence that destructive lifecycle operations leave no orphaned
  vector state

## Memex Migration Outlook After Operational Store

### Phase 1: `wm_*` families

Still feasible now.

Primary remaining substrate need:

- write-bundle builders

### Phase 2: meetings, knowledge items, conversation turns

Still feasible in phases, but depends on:

- purge/restore lifecycle
- richer read/query surfaces
- a practical answer for `last_accessed`

### Phase 3: operational telemetry

Operational store reduces this from a major substrate gap to a selective
adoption decision.

Likely outcome:

- `connector_health`, `auto_ingest_sources`, `tool_usage_stats`,
  `session_context`, and `intake_log` can live on the operational store
- `notifications` remain graph-native
- `audit_log` is a product decision: operational store with degraded reads now,
  or application-owned SQLite until filtered reads improve

## Recommended Next Sequence

1. Design and implement generic restore/purge lifecycle APIs.
2. Add write-bundle builders and request-local reference helpers.
3. Expand read/query result shapes and generic predicates.
4. Add a lightweight `last_accessed` strategy, preferably batched touch/update.
5. Improve operational filtered reads if `audit_log` is meant to move.
6. Finish stale vector-doc cleanup and lifecycle-proof testing.

## Bottom Line

The operational store solves an important Memex problem, but it does not by
itself complete Memex support.

After this implementation, the main remaining work is no longer "how do we
model high-churn operational state?" It is:

- lifecycle completeness
- write-assembly ergonomics
- richer read results
- lightweight high-churn metadata updates

Those are generic substrate improvements, not Memex-specific schema work.
