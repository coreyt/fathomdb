# Architecture Note: Current Memex Support

**Status:** Current
**Last updated:** 2026-04-22

## Purpose

This note records the current integration contract between Memex and
FathomDB. It replaces older gap-analysis wording that mixed implemented
features, stale risks, and future ideas.

FathomDB must not absorb Memex domain tables. Engine-owned typed tables stop at
`runs`, `steps`, and `actions`; Memex concepts such as knowledge items, goals,
meetings, reminders, trails, and entities remain application-defined node and
edge kinds.

## Current Fit

Memex can model its world on FathomDB using:

- nodes for application entities, knowledge objects, events, goals, and tasks;
- edges for references, provenance links, dependencies, and containment;
- chunks for canonical searchable text;
- `source_ref` and provenance events for traceability;
- supersession/retire semantics for soft deletion and correction;
- per-kind FTS property schemas for selected structured properties;
- per-kind vector tables as internal projection storage.

The active architecture boundary is defined in
[ARCHITECTURE.md](./ARCHITECTURE.md) and
[engine-vs-application-boundary.md](./engine-vs-application-boundary.md).

## Supported Today

The current release line supports these Memex-relevant substrate behaviors:

- atomic writes across nodes, edges, chunks, runtime rows, FTS projection work,
  and explicit vector inserts;
- retire and replace-upsert cleanup for chunks, FTS rows, and per-kind vector
  rows owned by the affected node kind;
- source-based trace and excise workflows for bad inference outputs;
- safe export, integrity checks, and targeted repair operations;
- per-kind property FTS through `fts_props_<kind>` tables;
- one database-wide vector embedding identity for query/regeneration identity
  checks;
- explicit vector regeneration from canonical chunks for a target node kind.

Raw `VecInsert` remains a low-level/admin/import mechanism. It is not the
desired normal Memex integration path.

## Active Gaps

The main remaining Memex gaps are generic FathomDB lifecycle and projection
ergonomics, not domain-schema mismatch:

- restore during a grace period after retire;
- hard purge/forget with auditable cascading cleanup;
- richer query result shapes that combine search hits with bounded graph
  context;
- batch write helpers for common graph + chunk + edge compositions;
- FathomDB-managed vector projection that does not require Memex to submit raw
  vectors or run explicit regeneration as the normal application path.

The authoritative design for the vector gap is
[design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md](./notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md).

## Vector Direction

Memex wants vector search to act like a managed FathomDB projection:

1. configure one database-wide embedding engine;
2. enable vector indexing for `KnowledgeItem` from chunk text;
3. write canonical Memex data;
4. let FathomDB asynchronously maintain the per-kind vector projection.

That is the target design. Current code still exposes explicit regeneration
and low-level `VecInsert`; docs should describe those as current operational
surfaces, not as the preferred long-term Memex path.

## Related Docs

- [ARCHITECTURE.md](./ARCHITECTURE.md)
- [engine-vs-application-boundary.md](./engine-vs-application-boundary.md)
- [repair-support-contract.md](./repair-support-contract.md)
- [schema-declared-full-text-projections-over-structured-node-properties.md](./schema-declared-full-text-projections-over-structured-node-properties.md)
- [design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md](./notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md)
