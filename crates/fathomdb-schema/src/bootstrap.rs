use rusqlite::{Connection, OptionalExtension};

use crate::{Migration, SchemaError, SchemaVersion};

static MIGRATIONS: &[Migration] = &[
    Migration::new(
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
    ),
    Migration::new(
        SchemaVersion(2),
        "durable audit trail: provenance_events table",
        r"
                CREATE TABLE IF NOT EXISTS provenance_events (
                    id         TEXT PRIMARY KEY,
                    event_type TEXT NOT NULL,
                    subject    TEXT NOT NULL,
                    source_ref TEXT,
                    created_at INTEGER NOT NULL DEFAULT (unixepoch())
                );
                CREATE INDEX IF NOT EXISTS idx_provenance_events_subject
                    ON provenance_events (subject, event_type);
                ",
    ),
    Migration::new(
        SchemaVersion(3),
        "vector regeneration contracts",
        r"
                CREATE TABLE IF NOT EXISTS vector_embedding_contracts (
                    profile TEXT PRIMARY KEY,
                    table_name TEXT NOT NULL,
                    model_identity TEXT NOT NULL,
                    model_version TEXT NOT NULL,
                    dimension INTEGER NOT NULL,
                    normalization_policy TEXT NOT NULL,
                    chunking_policy TEXT NOT NULL,
                    preprocessing_policy TEXT NOT NULL,
                    generator_command_json TEXT NOT NULL,
                    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
                );
                ",
    ),
    Migration::new(
        SchemaVersion(4),
        "vector regeneration apply metadata",
        r"
                ALTER TABLE vector_embedding_contracts
                    ADD COLUMN applied_at INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE vector_embedding_contracts
                    ADD COLUMN snapshot_hash TEXT NOT NULL DEFAULT '';
                UPDATE vector_embedding_contracts
                SET
                    applied_at = CASE
                        WHEN applied_at = 0 THEN updated_at
                        ELSE applied_at
                    END,
                    snapshot_hash = CASE
                        WHEN snapshot_hash = '' THEN 'legacy'
                        ELSE snapshot_hash
                    END;
                ",
    ),
    Migration::new(
        SchemaVersion(5),
        "vector regeneration contract format version",
        r"
                ALTER TABLE vector_embedding_contracts
                    ADD COLUMN contract_format_version INTEGER NOT NULL DEFAULT 1;
                UPDATE vector_embedding_contracts
                SET contract_format_version = 1
                WHERE contract_format_version = 0;
                ",
    ),
    Migration::new(
        SchemaVersion(6),
        "provenance metadata payloads",
        r"
                ALTER TABLE provenance_events
                    ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '';
                ",
    ),
    Migration::new(
        SchemaVersion(7),
        "operational store canonical and derived tables",
        r"
                CREATE TABLE IF NOT EXISTS operational_collections (
                    name TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    schema_json TEXT NOT NULL,
                    retention_json TEXT NOT NULL,
                    format_version INTEGER NOT NULL DEFAULT 1,
                    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                    disabled_at INTEGER
                );

                CREATE INDEX IF NOT EXISTS idx_operational_collections_kind
                    ON operational_collections(kind, disabled_at);

                CREATE TABLE IF NOT EXISTS operational_mutations (
                    id TEXT PRIMARY KEY,
                    collection_name TEXT NOT NULL,
                    record_key TEXT NOT NULL,
                    op_kind TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    source_ref TEXT,
                    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                    FOREIGN KEY(collection_name) REFERENCES operational_collections(name)
                );

                CREATE INDEX IF NOT EXISTS idx_operational_mutations_collection_key_created
                    ON operational_mutations(collection_name, record_key, created_at DESC, id DESC);
                CREATE INDEX IF NOT EXISTS idx_operational_mutations_source_ref
                    ON operational_mutations(source_ref);

                CREATE TABLE IF NOT EXISTS operational_current (
                    collection_name TEXT NOT NULL,
                    record_key TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    updated_at INTEGER NOT NULL,
                    last_mutation_id TEXT NOT NULL,
                    PRIMARY KEY(collection_name, record_key),
                    FOREIGN KEY(collection_name) REFERENCES operational_collections(name),
                    FOREIGN KEY(last_mutation_id) REFERENCES operational_mutations(id)
                );

                CREATE INDEX IF NOT EXISTS idx_operational_current_collection_updated
                    ON operational_current(collection_name, updated_at DESC);
                ",
    ),
    Migration::new(
        SchemaVersion(8),
        "operational mutation ordering hardening",
        r"
                ALTER TABLE operational_mutations
                    ADD COLUMN mutation_order INTEGER NOT NULL DEFAULT 0;
                UPDATE operational_mutations
                SET mutation_order = rowid
                WHERE mutation_order = 0;
                CREATE INDEX IF NOT EXISTS idx_operational_mutations_collection_key_order
                    ON operational_mutations(collection_name, record_key, mutation_order DESC);
                ",
    ),
    Migration::new(
        SchemaVersion(9),
        "node last_accessed metadata",
        r"
                CREATE TABLE IF NOT EXISTS node_access_metadata (
                    logical_id TEXT PRIMARY KEY,
                    last_accessed_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_node_access_metadata_last_accessed
                    ON node_access_metadata(last_accessed_at DESC);
                ",
    ),
    Migration::new(
        SchemaVersion(10),
        "operational filtered read contracts and extracted values",
        r"
                ALTER TABLE operational_collections
                    ADD COLUMN filter_fields_json TEXT NOT NULL DEFAULT '[]';

                CREATE TABLE IF NOT EXISTS operational_filter_values (
                    mutation_id TEXT NOT NULL,
                    collection_name TEXT NOT NULL,
                    field_name TEXT NOT NULL,
                    string_value TEXT,
                    integer_value INTEGER,
                    PRIMARY KEY(mutation_id, field_name),
                    FOREIGN KEY(mutation_id) REFERENCES operational_mutations(id) ON DELETE CASCADE,
                    FOREIGN KEY(collection_name) REFERENCES operational_collections(name)
                );

                CREATE INDEX IF NOT EXISTS idx_operational_filter_values_text
                    ON operational_filter_values(collection_name, field_name, string_value, mutation_id);
                CREATE INDEX IF NOT EXISTS idx_operational_filter_values_integer
                    ON operational_filter_values(collection_name, field_name, integer_value, mutation_id);
                ",
    ),
    Migration::new(
        SchemaVersion(11),
        "operational payload validation contracts",
        r"
                ALTER TABLE operational_collections
                    ADD COLUMN validation_json TEXT NOT NULL DEFAULT '';
                ",
    ),
    Migration::new(
        SchemaVersion(12),
        "operational secondary indexes",
        r"
                ALTER TABLE operational_collections
                    ADD COLUMN secondary_indexes_json TEXT NOT NULL DEFAULT '[]';

                CREATE TABLE IF NOT EXISTS operational_secondary_index_entries (
                    collection_name TEXT NOT NULL,
                    index_name TEXT NOT NULL,
                    subject_kind TEXT NOT NULL,
                    mutation_id TEXT NOT NULL DEFAULT '',
                    record_key TEXT NOT NULL DEFAULT '',
                    sort_timestamp INTEGER,
                    slot1_text TEXT,
                    slot1_integer INTEGER,
                    slot2_text TEXT,
                    slot2_integer INTEGER,
                    slot3_text TEXT,
                    slot3_integer INTEGER,
                    PRIMARY KEY(collection_name, index_name, subject_kind, mutation_id, record_key),
                    FOREIGN KEY(collection_name) REFERENCES operational_collections(name),
                    FOREIGN KEY(mutation_id) REFERENCES operational_mutations(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_operational_secondary_entries_slot1_text
                    ON operational_secondary_index_entries(
                        collection_name, index_name, subject_kind, slot1_text, sort_timestamp DESC, mutation_id, record_key
                    );
                CREATE INDEX IF NOT EXISTS idx_operational_secondary_entries_slot1_integer
                    ON operational_secondary_index_entries(
                        collection_name, index_name, subject_kind, slot1_integer, sort_timestamp DESC, mutation_id, record_key
                    );
                CREATE INDEX IF NOT EXISTS idx_operational_secondary_entries_composite_text
                    ON operational_secondary_index_entries(
                        collection_name, index_name, subject_kind, slot1_text, slot2_text, slot3_text, sort_timestamp DESC, record_key
                    );
                CREATE INDEX IF NOT EXISTS idx_operational_secondary_entries_composite_integer
                    ON operational_secondary_index_entries(
                        collection_name, index_name, subject_kind, slot1_integer, slot2_integer, slot3_integer, sort_timestamp DESC, record_key
                );
                ",
    ),
    Migration::new(
        SchemaVersion(13),
        "operational retention run metadata",
        r"
                CREATE TABLE IF NOT EXISTS operational_retention_runs (
                    id TEXT PRIMARY KEY,
                    collection_name TEXT NOT NULL,
                    executed_at INTEGER NOT NULL,
                    action_kind TEXT NOT NULL,
                    dry_run INTEGER NOT NULL DEFAULT 0,
                    deleted_mutations INTEGER NOT NULL,
                    rows_remaining INTEGER NOT NULL,
                    metadata_json TEXT NOT NULL DEFAULT '',
                    FOREIGN KEY(collection_name) REFERENCES operational_collections(name)
                );

                CREATE INDEX IF NOT EXISTS idx_operational_retention_runs_collection_time
                    ON operational_retention_runs(collection_name, executed_at DESC);
                ",
    ),
];

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
    /// Returns [`SchemaError`] if any migration or metadata-table SQL fails,
    /// or [`SchemaError::VersionMismatch`] if the database has been migrated
    /// to a version newer than this engine supports.
    pub fn bootstrap(&self, conn: &Connection) -> Result<BootstrapReport, SchemaError> {
        self.initialize_connection(conn)?;
        Self::ensure_metadata_tables(conn)?;

        // Downgrade protection
        let max_applied: u32 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM fathom_schema_migrations",
            [],
            |row| row.get(0),
        )?;
        let engine_version = self.current_version().0;
        trace_info!(
            current_version = max_applied,
            engine_version,
            "schema bootstrap: version check"
        );
        if max_applied > engine_version {
            trace_error!(
                database_version = max_applied,
                engine_version,
                "schema version mismatch: database is newer than engine"
            );
            return Err(SchemaError::VersionMismatch {
                database_version: max_applied,
                engine_version,
            });
        }

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

            let tx = conn.unchecked_transaction()?;
            match migration.version {
                SchemaVersion(4) => Self::ensure_vector_regeneration_apply_metadata(&tx)?,
                SchemaVersion(5) => Self::ensure_vector_contract_format_version(&tx)?,
                SchemaVersion(6) => Self::ensure_provenance_metadata(&tx)?,
                SchemaVersion(8) => Self::ensure_operational_mutation_order(&tx)?,
                SchemaVersion(9) => Self::ensure_node_access_metadata(&tx)?,
                SchemaVersion(10) => Self::ensure_operational_filter_contract(&tx)?,
                SchemaVersion(11) => Self::ensure_operational_validation_contract(&tx)?,
                SchemaVersion(12) => Self::ensure_operational_secondary_indexes(&tx)?,
                SchemaVersion(13) => Self::ensure_operational_retention_runs(&tx)?,
                _ => tx.execute_batch(migration.sql)?,
            }
            tx.execute(
                "INSERT INTO fathom_schema_migrations (version, description) VALUES (?1, ?2)",
                (i64::from(migration.version.0), migration.description),
            )?;
            tx.commit()?;
            trace_info!(
                version = migration.version.0,
                description = migration.description,
                "schema migration applied"
            );
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

    fn ensure_vector_regeneration_apply_metadata(conn: &Connection) -> Result<(), SchemaError> {
        let mut stmt = conn.prepare("PRAGMA table_info(vector_embedding_contracts)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        let has_applied_at = columns.iter().any(|column| column == "applied_at");
        let has_snapshot_hash = columns.iter().any(|column| column == "snapshot_hash");

        if !has_applied_at {
            conn.execute(
                "ALTER TABLE vector_embedding_contracts ADD COLUMN applied_at INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_snapshot_hash {
            conn.execute(
                "ALTER TABLE vector_embedding_contracts ADD COLUMN snapshot_hash TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        conn.execute(
            r"
            UPDATE vector_embedding_contracts
            SET
                applied_at = CASE
                    WHEN applied_at = 0 THEN updated_at
                    ELSE applied_at
                END,
                snapshot_hash = CASE
                    WHEN snapshot_hash = '' THEN 'legacy'
                    ELSE snapshot_hash
                END
            ",
            [],
        )?;
        Ok(())
    }

    fn ensure_vector_contract_format_version(conn: &Connection) -> Result<(), SchemaError> {
        let mut stmt = conn.prepare("PRAGMA table_info(vector_embedding_contracts)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        let has_contract_format_version = columns
            .iter()
            .any(|column| column == "contract_format_version");

        if !has_contract_format_version {
            conn.execute(
                "ALTER TABLE vector_embedding_contracts ADD COLUMN contract_format_version INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }
        conn.execute(
            r"
            UPDATE vector_embedding_contracts
            SET contract_format_version = 1
            WHERE contract_format_version = 0
            ",
            [],
        )?;
        Ok(())
    }

    fn ensure_provenance_metadata(conn: &Connection) -> Result<(), SchemaError> {
        let mut stmt = conn.prepare("PRAGMA table_info(provenance_events)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        let has_metadata_json = columns.iter().any(|column| column == "metadata_json");

        if !has_metadata_json {
            conn.execute(
                "ALTER TABLE provenance_events ADD COLUMN metadata_json TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        Ok(())
    }

    fn ensure_operational_mutation_order(conn: &Connection) -> Result<(), SchemaError> {
        let mut stmt = conn.prepare("PRAGMA table_info(operational_mutations)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        let has_mutation_order = columns.iter().any(|column| column == "mutation_order");

        if !has_mutation_order {
            conn.execute(
                "ALTER TABLE operational_mutations ADD COLUMN mutation_order INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        conn.execute(
            r"
            UPDATE operational_mutations
            SET mutation_order = rowid
            WHERE mutation_order = 0
            ",
            [],
        )?;
        conn.execute(
            r"
            CREATE INDEX IF NOT EXISTS idx_operational_mutations_collection_key_order
                ON operational_mutations(collection_name, record_key, mutation_order DESC)
            ",
            [],
        )?;
        Ok(())
    }

    fn ensure_node_access_metadata(conn: &Connection) -> Result<(), SchemaError> {
        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS node_access_metadata (
                logical_id TEXT PRIMARY KEY,
                last_accessed_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_node_access_metadata_last_accessed
                ON node_access_metadata(last_accessed_at DESC);
            ",
        )?;
        Ok(())
    }

    fn ensure_operational_filter_contract(conn: &Connection) -> Result<(), SchemaError> {
        let mut stmt = conn.prepare("PRAGMA table_info(operational_collections)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        let has_filter_fields_json = columns.iter().any(|column| column == "filter_fields_json");

        if !has_filter_fields_json {
            conn.execute(
                "ALTER TABLE operational_collections ADD COLUMN filter_fields_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }

        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS operational_filter_values (
                mutation_id TEXT NOT NULL,
                collection_name TEXT NOT NULL,
                field_name TEXT NOT NULL,
                string_value TEXT,
                integer_value INTEGER,
                PRIMARY KEY(mutation_id, field_name),
                FOREIGN KEY(mutation_id) REFERENCES operational_mutations(id) ON DELETE CASCADE,
                FOREIGN KEY(collection_name) REFERENCES operational_collections(name)
            );

            CREATE INDEX IF NOT EXISTS idx_operational_filter_values_text
                ON operational_filter_values(collection_name, field_name, string_value, mutation_id);
            CREATE INDEX IF NOT EXISTS idx_operational_filter_values_integer
                ON operational_filter_values(collection_name, field_name, integer_value, mutation_id);
            ",
        )?;
        Ok(())
    }

    fn ensure_operational_validation_contract(conn: &Connection) -> Result<(), SchemaError> {
        let mut stmt = conn.prepare("PRAGMA table_info(operational_collections)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        let has_validation_json = columns.iter().any(|column| column == "validation_json");

        if !has_validation_json {
            conn.execute(
                "ALTER TABLE operational_collections ADD COLUMN validation_json TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }

        Ok(())
    }

    fn ensure_operational_secondary_indexes(conn: &Connection) -> Result<(), SchemaError> {
        let mut stmt = conn.prepare("PRAGMA table_info(operational_collections)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        let has_secondary_indexes_json = columns
            .iter()
            .any(|column| column == "secondary_indexes_json");

        if !has_secondary_indexes_json {
            conn.execute(
                "ALTER TABLE operational_collections ADD COLUMN secondary_indexes_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }

        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS operational_secondary_index_entries (
                collection_name TEXT NOT NULL,
                index_name TEXT NOT NULL,
                subject_kind TEXT NOT NULL,
                mutation_id TEXT NOT NULL DEFAULT '',
                record_key TEXT NOT NULL DEFAULT '',
                sort_timestamp INTEGER,
                slot1_text TEXT,
                slot1_integer INTEGER,
                slot2_text TEXT,
                slot2_integer INTEGER,
                slot3_text TEXT,
                slot3_integer INTEGER,
                PRIMARY KEY(collection_name, index_name, subject_kind, mutation_id, record_key),
                FOREIGN KEY(collection_name) REFERENCES operational_collections(name),
                FOREIGN KEY(mutation_id) REFERENCES operational_mutations(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_operational_secondary_entries_slot1_text
                ON operational_secondary_index_entries(
                    collection_name, index_name, subject_kind, slot1_text, sort_timestamp DESC, mutation_id, record_key
                );
            CREATE INDEX IF NOT EXISTS idx_operational_secondary_entries_slot1_integer
                ON operational_secondary_index_entries(
                    collection_name, index_name, subject_kind, slot1_integer, sort_timestamp DESC, mutation_id, record_key
                );
            CREATE INDEX IF NOT EXISTS idx_operational_secondary_entries_composite_text
                ON operational_secondary_index_entries(
                    collection_name, index_name, subject_kind, slot1_text, slot2_text, slot3_text, sort_timestamp DESC, record_key
                );
            CREATE INDEX IF NOT EXISTS idx_operational_secondary_entries_composite_integer
                ON operational_secondary_index_entries(
                    collection_name, index_name, subject_kind, slot1_integer, slot2_integer, slot3_integer, sort_timestamp DESC, record_key
                );
            ",
        )?;

        Ok(())
    }

    fn ensure_operational_retention_runs(conn: &Connection) -> Result<(), SchemaError> {
        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS operational_retention_runs (
                id TEXT PRIMARY KEY,
                collection_name TEXT NOT NULL,
                executed_at INTEGER NOT NULL,
                action_kind TEXT NOT NULL,
                dry_run INTEGER NOT NULL DEFAULT 0,
                deleted_mutations INTEGER NOT NULL,
                rows_remaining INTEGER NOT NULL,
                metadata_json TEXT NOT NULL DEFAULT '',
                FOREIGN KEY(collection_name) REFERENCES operational_collections(name)
            );

            CREATE INDEX IF NOT EXISTS idx_operational_retention_runs_collection_time
                ON operational_retention_runs(collection_name, executed_at DESC);
            ",
        )?;
        Ok(())
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
            PRAGMA journal_size_limit = 536870912;
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

        assert_eq!(
            report.applied_versions.len(),
            manager.current_version().0 as usize
        );
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
        assert_eq!(
            count, 0,
            "no enabled profile without calling ensure_vector_profile"
        );
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

    #[test]
    fn bootstrap_backfills_vector_contract_format_and_provenance_metadata_columns() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE provenance_events (
                id         TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                subject    TEXT NOT NULL,
                source_ref TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE TABLE vector_embedding_contracts (
                profile TEXT PRIMARY KEY,
                table_name TEXT NOT NULL,
                model_identity TEXT NOT NULL,
                model_version TEXT NOT NULL,
                dimension INTEGER NOT NULL,
                normalization_policy TEXT NOT NULL,
                chunking_policy TEXT NOT NULL,
                preprocessing_policy TEXT NOT NULL,
                generator_command_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
                applied_at INTEGER NOT NULL DEFAULT 0,
                snapshot_hash TEXT NOT NULL DEFAULT ''
            );
            INSERT INTO vector_embedding_contracts (
                profile,
                table_name,
                model_identity,
                model_version,
                dimension,
                normalization_policy,
                chunking_policy,
                preprocessing_policy,
                generator_command_json,
                updated_at,
                applied_at,
                snapshot_hash
            ) VALUES (
                'default',
                'vec_nodes_active',
                'legacy-model',
                '0.9.0',
                4,
                'l2',
                'per_chunk',
                'trim',
                '["/bin/echo"]',
                100,
                100,
                'legacy'
            );
            "#,
        )
        .expect("seed legacy schema");
        let manager = SchemaManager::new();

        let report = manager.bootstrap(&conn).expect("bootstrap");

        assert!(
            report.applied_versions.iter().any(|version| version.0 >= 5),
            "bootstrap should apply hardening migrations"
        );
        let format_version: i64 = conn
            .query_row(
                "SELECT contract_format_version FROM vector_embedding_contracts WHERE profile = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("contract_format_version");
        assert_eq!(format_version, 1);
        let metadata_column_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM pragma_table_info('provenance_events') WHERE name = 'metadata_json'",
                [],
                |row| row.get(0),
            )
            .expect("metadata_json column count");
        assert_eq!(metadata_column_count, 1);
    }

    #[test]
    fn bootstrap_creates_operational_store_tables() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();

        manager.bootstrap(&conn).expect("bootstrap");

        for table in [
            "operational_collections",
            "operational_mutations",
            "operational_current",
        ] {
            let count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .expect("table existence");
            assert_eq!(count, 1, "{table} should exist after bootstrap");
        }
        let mutation_order_columns: i64 = conn
            .query_row(
                "SELECT count(*) FROM pragma_table_info('operational_mutations') WHERE name = 'mutation_order'",
                [],
                |row| row.get(0),
            )
            .expect("mutation_order column exists");
        assert_eq!(mutation_order_columns, 1);
    }

    #[test]
    fn bootstrap_is_idempotent_with_operational_store_tables() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();

        manager.bootstrap(&conn).expect("first bootstrap");
        let report = manager.bootstrap(&conn).expect("second bootstrap");

        assert!(
            report.applied_versions.is_empty(),
            "second bootstrap should apply no new migrations"
        );
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'operational_collections'",
                [],
                |row| row.get(0),
        )
        .expect("operational_collections table exists");
        assert_eq!(count, 1);
    }

    #[test]
    fn bootstrap_is_idempotent_for_recovered_operational_tables_without_migration_history() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE operational_collections (
                name TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                schema_json TEXT NOT NULL,
                retention_json TEXT NOT NULL,
                format_version INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                disabled_at INTEGER
            );

            CREATE TABLE operational_mutations (
                id TEXT PRIMARY KEY,
                collection_name TEXT NOT NULL,
                record_key TEXT NOT NULL,
                op_kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                source_ref TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                mutation_order INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(collection_name) REFERENCES operational_collections(name)
            );

            CREATE TABLE operational_current (
                collection_name TEXT NOT NULL,
                record_key TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                last_mutation_id TEXT NOT NULL,
                PRIMARY KEY(collection_name, record_key),
                FOREIGN KEY(collection_name) REFERENCES operational_collections(name),
                FOREIGN KEY(last_mutation_id) REFERENCES operational_mutations(id)
            );

            INSERT INTO operational_collections (name, kind, schema_json, retention_json)
            VALUES ('audit_log', 'append_only_log', '{}', '{"mode":"keep_all"}');
            INSERT INTO operational_mutations (
                id, collection_name, record_key, op_kind, payload_json, created_at, mutation_order
            ) VALUES (
                'mut-1', 'audit_log', 'entry-1', 'put', '{"ok":true}', 10, 0
            );
            "#,
        )
        .expect("seed recovered operational tables");

        let manager = SchemaManager::new();
        let report = manager
            .bootstrap(&conn)
            .expect("bootstrap recovered schema");

        assert!(
            report.applied_versions.iter().any(|version| version.0 == 8),
            "bootstrap should record operational mutation ordering hardening"
        );
        let mutation_order: i64 = conn
            .query_row(
                "SELECT mutation_order FROM operational_mutations WHERE id = 'mut-1'",
                [],
                |row| row.get(0),
            )
            .expect("mutation_order");
        assert_ne!(
            mutation_order, 0,
            "bootstrap should backfill recovered operational rows"
        );
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_operational_mutations_collection_key_order'",
                [],
                |row| row.get(0),
            )
            .expect("ordering index exists");
        assert_eq!(count, 1);
    }

    #[test]
    fn bootstrap_adds_operational_filter_contract_and_index_table() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE operational_collections (
                name TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                schema_json TEXT NOT NULL,
                retention_json TEXT NOT NULL,
                format_version INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                disabled_at INTEGER
            );

            CREATE TABLE operational_mutations (
                id TEXT PRIMARY KEY,
                collection_name TEXT NOT NULL,
                record_key TEXT NOT NULL,
                op_kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                source_ref TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                mutation_order INTEGER NOT NULL DEFAULT 1,
                FOREIGN KEY(collection_name) REFERENCES operational_collections(name)
            );

            INSERT INTO operational_collections (name, kind, schema_json, retention_json)
            VALUES ('audit_log', 'append_only_log', '{}', '{"mode":"keep_all"}');
            "#,
        )
        .expect("seed recovered operational schema");

        let manager = SchemaManager::new();
        let report = manager
            .bootstrap(&conn)
            .expect("bootstrap recovered schema");

        assert!(
            report
                .applied_versions
                .iter()
                .any(|version| version.0 == 10),
            "bootstrap should record operational filtered read migration"
        );
        assert!(
            report
                .applied_versions
                .iter()
                .any(|version| version.0 == 11),
            "bootstrap should record operational validation migration"
        );
        let filter_fields_json: String = conn
            .query_row(
                "SELECT filter_fields_json FROM operational_collections WHERE name = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("filter_fields_json added");
        assert_eq!(filter_fields_json, "[]");
        let validation_json: String = conn
            .query_row(
                "SELECT validation_json FROM operational_collections WHERE name = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("validation_json added");
        assert_eq!(validation_json, "");
        let table_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'operational_filter_values'",
                [],
                |row| row.get(0),
        )
        .expect("filter table exists");
        assert_eq!(table_count, 1);
    }

    #[test]
    fn bootstrap_reapplies_migration_history_without_readding_filter_contract_columns() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("initial bootstrap");

        conn.execute("DROP TABLE fathom_schema_migrations", [])
            .expect("drop migration history");
        SchemaManager::ensure_metadata_tables(&conn).expect("recreate migration metadata");

        let report = manager
            .bootstrap(&conn)
            .expect("rebootstrap existing schema");

        assert!(
            report
                .applied_versions
                .iter()
                .any(|version| version.0 == 10),
            "rebootstrap should re-record migration 10"
        );
        assert!(
            report
                .applied_versions
                .iter()
                .any(|version| version.0 == 11),
            "rebootstrap should re-record migration 11"
        );
        let filter_fields_json: String = conn
            .query_row(
                "SELECT filter_fields_json FROM operational_collections LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "[]".to_string());
        assert_eq!(filter_fields_json, "[]");
        let validation_json: String = conn
            .query_row(
                "SELECT validation_json FROM operational_collections LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or_default();
        assert_eq!(validation_json, "");
    }

    #[test]
    fn downgrade_detected_returns_version_mismatch() {
        use crate::SchemaError;

        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("initial bootstrap");

        conn.execute(
            "INSERT INTO fathom_schema_migrations (version, description) VALUES (?1, ?2)",
            (999_i64, "future migration"),
        )
        .expect("insert future version");

        let err = manager
            .bootstrap(&conn)
            .expect_err("should fail on downgrade");
        assert!(
            matches!(
                err,
                SchemaError::VersionMismatch {
                    database_version: 999,
                    ..
                }
            ),
            "expected VersionMismatch with database_version 999, got: {err}"
        );
    }

    #[test]
    fn journal_size_limit_is_set() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager
            .initialize_connection(&conn)
            .expect("initialize_connection");

        let limit: i64 = conn
            .query_row("PRAGMA journal_size_limit", [], |row| row.get(0))
            .expect("journal_size_limit pragma");
        assert_eq!(limit, 536_870_912);
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn vector_profile_created_when_feature_enabled() {
        // Register the sqlite-vec extension globally so the in-memory
        // connection can use the vec0 module.
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
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
        assert_eq!(
            count, 1,
            "vector profile must be enabled after ensure_vector_profile"
        );

        // Verify the virtual table exists by querying it
        let _: i64 = conn
            .query_row("SELECT count(*) FROM vec_nodes_active", [], |row| {
                row.get(0)
            })
            .expect("vec_nodes_active table must exist after ensure_vector_profile");
    }
}
