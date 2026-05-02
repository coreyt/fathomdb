pub const SCHEMA_VERSION: u32 = 1;

/// SQLite `PRAGMA` name carrying the on-disk schema-version sentinel.
///
/// Public on-disk surface per `dev/interfaces/wire.md` § Schema-version
/// sentinel; advanced by successful migrations per `dev/design/migrations.md`.
pub const PRAGMA_USER_VERSION: &str = "user_version";

/// Suffix of the canonical SQLite database file (`<db-name>.sqlite`).
pub const SQLITE_SUFFIX: &str = ".sqlite";

/// Suffix of the SQLite write-ahead log file (`<db-name>.sqlite-wal`).
pub const WAL_SUFFIX: &str = "-wal";

/// Suffix of the sidecar lock file (`<db-name>.sqlite.lock`).
///
/// Per `dev/design/bindings.md` § 7, this sidecar flock is the load-bearing
/// cross-process exclusion layer; it surfaces lock contention before SQLite
/// I/O begins.
pub const LOCK_SUFFIX: &str = ".lock";

/// Suffix of the optional SQLite rollback journal file
/// (`<db-name>.sqlite-journal`).
pub const JOURNAL_SUFFIX: &str = "-journal";

#[must_use]
pub fn bootstrap_steps() -> &'static [&'static str] {
    &["create canonical tables", "register projection metadata", "seed rewrite-era configuration"]
}
