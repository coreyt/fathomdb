---
title: ADR-0.6.0-projection-freshness-sli
date: 2026-04-27
target_release: 0.6.0
desc: Vector-projection freshness numerical SLI + projection_cursor surface
blast_radius: vector-projection scheduler; engine read-tx surface; interfaces/*.md (projection_cursor field); test-plan.md freshness tests; ADR-0.6.0-async-surface Invariant A; ADR-0.6.0-scheduler-shape (#14)
status: accepted
---

# ADR-0.6.0 — Projection freshness SLI

**Status:** accepted (HITL 2026-04-27).

Phase 2 #8 acceptance ADR. Sets numerical staleness target + the cursor surface clients use to reason about lag.

## Context

ADR-0.6.0-async-surface Invariant A: vector-projection scheduler dispatches post-commit. Clients may issue retrieval queries before the projection catches up — "vec-not-yet-consistent" surface. Acceptance must define how stale and how surfaced.

## Decision

- **Freshness target: p99 ≤ 5 seconds post-commit.** Measured: time from primary write commit to projection table containing the corresponding vector row.
- **`projection_cursor` exposed on read transactions.** Monotonic non-decreasing integer (or sortable opaque token). Client compares write-cursor (returned from a write commit) with query-cursor (read at search time) to detect "my write is not yet reflected."
- Cursor surface is **honest about asynchrony**: clients can choose to poll until query-cursor ≥ write-cursor, or accept stale results.

## Options considered

**A — p99 ≤ 5s + cursor (chosen).** Realistic for embedder-bound workloads (default candle embedder + sqlite-vec write). Cursor surface gives clients explicit control over freshness/latency tradeoff. Achievable on tier-1 baseline.

**B — p99 ≤ 1s + cursor.** Aggressive; pushes scheduler concurrency + embedder pool size up; may force batching limits. Useful only with explicit interactive freshness gate; no current forcing function.

**C — No SLI; cursor only.** Cheapest; abdicates the AC obligation. Hard to test against; weakens `acceptance.md`. Rejected.

## Consequences

- `design/scheduler.md` documents the post-commit dispatch path + cursor advancement semantics.
- `design/engine.md` documents how `projection_cursor` is allocated (likely a monotonic counter on the writer thread, written in the same tx as the projection-job enqueue).
- `interfaces/*.md` documents the cursor on read tx + on write commit returns.
- `test-plan.md`: freshness AC = batch of N writes, measure time from commit-N to query-cursor ≥ commit-N's cursor; p99 ≤ 5s.
- Tightening to B revisits this ADR; requires user-facing forcing function.
- Cross-cite #14 scheduler shape: pool sizing affects whether 5s p99 is met under load.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-async-surface § Invariant A.
- ADR-0.6.0-single-writer-thread (writer thread allocates cursor).
