# Schema-Declared Full-Text Projections Over Structured Node Properties

**Status:** Current
**Last updated:** 2026-04-22

## Purpose

This is the current developer contract for full-text search over selected JSON
properties. It supersedes older global-table designs.

## Contract

Property full-text search is schema-declared and per-kind:

- applications register one or more JSON property paths for a node kind;
- optional path weights are supported for BM25 tuning;
- FathomDB extracts scalar text values from active node properties;
- extracted text is stored in a per-kind FTS5 table named
  `fts_props_<kind>`;
- property FTS rows are derived projection state and may be rebuilt at any
  time from canonical nodes.

The former global `fts_node_properties` table is historical. Active code should
refer to per-kind `fts_props_<kind>` tables only, and public docs should prefer
FathomDB APIs over direct SQL table access.

## Registration

The admin surface registers property paths for a kind:

```python
db.admin.register_fts_property_schema("Book", ["$.title", "$.body"])
```

Weighted entries use the richer path-spec form:

```python
from fathomdb import FtsPropertyPathSpec

db.admin.register_fts_property_schema_with_entries("Book", [
    FtsPropertyPathSpec(path="$.title", weight=10.0),
    FtsPropertyPathSpec(path="$.body", weight=1.0),
])
```

Registration creates or refreshes the per-kind FTS projection and can trigger
rebuild work when existing rows need to be indexed.

## Query Semantics

Property FTS participates in the same public text-search surface as chunk FTS.
The query compiler expands property-search branches against the relevant
`fts_props_<kind>` table for the root kind. If a kind has no property FTS table
or no matching rows, the result should degrade cleanly rather than require
callers to know table names.

## Projection Lifecycle

The writer maintains property FTS for normal canonical writes:

- insert/replacement writes update the active row's derived property text;
- retire removes the row's property FTS projection;
- restore or projection rebuild repopulates the row from canonical properties.

Physical recovery and safe export treat property FTS as rebuildable. The
canonical source of truth is `nodes.properties`, not the FTS table.

## Implementation Anchors

- Admin registration: `crates/fathomdb-engine/src/admin/fts.rs`
- Writer projection maintenance: `crates/fathomdb-engine/src/writer/mod.rs`
- Query rewrite/coordinator behavior: `crates/fathomdb-engine/src/coordinator.rs`
- Public guide: [docs/guides/property-fts.md](../docs/guides/property-fts.md)
