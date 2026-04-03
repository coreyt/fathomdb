# Design Note: Resource Telemetry and Profiling

## Status

Phase 1 (Level 0 Counters) implemented — 2026-04-03. This document
specifies the resource-usage collection and reporting system for fathomdb.

## Relationship to Existing Systems

fathomdb already has two observability layers:

| Layer | What it does | Where it lives |
|---|---|---|
| **Response-cycle feedback** | Lifecycle events (started/slow/heartbeat/finished/failed) for public API operations | `feedback.rs` |
| **Structured tracing** | Diagnostic events with context (spans, fields, timing) for internal implementation | `trace_support.rs`, feature-gated behind `tracing` |

This design adds a third layer: **resource telemetry** — quantitative
measurements of what the engine consumed while doing its work. Feedback tells
you *how long* an operation took. Tracing tells you *where* it spent time.
Telemetry tells you *what it cost* — VM steps, cache misses, page reads, CPU
time, memory.

These three layers serve different consumers and must remain separate:

| Concern | Consumer | Question answered |
|---|---|---|
| Feedback | Application code, UX | "Is this operation still alive?" |
| Tracing | Developer debugging | "Why is this operation slow?" |
| Telemetry | Operator, capacity planning, query tuning | "What resources did this operation consume?" |

## Collection Levels

Resource telemetry is organized into three additive levels. Each level
includes everything from the level below it. The levels are defined by
cost, not by when collection happens — collection timing is a consequence
of the data source, not a knob.

### Level 0: Counters (always on, not optional)

Cumulative counters that increment as work happens. These are so cheap that
disabling them would save nothing measurable.

**Application-side counters** (AtomicU64, ~1-2ns per increment):

| Counter | What it counts |
|---|---|
| `queries_total` | Total read operations executed |
| `writes_total` | Total write operations committed |
| `write_rows_total` | Total rows written (nodes + edges + chunks) |
| `errors_total` | Total operation errors, by error code |
| `admin_ops_total` | Total admin operations, by operation type |

**SQLite counters** (always maintained internally, zero cost to read):

| Counter | SQLite API | What it counts |
|---|---|---|
| `cache_hits` | `sqlite3_db_status(CACHE_HIT)` | Page cache hits |
| `cache_misses` | `sqlite3_db_status(CACHE_MISS)` | Page cache misses |
| `cache_writes` | `sqlite3_db_status(CACHE_WRITE)` | Pages written to cache |
| `cache_spills` | `sqlite3_db_status(CACHE_SPILL)` | Cache pages spilled to disk |

SQLite counters are read on-demand or periodically (not per-operation),
since the values are cumulative per-connection.

**When to read**: On demand (e.g. status API call), at engine close, or on
a periodic timer if interval reporting is configured. Not per-query.

### Level 1: Statement Profiling (opt-in, low overhead)

Per-statement measurements read after each statement executes. The overhead
is a trace callback invocation plus a few integer reads — roughly 30-50ns
per statement.

**Per-statement metrics**:

| Metric | Source | What it measures |
|---|---|---|
| Wall-clock time (ns) | `sqlite3_trace_v2(PROFILE)` | Elapsed time for statement execution |
| VM steps | `sqlite3_stmt_status(VM_STEP)` | VDBE virtual machine operations (~query cost proxy) |
| Full-scan steps | `sqlite3_stmt_status(FULLSCAN_STEP)` | Rows examined without an index |
| Sort operations | `sqlite3_stmt_status(SORT)` | Number of sort operations |
| Autoindex creations | `sqlite3_stmt_status(AUTOINDEX)` | Temporary indexes created |
| Bloom filter hits/misses | `sqlite3_stmt_status(FILTER_HIT/MISS)` | Bloom filter effectiveness |
| Cache delta | `sqlite3_db_status(CACHE_HIT/MISS)` before/after | Cache efficiency for this specific statement |

**Aggregation**: Per-statement metrics can be reported individually (for
slow-query logging) or aggregated into histograms/summaries (for
dashboards). The collection layer captures per-statement; the reporting
layer decides granularity.

**Relationship to trace_v2**: The engine already installs
`sqlite3_trace_v2(PROFILE)` in debug builds (`sqlite.rs:44-78`). Level 1
extends this to release builds, behind a runtime configuration rather than
a compile-time `debug_assertions` gate. The existing callback emits a
tracing TRACE event; the telemetry system would additionally record the
value in a counter/histogram structure.

**Slow statement detection**: A statement whose wall-clock time exceeds a
configurable threshold (default: 100ms) or whose `FULLSCAN_STEP` count is
nonzero is flagged. This integrates naturally with the existing feedback
system — a slow statement contributes evidence for the "slow" phase
transition.

### Level 2: Deep Profiling (opt-in, requires high-telemetry build)

Everything in Level 1, plus data sources that require either a different
SQLite build or nontrivial per-call overhead.

**Scan status (compile-time opt-in)**:

Requires `SQLITE_ENABLE_STMT_SCANSTATUS` at SQLite compile time. Provides
per-scan-loop cost breakdown within a statement:

| Metric | SQLite API | What it measures |
|---|---|---|
| Loop count | `sqlite3_stmt_scanstatus(NLOOP)` | Times each scan loop executed |
| Rows visited | `sqlite3_stmt_scanstatus(NVISIT)` | Rows examined per scan loop |
| Estimated rows | `sqlite3_stmt_scanstatus(EST)` | Query planner's row estimate |
| Scan name | `sqlite3_stmt_scanstatus(NAME)` | Index or table name for each scan |
| Select ID | `sqlite3_stmt_scanstatus(SELECTID)` | Which sub-select this scan belongs to |

This is the only telemetry feature that adds overhead when *not read* —
the counters increment on every scan-loop iteration. The overhead is small
(one integer increment per iteration) but nonzero, which is why it requires
an explicit build variant.

**Process-level snapshots (periodic, microsecond-level cost)**:

| Metric | Source | What it measures |
|---|---|---|
| User CPU time | `libc::getrusage(RUSAGE_SELF)` | CPU time in user mode |
| System CPU time | `libc::getrusage(RUSAGE_SELF)` | CPU time in kernel mode |
| Max RSS | `libc::getrusage(RUSAGE_SELF)` | Peak resident set size |
| Voluntary context switches | `libc::getrusage(RUSAGE_SELF)` | Yielded CPU (I/O waits) |
| Involuntary context switches | `libc::getrusage(RUSAGE_SELF)` | Preempted by scheduler |
| Disk read bytes | `/proc/self/io` | Actual block device reads |
| Disk write bytes | `/proc/self/io` | Actual block device writes |
| Read syscalls | `/proc/self/io` | Number of read(2) calls |
| Write syscalls | `/proc/self/io` | Number of write(2) calls |

Sampled on a periodic timer (default: every 10 seconds). Cost per sample is
~200ns for `getrusage` plus ~2-5us for `/proc/self/io` parsing. Not
per-query.

**SQLite memory snapshots** (on the same periodic timer):

| Metric | SQLite API | What it measures |
|---|---|---|
| Total memory | `sqlite3_status(MEMORY_USED)` | Bytes allocated via sqlite3_malloc |
| Malloc count | `sqlite3_status(MALLOC_COUNT)` | Outstanding allocations |
| Page cache used | `sqlite3_db_status(CACHE_USED)` | Pager cache memory per connection |
| Statement memory | `sqlite3_db_status(STMT_USED)` | Memory held by prepared statements |
| Schema memory | `sqlite3_db_status(SCHEMA_USED)` | Memory for loaded schema |

## High-Telemetry Build

Level 2's scan status feature requires compiling the bundled SQLite with
`SQLITE_ENABLE_STMT_SCANSTATUS`. The `libsqlite3-sys` build script
supports this via the `LIBSQLITE3_FLAGS` environment variable.

### Build mechanism

The `libsqlite3-sys` crate (which `rusqlite` depends on with the `bundled`
feature) reads `LIBSQLITE3_FLAGS` at compile time and passes each flag to
the C compiler:

```
LIBSQLITE3_FLAGS="SQLITE_ENABLE_STMT_SCANSTATUS" cargo build
```

This is the correct mechanism — no `build.rs` in fathomdb is needed.

### Cargo feature

A `high-telemetry` feature in `fathomdb-engine` controls whether the Rust
code attempts to call `sqlite3_stmt_scanstatus()`:

```toml
# crates/fathomdb-engine/Cargo.toml
[features]
high-telemetry = []
```

The feature gates the Rust call sites with `#[cfg(feature = "high-telemetry")]`.
It does **not** add the SQLite compile flag — that is an environment variable
handled by `libsqlite3-sys`. The feature and the environment variable must
be used together:

```bash
LIBSQLITE3_FLAGS="SQLITE_ENABLE_STMT_SCANSTATUS" \
  cargo build --features high-telemetry
```

If the feature is enabled without the compile flag, the FFI call will return
`SQLITE_ERROR` (the function exists but is a no-op without the compile flag).
The telemetry layer handles this gracefully — it detects the error on first
call and disables scan-status collection for the session with a warning.

### Runtime detection

At engine open, when `high-telemetry` is enabled, the telemetry system
prepares a dummy statement and calls `sqlite3_stmt_scanstatus()`. If it
returns data, scan-status collection is active. If it errors, the system
logs a warning via tracing and falls back to Level 1 behavior. This means
a `high-telemetry` binary can run against a standard SQLite build without
crashing — it just collects less data.

## Reporting

Collection and reporting are separate concerns. The telemetry system
collects data into internal structures; reporting determines how and when
that data is exposed.

### Reporting surfaces

| Surface | What it exposes | When |
|---|---|---|
| **On-demand snapshot** | Current counter values, cache ratios, memory usage | Called by application code (e.g. health check endpoint) |
| **Statement report** | Per-statement metrics for the most recent N statements, or statements exceeding thresholds | Called by application code or emitted via feedback metadata |
| **Periodic dump** | All counters and snapshots as a structured record | On a configurable timer, emitted via tracing at INFO level |
| **Engine close summary** | Cumulative totals for the engine's lifetime | At `EngineRuntime::drop` |
| **Feedback metadata** | Selected telemetry values attached to feedback events | Automatically, when Level 1+ is active |

### Feedback integration

When Level 1 is active, the `Finished` feedback event for a write or query
operation can include telemetry metadata:

```
metadata: {
  "vm_steps": "14523",
  "wall_clock_us": "3200",
  "cache_hit_ratio": "0.94",
  "fullscan_steps": "0"
}
```

This enriches the existing feedback system without changing its contract —
metadata keys are informational and not part of the stable API.

## Data Structures

### Counter storage

Level 0 counters are stored as `AtomicU64` fields in a `TelemetryCounters`
struct, allocated once at engine open and shared (via `Arc`) across all
reader connections and the writer thread:

```rust
pub struct TelemetryCounters {
    pub queries_total: AtomicU64,
    pub writes_total: AtomicU64,
    pub write_rows_total: AtomicU64,
    pub errors_total: AtomicU64,
    pub admin_ops_total: AtomicU64,
}
```

All increments use `Ordering::Relaxed` — these are statistical counters,
not synchronization primitives. Exact cross-thread consistency is not
required.

### Statement metrics buffer

Level 1 per-statement metrics are collected into a bounded ring buffer.
The buffer holds the most recent N statement records (default: 1024).
Older records are silently dropped. The buffer is per-connection (not
shared), since `stmt_status` is per-prepared-statement.

No external crate is required — a fixed-size array with a write cursor is
sufficient.

### Process snapshots

Level 2 periodic snapshots are stored as a small struct with the most
recent sample and a few derived rates (e.g. CPU utilization since last
sample, I/O bytes/sec). Only the most recent snapshot is retained — this
is not a time-series store.

## Process and Runtime Telemetry

Beyond SQLite-specific metrics, the engine should collect process-level
resource usage. Rust has no runtime introspection equivalent to Go's
`runtime.ReadMemStats` or Java's JMX — process telemetry is composed from
OS interfaces and allocator APIs.

### Memory

Rust has no garbage collector, so the concerns are different from GC'd
languages:

| GC Language Concern | Rust Equivalent | How to Observe |
|---|---|---|
| GC pause time | **N/A** — no GC, no pauses | — |
| Heap occupancy | Allocator stats or RSS | jemalloc-ctl or procfs |
| Object count by type | **Nothing built-in** | Offline tooling (DHAT) |
| **Fragmentation** | Primary concern for long-running Rust | `allocated/active` ratio in jemalloc |
| **Capacity bloat** | Collections that grew large once keep capacity | Application-level tracking |
| **Leaked Arc cycles** | `Rc`/`Arc` cycles leak forever (no cycle collector) | `Arc::strong_count()` on key objects |

#### Process memory (no crate needed)

| Metric | Linux | macOS | Source |
|---|---|---|---|
| RSS (resident set) | `/proc/self/statm` field 2 | `mach_task_basic_info` | ~1-5us |
| Peak RSS | `/proc/self/status` `VmHWM` | `getrusage().ru_maxrss` | ~1-5us |
| Virtual size | `/proc/self/statm` field 1 | Same `task_info` | ~1-5us |

#### Allocator stats

The system allocator (default) exposes nothing. To get memory
introspection, fathomdb should use jemalloc:

| Metric | jemalloc API | Meaning |
|---|---|---|
| `stats::allocated` | Total bytes in active allocations | "Heap in use" |
| `stats::active` | Pages actively backing allocations | allocated + internal fragmentation |
| `stats::resident` | RSS attributable to allocator | What the OS has mapped |
| `stats::retained` | Bytes not returned to OS | Potential to reclaim |
| `stats::metadata` | Allocator bookkeeping | Overhead |
| `thread::allocatedp` | Per-thread cumulative allocations (TLS) | Allocation rate per thread |

Fragmentation ratio: `allocated / active`. Below 0.8 is concerning.

**Dependency**: `tikv-jemallocator` + `tikv-jemalloc-ctl`. jemalloc is
often faster than the system allocator, so this is not a performance
tradeoff — it is likely a net improvement. Reading stats requires calling
`epoch::advance()` first (~1-5us), then reading fields (atomic loads,
near-zero).

**Alternative — counting allocator wrapper** (no dependency):

A 15-line `GlobalAlloc` wrapper around `System` that increments
`AtomicUsize` counters on `alloc`/`dealloc` gives allocation count and
live bytes with ~5-15ns overhead per allocation. This is useful with or
without jemalloc. Limitation: `layout.size()` is the requested size, not
the actual allocated size.

#### glibc malloc stats (Linux-only fallback)

If jemalloc is not used, `libc::mallinfo2()` on glibc gives `uordblks`
(in-use bytes), `fordblks` (free bytes), and `hblkhd` (mmap'd bytes).
Not available on musl (Alpine/static builds).

### CPU

All via `libc::getrusage()` (~200ns) or procfs (~2us):

| Metric | Source | Meaning |
|---|---|---|
| User CPU time | `getrusage().ru_utime` | CPU spent in user code |
| System CPU time | `getrusage().ru_stime` | CPU spent in kernel |
| Voluntary context switches | `getrusage().ru_nvcsw` | Thread yielded CPU (I/O wait, lock contention) |
| Involuntary context switches | `getrusage().ru_nivcsw` | Thread preempted (CPU oversubscribed) |
| Per-thread CPU time | `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` | Nanosecond-resolution per-thread CPU |

**Interpretation for fathomdb**: High voluntary context switches on the
writer thread suggest lock contention or I/O stalls during WAL commits.
High involuntary switches across reader threads suggest the read pool
is sized larger than available CPU cores.

### File Descriptors

| Metric | Source | Cost |
|---|---|---|
| Open FD count | `readdir("/proc/self/fd")` | ~5-50us depending on count |
| FD soft limit | `getrlimit(RLIMIT_NOFILE)` | ~100ns |
| FD hard limit | Same | Same |
| Per-FD target | `readlink("/proc/self/fd/N")` | ~1us per FD |

**Why this matters for fathomdb**: The engine holds SQLite connections
(each with a database FD + WAL FD + SHM FD), plus the exclusive lock file.
FD leaks from unclosed connections are a real operational concern. A
periodic FD count check that alerts when count approaches the soft limit
or trends upward is cheap insurance.

### Threads and Concurrency

| Metric | Source | Cost |
|---|---|---|
| Thread count | `/proc/self/status` `Threads:` | ~2us |
| Per-thread state | `/proc/self/task/[tid]/stat` field 3 | ~2us per thread |
| Per-thread name | `/proc/self/task/[tid]/comm` | ~2us per thread |

#### Mutex contention

Rust has no built-in mutex contention API. Options:

- **Indirect**: High voluntary context switch rate correlates with
  contention. Zero-cost to check via `getrusage`.
- **Wrapper**: Measure time between `Mutex::lock()` call and acquisition
  using `Instant::now()`. Adds ~40ns per lock. Worth doing for the
  writer channel mutex and read pool mutex — not for every lock.

#### Channel backpressure

`std::sync::mpsc` (used for the writer channel) has no `.len()` method.
If observing writer queue depth becomes important, switching to
`crossbeam-channel` or `flume` (both provide `.len()`) would be the
change. This is deferred unless writer backpressure becomes a diagnosed
issue.

### Disk I/O

| Metric | Source | Meaning |
|---|---|---|
| `rchar` | `/proc/self/io` | Bytes passed to read syscalls (includes page cache) |
| `wchar` | `/proc/self/io` | Bytes passed to write syscalls (logical) |
| `read_bytes` | `/proc/self/io` | Bytes actually fetched from disk (physical) |
| `write_bytes` | `/proc/self/io` | Bytes actually written to disk |
| `syscr` / `syscw` | `/proc/self/io` | Read/write syscall counts |

**Key derived metric**: `read_bytes / rchar` ratio shows page cache
effectiveness. For a well-cached SQLite workload this should be near zero
(most reads hit the page cache). A rising ratio indicates the working set
exceeds available memory.

Linux-only. macOS provides only `getrusage().ru_inblock/ru_oublock` (block
I/O operation counts, much less detail). Parsed manually — 7 key-value
lines, no crate needed.

### PyO3-Specific Telemetry

| Metric | How | Cost |
|---|---|---|
| GIL acquisition latency | Wrap `Python::with_gil()` with `Instant::now()` before/after | ~40ns per call |
| Python GC stats | Call `gc.get_stats()` via PyO3 | One Python call |
| Python refcount for key objects | `pyo3::ffi::Py_REFCNT(obj)` | Cheap (field read) |
| Python heap size | Limited — `sys.getsizeof` is per-object only | Impractical at scale |

**Practical approach**: The most useful PyO3 metric is GIL acquisition
latency. If `with_gil()` regularly blocks for >1ms, the Python layer has
GIL contention that affects fathomdb operations. This can be measured with
a simple timing wrapper, no crate needed. The existing `pyo3-log` crate
already caches Python logger levels to minimize GIL acquisitions — the
telemetry wrapper would provide evidence for whether that caching is
sufficient.

### Process telemetry collection schedule

All process-level metrics are sampled periodically, not per-operation:

| Metric group | Default interval | Cost per sample |
|---|---|---|
| `getrusage` (CPU, context switches, peak RSS) | 10s | ~200ns |
| `/proc/self/io` (disk I/O) | 10s | ~2-5us |
| `/proc/self/statm` (current RSS, VSZ) | 10s | ~1-5us |
| FD count | 60s | ~5-50us |
| Thread count | 60s | ~2us |
| jemalloc stats (if enabled) | 10s | ~1-5us (`epoch::advance`) |

Total cost per 10-second cycle: <15us. Negligible.

### Dependency summary

| What | Dependency | Required? |
|---|---|---|
| Process CPU, context switches, peak RSS | `libc` (already present) | Level 0 — always |
| Process I/O, RSS, FDs, threads | `/proc` filesystem (Linux) | Level 2 — periodic |
| Allocator introspection | `tikv-jemallocator` + `tikv-jemalloc-ctl` | Optional — strongly recommended |
| Counting allocator | None (15 lines of `GlobalAlloc` wrapper) | Optional — useful even with jemalloc |
| GIL latency | None (timing wrapper around `with_gil`) | Optional — Python builds only |

## Implementation Considerations

### rusqlite FFI access

`rusqlite` does not provide high-level wrappers for `sqlite3_db_status()`,
`sqlite3_status()`, or `sqlite3_stmt_scanstatus()`. Access is through
`rusqlite::ffi` and `Connection::handle()`:

```rust
use rusqlite::ffi;

unsafe {
    let mut current: i32 = 0;
    let mut highwater: i32 = 0;
    ffi::sqlite3_db_status(
        conn.handle(),
        ffi::SQLITE_DBSTATUS_CACHE_HIT,
        &mut current,
        &mut highwater,
        0, // resetFlag
    );
}
```

This is straightforward but requires `unsafe`. The telemetry module should
encapsulate these calls behind safe wrappers.

### /proc/self/io portability

`/proc/self/io` is Linux-only. On other platforms (macOS, Windows), the
process I/O metrics are unavailable. The telemetry system should detect
the platform at compile time (`#[cfg(target_os = "linux")]`) and omit
those fields on unsupported platforms. The `libc::getrusage` call is
POSIX-portable and works on Linux and macOS.

### Thread safety

- Level 0 counters: `AtomicU64`, trivially safe.
- Level 1 statement buffer: per-connection, no sharing needed.
- Level 2 periodic snapshots: collected on a dedicated timer thread (or
  the writer thread during idle periods), stored behind an `RwLock` for
  read access from reporting surfaces.

### Interaction with connection pooling

The engine uses a reader connection pool (`ExecutionCoordinator`). Each
reader connection has its own SQLite-level counters. Level 0 SQLite
counters should be read from all pool connections and summed when
reporting. Level 1 statement profiling attaches to individual connections
— the `trace_v2` callback is installed per-connection in `open_connection`.

### Dependencies

Level 0 and Level 1 require no new crate dependencies:
- `AtomicU64` is in `std::sync::atomic`
- SQLite FFI is in `rusqlite::ffi` (already a dependency)
- `libc` is a transitive dependency (already present)
- Process snapshots parse `/proc/self/io` manually (7 key-value lines)

Optional dependencies:
- `tikv-jemallocator` + `tikv-jemalloc-ctl` for allocator introspection
  (strongly recommended — jemalloc is often faster than system malloc, so
  this is a net improvement, not a tradeoff)
- `hdrhistogram` (~3 deps) for latency distributions if reporting requires
  percentile aggregation (deferred until needed)

## Configuration

Telemetry level is configured at engine open:

```rust
pub enum TelemetryLevel {
    /// Level 0: cumulative counters only. Always active.
    Counters,
    /// Level 1: per-statement profiling (trace_v2 + stmt_status).
    Statements,
    /// Level 2: deep profiling (scan status + process snapshots).
    /// Requires high-telemetry build for full scan-status data.
    Profiling,
}
```

Default: `TelemetryLevel::Counters`.

The level cannot be changed after engine open — `trace_v2` callbacks are
registered per-connection at open time, and the reader pool connections are
created during initialization.

For Python, the level is set via `Engine.open()` options:

```python
engine = fathomdb.Engine.open(
    path,
    telemetry_level="statements",
)
```

## Open Questions

- Should Level 1 be the default for debug builds? The overhead is small
  enough that always-on statement profiling during development could
  surface performance regressions early.
- Should the periodic dump (Level 2) write to a sidecar file or only emit
  tracing events? A sidecar file would survive process crashes but adds
  file management concerns that the design-logging-and-tracing note
  explicitly avoids.
- Should per-statement metrics be exposed to the Python layer as part of
  query results, or only through the telemetry reporting API? Exposing
  them per-result is convenient for ad-hoc profiling but adds fields to
  the public API surface.
- What slow-statement threshold is appropriate as a default? 100ms is
  proposed; this should be validated against real workloads.
- Should jemalloc be the default allocator or opt-in via a feature flag?
  It provides critical introspection and is often faster, but adds a C
  dependency and changes allocation behavior (different fragmentation
  characteristics, different RSS patterns).
- Should the counting allocator wrapper be always-on (even alongside
  jemalloc)? The overhead (~5-15ns per allocation) is negligible, and
  it provides allocation rate data that jemalloc's per-thread TLS
  counters also cover.
- For the writer channel (`std::sync::mpsc`), is queue depth observation
  important enough to justify switching to `crossbeam-channel` or `flume`?
  Or is voluntary context switch rate on the writer thread a sufficient
  proxy for backpressure?
- Should GIL acquisition latency tracking be always-on in Python builds
  or gated behind the telemetry level? The ~40ns overhead per `with_gil`
  call is small, but `with_gil` is called frequently.

## Appendix A: Allocator Evaluation — jemalloc vs mimalloc

This design recommends `tikv-jemallocator` for allocator introspection.
This appendix documents the evaluation of mimalloc as an alternative.

### Rust crate ecosystem

| | jemalloc | mimalloc |
|---|---|---|
| Allocator crate | `tikv-jemallocator` v0.6 (63M downloads) | `mimalloc` v0.1 (31M downloads) |
| Stats/telemetry crate | `tikv-jemalloc-ctl` v0.6 — safe, typed Rust API | **Does not exist.** Raw FFI through `libmimalloc-sys` only |
| Build dependency | `cc` (compiles ~100K LOC C, slow builds) | `cc` (compiles ~10K LOC C, fast builds) |
| Static library size | ~400-700 KB | ~100-200 KB |

### Telemetry API comparison

This is the decisive difference. jemalloc provides structured, safe reads
for every metric fathomdb needs. mimalloc requires either parsing text
output or walking heaps with unsafe FFI.

| Metric | jemalloc | mimalloc |
|---|---|---|
| Allocated bytes | `stats::allocated::read()` — safe | Parse `mi_stats_print_out()` text or walk heaps via `mi_heap_visit_blocks()` — unsafe |
| Active bytes | `stats::active::read()` — safe | Sum `committed` from `mi_heap_area_t` across all thread heaps — unsafe, requires cross-thread coordination |
| Resident bytes | `stats::resident::read()` — safe | `mi_process_info()` — but this just wraps `getrusage`, not allocator-tracked |
| Retained (unreturned to OS) | `stats::retained::read()` — safe | Not directly exposed |
| Per-thread alloc rate | `thread::allocatedp` — safe TLS counter | `mi_thread_stats_print_out()` — text output only |
| Fragmentation ratio | `allocated / active` — two safe reads | Must compute from heap walk across all threads |
| Metadata overhead | `stats::metadata::read()` — safe | Not directly exposed |
| Cost to read all stats | ~1-5us (`epoch::advance()` + atomic loads) | ~10-100us (heap walking) or text parsing |

`mi_process_info()` — mimalloc's one structured API — returns process-level
OS metrics (RSS, CPU time, page faults) that are available from `getrusage()`
without any allocator. It does not provide allocator-internal metrics.

### Performance characteristics

For fathomdb's workload (single writer, 4-8 readers, long-running):

| Characteristic | jemalloc | mimalloc |
|---|---|---|
| Cross-thread free (writer allocates, reader frees) | tcache-local, occasional bin flush with lock | Single CAS to owning page's thread-free list — slightly more elegant |
| Thread exit handling | Arenas are shared, no orphaned memory | Pages become "abandoned", reclaimed gradually |
| Fragmentation control | Two-tier dirty/muzzy decay, background thread option | Eager page purging (`mi_option_purge_delay`, default 10ms) |
| Database production track record | Redis, ScyllaDB, TiKV, ClickHouse | Microsoft internal services |

Both handle fathomdb's thread pattern well. The cross-thread free
difference is negligible at 4-8 threads.

### Build and integration

| Concern | jemalloc | mimalloc |
|---|---|---|
| Build speed | Slow (~100K LOC C, autoconf) | Fast (~10K LOC C) |
| PyO3/cdylib safety | Proven (TiKV, Materialize ship this way). `je_` prefix avoids symbol conflicts | `override` feature has had issues loading into Python. Safe without `override`, but less tested |
| aarch64 | Well-tested | Supported |
| Windows | Supported, less tested | First-class (Microsoft project) |
| SQLite coexistence | SQLite uses `sqlite3_malloc` internally, separate from `#[global_allocator]` | Same — no conflict |

### Where mimalloc wins

- **Build speed**: ~10x less C code. Meaningful for CI.
- **Binary size**: ~2-3x smaller static library.
- **Windows support**: First-class, if Windows becomes important.
- **Simpler tuning**: Fewer knobs (48 options vs jemalloc's hundreds).

### Why jemalloc is recommended

The telemetry design requires structured allocator stats — fragmentation
ratios, allocated bytes, per-thread allocation rates — accessible from
Rust code. jemalloc provides all of these through `tikv-jemalloc-ctl`'s
safe API. mimalloc would require either:

1. Building a custom safe wrapper crate around `libmimalloc-sys` FFI
   functions (`mi_heap_visit_blocks`, `mi_stats_print_out`), or
2. Parsing text output from `mi_stats_print_out`, which is fragile and
   depends on format stability across versions.

Neither is justified when jemalloc meets all requirements out of the box.
The build speed cost (~30-60s additional on clean build) is the main
tradeoff, acceptable for the telemetry capability gained.

If mimalloc develops a `mimalloc-ctl` equivalent with safe, typed stats
access, this recommendation should be revisited.

## Appendix B: Custom Allocator Risk Assessment

Replacing the system allocator changes the memory management plumbing for
the entire Rust side of the process. This appendix evaluates the commonly
cited risks against fathomdb's specific architecture.

### B.1 Dual-Allocator Conflict — Not a risk

**Concern**: When a C library (SQLite) uses the system malloc and Rust uses
a custom allocator, memory allocated by one and freed by the other causes
segfaults.

**Why this does not apply**: fathomdb uses `rusqlite` with the `bundled`
feature, which statically compiles SQLite's C source. SQLite manages its
own memory internally through `sqlite3_malloc`/`sqlite3_free` — page
cache, prepared statements, parse trees, and result values are all
allocated and freed within SQLite's own memory system. These pointers are
never exposed to Rust for deallocation.

`rusqlite` handles the FFI boundary correctly: Rust-allocated buffers
(bind parameters) are passed to SQLite, which copies what it needs.
SQLite-allocated values (column results) are read through accessors and
copied into Rust-owned types (`String`, `Vec<u8>`, etc.). Ownership never
crosses the boundary in a way that would mix allocators.

Verified in the fathomdb codebase: zero direct calls to
`sqlite3_malloc`/`sqlite3_free`. The only raw pointer work is in
`sqlite.rs` (reading a `CStr` from SQLite's `trace_v2` callback — 
read-only, no ownership transfer) and `executable_trust.rs` (Windows API,
unrelated to SQLite).

The two allocators coexist without conflict: Rust's `#[global_allocator]`
handles `Box`, `Vec`, `String`, `Arc`. SQLite's internal
`sqlite3_malloc` handles page cache, statement memory, schema data. They
never cross.

### B.2 RSS Bloat from Thread-Local Caches — Manageable

**Concern**: High-performance allocators use thread-local caches that retain
memory beyond what is actively used. In constrained environments (Docker
containers, small VMs), this can trigger OOM killers.

**Assessment for fathomdb**: The thread count is small and fixed — 1 writer
thread, 4-8 reader pool threads, occasional admin operations. The
thread-local cache overhead scales with thread count:

| Allocator | Per-thread overhead | At 8 threads |
|---|---|---|
| jemalloc (tcache) | ~256KB-2MB per arena | ~8-32MB (depends on arena count, default 4x CPU cores) |
| mimalloc | ~64KB per thread-local segment | ~512KB |
| System (glibc) | ~64KB per-thread arena | ~512KB |

For fathomdb's scale, jemalloc's overhead is the largest but still modest
in absolute terms. The overhead is visible (not hidden) because jemalloc
reports it: `stats::active - stats::allocated` shows exactly how much
memory is held in caches and fragmentation. This is the metric the
telemetry design captures.

**Mitigation**: jemalloc's dirty page decay timer (default 10s) and muzzy
decay timer (default 120s) control how aggressively retained memory is
returned to the OS. For memory-constrained deployments, these can be tuned
shorter via `mallctl`:

```rust
jemalloc_ctl::set("arenas.dirty_decay_ms", 1000).ok();  // 1s
jemalloc_ctl::set("arenas.muzzy_decay_ms", 5000).ok();  // 5s
```

**Operational note**: In constrained environments, jemalloc's retained
memory can appear as a "leak" to monitoring tools that only track RSS. The
telemetry system's periodic reporting of `stats::allocated` (actual use)
vs `stats::resident` (OS-mapped) makes this transparent rather than
surprising.

### B.3 Debugging Tool Compatibility — Minor, handled by feature gate

**Concern**: Valgrind, ASAN, and heap profilers are tuned for the system
allocator and may produce false positives or miss allocations through a
custom allocator.

**Assessment**: This concern is partially outdated:

- **Valgrind**: Works with jemalloc. Valgrind intercepts `mmap`/`brk`
  regardless of which allocator is in use. jemalloc 5.x removed its
  explicit Valgrind integration (the `--enable-valgrind` flag), but
  Valgrind's `mmap` interception still tracks all memory. Leak detection
  may report jemalloc-retained (but not leaked) memory as "possibly lost"
  — this is a cosmetic issue, not a functional one.
- **ASAN (AddressSanitizer)**: Requires the system allocator. ASAN
  replaces `malloc`/`free` with its own instrumented versions, which
  conflicts with any custom allocator. This is standard — all projects
  using custom allocators disable them for ASAN builds.
- **Miri**: Uses its own simulated allocator. `#[global_allocator]` is
  ignored. No conflict.
- **jemalloc-specific tools**: `prof.dump` (heap profiling),
  `stats_print` (detailed stats) are *additional* capability, not a
  replacement for standard tools.

**Mitigation**: Feature-gate the allocator so it can be disabled for
sanitizer and profiler builds:

```toml
# Cargo.toml
[features]
jemalloc = ["dep:tikv-jemallocator", "dep:tikv-jemalloc-ctl"]
```

```rust
// lib.rs or main.rs
#[cfg(feature = "jemalloc")]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
```

Standard `cargo test` and ASAN builds omit `--features jemalloc` and use
the system allocator. Release and telemetry-enabled builds include it.

### B.4 Platform Compatibility — Scoped by feature gate

**Concern**: Custom allocators can have build or runtime issues on certain
platforms (Windows, older ARM).

**Assessment for fathomdb's targets**:

| Platform | jemalloc status | Risk |
|---|---|---|
| Linux x86_64 | Primary development platform for jemalloc. Extensively tested by TiKV, Redis, Firefox | None |
| Linux aarch64 | Well-tested. TiKV runs on ARM in production | None |
| macOS (x86_64 + Apple Silicon) | Supported. Apple Silicon tested by multiple projects | Low |
| Windows | Supported by `tikv-jemallocator`. Less battle-tested than Linux | Low-moderate |

**Mitigation**: The feature gate from B.3 also handles this. On platforms
where jemalloc is unsupported or untested, the feature is simply not
enabled. The telemetry system degrades gracefully — allocator-specific
metrics report `None`, and process-level metrics (`getrusage`, procfs)
remain available.

### Risk summary

| Risk | Severity | Mitigation |
|---|---|---|
| Dual-allocator conflict | **None** — `rusqlite` + `bundled` keeps allocators separated by design | No action needed |
| RSS bloat from thread caches | **Low** — small fixed thread count, jemalloc decay timers handle retention | Document tuning for constrained environments; telemetry makes overhead visible |
| Debugging tool compatibility | **Low** — feature-gate allocator; ASAN/Valgrind builds use system allocator | `jemalloc` Cargo feature, disabled for sanitizer builds |
| Platform compatibility | **Low** — primary targets (Linux x86_64, aarch64) well-supported | Feature-gate; graceful degradation in telemetry |
