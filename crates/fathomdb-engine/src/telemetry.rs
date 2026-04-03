//! Resource telemetry: always-on counters and `SQLite` cache statistics.
//!
//! See `dev/design-note-telemetry-and-profiling.md` for the full design.

use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;

/// Controls how much telemetry the engine collects.
///
/// Levels are additive — each level includes everything from below it.
/// Level 0 counters are always maintained regardless of this setting;
/// the level controls whether higher-cost collection is enabled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TelemetryLevel {
    /// Level 0: cumulative counters only. Always active.
    #[default]
    Counters,
    /// Level 1: per-statement profiling (`trace_v2` + `stmt_status`).
    Statements,
    /// Level 2: deep profiling (scan status + process snapshots).
    /// Requires high-telemetry build for full scan-status data.
    Profiling,
}

/// Always-on cumulative counters, shared across all engine components.
///
/// All increments use [`Ordering::Relaxed`] — these are statistical counters,
/// not synchronization primitives.
#[derive(Debug, Default)]
#[allow(clippy::struct_field_names)]
pub struct TelemetryCounters {
    queries_total: AtomicU64,
    writes_total: AtomicU64,
    write_rows_total: AtomicU64,
    errors_total: AtomicU64,
    admin_ops_total: AtomicU64,
}

impl TelemetryCounters {
    /// Increment the query counter by one.
    pub fn increment_queries(&self) {
        self.queries_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the write counter by one and add `row_count` to the row total.
    pub fn increment_writes(&self, row_count: u64) {
        self.writes_total.fetch_add(1, Ordering::Relaxed);
        self.write_rows_total
            .fetch_add(row_count, Ordering::Relaxed);
    }

    /// Increment the error counter by one.
    pub fn increment_errors(&self) {
        self.errors_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the admin operations counter by one.
    pub fn increment_admin_ops(&self) {
        self.admin_ops_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Read all counters into a [`TelemetrySnapshot`].
    ///
    /// The `sqlite_cache` field is left at defaults — callers that need
    /// cache status should populate it separately via
    /// [`read_db_cache_status`].
    #[must_use]
    pub fn snapshot(&self) -> TelemetrySnapshot {
        TelemetrySnapshot {
            queries_total: self.queries_total.load(Ordering::Relaxed),
            writes_total: self.writes_total.load(Ordering::Relaxed),
            write_rows_total: self.write_rows_total.load(Ordering::Relaxed),
            errors_total: self.errors_total.load(Ordering::Relaxed),
            admin_ops_total: self.admin_ops_total.load(Ordering::Relaxed),
            sqlite_cache: SqliteCacheStatus::default(),
        }
    }
}

/// Cumulative `SQLite` page-cache counters for a single connection.
///
/// Uses `i64` to allow safe summing across pool connections without overflow.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqliteCacheStatus {
    /// Page cache hits.
    pub cache_hits: i64,
    /// Page cache misses.
    pub cache_misses: i64,
    /// Pages written to cache.
    pub cache_writes: i64,
    /// Cache pages spilled to disk.
    pub cache_spills: i64,
}

impl SqliteCacheStatus {
    /// Add another status into this one (for aggregating across connections).
    pub fn add(&mut self, other: &Self) {
        self.cache_hits += other.cache_hits;
        self.cache_misses += other.cache_misses;
        self.cache_writes += other.cache_writes;
        self.cache_spills += other.cache_spills;
    }
}

/// Read cumulative page-cache counters from a `SQLite` connection.
///
/// Calls `sqlite3_db_status()` for `CACHE_HIT`, `CACHE_MISS`, `CACHE_WRITE`,
/// and `CACHE_SPILL` with `resetFlag=0` (non-destructive read).
///
/// # Safety contract
///
/// The function is safe because `Connection::handle()` returns a valid
/// `sqlite3*` for the connection's lifetime, and `sqlite3_db_status` is
/// read-only and thread-safe for the owning connection.
pub fn read_db_cache_status(conn: &Connection) -> SqliteCacheStatus {
    let mut status = SqliteCacheStatus::default();

    // Helper: read one db_status code, returning the current value.
    let read_one = |op: i32| -> i64 {
        let mut current: i32 = 0;
        let mut highwater: i32 = 0;
        // Safety: conn.handle() is valid for the connection's lifetime.
        // sqlite3_db_status with resetFlag=0 is a non-destructive read.
        unsafe {
            rusqlite::ffi::sqlite3_db_status(
                conn.handle(),
                op,
                &raw mut current,
                &raw mut highwater,
                0, // resetFlag
            );
        }
        i64::from(current)
    };

    status.cache_hits = read_one(rusqlite::ffi::SQLITE_DBSTATUS_CACHE_HIT);
    status.cache_misses = read_one(rusqlite::ffi::SQLITE_DBSTATUS_CACHE_MISS);
    status.cache_writes = read_one(rusqlite::ffi::SQLITE_DBSTATUS_CACHE_WRITE);
    status.cache_spills = read_one(rusqlite::ffi::SQLITE_DBSTATUS_CACHE_SPILL);

    status
}

/// Point-in-time snapshot of all telemetry counters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TelemetrySnapshot {
    /// Total read operations executed.
    pub queries_total: u64,
    /// Total write operations committed.
    pub writes_total: u64,
    /// Total rows written (nodes + edges + chunks).
    pub write_rows_total: u64,
    /// Total operation errors.
    pub errors_total: u64,
    /// Total admin operations.
    pub admin_ops_total: u64,
    /// Aggregated `SQLite` page-cache counters (summed across pool connections).
    pub sqlite_cache: SqliteCacheStatus,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use rusqlite::Connection;

    use super::{SqliteCacheStatus, TelemetryCounters, TelemetryLevel, read_db_cache_status};

    #[test]
    fn telemetry_level_default_is_counters() {
        assert_eq!(TelemetryLevel::default(), TelemetryLevel::Counters);
    }

    #[test]
    fn counter_defaults_to_zero() {
        let counters = TelemetryCounters::default();
        let snap = counters.snapshot();
        assert_eq!(snap.queries_total, 0);
        assert_eq!(snap.writes_total, 0);
        assert_eq!(snap.write_rows_total, 0);
        assert_eq!(snap.errors_total, 0);
        assert_eq!(snap.admin_ops_total, 0);
    }

    #[test]
    fn counter_increment_and_snapshot() {
        let counters = TelemetryCounters::default();

        counters.increment_queries();
        counters.increment_queries();
        counters.increment_writes(5);
        counters.increment_writes(3);
        counters.increment_errors();
        counters.increment_admin_ops();
        counters.increment_admin_ops();
        counters.increment_admin_ops();

        let snap = counters.snapshot();
        assert_eq!(snap.queries_total, 2);
        assert_eq!(snap.writes_total, 2);
        assert_eq!(snap.write_rows_total, 8);
        assert_eq!(snap.errors_total, 1);
        assert_eq!(snap.admin_ops_total, 3);
    }

    #[test]
    fn read_db_cache_status_on_fresh_connection() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        let status = read_db_cache_status(&conn);
        // Fresh connection should have valid (non-negative) values.
        assert!(status.cache_hits >= 0);
        assert!(status.cache_misses >= 0);
        assert!(status.cache_writes >= 0);
        assert!(status.cache_spills >= 0);
    }

    #[test]
    fn cache_status_reflects_queries() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, value TEXT);
             INSERT INTO t VALUES (1, 'a');
             INSERT INTO t VALUES (2, 'b');
             INSERT INTO t VALUES (3, 'c');",
        )
        .expect("setup");

        // Run several queries to exercise the cache.
        for _ in 0..10 {
            let mut stmt = conn.prepare("SELECT * FROM t").expect("prepare");
            let _rows: Vec<i64> = stmt
                .query_map([], |row| row.get(0))
                .expect("query")
                .map(|r| r.expect("row"))
                .collect();
        }

        let status = read_db_cache_status(&conn);
        // After queries, we should see cache activity.
        assert!(
            status.cache_hits + status.cache_misses > 0,
            "expected cache activity after queries, got hits={} misses={}",
            status.cache_hits,
            status.cache_misses,
        );
    }

    #[test]
    fn cache_status_add_sums_correctly() {
        let a = SqliteCacheStatus {
            cache_hits: 10,
            cache_misses: 2,
            cache_writes: 5,
            cache_spills: 1,
        };
        let b = SqliteCacheStatus {
            cache_hits: 3,
            cache_misses: 7,
            cache_writes: 0,
            cache_spills: 4,
        };
        let mut total = SqliteCacheStatus::default();
        total.add(&a);
        total.add(&b);
        assert_eq!(total.cache_hits, 13);
        assert_eq!(total.cache_misses, 9);
        assert_eq!(total.cache_writes, 5);
        assert_eq!(total.cache_spills, 5);
    }
}
