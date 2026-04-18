use rusqlite::{DatabaseName, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use crate::ids::new_id;
use crate::{EngineError, SkippedEdge};

use super::{
    AdminService, EXPORT_PROTOCOL_VERSION, LogicalPurgeReport, LogicalRestoreReport,
    ProvenancePurgeOptions, ProvenancePurgeReport, SafeExportManifest, SafeExportOptions,
    TraceReport, clear_operational_current_rows, i64_to_usize, persist_simple_provenance_event,
    rebuild_operational_current_rows,
};

impl AdminService {
    /// # Errors
    /// Returns [`EngineError`] if the database connection fails or any SQL query fails.
    pub fn trace_source(&self, source_ref: &str) -> Result<TraceReport, EngineError> {
        let conn = self.connect()?;

        let node_logical_ids = collect_strings(
            &conn,
            "SELECT logical_id FROM nodes WHERE source_ref = ?1 ORDER BY created_at",
            source_ref,
        )?;
        let action_ids = collect_strings(
            &conn,
            "SELECT id FROM actions WHERE source_ref = ?1 ORDER BY created_at",
            source_ref,
        )?;
        let operational_mutation_ids = collect_strings(
            &conn,
            "SELECT id FROM operational_mutations WHERE source_ref = ?1 ORDER BY mutation_order",
            source_ref,
        )?;

        Ok(TraceReport {
            source_ref: source_ref.to_owned(),
            node_rows: count_source_ref(&conn, "nodes", source_ref)?,
            edge_rows: count_source_ref(&conn, "edges", source_ref)?,
            action_rows: count_source_ref(&conn, "actions", source_ref)?,
            operational_mutation_rows: count_source_ref(
                &conn,
                "operational_mutations",
                source_ref,
            )?,
            node_logical_ids,
            action_ids,
            operational_mutation_ids,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the transaction cannot be
    /// started, or lifecycle restoration prerequisites are missing.
    #[allow(clippy::too_many_lines)]
    pub fn restore_logical_id(
        &self,
        logical_id: &str,
    ) -> Result<LogicalRestoreReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let active_count: i64 = tx.query_row(
            "SELECT count(*) FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
            [logical_id],
            |row| row.get(0),
        )?;
        if active_count > 0 {
            return Ok(LogicalRestoreReport {
                logical_id: logical_id.to_owned(),
                was_noop: true,
                restored_node_rows: 0,
                restored_edge_rows: 0,
                restored_chunk_rows: 0,
                restored_fts_rows: 0,
                restored_property_fts_rows: 0,
                restored_vec_rows: 0,
                skipped_edges: Vec::new(),
                notes: vec!["logical_id already active".to_owned()],
            });
        }

        let restored_node: Option<(String, String)> = tx
            .query_row(
                "SELECT row_id, kind FROM nodes \
                 WHERE logical_id = ?1 AND superseded_at IS NOT NULL \
                 ORDER BY superseded_at DESC, created_at DESC, rowid DESC LIMIT 1",
                [logical_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (restored_node_row_id, restored_kind) = restored_node.ok_or_else(|| {
            EngineError::InvalidWrite(format!("logical_id '{logical_id}' is not retired"))
        })?;

        tx.execute(
            "UPDATE nodes SET superseded_at = NULL WHERE row_id = ?1",
            [restored_node_row_id.as_str()],
        )?;

        let retire_scope: Option<(i64, Option<String>, i64)> = tx
            .query_row(
                "SELECT rowid, source_ref, created_at FROM provenance_events \
                 WHERE event_type = 'node_retire' AND subject = ?1 \
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [logical_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (restored_edge_rows, skipped_edges) = if let Some((
            retire_event_rowid,
            retire_source_ref,
            retire_created_at,
        )) = retire_scope
        {
            restore_validated_edges(
                &tx,
                logical_id,
                retire_source_ref.as_deref(),
                retire_created_at,
                retire_event_rowid,
            )?
        } else {
            (0, Vec::new())
        };

        let restored_chunk_rows: usize = tx
            .query_row(
                "SELECT count(*) FROM chunks WHERE node_logical_id = ?1",
                [logical_id],
                |row| row.get::<_, i64>(0),
            )
            .map(i64_to_usize)?;
        tx.execute(
            "DELETE FROM fts_nodes WHERE node_logical_id = ?1",
            [logical_id],
        )?;
        let restored_fts_rows = tx.execute(
            "INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content) \
             SELECT id, node_logical_id, ?2, text_content \
             FROM chunks WHERE node_logical_id = ?1",
            rusqlite::params![logical_id, restored_kind],
        )?;
        let restored_vec_rows = count_vec_rows_for_logical_id(&tx, logical_id)?;

        // Rebuild property FTS for the restored node.
        // Delete from the per-kind FTS table for this node (if the table exists).
        let table = fathomdb_schema::fts_kind_table_name(&restored_kind);
        let fts_table_exists: bool = tx
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name = ?1 \
                 AND sql LIKE 'CREATE VIRTUAL TABLE%'",
                rusqlite::params![table],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if fts_table_exists {
            tx.execute(
                &format!("DELETE FROM {table} WHERE node_logical_id = ?1"),
                [logical_id],
            )?;
        }
        let restored_property_fts_rows =
            rebuild_single_node_property_fts(&tx, logical_id, &restored_kind)?;

        persist_simple_provenance_event(
            &tx,
            "restore_logical_id",
            logical_id,
            Some(serde_json::json!({
                "restored_node_rows": 1,
                "restored_edge_rows": restored_edge_rows,
                "restored_chunk_rows": restored_chunk_rows,
                "restored_fts_rows": restored_fts_rows,
                "restored_property_fts_rows": restored_property_fts_rows,
                "restored_vec_rows": restored_vec_rows,
            })),
        )?;
        tx.commit()?;

        Ok(LogicalRestoreReport {
            logical_id: logical_id.to_owned(),
            was_noop: false,
            restored_node_rows: 1,
            restored_edge_rows,
            restored_chunk_rows,
            restored_fts_rows,
            restored_property_fts_rows,
            restored_vec_rows,
            skipped_edges,
            notes: Vec::new(),
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the transaction cannot be
    /// started, or the purge mutation fails.
    pub fn purge_logical_id(&self, logical_id: &str) -> Result<LogicalPurgeReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let active_count: i64 = tx.query_row(
            "SELECT count(*) FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
            [logical_id],
            |row| row.get(0),
        )?;
        if active_count > 0 {
            return Ok(LogicalPurgeReport {
                logical_id: logical_id.to_owned(),
                was_noop: true,
                deleted_node_rows: 0,
                deleted_edge_rows: 0,
                deleted_chunk_rows: 0,
                deleted_fts_rows: 0,
                deleted_vec_rows: 0,
                notes: vec!["logical_id is active; purge skipped".to_owned()],
            });
        }

        let node_rows: i64 = tx.query_row(
            "SELECT count(*) FROM nodes WHERE logical_id = ?1",
            [logical_id],
            |row| row.get(0),
        )?;
        if node_rows == 0 {
            return Err(EngineError::InvalidWrite(format!(
                "logical_id '{logical_id}' does not exist"
            )));
        }

        let deleted_vec_rows = delete_vec_rows_for_logical_id(&tx, logical_id)?;
        let deleted_fts_rows = tx.execute(
            "DELETE FROM fts_nodes WHERE node_logical_id = ?1",
            [logical_id],
        )?;
        let deleted_edge_rows = tx.execute(
            "DELETE FROM edges WHERE source_logical_id = ?1 OR target_logical_id = ?1",
            [logical_id],
        )?;
        let deleted_chunk_rows = tx.execute(
            "DELETE FROM chunks WHERE node_logical_id = ?1",
            [logical_id],
        )?;
        let deleted_node_rows =
            tx.execute("DELETE FROM nodes WHERE logical_id = ?1", [logical_id])?;
        tx.execute(
            "DELETE FROM node_access_metadata WHERE logical_id = ?1",
            [logical_id],
        )?;

        persist_simple_provenance_event(
            &tx,
            "purge_logical_id",
            logical_id,
            Some(serde_json::json!({
                "deleted_node_rows": deleted_node_rows,
                "deleted_edge_rows": deleted_edge_rows,
                "deleted_chunk_rows": deleted_chunk_rows,
                "deleted_fts_rows": deleted_fts_rows,
                "deleted_vec_rows": deleted_vec_rows,
            })),
        )?;
        tx.commit()?;

        Ok(LogicalPurgeReport {
            logical_id: logical_id.to_owned(),
            was_noop: false,
            deleted_node_rows,
            deleted_edge_rows,
            deleted_chunk_rows,
            deleted_fts_rows,
            deleted_vec_rows,
            notes: Vec::new(),
        })
    }

    /// Purge provenance events older than `before_timestamp`.
    ///
    /// By default, `excise` and `purge_logical_id` event types are preserved so that
    /// data-deletion audit trails survive. Pass an explicit
    /// `preserve_event_types` list to override this default.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the transaction
    /// cannot be started, or any SQL statement fails.
    pub fn purge_provenance_events(
        &self,
        before_timestamp: i64,
        options: &ProvenancePurgeOptions,
    ) -> Result<ProvenancePurgeReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let preserved_types: Vec<&str> = if options.preserve_event_types.is_empty() {
            vec!["excise", "purge_logical_id"]
        } else {
            options
                .preserve_event_types
                .iter()
                .map(String::as_str)
                .collect()
        };

        // Build the NOT IN clause dynamically based on preserved types.
        let placeholders: String = (0..preserved_types.len())
            .map(|i| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let count_query = format!(
            "SELECT count(*) FROM provenance_events \
             WHERE created_at < ?1 AND event_type NOT IN ({placeholders})"
        );
        let delete_query = format!(
            "DELETE FROM provenance_events WHERE rowid IN (\
             SELECT rowid FROM provenance_events \
             WHERE created_at < ?1 AND event_type NOT IN ({placeholders}) \
             LIMIT 10000)"
        );

        let bind_params = |stmt: &mut rusqlite::Statement<'_>| -> Result<(), rusqlite::Error> {
            stmt.raw_bind_parameter(1, before_timestamp)?;
            for (i, event_type) in preserved_types.iter().enumerate() {
                stmt.raw_bind_parameter(i + 2, *event_type)?;
            }
            Ok(())
        };

        let events_deleted = if options.dry_run {
            let mut stmt = tx.prepare(&count_query)?;
            bind_params(&mut stmt)?;
            stmt.raw_query()
                .next()?
                .map_or(0, |row| row.get::<_, u64>(0).unwrap_or(0))
        } else {
            let mut total_deleted: u64 = 0;
            loop {
                let mut stmt = tx.prepare(&delete_query)?;
                bind_params(&mut stmt)?;
                let deleted = stmt.raw_execute()?;
                if deleted == 0 {
                    break;
                }
                total_deleted += deleted as u64;
            }
            total_deleted
        };

        let total_after: u64 =
            tx.query_row("SELECT count(*) FROM provenance_events", [], |row| {
                row.get(0)
            })?;

        let oldest_remaining: Option<i64> = tx
            .query_row("SELECT MIN(created_at) FROM provenance_events", [], |row| {
                row.get(0)
            })
            .optional()?
            .flatten();

        if !options.dry_run {
            tx.commit()?;
        }

        // In dry_run mode nothing was deleted, so total_after includes the
        // would-be-deleted rows; subtract to get the preserved count.
        let events_preserved = if options.dry_run {
            total_after - events_deleted
        } else {
            total_after
        };

        Ok(ProvenancePurgeReport {
            events_deleted,
            events_preserved,
            oldest_remaining,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the transaction cannot be
    /// started, or any SQL statement fails.
    #[allow(clippy::too_many_lines)]
    pub fn excise_source(&self, source_ref: &str) -> Result<TraceReport, EngineError> {
        let mut conn = self.connect()?;

        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let affected_operational_collections = collect_strings_tx(
            &tx,
            "SELECT DISTINCT m.collection_name \
             FROM operational_mutations m \
             JOIN operational_collections c ON c.name = m.collection_name \
             WHERE m.source_ref = ?1 AND c.kind = 'latest_state' \
             ORDER BY m.collection_name",
            source_ref,
        )?;

        // Collect (row_id, logical_id) for active rows that will be excised.
        let pairs: Vec<(String, String)> = {
            let mut stmt = tx.prepare(
                "SELECT row_id, logical_id FROM nodes \
                 WHERE source_ref = ?1 AND superseded_at IS NULL",
            )?;
            stmt.query_map([source_ref], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        let affected_logical_ids: Vec<String> = pairs
            .iter()
            .map(|(_, logical_id)| logical_id.clone())
            .collect();

        // Supersede bad rows in all tables.
        tx.execute(
            "UPDATE nodes SET superseded_at = unixepoch() \
             WHERE source_ref = ?1 AND superseded_at IS NULL",
            [source_ref],
        )?;
        tx.execute(
            "UPDATE edges SET superseded_at = unixepoch() \
             WHERE source_ref = ?1 AND superseded_at IS NULL",
            [source_ref],
        )?;
        tx.execute(
            "UPDATE actions SET superseded_at = unixepoch() \
             WHERE source_ref = ?1 AND superseded_at IS NULL",
            [source_ref],
        )?;
        clear_operational_current_rows(&tx, &affected_operational_collections)?;
        tx.execute(
            "DELETE FROM operational_mutations WHERE source_ref = ?1",
            [source_ref],
        )?;
        for logical_id in &affected_logical_ids {
            delete_vec_rows_for_logical_id(&tx, logical_id)?;
            tx.execute(
                "DELETE FROM chunks WHERE node_logical_id = ?1",
                [logical_id.as_str()],
            )?;
        }

        // Restore the most recent prior version for each affected logical_id.
        for (excised_row_id, logical_id) in &pairs {
            let prior: Option<String> = tx
                .query_row(
                    "SELECT row_id FROM nodes \
                     WHERE logical_id = ?1 AND row_id != ?2 \
                     ORDER BY created_at DESC LIMIT 1",
                    [logical_id.as_str(), excised_row_id.as_str()],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(prior_id) = prior {
                tx.execute(
                    "UPDATE nodes SET superseded_at = NULL WHERE row_id = ?1",
                    [prior_id.as_str()],
                )?;
            }
        }

        for logical_id in &affected_logical_ids {
            let has_active_node = tx
                .query_row(
                    "SELECT 1 FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL LIMIT 1",
                    [logical_id.as_str()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .is_some();
            if !has_active_node {
                tx.execute(
                    "DELETE FROM node_access_metadata WHERE logical_id = ?1",
                    [logical_id.as_str()],
                )?;
            }
        }

        rebuild_operational_current_rows(&tx, &affected_operational_collections)?;

        // Rebuild FTS atomically within the same transaction so readers never
        // observe a post-excise node state with a stale FTS index.
        tx.execute("DELETE FROM fts_nodes", [])?;
        tx.execute(
            r"
            INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
            SELECT c.id, n.logical_id, n.kind, c.text_content
            FROM chunks c
            JOIN nodes n
              ON n.logical_id = c.node_logical_id
             AND n.superseded_at IS NULL
            ",
            [],
        )?;

        // Rebuild property FTS in the same transaction.
        rebuild_property_fts_in_tx(&tx)?;

        // Record the audit event inside the same transaction so the excision and its
        // audit record are committed atomically — no window where the excision is
        // durable but unaudited.
        tx.execute(
            "INSERT INTO provenance_events (id, event_type, subject, source_ref) \
             VALUES (?1, 'excise_source', ?2, ?2)",
            rusqlite::params![new_id(), source_ref],
        )?;

        tx.commit()?;

        self.trace_source(source_ref)
    }

    /// # Errors
    /// Returns [`EngineError`] if the WAL checkpoint fails, the `SQLite` backup fails,
    /// the SHA-256 digest cannot be computed, or the manifest file cannot be written.
    pub fn safe_export(
        &self,
        destination_path: impl AsRef<Path>,
        options: SafeExportOptions,
    ) -> Result<SafeExportManifest, EngineError> {
        let destination_path = destination_path.as_ref();

        // 1. Optionally checkpoint WAL before exporting. This keeps the on-disk file tidy for
        // callers that want a fully checkpointed export, but export correctness does not depend
        // on it because the backup API copies from the live SQLite connection state.
        let conn = self.connect()?;

        if options.force_checkpoint {
            trace_info!("safe_export: wal checkpoint started");
            let (busy, log, checkpointed): (i64, i64, i64) =
                conn.query_row("PRAGMA wal_checkpoint(FULL)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
            if busy != 0 {
                trace_warn!(
                    busy,
                    log_frames = log,
                    checkpointed_frames = checkpointed,
                    "safe_export: wal checkpoint blocked by active readers"
                );
                return Err(EngineError::Bridge(format!(
                    "WAL checkpoint blocked: {busy} active reader(s) prevented a full checkpoint; \
                     log frames={log}, checkpointed={checkpointed}; \
                     retry export when no readers are active"
                )));
            }
            trace_info!(
                log_frames = log,
                checkpointed_frames = checkpointed,
                "safe_export: wal checkpoint completed"
            );
        }

        let schema_version: u32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM fathom_schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // 2. Export the database through SQLite's online backup API so committed data in the WAL
        // is included even when `force_checkpoint` is false.
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent)?;
        }
        conn.backup(DatabaseName::Main, destination_path, None)?;

        drop(conn);

        // 2b. Query page_count from the EXPORTED file so the manifest reflects what was
        // actually backed up, not the source (which may have changed between the PRAGMA
        // and the backup call).
        let page_count: u64 = {
            let export_conn = rusqlite::Connection::open_with_flags(
                destination_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            export_conn.query_row("PRAGMA page_count", [], |row| row.get(0))?
        };

        // 3. Compute SHA-256 of the exported file.
        // FIX(review): was fs::read loading entire DB into memory; use streaming hash.
        let sha256 = {
            let mut file = fs::File::open(destination_path)?;
            let mut hasher = Sha256::new();
            io::copy(&mut file, &mut hasher)?;
            format!("{:x}", hasher.finalize())
        };

        // 4. Record when the export was created.
        let exported_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| EngineError::Bridge(format!("system clock error: {e}")))?
            .as_secs();

        let manifest = SafeExportManifest {
            exported_at,
            sha256,
            schema_version,
            protocol_version: EXPORT_PROTOCOL_VERSION,
            page_count,
        };

        // 5. Write manifest alongside the exported file, using Path API for the name.
        let manifest_path = {
            let mut p = destination_path.to_path_buf();
            let stem = p
                .file_name()
                .map(|n| format!("{}.export-manifest.json", n.to_string_lossy()))
                .ok_or_else(|| {
                    EngineError::Bridge("destination path has no filename".to_owned())
                })?;
            p.set_file_name(stem);
            p
        };
        let manifest_json =
            serde_json::to_string(&manifest).map_err(|e| EngineError::Bridge(e.to_string()))?;

        // Atomic manifest write: write to a temp file then rename so readers never
        // observe a partially-written manifest.
        let manifest_tmp = manifest_path.with_extension("json.tmp");
        if let Err(e) = fs::write(&manifest_tmp, &manifest_json)
            .and_then(|()| fs::rename(&manifest_tmp, &manifest_path))
        {
            let _ = fs::remove_file(&manifest_tmp);
            return Err(e.into());
        }

        Ok(manifest)
    }
}

pub(super) fn rebuild_property_fts_in_tx(
    conn: &rusqlite::Connection,
) -> Result<usize, EngineError> {
    // Delete from ALL per-kind FTS virtual tables (including orphaned ones without schemas).
    // Filter by sql LIKE 'CREATE VIRTUAL TABLE%' to exclude FTS5 shadow tables.
    let all_per_kind_tables: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'fts_props_%' \
             AND sql LIKE 'CREATE VIRTUAL TABLE%'",
        )?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    for table in &all_per_kind_tables {
        conn.execute_batch(&format!("DELETE FROM {table}"))?;
    }
    conn.execute("DELETE FROM fts_node_property_positions", [])?;
    let inserted = crate::projection::insert_property_fts_rows(
        conn,
        "SELECT logical_id, properties FROM nodes WHERE kind = ?1 AND superseded_at IS NULL",
    )?;
    Ok(inserted)
}

/// Rebuild property FTS for a single node. Returns 1 if a row was inserted, 0 otherwise.
/// The caller must delete any existing per-kind FTS row for this node first.
pub(super) fn rebuild_single_node_property_fts(
    conn: &rusqlite::Connection,
    logical_id: &str,
    kind: &str,
) -> Result<usize, EngineError> {
    let schema: Option<(String, String)> = conn
        .query_row(
            "SELECT property_paths_json, separator FROM fts_property_schemas WHERE kind = ?1",
            [kind],
            |row| {
                let paths_json: String = row.get(0)?;
                let separator: String = row.get(1)?;
                Ok((paths_json, separator))
            },
        )
        .optional()?;
    let Some((paths_json, separator)) = schema else {
        return Ok(0);
    };
    let parsed = crate::writer::parse_property_schema_json(&paths_json, &separator);
    let properties_str: Option<String> = conn
        .query_row(
            "SELECT properties FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
            [logical_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(properties_str) = properties_str else {
        return Ok(0);
    };
    let props: serde_json::Value = serde_json::from_str(&properties_str).unwrap_or_default();
    let (text, positions, _stats) = crate::writer::extract_property_fts(&props, &parsed);
    let Some(text) = text else {
        return Ok(0);
    };
    conn.execute(
        "DELETE FROM fts_node_property_positions WHERE node_logical_id = ?1",
        rusqlite::params![logical_id],
    )?;
    let table = fathomdb_schema::fts_kind_table_name(kind);
    let tok = fathomdb_schema::DEFAULT_FTS_TOKENIZER;
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS {table} \
         USING fts5(node_logical_id UNINDEXED, text_content, tokenize = '{tok}')"
    ))?;
    conn.execute(
        &format!("INSERT INTO {table} (node_logical_id, text_content) VALUES (?1, ?2)"),
        rusqlite::params![logical_id, text],
    )?;
    for pos in &positions {
        conn.execute(
            "INSERT INTO fts_node_property_positions \
             (node_logical_id, kind, start_offset, end_offset, leaf_path) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                logical_id,
                kind,
                i64::try_from(pos.start_offset).unwrap_or(i64::MAX),
                i64::try_from(pos.end_offset).unwrap_or(i64::MAX),
                pos.leaf_path,
            ],
        )?;
    }
    Ok(1)
}

fn count_source_ref(
    conn: &rusqlite::Connection,
    table: &str,
    source_ref: &str,
) -> Result<usize, EngineError> {
    let sql = match table {
        "nodes" => "SELECT count(*) FROM nodes WHERE source_ref = ?1",
        "edges" => "SELECT count(*) FROM edges WHERE source_ref = ?1",
        "actions" => "SELECT count(*) FROM actions WHERE source_ref = ?1",
        "operational_mutations" => {
            "SELECT count(*) FROM operational_mutations WHERE source_ref = ?1"
        }
        other => return Err(EngineError::Bridge(format!("unknown table: {other}"))),
    };
    let count: i64 = conn.query_row(sql, [source_ref], |row| row.get(0))?;
    // FIX(review): was `count as usize` — unsound cast.
    // Chose option (C) here: propagate error since this is a user-facing helper.
    usize::try_from(count)
        .map_err(|_| EngineError::Bridge(format!("count overflow for table {table}: {count}")))
}

fn collect_strings_tx(
    tx: &rusqlite::Transaction<'_>,
    sql: &str,
    value: &str,
) -> Result<Vec<String>, EngineError> {
    let mut stmt = tx.prepare(sql)?;
    let rows = stmt.query_map([value], |row| row.get::<_, String>(0))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(EngineError::from)
}

/// NOTE(review): sql parameter must be a hardcoded query string, never user input.
/// Options: (A) doc comment, (B) whitelist refactor like `count_source_ref`, (C) leave as-is.
/// Chose (A): function is private, only called with hardcoded SQL from `trace_source`.
/// Whitelist refactor not practical — queries have different SELECT/ORDER BY per table.
fn collect_strings(
    conn: &rusqlite::Connection,
    sql: &str,
    param: &str,
) -> Result<Vec<String>, EngineError> {
    let mut stmt = conn.prepare(sql)?;
    let values = stmt
        .query_map([param], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(values)
}

fn collect_edge_logical_ids_for_restore(
    tx: &rusqlite::Transaction<'_>,
    logical_id: &str,
    retire_source_ref: Option<&str>,
    retire_created_at: i64,
    retire_event_rowid: i64,
) -> Result<Vec<String>, EngineError> {
    let mut stmt = tx.prepare(
        "SELECT DISTINCT e.logical_id \
         FROM edges e \
         JOIN provenance_events p \
           ON p.subject = e.logical_id \
          AND p.event_type = 'edge_retire' \
          AND ( \
                p.created_at > ?3 \
                OR (p.created_at = ?3 AND p.rowid >= ?4) \
          ) \
          AND ((?2 IS NULL AND p.source_ref IS NULL) OR p.source_ref = ?2) \
         WHERE e.superseded_at IS NOT NULL \
           AND (e.source_logical_id = ?1 OR e.target_logical_id = ?1) \
           AND NOT EXISTS ( \
                SELECT 1 FROM edges active \
                WHERE active.logical_id = e.logical_id \
                  AND active.superseded_at IS NULL \
           ) \
         ORDER BY e.logical_id",
    )?;
    let edge_ids = stmt
        .query_map(
            rusqlite::params![
                logical_id,
                retire_source_ref,
                retire_created_at,
                retire_event_rowid
            ],
            |row| row.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(edge_ids)
}

/// Restores edges for a node being restored, skipping any whose counterpart
/// endpoint is not active (e.g. still retired or purged).
fn restore_validated_edges(
    tx: &rusqlite::Transaction<'_>,
    logical_id: &str,
    retire_source_ref: Option<&str>,
    retire_created_at: i64,
    retire_event_rowid: i64,
) -> Result<(usize, Vec<SkippedEdge>), EngineError> {
    let edge_logical_ids = collect_edge_logical_ids_for_restore(
        tx,
        logical_id,
        retire_source_ref,
        retire_created_at,
        retire_event_rowid,
    )?;
    let mut restored = 0usize;
    let mut skipped = Vec::new();
    for edge_logical_id in &edge_logical_ids {
        let edge_detail: Option<(String, String, String)> = tx
            .query_row(
                "SELECT row_id, source_logical_id, target_logical_id FROM edges \
                 WHERE logical_id = ?1 AND superseded_at IS NOT NULL \
                 ORDER BY superseded_at DESC, created_at DESC, rowid DESC LIMIT 1",
                [edge_logical_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((edge_row_id, source_lid, target_lid)) = edge_detail else {
            continue;
        };
        let other_endpoint = if source_lid == logical_id {
            &target_lid
        } else {
            &source_lid
        };
        let endpoint_active: bool = tx
            .query_row(
                "SELECT 1 FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL LIMIT 1",
                [other_endpoint.as_str()],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if !endpoint_active {
            skipped.push(SkippedEdge {
                edge_logical_id: edge_logical_id.clone(),
                missing_endpoint: other_endpoint.clone(),
            });
            continue;
        }
        restored += tx.execute(
            "UPDATE edges SET superseded_at = NULL WHERE row_id = ?1",
            [edge_row_id.as_str()],
        )?;
    }
    Ok((restored, skipped))
}

#[cfg(feature = "sqlite-vec")]
fn count_vec_rows_for_logical_id(
    tx: &rusqlite::Transaction<'_>,
    logical_id: &str,
) -> Result<usize, EngineError> {
    // Look up the kind for this logical_id to derive the per-kind vec table name.
    let kind: Option<String> = tx
        .query_row(
            "SELECT kind FROM nodes WHERE logical_id = ?1 LIMIT 1",
            [logical_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(kind) = kind else {
        return Ok(0);
    };
    let table_name = fathomdb_schema::vec_kind_table_name(&kind);
    match tx.query_row(
        &format!(
            "SELECT count(*) FROM {table_name} v \
             JOIN chunks c ON c.id = v.chunk_id \
             WHERE c.node_logical_id = ?1"
        ),
        [logical_id],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(count) => Ok(i64_to_usize(count)),
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
            if msg.contains(&table_name) || msg.contains("no such module: vec0") =>
        {
            Ok(0)
        }
        Err(error) => Err(EngineError::Sqlite(error)),
    }
}

#[cfg(not(feature = "sqlite-vec"))]
#[allow(clippy::unnecessary_wraps)]
fn count_vec_rows_for_logical_id(
    _tx: &rusqlite::Transaction<'_>,
    _logical_id: &str,
) -> Result<usize, EngineError> {
    Ok(0)
}

#[cfg(feature = "sqlite-vec")]
fn delete_vec_rows_for_logical_id(
    tx: &rusqlite::Transaction<'_>,
    logical_id: &str,
) -> Result<usize, EngineError> {
    // Look up the kind for this logical_id to derive the per-kind vec table name.
    let kind: Option<String> = tx
        .query_row(
            "SELECT kind FROM nodes WHERE logical_id = ?1 LIMIT 1",
            [logical_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(kind) = kind else {
        return Ok(0);
    };
    let table_name = fathomdb_schema::vec_kind_table_name(&kind);
    match tx.execute(
        &format!(
            "DELETE FROM {table_name} WHERE chunk_id IN (SELECT id FROM chunks WHERE node_logical_id = ?1)"
        ),
        [logical_id],
    ) {
        Ok(count) => Ok(count),
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
            if msg.contains(&table_name) || msg.contains("no such module: vec0") =>
        {
            Ok(0)
        }
        Err(error) => Err(EngineError::Sqlite(error)),
    }
}

#[cfg(not(feature = "sqlite-vec"))]
#[allow(clippy::unnecessary_wraps)]
fn delete_vec_rows_for_logical_id(
    _tx: &rusqlite::Transaction<'_>,
    _logical_id: &str,
) -> Result<usize, EngineError> {
    Ok(0)
}
