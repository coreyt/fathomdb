# Design: Adaptive Text Search Surface

## Status

Proposed.

No backwards compatibility is required. Existing `text_search()` semantics,
result shapes, and documentation may be changed directly to the new design.

## Purpose

Define the next search-surface design for `fathomdb` around four coherent
changes:

1. a single adaptive `text_search()` entry point
2. a first-class `SearchHit` result surface with score and source metadata
3. recursive property extraction for declared object paths
4. a narrow `fallback_search(strict, relaxed)` helper

This design replaces the earlier idea of splitting the public API into
multiple `text_search_*` methods. The database, not the client, should own the
default retrieval policy for text search.

## Problem

`fathomdb` already has a unified FTS entry point in name: `text_search()`
searches chunk-backed and property-backed text through one query surface.

That is not yet sufficient for real application search workloads.

The current design still leaves too much burden on callers:

- `text_search()` exposes a constrained safe syntax, but not a strong retrieval
  policy
- result rows expose matched nodes, but not enough search semantics to rank,
  debug, or explain results well
- property FTS is too shallow for nested payload-heavy nodes
- common strict-then-relaxed retrieval patterns must be rebuilt in SDK or
  application code

The result is that application code still has to compensate for a search
surface that is nominally unified but operationally incomplete.

## Design Goals

1. Keep one obvious default text-search entry point.
2. Make the engine responsible for safe, useful default search behavior.
3. Expose enough search metadata for applications to make informed decisions
   without reconstructing FTS behavior themselves.
4. Expand property FTS to cover declared nested structures better, while
   preserving the current derived-state and rebuild discipline.
5. Add a bounded best-effort search helper without committing to full general
   query branch composition.
6. Preserve cross-language parity across Rust, Python, and TypeScript.
7. Avoid changing backup/export format unless required by stored derived state.

## Non-Goals

1. Do not add multiple primary `text_search_*` entry points.
2. Do not expose raw FTS5 syntax as the default search surface.
3. Do not add full query `union()` / branch-composition semantics in this
   tranche.
4. Do not add a full staged search/traverse/hydrate language in this tranche.
5. Do not move domain-specific ranking or semantic filtering into the engine.

## Decision Summary

- Keep a single public `text_search(query, limit)` builder/API entry point.
- Redefine `text_search()` as an adaptive engine-owned search surface rather
  than a thin string-to-FTS lowering step.
- Introduce search-specific result types carrying hit metadata instead of
  returning only node rows for FTS-backed reads.
- Extend property FTS schemas to support recursive extraction at declared
  object paths.
- Add a narrow fallback helper for strict-vs-relaxed search behavior and use
  the same retrieval-policy machinery internally from `text_search()`.
- Treat all ranking/explanation surfaces as query-time behavior. Backup,
  export, restore, and integrity remain projection-centric rather than
  search-semantics-centric.

## Core Product Principle

The caller should not have to choose among several search methods to get
reasonable recall.

The default `text_search()` should be the easy, reliable way to find text in
`fathomdb`. The engine should carry the complexity of:

- safe query interpretation
- strict-versus-relaxed retrieval policy
- unified chunk/property search
- result metadata production

Applications should still own domain-specific reranking, semantic post-filter
rules, and presentation logic.

## Current State

Today the search surface is shaped roughly like this:

- the public query builders expose a single `text_search(query, limit)` method
- the builder parses a constrained safe subset into `TextQuery`
- the compiler lowers that typed query into SQLite FTS5-safe `MATCH` syntax
- the FTS branch compiles to a UNION over chunk FTS and property FTS
- execution returns `QueryRows`, which contain nodes plus `was_degraded`, but
  no search-specific metadata
- property FTS concatenates declared values into a single flat text document

This design is safe and narrow, but it is not yet an application-friendly
search surface.

## Proposed Public Surface

### 1. Keep one primary `text_search()`

Rust:

```rust
QueryBuilder::nodes("Goal").text_search("ship quarterly docs", 10)
```

Python:

```python
engine.nodes("Goal").text_search("ship quarterly docs", limit=10)
```

TypeScript:

```ts
engine.nodes("Goal").textSearch("ship quarterly docs", 10)
```

This remains the default text-search API in every surface.

### 2. Add search-specific execution/result surface

Introduce a search-oriented result shape rather than overloading plain
`QueryRows` with ad hoc fields.

Recommended core types:

```rust
pub enum SearchHitSource {
    Chunk,
    Property,
}

pub enum SearchMatchMode {
    Strict,
    Relaxed,
}

pub struct SearchHit {
    pub node: NodeRow,
    pub score: f64,
    pub source: SearchHitSource,
    pub match_mode: SearchMatchMode,
    pub matched_path: Option<String>,
}

pub struct SearchRows {
    pub hits: Vec<SearchHit>,
    pub was_degraded: bool,
}
```

Recommended SDK parity:

- Python: `SearchHit`, `SearchRows`, enums for `SearchHitSource` and
  `SearchMatchMode`
- TypeScript: corresponding exported types

Whether plain `execute()` on an FTS-backed query returns `SearchRows` directly
or whether search gets a distinct terminal verb is an API choice. With no
backwards-compatibility requirement, the preferred design is:

- `execute()` returns `SearchRows` when the driving table is FTS
- `execute()` returns `QueryRows` for non-search queries

That choice makes the result reflect the operation actually performed.

Alternative:

- preserve one terminal method per query type, e.g. `execute_search()`

The preferred direction is still to keep the public query surface compact and
natural; the implementation can determine which terminal shape best fits the
rest of the builder.

### 3. Add narrow explicit helper

```rust
fallback_search(strict_query, relaxed_query, limit)
```

This is not a generic query-composition language.

It is a focused helper for a common retrieval pattern:

- try strict search first
- if strict underperforms or misses, run relaxed search
- merge under explicit narrow rules

This helper may also be used internally by adaptive `text_search()`.

## Adaptive `text_search()` Semantics

`text_search()` remains the default and recommended entry point. It should be
adaptive, inspectable, and engine-owned.

### High-level behavior

1. Parse caller input into the existing safe `TextQuery` subset.
2. Treat that parsed query as the **strict** interpretation.
3. Derive a **relaxed** interpretation when appropriate.
4. Execute strict search first.
5. If strict search is strong enough, return strict hits.
6. If strict search is weak or empty, run relaxed search and merge or replace
   under bounded policy.
7. Return `SearchHit`s that describe what happened.

### Strict interpretation

Strict mode is the existing safe typed query subset:

- bare terms
- quoted phrases
- implicit `AND`
- uppercase `OR`
- uppercase `NOT`

This remains the core safe search grammar. The difference is that it is no
longer the full user-facing retrieval policy by itself.

### Relaxed interpretation

Relaxed mode is a derived query intended to recover useful partial matches when
strict interpretation is too brittle.

Examples of relaxation policy:

- break implicit-AND term sets into per-term alternatives
- preserve quoted phrases when possible
- drop or soften exclusions when they overconstrain recall
- search the same chunk/property union surface

The exact relaxed form is engine-owned and does not become a second public
query language.

### Fallback trigger policy

The first implementation should stay simple and explicit.

Recommended initial rule:

- if strict returns zero hits, run relaxed

Optional later rule:

- if strict returns fewer than `min(limit, N)` hits, run relaxed and merge

The trigger policy should be visible in docs and test fixtures.

### Result metadata

Every returned `SearchHit` should make the search behavior inspectable:

- `score`: raw engine score used for ordering
- `source`: whether the hit originated from chunk text or property text
- `match_mode`: whether the hit came from the strict or relaxed branch
- `matched_path`: optional, initially for property hits only when available

This metadata is essential to making adaptive search debuggable and useful.

## Ranking Semantics

The engine should expose a stable raw search ordering, not application-specific
relevance policy.

### Initial ranking requirements

1. FTS-backed search must no longer behave like unordered candidate selection.
2. Search results must expose score in the public result.
3. Scores should be comparable within a single search execution.
4. Merged strict/relaxed results must have deterministic precedence rules.

Recommended initial merge policy:

- strict hits rank ahead of relaxed hits by default
- duplicates are deduped by logical ID
- for duplicate logical IDs, prefer the higher-priority hit according to:
  1. strict over relaxed
  2. higher score within same mode

This remains intentionally simple.

## SearchHit Result Design

### Why a dedicated type

FTS-backed results are not just node rows. They carry retrieval semantics that
plain node reads do not.

A dedicated `SearchHit` type keeps that distinction explicit and prevents
search metadata from being awkwardly bolted onto generic row types.

### Source metadata

`source` should initially distinguish:

- chunk-backed hit
- property-backed hit

This matters for:

- application-side ranking
- debugging
- UI display
- operator testing

### Path metadata

`matched_path` is optional in the first cut.

It becomes much more valuable once recursive property extraction is present. If
the first implementation cannot cheaply emit path metadata for all property
hits, the API should still reserve a place for it.

## Recursive Property Extraction

### Current limitation

Property FTS currently works best for shallow scalar fields. Object values are
skipped, and extracted values are flattened into one text blob.

### Decision

Extend schema-declared property FTS to support recursive extraction for
declared object paths.

Recommended contract extension:

```rust
register_fts_property_schema(
    kind,
    [
        PropertyFtsPath::scalar("$.title"),
        PropertyFtsPath::recursive("$.payload"),
    ],
    separator,
)
```

Equivalent Python and TypeScript admin surfaces should exist.

If a narrower schema API is preferred, the same concept can be modeled with a
path-plus-mode record rather than bare strings.

### Extraction behavior

For recursive paths:

- if the path resolves to an object, walk descendant scalar leaves
- preserve stable path order
- include strings directly
- stringify numbers and booleans
- flatten arrays of scalars
- skip nulls and missing values
- optionally retain leaf path identity in derived rows or auxiliary metadata

### Storage-shape choice

This design does **not** require a full fielded/path-aware property-FTS
materialization yet.

The first step should remain compatible with the current derived-state model:

- property FTS stays rebuildable
- schema table remains canonical
- projection rows remain derived

However, recursive extraction should be implemented in a way that does not
block future path-aware materialization.

## `fallback_search(strict, relaxed)` Helper

### Purpose

Provide explicit access to the bounded strict-vs-relaxed search pattern
without adding full query branch composition.

### Proposed scope

The helper should:

- accept two search shapes
- execute strict first
- execute relaxed only when policy says to do so
- dedup and merge with deterministic precedence
- return `SearchRows`

It should not:

- allow arbitrary query-tree branching
- expose generic `union()` semantics
- become a general multi-stage query DSL

### Relationship to adaptive `text_search()`

`text_search()` should use the same retrieval-policy machinery internally.

This keeps behavior consistent:

- default search is easy
- explicit fallback remains available when an advanced caller wants control

## Core Architecture Changes

## `fathomdb-query`

Required changes:

- keep `TextQuery` as the strict safe-grammar representation
- add a search-policy layer above `TextQuery` that can derive relaxed search
  plans
- teach the compiler to produce ranked search candidates and carry search
  metadata forward

Recommended additions:

- `SearchPlan`
- `SearchPolicy`
- `SearchBranch` with exactly two supported branches in v1: strict and relaxed

This should remain narrower than a generic query composition framework.

## `fathomdb-engine`

Required changes:

- add search-specific execution path(s) returning `SearchRows`
- stop treating FTS execution as “find logical IDs, then project nodes only”
- compute and propagate score and source metadata
- support deterministic strict/relaxed merging

`QueryPlan` / `explain()` should also be expanded or paralleled with a
search-specific explanation surface so operator tooling can inspect:

- strict branch SQL or plan
- whether fallback ran
- how many hits came from each source

The read-result diagnostics design should remain conceptually separate from
payload results, but the search result itself must expose enough metadata to be
useful.

## Rust Public Facade

Required changes:

- export `SearchHit`, `SearchRows`, and supporting enums
- add terminal methods or adaptive terminal return types in the Rust facade
- rework serde/wire payloads used by Python and TypeScript bindings

No backwards compatibility is required, so existing FTS result contracts may be
replaced rather than layered.

## Python Binding Layer

The Python bindings should remain thin and Rust-owned in semantics.

Required changes:

- update Rust `python_types.rs` to serialize/deserialize `SearchRows`
- add Python dataclasses/enums in `python/fathomdb/_types.py`
- update `python/fathomdb/_query.py` so `text_search()` produces the new
  result contract
- update package exports in `python/fathomdb/__init__.py`

Python API goal:

```python
rows = engine.nodes("Goal").text_search("ship blocked", limit=10).execute()
for hit in rows.hits:
    hit.node.logical_id
    hit.score
    hit.source
    hit.match_mode
```

If Python continues to expose one `execute()` method, its return typing should
document that text-search queries produce `SearchRows`.

## Python SDK Documentation

Required documentation changes:

- rewrite `text_search()` docs around adaptive behavior rather than a narrow
  syntax-only contract
- document `SearchHit` and `SearchRows`
- document fallback behavior, match modes, and source metadata
- document recursive property extraction in admin/property FTS guides

Existing docs that describe `text_search()` as a transparent UNION over chunk
and property FTS remain true but incomplete; they must be updated to describe
the new result and retrieval behavior.

## TypeScript SDK

The TypeScript SDK should mirror Python conceptually and closely in coverage.

Required changes:

- add `SearchHit`, `SearchRows`, and enums to `typescript/packages/fathomdb/src/types.ts`
- update `query.ts` terminal behavior for text-search queries
- update package exports in `index.ts`
- keep AST construction thin and Rust-aligned

TypeScript API goal:

```ts
const rows = engine.nodes("Goal").textSearch("ship blocked", 10).execute();
for (const hit of rows.hits) {
  hit.node.logicalId;
  hit.score;
  hit.source;
  hit.matchMode;
}
```

The SDK must not invent different adaptive-search semantics from Rust. It
should only surface what Rust defines.

## Consumer Documentation

The following documentation areas must be rewritten, not patched lightly:

1. query guide
2. text-query syntax guide
3. property FTS guide
4. query API reference
5. Python README/API examples
6. TypeScript README/API examples

Key documentation changes:

- `text_search()` is now described as adaptive safe search, not just a typed
  boolean subset
- `TextQuery` remains the strict interpretation grammar
- `SearchHit` is the public unit of search results
- `recursive` property extraction is a declared projection behavior
- `fallback_search()` is documented as a bounded helper, not a general query
  composition API

## Backup / Restore / Rebuild / Integrity

### Export / backup

No backup format change is required unless recursive path definitions require a
schema-table migration.

The critical principle remains:

- search rows are derived state
- schema/contract rows are canonical

`safe_export()` should continue to preserve canonical schema declarations and
not rely on exporting live derived FTS rows as authoritative state.

### Restore and rebuild

Restore, rebuild, and rebuild-missing should continue to treat FTS as derived
state. If recursive extraction is added to property schemas:

- rebuild must use the new extraction rules
- restore must regenerate property FTS using the same rules
- semantic checks should continue to validate projection presence/drift, not
  search ranking semantics

### Integrity semantics

`check_integrity()` and `check_semantics()` should remain focused on
projection-health questions:

- missing rows
- stale rows
- drifted extraction content

They should not attempt to validate adaptive search result quality or fallback
policy.

## Testing Strategy

The current test matrix is not enough. Search tests must stop asserting mostly
row counts and start asserting search semantics.

### Core unit tests

Add tests for:

- strict query parsing
- relaxed-query derivation
- fallback trigger policy
- score ordering
- strict-versus-relaxed dedup precedence
- chunk-versus-property source metadata
- recursive property extraction over nested objects and arrays

### Engine integration tests

Add tests for:

- `text_search()` returns `SearchRows`
- fallback runs only when policy requires it
- duplicate logical IDs across strict/relaxed and chunk/property sources are
  merged deterministically
- restore/rebuild preserve searchability under recursive extraction

### Cross-language parity tests

Cross-language scenarios must expand beyond `count` / `expect_min_count`.

Required new assertions:

- ordered hit logical IDs
- hit source
- match mode
- optional matched path where present
- whether fallback was used, if surfaced

Python and TypeScript driver fixtures must both validate the richer wire
payload shape.

### Harness scenarios

The Python and TypeScript harnesses should gain explicit search semantics
scenarios:

- strict hit only
- strict miss, relaxed recovery
- mixed chunk/property hits
- recursive nested payload search
- rebuild / restore after recursive-property schema use

### Stress testing

Stress tests should continue to assert correctness under concurrency and load,
but add search-specific invariants:

- no panics or deadlocks under adaptive search workloads
- deterministic hit ordering under repeated runs
- fallback behavior remains stable under repeated concurrent reads
- property FTS rebuild plus search remains correct after heavy writes

## Migration / Rollout

No backwards compatibility is required, so the rollout should favor coherence
over bridging.

Recommended sequence:

1. redesign core result types and query execution for FTS-backed reads
2. add adaptive search policy and narrow fallback helper
3. add recursive property extraction and rebuild support
4. update Python and TypeScript bindings/SDKs
5. rewrite consumer docs
6. replace cross-language and stress fixtures with the new search assertions

## Open Questions

1. Should `execute()` dynamically return a search-specific type for FTS-backed
   queries, or should search queries gain a distinct terminal method?
2. Should relaxed search trigger only on zero hits in v1, or also on
   underfilled results?
3. Should `matched_path` be required in the first recursive-extraction cut, or
   remain optional until later path-aware materialization work?
4. Which score function is the stable initial contract for ordering and public
   `score` exposure?
5. Should fallback usage be surfaced explicitly on `SearchRows`, or inferred
   from per-hit `match_mode`?

## Done When

- `text_search()` remains the single primary text-search entry point
- FTS-backed reads return a search-specific result surface with score and
  source metadata
- property FTS supports recursive extraction for declared object paths
- a bounded strict-vs-relaxed fallback helper exists and shares machinery with
  adaptive `text_search()`
- Rust, Python, and TypeScript all expose the same semantics
- docs describe adaptive search rather than only strict syntax lowering
- backup/restore/rebuild remain correct under the new projection rules
- cross-language, harness, integration, and stress tests assert the new search
  semantics explicitly
