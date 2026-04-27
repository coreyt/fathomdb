---
title: ADR-0.6.0-single-writer-thread
date: 2026-04-27
target_release: 0.6.0
desc: Single-writer-thread engine for 0.6.0; MVCC and concurrent writers explicitly out of scope
blast_radius: crates/fathomdb-engine writer architecture; rusqlite usage; op-store dispatch; vector-projection scheduler; every binding's write path; ADR-0.6.0-async-surface; ADR-0.6.0-op-store-same-file; ADR-0.6.0-embedder-protocol
status: accepted
---

# ADR-0.6.0 — Single-writer-thread

**Status:** accepted (HITL 2026-04-27, decision-recording).

Promoted from critic-3 M-3. The single-writer-thread invariant is
already cited as load-bearing in three accepted ADRs
(`async-surface`, `op-store-same-file`, `embedder-protocol`) but had
no citable artifact of its own. This ADR records the invariant,
scopes it to 0.6.0, and explicitly defers MVCC / concurrent writers
to 0.7+.

This ADR also closes Phase 2 decision-index #12
("Single-writer thread vs MVCC") by recording the deferral. The
deliberation entry is not separately drafted; this decision-recording
suffices.

## Context

`rusqlite` is sync. SQLite WAL mode supports concurrent readers but
serialises writers at the file level. The 0.5.x writer architecture
attempted concurrent-writer patches several times (citations below);
each attempt produced regressions:

- WAL writer-thread-safety patches surfacing `SQLITE_BUSY` /
  `SQLITE_LOCKED` under realistic load.
- `SQLITE_SCHEMA` flooding when multiple writer threads observed
  schema mid-migration.
- Database lock file collisions when two engines opened the same
  path (codified in `crates/fathomdb-engine/src/database_lock.rs`:
  one Engine instance per database file at a time).

The Stop-doing list in `learnings.md` records "writer-thread-safety
patches" and "`SQLITE_SCHEMA` flooding" as a single anti-pattern
class. Single-writer is the structural fix.

The async-surface ADR locked **sync engine surface** based partly on
the single-writer-thread invariant (Invariant A: scheduler post-commit;
writer lock is released before any scheduler dispatch). The op-store
ADR placed op-store rows in the primary sqlite file partly because
"one writer means one transactional fence; no cross-store atomicity
problem." Embedder-protocol Invariant 4 placed the embedder pool
**off** the writer thread but inside the same engine. All three
load-bear on this invariant.

## Decision

### 0.6.0 engine writer model

- **One writer thread per Engine instance.** All write transactions
  serialise through this thread.
- **Invariant numbering note.** Embedder-protocol §Invariant 4
  ("engine-owned thread") is the same rule as async-surface
  §Invariant B; embedder-protocol re-numbers it within its own
  five-invariant set. Citations below use embedder-protocol's
  numbering.
- **Writer thread PRAGMA invariants (mandatory; not optional).**
  - `journal_mode=WAL`.
  - `busy_timeout` ≥ 5000 ms (engine-managed default).
  - `wal_autocheckpoint` engine-managed (specific value lives in
    `design/engine.md`; not zero, not unbounded).
  - `synchronous` setting lives in `design/engine.md` per
    durability ADR (Phase 2 #7).
  These pragmas are part of the single-writer invariant: they are
  what makes "no SQLITE_BUSY regressions" structural rather than
  empirical.
- **One Engine instance per database file.** Enforced by
  `EngineRuntime::open` acquiring an exclusive file lock on
  `{database_path}.lock`. Hard constraint already encoded in code.
- **Concurrent readers permitted** via SQLite WAL mode. Reader
  connections do not block on the writer thread except during
  checkpoint windows (SQLite default).
- **Op-store, projection scheduler, vector-projection writes all
  share the single writer thread.** No separate writer lane for
  op-store (per ADR-0.6.0-op-store-same-file). No separate writer
  lane for the scheduler (per ADR-0.6.0-async-surface Invariant A).
- **Embedder pool runs on a separate thread pool** (per
  ADR-0.6.0-embedder-protocol Invariant 4) but never holds the
  writer lock; embed results are submitted back to the writer
  thread for commit.

### Out of scope for 0.6.0

- **MVCC.** No multi-version concurrency. Reader transactions see
  the latest committed snapshot; no historical-snapshot reads.
- **Multiple writer threads.** No writer pool. No optimistic-
  concurrency `BEGIN IMMEDIATE` retry loops in engine internals.
  (Clients never write SQL per ADR-0.6.0-typed-write-boundary; this
  scope is engine-internal only.)
- **Write sharding by table or namespace.** All writes are global.

### Deferral target

MVCC / concurrent writers are revisited at 0.7+ if and only if a
documented user-facing performance gate (per Phase 2 #9 retrieval
latency ADR or a future write-throughput ADR) requires it. Until
that gate exists, MVCC is not designed, prototyped, or stubbed —
same posture as ADR-0.6.0-no-shims-policy applies to feature work
that lacks a forcing function.

## Options considered

**A — Single writer thread (chosen).** Pros: matches `rusqlite`'s
actual constraint; one transactional fence simplifies op-store +
projection + vector writes; deadlock surface collapses to a
known-finite set (writer-vs-reader checkpoint windows only);
single-writer plus WAL is the widely-deployed SQLite production
posture. Cons: write throughput bounded by single-thread CPU + I/O.
Acceptable for 0.6.0 (no write-throughput SLI yet; retrieval is the
read-heavy path).

**B — Writer pool with `BEGIN IMMEDIATE` retry.** Pros: theoretical
write parallelism. Rejected on two distinct grounds:
- (a) **No parallelism gain.** SQLite serialises writers at the file
  level under WAL; a writer pool only changes which thread waits, not
  whether wait happens. Retry loops add latency without throughput.
- (b) **Empirical 0.5.x failure mode.** Each retry re-reads schema,
  amplifying the `SQLITE_SCHEMA` window. 0.5.x writer-thread-safety
  patches regressed via this exact mechanism (Stop-doing entry).

**C — MVCC via sqlx + WAL2 / BEDROCK fork.** Pros: real concurrent
writers. Cons: forks SQLite; loses sqlite-vec extension parity (per
ADR-0.6.0-sqlite-vec-acceptance: no fallback, no fork); 5–8k LoC
engine rewrite; reopens every Phase 1 ADR. Rejected.

**D — External transaction coordinator (e.g. SQLite + Raft per
engine).** Pros: durable replication path. Cons: 0.6.0 is
single-process by scope; the rewrite proposal explicitly drops
multi-process / replicated topology as out of scope. Rejected as
premature.

## Consequences

- `architecture.md` (Phase 3) documents the single writer thread as
  the central invariant, with all subsystem writes routing through
  it.
- `crates/fathomdb-engine/src/runtime.rs` and
  `crates/fathomdb-engine/src/database_lock.rs` continue to enforce
  one Engine per database file (already a hard constraint).
- `design/engine.md` (Phase 3) documents the writer thread, the
  reader connection pool, and the writer dispatch protocol.
- `design/scheduler.md` (Phase 3) cites Invariant A (post-commit
  dispatch) and explains how the scheduler avoids holding any
  reference to the writer lock.
- `design/embedder.md` cites Invariant 4 (embedder thread separate)
  + the writer-thread submission protocol.
- `test-plan.md`: regression tests for the Stop-doing class —
  concurrent-writer attempts must not regress
  `SQLITE_SCHEMA`-flooding or writer-lock deadlocks. Acceptance
  predicate above (N=8, zero error returns, ordered commit).
- Phase 2 decision-index #12 → resolved by deferral. The index entry
  flips to a pointer at this ADR; no separate deliberation drafted.
- ADR-0.6.0-async-surface Invariant A wording remains canonical for
  scheduler post-commit dispatch.
- 0.7+ MVCC / concurrent-writer ADR (if forced by an SLI) re-opens
  this ADR rather than amending it.
- Long-running regen jobs (vector-projection rebuild,
  `rebuild_actor.rs`) must yield the writer thread between batches:
  one transaction per batch, batch size bounded. Prevents a multi-
  tenant regen on profile A from blocking interactive writes on
  profile B. Specific batch size lives in `design/scheduler.md`.
- Acceptance test (regression): N=8 concurrent client write attempts
  must all commit in submission order, with zero `SQLITE_BUSY` /
  `SQLITE_SCHEMA` error returns. Latency bound is set in
  `test-plan.md`.
- `database_lock.rs` lock is advisory (`flock`); concurrent access
  by non-fathomdb processes to the same sqlite file is undefined
  and out of scope.

## Non-consequences (what this ADR does NOT do)

- Does not decide write-throughput SLI (separate Phase 2 acceptance
  ADR).
- Does not decide read-connection pool sizing (design-level).
- Does not decide whether the scheduler runs on the writer thread or
  a worker thread (settled by ADR-0.6.0-async-surface Invariant A:
  post-commit dispatch on the worker).
- Does not pin specific SQLite pragmas (journal_mode, synchronous,
  cache_size) — those are design-level in `design/engine.md`.
- Does not rule out replicated / multi-process topologies for 0.7+.

## Citations

- HITL decision 2026-04-27 (M-3 elevation per critic-3).
- ADR-0.6.0-async-surface § Invariant A (scheduler post-commit).
- ADR-0.6.0-op-store-same-file (one transactional fence).
- ADR-0.6.0-embedder-protocol § Invariant 4 (embedder pool off
  writer thread).
- `crates/fathomdb-engine/src/runtime.rs`,
  `crates/fathomdb-engine/src/database_lock.rs` (hard constraint:
  one Engine per database file; verified 2026-04-27).
- Stop-doing entries: writer-thread-safety patches; `SQLITE_SCHEMA`
  flooding; layers-on-layers concurrency abstractions.
- `dev/notes/0.6.0-rewrite-proposal.md` § "Architectural
  invariants".
