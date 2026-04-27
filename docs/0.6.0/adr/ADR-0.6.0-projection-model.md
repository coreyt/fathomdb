---
title: ADR-0.6.0-projection-model
date: 2026-04-27
target_release: 0.6.0
desc: Push (eager) projection model; scheduler dispatches post-commit
blast_radius: design/scheduler.md; design/vector.md; design/projections.md; ADR-0.6.0-async-surface Invariant A; ADR-0.6.0-projection-freshness-sli; ADR-0.6.0-scheduler-shape
status: accepted
---

# ADR-0.6.0 — Projection model: push (eager)

**Status:** accepted (HITL 2026-04-27).

Phase 2 #16 design ADR. Central design question of the rewrite: when does projection computation happen relative to the originating write?

## Context

Vector projections must be computed for new chunks. Trigger model is the central question of the rewrite. Cross-cuts:
- ADR-0.6.0-async-surface § Invariant A (post-commit dispatch is structural).
- ADR-0.6.0-projection-freshness-sli (#8 freshness SLI).
- ADR-0.6.0-scheduler-shape (#14).
- ADR-0.6.0-retrieval-pipeline-shape (#17).

## Decision

**Push (eager).** Scheduler dispatches projection jobs post-commit per Invariant A. Projection table is eventually consistent against primary writes; freshness window per ADR-0.6.0-projection-freshness-sli (p99 ≤ 5s).

- Best read latency (vector available at query time without compute).
- Freshness window made explicit by `projection_cursor` surface.
- Matches accepted async-surface + scheduler-shape ADRs.

## Options considered

**A — Push / eager (chosen).** Aligns with async-surface Invariant A; freshness SLI #8 makes window explicit. Best read latency.

**B — Pull / lazy.** Projection computed on first read; cached. Worst-case read latency (first read for a chunk pays embedder cost); no scheduler complexity; freshness becomes per-read-trigger. First-read tail latency is brutal under interactive workloads.

**C — Hybrid (eager for "frequent," lazy for tail).** Best in theory; requires access-pattern tracking + a "what counts as frequent" threshold (speculative knob). Stop-doing speculative-knobs class.

## Consequences

- `design/projections.md` documents the push model end-to-end (write → enqueue projection job → embedder dispatch → projection-row commit + cursor advance).
- `design/vector.md` cross-cites the model when explaining the read path.
- Pull and hybrid are out of scope for 0.6.0 — re-opens require an ADR amendment.
- Cross-cite #8 freshness SLI: the push model is what makes p99 ≤ 5s testable.
- Cross-cite #17 retrieval pipeline: read path can rely on projection rows existing within the cursor window.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-async-surface § Invariant A.
- ADR-0.6.0-projection-freshness-sli (#8).
- ADR-0.6.0-scheduler-shape (#14).
- Stop-doing: speculative knobs.
