---
title: ADR-0.6.0-write-throughput-sli
date: 2026-04-27
target_release: 0.6.0
desc: Single-writer write-throughput SLI; forcing function for any future MVCC re-open
blast_radius: test-plan.md write-throughput tests; CI perf gate; ADR-0.6.0-single-writer-thread deferral language; design/engine.md
status: accepted
---

# ADR-0.6.0 — Write-throughput SLI

**Status:** accepted (HITL 2026-04-27).

Phase 2 #24 acceptance ADR. Provides the numerical anchor that future MVCC re-open discussions need (per ADR-0.6.0-single-writer-thread Deferral target).

## Context

Single-writer-thread is locked for 0.6.0. The deferral-to-0.7+ language references a "future write-throughput ADR" — this is it. Number set comfortably achievable on tier-1 baseline so MVCC isn't pulled forward by a too-aggressive gate.

## Decision

- **≥ 1,000 commits/sec @ 1 KB payload.**
- **≥ 100 commits/sec @ 100 KB payload.**
- **Hardware baseline:** tier-1 (8-core x86_64, NVMe SSD, 16 GB RAM).
- **Workload definition:** sequential `WriteTx` commits from a single client; payload size as stated; `synchronous=NORMAL` (per ADR-0.6.0-durability-fsync-policy); no projection scheduler load.
- **MVCC forcing function:** if a real-world workload sustains ≥ 80% of either gate AND the SLI is breached, ADR-0.6.0-single-writer-thread is re-opened.

## Options considered

**A — 1k / 100 commits/sec (chosen).** Conservative; matches realistic single-writer + WAL ceiling on tier-1 hardware; leaves headroom before MVCC pressure builds. Testable; gate-able in CI.

**B — 5k / 500 commits/sec.** Aggressive; would require write-batching API to hit; pulls MVCC pressure forward; opens speculative-knob class on batching parameters.

**C — No numerical SLI.** Document "single-writer is the limit; benchmark per release." Abdicates AC obligation; leaves MVCC re-open without anchor.

## Consequences

- `test-plan.md`: write-throughput AC with seeded fixture; CI perf gate on tier-1 runner; fails build on regression.
- `design/engine.md`: documents commit-path optimisations needed to hit A (prepared-statement reuse, transaction-batching internals).
- ADR-0.6.0-single-writer-thread Deferral target now has a concrete forcing function.
- Concurrent writes from N clients are **not** part of this SLI's workload — per single-writer ADR they serialise; concurrent-throughput is the same as sequential under WAL. Test-plan documents this.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-single-writer-thread § Deferral target.
- ADR-0.6.0-durability-fsync-policy (synchronous=NORMAL baseline).
