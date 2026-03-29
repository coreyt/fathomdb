# Design: Reader Connection Pool

## Purpose

Address the verified finding that a single reader connection serializes
all queries (M-1).

---

## Current State

`crates/fathomdb-engine/src/coordinator.rs:103,158`

`ExecutionCoordinator` holds one `Mutex<Connection>` for all reads. Every
query — flat reads, grouped reads, operational reads, trace calls — must
acquire `lock_connection()`. There is no concurrency for reads.

---

## Design

### Connection pool with fixed size

Replace the single `Mutex<Connection>` with a small pool of read-only
connections:

```rust
pub struct ExecutionCoordinator {
    pool: ReadPool,
    shape_sql_map: Mutex<HashMap<ShapeHash, String>>,
    vector_enabled: bool,
}

struct ReadPool {
    connections: Vec<Mutex<Connection>>,
}

impl ReadPool {
    fn new(db_path: &Path, pool_size: usize) -> Result<Self, EngineError> {
        let connections = (0..pool_size)
            .map(|_| {
                let conn = Connection::open_with_flags(
                    db_path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY
                        | OpenFlags::SQLITE_OPEN_NO_MUTEX
                        | OpenFlags::SQLITE_OPEN_WAL,
                )?;
                initialize_read_connection(&conn)?;
                Ok(Mutex::new(conn))
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        Ok(Self { connections })
    }

    fn acquire(&self) -> MutexGuard<'_, Connection> {
        // Try each connection without blocking first
        for conn in &self.connections {
            if let Ok(guard) = conn.try_lock() {
                return guard;
            }
        }
        // All busy — block on the first one
        self.connections[0].lock().unwrap()
    }
}
```

### Pool size

Default: 4 connections. This matches the expected concurrency profile of
a local agent datastore:

- Typically 1-2 concurrent readers (the agent process plus an optional
  admin/diagnostic tool).
- SQLite WAL mode supports unlimited concurrent readers, but each
  connection consumes file descriptors and memory for prepared statements.
- 4 connections allow the grouped-read expansion queries (M-4) to overlap
  with other reads without serialization.

Configurable via `EngineOptions::read_pool_size`.

### Read-only connection initialization

Each pooled connection runs the same PRAGMA initialization as the current
single connection, except:
- `journal_mode` does not need to be set — WAL is a database-level
  property, not per-connection.
- `busy_timeout` should be set to a short value (1000ms) for readers.
  Reader contention in WAL mode is rare but possible during checkpoints.

### `lock_connection()` migration

Replace all `self.lock_connection()` calls with `self.pool.acquire()`.
The return type is the same (`MutexGuard<Connection>`), so call sites
are unchanged.

### Interaction with the shape cache

The `shape_sql_map` is shared across all pooled connections. It remains
behind its own `Mutex`. This is correct — the cache maps shape hashes to
SQL strings, which are connection-independent.

### Interaction with sqlite-vec

If `vector_enabled` is true, each pooled connection must have the
sqlite-vec extension loaded. The `sqlite3_auto_extension` registration
happens once globally, so new connections inherit it automatically.

---

## Not in scope

- Dynamic pool sizing. Fixed size is simpler and sufficient.
- Connection health checks. SQLite read-only connections do not go stale.
- Prepared statement sharing across connections. Each connection maintains
  its own statement cache via `prepare_cached`.

---

## Test Plan

- Open an engine with pool_size=4. Run 4 concurrent reads. Verify all
  complete without serialization (wall time < 4x single read time).
- Verify that grouped reads with 3 expansion slots can overlap with an
  independent flat read.
- Verify sqlite-vec queries work on all pooled connections.
