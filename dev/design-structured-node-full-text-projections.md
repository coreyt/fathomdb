# Design: Per-Kind Structured Property FTS

**Status:** Implemented; retained as design rationale
**Last updated:** 2026-04-22

## Purpose

Describe the current structured-property full-text projection contract. Older
designs used one global `fts_node_properties` table; current FathomDB uses a
separate per-kind FTS5 table named `fts_props_<kind>` for each registered
property schema.

## Current Contract

Applications may register property paths for a node kind. FathomDB extracts
scalar text values from those JSON property paths and indexes them in that
kind's property FTS table.

Current behavior:

- property FTS is opt-in per node kind;
- each registered kind has its own `fts_props_<kind>` table;
- extracted text is derived from active canonical node properties;
- property FTS rows are rebuilt from canonical state during rebuild/repair;
- query compilation routes property-search arms to the target kind's per-kind
  table;
- the old global `fts_node_properties` table is historical and should not be
  described as active storage.

## Schema Shape

Conceptually, each registered kind owns:

```sql
CREATE VIRTUAL TABLE fts_props_<kind> USING fts5(
    node_logical_id UNINDEXED,
    text_content
);
```

Implementations may add generated columns or tokenizer-specific details. The
public contract is per-kind property FTS, not a stable SQL table ABI.

## Write And Retire Behavior

On insert or replacement of an active node whose kind has a registered property
schema, the writer:

1. extracts configured scalar property values;
2. writes or replaces that logical ID's row in `fts_props_<kind>`;
3. keeps projection changes in the same write transaction as the canonical row.

On retire, purge, or replacement that removes searchable values, the writer
removes stale rows from that same per-kind table.

## Rebuild And Restore

Property FTS is derived state. Repair, restore, and recovery correctness must
not depend on preserving property FTS rows. A rebuild scans active canonical
nodes for registered kinds and repopulates `fts_props_<kind>` tables from the
configured property paths.

## Related Docs

- [schema-declared-full-text-projections-over-structured-node-properties.md](./schema-declared-full-text-projections-over-structured-node-properties.md)
- [docs/guides/property-fts.md](../docs/guides/property-fts.md)
