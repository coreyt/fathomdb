---
title: ADR-0.6.0-vector-index-location
date: 2026-04-27
target_release: 0.6.0
desc: Vector index lives in the same SQLite file as a vec0 virtual table; closes against sqlite-vec-acceptance + zerocopy-blob
blast_radius: fathomdb-engine vector index path; on-disk file layout; backup / recovery semantics; deps/sqlite-vec.md
status: accepted
---

# ADR-0.6.0 — Vector index location

**Status:** accepted (HITL 2026-04-27, decision-recording — lite batch).

Phase 2 #13 architecture ADR. Closure ADR — the substantive choice was
already made when ADR-0.6.0-sqlite-vec-acceptance accepted `sqlite-vec`
sole-maintainer risk. This ADR records the **placement** consequence for
future cross-cite.

## Context

Vector index can live in: (a) the same SQLite file as application data via
`sqlite-vec`'s `vec0` virtual table; (b) a sibling SQLite file; (c) an
external store (lance, qdrant, FAISS-on-disk, hnswlib + manual persistence).

Already-decided constraints lock the answer:

- **ADR-0.6.0-sqlite-vec-acceptance** chose `sqlite-vec` because it is the
  only ANN that integrates as a SQLite virtual table, preserving the
  single-file invariant.
- **ADR-0.6.0-zerocopy-blob** specified vector storage as LE-f32 BLOB rows
  inside the same SQLite file.
- **ADR-0.6.0-op-store-same-file** keeps the operational store in the same
  file (no dual-store).
- **Non-goal** "no data migration" forbids splitting vectors out later
  without a migration ADR.

## Decision

**Vector index lives in the same SQLite file as application + op-store
data, as a `vec0` virtual table.**

- One file per database. Vectors, application data, op-store, and `vec0`
  shadow tables all share that file.
- Backup = copy the file (or use `sqlite3` online backup API). No split
  backup orchestration.
- Recovery = open the file. No second-store reconciliation.
- `vec0` shadow tables are an implementation detail of `sqlite-vec`; the
  engine treats them as engine-private rows (clients never see them per
  ADR-0.6.0-typed-write-boundary).

## Options considered

**A — Same SQLite file, `vec0` virtual table (chosen).** Pros: preserves
single-file invariant; backup/recovery story is one file; transactional
writes across application + vector data; no cross-store consistency
problem; matches three already-accepted ADRs. Cons: file size grows with
vector count; one corruption blast-radius; tied to `sqlite-vec` (already
accepted in #3).

**B — Sibling `.vec.sqlite` file.** Pros: smaller main DB; vectors
isolatable. Cons: breaks single-file invariant; backup orchestration; no
cross-file transactions (vec writes can succeed when application writes
roll back); reconciliation logic on recovery. Rejected — re-litigates
`sqlite-vec-acceptance`.

**C — External store (lance / qdrant / FAISS-on-disk).** Pros: more
mature ANN. Cons: breaks embedded-DB premise of fathomdb; network /
process boundary; no transactional vector writes; adds operational
surface. Rejected — same reason as `sqlite-vec-acceptance` Option D.

## Consequences

- `architecture.md` data-flow lists one SQLite file per database;
  vector + op-store + application share it.
- `design/vector.md` documents `vec0` virtual table use; shadow tables
  named per `sqlite-vec` conventions; engine never exposes them.
- `design/engine.md` durability section: a vector write is itself a
  single atomic transaction on the writer thread, dispatched
  **post-commit** of the originating application insert per
  ADR-0.6.0-async-surface Invariant A. The originating row and its
  vector are NOT in the same transaction — the embedder runs after
  the originating commit, and a second writer-thread transaction
  inserts the vector. Atomicity is per-write, not application-row +
  vector-row paired.
- **Corruption posture (deferred).** Single-file means single corruption
  blast-radius across application + op-store + `vec0` shadow tables.
  Detection, `Engine.open` behavior on detected corruption, and recovery
  ownership are deferred to a future "ADR-0.6.x-corruption-recovery";
  this ADR records that the deferral is intentional, not an oversight.
  Followup tracked in `followups.md`.
- Splitting vectors into a separate file in a later **major** release
  would require its own migration ADR. (The 0.5→0.6 "no data migration"
  non-goal does not by itself govern future major boundaries; it is the
  per-release ADR that does.)
- Backup story = "copy the file" (or `VACUUM INTO` / online backup);
  no per-store backup tooling needed.
- File-size monitoring is a single number, not two.

## Citations

- ADR-0.6.0-sqlite-vec-acceptance (substantive choice).
- ADR-0.6.0-zerocopy-blob (BLOB layout).
- ADR-0.6.0-op-store-same-file (single-file precedent).
- ADR-0.6.0-typed-write-boundary (clients never see `vec0` shadow tables).
- HITL 2026-04-27.
