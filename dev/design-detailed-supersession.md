# Design: Supersession And Restore Contract

**Status:** Current
**Last updated:** 2026-04-22

## Purpose

Define the active supersession contract for canonical graph rows and derived
projections. This replaces older typed-write-era design text whose module paths
and projection assumptions are stale.

## Canonical Identity

FathomDB separates physical row identity from logical entity identity:

- `row_id` identifies one physical version of a node or edge.
- `logical_id` identifies the application entity across versions.
- The active version is the row for a `logical_id` with `superseded_at IS NULL`.
- Replacements append a new physical row and supersede the previous active row.
- Edges reference logical identities so traversals resolve to the current active
  endpoint at read time.

The writer implementation lives in `crates/fathomdb-engine/src/writer/mod.rs`.

## Write Semantics

The writer applies each accepted `WriteRequest` inside one SQLite transaction:

1. validate canonical rows and references;
2. append new node, edge, chunk, run, step, and action rows;
3. supersede or retire older active rows requested by the write;
4. update deterministic derived projections that are part of the synchronous
   write path;
5. commit or roll back the whole request.

Supersession never mutates the old canonical row into the new value. It marks
the old row inactive and preserves the new row as a separate fact.

## Projection Semantics

Derived projections follow canonical active state:

- chunk FTS rows in `fts_nodes` are derived from active chunks;
- property FTS rows live in per-kind `fts_props_<kind>` tables;
- vector rows live in per-kind `vec_<kind>` sqlite-vec tables;
- retired or replaced nodes must have stale derived rows removed for the
  affected logical identity and kind.

Per-kind FTS and vector table names are implementation details. Application
code should go through FathomDB APIs instead of querying projection tables
directly.

## Restore Semantics

Restore means making a previous canonical row active again. A correct restore
operation must:

- select the intended prior physical row deterministically;
- mark the currently active row superseded;
- clear `superseded_at` on the restored row, or append a new active row that
  preserves restore provenance;
- rebuild or update derived projections from the restored canonical state;
- emit provenance/audit evidence for the restore.

The current repair and restore behavior is documented in
[restore-reestablishes-retired-projection-state.md](./restore-reestablishes-retired-projection-state.md)
and [repair-support-contract.md](./repair-support-contract.md).

## Repair And Recovery

Physical recovery treats canonical rows as authoritative and projections as
rebuildable. FTS rows can be rebuilt from canonical state. Current vector
recovery preserves profile/capability metadata and uses explicit regeneration
for embedding rows; the target managed-vector design will make vector
projection catch-up FathomDB-owned asynchronous work.

## Tests

Supersession-sensitive tests should cover:

- one active row per logical ID after replace-upsert;
- retire removes active projection rows for chunks, property FTS, and vectors;
- restore reestablishes projection rows for the restored active state;
- failed writes leave both canonical and derived state unchanged;
- repair workflows do not depend on projection tables as canonical state.
