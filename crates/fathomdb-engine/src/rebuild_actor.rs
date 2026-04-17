/// Background actor that serializes async property-FTS rebuild tasks.
///
/// Modeled exactly on [`crate::writer::WriterActor`]: one OS thread,
/// `std::sync::mpsc`, `JoinHandle` for shutdown.  No tokio.
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use fathomdb_schema::SchemaManager;
use rusqlite::OptionalExtension;

use crate::{EngineError, sqlite};

/// Mode passed to `register_fts_property_schema_with_entries`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RebuildMode {
    /// Legacy behavior: full rebuild runs inside the register transaction.
    Eager,
    /// 0.4.1+: schema is persisted synchronously; rebuild runs in background.
    #[default]
    Async,
}

/// A request to rebuild property-FTS for a single kind.
#[derive(Debug)]
pub struct RebuildRequest {
    pub kind: String,
    pub schema_id: i64,
}

/// Single-threaded actor that processes property-FTS rebuild requests one at
/// a time.  Shutdown is cooperative: drop the sender side to close the channel,
/// then join the thread.
///
/// The `RebuildActor` owns the `JoinHandle` only. The `SyncSender` lives in
/// [`crate::admin::AdminService`] so the service can enqueue rebuild requests
/// directly without going through the runtime.  The channel is created by
/// [`RebuildActor::create_channel`] and the two halves are distributed by
/// [`crate::runtime::EngineRuntime::open`].
#[derive(Debug)]
pub struct RebuildActor {
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl RebuildActor {
    /// Create the mpsc channel used to communicate with the rebuild thread.
    ///
    /// Returns `(sender, actor)`.  The sender is given to
    /// [`crate::admin::AdminService`]; the actor is kept in
    /// [`crate::runtime::EngineRuntime`] for lifecycle management.
    ///
    /// # Errors
    /// Returns [`EngineError::Io`] if the thread cannot be spawned.
    pub fn start(
        path: impl AsRef<Path>,
        schema_manager: Arc<SchemaManager>,
        receiver: mpsc::Receiver<RebuildRequest>,
    ) -> Result<Self, EngineError> {
        let database_path = path.as_ref().to_path_buf();

        let handle = thread::Builder::new()
            .name("fathomdb-rebuild".to_owned())
            .spawn(move || {
                rebuild_loop(&database_path, &schema_manager, receiver);
            })
            .map_err(EngineError::Io)?;

        Ok(Self {
            thread_handle: Some(handle),
        })
    }
}

impl Drop for RebuildActor {
    fn drop(&mut self) {
        // The sender was already closed by AdminService (or dropped when the
        // engine closes).  Just join the thread.
        if let Some(handle) = self.thread_handle.take() {
            match handle.join() {
                Ok(()) => {}
                Err(payload) => {
                    if std::thread::panicking() {
                        trace_warn!(
                            "rebuild thread panicked during shutdown (suppressed: already panicking)"
                        );
                    } else {
                        std::panic::resume_unwind(payload);
                    }
                }
            }
        }
    }
}

// ── rebuild loop ────────────────────────────────────────────────────────────

/// Target wall-clock time for each batch transaction.
const BATCH_TARGET_MS: u128 = 1000;
/// Initial batch size.
const INITIAL_BATCH_SIZE: usize = 5000;

fn rebuild_loop(
    database_path: &Path,
    schema_manager: &Arc<SchemaManager>,
    receiver: mpsc::Receiver<RebuildRequest>,
) {
    trace_info!("rebuild thread started");

    let mut conn = match sqlite::open_connection(database_path) {
        Ok(conn) => conn,
        #[allow(clippy::used_underscore_binding)]
        Err(_error) => {
            trace_error!(error = %_error, "rebuild thread: database connection failed");
            return;
        }
    };

    #[allow(clippy::used_underscore_binding)]
    if let Err(_error) = schema_manager.bootstrap(&conn) {
        trace_error!(error = %_error, "rebuild thread: schema bootstrap failed");
        return;
    }

    for req in receiver {
        trace_info!(kind = %req.kind, schema_id = req.schema_id, "rebuild task started");
        match run_rebuild(&mut conn, &req) {
            Ok(()) => {
                trace_info!(kind = %req.kind, "rebuild task COMPLETE");
            }
            Err(error) => {
                trace_error!(kind = %req.kind, error = %error, "rebuild task failed");
                let _ = mark_failed(&conn, &req.kind, &error.to_string());
            }
        }
    }

    trace_info!("rebuild thread exiting");
}

#[allow(clippy::too_many_lines)]
fn run_rebuild(conn: &mut rusqlite::Connection, req: &RebuildRequest) -> Result<(), EngineError> {
    // Step 1: mark BUILDING.
    {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE fts_property_rebuild_state SET state = 'BUILDING' \
             WHERE kind = ?1 AND schema_id = ?2",
            rusqlite::params![req.kind, req.schema_id],
        )?;
        tx.commit()?;
    }

    // Step 2: count nodes for this kind (plain SELECT, no tx needed).
    let rows_total: i64 = conn.query_row(
        "SELECT count(*) FROM nodes WHERE kind = ?1 AND superseded_at IS NULL",
        rusqlite::params![req.kind],
        |r| r.get(0),
    )?;

    {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE fts_property_rebuild_state SET rows_total = ?1 WHERE kind = ?2",
            rusqlite::params![rows_total, req.kind],
        )?;
        tx.commit()?;
    }

    // Load the schema for this kind (plain SELECT).
    let (paths_json, separator): (String, String) = conn
        .query_row(
            "SELECT property_paths_json, separator FROM fts_property_schemas WHERE kind = ?1",
            rusqlite::params![req.kind],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| {
            EngineError::Bridge(format!("rebuild: schema for kind '{}' missing", req.kind))
        })?;
    let schema = crate::writer::parse_property_schema_json(&paths_json, &separator);

    // Step 3: batch-iterate nodes, insert into staging.
    let mut offset: i64 = 0;
    let mut batch_size = INITIAL_BATCH_SIZE;
    let mut rows_done: i64 = 0;

    loop {
        // Fetch a batch of node logical_ids + properties (plain SELECT — no tx needed for reads).
        let batch: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT logical_id, properties FROM nodes \
                 WHERE kind = ?1 AND superseded_at IS NULL \
                 ORDER BY logical_id \
                 LIMIT ?2 OFFSET ?3",
            )?;
            stmt.query_map(
                rusqlite::params![
                    req.kind,
                    i64::try_from(batch_size).unwrap_or(i64::MAX),
                    offset
                ],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )?
            .collect::<Result<Vec<_>, _>>()?
        };

        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len();
        let batch_start = Instant::now();

        // Insert staging rows in a single short transaction.
        {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

            let has_weights = schema.paths.iter().any(|p| p.weight.is_some());

            for (logical_id, properties_str) in &batch {
                let props: serde_json::Value =
                    serde_json::from_str(properties_str).unwrap_or_default();
                let (text, positions, _stats) =
                    crate::writer::extract_property_fts(&props, &schema);

                // Serialize positions to a compact JSON blob for later use at swap time.
                let positions_blob: Option<Vec<u8>> = if positions.is_empty() {
                    None
                } else {
                    let v: Vec<(usize, usize, &str)> = positions
                        .iter()
                        .map(|p| (p.start_offset, p.end_offset, p.leaf_path.as_str()))
                        .collect();
                    serde_json::to_vec(&v).ok()
                };

                let text_content = text.unwrap_or_default();

                if has_weights {
                    let cols = crate::writer::extract_property_fts_columns(&props, &schema);
                    let json_map: serde_json::Map<String, serde_json::Value> = cols
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::Value::String(v)))
                        .collect();
                    let columns_json =
                        serde_json::to_string(&serde_json::Value::Object(json_map)).ok();
                    tx.execute(
                        "INSERT INTO fts_property_rebuild_staging \
                         (kind, node_logical_id, text_content, positions_blob, columns_json) \
                         VALUES (?1, ?2, ?3, ?4, ?5) \
                         ON CONFLICT(kind, node_logical_id) DO UPDATE \
                         SET text_content = excluded.text_content, \
                             positions_blob = excluded.positions_blob, \
                             columns_json = excluded.columns_json",
                        rusqlite::params![
                            req.kind,
                            logical_id,
                            text_content,
                            positions_blob,
                            columns_json
                        ],
                    )?;
                } else {
                    tx.execute(
                        "INSERT INTO fts_property_rebuild_staging \
                         (kind, node_logical_id, text_content, positions_blob) \
                         VALUES (?1, ?2, ?3, ?4) \
                         ON CONFLICT(kind, node_logical_id) DO UPDATE \
                         SET text_content = excluded.text_content, \
                             positions_blob = excluded.positions_blob",
                        rusqlite::params![req.kind, logical_id, text_content, positions_blob],
                    )?;
                }
            }

            rows_done += i64::try_from(batch_len).unwrap_or(i64::MAX);
            let now_ms = now_unix_ms();
            tx.execute(
                "UPDATE fts_property_rebuild_state \
                 SET rows_done = ?1, last_progress_at = ?2 \
                 WHERE kind = ?3",
                rusqlite::params![rows_done, now_ms, req.kind],
            )?;
            tx.commit()?;
        }

        let elapsed_ms = batch_start.elapsed().as_millis();
        // Save the limit used for THIS batch before adjusting for the next one.
        let limit_used = batch_size;
        // Dynamically adjust batch size to target ~1s per batch.
        if let Some(new_size) = (batch_size as u128 * BATCH_TARGET_MS).checked_div(elapsed_ms) {
            let new_size = new_size.clamp(100, 50_000);
            batch_size = usize::try_from(new_size).unwrap_or(50_000);
        }

        offset += i64::try_from(batch_len).unwrap_or(i64::MAX);

        // If the batch was smaller than the limit used for THIS query, we've reached the end.
        if batch_len < limit_used {
            break;
        }
    }

    // Step 4: mark SWAPPING.
    {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let now_ms = now_unix_ms();
        tx.execute(
            "UPDATE fts_property_rebuild_state \
             SET state = 'SWAPPING', last_progress_at = ?1 \
             WHERE kind = ?2",
            rusqlite::params![now_ms, req.kind],
        )?;
        tx.commit()?;
    }

    // Step 5: Final swap — atomic IMMEDIATE transaction replacing live FTS rows.
    {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

        let table = fathomdb_schema::fts_kind_table_name(&req.kind);

        // Ensure the per-kind table exists before the swap (defensive: created at write
        // time normally, but may be absent on async first-time registration with no writes).
        let tokenizer = fathomdb_schema::DEFAULT_FTS_TOKENIZER;
        let create_ddl = format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS {table} USING fts5(\
                node_logical_id UNINDEXED, text_content, \
                tokenize = '{tokenizer}'\
            )"
        );
        tx.execute_batch(&create_ddl)?;

        // 5a. Delete old live FTS rows for this kind (entire per-kind table).
        tx.execute(&format!("DELETE FROM {table}"), [])?;

        // 5b. Insert new rows from staging into the per-kind FTS table.
        // For weighted schemas (columns_json IS NOT NULL), use per-column INSERT.
        // For non-weighted schemas, use bulk INSERT of text_content.
        {
            // Check if any staging rows have columns_json set (weighted schema).
            let has_columns: bool = tx
                .query_row(
                    "SELECT count(*) FROM fts_property_rebuild_staging \
                     WHERE kind = ?1 AND columns_json IS NOT NULL",
                    rusqlite::params![req.kind],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0)
                > 0;

            if has_columns {
                // Weighted schema: per-column INSERT row by row.
                let rows_with_columns: Vec<(String, String)> = {
                    let mut stmt = tx.prepare(
                        "SELECT node_logical_id, columns_json \
                         FROM fts_property_rebuild_staging \
                         WHERE kind = ?1 AND columns_json IS NOT NULL",
                    )?;
                    stmt.query_map(rusqlite::params![req.kind], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
                };

                for (node_id, columns_json_str) in &rows_with_columns {
                    let col_map: serde_json::Map<String, serde_json::Value> =
                        serde_json::from_str(columns_json_str).unwrap_or_default();
                    let col_names: Vec<String> = col_map.keys().cloned().collect();
                    let col_values: Vec<String> = col_names
                        .iter()
                        .map(|k| {
                            col_map
                                .get(k)
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_owned()
                        })
                        .collect();
                    let placeholders: Vec<String> =
                        (2..=col_names.len() + 1).map(|i| format!("?{i}")).collect();
                    let sql = format!(
                        "INSERT INTO {table}(node_logical_id, {cols}) VALUES (?1, {placeholders})",
                        cols = col_names.join(", "),
                        placeholders = placeholders.join(", "),
                    );
                    let mut stmt = tx.prepare(&sql)?;
                    stmt.execute(rusqlite::params_from_iter(
                        std::iter::once(node_id.as_str())
                            .chain(col_values.iter().map(String::as_str)),
                    ))?;
                }

                // For weighted schemas, all staging rows should have columns_json set.
                // Any rows without columns_json are skipped (they have no per-column data
                // and the weighted table has no text_content column).
            } else {
                // Non-weighted schema: bulk INSERT of text_content.
                tx.execute(
                    &format!(
                        "INSERT INTO {table}(node_logical_id, text_content) \
                         SELECT node_logical_id, text_content \
                         FROM fts_property_rebuild_staging WHERE kind = ?1"
                    ),
                    rusqlite::params![req.kind],
                )?;
            }
        }

        // 5c. Delete old position rows for this kind.
        tx.execute(
            "DELETE FROM fts_node_property_positions WHERE kind = ?1",
            rusqlite::params![req.kind],
        )?;

        // 5d. Re-populate fts_node_property_positions from positions_blob in staging.
        {
            let mut stmt = tx.prepare(
                "SELECT node_logical_id, positions_blob \
                 FROM fts_property_rebuild_staging \
                 WHERE kind = ?1 AND positions_blob IS NOT NULL",
            )?;
            let mut ins_pos = tx.prepare(
                "INSERT INTO fts_node_property_positions \
                 (node_logical_id, kind, start_offset, end_offset, leaf_path) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;

            let rows: Vec<(String, Vec<u8>)> = stmt
                .query_map(rusqlite::params![req.kind], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            for (node_logical_id, blob) in &rows {
                // positions_blob is JSON: Vec<(start, end, leaf_path)>
                let positions: Vec<(usize, usize, String)> =
                    serde_json::from_slice(blob).unwrap_or_default();
                for (start, end, leaf_path) in positions {
                    ins_pos.execute(rusqlite::params![
                        node_logical_id,
                        req.kind,
                        i64::try_from(start).unwrap_or(i64::MAX),
                        i64::try_from(end).unwrap_or(i64::MAX),
                        leaf_path,
                    ])?;
                }
            }
        }

        // 5e. Delete staging rows for this kind.
        tx.execute(
            "DELETE FROM fts_property_rebuild_staging WHERE kind = ?1",
            rusqlite::params![req.kind],
        )?;

        // 5f. Mark state COMPLETE.
        let now_ms = now_unix_ms();
        tx.execute(
            "UPDATE fts_property_rebuild_state \
             SET state = 'COMPLETE', last_progress_at = ?1 \
             WHERE kind = ?2",
            rusqlite::params![now_ms, req.kind],
        )?;

        tx.commit()?;
    }

    Ok(())
}

fn mark_failed(
    conn: &rusqlite::Connection,
    kind: &str,
    error_message: &str,
) -> Result<(), EngineError> {
    let now_ms = now_unix_ms();
    conn.execute(
        "UPDATE fts_property_rebuild_state \
         SET state = 'FAILED', error_message = ?1, last_progress_at = ?2 \
         WHERE kind = ?3",
        rusqlite::params![error_message, now_ms, kind],
    )?;
    Ok(())
}

fn now_unix_ms() -> i64 {
    now_unix_ms_pub()
}

/// Public-in-crate version of `now_unix_ms` so `admin.rs` can use it.
pub(crate) fn now_unix_ms_pub() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

/// Rebuild progress row returned from `AdminService::get_property_fts_rebuild_state`.
#[derive(Debug)]
pub struct RebuildStateRow {
    pub kind: String,
    pub schema_id: i64,
    pub state: String,
    pub rows_total: Option<i64>,
    pub rows_done: i64,
    pub started_at: i64,
    pub is_first_registration: bool,
    pub error_message: Option<String>,
}

/// Public progress snapshot returned from
/// [`crate::coordinator::ExecutionCoordinator::get_property_fts_rebuild_progress`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct RebuildProgress {
    /// Current state: `"PENDING"`, `"BUILDING"`, `"SWAPPING"`, `"COMPLETE"`, or `"FAILED"`.
    pub state: String,
    /// Total rows to process. `None` until the actor has counted the nodes.
    pub rows_total: Option<i64>,
    /// Rows processed so far.
    pub rows_done: i64,
    /// Unix milliseconds when the rebuild was registered.
    pub started_at: i64,
    /// Unix milliseconds of the last progress update, if any.
    pub last_progress_at: Option<i64>,
    /// Error message if `state == "FAILED"`.
    pub error_message: Option<String>,
}

/// Run crash recovery: mark any in-progress rebuilds as FAILED and clear their
/// staging rows.  Called by `EngineRuntime::open` before spawning the actor.
///
/// # Errors
/// Returns [`crate::EngineError`] if database access fails.
pub(crate) fn recover_interrupted_rebuilds(
    conn: &rusqlite::Connection,
) -> Result<(), crate::EngineError> {
    // Collect kinds that are in a non-terminal state.
    let kinds: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT kind FROM fts_property_rebuild_state \
             WHERE state IN ('BUILDING', 'SWAPPING')",
        )?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };

    for kind in &kinds {
        conn.execute(
            "DELETE FROM fts_property_rebuild_staging WHERE kind = ?1",
            rusqlite::params![kind],
        )?;
        conn.execute(
            "UPDATE fts_property_rebuild_state \
             SET state = 'FAILED', error_message = 'interrupted by engine restart' \
             WHERE kind = ?1",
            rusqlite::params![kind],
        )?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use rusqlite::Connection;

    use fathomdb_schema::SchemaManager;

    use super::recover_interrupted_rebuilds;

    fn bootstrapped_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("bootstrap");
        conn
    }

    fn insert_rebuild_state(conn: &Connection, kind: &str, state: &str) {
        conn.execute(
            "INSERT INTO fts_property_rebuild_state \
             (kind, schema_id, state, rows_done, started_at, is_first_registration) \
             VALUES (?1, 1, ?2, 0, 0, 0)",
            rusqlite::params![kind, state],
        )
        .expect("insert rebuild state");
    }

    #[test]
    fn pending_row_survives_restart() {
        let conn = bootstrapped_conn();
        insert_rebuild_state(&conn, "MyKind", "PENDING");

        recover_interrupted_rebuilds(&conn).expect("recover");

        let state: String = conn
            .query_row(
                "SELECT state FROM fts_property_rebuild_state WHERE kind = 'MyKind'",
                [],
                |r| r.get(0),
            )
            .expect("state row");
        assert_eq!(state, "PENDING", "PENDING rows must survive engine restart");
    }

    #[test]
    fn building_row_marked_failed_on_restart() {
        let conn = bootstrapped_conn();
        insert_rebuild_state(&conn, "MyKind", "BUILDING");

        recover_interrupted_rebuilds(&conn).expect("recover");

        let state: String = conn
            .query_row(
                "SELECT state FROM fts_property_rebuild_state WHERE kind = 'MyKind'",
                [],
                |r| r.get(0),
            )
            .expect("state row");
        assert_eq!(
            state, "FAILED",
            "BUILDING rows must be marked FAILED on restart"
        );
    }

    #[test]
    fn swapping_row_marked_failed_on_restart() {
        let conn = bootstrapped_conn();
        insert_rebuild_state(&conn, "MyKind", "SWAPPING");

        recover_interrupted_rebuilds(&conn).expect("recover");

        let state: String = conn
            .query_row(
                "SELECT state FROM fts_property_rebuild_state WHERE kind = 'MyKind'",
                [],
                |r| r.get(0),
            )
            .expect("state row");
        assert_eq!(
            state, "FAILED",
            "SWAPPING rows must be marked FAILED on restart"
        );
    }

    // --- A-6: rebuild swap targets per-kind table ---
    #[test]
    fn rebuild_swap_populates_per_kind_table() {
        // This test calls run_rebuild() end-to-end and asserts the final rows
        // land in the per-kind FTS table (fts_props_testkind), NOT in
        // fts_node_properties.
        let mut conn = bootstrapped_conn();
        let kind = "TestKind";
        let table = fathomdb_schema::fts_kind_table_name(kind);

        // NOTE: The per-kind FTS table is intentionally NOT created here.
        // The guard in run_rebuild (Step 5) must create it if absent.

        // Insert a node with extractable property.
        conn.execute(
            "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
             VALUES ('row-1', 'node-1', ?1, '{\"name\":\"hello world\"}', 100, 'seed')",
            rusqlite::params![kind],
        )
        .expect("insert node");

        // Insert schema row.
        let schema_id: i64 = {
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES (?1, '[\"$.name\"]', ' ')",
                rusqlite::params![kind],
            )
            .expect("insert schema");
            conn.query_row(
                "SELECT rowid FROM fts_property_schemas WHERE kind = ?1",
                rusqlite::params![kind],
                |r| r.get(0),
            )
            .expect("schema_id")
        };

        // Insert rebuild state (PENDING).
        conn.execute(
            "INSERT INTO fts_property_rebuild_state \
             (kind, schema_id, state, rows_done, started_at, is_first_registration) \
             VALUES (?1, ?2, 'PENDING', 0, 0, 1)",
            rusqlite::params![kind, schema_id],
        )
        .expect("insert rebuild state");

        // Run the rebuild end-to-end.
        let req = super::RebuildRequest {
            kind: kind.to_owned(),
            schema_id,
        };
        super::run_rebuild(&mut conn, &req).expect("run_rebuild");

        // After A-6: the per-kind table must have the rebuilt row.
        let per_kind_count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE node_logical_id = 'node-1'"),
                [],
                |r| r.get(0),
            )
            .expect("per-kind count");
        assert_eq!(
            per_kind_count, 1,
            "per-kind table must have the rebuilt row after run_rebuild"
        );
    }

    // --- B-3: rebuild_actor uses per-column INSERT for weighted schemas ---

    #[test]
    fn rebuild_actor_uses_per_column_for_weighted_schema() {
        let mut conn = bootstrapped_conn();
        let kind = "Article";
        let table = fathomdb_schema::fts_kind_table_name(kind);

        let title_col = fathomdb_schema::fts_column_name("$.title", false);
        let body_col = fathomdb_schema::fts_column_name("$.body", false);

        // Insert a node with two extractable properties.
        conn.execute(
            "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
             VALUES ('row-1', 'article-1', ?1, '{\"title\":\"Hello\",\"body\":\"World\"}', 100, 'seed')",
            rusqlite::params![kind],
        )
        .expect("insert node");

        // Register schema with weights.
        let paths_json = r#"[{"path":"$.title","mode":"scalar","weight":2.0},{"path":"$.body","mode":"scalar","weight":1.0}]"#;
        let schema_id: i64 = {
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES (?1, ?2, ' ')",
                rusqlite::params![kind, paths_json],
            )
            .expect("insert schema");
            conn.query_row(
                "SELECT rowid FROM fts_property_schemas WHERE kind = ?1",
                rusqlite::params![kind],
                |r| r.get(0),
            )
            .expect("schema_id")
        };

        // Create the weighted per-kind FTS table.
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS {table} USING fts5(\
                node_logical_id UNINDEXED, {title_col}, {body_col}, \
                tokenize = 'porter unicode61 remove_diacritics 2'\
            )"
        ))
        .expect("create weighted per-kind table");

        // Insert rebuild state (PENDING).
        conn.execute(
            "INSERT INTO fts_property_rebuild_state \
             (kind, schema_id, state, rows_done, started_at, is_first_registration) \
             VALUES (?1, ?2, 'PENDING', 0, 0, 1)",
            rusqlite::params![kind, schema_id],
        )
        .expect("insert rebuild state");

        // Run the rebuild end-to-end.
        let req = super::RebuildRequest {
            kind: kind.to_owned(),
            schema_id,
        };
        super::run_rebuild(&mut conn, &req).expect("run_rebuild");

        // The per-kind table must have the rebuilt row.
        let count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE node_logical_id = 'article-1'"),
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 1, "per-kind table must have the rebuilt row");

        // Verify per-column values.
        let (title_val, body_val): (String, String) = conn
            .query_row(
                &format!(
                    "SELECT {title_col}, {body_col} FROM {table} \
                     WHERE node_logical_id = 'article-1'"
                ),
                [],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .expect("select per-column");
        assert_eq!(
            title_val, "Hello",
            "title column must have correct value after rebuild"
        );
        assert_eq!(
            body_val, "World",
            "body column must have correct value after rebuild"
        );
    }
}
