# Repair Support Contract

**Status:** Current
**Last updated:** 2026-04-22

## Purpose

Define which corruption classes `fathom-integrity` supports with automated
repair in the current release line and the deterministic policy used for each
supported class.

This document is normative for the current production claim.

## Supported Automated Repair

The following corruption classes are inside the automated repair contract:

- missing or stale FTS projections
  - supported via `rebuild --target fts` and `rebuild-missing`
- missing vector profile schema after recovery
  - supported via `restore_vector_profiles`
- recoverable physical corruption where canonical tables can be replayed into a
  fresh database and projections rebuilt
  - supported via `recover`
- bad lineage from a known `source_ref`
  - supported via `trace --source-ref` and `excise --source-ref`
- duplicate active logical IDs
  - supported via `repair --target duplicate-active`
- broken runtime FK chains
  - supported via `repair --target runtime-fk`
- orphaned chunks
  - supported via `repair --target orphaned-chunks`

## Deterministic Repair Policies

### Duplicate active logical IDs

Repair command:

- `fathom-integrity repair --target duplicate-active --db <path>`

Policy:

- keep exactly one active row per `logical_id`
- choose the winner deterministically by:
  1. greatest `created_at`
  2. then greatest `row_id` as the tie-break
- supersede all other active rows for that `logical_id`
- recreate the `idx_nodes_active_logical_id` partial unique index
- emit provenance audit rows with event type `repair_duplicate_active_node`

### Broken runtime FK chains

Repair command:

- `fathom-integrity repair --target runtime-fk --db <path>`

Policy:

- delete actions whose `step_id` is missing
- delete actions whose parent step exists but whose step's `run_id` is missing
- delete steps whose `run_id` is missing
- leave valid runs, steps, and actions untouched
- emit provenance audit rows with event types:
  - `repair_delete_broken_action`
  - `repair_delete_broken_step`

### Orphaned chunks

Repair command:

- `fathom-integrity repair --target orphaned-chunks --db <path>`

Policy:

- treat a chunk as orphaned when its `node_logical_id` has no active node
- delete the orphaned chunk
- delete matching `fts_nodes` rows
- delete matching rows from any per-kind vec table that exists for the
  affected node kind
- emit provenance audit rows with event type `repair_delete_orphaned_chunk`

## Operator Workflow

Recommended order:

1. run `fathom-integrity check`
2. run the targeted `repair` command for the reported class, or `--target all`
3. run `fathom-integrity check` again to confirm the database is clean

If an operator prefers review first, every repair target also supports:

- `--dry-run`

## Vector Recovery Contract

Physical recovery guarantees:

- canonical tables are recovered first
- database-wide vector profile metadata is preserved
- per-kind vector table capability is restored when supported
- vector regeneration metadata is preserved for audit and replay

Physical recovery does **not** treat vector rows as canonical data. Current
releases rebuild vector rows through explicit regeneration from canonical
chunks for the target kind. `VecInsert` is a low-level/admin/import path; rows
written through it are still projection material and should not be documented
as the normal application recovery contract.

The target design is FathomDB-managed vector projection:

1. configure one database-wide embedding engine;
2. configure vector indexing for each kind that needs semantic search;
3. enqueue or run backfill from canonical chunks for existing rows;
4. enqueue high-priority incremental vector work after future canonical writes;
5. expose per-kind vector index status, including pending work, failures, and
   the active embedding identity.

Until that target is implemented, operators should use the current explicit
`regenerate_vector_embeddings` admin/API path to rebuild per-kind vector rows.

Current platform boundary:

- Linux, macOS, and Windows builds enforce regeneration contract validation,
  typed failure auditing, and `sqlite-vec` vector workflows
- CI validates vector-enabled Rust tests and Python harness (baseline + vector
  modes) on both Linux and Windows
