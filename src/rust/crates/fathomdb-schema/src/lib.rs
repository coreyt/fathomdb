use std::fmt::{Display, Formatter};
use std::time::Instant;

use rusqlite::Connection;

pub const SCHEMA_VERSION: u32 = 7;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Migration {
    pub step_id: u32,
    pub sql: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStepReport {
    pub step_id: u32,
    pub duration_ms: Option<u64>,
    pub failed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationReport {
    pub schema_version_before: u32,
    pub schema_version_after: u32,
    pub migration_steps: Vec<MigrationStepReport>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationFailureReport {
    pub schema_version_before: u32,
    pub schema_version_current: u32,
    pub migration_steps: Vec<MigrationStepReport>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationError {
    IncompatibleSchemaVersion { seen: u32, supported: u32 },
    MigrationError(MigrationFailureReport),
    Storage { message: &'static str },
}

impl Display for MigrationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompatibleSchemaVersion { seen, supported } => {
                write!(f, "database schema version {seen} is incompatible with supported version {supported}")
            }
            Self::MigrationError(report) => write!(
                f,
                "schema migration failed at step {}",
                report.migration_steps.last().map_or(0, |step| step.step_id)
            ),
            Self::Storage { message } => write!(f, "schema storage error: {message}"),
        }
    }
}

impl std::error::Error for MigrationError {}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        step_id: 1,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_schema_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)",
    },
    Migration {
        step_id: 2,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_migrations(step_id INTEGER PRIMARY KEY, applied_at_ms INTEGER NOT NULL);
              CREATE TABLE IF NOT EXISTS canonical_nodes(write_cursor INTEGER NOT NULL, kind TEXT NOT NULL, body TEXT NOT NULL);
              CREATE TABLE IF NOT EXISTS canonical_edges(write_cursor INTEGER NOT NULL, kind TEXT NOT NULL, from_id TEXT NOT NULL, to_id TEXT NOT NULL);",
    },
    Migration {
        step_id: 3,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_embedder_profiles(profile TEXT PRIMARY KEY, name TEXT NOT NULL, revision TEXT NOT NULL, dimension INTEGER NOT NULL)",
    },
    Migration {
        step_id: 4,
        sql: "CREATE TABLE IF NOT EXISTS operational_collections(
                  name TEXT PRIMARY KEY,
                  kind TEXT NOT NULL CHECK(kind IN ('append_only_log', 'latest_state')),
                  schema_json TEXT NOT NULL,
                  retention_json TEXT NOT NULL,
                  format_version INTEGER NOT NULL,
                  created_at INTEGER NOT NULL
              );
              CREATE TABLE IF NOT EXISTS operational_mutations(
                  id INTEGER PRIMARY KEY AUTOINCREMENT,
                  collection_name TEXT NOT NULL,
                  record_key TEXT NOT NULL,
                  op_kind TEXT NOT NULL CHECK(op_kind = 'append'),
                  payload_json TEXT NOT NULL,
                  schema_id TEXT,
                  write_cursor INTEGER NOT NULL
              );
              CREATE TABLE IF NOT EXISTS operational_state(
                  collection_name TEXT NOT NULL,
                  record_key TEXT NOT NULL,
                  payload_json TEXT NOT NULL,
                  schema_id TEXT,
                  write_cursor INTEGER NOT NULL,
                  PRIMARY KEY(collection_name, record_key)
              );
              CREATE TABLE IF NOT EXISTS _fathomdb_open_state(key TEXT PRIMARY KEY, value TEXT NOT NULL);
              INSERT OR IGNORE INTO operational_collections(
                  name, kind, schema_json, retention_json, format_version, created_at
              ) VALUES (
                  'projection_failures',
                  'append_only_log',
                  '{\"type\":\"object\"}',
                  '{}',
                  1,
                  0
              );",
    },
    Migration {
        step_id: 5,
        sql: "CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
                  body,
                  kind UNINDEXED,
                  write_cursor UNINDEXED
              );",
    },
    Migration {
        step_id: 6,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_projection_state(
                  kind TEXT PRIMARY KEY,
                  last_enqueued_cursor INTEGER NOT NULL DEFAULT 0,
                  updated_at INTEGER NOT NULL DEFAULT 0
              );
              CREATE TABLE IF NOT EXISTS _fathomdb_vector_kinds(
                  kind TEXT PRIMARY KEY,
                  profile TEXT NOT NULL,
                  created_at INTEGER NOT NULL DEFAULT 0
              );
              CREATE TABLE IF NOT EXISTS _fathomdb_vector_rows(
                  rowid INTEGER PRIMARY KEY,
                  kind TEXT NOT NULL,
                  write_cursor INTEGER NOT NULL UNIQUE
              );",
    },
    Migration {
        step_id: 7,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_projection_terminal(
                  write_cursor INTEGER PRIMARY KEY,
                  state TEXT NOT NULL CHECK(state IN ('failed', 'up_to_date'))
              );",
    },
];

pub fn migrate(conn: &Connection) -> Result<MigrationReport, MigrationError> {
    migrate_with_steps(conn, MIGRATIONS)
}

pub fn migrate_with_steps(
    conn: &Connection,
    migrations: &[Migration],
) -> Result<MigrationReport, MigrationError> {
    migrate_with_event_sink(conn, migrations, |_| {})
}

pub fn migrate_with_event_sink(
    conn: &Connection,
    migrations: &[Migration],
    mut emit: impl FnMut(&MigrationStepReport),
) -> Result<MigrationReport, MigrationError> {
    let before = user_version(conn)?;
    if before > SCHEMA_VERSION {
        return Err(MigrationError::IncompatibleSchemaVersion {
            seen: before,
            supported: SCHEMA_VERSION,
        });
    }

    let mut current = before;
    let mut reports = Vec::new();

    for migration in migrations.iter().filter(|migration| migration.step_id > before) {
        if migration.step_id != current.saturating_add(1) {
            return Err(MigrationError::Storage {
                message: "migration registry is not contiguous",
            });
        }

        let started = Instant::now();
        if let Err(_err) = apply_one(conn, migration) {
            reports.push(MigrationStepReport {
                step_id: migration.step_id,
                duration_ms: Some(duration_ms(started)),
                failed: true,
            });
            emit(reports.last().expect("failed step report was just pushed"));
            let schema_version_current = user_version(conn).unwrap_or(current);
            return Err(MigrationError::MigrationError(MigrationFailureReport {
                schema_version_before: before,
                schema_version_current,
                migration_steps: reports,
            }));
        }

        current = migration.step_id;
        reports.push(MigrationStepReport {
            step_id: migration.step_id,
            duration_ms: Some(duration_ms(started)),
            failed: false,
        });
        emit(reports.last().expect("successful step report was just pushed"));
    }

    Ok(MigrationReport {
        schema_version_before: before,
        schema_version_after: user_version(conn)?,
        migration_steps: reports,
    })
}

fn apply_one(conn: &Connection, migration: &Migration) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        conn.execute_batch(migration.sql)?;
        conn.pragma_update(None, PRAGMA_USER_VERSION, migration.step_id)?;
        Ok(())
    })();

    match result {
        Ok(()) => conn.execute_batch("COMMIT"),
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

fn user_version(conn: &Connection) -> Result<u32, MigrationError> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .map_err(|_| MigrationError::Storage { message: "could not read schema version" })
}

fn duration_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationAccretionError {
    pub offender: String,
}

impl Display for MigrationAccretionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "migration accretion guard rejected {}", self.offender)
    }
}

impl std::error::Error for MigrationAccretionError {}

pub fn check_migration_accretion(name: &str, sql: &str) -> Result<(), MigrationAccretionError> {
    let upper = sql.to_ascii_uppercase();
    let adds_schema = upper.contains("CREATE TABLE") || upper.contains("ADD COLUMN");
    let names_removal = upper.contains("DROP TABLE") || upper.contains("DROP COLUMN");
    let has_exemption = sql.contains("-- MIGRATION-ACCRETION-EXEMPTION: ");

    if adds_schema && !names_removal && !has_exemption {
        return Err(MigrationAccretionError { offender: name.to_string() });
    }

    Ok(())
}
