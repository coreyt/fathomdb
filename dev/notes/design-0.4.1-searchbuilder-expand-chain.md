# Design: `.search().expand()` call chain

**Release:** 0.4.1
**Scope item:** part of roadmap item 8 (grouped expand public surface)
**Related:** `dev/notes/scope-0.4.1.md`

## Problem

Memex's target call shape in
`~/projects/memex/dev/notes/fathomdb-searchbuilder-expand-grouped.md`
is:

```python
engine.nodes("WMGoal").search(query, limit).expand(slot=..., ...).execute_grouped()
```

On the Rust core today, `.search()` returns `SearchBuilder`
(`crates/fathomdb/src/search.rs`), which does **not** have `.expand()`.
`.expand()` is defined on `NodeQueryBuilder`
(`crates/fathomdb/src/search.rs:370-379`) — the builder you get from
`engine.nodes(kind)` before the `.search(...)` call. So the chain
Memex wrote does not compile against Rust today.

Python and TypeScript are a separate question — both expose
`.expand()` on their `Query` class
(`python/fathomdb/_query.py:254-280`, `typescript/packages/fathomdb/src/query.ts:311-318`)
and `.execute_grouped()` on the same class
(`_query.py:356-373`, `query.ts:393-398`). Whether Memex's chain
already works end-to-end in Python depends on whether `Query.search()`
returns `Self` (fluent) or a distinct wrapper type; this needs to be
verified before implementation scope is locked.

## Current state

- `NodeQueryBuilder::expand` at `crates/fathomdb/src/search.rs:370-379`
  — takes `(slot, TraverseDirection, label, max_depth)`, mutates and
  returns `self`.
- `NodeQueryBuilder::compile_grouped` at
  `crates/fathomdb/src/search.rs:419-420` — returns
  `Result<CompiledGroupedQuery, CompileError>`.
- `Coordinator::execute_compiled_grouped_read` at
  `crates/fathomdb-engine/src/coordinator.rs:1588-1620` — the
  execution terminal.
- `SearchBuilder` has its own `.compile()` / `.execute()` for
  non-grouped paths; no `.expand()` / `.compile_grouped()` methods
  today.
- Test coverage at
  `crates/fathomdb/tests/grouped_query_reads.rs:150` uses
  `engine.nodes(kind).expand(...)` — i.e. expand *without* a
  preceding `.search(...)`.

## Design

Expose grouped-expand through the `.search(...)` chain by adding
three methods to `SearchBuilder`:

- `SearchBuilder::expand(slot, direction, label, max_depth) -> Self`
- `SearchBuilder::compile_grouped() -> Result<CompiledGroupedQuery, CompileError>`
- `SearchBuilder::execute_grouped(&Engine) -> Result<GroupedQueryRows, _>`
  *(mirrors the convenience terminal that already exists in the
  Python/TypeScript bindings but is absent in Rust today)*

**No AST or execution changes.** `SearchBuilder` already owns a
`QueryBuilder` internally; `.expand()` on `SearchBuilder` delegates
to `QueryBuilder::expand()` (`crates/fathomdb-query/src/builder.rs:370`)
just like `NodeQueryBuilder::expand` does today. The compile and
execute paths are identical.

This is deliberately a thin plumbing change. The work the scope doc
calls "grouped expand public surface" is largely about giving Memex
a chain that compiles — the execution, the per-originator limit
semantics, and the return shape all already exist.

### Rust convenience terminal

Add `SearchBuilder::execute_grouped(&self, engine: &Engine)` that
wraps `compile_grouped() + execute_compiled_grouped_read()`. Rust
callers currently have to do this two-step manually; Python and
TypeScript bundle it. Closing this ergonomic gap at the same time
keeps the three surfaces consistent and makes Rust test-writing
simpler for the stress-test work
(`dev/notes/design-0.4.1-stress-tests.md`).

Also add the same convenience terminal to `NodeQueryBuilder` so the
`engine.nodes(kind).expand(...).execute_grouped()` shape used by
existing tests can be tightened from the current
two-step pattern.

### Python / TypeScript verification (not an implementation task yet)

Before locking 0.4.1 scope, verify that Memex's Python call chain
actually works today:

```python
engine.nodes("WMGoal").search(query, limit).expand(...).execute_grouped()
```

If `Query.search()` returns the same `Query` instance, this already
works and no binding change is needed. If it returns a distinct
type (e.g. `SearchResult` wrapper) that lacks `.expand()`, the
bindings need a parallel fix to the Rust change above.

**Action:** read `python/fathomdb/_query.py` around the `search`
method and confirm the return type. Same for TypeScript. Fold any
required binding changes into this design before implementation
starts.

## Acceptance

1. `SearchBuilder::expand`, `compile_grouped`, `execute_grouped`
   compile and pass existing tests.
2. A new Rust test in `crates/fathomdb/tests/grouped_query_reads.rs`
   exercises the full `.nodes(kind).search(query, limit).expand(...)
   .execute_grouped()` chain end-to-end.
3. Python call chain from Memex's note compiles and returns
   `GroupedQueryRows`-equivalent shape. Add a Python integration
   test mirroring the Rust one.
4. TypeScript parity test.
5. No regression in `grouped_query_reads.rs`.

## Out of scope

- Target-side filter on expand — separate design
  (`design-0.4.1-expand-target-filter.md`).
- Stress test coverage for per-originator limit, ordering, and
  self-expand — separate design
  (`design-0.4.1-stress-tests.md`).
- Documentation — separate design
  (`design-0.4.1-documentation.md`).
- `NodeQueryBuilder::expand` signature changes — keep current
  signature stable.

## Open questions

1. Does Python `Query.search()` return the same `Query` (making
   Memex's chain already work), or a distinct wrapper? Verify
   before locking binding scope.
2. Should `SearchBuilder::execute_grouped` take `&Engine` (mirroring
   `execute`) or be a consuming method? Match existing `execute`
   for consistency.
