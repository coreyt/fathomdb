# Memex input needed: CTE implementation strategy for 0.5.3

**Date:** 2026-04-20
**Context:** `dev/notes/design-0.5.3-edge-projecting-traversal.md` §5 / §6
**Decisions pending:** L1 (CTE strategy), S2 (doc prose trim, derivative)

0.5.3 exists to unblock Memex wm2. Before we freeze implementation
shape, one internal choice has a small client-visible tail. Asking
upfront so we don't surprise you.

---

## The choice

Edge-projecting traversal needs edge columns in the recursive CTE that
today only carries node data (`coordinator.rs:2281`). Two ways to get
there:

- **Option A — two SQL builders.** Existing node-expand CTE stays
  byte-for-byte identical to 0.5.2. New, separate CTE builder for edge-
  expand. Zero cross-contamination.
- **Option B — one shared CTE, branched outer SELECT.** Single
  builder emits edge columns in the recursive body for both kinds;
  node path ignores them. Less code. Node-expand SQL text changes
  (new columns, even when unused).

Both options ship the same `.traverse_edges(...)` API. Same
`GroupedQueryRows` shape. Same wire. Same Memex-visible behavior at
the SDK level.

## The tail that could affect you

FathomDB caches compiled query plans keyed by SQL-text `shape_hash`.

- **Option A:** existing `.expand(...)` call sites hit the same
  `shape_hash` post-upgrade. Plan cache survives. First query after
  upgrade is as warm as the last query before.
- **Option B:** existing `.expand(...)` call sites get a new
  `shape_hash` (new column in SQL text). Plan cache misses once per
  distinct expansion shape. Cold-cache rebuild on first execution of
  each shape; steady state unchanged within seconds.

For a wm2 process that restarts often, the delta is invisible (process
start already warms from zero). For a long-lived wm2 process with a
tight p99 budget on the very first query after a rolling upgrade,
Option B adds one cache-miss's worth of SQL compile per shape on that
first query.

## What we need from Memex

1. Does wm2 hold a long-lived FathomDB process across 0.5.2 → 0.5.3
   upgrade, or restart on upgrade?
   - Restart on upgrade: choose either; A and B are equivalent for
     you.
   - Long-lived: do you have a p99 budget on first-query-after-upgrade
     that one extra SQL-compile round could violate? (Typical SQLite
     plan compile on a grouped query is sub-millisecond; flagging
     only because "unblock Memex" is the whole point of 0.5.3.)

2. Any Memex tooling that snapshots or asserts on FathomDB's generated
   SQL text? (We don't think so, but Option B changes node-expand SQL
   text slightly.)

3. Preference or indifference?

## Default if no response / indifference

We pick **Option A** (two builders). Conservative on upgrade
semantics, zero plan-cache churn, matches the "ExpansionSlot remains
unchanged, zero risk to 0.5.1 callers" promise already in the design
doc §3. Cost is ~150 LOC of near-duplicate SQL scaffolding — a price
we'll pay for upgrade cleanliness.

S2 (a doc prose trim) is mechanical follow-through on whichever L1
answer lands; no separate input needed.

Reply on this note or the 2026-04-20 thread. Need answer before
Pack C implementation (CTE edit).

---

## Resolution (2026-04-20)

Memex response:

1. Restart on upgrade. FathomDB embedded in Memex process
   (`src/memex/fathom_store.py`). Version bump = pip install + process
   restart. Cold cache on startup already baseline.
2. No SQL text snapshots. Grep for
   `shape_hash|snapshot.*sql|assert.*SELECT|plan_cache` zero hits.
   Tests assert SDK-shape behavior, not generated SQL.
3. No p99 budget on first-query-after-upgrade. Sub-ms plan compile
   invisible against current 44s / 12s search latencies.

Preference: indifference. A and B equivalent for Memex.

**Decision: Option A (two SQL builders).** Node-expand SQL unchanged,
zero plan-cache churn. Design doc §5 rewritten to show edge-expand
SQL only; node-expand CTE reference anchored to unchanged
`coordinator.rs:2281`. S2 applied.
