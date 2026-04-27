---
title: ADR-0.6.0-scheduler-shape
date: 2026-04-27
target_release: 0.6.0
desc: Vector-projection scheduler implementation — single tokio runtime + per-job async tasks + projection_cursor surface
blast_radius: design/scheduler.md; engine open path; tokio dep; ADR-0.6.0-async-surface Invariant A; ADR-0.6.0-projection-freshness-sli (#8); embedder pool sizing
status: accepted
---

# ADR-0.6.0 — Scheduler shape

**Status:** accepted (HITL 2026-04-27).

Phase 2 #14 architecture ADR. Pins the dispatcher implementation behind ADR-0.6.0-async-surface Invariant A and the cursor surface from ADR-0.6.0-projection-freshness-sli.

## Context

ADR-0.6.0-async-surface Invariant A: scheduler dispatches post-commit; writer lock released before any scheduler dispatch begins. Open: dispatcher implementation (tokio actor / thread-pool / single sync thread) and what clients see when projection lags behind commits. Cross-cuts #8 freshness SLI.

## Decision

- **Single tokio runtime owned by the engine.** Created at `Engine.open`; dropped at `Engine.close`.
- **One async task per pending projection job.** Tasks consume embedder pool slots (per ADR-0.6.0-embedder-protocol § Invariant 4); embed result is submitted back to the writer thread for commit (per ADR-0.6.0-single-writer-thread).
- **`projection_cursor` advanced atomically with the projection-row commit.** Cursor exposed on read transactions (per ADR-0.6.0-projection-freshness-sli).
- **Pool size configurable** at `Engine.open` (default `num_cpus::get()`); same pool that drives embedder dispatch.

## Options considered

**A — Single tokio runtime + per-job async tasks (chosen).** Tokio is engineering-standard in Rust async; embedder calls are I/O-bound (HF disk + compute) and benefit from concurrency; cursor surface composes with #8 SLI. Pool size controllable.

**B — Thread-pool of N workers (no tokio); mpsc job queue.** Avoids tokio dep on the scheduler path. Loses async ergonomics for I/O-bound embedder calls; needs hand-rolled cancellation + timeout machinery that tokio gives free.

**C — Single sync scheduler thread; jobs serialised.** Simplest possible. Cannot parallelise embedder calls across pending jobs; widens freshness window beyond #8 target. Rejected.

## Consequences

- `design/scheduler.md` documents the runtime lifecycle, task spawn model, and the cursor advancement protocol.
- Tokio is a confirmed engine dep (was already in deps audit).
- Embedder dispatch and projection scheduling share the same pool by default; per-`Engine.open` overrides can split them.
- Per-task cancellation on `Engine.close`: scheduler runtime is dropped; in-flight tasks are abandoned (their writes never commit because the writer thread has shut down).
- Cross-cite #8: pool size + task scheduling are what make p99 ≤ 5s achievable.
- Cross-cite ADR-0.6.0-async-surface § Invariant A (post-commit dispatch is structural, not implementation-only).

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-async-surface § Invariant A.
- ADR-0.6.0-projection-freshness-sli (#8 — cursor surface + p99 target).
- ADR-0.6.0-embedder-protocol § Invariant 4 (engine-owned pool).
- ADR-0.6.0-single-writer-thread (writer-thread submission for commit).
