# Telemetry

fathomdb collects resource-usage telemetry at configurable levels. All
telemetry is in-process — no external services, network calls, or
background threads (at Level 0).

## Telemetry Levels

Levels are additive. Each level includes everything from below it. The
level is set at engine open and cannot be changed afterwards.

### Level 0: Counters (default, always on)

Cumulative counters that increment as work happens. Near-zero overhead
(~1-2ns per increment via atomic operations).

**Application counters:**

| Counter | What it counts |
|---|---|
| `queries_total` | Read operations executed |
| `writes_total` | Write operations committed |
| `write_rows_total` | Total rows written (nodes + edges + chunks) |
| `errors_total` | Operation errors (query failures, write rejections, panics) |
| `admin_ops_total` | Admin operations (integrity checks, exports, rebuilds, etc.) |

**SQLite page-cache counters** (aggregated across the reader pool):

| Counter | What it counts |
|---|---|
| `cache_hits` | Page cache hits |
| `cache_misses` | Page cache misses (required disk read) |
| `cache_writes` | Pages written to cache |
| `cache_spills` | Cache pages spilled to disk under memory pressure |

The cache hit ratio (`cache_hits / (cache_hits + cache_misses)`) indicates
how well the working set fits in memory. A ratio below 0.90 on a
steady-state workload suggests the database is larger than available RAM.

### Level 1: Statements (future)

Per-statement profiling: wall-clock time, VM steps, full-scan steps, sort
operations, and per-statement cache hit/miss deltas. Approximately 30-50ns
overhead per statement.

### Level 2: Profiling (future)

Deep profiling including SQLite scan-status counters (requires
high-telemetry build) and periodic process snapshots (CPU time, RSS, disk
I/O via `getrusage` and `/proc/self/io`).

---

## Python SDK

### Configuration

```python
from fathomdb import Engine, TelemetryLevel

# Default — counters only (always on, near-zero overhead)
engine = Engine.open("agent.db")

# Explicit level via enum
engine = Engine.open("agent.db", telemetry_level=TelemetryLevel.COUNTERS)

# Or via string
engine = Engine.open("agent.db", telemetry_level="counters")
```

Valid values: `"counters"` (default), `"statements"`, `"profiling"`.

### Reading Telemetry

`Engine.telemetry_snapshot()` returns a `TelemetrySnapshot` dataclass with
all current counter values:

```python
snap = engine.telemetry_snapshot()

# Operation counters
print(f"queries: {snap.queries_total}")
print(f"writes: {snap.writes_total}")
print(f"rows written: {snap.write_rows_total}")
print(f"errors: {snap.errors_total}")
print(f"admin ops: {snap.admin_ops_total}")

# SQLite cache efficiency
total = snap.cache_hits + snap.cache_misses
if total > 0:
    print(f"cache hit ratio: {snap.cache_hits / total:.2%}")
```

### TelemetrySnapshot Fields

| Field | Type | Description |
|---|---|---|
| `queries_total` | `int` | Cumulative read operations |
| `writes_total` | `int` | Cumulative write operations |
| `write_rows_total` | `int` | Cumulative rows written (nodes + edges + chunks) |
| `errors_total` | `int` | Cumulative operation errors |
| `admin_ops_total` | `int` | Cumulative admin operations |
| `cache_hits` | `int` | SQLite page cache hits (summed across reader pool) |
| `cache_misses` | `int` | SQLite page cache misses |
| `cache_writes` | `int` | Pages written to cache |
| `cache_spills` | `int` | Cache pages spilled to disk |

All counters are cumulative since engine open. They never reset or
decrease (except by closing and reopening the engine).

### Periodic Collection Example

```python
import time
import threading
from fathomdb import Engine

engine = Engine.open("agent.db")

def collect_metrics():
    while True:
        snap = engine.telemetry_snapshot()
        # Forward to your metrics system (Prometheus, StatsD, logging, etc.)
        print({
            "queries": snap.queries_total,
            "writes": snap.writes_total,
            "errors": snap.errors_total,
            "cache_hit_ratio": (
                snap.cache_hits / max(snap.cache_hits + snap.cache_misses, 1)
            ),
        })
        time.sleep(10)

collector = threading.Thread(target=collect_metrics, daemon=True)
collector.start()
```

### Computing Rates

Since counters are cumulative, compute rates by taking deltas between
snapshots:

```python
import time
from fathomdb import Engine

engine = Engine.open("agent.db")

prev = engine.telemetry_snapshot()
time.sleep(interval_seconds)
curr = engine.telemetry_snapshot()

queries_per_sec = (curr.queries_total - prev.queries_total) / interval_seconds
writes_per_sec = (curr.writes_total - prev.writes_total) / interval_seconds
```

### Thread Safety

`telemetry_snapshot()` is safe to call from any thread at any time. Counter
reads use relaxed atomics — values are eventually consistent across
threads, not instantaneously synchronized. For monitoring purposes this is
sufficient; for exact point-in-time snapshots under concurrent load, small
discrepancies between counters (e.g., `writes_total` incrementing slightly
before `write_rows_total`) are expected and harmless.

### Integration with Response-Cycle Feedback

Telemetry counters and response-cycle feedback are complementary systems:

| Concern | Telemetry | Feedback |
|---|---|---|
| Question answered | "How many operations have run?" | "Is this operation still alive?" |
| Granularity | Cumulative totals | Per-operation lifecycle events |
| Consumer | Monitoring, dashboards, capacity planning | Application UX, progress indicators |
| Overhead | ~1-2ns per operation | Timer thread per active operation |

Both can be used together — telemetry for aggregate monitoring, feedback
for per-operation progress:

```python
from fathomdb import Engine, FeedbackConfig

def on_progress(event):
    print(f"[{event.phase.value}] {event.operation_kind} ({event.elapsed_ms}ms)")

engine = Engine.open("agent.db")

# Use feedback for individual operations
rows = engine.nodes("Document").limit(10).execute(progress_callback=on_progress)

# Use telemetry for aggregate monitoring
snap = engine.telemetry_snapshot()
print(f"total queries so far: {snap.queries_total}")
```

---

## Rust SDK

### Configuration

```rust
use fathomdb::{Engine, EngineOptions, TelemetryLevel};

let mut options = EngineOptions::new("agent.db");
options.telemetry_level = TelemetryLevel::Counters; // default

let engine = Engine::open(options)?;
```

### Reading Telemetry

```rust
let snapshot = engine.telemetry_snapshot();
println!("queries: {}", snapshot.queries_total);
println!("writes: {}", snapshot.writes_total);
println!("cache hit ratio: {:.2}",
    snapshot.sqlite_cache.cache_hits as f64
    / (snapshot.sqlite_cache.cache_hits + snapshot.sqlite_cache.cache_misses).max(1) as f64
);
```

The `TelemetrySnapshot` struct contains:
- `queries_total`, `writes_total`, `write_rows_total`, `errors_total`,
  `admin_ops_total` — all `u64`
- `sqlite_cache: SqliteCacheStatus` — with `cache_hits`, `cache_misses`,
  `cache_writes`, `cache_spills` as `i64`

---

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
