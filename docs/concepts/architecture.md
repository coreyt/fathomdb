# System Architecture

fathomdb is a local datastore built in Rust with language bindings for Python,
TypeScript, and Go. The engine uses a **single-writer / multi-reader** model
backed by SQLite with WAL (Write-Ahead Logging) for crash recovery and
concurrent access.

## Architecture Overview

```
                        ┌──────────────┐  ┌────────────────┐  ┌─────────────────┐
                        │  Python SDK  │  │ TypeScript SDK  │  │  Go CLI         │
                        │  (maturin)   │  │  (npm package)  │  │ (fathom-        │
                        │              │  │                 │  │  integrity)     │
                        └──────┬───────┘  └───────┬────────┘  └───────┬─────────┘
                               │ PyO3             │ napi-rs           │ JSON stdio
                        ───────┴──────────────────┴───────────────────┴──────────
                                        Rust Bindings Layer
                        ────────────────────────┬───────────────────────────────
                                                │
                                    ┌───────────┴───────────┐
                                    │   crate: fathomdb     │
                                    │  (public API facade)  │
                                    │  Engine · Session ·   │
                                    │  WriteRequestBuilder  │
                                    └───────────┬───────────┘
                                                │
                           ┌────────────────────┼────────────────────┐
                           │                    │                    │
                  ┌────────┴─────────┐ ┌───────┴────────┐ ┌────────┴─────────┐
                  │ fathomdb-engine   │ │ fathomdb-query  │ │ fathomdb-schema  │
                  │                   │ │                 │ │                  │
                  │ EngineRuntime     │ │ QueryAst        │ │ SchemaManager    │
                  │ WriterActor       │ │ compile()       │ │ bootstrap()      │
                  │ ExecCoordinator   │ │ CompiledQuery   │ │ migrations       │
                  │ AdminService      │ │ shape cache     │ │ PRAGMAs          │
                  │ Operational Store │ │                 │ │ sqlite-vec load  │
                  │ Projections       │ │ "narrow-first"  │ │                  │
                  │ Provenance        │ │  strategy       │ │                  │
                  └────────┬──────────┘ └───────┬────────┘ └────────┬─────────┘
                           │                    │                    │
                           └────────────────────┼────────────────────┘
                                                │
                  ┌─────────────────────────────┴──────────────────────────────┐
                  │                     EngineRuntime                          │
                  │  (assembles all components; enforces critical drop order:  │
                  │   readers drop before writer, writer before lock,          │
                  │   ensuring final WAL checkpoint fires)                     │
                  └──────────────┬──────────────────────────┬──────────────────┘
                                 │                          │
                  ┌──────────────┴──────────┐  ┌───────────┴───────────────┐
                  │       WRITE PATH        │  │        READ PATH          │
                  │                         │  │                           │
                  │   WriterActor           │  │  ExecutionCoordinator     │
                  │   (dedicated thread)    │  │                           │
                  │         │               │  │  ┌────┐ ┌────┐ ┌────┐    │
                  │    mpsc channel         │  │  │conn│ │conn│ │conn│    │
                  │         │               │  │  │R/O │ │R/O │ │R/O │    │
                  │         ▼               │  │  └─┬──┘ └─┬──┘ └─┬──┘    │
                  │  Validate & Supersede   │  │    └──────┼──────┘       │
                  │  (upsert logic)         │  │    try_lock() fast path  │
                  │         │               │  │    blocking fallback     │
                  │  Exclusive Transaction  │  │                           │
                  │  (1 writer connection)  │  │                           │
                  │         │               │  │                           │
                  │    WriteReceipt         │  │                           │
                  └─────────┬───────────────┘  └───────────┬───────────────┘
                            │                              │
                            ▼                              ▼
                  ┌────────────────────────────────────────────────────────────┐
                  │                     SQLite + WAL                          │
                  │                                                           │
                  │  Canonical:  nodes · edges · chunks · actions/runs/steps  │
                  │  Derived:    FTS5 index · sqlite-vec (vector search)      │
                  │  Operational: operational_mutations (append-only log)     │
                  │               operational_current (materialized view)     │
                  │  Audit:      provenance_events                     │
                  │                                                           │
                  │  {db_path}.lock  ← exclusive file lock (1 engine / file) │
                  └────────────────────────────────────────────────────────────┘
```

## Rust Crates

The Rust workspace contains four crates:

| Crate | Role |
|---|---|
| **`fathomdb`** | Public API facade. Exports `Engine`, `Session`, and `WriteRequestBuilder`. Conditionally compiles Python (PyO3) and Node.js (napi-rs) bindings via feature flags. |
| **`fathomdb-engine`** | Core engine. Implements the `WriterActor`, `ExecutionCoordinator`, `AdminService`, operational store, projections, and provenance tracking. |
| **`fathomdb-query`** | Query compiler. Builds `QueryAst`, compiles to SQL with bind parameters, and chooses execution plans using a "narrow-first" strategy. |
| **`fathomdb-schema`** | SQLite schema management. Handles bootstrap, migrations, connection initialization (PRAGMAs), and sqlite-vec extension loading. |

## Single-Writer / Multi-Reader Model

The engine enforces a strict concurrency model:

### Write Path

1. A single **`WriterActor`** runs on a dedicated background thread.
2. Write requests arrive via an **mpsc channel**.
3. The actor validates provenance, applies upsert/supersession logic, and executes
   an **exclusive SQLite transaction** through one writer connection.
4. Callers receive a **`WriteReceipt`** with warnings and provenance warnings.

Only one `Engine` instance may be open per database file at a time, enforced by an
exclusive file lock on `{db_path}.lock`.

### Read Path

1. The **`ExecutionCoordinator`** manages a pool of read-only SQLite connections
   (default 4, configurable).
2. Each connection is wrapped in its own `Mutex`. Acquisition uses `try_lock()`
   for a fast path with a blocking fallback.
3. Compiled queries (with shape-hash caching) execute concurrently without blocking
   writes.

### Drop Order

`EngineRuntime` enforces a critical drop sequence:

1. **Readers drop first** -- all read-only connections close.
2. **Writer drops** -- the writer connection closes, triggering a final WAL checkpoint.
3. **Lock drops** -- the file lock releases.

This ordering guarantees that the WAL is checkpointed cleanly on shutdown.

## Language Bindings

All three language SDKs wrap the same Rust engine:

| Language | Binding | Mechanism |
|---|---|---|
| **Python** | PyO3 via maturin | `EngineCore` wraps `Engine` in `RwLock<Option<Engine>>`. All operations release the GIL via `py.allow_threads()`. Rust errors map to typed Python exceptions. |
| **TypeScript** | napi-rs | `NodeEngineCore` wraps `Engine` similarly. All input/output serialized as JSON. Prebuilt `.node` binaries per platform. |
| **Go** | JSON stdio bridge | `fathom-integrity` CLI spawns a Rust subprocess. Commands and results exchanged as JSON over stdout/stderr. Used for operator tooling (recovery, repair, export). |

Cross-language tests in `tests/cross-language/` verify that the Python and TypeScript
SDKs produce identical database state and can read each other's writes.

## Storage Layout

The engine stores everything in a single SQLite database file with WAL enabled.

### Canonical Tables

These are the source of truth:

- **`nodes`** -- primary data records with `kind`, `properties`, `logical_id`, `row_id`
- **`edges`** -- relationships between nodes
- **`chunks`** -- text segments associated with nodes, used as input to projections
- **`runs` / `steps` / `actions`** -- agent execution history

### Derived Projections

These are rebuilt deterministically from canonical state:

- **FTS5 index** -- full-text search over chunks
- **sqlite-vec index** -- vector similarity search (configurable dimensions)

Because projections are derived, they can be rebuilt from scratch via admin
operations (`rebuild_projections`, `rebuild_missing_projections`) without data loss.

### Operational Store

An append-only mutation log for application-managed collections:

- **`operational_mutations`** -- every mutation recorded with timestamp, record key,
  payload, and mutation type
- **`operational_current`** -- materialized view of the latest state per record key
- Supports validation contracts, retention policies, and secondary indexes

### Provenance & Audit

- **`provenance_events`** -- every write records its `source_ref` for
  traceability
- **`trace_source()`** -- find all data written by a given source
- **`excise_source()`** -- surgically remove all data from a given source

## Query Compilation

The query compiler follows a "narrow-first" strategy:

1. **Build** -- user constructs a `QueryAst` via the fluent `QueryBuilder` API.
2. **Compile** -- `compile_query()` produces a `CompiledQuery` with SQL, bind
   parameters, and a shape hash.
3. **Plan** -- start from the narrowest indexed candidate set (vector or FTS),
   resolve through chunks, join to canonical graph state, apply late filtering.
4. **Cache** -- shape hashes key into the executor's shape cache (max 4096 entries)
   to avoid recompilation of repeated query shapes.
5. **Execute** -- the `ExecutionCoordinator` runs the compiled SQL on a pooled
   read-only connection and returns `QueryRows`.
