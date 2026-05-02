---
title: ADR-0.6.0-database-lock-mechanism
date: 2026-04-29
target_release: 0.6.0
desc: Hybrid lock — sidecar flock (Rust File::try_lock, BSD flock semantics) PLUS PRAGMA locking_mode=EXCLUSIVE on the writer connection in WAL; sidecar = pre-open fail-fast + PID diagnostic, SQLite EXCLUSIVE = same-process two-Engine correctness backstop + removes -shm
blast_radius: architecture.md § 5 + § 8 + § 11 (amend "no sidecar" assertion); design/engine.md (runtime open path); design/bindings.md § 7 (lock contract); src/rust/crates/fathomdb-engine/src/database_lock.rs (existing sidecar code retained); src/rust/crates/fathomdb-engine/src/runtime.rs (PRAGMA application); REQ-022a/b; REQ-041 (single-file-deploy interpretation refined)
status: accepted
---

# ADR-0.6.0 — Database lock mechanism

**Status:** accepted (HITL 2026-04-29).

Phase 3d-promoted ADR. Resolves the architecture-vs-code delta on database locking and the "no sidecar" assertion in architecture.md § 5 + § 8 + § 11, which was a critic-finding-driven removal not backed by an ADR. This ADR overrides that assertion with a documented trade-off analysis.

## Context

`Engine.open` must enforce: only one `Engine` instance per database file at a time, across processes AND across same-process bindings (Python + TypeScript constructed by the same operator). The lock protects against concurrent writer-thread instantiation, WAL ownership conflicts, and the single-writer architectural invariant.

Two failure modes drive the choice:
- **Concurrent open across processes** — second process attempting `Engine.open` on a held DB.
- **Concurrent open within one process** — Python and TypeScript bindings in the same process targeting the same DB path.

Engine.open is heavy: schema migrations + eager embedder model warmup (1–2 s typical, default-embedder per ADR-0.6.0-default-embedder) + PRAGMA application happen during open. Failing AFTER paying that cost is operationally bad.

`architecture.md` (locked 2026-04-29) § 5 + § 8 + § 11 currently committed: SQLite native exclusive lock; **no sidecar `.lock` file**. That commitment was a critic-finding removal without an ADR (the locked architecture.md cites: *"Earlier draft proposed a sidecar file — dropped per critic finding; no ADR authorizes it."*). Subsequent research surfaced that the chosen single-mechanism path does NOT satisfy both failure modes:

- `PRAGMA locking_mode=EXCLUSIVE` defers lock acquisition to the **first read** (per [pragma.html](https://www.sqlite.org/pragma.html#pragma_locking_mode): *"The first time the database is read in EXCLUSIVE mode, a shared lock is obtained and held."*). It is NOT acquired at `sqlite3_open`. Engine.open's migrations + embedder warmup run BEFORE the first read; `SQLITE_BUSY` only surfaces after the cost is paid.
- BSD `flock` (Rust 1.89 `File::try_lock` Unix path) is per-open-file-description. Two `open()` + `try_lock` calls in the same process succeed independently (per [gavv.net](https://gavv.net/articles/file-locks/)). Sidecar flock alone does NOT block same-process two-Engine.

A single mechanism cannot cover both failure modes. This ADR commits a hybrid.

## Decision

### Hybrid: sidecar `.lock` flock + `PRAGMA locking_mode=EXCLUSIVE` on writer connection in WAL.

Both layers MUST be present. Neither alone is sufficient.

### 1. Sidecar `{database_path}.lock` flock

- **Path derivation.** Database path is canonicalized (resolves symlinks + bind mounts) before deriving the lock path. Because `Engine.open` may target a fresh DB whose file does not yet exist, the canonicalization algorithm is: canonicalize the **parent directory** (which MUST exist; absence → `EngineOpenError::Io`) and append the leaf filename verbatim. Lock path = `{canonical_parent}/{leaf}.lock`. This defeats symlink + bind-mount aliasing (see [SQLite lockingv3](https://www.sqlite.org/lockingv3.html) alias warning, generalized to our sidecar) without requiring the DB file to pre-exist.
- **Acquisition primitive.** Rust std `File::try_lock` (stabilized 1.89) with its documented per-open-file-description exclusion semantics. The current stdlib implementation uses BSD `flock(2)` on Unix and `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY)` on Windows; this ADR's correctness depends on the *observable per-OFD exclusion semantics*, not on the underlying syscall. (Rationale for relying on `flock`-class semantics: avoid the close-fd-drops-lock bug demonstrated by RocksDB issue [#1780](https://github.com/facebook/rocksdb/issues/1780), which is specific to `fcntl` POSIX byte-range locks.) If a future Rust release changes the underlying syscall while preserving per-OFD semantics, this ADR is unaffected.
- **Order.** Acquired BEFORE any SQLite I/O — pre-open fail-fast surface (avoids paying migration + embedder warmup cost on a doomed open).
- **Lock-file content.** ASCII decimal PID of the holding process, no trailing newline, file mode `0600` on Unix (PID is operator diagnostic, not world-readable). Format is **best-effort diagnostic**, not parsed for correctness. The PID is surfaced in the `DatabaseLocked` error so operators can identify the holder.
- **Lifetime.** Released automatically when the file descriptor is closed (kernel-managed). The `.lock` *file* persists after process termination; this is normal — the kernel-held lock is released, and the next opener will re-acquire it. We do NOT unlink the lock file on shutdown (avoids races with concurrent openers).
- **Close semantics.** `Engine.close` releases the lock by dropping the `DatabaseLock` struct (which closes the fd). On `fork` after `Engine.open`, BSD `flock` semantics share the lock across all fds referring to the same open file description; the lock is released only when the last such fd is closed. A forked child that calls `Engine.open` independently opens a NEW `File` and gets `WouldBlock` → `DatabaseLocked` per § 4.

### 2. `PRAGMA locking_mode=EXCLUSIVE` on writer connection only, in WAL

- Applied at `Engine.open` on the **writer connection only**, after lock acquisition + before the first read.
- Reader connections in the same process use **default (NORMAL) locking_mode** and benefit from WAL multi-reader concurrency. This is required by REQ-018 ("reads do not serialize behind a single reader connection"); applying EXCLUSIVE to readers would defeat WAL's parallel-reader design.
- In WAL mode + EXCLUSIVE on the writer, SQLite does NOT create the `-shm` shared-memory file (per [wal.html](https://www.sqlite.org/wal.html)). The DB file set during normal operation is `db.sqlite` + `db.sqlite-wal`; `-shm` is absent.
- **Same-process two-Engine backstop.** Cross-process protection is fully delegated to the sidecar flock (§ 1). Same-process two-`Engine.open` is the case the SQLite EXCLUSIVE writer-lock catches *if* per-OFD flock semantics surprise us on an exotic platform: a hypothetical second writer connection attempting any read or write fails with `SQLITE_BUSY` because the first writer's EXCLUSIVE-mode SHARED lock blocks all other connections from acquiring even SHARED. In practice, the sidecar flock is the load-bearing layer for same-process exclusion (§ 4 + § 5); SQLite EXCLUSIVE is the defense-in-depth backstop.
- Same-process reader/writer coordination is owned by `EngineRuntime` lifecycle (single owner of `DatabaseLock`), NOT by SQLite lock state.

### 3. Acquisition invariants at `Engine.open`

This ADR commits the *invariants*; the concrete step ordering at `Engine.open` is owned by `design/engine.md` (runtime open path).

- **Inv-Lock-1.** The sidecar flock MUST be acquired before any SQLite I/O on the database file. Migration work + embedder warmup MUST NOT begin before the lock is held.
- **Inv-Lock-2.** `PRAGMA journal_mode=WAL` MUST be applied before `PRAGMA locking_mode=EXCLUSIVE` on the writer connection (WAL must be active when EXCLUSIVE first takes effect, so SQLite never creates `-shm`).
- **Inv-Lock-3.** `PRAGMA locking_mode=EXCLUSIVE` MUST be applied before the writer connection issues its first read (per [pragma.html](https://www.sqlite.org/pragma.html#pragma_locking_mode), the SHARED lock is taken on first read; EXCLUSIVE must be active by then for the same-process backstop to engage).
- **Inv-Lock-4.** On `EngineOpenError::Corruption` (per ADR-0.6.0-corruption-open-behavior § 5) or any other open-path failure after the sidecar lock is acquired, the lock MUST be released before the error is returned. No partial-state engine handle is observable.

### 4. Failure mode mapping

| Scenario | Detected by | Surfaces as |
|---|---|---|
| Different process holds DB | sidecar flock (Inv-Lock-1) | `DatabaseLocked { holder_pid: Some(N) }` |
| Same process, second `Engine.open` from sibling binding | sidecar flock (load-bearing): the second `Engine.open` opens a NEW `File` handle and calls `try_lock`; per-OFD exclusion semantics return `WouldBlock`. SQLite EXCLUSIVE on the writer is the defense-in-depth backstop only. | `DatabaseLocked` (PID = self if readable) |
| Cannot create or open `.lock` (read-only filesystem, permission denied, etc.) | filesystem error before flock attempt | `EngineOpenError::Io { source }` — NOT `DatabaseLocked`. |
| Crash with `.lock` file leftover | kernel released flock on process exit; next opener re-acquires successfully | (no error; transparent recovery) |
| PID in `.lock` file is stale / recycled | best-effort diagnostic only; correctness owned by kernel-held flock | (PID may misidentify; operators tolerate this) |
| Symlink / bind-mount aliasing | path canonicalization (§ 1) — canonicalize parent dir, append leaf filename | (correctness preserved) |
| Parent directory does not exist | canonicalization fails | `EngineOpenError::Io { source }` |
| NFS / network filesystem | both layers unreliable on NFS; not a 0.6.0 deployment target | undefined; documented out of scope |
| `EngineOpenError::Corruption` after lock held | per ADR-0.6.0-corruption-open-behavior § 5 + Inv-Lock-4: lock released before error returned | `CorruptionError` (lock released) |
| `fork` after `Engine.open`; child inherits fd | BSD flock semantics — parent and child share the lock via the same OFD; lock released when last fd closed | benign; no error |
| `fork`+child calls `Engine.open` independently | child's new `File`+`try_lock` returns `WouldBlock` | `DatabaseLocked` |

### 5. Same-process double-lock semantic per Rust 1.89 `File::try_lock`

[std::fs::File::try_lock](https://doc.rust-lang.org/std/fs/struct.File.html) documents that a same-process attempt to lock when this file handle (or a clone) already holds the lock is *"unspecified and platform dependent, including the possibility that it will deadlock."*

Mitigation: the second `Engine.open` in the same process opens a NEW `File` handle (independent from the first, not a clone). On Unix BSD `flock` semantics, the second `try_lock` call returns `WouldBlock` (EWOULDBLOCK). On Windows `LockFileEx` with `LOCKFILE_FAIL_IMMEDIATELY`, returns `ERROR_LOCK_VIOLATION`. Behavior is "fail" not "deadlock" because the docs' deadlock warning concerns the SAME `File` handle re-locking, which we do not do.

The SQLite EXCLUSIVE layer is the correctness backstop: even on a hypothetical platform where same-process flock semantics surprise us, SQLite's EXCLUSIVE-mode SHARED-lock conflict catches the second-Engine case.

## Options considered

**A — SQLite-native `PRAGMA locking_mode=EXCLUSIVE` only.** Rejected. Lock is acquired on first read (per pragma.html), not at `sqlite3_open`. Engine.open pays migration + embedder warmup cost BEFORE the lock surface — failed locks waste ~1–2 s of work and produce confusing operator UX (long-running open that fails late). Plus `SQLITE_BUSY` is generic; cannot distinguish "another process holds DB" from "transient busy" without nuanced extended-code interpretation, and there is no extended code for "another process opened with EXCLUSIVE locking_mode" ([rescode.html](https://www.sqlite.org/rescode.html)).

**B — Sidecar `flock` only.** Rejected. BSD `flock` is per-open-file-description (per gavv.net); two `File::open` + `try_lock` calls in the same process can both succeed. Same-process two-Engine is not blocked. Rust 1.89 docs explicitly state same-process double-lock semantics are "unspecified, possibly deadlock" — we cannot rely on it.

**C — Hybrid: SQLite native lock + sidecar PID-hint file (no flock).** Rejected. PID file is metadata; correctness reverts to A's failure modes. Adds disk artifact without adding the fail-fast or same-process correctness benefits.

**D — Hybrid: sidecar flock + PRAGMA locking_mode=EXCLUSIVE in WAL (chosen).** Both failure modes covered. Sidecar = pre-open fail-fast + diagnostic; SQLite EXCLUSIVE = same-process backstop + removes `-shm`. Trade: one extra on-disk file (`.lock`).

**E — Lock the data file directly (BoltDB approach).** Rejected. Locking the file SQLite is also touching may interact with SQLite's own POSIX locks on Unix in ways the SQLite team has not endorsed. Sidecar isolates the application-level lock from SQLite's internal locking machinery.

**F — `open(O_EXCL | O_CREAT)` on a marker file.** Rejected. Not crash-safe — leftover marker after SIGKILL blocks all future opens until manual cleanup. Anti-pattern.

**G — `BEGIN EXCLUSIVE` on writer transaction only.** Rejected. Per-transaction granularity; doesn't satisfy "one Engine per file" (two Engines coexist for reads, serialize on writes; writer thread constructs but writes block). Wrong protection layer.

**H — Process-global Rust registry keyed by canonical path (no OS lock).** A `OnceLock<Mutex<HashMap<PathBuf, Weak<EngineRuntime>>>>` or similar at the crate level that refuses a second `Engine.open` for the same canonical path. Rejected: covers same-process only, not cross-process; cross-binding (Python and TypeScript loaded as separate cdylib instances within one operator process) bypasses it because each cdylib has its own static — separate `OnceLock`s, separate registries, no coordination; not load-bearing for the failure modes that matter. Useful only as an additional in-language ergonomic guard, not as the primary mechanism.

## Consequences

- **architecture.md amendment required.** § 5 + § 8 + § 11 currently assert "no sidecar `.lock` file." This ADR overrides those assertions. Amendment to architecture.md (which is locked) is authorized by this ADR per the precedent set by ADR-0.6.0-crate-topology's 2026-04-27 amendment.
- **REQ-041 (single-file-deploy) refinement.** Strict reading of REQ-041 ("operator deploys one binary + one `.sqlite` path") was already a clean-shutdown invariant, not an at-runtime invariant: WAL mode creates `-wal` (and pre-this-ADR also `-shm`); a crash leaves `-wal` orphaned. This ADR adds `.lock` as a third sidecar artifact during operation. After clean shutdown, `db.sqlite` + `db.sqlite.lock` remain; `-wal` auto-deletes; `-shm` was already absent under EXCLUSIVE. Operators must understand `db.sqlite.lock` is part of the database file set. REQ-041 wording does NOT need amendment — REQ-041 commits to "one binary + one path"; the sidecar lockfile is at the same path with a `.lock` suffix and is not an additional operator-managed artifact. **REQ-041 SHOULD gain a Cross-cite to this ADR** so future readers learn of the refined interpretation.
- **`DatabaseLocked` error variant.** Carries optional `holder_pid: Option<u32>`. Surfaces in bindings as a typed error per ADR-0.6.0-error-taxonomy. Variant added to `design/errors.md` taxonomy on the open-path module-error enum, surfacing on `EngineError` per ADR-0.6.0-error-taxonomy. (The exact module-error enum name — e.g. `EngineOpenError` — is owned by `design/errors.md`.)
- **`design/engine.md` runtime open path** spec must implement the 10-step acquisition order in § 3.
- **Path canonicalization** is a new requirement for `Engine.open` (was not previously in architecture.md). Failure to canonicalize → return `EngineOpenError::Io` with the underlying error.
- **NFS / network filesystem:** both lock layers unreliable on NFS. Not a 0.6.0 deployment target. Behavior is undefined on NFS; out of scope for this ADR. Operators choosing to deploy on a network filesystem do so against guidance.
- **Existing code at `src/rust/crates/fathomdb-engine/src/database_lock.rs`** is the sidecar half of D. It needs (a) path canonicalization added, (b) `PRAGMA locking_mode=EXCLUSIVE` applied at the writer connection in `runtime.rs`, (c) the order-of-operations in § 3 enforced. This ADR does NOT mandate immediate refactor — implementation phase wires it.
- **Cross-binding lock contract** (`design/bindings.md` § 7) refreshed: SDK-native locking + sidecar flock; both layers; same-process two-Engine guaranteed by the union, not by either alone.

## Citations

- HITL 2026-04-29.
- [PRAGMA locking_mode (sqlite.org)](https://www.sqlite.org/pragma.html#pragma_locking_mode) — *"The first time the database is read in EXCLUSIVE mode, a shared lock is obtained and held"*; *"WAL databases can be accessed in EXCLUSIVE mode without the use of shared memory."*
- [File Locking And Concurrency v3 (sqlite.org)](https://www.sqlite.org/lockingv3.html) — lock states; alias warning re: journal files.
- [Write-Ahead Logging (sqlite.org)](https://www.sqlite.org/wal.html) — WAL + EXCLUSIVE removes `-shm`.
- [SQLite Result and Error Codes](https://www.sqlite.org/rescode.html) — `SQLITE_BUSY` is generic; no extended code for "EXCLUSIVE locking_mode held by other process."
- [Rust std::fs::File::try_lock](https://doc.rust-lang.org/std/fs/struct.File.html) — stabilized 1.89; Unix `flock` / Windows `LockFileEx`; same-handle double-lock unspecified.
- [Rust 1.89 release notes](https://blog.rust-lang.org/2025/08/07/Rust-1.89.0/) — stabilization.
- [gavv.net — File locking in Linux](https://gavv.net/articles/file-locks/) — BSD `flock` per-open-file-description vs `fcntl` per-`(inode,pid)`.
- [apenwarr — Everything you never wanted to know about file locking](https://apenwarr.ca/log/20101213) — `fcntl` close-bug; NFS unreliability.
- [RocksDB issue #1780](https://github.com/facebook/rocksdb/issues/1780) — `fcntl` close-fd-drops-lock bug; cautionary case for `flock(2)` over `fcntl`.
- [BoltDB README](https://github.com/boltdb/bolt) — locks data file directly (rejected option E).
- ADR-0.6.0-corruption-open-behavior § 5 — lock released before `CorruptionError` returned.
- ADR-0.6.0-single-writer-thread (architectural prerequisite for one-Engine-per-file).
- ADR-0.6.0-error-taxonomy (DatabaseLocked variant home).
- REQ-020a/b, REQ-022a/b (close releases lock; lock-acquisition contract).
- REQ-041 (single-file-deploy interpretation).
