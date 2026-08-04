---
title: Nested-Source Projections
date: 2026-08-04
target_release: 0.8.21 Slice 60
status: ACTIVE
desc: Make declared FathomDB projections read scalar values from nested canonical bodies and expose them through public attribute query APIs.
blast_radius: projection registry, projection extraction, property index, Rust/Python/TypeScript query contracts, Memex C-1 integration
---

# Nested-Source Projections

## Decision

HITL selected this design for Memex B15 (steward ledger `seq-242` (remap authorization), decision
`b15-nested-source-projections`). A projection's source may be a declared,
nested path in a node's canonical body. FathomDB derives the existing
engine-owned projection, property, and property-full-text rows from that
source, and exposes declared projected attributes through public query APIs.

The canonical node body remains authoritative. This design does **not** add a
first-class engine attribute-write store, ask Memex to duplicate attributes as
top-level scalar fields, or permit a consumer-owned projection database.

This is a successor to the future-fulfillment mechanics in
[OPP-12/C-1 converged contract](record-lifecycle-protocol/OPP-12-C1-converged-contract.md).
It is authorized as FathomDB 0.8.21 Slice 60. The implementation remains on a
retained candidate branch until a separate HITL decision authorizes its merge.

## Problem and current boundary

FathomDB 0.8.20 has a durable projection registry, engine-owned canonical
attribute/property rows, and property full-text storage. Its projection
extractor, however, derives a field named `name` only from the top-level body
path `$."name"` and skips object and array values. Its public Python and
TypeScript `SearchFilter` contracts do not expose the Rust-internal attribute
filter.

Memex's canonical entity body instead stores typed attributes at paths such
as:

```json
{
  "attributes": {
    "core:deadline": {
      "group": "core",
      "key": "deadline",
      "value": "2026-10-01",
      "confidence": 1.0
    }
  }
}
```

Requiring a second `deadline` field at the body root would create two
representations of the same fact. Asking Memex to materialize a private index
would recreate a projection system that the engine must own. Neither is an
acceptable C-1 outcome.

## Goals

- Let an application map its declared entity schema to FathomDB projection
  specifications, including nested scalar values.
- Preserve the body as the sole authoritative representation of those values.
- Reuse FathomDB's transactional projection replacement, lifecycle cleanup,
  and property full-text index rather than add consumer-maintained copies.
- Make exact projected-attribute filtering and projected-attribute text search
  public and consistent across Rust, Python, and TypeScript.
- Keep existing top-level projection declarations and their query behavior
  compatible.

## Non-goals

- A generic JSONPath interpreter, arbitrary SQL expression, or a public query
  API that accepts an unvalidated body path.
- An engine-owned attribute CRUD resource, per-attribute concurrency token, or
  independent attribute lifecycle.
- Flattening or rewriting existing canonical bodies during migration.
- Blending projected-property lexical scores with body full-text or vector
  search in the first delivery.
- A Memex release, schema migration, or changes to Memex's authorization and
  body-write protocol.

## Projection declaration

`ProjectionSpec` gains an additive source declaration. Omitted `source`
preserves the current behavior: the projection named `name` reads the direct
body member `name`.

```text
ProjectionSpec {
  name: "core:deadline",
  roles: [Filterable, Searchable],
  source: Path(["attributes", "core:deadline", "value"]),
}
```

A source path is an ordered list of JSON object-member names, not a JSONPath
string. Segments are literal: `core:deadline` is one member name, and dots,
brackets, wildcards, or SQLite syntax have no special meaning. Every segment
uses the existing safe member-name grammar: it is non-empty and contains no
double quote, backslash, or ASCII control character. This permits the path to
be encoded safely in SQLite's quoted-member JSON path form. The registry
validates the path, stable projection name, and supported role/source
combination before accepting the declaration.

The first delivery supports one terminal JSON scalar. It retains the existing
engine canonical-text representation: strings are verbatim; integers and reals
use SQLite's decimal `CAST AS TEXT`; booleans are `true` or `false`; a missing
path or null produces no projected row. The equality API consequently accepts
that canonical text, not a new typed scalar union. A string `"1"` and number
`1` therefore compare equal, as do any other values whose canonical text is the
same.

**HITL decision (2026-08-04): Memex B15 explicitly accepts this
type-collapsed equality.** Slice 60 must pin the behavior in all three bindings
with a real-database test: a projected string `"1"` and projected number `1`
match the same equality predicate. It is not an accidental coercion and is not
an invitation to broaden the query language. A consumer that needs
type-distinct equality must encode a typed string in its canonical body; a
typed-property model remains a separately designed follow-on.

A terminal object or array is a `WriteValidation` error, never silently
stringified or partially indexed. During `configure_projections`, a composite
encountered while backfilling causes the entire configuration transaction to
roll back; during a normal node write, it causes that write transaction to roll
back. Rust, Python, and TypeScript expose the same typed validation error. This
deliberately makes the application choose a scalar leaf such as `value`.

The registry stores the source declaration with the projection definition. On
every node write, FathomDB evaluates it against the canonical body and replaces
the derived rows in the same transaction as the node revision. Rewrites,
deletes, purge, erasure, and projection reconfiguration retain the current
derived-state cleanup semantics. No projected value is independently writable.

An application that genuinely needs a repeated property must declare that
shape in a later, separately designed cardinality extension. This avoids
accidentally defining fan-out, ordering, duplicate, and query semantics while
solving Memex's scalar `EntityTypeSpec` attributes.

## Public query surface

The public contracts add the following portable concepts and expose them in
Rust, Python, and TypeScript together:

```text
SearchFilter {
  ...existing fields,
  attributes: [(ProjectionName, CanonicalText)],
}

Engine.search_projected_text(
  query: String,
  name: ProjectionName,
  filter: SearchFilter?,
  view: ReadView?
) -> SearchResult
```

`SearchFilter.attributes` is an AND of existential predicates over a node's
engine-derived property rows. It retains the existing Rust shape
`Vec<(String, String)>`; Python and TypeScript expose equivalent ordered pairs
of projection name and canonical text. Each named projection must be declared
`Filterable`; an unknown name or unsuitable role is a validation error, never
a body scan. It exposes equality only, matching the current engine's indexed
pre-KNN attribute predicate. Missing or null sources produce no row and make
every equality predicate false. It does not imply negation, range, type-aware,
or arbitrary-body-filter semantics.

The unified `Filter` grammar remains unchanged in this delivery. A conversion
from a `SearchFilter` carrying attributes to `Filter` must fail with
`InvalidFilter` rather than silently drop those predicates; the reverse
conversion produces an empty attribute list. A future attribute term for the
unified grammar needs a separate dispatch design for both search and
`read.list`.

`search_projected_text` is a lexical property-index search for exactly one
declared field. `name` must be declared `Searchable` with property full-text
enabled. It applies the ordinary kind/source/status, validity, and attribute
filters. Results are ordered by FTS5 `bm25` ascending, then write cursor
ascending, and use the engine's existing fixed search-result limit. A returned
hit's score is `-bm25`, so higher remains better in the existing hit convention.
It returns an ordinary `SearchResult` with `branch=Text`,
`soft_fallback=None`, and no explanation; its projection cursor is the query
snapshot marker, not a page cursor. The endpoint has no caller pagination in
this first delivery.

It does not fall back to scanning bodies, invoke vector search, or fuse results
with the existing body-search endpoint. Multi-field score aggregation and
cursor pagination are separate designs, rather than accidental semantics of an
array-valued `names` argument.

The projection name—not its source path—is the public query key. This keeps
the query contract stable when an application reorganizes its body internally,
and prevents a query caller from reading arbitrary body fields simply by
supplying a path.

## Memex mapping

Memex converts each filterable or searchable `EntityTypeSpec` attribute to one
FathomDB projection declaration. For a normal scalar attribute, the source is:

```text
["attributes", "<group>:<key>", "value"]
```

Memex continues to validate entity data, author the complete canonical body,
and enforce graph authorization. It configures the FathomDB registry
idempotently at startup, then relies on FathomDB to replace/query its derived
property rows. The mapping creates neither a flattened copy in the body nor a
Memex-owned index.

The paired consumer work may begin only after these public contracts land;
it must use `Engine.write` for node validity windows rather than direct SQLite
mutation. FathomDB 0.8.20 already supports those validity fields on node
writes, so that part is a consumer integration change, not a storage-engine
feature prerequisite.

## Compatibility and rollout

- Existing projections without `source` keep their direct top-level lookup.
- Changing a registered projection's source is a destructive declaration
  change, with the same explicit-drop requirement as another incompatible
  projection change; omission never drops it.
- Existing projection registry data needs an additive schema representation for
  the source declaration. No `INSERT ... SELECT` backfill and no body rewrite
  is permitted.
- `configure_projections` backfills a newly accepted declaration from existing
  canonical nodes in its normal transaction, using the same source evaluator
  as governed writes. A separate full reindex remains an operator recovery
  concern, not an implicit consumer migration.
- Python and TypeScript must not expose a partial version of the query contract
  while Rust alone can use it. Bindings ship only with matching validation and
  error behavior.

## Acceptance evidence

- A nested Memex-shaped scalar source creates the expected canonical attribute,
  property, and property-full-text rows through a normal node write.
- Rewriting, deleting, purging, erasing, and changing a projection declaration
  leave no stale projected values or text hits.
- Existing direct top-level projections retain their current results.
- Exact predicates, the rule that missing/null values never match, role
  validation, deterministic ordering, read views, status filtering, and
  projected text search agree across all three public bindings.
- Property-based tests cover literal path segments and scalar round-trips;
  integration tests use a real database and normal writes, not direct SQLite
  mutation.
- A Memex fixture maps its `EntityTypeSpec` declarations without top-level
  duplicate fields or a consumer-maintained projection table.

## Rejected alternatives

**First-class engine attribute-write store.** This would be justified if
attributes needed independent writes, concurrency, provenance, ACLs, history,
or lifecycles apart from the node body. Memex does not require those semantics.
Adding them now would create a second authority and a materially broader data
model.

**Flattened canonical fields.** This makes FathomDB's current extractor work,
but duplicates Memex facts and creates update-consistency obligations.

**Consumer-owned projections or direct SQLite writes.** Both bypass FathomDB's
transactional ownership and lifecycle cleanup; direct SQLite mutation is also
unsupported production behavior.

**Raw nested paths in filters.** They leak body layout into the public contract,
make validation and indexing unclear, and turn a declared projection system
into an arbitrary document-query surface.
