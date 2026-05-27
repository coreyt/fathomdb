# PCACHE2 — post-0.7.0 follow-up candidates

Written: 2026-05-26
Status: **DEFERRED to post-0.7.0 production-hardening pass.**

The 0.7.0 perf-experiments campaign locked
**W4.1-stacked-O1** as the production stack for PCACHE2 (see
`dev/plans/0.7.0-HITL-recommendations.md`). The prototype landed
GREEN on both AC-012 (canonical CI 57/135 vs 60/150 budget) and
AC-020 (canonical CI 56 ms concurrent vs 80 ms bound) with
material headroom.

This note captures three variations that were **considered but
not dispatched** before lock-flip, with a documented rationale.

## What's deferred

1. **Replace PCACHE2's `HashSet<key>` eviction-candidate set with a
   proper LRU.** Current implementation (`pcache2.rs:69`) tracks
   unpinned pages in a `HashSet<c_uint>` for O(1) membership and
   "pick any unpinned page" eviction. This is best-effort, not
   recency-ordered.

2. **Change writer pragma `page_size=8192` → `page_size=16384`.**

3. **Both together.**

## Why not dispatched for 0.7.0

### Variation 1 — proper LRU

- **AC-020 impact: 0 to −3%** (neutral). The eviction branch in
  `pcache_fetch` only fires when `cache_size_hint > 0 AND
  pages.len() >= limit` (pcache2.rs:237-247). AC-020's fixture is
  4 nodes (perf_gates.rs:102-121) — eviction never triggers. LRU
  pointer-bookkeeping adds 2 extra writes per `xFetch`/`xUnpin`
  under the held mutex on the actually-hot path, so the prototype
  is *faster* than an LRU on this workload.
- **AC-012 impact: p50 neutral; p99 possibly −5 to −10 ms** at
  best, neutral otherwise. Random "pick any unpinned" eviction
  can occasionally drop a hot B-tree inner node and incur a
  cold-read miss on the next access; LRU protects against that.
- **Confidence: medium-low.** No measured prior; the eviction-
  quality benefit is tail-only and hard to size without G.3.5-
  style miss-rate telemetry on a production workload.
- **Workload sensitivity is where this flips.** Under tight cache
  pressure (working set > `cache_size = -262144` 256 MB +
  `mmap_size` 256 MB), random eviction can degrade miss rate
  2–5× vs LRU. Production callers with >512 MB hot sets would
  see materially worse p99. **This is the strongest production-
  hardening argument** in the three variations — its AC-020/AC-012
  cost is negligible, and its production tail-latency benefit is
  large.

### Variation 2 — `page_size=16384`

- **AC-020 impact: −3% to −8%** (7.375× dev-box → ~6.8–7.1×).
  Mechanism: 16K pages halve `xFetch`/`xUnpin` calls (less mutex
  acquisition), but PCACHE2's `alloc_zeroed` cost (pcache2.rs:115)
  doubles per page-create, memcpy from mmap doubles, and the
  cache-line working-set on AC-020's tiny 4-node fixture wastes
  L2/L3.
- **AC-012 impact: p50 possibly −1 to −3 ms** (fewer B-tree
  levels for 1M-row FTS5); **p99 likely +15 to +35 ms** (alloc-
  zeroed tail spikes, 2× memcpy on cold misses → 96 ms toward
  110–130 ms).
- **Confidence: medium-high negative.** W4.0g is a direct measured
  prior — 16K solo (×=2.818) underperformed 8K solo (×=3.025) on
  the same AC-020 workload. PCACHE2 stacking *might* change the
  sign because it removes the pcache1 mutex baseline 16K was
  tested against — but the dominant 16K downsides (alloc cost,
  cacheline waste on the tiny AC-020 fixture) are independent of
  the pcache implementation. **The prior should hold.**
- **Workload sensitivity:** partial flip on large working sets.
  16K halves pcache management overhead and improves sequential-
  scan throughput (better mmap prefetch). Production OLAP-shape
  workloads on large corpora would benefit. AC-020/AC-012 do not
  represent that shape.

### Variation 3 — LRU + 16K together

- **AC-020: net negative** (−3% to −10%), dominated by 16K's
  alloc cost / cacheline penalties. LRU's tail benefit is
  invisible on AC-020 (no evictions to order).
- **AC-012: p50 neutral-to-slight-positive (~40 ms); p99
  ambiguous** (LRU's p99 protection partially offsets 16K's p99
  alloc spikes; net could be anywhere 90–120 ms).
- **Confidence: low.** Two unmeasured changes interacting; risk
  of compounding regressions.
- **Workload sensitivity:** this combo is the **most defensible
  under tight cache pressure** (LRU quality × fewer-but-larger
  pages = better hit rate + lower management overhead). For
  perf-gates it likely loses; for production storage of large
  graphs it's the strongest candidate.

## Decision

**Defer all three to a post-0.7.0 production-hardening pass keyed
on real-workload telemetry** (G.3.5-style miss-rate / contention
counters). Reasons:

1. W4.1-stacked-O1 lands GREEN with material headroom; lock-flip
   is gated on AC-013/AC-019 canonical-CI numbers, not on these.
2. The 16K-solo prior (W4.0g) is a directly-measured AC-020
   regression. Risk of de-GREEN-ing AC-020 on canonical CI is
   real and a dispatch+re-measure round costs more than the
   expected gain (none on AC gates).
3. LRU is the only one with a defensible production-quality
   argument, but the benefit is invisible on the AC fixtures —
   you cannot measure the upside on the current gates. Land it
   when there is a workload where eviction quality measurably
   affects p99.

## When to revisit

Trigger conditions for spawning a `0.8.0-PERF-PCACHE2-PRODUCTION`
or similar slice:

- A production / sandbox workload appears with **working set >
  cache_size + mmap_size** (i.e. >512 MB hot pages) and shows
  p99 regression vs synthetic benchmarks. Either of the three
  variations could move that.
- A future AC adds a **cache-pressured retrieval gate** (1M+
  vectors with random-access query patterns; AC-013 / AC-019 may
  partially fall into this category once their canonical-CI
  numbers land — re-evaluate then).
- SQLite upstream changes the pcache2 ABI or the default pcache1
  internals; either could invalidate measurements and warrant a
  re-measure regardless.

The first variation to dispatch in that follow-up should be
**LRU alone**. Page_size changes carry a larger blast radius
(file-format choice on write, affects every B-tree access) and a
worse measured prior. Land LRU first; observe; then consider 16K
if and only if LRU + workload telemetry indicates the
larger-page benefit would compose.

## Implementation pointers (for the follow-up slice)

- `pcache2.rs:69` (`unpinned: HashSet<c_uint>`) — replace with an
  intrusive doubly-linked list. Add `lru_prev: Option<c_uint>` /
  `lru_next: Option<c_uint>` to `struct Page` (pcache2.rs:75).
  Maintain `lru_head` / `lru_tail` in `struct State`. Touch on
  `xFetch` hit + `xUnpin`. Eviction picks `lru_head`.
- `lib.rs:3881` (writer pragmas) — `page_size=16384` change site
  if 16K is ever picked.
- `dev/plans/0.7.0-perf-experiments.md` W4.0g — measured prior
  for the 16K regression on `pcache1`.
- A future canonical-CI dispatch should use the same
  `perf-canonical` workflow with new `experiment_id` and the
  full W4.1-stacked-O1 env vars plus the variation under test.

## Related

- `dev/plans/0.7.0-HITL-recommendations.md` § Q2 clarification
  table — documents what's in the locked stack and what isn't.
- `ADR-0.7.0-ac020-architectural-lever.md` — records PCACHE2 as
  the chosen lever and WAL2 / R-W split / libSQL as rejected.
- `dev/notes/performance-whitepaper-notes.md` §13 — should be
  updated by the lock-flip commit to reference both the
  W4.1-stacked-O1 configuration and this followups note.
