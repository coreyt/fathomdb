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

### Granularity

- **One projection job per chunk-batch**, default batch size B = 64.
- Fan-out happens at scheduler entry, **not** at commit. A 10k-chunk write transaction commits as one writer-thread operation; the scheduler then enqueues ⌈10000 / B⌉ jobs.
- Per-call timeout (per ADR-0.6.0-embedder-protocol § Invariant 5) applies to one job, i.e. one batch — not per chunk.

### Backpressure (layered defence; engine does NOT impose internal projection-queue bound)

The 0.6.0 backpressure story is a four-layer composition; no single-knob queue limit:

1. **WAL mode** (already mandatory per ADR-0.6.0-single-writer-thread + ADR-0.6.0-durability-fsync-policy) decouples readers from writers.
2. **`busy_timeout` ≥ 5s** (already mandatory) absorbs short-term writer contention without erroring.
3. **Single writer thread** (already locked) strictly limits concurrent writers to 1.
4. **Adapter-level queue-depth monitoring + 429-equivalent shed.** Engine exposes `projection_queue_depth` and `embedder_saturation` metrics (per ADR-0.6.0-scheduler-shape § Observability). Bindings (Python, TS, CLI) sample these metrics and shed write submissions before memory saturates by returning a typed `EngineError::Overloaded { queue_depth, threshold }`. Threshold default + sampling cadence live in `design/scheduler.md`.

This means the engine itself does not block writer-thread submission, does not reject-commit on backlog, and does not shed projection rows. Backlog is observable; binding-level shedding is the policy lever.

### Failure handling

Projection jobs that fail (embedder timeout per Invariant 5, transient I/O):

- **Bounded retry** with exponential backoff using fixed 0.6.0 constants:
  3 retries, then 1s / 4s / 16s backoff.
- After exhausted retries: **mark-failed-and-advance.** Failed batch is recorded in `operational_*` op-store row (per ADR-0.6.0-op-store-same-file `operational_mutations` with op_kind=append) under a `projection_failures` collection; cursor advances past the failed batch.
- `projection_cursor` advances on **terminal state** (success or failed), never on in-flight. SLI #8 (p99 ≤ 5s) is measured against terminal states; in-flight rows are not part of the freshness contract.
- Operators inspect durable `projection_failures` rows as part of the accepted
  projection-failure workflow. The regenerate workflow in 0.6.0 is
  `fathomdb recover --accept-data-loss --rebuild-projections` per
  ADR-0.6.0-cli-scope; there is no separate top-level `regenerate` verb.

### Restart durability

- **No durable projection queue table.** Queue is **derived state.**
- On `Engine.open`, scheduler scans for chunks where `chunk_id > projection_cursor` and re-enqueues them. No new schema; no migration step.
- Failed batches recorded in op-store survive restart (op-store is durable per
  ADR-0.6.0-op-store-same-file); the explicit regenerate workflow
  (`recover --rebuild-projections`) re-enqueues them on demand.

### Read-path contract under cursor lag

Reads MAY return results that lag the latest write by up to the freshness window (#8 p99 ≤ 5s). Reads NEVER block on cursor lag and NEVER error on cursor lag. Clients that need strict read-after-write semantics compare write-cursor (returned at commit) with query-cursor (returned at read) and poll. Cross-cite ADR-0.6.0-retrieval-pipeline-shape (#17 stage 1 read-path).

## Options considered

**A — Push / eager (chosen).** Aligns with async-surface Invariant A; freshness SLI #8 makes window explicit. Best read latency.

**B — Pull / lazy.** Projection computed on first read; cached. Worst-case read latency (first read for a chunk pays embedder cost); no scheduler complexity; freshness becomes per-read-trigger. First-read tail latency is brutal under interactive workloads. Note: decision assumes interactive read mix; analytical-only deployments (ingest-then-RAG-later, batch readers) where most chunks are never queried may revisit via amendment — push pays embedder cost on 100% of writes vs pull on N% of reads.

**C — Access-pattern hybrid (auto-detect "frequent," lazy for tail).** Requires access-pattern tracking + a "what counts as frequent" threshold (speculative knob). Stop-doing speculative-knobs class. **Rejected for 0.6.0.**

**C′ — Per-profile lazy mode (operator-declared).** Operator marks a vector profile as `lazy=true` at registration time; profile-level pull semantics; no per-row decision. Not speculative-knob — explicit operator policy. **Deferred to a future ADR**, not foreclosed by this one. Forcing function: a real consumer with cold long-tail profiles.

## Consequences

- `design/projections.md` documents the push model end-to-end (write → enqueue projection job → embedder dispatch → projection-row commit + cursor advance).
- `design/vector.md` cross-cites the model when explaining the read path.
- Pull and access-pattern hybrid are out of scope for 0.6.0 — re-opens require an ADR amendment.
- Per-profile lazy mode (Option C′) is **deferred** (not rejected); future ADR if a real consumer needs it.
- New typed error: `EngineError::Overloaded { queue_depth, threshold }` (added to error taxonomy per ADR-0.6.0-error-taxonomy).
- Cross-cite #8 freshness SLI: the push model is what makes p99 ≤ 5s testable.
- Cross-cite #17 retrieval pipeline: read path can rely on projection rows existing within the cursor window.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-async-surface § Invariant A.
- ADR-0.6.0-projection-freshness-sli (#8).
- ADR-0.6.0-scheduler-shape (#14).
- Stop-doing: speculative knobs.
