# Design: target-side filter on `.expand(...)`

**Release:** 0.4.1
**Scope item:** part of roadmap item 8 (grouped expand) — the
genuinely new feature
**Related:** `dev/notes/scope-0.4.1.md`

## Problem

Memex's use case 3 needs one edge label (`discussed_in`) to back two
semantically-distinct slots, partitioned by a property on the
expanded node (`action_kind == "decision"` vs `"action_item"`). The
target call shape:

```python
engine.nodes("WMMeeting")
    .search(query, limit)
    .expand(slot="decisions", edge="discussed_in", direction="IN",
            filter=F.json_eq("$.action_kind", "decision"), limit=20)
    .expand(slot="action_items", edge="discussed_in", direction="IN",
            filter=F.json_eq("$.action_kind", "action_item"), limit=20)
    .execute_grouped()
```

Today, `QueryStep::Traverse` in the AST carries only
`{direction, label, max_depth}` and has no predicate field
(`crates/fathomdb-query/src/ast.rs:61-68`). The coordinator
execution at `coordinator.rs:1627-1772` builds a recursive CTE that
joins `edges → nodes` with no WHERE clause on target node
properties. Any partitioning has to happen client-side today, which
defeats the per-originator `limit` (the client sees 20 action rows
per originator, 10 of each kind, instead of 20 of each kind).

## Locked semantics

Per `scope-0.4.1.md`: the target-side filter accepts **the same
predicate grammar as the main query path** — `filter_json_eq`,
0.4.0 named fused filters, etc. Semantics are locked; the builder
surface is an implementation choice.

## Current state

- `QueryStep::Traverse` AST: `{direction: TraverseDirection, label:
  String, max_depth: usize}` at
  `crates/fathomdb-query/src/ast.rs:61-68`.
- `QueryStep::Filter(Predicate)` is a **separate AST step**, not a
  field on traverse. Filters are applied to the current result set,
  not to an in-progress traversal.
- Traverse compile at `crates/fathomdb-query/src/compile.rs:536-551`
  builds a recursive CTE joining `edges` to `nodes` with no target
  predicate.
- Traverse execution at
  `crates/fathomdb-engine/src/coordinator.rs:1627-1772` applies
  per-originator `limit` via
  `ROW_NUMBER() OVER (PARTITION BY root_id)` then
  `WHERE rn <= hard_limit` (lines 1712, 1721).
- Predicate compilation for the main query path lives in
  `crates/fathomdb-query/src/compile.rs` and handles
  `Predicate::JsonPathEq`, `Predicate::JsonPathFusedEq` (0.4.0
  named fused filters), etc.

## Design

### AST change

**Serde concern resolved:** `QueryStep` at
`crates/fathomdb-query/src/ast.rs:30` derives only
`Clone, Debug, PartialEq, Eq` — no `Serialize`/`Deserialize`.
Adding `filter: Option<Predicate>` is NOT a wire-format break;
it is a pure source-level additive change. Proceed without concern.

Add a `filter: Option<Predicate>` field to `QueryStep::Traverse`:

```rust
QueryStep::Traverse {
    direction: TraverseDirection,
    label: String,
    max_depth: usize,
    filter: Option<Predicate>,
}
```

`None` preserves today's behavior exactly — no WHERE clause on the
target side. The existing `.traverse()` and `.expand()` builder
methods set `filter: None` and are unchanged.

### Builder change

Add a `filter` parameter to `NodeQueryBuilder::expand` and
`SearchBuilder::expand` (once the latter exists — see
`design-0.4.1-searchbuilder-expand-chain.md`). Two candidate
surfaces:

**Option A — builder argument:**
```rust
pub fn expand(
    mut self,
    slot: impl Into<String>,
    direction: TraverseDirection,
    label: impl Into<String>,
    max_depth: usize,
    filter: Option<Predicate>,
) -> Self
```

Cleanest for Rust, but every existing call site has to add
`None` — a mechanical churn on the existing
`grouped_query_reads.rs` tests.

**Option B — chained variant:**
```rust
pub fn expand(...) -> Self { /* unchanged */ }
pub fn expand_where(
    mut self,
    slot: ..., direction: ..., label: ..., max_depth: ...,
    filter: Predicate,
) -> Self
```

No churn; `expand_where` is the opt-in entry point. Uglier name,
two methods to maintain.

**Option C — expand returns an `ExpandClauseBuilder`:**
```rust
builder.expand(slot, direction, label, max_depth)
       .filter(Predicate)   // optional
       .done()              // back to NodeQueryBuilder
```

Most flexible; matches how the main query path composes
`filter(...)` as a fluent step. But `.expand()` currently returns
`Self` and `grouped_query_reads.rs` depends on that — option C is a
breaking change to the existing return type.

**Recommendation: Option A.** The existing `.expand()` sites are
few (mostly tests) and the mechanical `None` churn is acceptable.
Option B bifurcates the API and Option C is a breaking change for
negligible ergonomic win. Lock at implementation time; flag in PR
if something surfaces.

### Python / TypeScript surface

The binding `.expand()` methods currently take
`(slot, direction, label, max_depth)`. Add an optional `filter`
keyword that takes the binding's existing filter expression type
(whatever `filter_json_eq` currently returns/constructs on the main
query path). Exact type name per language, but the grammar must be
the same as main-path filters — no `.expand()`-specific predicate
grammar.

### Compile change

In `crates/fathomdb-query/src/compile.rs` around lines 536-551,
when `QueryStep::Traverse.filter` is `Some(predicate)`, compile
the predicate via the existing predicate compiler and inject the
resulting SQL fragment into the recursive CTE's terminal JOIN with
`nodes` — specifically, the WHERE clause that qualifies target
rows *before* the per-originator `ROW_NUMBER()` partition.

**Critical ordering:** the filter MUST be applied before
`ROW_NUMBER()` / `LIMIT`. Otherwise the per-originator budget would
count filtered-out rows, producing the exact starvation bug
grouped expand is meant to prevent (one originator fills its budget
with matches, another fills it with non-matches that get dropped,
leaving the second originator with fewer usable results).

### Fused-filter compatibility

0.4.0 named fused filters (`JsonPathFusedEq`,
`JsonPathFusedTimestampCmp`) require a registered property-FTS
schema on the target kind. Because a traverse can return **multiple
kinds in one slot** (edge-scoped, not kind-scoped), fused filters
on the expanded side raise the question: what if one target kind
has the required FTS schema and another doesn't?

Locked behavior per the 0.4.0 fused-filter contract in
`memory/project_fused_json_filters_contract.md`: raise
`BuilderValidationError::MissingPropertyFtsSchema` at filter-add
time, not silently degrade. For expand filters, this check runs
against **every target kind reachable via the edge label**, which
may require a new introspection path (edge label → set of possible
target kinds).

**Open question:** is the target-kind set discoverable at
builder-validation time, or only at execution time? If only at
execution time, raise `MissingPropertyFtsSchema` at execute time
with a clear error tying the missing schema to the specific kind
reached via the edge. Non-fused `JsonPathEq` has no such concern
and can always be validated statically.

### Execution change

In `coordinator.rs:1627-1772`, the recursive CTE body gets the
compiled predicate fragment injected at the `edges → nodes` join.
Per-originator `ROW_NUMBER()` at line 1712 is unchanged; it now
partitions over the filtered result set. Hard-limit check at
line 1706 stays the same.

The `was_degraded` flag on `GroupedQueryRows`
(`coordinator.rs:234-241`) is unchanged — degradation semantics
don't apply to target filtering.

## Acceptance

1. `QueryStep::Traverse.filter` field exists; existing traverse
   tests pass with `filter: None`.
2. `.expand(...)` with `filter=Some(JsonPathEq(...))` returns only
   target nodes matching the predicate.
3. `.expand(...)` with `filter=Some(JsonPathFusedEq(...))` against
   a kind with the required FTS schema returns matching rows.
4. Same fused filter against a kind *without* the schema raises
   `BuilderValidationError::MissingPropertyFtsSchema` with a clear
   error message naming the kind and path.
5. Per-originator limit honored: test with 10 originators ×
   `limit=5`, filter matches ~50% of targets per originator,
   result has up to 5 matches per originator — not 2-3 (which
   would indicate filter-after-limit).
6. Python + TypeScript integration tests exercising the
   `discussed_in` + `action_kind` partition pattern from Memex's
   use case 3.

## Out of scope

- Predicate grammar divergence between main path and expand.
  **Must be the same grammar** — no expand-specific filter DSL.
- Filtering on edge properties (as opposed to target-node
  properties). Edges in fathomdb carry labels but not arbitrary
  properties today; this question doesn't arise.
- Filter-based short-circuiting of traversal itself (stopping the
  walk at a filtered node). Out of scope; filter applies only to
  terminal target selection.

## Risks

- **Target-kind fused-filter validation** is the biggest unknown.
  If the multi-kind-per-slot case forces execute-time validation,
  that's a small contract shift from the 0.4.0 "at filter-add time"
  guarantee. Acceptable IMO — still loud, still raises, just at a
  different point in the lifecycle — but flag it in the 0.4.1
  changelog explicitly.
- **Compile-path complexity.** Injecting a predicate into the
  recursive CTE body is more invasive than the usual
  `QueryStep::Filter` step, which is a separate compile phase.
  Keep the predicate compiler output shape portable by returning
  a `SqlFragment { sql: String, params: Vec<Value> }` that can be
  consumed by both the main filter step and the traverse step.
