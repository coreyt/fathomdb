# Design: grouped-expand stress test coverage

**Release:** 0.4.1
**Scope item:** Ship gate 1 in `dev/notes/scope-0.4.1.md` — "all
four Memex stress shapes have integration test coverage and pass"
**Grouping:** small/quality hardening issues grouped into one design

## Problem

Memex's round-3 response
(`~/projects/memex/dev/notes/fathomdb-searchbuilder-expand-grouped.md:1003-1068`)
flagged four slot shapes that are most likely to surface an edge
case in v1. Three validate semantics that fathomdb believes to
already be correct; the fourth documents a sharp edge. All four
need integration tests before 0.4.1 ships so that the claimed
semantics have test coverage Memex can point to.

The underlying execution path already exists
(`coordinator.rs:1627-1772`, `ROW_NUMBER() OVER (PARTITION BY
root_id)` at 1712,1721) — this design is about locking the
behavior with tests, not changing it.

## Stress shape 1: unbounded fan-out (`WMAction → WMExecutionRecord`)

### Claim to validate

Per-originator `limit` is enforced **independently per root**, not
as a global cap. A heavily-retried originator with 500 expansion
candidates must not starve a low-fan-out originator.

### Test shape

- Set up 50 origin nodes (mimicking `WMAction`).
- Attach child nodes (mimicking `WMExecutionRecord`) via a single
  edge label, with a heavily skewed distribution:
  - Originator 0: 500 children
  - Originators 1-10: 20 children each
  - Originators 11-49: 2 children each
- Run `.nodes(kind).search_all_or_equivalent().expand(slot, dir,
  label, max_depth=1, limit=20).execute_grouped()`.
- Assert:
  - `grouped.groups.len() == 50`.
  - For originator 0: exactly 20 results (capped).
  - For originators 1-10: exactly 20 results each.
  - For originators 11-49: exactly 2 results each (all available).
  - Total result count: `20 + 10*20 + 39*2 = 298`. **Not** 20.
- Assert result sets are disjoint per originator (no cross-leak).

### Location

Add to `crates/fathomdb/tests/grouped_query_reads.rs` as a new
test `expand_per_originator_limit_under_skewed_fanout`.

## Stress shape 2: ordered slot (`WMPlan → WMPlanStep`)

### Claim to validate

Per-slot result order is **undefined**. Callers must sort
client-side. 0.4.1 does not accidentally grow an ordering contract
that later becomes load-bearing.

### Test shape

- Set up 1 originator with 10 children, inserted in a specific
  order (say, reverse lexicographic by logical_id), each carrying
  a `$.sequence_index` property with a different order.
- Run grouped expand.
- Assert: the result set **contains** all 10 children
  (correctness).
- Do **not** assert on order. The test code includes a comment
  citing `docs/reference/query.md` (see
  `design-0.4.1-documentation.md`) that per-slot order is
  undefined.
- A second test sorts the results client-side by
  `$.sequence_index` and asserts the sorted order matches the
  expected sequence — demonstrating the idiomatic
  "sort-client-side" pattern for callers that care about order.

### Location

Same file, new test `expand_per_slot_order_is_unordered` plus
`expand_sort_by_property_client_side` as the companion.

## Stress shape 3: small-kind wide fan-in (`WMClaimEvaluation → WMKnowledgeObject`)

### Claim to validate

Per-originator budget math doesn't degenerate at small N. With 2
originators and a generous per-originator `limit`, each originator
should get its full budget applied against its own expansion pool.

### Test shape

- 2 originator nodes, 200 children each.
- `limit=50`.
- Assert: 2 groups, each with exactly 50 results.
- Assert: each originator's 50 results all come from its own
  children set (no cross-leak).

### Location

Same file, new test `expand_small_originator_set_large_expansion`.

This is a thin variant of shape 1 but catches a different
degeneration mode (small-N per-originator partitioning — if the
CTE accidentally degraded to global `LIMIT N * originator_count`
it would pass shape 1 but fail shape 3 by mixing child sets).

## Stress shape 4: self-expand (`WMKnowledgeObject → WMKnowledgeObject`)

### Claim to validate / document

v1 ships `related_knowledge` at `max_depth=1` only. At
`max_depth=1`, cycles in the edge graph are irrelevant — one hop
cannot loop.

**VERIFIED (post-Pack 4):** The claim "v1 walks blindly with no
cycle detection" is INCORRECT. The recursive CTE uses a
visited-string accumulator (`printf(',%s,', logical_id)`) and a
WHERE clause `instr(t.visited, printf(',%s,', next_id)) = 0` that
blocks revisiting any node already on the current path. The root node
is always pre-seeded as visited. On a cycle A→B→C→A with originator
A, depth=2 returns exactly {B, C}, and depth=3 also returns exactly
{B, C} — the walk back to A is blocked at every depth. No hang, no
OOM. Self-expand is safe at any `max_depth`. This is NOT a sharp edge.
The doc callout in Pack 12 must describe this actual behavior.

### Test shape

- Three nodes A, B, C all of the same kind, with edges
  `A→B, B→C, C→A` forming a cycle.
- Test 1: `max_depth=1`, originator A. Assert: exactly 1 result
  (B). Cycle-irrelevant.
- Test 2: `max_depth=2`, originator A. Assert: results contain B
  and C. If the implementation dedups, A may also appear (the
  walk reaches A via C→A); if it doesn't, A may appear multiple
  times or be excluded by row dedup at the `ROW_NUMBER` step.
  **Assert whichever the shipped behavior actually is** and
  document that exact behavior in
  `docs/reference/query.md`.
- Test 3: `max_depth=3`, originator A. Run with a small hard-limit
  (`limit=10`) to prove termination. Assert the test does not
  hang / blow up memory. Document the worst-case bound.

The goal is to lock the current behavior with tests so that
future depth>1 work (if it ever happens) has a starting point.

### Location

Same file, new test `expand_self_expand_at_depth_1` +
`expand_self_expand_at_depth_greater_than_1_no_cycle_detection`.

## Non-shape tests (also required for ship)

### Target-side filter integration (from `design-0.4.1-expand-target-filter.md`)

Separately from the four stress shapes, target-side filter needs
its own integration test covering:

- Non-fused `filter=JsonPathEq(...)` against a non-indexed kind.
- Fused `filter=JsonPathFusedEq(...)` against an indexed kind.
- Fused filter against a kind without the schema → raises
  `BuilderValidationError::MissingPropertyFtsSchema`.
- `discussed_in` + `action_kind` partition pattern from Memex
  use case 3 — verifies that one edge label backing two
  semantically-distinct slots works end-to-end.

These live in the same file for locality but are conceptually
part of the target-filter design's acceptance criteria.

### Cross-binding parity

Each of the four stress shapes gets a thin smoke test in Python
and TypeScript, exercising the same call chain end-to-end and
asserting the same top-level counts. These are smoke tests — full
correctness coverage lives in the Rust tests. Python file:
`python/fathomdb/tests/test_grouped_expand.py` (create if
missing). TypeScript file:
`typescript/packages/fathomdb/test/grouped-expand.test.ts`
(create if missing).

## Acceptance

1. All four new Rust tests pass.
2. Cross-binding smoke tests pass in Python and TypeScript.
3. Target-filter integration tests pass (scope also covered by
   `design-0.4.1-expand-target-filter.md`).
4. No regression in existing `grouped_query_reads.rs` tests.
5. Shape 4 test results inform the sharp-edge callout in
   `docs/reference/query.md` (see `design-0.4.1-documentation.md`)
   — the docs must match the tested behavior exactly.

## Out of scope

- Performance benchmarking. These are correctness tests, not perf
  tests. Memex's 44s → single-digit latency claim is a caller-side
  win, not an engine-side perf requirement — no benchmark gate.
- Stress testing the coordinator at realistic Memex data volumes
  (~1M WMExecutionRecord rows). Unit-scale fixtures are enough for
  semantic correctness; real-volume testing happens in Memex's
  adoption PR.
- Cycle detection implementation — the CTE already has per-path
  visited-node deduplication (confirmed by Pack 4 tests). No further
  cycle-detection work is needed for 0.4.1.

## Risks

- **Shape 4 surprise.** If depth>1 self-expand with a cycle
  actually blows up (OOM or timeout), that changes this from "lock
  with tests and document the sharp edge" to "add cycle detection
  before shipping." Hedge: run shape 4 early in implementation and
  re-scope if needed.
- **Python/TypeScript chain verification.** If Memex's Python call
  chain doesn't already work end-to-end (see open question in
  `design-0.4.1-searchbuilder-expand-chain.md`), the cross-binding
  smoke tests will fail fast and scope grows.
