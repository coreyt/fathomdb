---
title: ADR-0.6.0-retrieval-latency-gates
date: 2026-04-27
target_release: 0.6.0
desc: Vector retrieval p50/p99 end-to-end latency gates at 1M-vector scale
blast_radius: test-plan.md perf-test suite; CI perf gate; design/vector.md tuning section; sqlite-vec usage; ADR-0.6.0-default-embedder
status: accepted
---

# ADR-0.6.0 — Retrieval latency gates

**Status:** accepted (HITL 2026-04-27).

Phase 2 #9 acceptance ADR. Sets end-to-end vector-search latency that 0.6.0 must satisfy as a release gate.

## Context

`acceptance.md` requires numerical, testable, falsifiable gates. Retrieval latency drives interactive UX expectations and frames what `design/vector.md` must deliver. Numbers chosen absolute (per plan.md non-goal: no perf delta from 0.5.x).

## Decision

- **p50 ≤ 50 ms; p99 ≤ 200 ms.**
- **Workload definition:** 1,000,000 vectors @ 768-dim (default embedder), `k=10`, single-process, **no concurrent writes**, warm cache, default sqlite-vec parameters.
- **Latency boundary:** end-to-end client call → result list. Includes query embedding + ANN candidate fetch + (default) no rerank + no graph expansion.

## Options considered

**A — p50 ≤ 50 ms; p99 ≤ 200 ms (chosen).** Matches embedded-vector-DB norms; vec0 + sqlite mmap pragmas hit it on tier-1 baseline. Doesn't overpromise on tail; testable.

**B — p50 ≤ 20 ms; p99 ≤ 100 ms.** Aggressive; requires careful sqlite-vec tuning + heavy mmap; risks failing CI on slower runners.

**C — p50 ≤ 100 ms; p99 ≤ 500 ms.** Loose; easy to hit; may not satisfy interactive UX.

## Consequences

- `test-plan.md`: perf AC with seeded fixture (1M vectors), warm-cache run, k=10, p50/p99 reported per CI run; fails build on regression.
- `design/vector.md`: documents required SQLite pragmas + sqlite-vec parameters that achieve A.
- CI perf job runs on a pinned tier-1 runner shape (specifics in `test-plan.md`).
- Concurrent-write impact on retrieval latency is a separate followup (write-throughput SLI #24 + checkpoint stall analysis); not gated here.
- Reranker / graph-expand stages add their own latency; ADR sets the **default-pipeline** gate. Stage-augmented latency is documented but not gated in 0.6.0.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-default-embedder (768-dim).
- ADR-0.6.0-sqlite-vec-acceptance.
- Plan.md non-goal: no perf delta from 0.5.x.
