use rusqlite::{Connection, OptionalExtension};

use crate::{Migration, SchemaError, SchemaVersion};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapReport {
    pub sqlite_version: String,
    pub applied_versions: Vec<SchemaVersion>,
    pub vector_profile_enabled: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SchemaManager;

impl SchemaManager {
    pub fn new() -> Self {
        Self
    }

    pub fn bootstrap(&self, conn: &Connection) -> Result<BootstrapReport, SchemaError> {
        self.initialize_connection(conn)?;
        self.ensure_metadata_tables(conn)?;

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
        Ok(BootstrapReport {
            sqlite_version,
            applied_versions,
            vector_profile_enabled: false,
        })
    }

    pub fn current_version(&self) -> SchemaVersion {
        self.migrations()
            .last()
            .map(|migration| migration.version)
            .unwrap_or(SchemaVersion(0))
    }

    pub fn migrations(&self) -> &'static [Migration] {
        &[
            Migration::new(
                SchemaVersion(1),
                "initial canonical schema and runtime tables",
                r#"
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
                "#,
            ),
        ]
    }

    pub fn initialize_connection(&self, conn: &Connection) -> Result<(), SchemaError> {
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA temp_store = MEMORY;
            PRAGMA mmap_size = 3000000000;
            "#,
        )?;
        Ok(())
    }

    pub fn ensure_vector_profile(
        &self,
        _conn: &Connection,
        _profile: &str,
        _table_name: &str,
        _dimension: usize,
    ) -> Result<(), SchemaError> {
        Err(SchemaError::MissingCapability("sqlite-vec"))
    }

    fn ensure_metadata_tables(&self, conn: &Connection) -> Result<(), SchemaError> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS fathom_schema_migrations (
                version INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                applied_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            "#,
        )?;
        Ok(())
    }
}

#[cfg(test)]
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
}
