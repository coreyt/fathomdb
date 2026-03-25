use rusqlite::{Connection, OptionalExtension};

use crate::{Migration, SchemaError, SchemaVersion};

static MIGRATIONS: &[Migration] = &[Migration::new(
    SchemaVersion(1),
    "initial canonical schema and runtime tables",
    r"
                CREATE TABLE IF NOT EXISTS nodes (
                    row_id TEXT PRIMARY KEY,
                    logical_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    properties BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    superseded_at INTEGER,
                    source_ref TEXT,
                    confidence REAL
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_active_logical_id
                    ON nodes(logical_id)
                    WHERE superseded_at IS NULL;
                CREATE INDEX IF NOT EXISTS idx_nodes_kind_active
                    ON nodes(kind, superseded_at);
                CREATE INDEX IF NOT EXISTS idx_nodes_source_ref
                    ON nodes(source_ref);

                CREATE TABLE IF NOT EXISTS edges (
                    row_id TEXT PRIMARY KEY,
                    logical_id TEXT NOT NULL,
                    source_logical_id TEXT NOT NULL,
                    target_logical_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    properties BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    superseded_at INTEGER,
                    source_ref TEXT,
                    confidence REAL
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_edges_active_logical_id
                    ON edges(logical_id)
                    WHERE superseded_at IS NULL;
                CREATE INDEX IF NOT EXISTS idx_edges_source_active
                    ON edges(source_logical_id, kind, superseded_at);
                CREATE INDEX IF NOT EXISTS idx_edges_target_active
                    ON edges(target_logical_id, kind, superseded_at);
                CREATE INDEX IF NOT EXISTS idx_edges_source_ref
                    ON edges(source_ref);

                CREATE TABLE IF NOT EXISTS chunks (
                    id TEXT PRIMARY KEY,
                    node_logical_id TEXT NOT NULL,
                    text_content TEXT NOT NULL,
                    byte_start INTEGER,
                    byte_end INTEGER,
                    created_at INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_chunks_node_logical_id
                    ON chunks(node_logical_id);

                CREATE VIRTUAL TABLE IF NOT EXISTS fts_nodes USING fts5(
                    chunk_id UNINDEXED,
                    node_logical_id UNINDEXED,
                    kind UNINDEXED,
                    text_content
                );

                CREATE TABLE IF NOT EXISTS vector_profiles (
                    profile TEXT PRIMARY KEY,
                    table_name TEXT NOT NULL,
                    dimension INTEGER NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS runs (
                    id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    properties BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    completed_at INTEGER,
                    superseded_at INTEGER,
                    source_ref TEXT
                );

                CREATE TABLE IF NOT EXISTS steps (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    properties BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    completed_at INTEGER,
                    superseded_at INTEGER,
                    source_ref TEXT,
                    FOREIGN KEY(run_id) REFERENCES runs(id)
                );

                CREATE TABLE IF NOT EXISTS actions (
                    id TEXT PRIMARY KEY,
                    step_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    properties BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    completed_at INTEGER,
                    superseded_at INTEGER,
                    source_ref TEXT,
                    FOREIGN KEY(step_id) REFERENCES steps(id)
                );

                CREATE INDEX IF NOT EXISTS idx_runs_source_ref
                    ON runs(source_ref);
                CREATE INDEX IF NOT EXISTS idx_steps_source_ref
                    ON steps(source_ref);
                CREATE INDEX IF NOT EXISTS idx_actions_source_ref
                    ON actions(source_ref);
                ",
)];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapReport {
    pub sqlite_version: String,
    pub applied_versions: Vec<SchemaVersion>,
    pub vector_profile_enabled: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SchemaManager;

impl SchemaManager {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Bootstrap the database schema, applying any pending migrations.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] if any migration or metadata-table SQL fails.
    pub fn bootstrap(&self, conn: &Connection) -> Result<BootstrapReport, SchemaError> {
        self.initialize_connection(conn)?;
        Self::ensure_metadata_tables(conn)?;

        let mut applied_versions = Vec::new();
        for migration in self.migrations() {
            let already_applied = conn
                .query_row(
                    "SELECT 1 FROM fathom_schema_migrations WHERE version = ?1",
                    [i64::from(migration.version.0)],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .is_some();

            if already_applied {
                continue;
            }

            conn.execute_batch(migration.sql)?;
            conn.execute(
                "INSERT INTO fathom_schema_migrations (version, description) VALUES (?1, ?2)",
                (i64::from(migration.version.0), migration.description),
            )?;
            applied_versions.push(migration.version);
        }

        let sqlite_version = conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
        let vector_profile_count: i64 = conn.query_row(
            "SELECT count(*) FROM vector_profiles WHERE enabled = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(BootstrapReport {
            sqlite_version,
            applied_versions,
            vector_profile_enabled: vector_profile_count > 0,
        })
    }

    #[must_use]
    pub fn current_version(&self) -> SchemaVersion {
        self.migrations()
            .last()
            .map_or(SchemaVersion(0), |migration| migration.version)
    }

    #[must_use]
    pub fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }

    /// Set the recommended `SQLite` connection pragmas for fathomdb.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] if any PRAGMA fails to execute.
    pub fn initialize_connection(&self, conn: &Connection) -> Result<(), SchemaError> {
        conn.execute_batch(
            r"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA temp_store = MEMORY;
            PRAGMA mmap_size = 3000000000;
            ",
        )?;
        Ok(())
    }

    /// Ensure the sqlite-vec vector extension profile is registered and the
    /// virtual vec table exists.
    ///
    /// When the `sqlite-vec` feature is enabled this creates the virtual table
    /// and records the profile in `vector_profiles` (with `enabled = 1`).
    /// When the feature is absent the call always returns
    /// [`SchemaError::MissingCapability`].
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] if the DDL fails or the feature is absent.
    #[cfg(feature = "sqlite-vec")]
    pub fn ensure_vector_profile(
        &self,
        conn: &Connection,
        profile: &str,
        table_name: &str,
        dimension: usize,
    ) -> Result<(), SchemaError> {
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS {table_name} USING vec0(\
                chunk_id TEXT PRIMARY KEY,\
                embedding float[{dimension}]\
            )"
        ))?;
        conn.execute(
            "INSERT OR REPLACE INTO vector_profiles \
             (profile, table_name, dimension, enabled) VALUES (?1, ?2, ?3, 1)",
            rusqlite::params![profile, table_name, dimension as i64],
        )?;
        Ok(())
    }

    /// # Errors
    ///
    /// Always returns [`SchemaError::MissingCapability`] when the `sqlite-vec`
    /// feature is not compiled in.
    #[cfg(not(feature = "sqlite-vec"))]
    pub fn ensure_vector_profile(
        &self,
        _conn: &Connection,
        _profile: &str,
        _table_name: &str,
        _dimension: usize,
    ) -> Result<(), SchemaError> {
        Err(SchemaError::MissingCapability("sqlite-vec"))
    }

    /// Create the internal migration-tracking table if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] if the DDL fails to execute.
    fn ensure_metadata_tables(conn: &Connection) -> Result<(), SchemaError> {
        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS fathom_schema_migrations (
                version INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                applied_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            ",
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use rusqlite::Connection;

    use super::SchemaManager;

    #[test]
    fn bootstrap_applies_initial_schema() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();

        let report = manager.bootstrap(&conn).expect("bootstrap report");

        assert_eq!(report.applied_versions.len(), 1);
        assert!(report.sqlite_version.starts_with('3'));
        let table_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'nodes'",
                [],
                |row| row.get(0),
            )
            .expect("nodes table exists");
        assert_eq!(table_count, 1);
    }

    // --- Item 2: vector profile tests ---

    #[test]
    fn vector_profile_not_enabled_without_feature() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        let report = manager.bootstrap(&conn).expect("bootstrap");
        assert!(
            !report.vector_profile_enabled,
            "vector_profile_enabled must be false on a fresh bootstrap"
        );
    }

    #[test]
    fn vector_profile_skipped_when_dimension_absent() {
        // ensure_vector_profile is never called → enabled stays false
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("bootstrap");

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vector_profiles WHERE enabled = 1",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 0, "no enabled profile without calling ensure_vector_profile");
    }

    #[test]
    fn bootstrap_report_reflects_actual_vector_state() {
        // After a fresh bootstrap with no vector profile, the report reflects reality.
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        let report = manager.bootstrap(&conn).expect("bootstrap");

        let db_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vector_profiles WHERE enabled = 1",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(
            report.vector_profile_enabled,
            db_count > 0,
            "BootstrapReport.vector_profile_enabled must match actual DB state"
        );
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn vector_profile_created_when_feature_enabled() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("bootstrap");

        manager
            .ensure_vector_profile(&conn, "default", "vec_nodes_active", 128)
            .expect("ensure_vector_profile");

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vector_profiles WHERE enabled = 1",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1, "vector profile must be enabled after ensure_vector_profile");

        // Verify the virtual table exists by querying it
        let _: i64 = conn
            .query_row("SELECT count(*) FROM vec_nodes_active", [], |row| row.get(0))
            .expect("vec_nodes_active table must exist after ensure_vector_profile");
    }
}
