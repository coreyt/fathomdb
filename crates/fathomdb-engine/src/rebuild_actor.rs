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
        Err(_error) => {
            trace_error!(error = %_error, "rebuild thread: database connection failed");
            return;
        }
    };

    if let Err(_error) = schema_manager.bootstrap(&conn) {
        trace_error!(error = %_error, "rebuild thread: schema bootstrap failed");
        return;
    }

    for req in receiver {
        trace_info!(kind = %req.kind, schema_id = req.schema_id, "rebuild task started");
        match run_rebuild(&mut conn, &req) {
            Ok(()) => {
                trace_info!(kind = %req.kind, "rebuild task reached SWAPPING");
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
        if elapsed_ms > 0 {
            let new_size = (batch_size as u128 * BATCH_TARGET_MS / elapsed_ms).clamp(100, 50_000);
            batch_size = usize::try_from(new_size).unwrap_or(50_000);
        }

        offset += i64::try_from(batch_len).unwrap_or(i64::MAX);

        // If the batch was smaller than the limit used for THIS query, we've reached the end.
        if batch_len < limit_used {
            break;
        }
    }

    // Step 4: mark SWAPPING (this pack ends here — no actual swap yet).
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
