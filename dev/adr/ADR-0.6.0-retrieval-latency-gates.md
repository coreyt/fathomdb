---
title: ADR-0.6.0-retrieval-latency-gates
date: 2026-04-27
target_release: 0.6.0
desc: Vector retrieval p50/p99 end-to-end latency gates at 1M-vector scale
blast_radius: test-plan.md perf-test suite; CI perf gate; design/vector.md tuning section; sqlite-vec usage; ADR-0.6.0-default-embedder
status: accepted
---

# ADR-0.6.0 — Retrieval latency gates

**Status:** accepted (HITL 2026-04-27); measurement-protocol amendment
applied 2026-04-27 (FU-PERF-ADR-ALIGN). Numerical gate unchanged.

Phase 2 #9 acceptance ADR. Sets end-to-end vector-search latency that 0.6.0 must satisfy as a release gate.

## Context

`acceptance.md` requires numerical, testable, falsifiable gates. Retrieval latency drives interactive UX expectations and frames what `design/vector.md` must deliver. Numbers chosen absolute (per plan.md non-goal: no perf delta from 0.5.x).

## Decision

- **p50 ≤ 50 ms; p99 ≤ 200 ms.**
- **Scope:** vector retrieval mode (vector-only, or hybrid `search` —
  this is the embedder-bearing path). Text-only FTS5 path is gated
  separately by ADR-0.6.0-text-query-latency-gates.
- **Workload definition:**
  - **Dataset:** 1,000,000 vectors @ 768-dim, default embedder. Vector
    distribution and chunk-row table specified by `test-plan.md`
    fixture (the same chunk-row table that backs
    ADR-0.6.0-text-query-latency-gates; only the secondary index under
    test differs — `vec0` here).
  - **Query mix:** `k=10`. Query vectors drawn from a held-out slice
    of the same distribution as the indexed corpus (avoids both
    nearest-neighbor-trivial and out-of-distribution degenerate paths).
  - **Concurrency:** **QPS = 1** (sequential, one in-flight query at
    a time), single-process, **no concurrent writes**.
  - **Cache state:** warm. Warmup protocol = run the full query suite
    once and discard; measure on the second pass.
  - **Sample count:** ≥ 1,000 measured queries per percentile
    calculation.
  - **sqlite-vec parameters:** defaults.
- **Latency boundary:** **in-process** client call → result list.
  Includes query embedding (the embedder call dispatched on the
  engine-owned thread per ADR-0.6.0-async-surface Invariant B) + ANN
  candidate fetch + canonical row fetch + result serialization to
  in-process result type. **Excludes** IPC / network / subprocess-bridge
  envelope and graph-expand stages.

## Options considered

**A — p50 ≤ 50 ms; p99 ≤ 200 ms (chosen).** Matches embedded-vector-DB norms; vec0 + sqlite mmap pragmas hit it on tier-1 baseline. Doesn't overpromise on tail; testable.

**B — p50 ≤ 20 ms; p99 ≤ 100 ms.** Aggressive; requires careful sqlite-vec tuning + heavy mmap; risks failing CI on slower runners.

**C — p50 ≤ 100 ms; p99 ≤ 500 ms.** Loose; easy to hit; may not satisfy interactive UX.

## Consequences

- `test-plan.md`: perf AC with seeded fixture per the workload
  definition above; p50/p99 reported per CI run; fails build on
  regression. Fixture row-set shared with
  ADR-0.6.0-text-query-latency-gates fixture (same 1M chunk-row table);
  only the secondary index under test differs (`vec0` here, FTS5 in
  the text ADR).
- `design/vector.md`: documents required SQLite pragmas + sqlite-vec
  parameters that achieve A.
- CI perf job runs on a pinned tier-1 runner shape (`x86_64-unknown-linux-gnu`
  per ADR-0.6.0-tier1-ci-platforms reference target; specifics in
  `test-plan.md`).
- Concurrent-write impact on retrieval latency is a separate followup
  (write-throughput SLI #24 + checkpoint stall analysis); not gated
  here.
- Graph-expand adds its own latency; ADR sets the **default-pipeline**
  gate. Stage-augmented latency is documented but not gated in 0.6.0.
- FU-PERF-ADR-ALIGN closed by this amendment: workload definition now
  matches text-query-latency-gates' protocol granularity.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-default-embedder (768-dim).
- ADR-0.6.0-sqlite-vec-acceptance.
- Plan.md non-goal: no perf delta from 0.5.x.
