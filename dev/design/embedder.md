---
title: Embedder Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: Dispatch pool, warmup behavior, timeout handling, and identity checks
blast_radius: embedder dispatch; Engine.open warmup; REQ-028*, REQ-033, REQ-044
status: locked
---

# Embedder Design

This file owns dispatch onto the engine-owned embedder pool, eager warmup,
per-call timeout handling, and the runtime mechanics behind
`EmbedderIdentityMismatch`.

## `embedder_pool_size` rationale

`embedder_pool_size` remains an engine-level knob in 0.6.0 because embedded
deployments are not uniform. Some hosts run FathomDB beside latency-sensitive
application work and need to cap embedder parallelism; others run heavier local
models or dedicated ingest jobs and need to tune concurrency around actual CPU
and memory pressure. The knob exists to control embedder contention, not to
create binding-specific surface.
