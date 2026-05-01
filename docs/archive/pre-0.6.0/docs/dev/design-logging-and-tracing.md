# Design Note: Logging and Tracing for fathomdb

## Status

Proposed — 2026-03-30

## Problem

fathomdb has **no logging framework**. The entire codebase produces two `eprintln!` calls
(one in the writer thread's panic-during-shutdown handler, one in the admin bridge's error
response path) and zero structured diagnostic output. For an embedded database library
whose design goals center on recoverability and provenance, this is a significant gap:

- **Writer thread failures** are invisible until a write is rejected.
- **Read pool contention** cannot be detected without instrumentation.
- **Admin operation progress** (exports, rebuilds, vector regeneration) is unknown —
  could be stuck, could be slow, cannot tell.
- **Schema migrations** can fail with opaque version-mismatch errors and no step-level
  detail.
- **SQLite-level events** (corruption notices, recovery events, WAL checkpoint status)
  never surface to the host application.
- **Error context** is lost at every boundary: Rust→Python (map\_engine\_error discards
  operation details), writer thread→caller (channel strips panic location), bridge→Go
  (generic "internal error; check bridge stderr").

The challenge is that fathomdb is consumed by three different runtimes — direct Rust,
Python via PyO3, and Go via a JSON-over-stdio bridge — each with different logging
infrastructure expectations.

## Lessons from SQLite and Other Embedded Databases

### What SQLite does (and why it isn't enough)

**SQLITE\_CONFIG\_LOG** is a process-global error callback registered before any
connections are opened. It receives an error code and a printf-formatted string. It
covers corruption detection, recovery notices, I/O errors, misuse errors, and OOM.

Constraints that matter for fathomdb:

| Constraint | Impact |
|---|---|
| **Process-wide singleton** | One callback for the entire process. Multiple libraries embedding SQLite fight over it. |
| **Not reentrant** | Callback must not call any SQLite API. |
| **Unstable message format** | Applications must not parse message text — it changes between releases. |
| **Fixed-length stack buffer** | Messages truncated silently at a few hundred characters. |
| **Must be configured before any connections** | Requires process-level coordination before library initialization. |
| **No structured data** | Everything is a string. No severity levels beyond the raw error code integer. |

What SQLITE\_CONFIG\_LOG does **not** cover: WAL checkpoint progress, query execution
timing, connection-level scoping, anything that distinguishes between database files in
the same process.

**sqlite3\_trace\_v2** is a per-connection hook with four masks: STMT (statement begins),
PROFILE (statement finishes with nanosecond timing), ROW (each result row), CLOSE
(connection closes). This is more useful for operational observability but only covers SQL
execution, not internal SQLite events.

**Known production pitfalls with SQLite logging:**

- The WAL-reset data race (3.7.0–3.51.2, fixed 3.51.3) was a **silent corruption** —
  no SQLITE\_CONFIG\_LOG event fired because the bug was a race condition, not a
  detectable error state. This is the class of problem where fathomdb's own
  instrumentation matters more than SQLite's built-in logging.
- SQLITE\_TRACE\_ROW fires per result row and devastates performance on large result
  sets. The callback runs on the SQLite execution thread; any blocking in the callback
  blocks query execution.
- SQLITE\_TRACE\_PROFILE is safe for production use — one event per statement completion
  with nanosecond timing.

### What elite embedded databases do differently

**RocksDB** separates three concerns that SQLite conflates:

1. **Diagnostic logging** (Logger class hierarchy) — human-readable messages for
   debugging. Configurable via `db_log_dir`, `info_log_level`, `max_log_file_size`,
   `keep_log_file_num`. Custom Logger subclasses are explicitly encouraged.
2. **Structural events** (EventListener) — typed callbacks for operational automation
   (flush completed, compaction completed, stall condition changed, background error).
   Thread-safe, non-blocking, no locks held during callback.
3. **Quantitative metrics** (Statistics) — counters and histograms for monitoring, dumped
   periodically. Entirely separate from the log stream.

This three-way separation is the key insight: diagnostic logging, lifecycle events, and
metrics serve different audiences and should not be forced through the same channel.

**DuckDB** makes logging queryable structured data — each log type has a defined schema,
logs are stored in a `duckdb_logs` view, and per-type enable/disable is supported. This
is the opposite of SQLite's opaque string approach.

**LMDB** has essentially no logging. It relies entirely on return codes. This is the
minimalist extreme — appropriate for LMDB's simplicity but not for an engine with
fathomdb's admin/provenance/recovery ambitions.

### The Rust ecosystem consensus (2024–2025)

The `tracing` crate has won. New Rust libraries use `tracing` with the `"log"` feature
flag (which emits `log::Record`s when no tracing subscriber is active, providing
backward compatibility with the older `log` ecosystem). Applications configure
`tracing-subscriber` with composable layers.

| Library | Facade | Notes |
|---|---|---|
| tantivy | `log` | Older choice; works but loses structured spans |
| rusqlite | neither | Exposes SQLite's native trace hooks as Rust callbacks |
| sled | `log` | Uses `log` for internal diagnostics |
| meilisearch | `tracing` | Full structured tracing with JSON and human-readable layers |

The rule: **libraries emit events via the facade; libraries never configure subscribers.**

## Design

### Principle 1: Three separate concerns

Following RocksDB's architecture, fathomdb distinguishes three layers:

| Layer | Purpose | Audience | Existing infrastructure |
|---|---|---|---|
| **Structured tracing** | Diagnostic events with context (spans, fields, timing) | Developers, operators debugging specific issues | None — this design adds it |
| **Response-cycle feedback** | Lifecycle events for public API operations (started/slow/heartbeat/finished/failed) | Application code, UX layers, operational dashboards | `feedback.rs` — already integrated into all public Engine methods |
| **Metrics** (future) | Counters and histograms (write throughput, query latency percentiles, cache hit rate) | Monitoring systems | Not yet needed; deferred |

Structured tracing and feedback are complementary, not competing:

- **Feedback** tells you "the write operation took 450ms and succeeded." It is
  high-level, public-API-scoped, and designed for application consumption.
- **Tracing** tells you "the writer thread spent 12ms in FTS row resolution, 380ms
  waiting for `BEGIN IMMEDIATE`, and 58ms committing." It is low-level,
  internal-implementation-scoped, and designed for diagnostic drill-down.

The feedback system is already shipped and integrated. This design adds the tracing
layer without disturbing feedback.

### Principle 2: Library emits, application configures

fathomdb library crates (`fathomdb`, `fathomdb-engine`, `fathomdb-query`,
`fathomdb-schema`) depend on the `tracing` facade crate and emit events via its macros.
They **never** call `tracing::subscriber::set_global_default()` or configure output
format, destination, or filtering.

The consuming application — whether Rust, Python, or the bridge binary — is responsible
for configuring the subscriber. If no subscriber is configured, all tracing events are
silently discarded with near-zero overhead (a single global atomic load per event site
after compile-time level filtering).

### Principle 3: Feature-gated, zero-cost when disabled

```toml
# In each library crate's Cargo.toml
[features]
default = []
tracing = ["dep:tracing"]

[dependencies]
tracing = { version = "0.1", optional = true, features = ["log"] }
```

When the `tracing` feature is not enabled, all instrumentation compiles away entirely.
When enabled, compile-time level filtering (`release_max_level_info` or
`release_max_level_warn`) eliminates debug/trace events from release binaries at compile
time — zero runtime cost for disabled levels.

In library code, instrumentation is conditional:

```rust
#[cfg_attr(feature = "tracing", tracing::instrument(skip(self, request), fields(label)))]
pub fn submit_write(&self, request: WriteRequest) -> Result<WriteReceipt, EngineError> {
    #[cfg(feature = "tracing")]
    tracing::Span::current().record("label", &request.label);
    // ...
}
```

For events inside function bodies, a thin macro wrapper avoids `#[cfg]` clutter:

```rust
macro_rules! trace_event {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::info!($($arg)*);
    };
}
```

### Principle 4: Instrument at the seams, not everywhere

Tracing adds value at structural boundaries. Per-row or per-iteration instrumentation
in hot paths is wasteful. The instrumentation tiers:

**Tier 1 — Always (error + lifecycle, WARN/ERROR level):**

| Subsystem | Events |
|---|---|
| Engine lifecycle | open (path, provenance\_mode, vector\_dimension, pool\_size), close, component init failure |
| Writer thread | start, fatal connection/bootstrap error, panic in resolve\_and\_apply (with write label and phase), shutdown |
| Read pool | mutex poisoning detected, first vector-table-absent degradation |
| Schema bootstrap | version at open, migration failure (version, description), version mismatch |
| Admin ops | safe\_export checkpoint blocked (busy != 0), subprocess exit with non-zero code |
| Bridge | request parse failure, command dispatch error |

**Tier 2 — Operational (INFO level):**

| Subsystem | Events |
|---|---|
| Schema bootstrap | each migration applied (version, description, duration\_ms) |
| Writer thread | write committed (label, node/edge/chunk counts, duration\_ms) |
| Projection rebuild | started (target), completed (rows\_deleted, rows\_inserted, duration\_ms) |
| Admin ops | operation started/completed (op type, key parameters, duration\_ms) |
| WAL checkpoint | started, completed (busy, log\_frames, checkpointed\_frames) |
| Operational collections | collection registered, compaction completed (rows\_deleted) |

**Tier 3 — Diagnostic (DEBUG level):**

| Subsystem | Events |
|---|---|
| Writer thread | FTS row resolution (nodes resolved, chunks processed), operational write resolution |
| Read pool | connection lock acquired (wait\_ms), shape cache hit/miss |
| Query compilation | compilation completed (shape\_hash, bind\_count, duration\_ms) |
| Expansion reads | batch count, per-batch row count |
| Projection rebuild | per-phase detail (delete FTS, insert FTS, delete vec, insert vec) |
| Admin ops | sub-operation boundaries, file I/O, subprocess stdin/stdout exchange |

**Tier 4 — Verbose (TRACE level, compiled out in release):**

| Subsystem | Events |
|---|---|
| SQLite operations | individual prepared statement execution (via sqlite3\_trace\_v2 PROFILE callback) |
| Write preparation | per-insert detail (node ID, chunk count, edge endpoints) |
| Operational mutations | per-mutation detail (collection, record\_key, op\_kind) |

### Principle 5: Bridge SQLite's logging into the tracing facade

Register `SQLITE_CONFIG_LOG` once during engine initialization (before any connections
open) to forward SQLite's internal error/warning events into tracing:

```rust
unsafe extern "C" fn sqlite_log_callback(_arg: *mut std::ffi::c_void, code: i32, msg: *const i8) {
    let msg = unsafe { std::ffi::CStr::from_ptr(msg) }.to_string_lossy();
    let level = match code & 0xFF {
        ffi::SQLITE_NOTICE => tracing::Level::INFO,
        ffi::SQLITE_WARNING => tracing::Level::WARN,
        _ => tracing::Level::ERROR,
    };
    tracing::event!(level, sqlite.error_code = code, "{msg}");
}
```

This surfaces corruption notices (`SQLITE_CORRUPT`), recovery events
(`SQLITE_NOTICE_RECOVER_WAL`), I/O errors, and misuse warnings through the same
tracing infrastructure the application already configured. The process-global singleton
constraint is acceptable because fathomdb owns the SQLite connection lifecycle.

For query profiling, enable `sqlite3_trace_v2(SQLITE_TRACE_PROFILE)` on each connection
when the `tracing` feature is active and TRACE level is enabled for the
`fathomdb_engine::sqlite` target. This provides per-statement execution timing without
the per-row overhead of SQLITE\_TRACE\_ROW.

### Per-consumer configuration

Each consumer surface configures its own subscriber:

**Direct Rust callers:**

```rust
// Application code — not in fathomdb
tracing_subscriber::fmt()
    .with_env_filter("fathomdb_engine=info,fathomdb_engine::writer=debug")
    .init();

let engine = Engine::open(options)?;
```

The application controls format (JSON, human-readable, compact), destination (stdout,
file via `tracing-appender`, journald via `tracing-journald`), and per-module filtering.
fathomdb has no opinion about any of this.

**Python via PyO3:**

The `pyo3-log` crate bridges Rust's `log` records (emitted by tracing's `"log"` feature
flag when no native tracing subscriber is active) into Python's `logging` module. Rust
module paths map to Python logger names: `fathomdb_engine::writer` →
`fathomdb_engine.writer`.

```python
import logging
logging.basicConfig(level=logging.INFO)

# In fathomdb's Python __init__.py, during module import:
import pyo3_log
pyo3_log.init()

engine = fathomdb.Engine.open(options)
```

The Python application controls log level and destination through standard Python
`logging` configuration. Memex (`~/projects/memex/`) would configure logging however it
normally does — file handler, JSON formatter, whatever — and fathomdb events appear in
that stream automatically.

GIL overhead note: `pyo3-log` caches Python logger effective levels and only acquires
the GIL when a message will actually be emitted. At INFO level with well-chosen
instrumentation points, this is negligible.

**Go via the admin bridge binary:**

The bridge binary is an **application**, not a library, so it configures its own
subscriber. Structured JSON on stderr keeps stdout clean for the bridge protocol:

```rust
// In fathomdb-admin-bridge main()
tracing_subscriber::fmt()
    .json()
    .with_writer(std::io::stderr)
    .with_env_filter(std::env::var("FATHOMDB_LOG").unwrap_or_else(|_| "warn".into()))
    .init();
```

The Go side reads structured JSON log lines from stderr alongside the JSON protocol
responses on stdout. The Go `fathom-integrity` tool can parse, forward, or discard
these as appropriate.

### How Memex would configure logging

Given that Memex is a Python application at `~/projects/memex/`:

```python
import logging
import logging.handlers

# Standard Python logging configuration
handler = logging.handlers.RotatingFileHandler(
    "logs/memex.log", maxBytes=10_000_000, backupCount=5
)
handler.setFormatter(logging.Formatter(
    "%(asctime)s %(name)s %(levelname)s %(message)s"
))
logging.getLogger().addHandler(handler)
logging.getLogger().setLevel(logging.INFO)

# fathomdb events appear under fathomdb_engine.*, fathomdb_query.*, etc.
# Tune fathomdb verbosity independently:
logging.getLogger("fathomdb_engine.writer").setLevel(logging.DEBUG)

# Initialize the Rust→Python log bridge (once, at import time)
import fathomdb
engine = fathomdb.Engine.open(options)
```

No fathomdb-specific configuration. No special log file path. Memex's existing logging
infrastructure receives fathomdb events through the standard Python logging hierarchy.

### Relationship to the feedback system

The feedback system (`feedback.rs`) and structured tracing serve different purposes and
should remain separate:

| Aspect | Feedback | Tracing |
|---|---|---|
| Scope | Public API operations | Internal implementation |
| Granularity | Per-operation lifecycle (5 phases) | Per-subsystem events (unlimited) |
| Consumer | Application code (observer callbacks) | Operator/developer (log output) |
| Contract | Stable, semantic, versioned | Unstable, diagnostic, best-effort |
| Overhead | Always active (observer pattern) | Feature-gated, level-filtered |

A feedback observer **could** emit tracing events as an implementation detail, but the
two systems should not be merged. Feedback is an application-facing contract; tracing is
a diagnostic escape hatch.

### What fathomdb does NOT do

- **Does not manage log files.** No `db_log_dir`, no rotation, no cleanup. File
  management is the application's responsibility (via `tracing-appender` for Rust,
  `logging.handlers` for Python, etc.).
- **Does not write to journald.** If the application wants journald output, it
  configures a `tracing-journald` layer. fathomdb is a library, not a daemon.
- **Does not configure filtering.** No `FATHOMDB_LOG` environment variable is read by
  the library. The application controls filtering through its subscriber configuration.
  (The bridge binary is an exception — it is an application.)
- **Does not add metrics.** Counters and histograms are a separate concern. When needed,
  they should use a metrics facade (`metrics` crate) rather than encoding quantitative
  data in log messages.
- **Does not instrument per-row or per-iteration hot paths.** Tracing at TRACE level
  covers per-statement SQLite profiling. Nothing instruments individual row processing.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| `SQLITE_CONFIG_LOG` conflicts with other SQLite embedders in the same process | fathomdb owns the SQLite lifecycle; document that the callback is registered at first engine open |
| GIL overhead in Python path | `pyo3-log` caches effective levels; INFO-level instrumentation at seams (not hot loops) keeps GIL acquisitions sparse |
| Feature flag complexity | Single `tracing` feature on each crate; conditional compilation limited to `#[cfg(feature = "tracing")]` attributes and one helper macro |
| Tracing overhead on writer thread | Writer thread events are at INFO (committed) and WARN/ERROR (failures) — no per-row instrumentation; compile-time `release_max_level_info` eliminates DEBUG/TRACE |
| Span context across thread boundary (caller → writer channel → writer thread) | Spans do not cross the channel. Writer thread events carry the write label as a field, not as a parent span. Correlation is by label, not by span hierarchy. |

## Implementation order

1. Add `tracing` as an optional dependency to all four library crates.
2. Instrument Tier 1 events (error + lifecycle) — writer thread, engine open/close,
   schema bootstrap failures, mutex poisoning.
3. Bridge SQLITE\_CONFIG\_LOG into tracing.
4. Instrument Tier 2 events (operational) — write commits, projection rebuilds, admin
   ops, WAL checkpoints.
5. Configure the bridge binary's subscriber (JSON stderr).
6. Add `pyo3-log` initialization to the Python binding.
7. Instrument Tier 3/4 events as diagnostic needs arise.
