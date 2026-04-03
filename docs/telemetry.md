# Telemetry

fathomdb collects resource-usage telemetry at configurable levels. All
telemetry is in-process — no external services or network calls.

## Telemetry Levels

Levels are additive. Each level includes everything from below it.

### Level 0: Counters (default, always on)

Cumulative `AtomicU64` counters that increment as work happens. Near-zero
overhead (~1-2ns per increment).

| Counter | What it counts |
|---|---|
| `queries_total` | Read operations executed |
| `writes_total` | Write operations committed |
| `write_rows_total` | Total rows written (nodes + edges + chunks) |
| `errors_total` | Operation errors |
| `admin_ops_total` | Admin operations (integrity checks, exports, rebuilds, etc.) |

SQLite page-cache counters (aggregated across the reader pool):

| Counter | What it counts |
|---|---|
| `cache_hits` | Page cache hits |
| `cache_misses` | Page cache misses |
| `cache_writes` | Pages written to cache |
| `cache_spills` | Cache pages spilled to disk |

### Level 1: Statements (future)

Per-statement profiling: wall-clock time, VM steps, full-scan steps, sort
operations, and per-statement cache hit/miss deltas. Approximately 30-50ns
overhead per statement.

### Level 2: Profiling (future)

Deep profiling including SQLite scan-status counters (requires
high-telemetry build) and periodic process snapshots (CPU time, RSS, disk
I/O via `getrusage` and `/proc/self/io`).

## Configuration

### Rust

```rust
use fathomdb::{Engine, EngineOptions, TelemetryLevel};

let mut options = EngineOptions::new("agent.db");
options.telemetry_level = TelemetryLevel::Counters; // default

let engine = Engine::open(options)?;
```

The telemetry level is set at engine open and cannot be changed afterwards.

### Python

```python
import fathomdb

engine = fathomdb.Engine.open(
    "agent.db",
    provenance_mode="warn",
    telemetry_level="counters",  # default; also "statements", "profiling"
)
```

## Reading Telemetry

### Rust

```rust
let snapshot = engine.telemetry_snapshot();
println!("queries: {}", snapshot.queries_total);
println!("cache hit ratio: {:.2}",
    snapshot.sqlite_cache.cache_hits as f64
    / (snapshot.sqlite_cache.cache_hits + snapshot.sqlite_cache.cache_misses).max(1) as f64
);
```

### Python

```python
snap = engine.telemetry_snapshot()
print(f"queries: {snap['queries_total']}")
print(f"cache hits: {snap['cache_hits']}")
```

The Python method returns a flat dict with keys: `queries_total`,
`writes_total`, `write_rows_total`, `errors_total`, `admin_ops_total`,
`cache_hits`, `cache_misses`, `cache_writes`, `cache_spills`.

## High-Telemetry Build (Level 2)

Level 2 scan-status counters require compiling SQLite with
`SQLITE_ENABLE_STMT_SCANSTATUS`. This is done via the `LIBSQLITE3_FLAGS`
environment variable:

```bash
LIBSQLITE3_FLAGS="SQLITE_ENABLE_STMT_SCANSTATUS" \
  cargo build --features high-telemetry
```

The `high-telemetry` Cargo feature gates the Rust code that calls
`sqlite3_stmt_scanstatus()`. If the feature is enabled but SQLite was not
compiled with the flag, the engine detects this at runtime and falls back
to Level 1 behavior.

## Design

See `dev/design-note-telemetry-and-profiling.md` for the full design
including allocator evaluation (jemalloc vs mimalloc) and custom allocator
risk assessment.
