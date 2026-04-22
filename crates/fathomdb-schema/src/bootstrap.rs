use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};

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
    Migration::new(
        SchemaVersion(14),
        "external content object columns",
        r"
                ALTER TABLE nodes ADD COLUMN content_ref TEXT;

                CREATE INDEX IF NOT EXISTS idx_nodes_content_ref
                    ON nodes(content_ref)
                    WHERE content_ref IS NOT NULL AND superseded_at IS NULL;

                ALTER TABLE chunks ADD COLUMN content_hash TEXT;
                ",
    ),
    Migration::new(
        SchemaVersion(15),
        "FTS property projection schemas",
        r"
                CREATE TABLE IF NOT EXISTS fts_property_schemas (
                    kind TEXT PRIMARY KEY,
                    property_paths_json TEXT NOT NULL,
                    separator TEXT NOT NULL DEFAULT ' ',
                    format_version INTEGER NOT NULL DEFAULT 1,
                    created_at INTEGER NOT NULL DEFAULT (unixepoch())
                );

                CREATE VIRTUAL TABLE IF NOT EXISTS fts_node_properties USING fts5(
                    node_logical_id UNINDEXED,
                    kind UNINDEXED,
                    text_content
                );
                ",
    ),
    Migration::new(
        SchemaVersion(16),
        "rebuild fts_nodes and fts_node_properties on porter+unicode61 tokenizer",
        // DDL applied by `ensure_unicode_porter_fts_tokenizers`; the hook
        // drops, recreates, and rebuilds both tables from canonical state.
        // The FTS5 chained tokenizer syntax requires the wrapper (porter)
        // before the base tokenizer, so the applied `tokenize=` clause is
        // `'porter unicode61 remove_diacritics 2'`.
        "",
    ),
    Migration::new(
        SchemaVersion(17),
        "fts property position-map sidecar for recursive extraction",
        // Sidecar position map for property FTS. The recursive extraction
        // walk in the engine emits one row per scalar leaf contributing to a
        // given node's property FTS blob, carrying the half-open byte range
        // `[start_offset, end_offset)` within the blob and the JSON-path of
        // the originating leaf. Phase 5 uses this to attribute tokens back
        // to their source leaves.
        //
        // The existing `fts_property_schemas.property_paths_json` column is
        // reused unchanged at the DDL level; the engine-side JSON decoder
        // tolerates both the legacy shape (bare strings = scalar) and the
        // new shape (objects with `mode` = `scalar`|`recursive`, optional
        // `exclude_paths`). Backwards compatibility is guaranteed because a
        // bare JSON array of strings still deserialises into scalar-mode
        // entries.
        r"
                CREATE TABLE IF NOT EXISTS fts_node_property_positions (
                    node_logical_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    start_offset INTEGER NOT NULL,
                    end_offset INTEGER NOT NULL,
                    leaf_path TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_fts_node_property_positions_node
                    ON fts_node_property_positions(node_logical_id, kind);

                CREATE INDEX IF NOT EXISTS idx_fts_node_property_positions_kind
                    ON fts_node_property_positions(kind);
                ",
    ),
    Migration::new(
        SchemaVersion(18),
        "add UNIQUE constraint on fts_node_property_positions (node_logical_id, kind, start_offset)",
        // P4-P2-4: the v17 sidecar DDL did not enforce uniqueness of the
        // `(node_logical_id, kind, start_offset)` tuple, so a buggy rebuild
        // path could silently double-insert a leaf and break attribution
        // lookups. Drop and recreate the table with the UNIQUE constraint,
        // preserving the existing indexes. The DDL leaves the table empty,
        // which the open-time rebuild guard in `ExecutionCoordinator::open`
        // detects (empty positions + recursive schemas present) and
        // repopulates from canonical state on the next open. The rebuild
        // path is idempotent and safe to run unconditionally.
        //
        // `fts_node_property_positions` is a regular SQLite table, not an
        // FTS5 virtual table, so UNIQUE constraints are supported.
        r"
                DROP TABLE IF EXISTS fts_node_property_positions;

                CREATE TABLE fts_node_property_positions (
                    node_logical_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    start_offset INTEGER NOT NULL,
                    end_offset INTEGER NOT NULL,
                    leaf_path TEXT NOT NULL,
                    UNIQUE(node_logical_id, kind, start_offset)
                );

                CREATE INDEX IF NOT EXISTS idx_fts_node_property_positions_node
                    ON fts_node_property_positions(node_logical_id, kind);

                CREATE INDEX IF NOT EXISTS idx_fts_node_property_positions_kind
                    ON fts_node_property_positions(kind);
                ",
    ),
    Migration::new(
        SchemaVersion(19),
        "async property-FTS rebuild staging and state tables",
        r"
                CREATE TABLE IF NOT EXISTS fts_property_rebuild_staging (
                    kind TEXT NOT NULL,
                    node_logical_id TEXT NOT NULL,
                    text_content TEXT NOT NULL,
                    positions_blob BLOB,
                    PRIMARY KEY (kind, node_logical_id)
                );

                CREATE TABLE IF NOT EXISTS fts_property_rebuild_state (
                    kind TEXT PRIMARY KEY,
                    schema_id INTEGER NOT NULL,
                    state TEXT NOT NULL,
                    rows_total INTEGER,
                    rows_done INTEGER NOT NULL DEFAULT 0,
                    started_at INTEGER NOT NULL,
                    last_progress_at INTEGER,
                    error_message TEXT,
                    is_first_registration INTEGER NOT NULL DEFAULT 0
                );
                ",
    ),
    Migration::new(
        SchemaVersion(20),
        "projection_profiles table for per-kind FTS tokenizer configuration",
        r"CREATE TABLE IF NOT EXISTS projection_profiles (
            kind        TEXT    NOT NULL,
            facet       TEXT    NOT NULL,
            config_json TEXT    NOT NULL,
            active_at   INTEGER,
            created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY (kind, facet)
        );",
    ),
    Migration::new(
        SchemaVersion(21),
        "per-kind FTS5 tables replacing fts_node_properties",
        "",
    ),
    Migration::new(
        SchemaVersion(22),
        "add columns_json to fts_property_rebuild_staging for multi-column rebuild support",
        "ALTER TABLE fts_property_rebuild_staging ADD COLUMN columns_json TEXT;",
    ),
    Migration::new(
        SchemaVersion(23),
        "drop global fts_node_properties table (replaced by per-kind fts_props_<kind> tables)",
        "DROP TABLE IF EXISTS fts_node_properties;",
    ),
    // NOTE: Vector identity belongs to the embedder.
    // `vector_embedding_profiles.model_identity` (and `model_version`) is a
    // record of what the embedder reports on activation — it is never a
    // user-authored or user-editable configuration field. The singleton
    // partial unique index below enforces at most one active profile per
    // database. See
    // dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md.
    Migration::new(
        SchemaVersion(24),
        "managed vector projection tables: embedding profiles, per-kind index schemas, async work queue",
        r"
                CREATE TABLE IF NOT EXISTS vector_embedding_profiles (
                    profile_id           INTEGER PRIMARY KEY AUTOINCREMENT,
                    profile_name         TEXT    NOT NULL,
                    model_identity       TEXT    NOT NULL,
                    model_version        TEXT,
                    dimensions           INTEGER NOT NULL,
                    normalization_policy TEXT,
                    max_tokens           INTEGER,
                    active               INTEGER NOT NULL DEFAULT 0,
                    activated_at         INTEGER,
                    created_at           INTEGER NOT NULL
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_vep_singleton_active
                    ON vector_embedding_profiles(active)
                    WHERE active = 1;

                CREATE UNIQUE INDEX IF NOT EXISTS idx_vep_identity
                    ON vector_embedding_profiles(model_identity, model_version, dimensions);

                CREATE TABLE IF NOT EXISTS vector_index_schemas (
                    kind                  TEXT PRIMARY KEY,
                    enabled               INTEGER NOT NULL DEFAULT 1,
                    source_mode           TEXT    NOT NULL,
                    source_config_json    TEXT,
                    chunking_policy       TEXT,
                    preprocessing_policy  TEXT,
                    state                 TEXT    NOT NULL DEFAULT 'fresh',
                    last_error            TEXT,
                    last_completed_at     INTEGER,
                    created_at            INTEGER NOT NULL,
                    updated_at            INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS vector_projection_work (
                    work_id              INTEGER PRIMARY KEY AUTOINCREMENT,
                    kind                 TEXT    NOT NULL,
                    node_logical_id      TEXT,
                    chunk_id             TEXT    NOT NULL,
                    canonical_hash       TEXT    NOT NULL,
                    priority             INTEGER NOT NULL DEFAULT 0,
                    embedding_profile_id INTEGER NOT NULL REFERENCES vector_embedding_profiles(profile_id),
                    attempt_count        INTEGER NOT NULL DEFAULT 0,
                    last_error           TEXT,
                    state                TEXT    NOT NULL DEFAULT 'pending',
                    created_at           INTEGER NOT NULL,
                    updated_at           INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_vpw_schedule
                    ON vector_projection_work(state, priority DESC, created_at ASC);

                CREATE INDEX IF NOT EXISTS idx_vpw_chunk
                    ON vector_projection_work(chunk_id);
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
                SchemaVersion(14) => Self::ensure_external_content_columns(&tx)?,
                SchemaVersion(15) => Self::ensure_fts_property_schemas(&tx)?,
                SchemaVersion(16) => Self::ensure_unicode_porter_fts_tokenizers(&tx)?,
                SchemaVersion(21) => Self::ensure_per_kind_fts_tables(&tx)?,
                SchemaVersion(22) => Self::ensure_staging_columns_json(&tx)?,
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

    fn ensure_external_content_columns(conn: &Connection) -> Result<(), SchemaError> {
        let node_columns = Self::column_names(conn, "nodes")?;
        if !node_columns.iter().any(|c| c == "content_ref") {
            conn.execute("ALTER TABLE nodes ADD COLUMN content_ref TEXT", [])?;
        }
        conn.execute_batch(
            r"
            CREATE INDEX IF NOT EXISTS idx_nodes_content_ref
                ON nodes(content_ref)
                WHERE content_ref IS NOT NULL AND superseded_at IS NULL;
            ",
        )?;

        let chunk_columns = Self::column_names(conn, "chunks")?;
        if !chunk_columns.iter().any(|c| c == "content_hash") {
            conn.execute("ALTER TABLE chunks ADD COLUMN content_hash TEXT", [])?;
        }
        Ok(())
    }

    /// Migration 16: migrate both `fts_nodes` and `fts_node_properties` from
    /// the default FTS5 simple tokenizer to `unicode61 remove_diacritics 2
    /// porter` so diacritic-insensitive and stem-aware matches work (e.g.
    /// `cafe` matching `café`, `shipping` matching `ship`).
    ///
    /// FTS5 does not support re-tokenizing an existing index in place, so
    /// both virtual tables are dropped and recreated with the new
    /// `tokenize=...` clause. `fts_nodes` is rebuilt inline from the
    /// canonical `chunks + nodes` join. `fts_node_properties` is left empty
    /// here and repopulated from canonical state by the engine runtime after
    /// bootstrap (the property FTS rebuild requires the per-kind
    /// `fts_property_schemas` projection that lives in the engine crate).
    ///
    /// A malformed row encountered during the inline `INSERT ... SELECT`
    /// causes the migration to abort: the rusqlite error propagates up
    /// through `execute_batch` and rolls back the outer migration
    /// transaction so the schema version is not advanced.
    fn ensure_unicode_porter_fts_tokenizers(conn: &Connection) -> Result<(), SchemaError> {
        conn.execute_batch(
            r"
            DROP TABLE IF EXISTS fts_nodes;
            CREATE VIRTUAL TABLE fts_nodes USING fts5(
                chunk_id UNINDEXED,
                node_logical_id UNINDEXED,
                kind UNINDEXED,
                text_content,
                tokenize = 'porter unicode61 remove_diacritics 2'
            );

            DROP TABLE IF EXISTS fts_node_properties;
            CREATE VIRTUAL TABLE fts_node_properties USING fts5(
                node_logical_id UNINDEXED,
                kind UNINDEXED,
                text_content,
                tokenize = 'porter unicode61 remove_diacritics 2'
            );

            INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
            SELECT c.id, n.logical_id, n.kind, c.text_content
            FROM chunks c
            JOIN nodes n
              ON n.logical_id = c.node_logical_id
             AND n.superseded_at IS NULL;
            ",
        )?;
        Ok(())
    }

    fn ensure_fts_property_schemas(conn: &Connection) -> Result<(), SchemaError> {
        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS fts_property_schemas (
                kind TEXT PRIMARY KEY,
                property_paths_json TEXT NOT NULL,
                separator TEXT NOT NULL DEFAULT ' ',
                format_version INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS fts_node_properties USING fts5(
                node_logical_id UNINDEXED,
                kind UNINDEXED,
                text_content
            );
            ",
        )?;
        Ok(())
    }

    fn ensure_per_kind_fts_tables(conn: &Connection) -> Result<(), SchemaError> {
        // Collect all registered kinds
        let kinds: Vec<String> = {
            let mut stmt = conn.prepare("SELECT kind FROM fts_property_schemas")?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };

        for kind in &kinds {
            let table_name = fts_kind_table_name(kind);
            let ddl = format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {table_name} USING fts5(\
                    node_logical_id UNINDEXED, \
                    text_content, \
                    tokenize = '{DEFAULT_FTS_TOKENIZER}'\
                )"
            );
            conn.execute_batch(&ddl)?;

            // Enqueue a PENDING rebuild for this kind (idempotent)
            conn.execute(
                "INSERT OR IGNORE INTO fts_property_rebuild_state \
                 (kind, schema_id, state, rows_done, started_at, is_first_registration) \
                 SELECT ?1, rowid, 'PENDING', 0, unixepoch('now') * 1000, 0 \
                 FROM fts_property_schemas WHERE kind = ?1",
                rusqlite::params![kind],
            )?;
        }

        // Drop the old global table (deferred: write sites must be updated in A-6 first)
        // Note: actual DROP happens in migration 23, after A-6 redirects write sites
        Ok(())
    }

    fn ensure_staging_columns_json(conn: &Connection) -> Result<(), SchemaError> {
        // Idempotent: check if the column already exists before altering
        let column_exists: bool = {
            let mut stmt = conn.prepare("PRAGMA table_info(fts_property_rebuild_staging)")?;
            let names: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            names.iter().any(|n| n == "columns_json")
        };

        if !column_exists {
            conn.execute_batch(
                "ALTER TABLE fts_property_rebuild_staging ADD COLUMN columns_json TEXT;",
            )?;
        }

        Ok(())
    }

    fn column_names(conn: &Connection, table: &str) -> Result<Vec<String>, SchemaError> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(names)
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

    /// Initialize a **read-only** connection with PRAGMAs that are safe for
    /// readers.
    ///
    /// Skips `journal_mode` (requires write; the writer already set WAL),
    /// `synchronous` (irrelevant for readers), and `journal_size_limit`
    /// (requires write).
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] if any PRAGMA fails to execute.
    pub fn initialize_reader_connection(&self, conn: &Connection) -> Result<(), SchemaError> {
        conn.execute_batch(
            r"
            PRAGMA foreign_keys = ON;
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
        // Vector dimensions are small positive integers (typically <= a few
        // thousand); convert explicitly so clippy's cast_possible_wrap is happy.
        let dimension_i64 = i64::try_from(dimension).map_err(|_| {
            SchemaError::Sqlite(rusqlite::Error::ToSqlConversionFailure(
                format!("vector dimension {dimension} does not fit in i64").into(),
            ))
        })?;
        conn.execute(
            "INSERT OR REPLACE INTO vector_profiles \
             (profile, table_name, dimension, enabled) VALUES (?1, ?2, ?3, 1)",
            rusqlite::params![profile, table_name, dimension_i64],
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

    /// Ensure a per-kind sqlite-vec virtual table exists and the
    /// `projection_profiles` row is recorded under `(kind, 'vec')`.
    ///
    /// The virtual table is named `vec_<sanitized_kind>` (via
    /// [`vec_kind_table_name`]).  A row is also written to the legacy
    /// `vector_profiles` table so that [`BootstrapReport::vector_profile_enabled`]
    /// continues to work.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] if the DDL fails or the feature is absent.
    #[cfg(feature = "sqlite-vec")]
    pub fn ensure_vec_kind_profile(
        &self,
        conn: &Connection,
        kind: &str,
        dimension: usize,
    ) -> Result<(), SchemaError> {
        let table_name = vec_kind_table_name(kind);
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS {table_name} USING vec0(\
                chunk_id TEXT PRIMARY KEY,\
                embedding float[{dimension}]\
            )"
        ))?;
        let dimension_i64 = i64::try_from(dimension).map_err(|_| {
            SchemaError::Sqlite(rusqlite::Error::ToSqlConversionFailure(
                format!("vector dimension {dimension} does not fit in i64").into(),
            ))
        })?;
        // Record in the legacy vector_profiles table so vector_profile_enabled works.
        conn.execute(
            "INSERT OR REPLACE INTO vector_profiles \
             (profile, table_name, dimension, enabled) VALUES (?1, ?2, ?3, 1)",
            rusqlite::params![kind, table_name, dimension_i64],
        )?;
        // Record in projection_profiles under (kind, 'vec').
        // Use "dimensions" (plural) to match what get_vec_profile extracts via json_extract.
        let config_json =
            format!(r#"{{"table_name":"{table_name}","dimensions":{dimension_i64}}}"#);
        conn.execute(
            "INSERT INTO projection_profiles (kind, facet, config_json, active_at, created_at) \
             VALUES (?1, 'vec', ?2, unixepoch(), unixepoch()) \
             ON CONFLICT(kind, facet) DO UPDATE SET \
                 config_json = ?2, \
                 active_at   = unixepoch()",
            rusqlite::params![kind, config_json],
        )?;
        Ok(())
    }

    /// # Errors
    ///
    /// Always returns [`SchemaError::MissingCapability`] when the `sqlite-vec`
    /// feature is not compiled in.
    #[cfg(not(feature = "sqlite-vec"))]
    pub fn ensure_vec_kind_profile(
        &self,
        _conn: &Connection,
        _kind: &str,
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

/// Default FTS5 tokenizer used when no per-kind profile is configured.
pub const DEFAULT_FTS_TOKENIZER: &str = "porter unicode61 remove_diacritics 2";

/// Derive the canonical FTS5 virtual-table name for a given node `kind`.
///
/// Rules:
/// 1. Lowercase the kind string.
/// 2. Replace every character that is NOT `[a-z0-9]` with `_`.
/// 3. Collapse consecutive underscores to a single `_`.
/// 4. Prefix with `fts_props_`.
/// 5. If the result exceeds 63 characters: truncate the slug to 55 characters
///    and append `_` + the first 7 hex chars of the SHA-256 of the original kind.
#[must_use]
pub fn fts_kind_table_name(kind: &str) -> String {
    // Step 1-3: normalise the slug
    let lowered = kind.to_lowercase();
    let mut slug = String::with_capacity(lowered.len());
    let mut prev_was_underscore = false;
    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            prev_was_underscore = false;
        } else {
            if !prev_was_underscore {
                slug.push('_');
            }
            prev_was_underscore = true;
        }
    }

    // Step 4: prefix
    let prefixed = format!("fts_props_{slug}");

    // Step 5: truncate if needed
    if prefixed.len() <= 63 {
        prefixed
    } else {
        // Hash the original kind string
        let hash = Sha256::digest(kind.as_bytes());
        let mut hex = String::with_capacity(hash.len() * 2);
        for b in &hash {
            use std::fmt::Write as _;
            let _ = write!(hex, "{b:02x}");
        }
        let hex_suffix = &hex[..7];
        // Slug must be 55 chars so that "fts_props_" (10) + slug (55) + "_" (1) + hex7 (7) = 73 — too long.
        // Wait: total = 10 + slug_len + 1 + 7 = 63  => slug_len = 45
        let slug_truncated = if slug.len() > 45 { &slug[..45] } else { &slug };
        format!("fts_props_{slug_truncated}_{hex_suffix}")
    }
}

/// Derive the canonical sqlite-vec virtual-table name for a given node `kind`.
///
/// Rules:
/// 1. Lowercase the kind string.
/// 2. Replace every character that is NOT `[a-z0-9]` with `_`.
/// 3. Collapse consecutive underscores to a single `_`.
/// 4. Prefix with `vec_`.
#[must_use]
pub fn vec_kind_table_name(kind: &str) -> String {
    let lowered = kind.to_lowercase();
    let mut slug = String::with_capacity(lowered.len());
    let mut prev_was_underscore = false;
    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            prev_was_underscore = false;
        } else {
            if !prev_was_underscore {
                slug.push('_');
            }
            prev_was_underscore = true;
        }
    }
    format!("vec_{slug}")
}

/// Derive the canonical FTS5 column name for a JSON path.
///
/// Rules:
/// 1. Strip leading `$.` or `$` prefix.
/// 2. Replace every character that is NOT `[a-z0-9_]` (after lowercasing) with `_`.
/// 3. Collapse consecutive underscores.
/// 4. If `is_recursive` is `true`, append `_all`.
#[must_use]
pub fn fts_column_name(path: &str, is_recursive: bool) -> String {
    // Step 1: strip prefix
    let stripped = if let Some(rest) = path.strip_prefix("$.") {
        rest
    } else if let Some(rest) = path.strip_prefix('$') {
        rest
    } else {
        path
    };

    // Step 2-3: normalise
    let lowered = stripped.to_lowercase();
    let mut col = String::with_capacity(lowered.len());
    let mut prev_was_underscore = false;
    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            col.push(ch);
            prev_was_underscore = ch == '_';
        } else {
            if !prev_was_underscore {
                col.push('_');
            }
            prev_was_underscore = true;
        }
    }

    // Strip trailing underscores
    let col = col.trim_end_matches('_').to_owned();

    // Step 4: recursive suffix
    if is_recursive {
        format!("{col}_all")
    } else {
        col
    }
}

/// Look up the FTS tokenizer configured for a given `kind` in `projection_profiles`.
///
/// Returns [`DEFAULT_FTS_TOKENIZER`] when:
/// - the `projection_profiles` table does not exist yet, or
/// - no row exists for `(kind, 'fts')`, or
/// - the `tokenizer` field in `config_json` is absent or empty.
///
/// # Errors
///
/// Returns [`SchemaError::Sqlite`] on any `SQLite` error other than a missing table.
pub fn resolve_fts_tokenizer(conn: &Connection, kind: &str) -> Result<String, SchemaError> {
    let result = conn
        .query_row(
            "SELECT json_extract(config_json, '$.tokenizer') FROM projection_profiles WHERE kind = ?1 AND facet = 'fts'",
            [kind],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional();

    match result {
        Ok(Some(Some(tok))) if !tok.is_empty() => Ok(tok),
        Ok(_) => Ok(DEFAULT_FTS_TOKENIZER.to_owned()),
        Err(rusqlite::Error::SqliteFailure(_, _)) => {
            // Table doesn't exist or other sqlite-level error — return default
            Ok(DEFAULT_FTS_TOKENIZER.to_owned())
        }
        Err(e) => Err(SchemaError::Sqlite(e)),
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

    // --- A-1: fts_kind_table_name tests ---

    #[test]
    fn fts_kind_table_name_simple_kind() {
        assert_eq!(
            super::fts_kind_table_name("WMKnowledgeObject"),
            "fts_props_wmknowledgeobject"
        );
    }

    #[test]
    fn fts_kind_table_name_another_simple_kind() {
        assert_eq!(
            super::fts_kind_table_name("WMExecutionRecord"),
            "fts_props_wmexecutionrecord"
        );
    }

    #[test]
    fn fts_kind_table_name_with_separator_chars() {
        assert_eq!(
            super::fts_kind_table_name("MyKind-With.Dots"),
            "fts_props_mykind_with_dots"
        );
    }

    #[test]
    fn fts_kind_table_name_collapses_consecutive_underscores() {
        assert_eq!(
            super::fts_kind_table_name("Kind__Double__Underscores"),
            "fts_props_kind_double_underscores"
        );
    }

    #[test]
    fn fts_kind_table_name_long_kind_truncates_with_hash() {
        // 61 A's — slug after prefix would be 61 chars, exceeding 63-char limit
        let long_kind = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let result = super::fts_kind_table_name(long_kind);
        assert_eq!(result.len(), 63, "result must be exactly 63 chars");
        assert!(
            result.starts_with("fts_props_"),
            "result must start with fts_props_"
        );
        // Must contain underscore before 7-char hex suffix
        let last_underscore = result.rfind('_').expect("must contain underscore");
        let hex_suffix = &result[last_underscore + 1..];
        assert_eq!(hex_suffix.len(), 7, "hex suffix must be 7 chars");
        assert!(
            hex_suffix.chars().all(|c| c.is_ascii_hexdigit()),
            "hex suffix must be hex digits"
        );
    }

    #[test]
    fn fts_kind_table_name_testkind() {
        assert_eq!(super::fts_kind_table_name("TestKind"), "fts_props_testkind");
    }

    // --- A-1: fts_column_name tests ---

    #[test]
    fn fts_column_name_simple_field() {
        assert_eq!(super::fts_column_name("$.title", false), "title");
    }

    #[test]
    fn fts_column_name_nested_path() {
        assert_eq!(
            super::fts_column_name("$.payload.content", false),
            "payload_content"
        );
    }

    #[test]
    fn fts_column_name_recursive() {
        assert_eq!(super::fts_column_name("$.payload", true), "payload_all");
    }

    #[test]
    fn fts_column_name_special_chars() {
        assert_eq!(
            super::fts_column_name("$.some-field[0]", false),
            "some_field_0"
        );
    }

    // --- A-1: resolve_fts_tokenizer tests ---

    #[test]
    fn resolve_fts_tokenizer_returns_default_when_no_table() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        // No projection_profiles table — should return default
        let result = super::resolve_fts_tokenizer(&conn, "MyKind").expect("should not error");
        assert_eq!(result, super::DEFAULT_FTS_TOKENIZER);
    }

    #[test]
    fn resolve_fts_tokenizer_returns_configured_value() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        conn.execute_batch(
            "CREATE TABLE projection_profiles (
                kind TEXT NOT NULL,
                facet TEXT NOT NULL,
                config_json TEXT NOT NULL,
                active_at INTEGER,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                PRIMARY KEY (kind, facet)
            );
            INSERT INTO projection_profiles (kind, facet, config_json)
            VALUES ('MyKind', 'fts', '{\"tokenizer\":\"trigram\"}');",
        )
        .expect("setup table");

        let result = super::resolve_fts_tokenizer(&conn, "MyKind").expect("should not error");
        assert_eq!(result, "trigram");

        let default_result =
            super::resolve_fts_tokenizer(&conn, "OtherKind").expect("should not error");
        assert_eq!(default_result, super::DEFAULT_FTS_TOKENIZER);
    }

    // --- A-2: migration 20 tests ---

    #[test]
    fn migration_20_creates_projection_profiles_table() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("bootstrap");

        let table_exists: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='projection_profiles'",
                [],
                |row| row.get(0),
            )
            .expect("query sqlite_master");
        assert_eq!(table_exists, 1, "projection_profiles table must exist");

        // Check columns exist by querying them
        conn.execute_batch(
            "INSERT INTO projection_profiles (kind, facet, config_json)
             VALUES ('TestKind', 'fts', '{}')",
        )
        .expect("insert row to verify columns");
        let (kind, facet, config_json): (String, String, String) = conn
            .query_row(
                "SELECT kind, facet, config_json FROM projection_profiles WHERE kind='TestKind'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("select columns");
        assert_eq!(kind, "TestKind");
        assert_eq!(facet, "fts");
        assert_eq!(config_json, "{}");
    }

    #[test]
    fn migration_20_primary_key_is_kind_facet() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("bootstrap");

        conn.execute_batch(
            "INSERT INTO projection_profiles (kind, facet, config_json)
             VALUES ('MyKind', 'fts', '{\"tokenizer\":\"porter\"}');",
        )
        .expect("first insert");

        // Second insert with same (kind, facet) must fail
        let result = conn.execute_batch(
            "INSERT INTO projection_profiles (kind, facet, config_json)
             VALUES ('MyKind', 'fts', '{\"tokenizer\":\"trigram\"}');",
        );
        assert!(
            result.is_err(),
            "duplicate (kind, facet) must violate PRIMARY KEY"
        );
    }

    #[test]
    fn migration_20_resolve_fts_tokenizer_end_to_end() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("bootstrap");

        conn.execute_batch(
            "INSERT INTO projection_profiles (kind, facet, config_json)
             VALUES ('MyKind', 'fts', '{\"tokenizer\":\"trigram\"}');",
        )
        .expect("insert profile");

        let result = super::resolve_fts_tokenizer(&conn, "MyKind").expect("should not error");
        assert_eq!(result, "trigram");

        let default_result =
            super::resolve_fts_tokenizer(&conn, "UnknownKind").expect("should not error");
        assert_eq!(default_result, super::DEFAULT_FTS_TOKENIZER);
    }

    #[test]
    fn migration_21_creates_per_kind_fts_table_and_pending_row() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();

        // Bootstrap up through migration 20 by using a fresh DB, then manually insert
        // a kind into fts_property_schemas so migration 21 picks it up.
        // We do a full bootstrap (which applies all migrations including 21).
        // Insert the kind before bootstrapping so it is present when migration 21 runs.
        // Since bootstrap applies migrations in order and migration 15 creates
        // fts_property_schemas, we must insert after that table exists.
        // Strategy: bootstrap first, insert kind, then run bootstrap again (idempotent).
        manager.bootstrap(&conn).expect("first bootstrap");

        conn.execute_batch(
            "INSERT INTO fts_property_schemas (kind, property_paths_json, separator, format_version) \
             VALUES ('TestKind', '[]', ',', 1)",
        )
        .expect("insert kind");

        // Now run ensure_per_kind_fts_tables directly by calling bootstrap again — migration 21
        // is already applied, so we test the function directly.
        SchemaManager::ensure_per_kind_fts_tables(&conn).expect("ensure_per_kind_fts_tables");

        // fts_props_testkind should exist
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='fts_props_testkind'",
                [],
                |r| r.get(0),
            )
            .expect("count fts table");
        assert_eq!(
            count, 1,
            "fts_props_testkind virtual table should be created"
        );

        // PENDING row should exist
        let state: String = conn
            .query_row(
                "SELECT state FROM fts_property_rebuild_state WHERE kind='TestKind'",
                [],
                |r| r.get(0),
            )
            .expect("rebuild state row");
        assert_eq!(state, "PENDING");
    }

    #[test]
    fn migration_22_adds_columns_json_to_staging_table() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("bootstrap");

        let col_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM pragma_table_info('fts_property_rebuild_staging') WHERE name='columns_json'",
                [],
                |r| r.get(0),
            )
            .expect("pragma_table_info");
        assert_eq!(
            col_count, 1,
            "columns_json column must exist after migration 22"
        );
    }

    // --- 0.5.0 item 6: vec_kind_table_name tests ---

    #[test]
    fn vec_kind_table_name_simple_kind() {
        assert_eq!(
            super::vec_kind_table_name("WMKnowledgeObject"),
            "vec_wmknowledgeobject"
        );
    }

    #[test]
    fn vec_kind_table_name_another_kind() {
        assert_eq!(super::vec_kind_table_name("MyKind"), "vec_mykind");
    }

    #[test]
    fn vec_kind_table_name_with_separator_chars() {
        assert_eq!(
            super::vec_kind_table_name("MyKind-With.Dots"),
            "vec_mykind_with_dots"
        );
    }

    #[test]
    fn vec_kind_table_name_collapses_consecutive_underscores() {
        assert_eq!(
            super::vec_kind_table_name("Kind__Double__Underscores"),
            "vec_kind_double_underscores"
        );
    }

    // --- 0.5.0 item 6: per-kind vec tables ---

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn per_kind_vec_table_created_when_vec_profile_registered() {
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

        // Register a vec profile for MyKind — should create vec_mykind, NOT vec_nodes_active
        manager
            .ensure_vec_kind_profile(&conn, "MyKind", 128)
            .expect("ensure_vec_kind_profile");

        // vec_mykind virtual table must exist
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='vec_mykind'",
                [],
                |r| r.get(0),
            )
            .expect("query sqlite_master");
        assert_eq!(count, 1, "vec_mykind virtual table must be created");

        // projection_profiles row must exist with (kind='MyKind', facet='vec')
        let pp_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM projection_profiles WHERE kind='MyKind' AND facet='vec'",
                [],
                |r| r.get(0),
            )
            .expect("query projection_profiles");
        assert_eq!(
            pp_count, 1,
            "projection_profiles row must exist for (MyKind, vec)"
        );

        // The old global vec_nodes_active must NOT have been created
        let old_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='vec_nodes_active'",
                [],
                |r| r.get(0),
            )
            .expect("query sqlite_master");
        assert_eq!(
            old_count, 0,
            "vec_nodes_active must NOT be created for per-kind registration"
        );
    }

    // --- A-6: migration 23 drops fts_node_properties ---
    #[test]
    fn migration_23_drops_global_fts_node_properties_table() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("bootstrap");

        // After migration 23, fts_node_properties must NOT exist in sqlite_master.
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master \
                 WHERE type='table' AND name='fts_node_properties'",
                [],
                |r| r.get(0),
            )
            .expect("check sqlite_master");
        assert_eq!(
            count, 0,
            "fts_node_properties must be dropped by migration 23"
        );
    }
}
