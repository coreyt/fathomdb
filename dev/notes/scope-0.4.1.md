# FathomDB 0.4.1 scope

Two headline items:

1. **`SearchBuilder.expand(slot=...).execute_grouped()`** — roadmap
   item 8. The public-surface grouped expand that Memex is waiting on
   to fix the search-latency crisis.
2. **Async property-FTS rebuild (shadow-build)** — roadmap item 9,
   pulled forward from 0.5.0. Decoupled from item 10b; ships on its
   own merits (baking time, and eliminates the register-schema stall
   that Memex currently works around with
   `_warn_if_large_rebuild`). Item 10 remains in 0.5.0.

Memex's round-3 response in
`~/projects/memex/dev/notes/fathomdb-searchbuilder-expand-grouped.md`
(lines 913–1111) is final from their side for item 8. Memex accepted
0.5.0 deferral for item 9 and explicitly said "no urgency" — pulling
it forward into 0.4.1 is a fathomdb-side decision driven by (a)
release-engineering value of baking the async rebuild machinery
before item 10b lands on top of it, and (b) spreading the breaking-
change load across two releases instead of stacking it in 0.5.0.

> Terminology note: Memex referred to this release as 0.4.2. fathomdb
> is shipping it as 0.4.1 — purely a version-number choice, no scope
> difference on item 8. Item 9 is entirely fathomdb's addition to the
> scope.

## Critical path

**Item 8** is on Memex's critical path for the search-latency crisis
tracked under their `project_search_latency`. The two worst offenders
— `canonical_outcome_hits` at ~44s and `execution_record_docs` at
~12s — are dominated by post-search N+1 fan-out, which grouped expand
eliminates. Expected impact per Memex: `canonical_outcome_hits` drops
from ~44s toward single-digit seconds.

**Item 9** is not on Memex's critical path — they've already paid the
m004 rebuild cost and explicitly said no urgency. Pulling it forward
is a release-engineering win, not a consumer unblock. It lets the
shadow-build machinery bake for a release cycle before item 10b's
storage migration depends on it.

No Memex schema changes are gated on 0.4.1; the adoption for item 8
is a pure caller-side rewrite, and item 9 is a roll-forward unlock
(Memex will drop `_warn_if_large_rebuild` and the maintenance-window
docstring once 0.4.1 ships).

## What ships

### `.expand(slot, edge, direction, limit, ...).execute_grouped()`

Thin public-surface wrapper over primitives that already exist in the
traverse path. Tested shape at
`crates/fathomdb/tests/grouped_query_reads.rs:150` uses
`.expand("task_descendants", TraverseDirection::Out, "HAS_TASK", 2)`
today — the 0.4.1 work is exposing this through the public
`SearchBuilder` surface across Rust, Python, and TypeScript bindings,
plus the new target-side filter support below.

#### Locked semantics

- **Multi-slot per call.** Chained `.expand(...)` calls each add a
  slot. `.execute_grouped()` returns all slots in one round trip.
  Backing type `ExpansionSlotRows { root_logical_id, nodes }` is
  already the execution shape.
- **Per-originator `limit`.** Slot `limit` is the cap per originating
  search hit, not a global cap. Goal A cannot starve goal B.
- **Same-label `max_depth > 1`.** Works today for a single repeated
  edge label. Result is a flat node list at that slot; path
  structure is not preserved.
- **Full `NodeRow` properties returned.** No projection parameter;
  callers read properties directly off expanded nodes. This makes
  the N+1 for display-title loads (Memex's
  `list_knowledge_relationships` → `load_knowledge_object` pattern)
  go away on day one.
- **Multi-kind per slot.** Traverse is edge-label scoped, not
  kind-scoped. A single `derived_from` expand naturally returns
  both `WMObservation` and `WMExecutionRecord` in one slot.
- **Target-side filter — new surface.** `.expand(...)` accepts the
  same predicate grammar as the main query path: `filter_json_eq`,
  the 0.4.0 named fused filters, etc. Lets one edge back two
  semantically-distinct slots via property partitioning (Memex's
  `discussed_in` + `action_kind` case).

Target Python shape (locked):

```python
grouped = (
    engine.nodes("WMMeeting")
    .search(query, limit)
    .expand(slot="decisions", edge="discussed_in", direction="IN",
            filter=F.json_eq("$.action_kind", "decision"),
            limit=20)
    .expand(slot="action_items", edge="discussed_in", direction="IN",
            filter=F.json_eq("$.action_kind", "action_item"),
            limit=20)
    .execute_grouped()
)
# grouped.groups: list[ExpandedHit]
#   .hit: original SearchHit
#   .slots: {"decisions": [NodeRow, ...], "action_items": [NodeRow, ...]}
```

The exact filter-param surface (full builder vs. dict helper) is an
implementation choice — the *semantics* are locked: anything that
works on main-path filter works target-side.

#### Stress cases Memex flagged (must pass before ship)

Memex called out four slot shapes to stress-test v1 against,
ordered by how likely they are to surface an edge.

1. **`WMAction → WMExecutionRecord` (unbounded fan-out).** Most
   likely to surface a per-originator budgeting bug. WMExecutionRecord
   is the largest kind in Memex's store; single heavily-retried
   actions can carry hundreds of records. Test shape: 50 originators
   × `limit=20`, with the distribution skewed so one originator has
   500 candidate expansions and another has 2. Each must receive
   its own `limit` budget — the skewed originator must not starve
   the others. **Required test coverage in 0.4.1.**

2. **`WMPlan → WMPlanStep` (ordered slot).** The stress here is
   result ordering within a slot. Plan steps carry a sequence and
   Memex's UI iterates in order. Grouped expand returns
   `Vec<NodeRow>` per slot — the contract 0.4.1 commits to is:
   **per-slot order is undefined and callers must sort client-side
   by a schema-level key if order matters.** Document this
   explicitly in the `.expand()` reference. Memex plans to sort by
   `$.sequence_index` client-side either way; the doc prevents
   silent drift if we later add implicit ordering.

3. **`WMClaimEvaluation → WMKnowledgeObject` (small-kind wide-fan-in).**
   Inverse of case 1: tiny originator set, large expansion per
   originator. The per-originator budget math should give each
   originator the full `limit` without degenerating at small N.
   Test shape: 2 originators × `limit=50` where each has 200
   candidates.

4. **`WMKnowledgeObject → WMKnowledgeObject` (self-expand).**
   Knowledge relationships can form cycles in Memex's data model
   (A↔B↔C↔A is legal). v1 ships `related_knowledge` at depth=1
   only, so cycles are not a concern in the shipped path. **Sharp
   edge to document:** same-kind self-expand at `depth>1` currently
   walks blindly — no cycle detection, no dedup. Document this in
   the `.expand()` reference and the `max_depth` parameter docs.
   If a future caller wants depth>1 on a self-expand, cycle
   handling becomes a follow-up scope item; it is not in 0.4.1.

#### Non-asks, confirmed out of scope

- **Named multi-hop aliases across different edge labels.** Memex
  withdrew this ask. All three known multi-hop cases (`WMPlanStep
  → WMPlan → WMGoal`, `WMExecutionRecord → WMAction → WMGoal`,
  `WMCommitment → WMGoal → WMPlan`) will be denormalized Memex-side
  as single-label edges written at insert time. 0.4.1 needs no
  server-side alias machinery.
- **Engine-side scoring/ranking of expanded entities.** Memex ranks
  client-side.
- **Cross-kind unions in one slot.** Already works — traverse is
  edge-scoped, not kind-scoped.
- **Global (cross-originator) `limit` budgets.** Per-originator
  only. Apply cross-originator caps client-side if needed.
- **Projection / `include_target_properties`.** Not needed — full
  `NodeRow` always returned.
- **Path-structure preservation for `max_depth > 1`.** Flat node
  list only.

### Async property-FTS rebuild (shadow-build)

`Engine::admin::register_fts_property_schema_with_entries`
(`crates/fathomdb-engine/src/admin.rs:1558-1656`) currently runs the
full FTS rebuild inside an IMMEDIATE transaction and stalls every
writer on that engine for the duration. 0.4.1 decouples registration
from rebuild: the call persists the new schema and returns in
milliseconds, the rebuild runs in the background, and reads stay
correct throughout.

Full implementation design:
`dev/notes/design-0.4.1-async-rebuild.md`.

#### Locked semantics

- **`register_fts_property_schema_with_entries` is semi-async.** The
  call is synchronous with respect to schema validation and metadata
  persistence — if it returns `Ok`, the schema row is committed and
  the rebuild is queued. It is asynchronous with respect to the FTS
  rebuild itself — the new schema's rows are not live when the call
  returns.
- **Reads during the rebuild window serve the old schema.** Always
  correct with respect to the schema that was live when the register
  call started. No partial-results window. No inconsistency window.
- **First-registration scan fallback.** When a kind registers its
  *first* property-FTS schema, there is no old index to read from.
  For this narrow case, reads fall back to a JSON scan over nodes of
  that kind until the shadow build completes. Slow but correct. Not
  a general read path.
- **Writes during rebuild double-write.** A write to a kind with a
  pending rebuild pays both the old-schema extraction cost (keeps
  the live index correct) and the new-schema extraction cost (keeps
  the shadow caught up). Roughly 2× per-row property-FTS cost during
  the rebuild window.
- **Atomic swap on completion.** When the shadow is caught up, a
  short IMMEDIATE transaction swaps the new schema's FTS5 rows into
  the live table and clears the rebuild state. The swap is a bulk
  FTS5 insert scoped to one kind — minutes on large kinds, not the
  current 5–10 minutes because the JSON-walking cost already ran
  async. See the design doc for the full cost breakdown and the
  post-0.4.1 "zero-stall swap" follow-up question.
- **Crash semantics.** On engine restart during a pending rebuild,
  the shadow state is discarded and the register call must be
  re-invoked by the caller. 0.4.1 does not persist rebuild progress
  across restarts.
- **Eager-rebuild escape hatch.** For tests and small kinds that
  want the old synchronous behavior, the register call accepts an
  optional `RebuildMode::Eager` parameter that runs the rebuild
  in-line and stalls the caller as today. Default is `Async`.

#### Observability

- Coordinator exposes `get_property_fts_rebuild_progress(kind) ->
  Option<RebuildProgress>` returning `{rows_done, rows_total,
  started_at, estimated_completion}`.
- Python and TypeScript bindings expose the same.
- Rebuild start, batch completion, and swap events logged at
  `info` level with kind + row counts.
- New metric: rebuild-window duration, batch count, double-write
  overhead per kind.

#### Non-goals for 0.4.1

- **Cross-restart rebuild resume.** First cut: restart = drop
  shadow, caller re-registers. Persistent progress is a post-0.4.1
  scope item.
- **User-facing cancellation of a rebuild.** Runs to completion.
- **Multiple concurrent rebuilds on different kinds.** First cut
  serializes; can relax later if profiling shows value.
- **Zero-stall final swap.** The atomic swap is still
  O(rows-in-kind) FTS5 insert cost, possibly minutes on large
  kinds. This is a major improvement over today (5–10 minutes of
  JSON walking + FTS5 insert, down to 1–2 minutes of FTS5 insert
  only), but not zero. Zero-stall swap is a post-0.4.1 design
  question — see the design doc open questions.

#### Memex impact

On ship, Memex can drop:
- `_warn_if_large_rebuild` probe at
  `m004_register_fts_property_schemas_v2.py:185-213`.
- Maintenance-window caveat in the same file's docstring.
- m004-maintenance-window footnote in `project_search_latency`.

And replace:
- The probe with a live progress observer backed by
  `get_property_fts_rebuild_progress`.

No Memex schema changes required.

## Documentation deliverables

These are load-bearing for the 0.4.1 ship and must be written
before release, not after:

1. **`docs/reference/query.md`** — `.expand()` / `.execute_grouped()`
   API reference. Must cover:
   - Multi-slot semantics and the `ExpandedHit { hit, slots }`
     return shape.
   - Per-originator `limit` guarantee, with a worked example showing
     skewed distribution.
   - `max_depth` meaning (same-label only; flat result; undefined
     order).
   - Target-side filter grammar (which predicates work, which don't).
   - **Explicit statement that per-slot result order is
     undefined** — callers must sort client-side if order matters.
   - **Explicit sharp-edge note on same-kind self-expand at
     `depth>1`** — no cycle detection in 0.4.1.
2. **`docs/guides/querying.md`** — worked example following Memex's
   use case 1 shape (goal → commitments + provenance_actions +
   plan_steps). Serves as the canonical grouped-expand guide.
3. **`docs/guides/property-fts.md`** — update the existing rebuild
   cost callout at `:185-196` (currently documents eager rebuild as
   a maintenance-window operation) to describe the async shadow
   build, the read-from-old-schema guarantee, the double-write cost
   during the rebuild window, and the eager-mode escape hatch.
4. **`docs/reference/admin.md`** (or wherever
   `register_fts_property_schema_with_entries` is documented) — add
   the `RebuildMode` parameter, the `get_property_fts_rebuild_progress`
   method, and the semi-async contract.
5. **Changelog entry** — link to both grouped-expand docs and call
   out the per-originator limit guarantee. Under a separate
   **"Behavior change"** heading, call out the async rebuild
   semantics shift: register-then-query no longer sees the new
   schema immediately, reads serve the old schema during the
   rebuild window.

## Test coverage required before ship

**Item 8 (grouped expand):**

- Existing test `crates/fathomdb/tests/grouped_query_reads.rs`
  extended with the four Memex stress shapes (see
  `design-0.4.1-stress-tests.md`).
- New integration test covering target-side filter on an expand
  slot, validating the `discussed_in` + `action_kind` partition
  pattern.
- Cross-binding smoke tests (Python, TypeScript) exercising the
  multi-slot call shape end-to-end.
- Regression: existing `.search()` / `.traverse()` callers
  unchanged. Grouped expand is purely additive.

**Item 9 (async rebuild):**

- Eager-mode regression: existing tests that call
  `register_fts_property_schema_with_entries` continue to pass
  when updated to pass `RebuildMode::Eager` explicitly, or when
  eager remains the default for existing call sites (design doc
  decides the default-mode policy).
- Async-mode correctness: register under async mode, query before
  swap completes, assert old-schema results are returned. After
  swap, assert new-schema results.
- First-registration scan fallback: register the first schema for
  a new kind under async mode, query during the shadow build,
  assert scan-fallback results match what the async-rebuilt index
  will return after swap.
- Double-write correctness: insert a node during an in-progress
  rebuild, complete the rebuild, assert the new node is findable
  via the new schema (i.e. the double-write kept the shadow
  caught up).
- Crash / restart semantics: simulate engine restart during a
  pending rebuild, assert shadow state is discarded cleanly and
  subsequent register call starts from scratch.
- Progress observability: assert `get_property_fts_rebuild_progress`
  returns monotonically-increasing `rows_done` during a rebuild
  and `None` after swap completes.
- Cross-binding smoke tests (Python, TypeScript) for the
  register-then-progress-poll pattern.

## Binding surface

Grouped expand must land in all three bindings in 0.4.1 (not in
phases). Rust core → Python `fathomdb` package → TypeScript
`@fathomdb/fathomdb` package. Memex's adoption plan depends on the
Python surface; the TypeScript surface ships for parity.

## What 0.4.1 does not touch

- Per-kind dynamic FTS5 tables / per-column BM25 weighting
  (item 10b / 0.5.0). Async rebuild (item 9) is decoupled and
  ships in 0.4.1; 10b's storage migration will ride on top of
  item 9's machinery when 0.5.0 lands.
- `matched_paths` attribution on `SearchHit` (item 10a / 0.5.0).
- Snippet format stability statement (item 10c / 0.5.0).
- Write-priority / foreground-read isolation (item 11 / post-0.5.0).
- Persistent rebuild progress across engine restarts. First cut
  of item 9 discards shadow state on restart.

## Memex adoption on ship

For visibility — this is the work Memex will do against 0.4.1
once it lands, captured from their round-3 response:

- Rewrite retrieval dispatchers at `retrieval.py:249`,
  `retrieval.py:326`, `retrieval.py:427` to use
  `.expand(...).execute_grouped()`.
- Rewrite loader fan-out in `world_model_reads.py` — retire
  per-hit calls at `:344`, `:401`, `:414`, `:489`, `:514-515`.
- Add denorm edges at Memex write sites:
  - `part_of_plan_goal` on plan-step insert (`fathom_store.py` /
    `world_model_plans.py`).
  - `executes_for_goal` on execution-record insert (partially live
    post-commit `29e52bd`).
  - `commitment_plan_context` on commitment insert.
- Expected latency impact: `canonical_outcome_hits` from ~44s
  toward single-digit seconds.

## Release gating

Ship criteria for 0.4.1:

**Item 8 (grouped expand):**

1. All four Memex stress shapes have integration test coverage
   and pass.
2. Target-side filter works with `filter_json_eq` and at least one
   0.4.0 named fused filter.
3. Python and TypeScript bindings expose the full surface.
4. `docs/reference/query.md` and `docs/guides/querying.md` updated
   with all three locked-semantics callouts (per-originator limit,
   undefined per-slot order, self-expand cycle sharp edge).
5. No regression in existing `grouped_query_reads.rs` tests.

**Item 9 (async rebuild):**

6. All item-9 test cases in the "Test coverage required before
   ship" section pass (async correctness, scan fallback, double-
   write, crash/restart, progress observability, bindings).
7. `docs/guides/property-fts.md` rebuild section updated to
   describe async semantics and read-from-old-schema guarantee.
8. `RebuildMode` enum and `get_property_fts_rebuild_progress`
   exposed on Python and TypeScript bindings.
9. Manual stress validation: rebuild a kind with at least 100k
   rows and measure (a) register call latency (should be <100ms),
   (b) total rebuild wall clock, (c) final swap lock-hold
   duration. Record numbers in the 0.4.1 release notes.
10. No regression in existing admin.rs / writer.rs tests; existing
    eager-mode callers either pass `RebuildMode::Eager` explicitly
    or rely on a preserved eager default (per design-doc
    decision).

**Shared:**

11. Changelog entry written, including the **"Behavior change"**
    section for the async rebuild semantics shift.
