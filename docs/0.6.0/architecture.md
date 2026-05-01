---
title: 0.6.0 Architecture
date: 2026-04-27
target_release: 0.6.0
desc: Crate topology, module subsystems, write/read flow, on-disk layout, ADR + REQ traceability
blast_radius: workspace Cargo.toml; every crates/* dir; every design/*.md (subsystem boundaries derive from this doc); every interfaces/*.md (binding boundary derives from this doc); op-store + vector + projection writer paths; rusqlite usage shape
status: locked
locked_date: 2026-04-29
---

# Architecture

Authoritative source for crate boundaries, module subsystems inside
`fathomdb-engine`, write/read data flow, and on-disk layout. Every
component traces to ≥1 accepted ADR and/or REQ from `requirements.md`;
no orphans.

This doc is **load-bearing for `design/*.md`** — each subsystem listed
below gets one design doc that owns its detailed semantics.

---

## 1. Crate topology

Per ADR-0.6.0-crate-topology (and 2026-04-27 amendment): monolithic
`fathomdb-engine`; module boundaries inside the engine via `pub(crate)`.

**Current workspace (2026-04-27):**

| Crate | Status | Responsibility |
|---|---|---|
| `crates/fathomdb` | exists | Top-level facade re-export crate (thin shim around `fathomdb-engine`) |
| `crates/fathomdb-engine` | exists | Engine core — all module subsystems below |
| `crates/fathomdb-query` | exists | Pure AST-to-plan compiler — `QueryAst`, `QueryBuilder`, `compile_*` fns, `CompiledQuery` / `CompiledSearchPlan` types, FTS5 grammar adapt. No `dyn` trait objects, no runtime state, no I/O. Engine consumes as pure-function dependency. (0.6.0 disposition: KEPT separate per HITL 2026-04-29; rationale = compile-vs-runtime split, snapshot-test isolation; `design/retrieval.md` consumes.) |
| `crates/fathomdb-schema` | exists | Schema migration definitions; runs at `Engine.open` per REQ-042 |
| `python/` (cdylib package `fathomdb`) | exists | PyO3 binding (built via `pip install -e python/` per memory `feedback_python_native_build`); Python sync surface per ADR-0.6.0-python-api-shape |

**0.6.0-target additions (created during implementation phase):**

| Crate | Phase 5 disposition | Responsibility |
|---|---|---|
| `crates/fathomdb-cli` | new in 0.6.0 | Single binary: two-root operator surface (`fathomdb recover` for lossy recovery; `fathomdb doctor <verb>` for bit-preserving inspection/export) per ADR-0.6.0-cli-scope. Does NOT mirror full SDK 5-verb surface. |
| `ts/` (cdylib package `fathomdb`) | new in 0.6.0 | napi-rs binding: Promise surface per ADR-0.6.0-typescript-api-shape (Path 2 ThreadsafeFunction pool sized at `num_cpus::get()` per ADR-0.6.0-async-surface) |
| `crates/fathomdb-embedder-api` | new in 0.6.0 | Sibling: shared trait crate — semver-stable surface pinning `Embedder` + `EmbedderIdentity` per ADR-0.6.0-embedder-protocol; enables version-skew detection at resolution time (REQ-047). Authorized by ADR-0.6.0-crate-topology 2026-04-27 amendment. |
| `crates/fathomdb-embedder` | new in 0.6.0 | Sibling: operator-installable embedder package; depends on `fathomdb-embedder-api`; bundles default candle + tokenizers per ADR-0.6.0-default-embedder. |

`fathomdb-query` disposition resolved 2026-04-29 (HITL): **kept separate**.
Rationale = pure AST-to-plan compiler with no `dyn` trait objects and no
runtime state (the `QueryEmbedder` trait is deliberately placed in
`fathomdb-engine`, not here, to preserve this property — see
`crates/fathomdb-engine/src/embedder/mod.rs:1-7`); enables hermetic
snapshot tests of compiled SQL without engine fs/lock/db dependencies;
mirrors a compile-vs-runtime split. `design/retrieval.md` consumes it
as a pure-function dependency.

**Directory layout (`python/`, `ts/`) is unchanged from 0.5.x.** Only
the cdylib crate name inside changes (the build path
`pip install -e python/` per memory continues to work; the npm package
is built from `ts/` analogously).

**No subprocess bridge crate** (FU-WIRE15 deferred to 0.8.0).

**No internal-types public surface.** Module boundaries inside
`fathomdb-engine` are not semver-stable; only the surface defined by
`interfaces/rust.md` is.

**Engine deps-out (informational):** `rusqlite`, `sqlite-vec` (loadable
extension), `tokio`, `serde`, `thiserror`, `tracing`, `candle`,
`tokenizers`. (`hf-hub` model-resolver is owned by FU-EMB5 and not
committed here.)

## 2. Module subsystems inside `fathomdb-engine`

Each module = one `design/<name>.md` file. Modules listed in approximate
**top-down** order: `runtime` is the highest-level facade; lower rows
are dependencies.

| Module | Responsibility | Owning ADRs | Owning REQs | design/*.md file |
|---|---|---|---|---|
| `runtime` | `Engine.open` / `close`; database file lock acquisition (via SQLite native exclusive WAL lock — see § 5); eager embedder warmup at open (Invariant D dispatched here, owned by `embedder`); engine-config marshal; PRAGMA application; `Engine` struct lifetime; binding-facing facade | single-writer-thread, async-surface (Invariant A coordination), default-embedder, op-store-same-file, vector-index-location, durability-fsync-policy (PRAGMAs at open), provenance-retention (cap configurable here) | REQ-020a/b, REQ-021, REQ-022a/b, REQ-023, REQ-031c, REQ-032, REQ-033, REQ-041, REQ-042, REQ-043, REQ-044, REQ-051 | `design/engine.md` (runtime + writer + reader + migrations sub-sections) |
| `writer` | Single dedicated OS thread; owns the only writer rusqlite connection; processes `PreparedWrite` enum variants; releases lock before scheduler dispatch | single-writer-thread, typed-write-boundary, prepared-write-shape, async-surface (Invariant A), durability-fsync-policy, provenance-retention (eviction here) | REQ-009a/b, REQ-019, REQ-027, REQ-028a/b/c, REQ-031, REQ-031b, REQ-053 | `design/engine.md` (writer sub-section) |
| `reader` | Multi-connection reader pool (no serialization on a single conn per REQ-018); per-thread connection acquisition; SQLite WAL read-tx | async-surface (Path 1 sync read surface) | REQ-013, REQ-014, REQ-018 | `design/engine.md` (reader sub-section) |
| `migrations` | Auto-migrate at `Engine.open`; per-step structured event emission (success + failure); accretion-guard linter target | (no dedicated ADR — leverages `fathomdb-schema`) | REQ-042, REQ-045 | `design/migrations.md` |
| `errors` | Per-module error enums; top-level `EngineError` wrapping via `#[from]`; binding error-mapping tables | error-taxonomy | REQ-056 | `design/errors.md` (cross-cuts every module; standalone for clarity) |
| `op_store` | `OpStoreInsert` PreparedWrite variant + `operational_*` tables in same SQLite file; transactional with primary writes; payload validation via JSON Schema per `schema_id` | op-store-same-file, json-schema-policy, typed-write-boundary | REQ-053, REQ-057, REQ-058, REQ-059 | `design/op-store.md` |
| `embedder` | Embedder dispatch pool (`embedder_pool_size`, default = `num_cpus::get()`); `embed()` invocation per ADR-0.6.0-embedder-protocol Invariants 1–4; eager warmup; per-call timeout (Invariant D); returns logical `Vec<Vector>` (BLOB encoding owned by `vector`) | async-surface (Invariants B + C + D), embedder-protocol, default-embedder, vector-identity-embedder-owned | REQ-028a/b/c, REQ-033 (no implicit network fetch) | `design/embedder.md` |
| `scheduler` | Tokio runtime worker pool (orchestration only, default 2 threads); dispatches projection jobs post-commit per Invariant A; `spawn_blocking` to embedder pool; mpsc back to writer; 4-layer backpressure | scheduler-shape, async-surface (Invariant A), projection-model | REQ-015, REQ-016, REQ-027, REQ-029, REQ-030, REQ-055 | `design/scheduler.md` |
| `vector` | `vec0` virtual table inside same SQLite file; LE-f32 BLOB encoding + alignment + byte-length invariants (BLOB encoding boundary); sqlite-vec usage; rebuild path for physical recovery | vector-index-location, sqlite-vec-acceptance, zerocopy-blob, vector-identity-embedder-owned, recovery-rank-correlation | REQ-011, REQ-025c, REQ-040, REQ-044, REQ-051 | `design/vector.md` |
| `projection` | Push-model FTS5 + vector projections; `projection_cursor` allocation + advancement on writer thread; backpressure cooperation with scheduler; projection-status enum | projection-model, projection-freshness-sli, scheduler-shape | REQ-008, REQ-013, REQ-014, REQ-015, REQ-027, REQ-029, REQ-055, REQ-059 | `design/projections.md` |
| `retrieval` | Fixed-stage pipeline (parse → match → fetch → optional expand); FTS5 + vector + hybrid paths; safe FTS5 grammar parser (no raw input passthrough) | retrieval-pipeline-shape, retrieval-latency-gates, text-query-latency-gates | REQ-010, REQ-011, REQ-017, REQ-018, REQ-029, REQ-034 | `design/retrieval.md` |
| `lifecycle` | Phase tags ({Started, Slow, Heartbeat, Finished, Failed}); slow-statement detection at configurable threshold; host-subscriber routing; SQLite-internal event surfacing; cumulative counters; per-statement profile records | (no dedicated ADR — REQ-derived) | REQ-001..REQ-005, REQ-006a/b, REQ-007, REQ-003 (counter shape) | `design/lifecycle.md` |
| `recovery` | `fathomdb doctor` recovery verbs (CLI-only — unreachable from runtime SDK); `check-integrity` aggregator; physical recovery from canonical state; `safe_export` + SHA-256 manifest; safe-export latency target | (no dedicated ADR — REQ-derived) | REQ-012, REQ-024, REQ-025a/b/c, REQ-026, REQ-035, REQ-036, REQ-037, REQ-038, REQ-039, REQ-040, REQ-054, REQ-059 | `design/recovery.md` |
| `bindings facade` | Binding-side surface mapping (Rust = sync engine API; Python = sync, snake_case; TS = Promise, camelCase, idiomatic; CLI = typed verbs); error-mapping per binding; soft-fallback signal field-naming; drain verb name; cursor field on read-tx + write-commit return | python-api-shape, typescript-api-shape, async-surface (Path 1 + Path 2), cli-scope, no-shims-policy, deprecation-policy-0-5-names, prepared-write-shape | REQ-029, REQ-030, REQ-042, REQ-046a/b, REQ-053, REQ-055, REQ-056 | `design/bindings.md` (load-bearing — owns error-mapping + path-1/path-2 cross-binding contracts) |
| `release` | CI / publish artifacts: version-consistency check, atomic multi-registry publish, registry-installed wheel smoke, sibling co-tagging | tier1-ci-platforms | REQ-047, REQ-048, REQ-049, REQ-050, REQ-052 | `design/release.md` |

## 3. Write path

ASCII data flow for a single application write through to vector
projection visibility.

```
Caller (Rust / Python / TS / CLI)
  engine.write([PreparedWrite, ...])
  - Python: PyO3 wrapper releases GIL via Python::allow_threads
  - TS:     napi-rs ThreadsafeFunction → engine-owned Rust pool
            sized at num_cpus::get() per ADR-async-surface (Path 2)
                              │
                              ▼
(1) bindings facade (module)
    - Marshal binding-native types → typed PreparedWrite variants
      (Node / Edge / OpStore / AdminSchema)
    - If batch contains an OpStoreInsert with schema_id, validate
      JSON payload against operator-registered schema
      (json-schema-policy)
                              │ &[PreparedWrite] over mpsc
                              ▼
(2) writer (single dedicated OS thread)
    - Owns the ONLY writer rusqlite connection
      (single-writer-thread; hybrid lock per ADR-0.6.0-database-lock-mechanism:
       sidecar flock + PRAGMA locking_mode=EXCLUSIVE on writer in WAL)
    - synchronous=NORMAL + WAL (durability-fsync-policy)
    - Begin tx → insert canonical rows + matching FTS5 rows
                + op-store rows (when batch carries OpStoreInsert)
                IN THE SAME TX
    - Allocate write-commit cursor c_w (monotonic)
    - Enqueue projection-job rows for vector-indexed kinds
      (projection_cursor for the vector projection NOT YET advanced —
       advances at step 5)
    - COMMIT
    - Provenance retention: if row count > cap × 1.05, evict oldest
      (provenance-retention)
    - Return WriteReceipt { cursor: c_w, ... } to caller
      (`cursor` here is the write-commit cursor, not the read-side
       `projection_cursor`)
    - INVARIANT (async-surface A): writer lock RELEASED before any
      scheduler dispatch fires
    - Visibility AFTER step 2:
        canonical reads see the row immediately (REQ-013)
        FTS5 reads see the row immediately (REQ-014)
        vector reads do NOT yet see the row (advances at step 5)
                              │ writer commits; mpsc → scheduler
                              ▼
(3) scheduler (tokio runtime; 2 worker threads default)
    - Pulls projection-job rows; spawns one async task per job
      (subject to scheduler-shape concurrency cap — 4-layer
       backpressure prevents unbounded in-flight)
    - Per task:
        a. Pull batch (default B=64) of canonical rows
        b. tokio::task::spawn_blocking → embedder pool
        c. await embedder result
        d. Send vectors to writer via mpsc
    - Tokio worker threads NEVER run embed() directly
      (async-surface B + embedder-protocol Invariant 4)
                              │ spawn_blocking
                              ▼
(4) embedder (engine-owned pool; default num_cpus::get())
    - Synchronous embed() per embedder-protocol
    - No re-entrancy (Invariant C); per-call timeout (Invariant D)
    - Returns logical Vec<Vector> (BLOB encoding owned by `vector`
      module — embedder is identity-aware but encoding-naive per
      vector-identity-embedder-owned)
                              │ logical vectors over mpsc
                              ▼
(5) writer (re-enters for projection-row commit)
    - `vector` module encodes logical vectors → LE-f32 BLOB
      (zerocopy-blob: alignment + byte-length invariants)
    - Begin tx → insert vector rows into vec0 virtual table
      (vector-index-location: same SQLite file)
    - Advance projection_cursor ATOMICALLY with the vector-row commit
      (cursor is a column written in same tx)
    - COMMIT
    - Now: read-tx with cursor >= projection_cursor sees the vector
      projection (REQ-055)
                              │
                              ▼
                         (vector projection visible to readers)
```

**Two cursors, distinct purposes:**
- `c_w` (write-commit cursor; returned at step 2): identifies the
  write transaction; canonical+FTS visible immediately at this cursor.
- `projection_cursor` (advanced at step 5): identifies the latest
  projection-visible cursor. Clients poll read-tx until
  `read_projection_cursor >= c_w` to confirm the vector projection
  has caught up.

Numbered traceability: see § 7.

## 4. Read path

```
Caller
  engine.search(text="...", k=10, mode=Hybrid)
                              │ binding facade marshalling
                              ▼
(1) retrieval parse stage
    - Safe FTS5 grammar parse (REQ-034) — never raw input to FTS5
    - Returns typed Query AST
                              │
                              ▼
(2) reader connection acquired
    - From multi-connection pool (REQ-018: no serialization)
    - Open read-tx; record projection_cursor for return value
      (REQ-055 surface)
                              │
                              ▼
(3) retrieval match stage (mode-dependent fixed pipeline)
    - text-only: FTS5 MATCH against safe-grammar tokens
    - vector-only / hybrid (vector branch):
          reader thread blocks via mpsc to embedder pool
          embedder.embed(query_text) on engine-owned pool
            (Invariant B: never on reader thread directly)
          embedder result → vec0 ANN search on reader's read-tx conn
    - hybrid: text + vector branches → score fusion
    - Per-branch availability: if one branch fails, soft-fallback
      record returned alongside results (REQ-029)
                              │ candidate row ids
                              ▼
(4) retrieval fetch stage
    - Canonical row fetch by id from same read-tx
                              │
                              ▼
(5) Optional graph-expand (default: off)
    - Stage-augmented latency NOT gated by 0.6.0 perf ADRs
                              │
                              ▼
Return Search { results, projection_cursor: c_r,
                soft_fallback: Option<...> }
```

Numbered traceability: see § 7.

## 5. On-disk layout

Per ADR-0.6.0-vector-index-location, ADR-0.6.0-op-store-same-file,
ADR-0.6.0-zerocopy-blob, ADR-0.6.0-durability-fsync-policy.

```
<db-name>.sqlite           ← single file: app + op-store + vec0 shadow
<db-name>.sqlite-wal       ← SQLite WAL (synchronous=NORMAL)
<db-name>.sqlite.lock      ← sidecar advisory lock (BSD flock per OFD;
                              ADR-database-lock-mechanism #30)
<db-name>.sqlite-journal   ← optional: only if mode flips off WAL
                              (not in 0.6.0 default)
                            (no `-shm` under WAL+EXCLUSIVE writer per
                              ADR-database-lock-mechanism)
```

**Lock mechanism (per ADR-0.6.0-database-lock-mechanism, accepted
2026-04-29 — overrides earlier "no sidecar" assertion):** Hybrid lock:
sidecar `{database_path}.lock` flock (Rust std `File::try_lock`,
per-OFD exclusion semantics) PLUS `PRAGMA locking_mode=EXCLUSIVE` on
the writer connection in WAL. Sidecar = pre-open fail-fast +
operator-diagnostic PID; SQLite EXCLUSIVE on writer = same-process
backstop + removes `-shm`. Reader connections use NORMAL locking_mode
(REQ-018 multi-reader concurrency preserved). REQ-022a/b satisfied;
locked-DB second open surfaces as typed `DatabaseLocked { holder_pid:
Option<u32> }`. Acquisition invariants Inv-Lock-1..4 in the ADR;
runtime open-path step ordering owned by `design/engine.md`.

**Embedder model files:** caller-supplied path; default-embedder per
ADR-0.6.0-default-embedder loads from a path the operator provides
(via FU-EMB5 model-resolver design). Model files live under
`design/embedder.md`-named cache root, NOT under `<db-name>` — the DB
file is the entire database, the model is operator infrastructure.

**`safe_export` artifacts:** `<export-name>.fathomdb-export` +
`<export-name>.fathomdb-export.sha256` (manifest) per REQ-035 +
AC-039a/b. Layout owned by `design/recovery.md`.

**No sibling files** for vector / op-store / provenance. All inside
the one `.sqlite` file.

**Tables inside the SQLite file:**
- Canonical: `nodes`, `edges`, `chunks` (subject to `design/engine.md`
  refinement).
- FTS5: `<canonical>_fts` virtual tables (one per FTS5-indexed kind).
- Vector: `vec0` virtual tables (one per vector-indexed kind), plus
  shadow tables `sqlite-vec` emits internally.
- Op-store: `operational_*` prefixed tables (FU-OPS1).
- Provenance: provenance event table (operator-configurable retention
  cap per ADR-0.6.0-provenance-retention).
- Schema: SQLite `PRAGMA user_version` advanced by `fathomdb-schema`
  migrations on `Engine.open` (REQ-042).

**File-level invariants:**
- Tier-1 platform (per ADR-0.6.0-tier1-ci-platforms) is little-endian
  — vec0 BLOBs are LE-f32; CI matrix asserts per
  ADR-0.6.0-zerocopy-blob Z-2.
- `safe_export` produces a SHA-256 manifest alongside the artifact
  (REQ-035, AC-039a/b).

## 6. Process / thread topology

| Thread | Owner | Concurrency | Notes |
|---|---|---|---|
| Writer | engine | 1 (single OS thread) | Owns writer rusqlite conn; never a tokio worker; never calls `block_on` |
| Reader | engine | N (per-thread acquisition from pool) | Multi-connection pool; serves SDK reads; sizing decided in `design/engine.md` |
| Tokio runtime workers | engine | default 2 (`scheduler_runtime_threads`) | Orchestration only; spawn_blocking to embedder pool |
| Embedder dispatch pool | engine | default `num_cpus::get()` (`embedder_pool_size`) | Runs `embed()` only; never tokio workers; never asyncio/V8 main thread |
| napi-rs Rust pool | engine (TS binding only) | default `num_cpus::get()` per ADR-0.6.0-async-surface Path 2 | ThreadsafeFunction handoff; decouples from libuv 4-thread default |
| Caller threads | host | 1+ (Python `asyncio.run_in_executor`, TS Promise consumers, etc.) | Engine API is sync from caller's POV (Rust/Python/CLI) or Promise (TS) |

All engine-internal threads are dropped on `Engine.close()`; the
engine releases the lock before `close()` returns (REQ-020a, AC-022a).

## 7. Component → ADR / REQ matrix

### Runtime ADRs (decision-index #1..#28)

Every accepted ADR with a runtime architectural footprint maps to ≥1
component above:

| ADR | Architectural home |
|---|---|
| async-surface | bindings facade + writer (Invariant A) + embedder (B/C/D) + reader (sync surface) |
| default-embedder | embedder + runtime (warmup at open) |
| sqlite-vec-acceptance | vector |
| operator-config-json-only | bindings facade (config marshal) + op_store (schema validation) |
| typed-write-boundary | bindings facade + writer |
| op-store-same-file | op_store + on-disk layout |
| embedder-protocol | embedder |
| zerocopy-blob | vector |
| no-shims-policy | bindings facade (no legacy_*); writer (no compat verbs) |
| single-writer-thread | writer + runtime (lock) |
| vector-identity-embedder-owned | embedder + vector + writer (boundary validation) |
| durability-fsync-policy | runtime (PRAGMAs at open) + writer (commit semantics) |
| projection-freshness-sli | scheduler + projection (cursor advancement) |
| retrieval-latency-gates | retrieval (vector path) |
| scheduler-shape | scheduler + embedder |
| projection-model | projection + scheduler |
| retrieval-pipeline-shape | retrieval |
| error-taxonomy | errors module + bindings facade error mapping |
| typescript-api-shape | bindings facade (TS) |
| cli-scope | bindings facade (CLI) + recovery |
| write-throughput-sli | writer |
| json-schema-policy | op_store |
| text-query-latency-gates | retrieval (FTS5 path) |
| recovery-rank-correlation | recovery + vector (rebuild semantics) |
| provenance-retention | writer (eviction) + runtime (configurable cap) |
| vector-index-location | vector + on-disk layout |
| prepared-write-shape | bindings facade + writer |
| python-api-shape | bindings facade (Python) |
| deprecation-policy-0-5-names | bindings facade |

### Meta ADRs (no runtime footprint)

These ADRs decide policy / topology / deferrals; they have no runtime
component and are intentionally out of the runtime mapping above.

| ADR | Why no runtime home |
|---|---|
| crate-topology | Decided by this doc § 1; not a runtime component |
| subprocess-bridge-deferral | 0.6.0 ships nothing; deferred to 0.8.0 (FU-WIRE15) |
| tier1-ci-platforms | CI gate; runtime engine has no per-platform code path beyond standard cross-compile |

### REQ coverage

Every REQ in `requirements.md` traces to ≥1 module via § 2:

| REQ | Module(s) |
|---|---|
| REQ-001..REQ-005, REQ-006a/b, REQ-007 | lifecycle |
| REQ-003 (counters — restated for clarity) | lifecycle |
| REQ-008 | projection |
| REQ-009a/b | writer |
| REQ-010 | retrieval (FTS5 path) |
| REQ-011 | retrieval (vector path) |
| REQ-012 | recovery |
| REQ-013 | reader (canonical), writer (commit-shape) |
| REQ-014 | reader (FTS), writer (commit-shape) |
| REQ-015 | scheduler + projection |
| REQ-016 | scheduler (drain semantics) |
| REQ-017 | retrieval |
| REQ-018 | reader |
| REQ-019 | writer |
| REQ-020a/b | runtime |
| REQ-021 | runtime |
| REQ-022a/b | runtime + writer |
| REQ-023 | runtime + scheduler |
| REQ-024 | recovery |
| REQ-025a/b/c | recovery + vector |
| REQ-026 | recovery |
| REQ-027 | writer + projection |
| REQ-028a/b/c | embedder + writer (boundary check) |
| REQ-029 | retrieval + bindings facade (signal field) |
| REQ-030 | scheduler + bindings facade (verb name) |
| REQ-031 | writer + runtime |
| REQ-031b | writer + runtime |
| REQ-031c | runtime |
| REQ-032 | runtime (no listener opened anywhere) |
| REQ-033 | embedder + runtime (no fetch on open) |
| REQ-034 | retrieval (parse stage) |
| REQ-035 | recovery |
| REQ-036, REQ-037, REQ-038, REQ-039, REQ-040 | recovery |
| REQ-041 | runtime + on-disk layout |
| REQ-042, REQ-045 | migrations |
| REQ-043 | runtime (POST check) |
| REQ-044 | runtime (POST check) + embedder + vector |
| REQ-046a/b | bindings facade + release (changelog discipline) |
| REQ-047, REQ-048, REQ-049, REQ-050, REQ-052 | release |
| REQ-051 | runtime (POST check) + vector |
| REQ-053 | bindings facade |
| REQ-054 | recovery + bindings facade (SDK-side absence) |
| REQ-055 | bindings facade (cursor field) + projection (allocation) |
| REQ-056 | errors + bindings facade (mapping) |
| REQ-057, REQ-058 | op_store |
| REQ-059 | projection + recovery + op_store |

No orphan REQs.

## 8. Non-goals (architectural)

- **No subprocess bridge.** Per FU-WIRE15 / ADR-subprocess-bridge-deferral;
  revisit 0.8.0.
- **No MVCC.** Single-writer-thread + WAL is the durability + concurrency
  model; ADR-write-throughput-sli is the forcing function.
- **No external ANN store.** vec0 in same file
  (ADR-vector-index-location).
- **No second SQLite file.** Op-store + vector + canonical share one
  file.
- **No data migration from 0.5.x.** Fresh-DB-only per plan.md non-goals.
- **No internal-types public surface.** `pub(crate)` boundaries inside
  `fathomdb-engine`.
- ~~**No sidecar lock file.** SQLite native locking suffices.~~ Reversed
  2026-04-29 by ADR-0.6.0-database-lock-mechanism (#30) after research
  showed SQLite EXCLUSIVE locking_mode acquires lock on first read (not
  at open) and BSD `flock` semantics are per-OFD. See § 5; the file set
  during operation is `db.sqlite` + `db.sqlite-wal` + `db.sqlite.lock`.

## 9. Open architectural questions

These do not block lock; answered in named follow-on docs.

- **Reader pool sizing + acquisition semantics.** No dedicated ADR;
  `design/engine.md` (reader sub-section) decides. Constraint: REQ-018
  + REQ-013/014. Critic flagged candidate ADR; folded into design doc
  pending forcing function.
- **Provenance event table schema.** `design/engine.md`; retention shape
  settled by ADR-provenance-retention.
- ~~**Op-store transactional API shape.**~~ Resolved in
  `design/engine.md` + `design/op-store.md`: one ordered
  `&[PreparedWrite]` submission, single writer-thread transaction, atomic
  visibility for same-batch primary and op-store rows.
- **Recovery flow detail.** `design/recovery.md` owns; canonical model
  is dbim-playbook 3-class / 4-invariant (folded; HITL F8).
- **Backpressure detail.** `design/scheduler.md` owns the layer-by-layer
  detail of scheduler-shape's 4-layer backpressure.
- ~~**Corruption detection / `Engine.open` behavior.**~~ Resolved
  2026-04-29 by ADR-0.6.0-corruption-open-behavior (#29): refuse-fail-closed
  with structured `EngineOpenError::Corruption`; recovery via
  `fathomdb recover` CLI; detection cadence delegated to
  `design/recovery.md` with anti-regression clause. FU-VEC13-CORRUPTION
  + FU-RECOVERY-CORRUPTION-DETECTION closed.
- ~~**`fathomdb-query` disposition.**~~ Resolved 2026-04-29 (HITL): kept
  separate as pure AST-to-plan compiler. See § 1.

## 10. Architectural deltas vs 0.5.x

(Informational — not load-bearing.)

- **No more raw-SQL leak paths.** Typed-write boundary at engine surface.
- **Single writer thread**, dropped multi-writer-coordination layer.
  Eliminates `SQLITE_SCHEMA` flood class.
- **Vector identity owned by embedder**, not vector config.
- **Op-store in same file**, not a sibling DB.
- **Push projections eager**, not pull-on-read.
- **Five-verb application SDK surface** (REQ-053); recovery moved to
  CLI-only (REQ-037 / REQ-054).
- **No 0.5.x compat shims.**
- **Default embedder ships with engine** (candle + tokenizers via
  `fathomdb-embedder` sibling crate per ADR-0.6.0-default-embedder).
- **Vector index ships with engine** (`sqlite-vec` extension per
  ADR-0.6.0-sqlite-vec-acceptance).
