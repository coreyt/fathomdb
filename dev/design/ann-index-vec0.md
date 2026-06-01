---
title: ANN index on vec0 (post-1.0 perf follow-up)
date: 2026-06-01
target_release: post-1.0 (pre-2.1)
status: tracked / not started
desc: Tracked home for the approximate-nearest-neighbor index that takes vector search from O(N) to O(log N)/O(√N) and unblocks the AC-013/AC-019 100k and 1M latency tiers. Named across several 0.7.x ADRs/notes; this file is the single tracked record.
---

# ANN index on the `vec0` virtual table

**Status: tracked, not started. Target window: post-1.0, pre-2.1.** This is the
named follow-up that several 0.7.x documents point at but that had no single
tracked home until now (created 0.7.2 PR-3).

## Problem

The production vector read path (`read_search_in_tx`, two-phase bit-KNN + f32
rerank) has **no ANN index** on the `vec0` virtual table: the bit-KNN candidate
stage is a per-query **O(N) linear scan** over all rows. Latency therefore grows
~linearly with corpus size. Measured 0.7.2 PR-3 numbers
(`dev/plans/runs/0.7.2-PR-3-perf-data.md`), production-faithful 384-d:

| N | p50 | budget (80/300) | binding? |
| --- | --- | --- | --- |
| 10,000 | 36 ms (real bge) / 15 ms (synthetic) | 80 / 300 | **YES (0.x/1.x gate) — MET** |
| 100,000 | ~147 ms | 80 / 300 | tracked (not met) |
| 1,000,000 | ~1.5 s (O(N) extrapolation; 0.7.0 W4.1 f32-brute anchor 2,048 ms) | 80 / 300 | tracked (not met) |

So the 80 ms p50 budget is met only up to ~50k rows. The tiered AC-013/AC-019
budget (`ADR-0.7.0-text-query-latency-gates-revised`, AC-072/AC-073 in
`dev/acceptance.md`) makes the **10k tier the binding release gate for the 0.x
and 1.x lines** and defers the 100k/1M tiers to this work.

## What this unblocks

- **AC-072** (revised AC-013) 100k and 1M tiers → binding once this lands.
- **AC-073** (revised AC-019) 100k/1M tiers (inherit AC-072's growth).
- Makes the canonical N=1M latency/recall measurement tractable enough to
  potentially re-gate on CI (today it is local-only / dispatch-only — see
  `dev/notes/ac013-ac019-canonical-scale-policy.md`).

## Candidate approaches (to evaluate when the slice opens)

- **HNSW** — graph index; strong recall/latency, higher memory + build cost.
- **IVF** (inverted-file / coarse-quantizer) — cheaper build, tunable nprobe.
- **DiskANN** — disk-resident, fits the local-first / large-corpus posture.

Open design questions: integration with the existing sign-bit-quant + f32-rerank
two-phase pipeline (index the bit codes vs the f32 vectors?); incremental
maintenance under the single-writer projection path; whether sqlite-vec gains a
native ANN surface vs an engine-side index. Decide via an ADR when the slice
opens.

## Why post-1.0 / pre-2.1 (not sooner)

The 0.x/1.x consumer profile (local-first agent memory — Memex/Hermes/OpenClaw)
runs corpora well under the ~50k crossover where the linear scan still meets
budget, so the 10k gate is the right bar for those lines. Larger-N performance
is a maturity goal for the 2.x line. No dated milestone yet; this file is the
tracking anchor.

## References

- `dev/adr/ADR-0.7.0-text-query-latency-gates-revised.md` — tiered budget; "post-1.0 obligation".
- `dev/acceptance.md` — AC-072 / AC-073 (tracked 100k/1M tiers).
- `dev/notes/ac013-ac019-canonical-scale-policy.md` — "Long-term follow-up" (originating mention).
- `dev/plans/runs/0.7.2-PR-3-perf-data.md` — the O(N) scaling measurements.
- `dev/notes/pcache2-followups.md` — sibling post-0.7.0 perf follow-ups.
