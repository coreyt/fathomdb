---
title: ADR-0.6.0-durability-fsync-policy
date: 2026-04-27
target_release: 0.6.0
desc: SQLite synchronous mode + WAL checkpoint policy + durability/recovery numerical targets
blast_radius: src/rust/crates/fathomdb-engine writer + open path; design/engine.md PRAGMA section; test-plan.md durability tests; ADR-0.6.0-single-writer-thread mandatory PRAGMAs
status: accepted
---

# ADR-0.6.0 — Durability + fsync policy

**Status:** accepted (HITL 2026-04-27).

Phase 2 #7 acceptance ADR. Sets fsync policy + numerical durability and recovery-time targets.

## Context

SQLite WAL mode has `synchronous` levels (`OFF` / `NORMAL` / `FULL` / `EXTRA`) controlling fsync frequency. Crash-recovery semantics depend on which commit batches survive a power-cut vs OS-crash. Required for `acceptance.md` (each AC must be testable + numerical).

## Decision

- **`synchronous=NORMAL`** (default for engine writer connection).
- **WAL `journal_mode=WAL`** + engine-managed `wal_autocheckpoint` (specific value lives in `design/engine.md`; not zero, not unbounded). Already mandatory per ADR-0.6.0-single-writer-thread.
- **Durability target.** Zero corruption on power-cut. Up to 100ms of final-commit loss on power-cut acceptable. Zero commit loss on OS-crash (fsync at checkpoint).
- **Recovery-time target.** ≤ 2 seconds for a 1 GB database at `Engine.open` after unclean shutdown. Measured from process start to first accepted write transaction.

## Options considered

**A — `synchronous=NORMAL` + default checkpointing (chosen).** Industry-standard SQLite production posture. Survives OS-crash without loss; may lose final ~100ms on power-cut but never corrupts. Modest fsync cost.

**B — `synchronous=FULL` + smaller checkpoint window.** Per-commit fsync; survives power-cut with zero loss. Write latency 5–10× higher per commit. Strongest durability; not justified by any 0.6.0 forcing function.

**C — `synchronous=OFF` + engine-controlled fsync every N commits.** Fastest writes; engine drives durability cadence. Adds complexity + speculative-knob "N." Rejected.

## Consequences

- `design/engine.md` documents the PRAGMA set + `wal_autocheckpoint` value.
- `test-plan.md`: durability AC (power-cut simulation: kill -9 mid-commit, reopen, no corruption); recovery-time AC (1 GB seeded DB, unclean shutdown, reopen ≤ 2s wall-clock).
- Tightening to B revisits this ADR; requires forcing function (e.g. user/regulatory durability gate).

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-single-writer-thread (mandatory PRAGMA invariants).
- SQLite WAL documentation; `synchronous` semantics.
