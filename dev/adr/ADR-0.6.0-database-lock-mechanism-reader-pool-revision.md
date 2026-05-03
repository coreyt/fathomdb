---
title: ADR-0.6.0-database-lock-mechanism-reader-pool-revision
date: 2026-05-02
target_release: 0.6.0
desc: Revise the database-lock contract to drop `PRAGMA locking_mode=EXCLUSIVE` on the writer connection. The sidecar `.lock` flock is retained verbatim from the predecessor ADR and remains the load-bearing mechanism for both cross-process and same-process two-`Engine` exclusion. EXCLUSIVE on the writer is incompatible with the Phase 8 reader pool because WAL + EXCLUSIVE skips the shared-memory wal-index, serializing all reader connections.
blast_radius: dev/acceptance.md AC-002 (allow-list expands to include `-shm`); dev/design/bindings.md § 7 + § 14 item 8 (drop EXCLUSIVE clause); dev/design/engine.md (runtime open path — writer omits locking_mode pragma); dev/requirements.md REQ-041 cross-cite; src/rust/crates/fathomdb-engine writer-connection setup; src/rust/crates/fathomdb-engine/tests/lifecycle_observability.rs AC-002 allow-list.
status: accepted
supersedes: ADR-0.6.0-database-lock-mechanism
---

# ADR-0.6.0 — Database lock mechanism (reader-pool revision)

**Status:** accepted (HITL 2026-05-02; per `dev/progress/0.6.0.md` Phase 8 architectural-conflict resolution).

Successor to [`ADR-0.6.0-database-lock-mechanism`](./ADR-0.6.0-database-lock-mechanism.md). Retains that ADR's sidecar-flock half verbatim by reference; drops the `PRAGMA locking_mode=EXCLUSIVE` half. The predecessor ADR remains in-tree marked `superseded` with the original body preserved for history.

## Context

Phase 8 wires a reader pool of long-lived rusqlite connections sharing the database file with a single writer connection. Acceptance gates AC-020 (reader parallelism — multiple in-flight `search()` calls progress concurrently) and AC-021 (zero `SQLITE_SCHEMA` errors under concurrent reads + admin DDL on a 60 s window) require multi-reader concurrency through SQLite's WAL shared-memory wal-index.

The predecessor ADR § 2 mandated `PRAGMA locking_mode=EXCLUSIVE` on the writer connection, citing two motivations: (a) defense-in-depth for the same-process two-`Engine` case as a backstop behind the sidecar flock, and (b) suppressing the `-shm` file so REQ-041's "single-file deploy" interpretation tolerated only `db.sqlite` + `db.sqlite-wal` + `db.sqlite.lock` during operation.

SQLite's [WAL documentation](https://www.sqlite.org/wal.html) is explicit: _"if EXCLUSIVE locking mode is set prior to the first WAL-mode read transaction, then SQLite skips the use of shared memory wal-index ... only a single database connection at a time can access the database file."_ Empirical Phase 8 measurement confirmed the consequence: with the writer holding EXCLUSIVE WAL, every reader-pool connection contended for the database via on-disk locks rather than the wal-index, producing sustained `SQLITE_BUSY` on every reader. Each reader call hit rusqlite's 5 s `busy_timeout`. The AC-004b workload (500 search operations) measured ~2503 s wall-clock per test under EXCLUSIVE, versus seconds without it.

The predecessor ADR's hybrid posture is therefore irreconcilable with the reader pool. Both `PRAGMA locking_mode=EXCLUSIVE` on the writer AND a parallel-reader pool cannot be true simultaneously under current SQLite semantics. One must be dropped. The reader pool is gated by AC-020 and AC-021; the EXCLUSIVE writer pragma is a defense-in-depth backstop for a hypothetical platform-flock surprise that has not been observed.

## Decision

Drop `PRAGMA locking_mode=EXCLUSIVE` from the writer connection. Retain the sidecar `{database_path}.lock` flock unchanged. Reader-pool connections continue to open with default (NORMAL) locking_mode and `query_only=ON`. The writer connection opens with `journal_mode=WAL` and no locking_mode pragma; `-shm` is created and used as a normal WAL artifact.

### What is retained verbatim from the predecessor ADR

The sidecar-flock half of `ADR-0.6.0-database-lock-mechanism` is retained without modification. Specifically:

- **§ 1** — sidecar `{database_path}.lock` flock: path canonicalization (canonicalize parent dir, append leaf), Rust std `File::try_lock` with BSD `flock` semantics on Unix and `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY)` on Windows, ASCII decimal PID lock-file content (mode 0600 on Unix, best-effort diagnostic), kernel-managed lifetime, no unlink on shutdown, fork semantics.
- **§ 3 Inv-Lock-1** — the sidecar flock MUST be acquired before any SQLite I/O on the database file.
- **§ 3 Inv-Lock-4** — on `EngineOpenError::Corruption` or any other open-path failure after the sidecar lock is acquired, the lock MUST be released before the error is returned.
- **§ 4 failure-mode mapping** — the cases-table rows for "different process holds DB", "same process, second `Engine.open` from sibling binding", "cannot create or open `.lock`", "crash with `.lock` file leftover", "stale PID", "symlink / bind-mount aliasing", "parent directory does not exist", "NFS / network filesystem", "Corruption after lock held", "fork after Engine.open", and "fork+child Engine.open".
- **§ 5** — same-process double-lock semantic (the second `Engine.open` opens a new `File` handle, not a clone; per-OFD flock returns `WouldBlock`).

Refer to the predecessor ADR for the full text. This ADR does not duplicate it.

### What is dropped

- **Predecessor ADR § 2** — `PRAGMA locking_mode=EXCLUSIVE` on the writer connection. The pragma is not applied. Writer connection setup applies `PRAGMA journal_mode=WAL` only, alongside the unrelated PRAGMAs already enumerated in `dev/design/engine.md`.
- **Predecessor ADR § 3 Inv-Lock-2** — "WAL must be active before EXCLUSIVE first takes effect, so SQLite never creates `-shm`." Removed; `-shm` is created normally.
- **Predecessor ADR § 3 Inv-Lock-3** — "EXCLUSIVE must be active before the writer's first read." Removed.

### Invariant re-derivation

With EXCLUSIVE removed, the invariant set reduces to:

- **Inv-Lock-1 (retained).** The sidecar flock MUST be acquired before any SQLite I/O on the database file. Migration work + embedder warmup MUST NOT begin before the lock is held.
- **Inv-Lock-2 (revised).** `PRAGMA journal_mode=WAL` MUST be applied on the writer connection at open. (The original Inv-Lock-2's ordering relative to EXCLUSIVE is moot.)
- **Inv-Lock-3 (removed).** No EXCLUSIVE-ordering invariant remains.
- **Inv-Lock-4 (retained).** On open-path failure after the sidecar lock is acquired, the lock MUST be released before the error is returned.

The predecessor ADR's § 4 cases-table row for "Same process, second `Engine.open` from sibling binding" already labels the sidecar flock as the load-bearing layer for this case ("sidecar flock (load-bearing): the second `Engine.open` opens a NEW `File` handle and calls `try_lock`; per-OFD exclusion semantics return `WouldBlock`. SQLite EXCLUSIVE on the writer is the defense-in-depth backstop only."). The load-bearing mechanism is therefore preserved by this ADR. Only the defense-in-depth backstop is removed.

Cross-process exclusion likewise depends entirely on the sidecar flock (predecessor ADR § 4, "Different process holds DB"). EXCLUSIVE was never load-bearing for that case. No invariant covering cross-process exclusion changes.

## Consequences

- **`-shm` is created during normal WAL operation** per [https://www.sqlite.org/wal.html](https://www.sqlite.org/wal.html). It is part of the documented database file set during operation. After clean shutdown the WAL checkpoint runs and `-shm` may persist or be removed depending on connection close ordering; both states are normal and not error conditions.
- **AC-002 amendment.** The allow-list of files an `Engine.open` + write + search + close cycle may create grows to include `{database}-shm`. `dev/acceptance.md` AC-002 assertion text is updated by this ADR. The corresponding test allow-list in `src/rust/crates/fathomdb-engine/tests/lifecycle_observability.rs` is updated likewise.
- **REQ-041 (single-file-deploy) interpretation.** During runtime, the operational artifact set is `{db}`, `{db}-wal`, `{db}-shm`, `{db}.lock`. After clean shutdown, `{db}` and `{db}.lock` remain (per the predecessor ADR's REQ-041 refinement; `-wal` auto-deletes on clean checkpoint, `-shm` may auto-delete with last connection close). REQ-041 wording does not change beyond the cross-cite update to add this ADR alongside the predecessor.
- **Defense-in-depth gap.** The same-process backstop (EXCLUSIVE catching a hypothetical platform where flock per-OFD semantics surprise us) is removed. For 0.6.0 the sidecar flock is the sole mechanism for same-process two-`Engine` exclusion. This is the same load-bearing layer the predecessor ADR already used; only the secondary backstop is gone. Future hardening — a process-global Rust registry per option H of the predecessor ADR, or any equivalent in-process guard — is out of scope for 0.6.0 and revisits in a 0.8.0 hardening release if a real failure surfaces.
- **`architecture.md` § 5 + § 8 + § 11** were already amended by the predecessor ADR to admit the sidecar `.lock` file. No further amendment is required by this ADR; the sidecar half is unchanged.
- **`dev/design/engine.md`** runtime open path: the writer connection opens without a `locking_mode` pragma. If the spec text currently states EXCLUSIVE, update to reflect this ADR.
- **`dev/design/bindings.md` § 7** lock contract: the SDK-binding contract now reads "sidecar flock + reader pool"; the EXCLUSIVE writer-lock clause is removed. § 14 item 8 is updated likewise.
- **`dev/adr/ADR-0.6.0-decision-index.md`** — the predecessor ADR row is marked `superseded (2026-05-02)` and a new row is added for this ADR with status `accepted (HITL 2026-05-02)`.

## Options considered

- **A — Keep EXCLUSIVE; drop the reader pool.** Rejected. Funnels reads through a single connection, fails AC-020 (reader parallelism gate), and fails AC-021 (concurrent reads + admin DDL within the 60 s window).
- **B — Drop EXCLUSIVE only; route readers through the writer connection (single connection, shared cache).** Rejected. Single-connection serialization fails AC-020 in the same way option A does; SQLite's `SQLITE_OPEN_SHAREDCACHE` mode is also globally deprecated by SQLite for new code.
- **C — Drop EXCLUSIVE; admit `-shm`; expand AC-002 allow-list (chosen).** Sidecar flock remains load-bearing for both cross-process and same-process exclusion per the predecessor ADR's own § 4 cases-table. The defense-in-depth backstop is removed; no observed failure mode regresses.
- **D — Open the writer with EXCLUSIVE only when the reader pool is disabled (config-conditional).** Rejected. Adds runtime mode branching to the lock contract (which must be invariant for ADR review and for binding parity), and the reader pool is unconditionally on for 0.6.0.

## Citations

- HITL 2026-05-02; resolves the Phase 8 architectural blocker recorded in `dev/progress/0.6.0.md` (2026-05-02 entry — Fix 1 escalated by the code-reviewer).
- Predecessor: [`ADR-0.6.0-database-lock-mechanism`](./ADR-0.6.0-database-lock-mechanism.md) — sidecar half retained verbatim by reference.
- [SQLite WAL documentation](https://www.sqlite.org/wal.html) — _"if EXCLUSIVE locking mode is set prior to the first WAL-mode read transaction, then SQLite skips the use of shared memory wal-index ... only a single database connection at a time can access the database file."_
- AC-020, AC-021 in `dev/acceptance.md` (reader parallelism + concurrent reads + admin DDL).
- AC-002 in `dev/acceptance.md` (allow-list amendment in this ADR).
- REQ-041 in `dev/requirements.md` (single-file-deploy interpretation).
