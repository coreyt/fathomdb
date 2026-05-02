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

### Two pools (corrected from prior draft)

The engine owns **two distinct pools** — they are not the same pool:

- **Tokio runtime worker pool — orchestration only.** Drives async tasks for projection jobs (await embedder result, submit commit to writer). Default size: 2 worker threads (`scheduler_runtime_threads`). Configurable per `Engine.open`. Tokio workers NEVER run `embed()` directly — that would violate ADR-0.6.0-embedder-protocol § Invariant 4.
- **Embedder dispatch pool — `embed()` calls only.** Per ADR-0.6.0-embedder-protocol § Invariant 4. Default size: `num_cpus::get()` (`embedder_pool_size`). Configurable per `Engine.open`.
  The override exists because embedded deployments range from laptops
  sharing cores with the host app to dedicated ingest workers; operators
  need one lever to trade projection throughput against CPU / memory
  contention from local embedding.
- Scheduler tasks call `tokio::task::spawn_blocking` (or equivalent handoff) to move from the runtime pool onto the embedder pool when invoking `embed()`. Result is awaited back on the runtime pool.

### Thread isolation

- **Tokio runtime runs on its own dedicated worker threads.** Writer thread is a dedicated OS thread (per ADR-0.6.0-single-writer-thread); the writer thread is **never** a tokio worker.
- Writer thread holds only a `tokio::runtime::Handle` for `spawn`. Writer thread NEVER calls `block_on`. (Writer code path: write commit → spawn projection job onto tokio handle → return; handle.spawn does not block.)
- Submission from a scheduler task back to the writer (for projection-row commit) goes through an mpsc channel, not via tokio entering the writer thread.

### Per-job lifecycle

- **One async task per pending projection job.** (Job = one chunk-batch per ADR-0.6.0-projection-model § Granularity, default B=64.)
- Tasks consume embedder pool slots via `spawn_blocking`; embed result is submitted to the writer thread via the mpsc channel.
- **`projection_cursor` advanced atomically with the projection-row commit** on the writer thread (cursor is a column on the projection row's tx).

### Concurrency bound (backpressure-internal)

- **Bounded in-flight scheduler tasks** via `tokio::sync::Semaphore` sized `4 * embedder_pool_size`.
- Excess pending jobs queue on the cursor surface (i.e. as un-spawned chunk_id-greater-than-cursor rows), NOT as live futures. Memory does not grow with backlog.
- `projection_queue_depth` counts un-spawned pending jobs; `embedder_saturation`
  counts active embedder pool slots / pool size.

### Per-job cancellation

- **Generation-token check before embedder dispatch.** Each scheduler task carries the chunk's generation token (last-modified-version). Before `spawn_blocking(embed)`, task checks current token; if superseded by a later write, task discards (best-effort).
- **In-flight `embed()` is NOT aborted** (best-effort per Invariant 5; no portable thread-cancel API). Result is discarded if generation has advanced by submission time.

### Engine.close shutdown protocol (deadlock-free)

Ordered shutdown:

1. Stop accepting new write commits at the engine boundary (`Engine.close` returns `EngineError::Closing` for new writes).
2. Cease scheduler dispatch (no new tokio tasks spawned).
3. `runtime.shutdown_timeout(grace)` where `grace ≥ embedder_per_call_timeout` (default 30s per Invariant 5). In-flight embed calls finish or timeout; results are discarded if writer is already drained.
4. Drain submission mpsc channel (writer thread processes any pending projection-row commits).
5. Stop writer thread last; release database lock.

### Observability

- Metrics surface exposes: `projection_queue_depth`, `embedder_saturation`.
  Used by binding-level adapters for 429-shed (per
  ADR-0.6.0-projection-model § Backpressure layer 4).
- **Pool resize at runtime: out of scope** for 0.6.0. Sizes set at `Engine.open` only.

## Options considered

**A — Single tokio runtime + per-job async tasks (chosen).** Tokio is engineering-standard in Rust async; embedder calls are I/O-bound (HF disk + compute) and benefit from concurrency; cursor surface composes with #8 SLI. Pool size controllable.

**B — Thread-pool of N workers (no tokio); mpsc job queue.** Avoids tokio dep on the scheduler path. Loses async ergonomics for I/O-bound embedder calls; needs hand-rolled cancellation + timeout machinery that tokio gives free.

**C — Single sync scheduler thread; jobs serialised.** Simplest possible. Cannot parallelise embedder calls across pending jobs; widens freshness window beyond #8 target. Rejected.

## Consequences

- `design/scheduler.md` documents the runtime lifecycle, task spawn model, and the cursor advancement protocol.
- Tokio is a confirmed engine dep (was already in deps audit).
- Embedder dispatch and projection scheduling use distinct pools by default;
  per-`Engine.open` overrides size each pool independently.
- Per-task cancellation on `Engine.close`: scheduler runtime is dropped; in-flight tasks are abandoned (their writes never commit because the writer thread has shut down).
- Cross-cite #8: pool size + task scheduling are what make p99 ≤ 5s achievable.
- Cross-cite ADR-0.6.0-async-surface § Invariant A (post-commit dispatch is structural, not implementation-only).

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-async-surface § Invariant A.
- ADR-0.6.0-projection-freshness-sli (#8 — cursor surface + p99 target).
- ADR-0.6.0-embedder-protocol § Invariant 4 (engine-owned pool).
- ADR-0.6.0-single-writer-thread (writer-thread submission for commit).
