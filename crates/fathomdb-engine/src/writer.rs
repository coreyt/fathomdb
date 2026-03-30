use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender, SyncSender};
use std::thread;
use std::time::Duration;

use fathomdb_schema::SchemaManager;
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::operational::{
    OperationalCollectionKind, OperationalFilterField, OperationalFilterFieldType,
    OperationalFilterMode, OperationalSecondaryIndexDefinition, OperationalValidationContract,
    OperationalValidationMode, extract_secondary_index_entries_for_current,
    extract_secondary_index_entries_for_mutation, parse_operational_secondary_indexes_json,
    parse_operational_validation_contract, validate_operational_payload_against_contract,
};
use crate::{EngineError, ids::new_id, projection::ProjectionTarget, sqlite};

/// A deferred projection backfill task submitted alongside a write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionalProjectionTask {
    /// Which projection to backfill (FTS, vec, etc.).
    pub target: ProjectionTarget,
    /// JSON payload describing the backfill work.
    pub payload: String,
}

/// Policy for handling existing chunks when upserting a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChunkPolicy {
    /// Keep existing chunks and FTS rows.
    #[default]
    Preserve,
    /// Delete existing chunks and FTS rows before inserting new ones.
    Replace,
}

/// Controls how missing `source_ref` values are handled at write time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProvenanceMode {
    /// Emit a warning in the [`WriteReceipt`] but allow the write to proceed.
    #[default]
    Warn,
    /// Reject the write with [`EngineError::InvalidWrite`] if any canonical
    /// insert or retire is missing `source_ref`.
    Require,
}

/// A node to be inserted in a [`WriteRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeInsert {
    pub row_id: String,
    pub logical_id: String,
    pub kind: String,
    pub properties: String,
    pub source_ref: Option<String>,
    /// When true the writer supersedes the current active row for this `logical_id`
    /// before inserting this new version. The supersession and insert are atomic.
    pub upsert: bool,
    /// Controls whether existing chunks and FTS rows are deleted when upsert=true.
    pub chunk_policy: ChunkPolicy,
}

/// An edge to be inserted in a [`WriteRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeInsert {
    pub row_id: String,
    pub logical_id: String,
    pub source_logical_id: String,
    pub target_logical_id: String,
    pub kind: String,
    pub properties: String,
    pub source_ref: Option<String>,
    /// When true the writer supersedes the current active edge for this `logical_id`
    /// before inserting this new version. The supersession and insert are atomic.
    pub upsert: bool,
}

/// A node to be retired (soft-deleted) in a [`WriteRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRetire {
    pub logical_id: String,
    pub source_ref: Option<String>,
}

/// An edge to be retired (soft-deleted) in a [`WriteRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeRetire {
    pub logical_id: String,
    pub source_ref: Option<String>,
}

/// A text chunk to be inserted in a [`WriteRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkInsert {
    pub id: String,
    pub node_logical_id: String,
    pub text_content: String,
    pub byte_start: Option<i64>,
    pub byte_end: Option<i64>,
}

/// A vector embedding to attach to an existing chunk.
///
/// The `chunk_id` must reference a chunk already present in the database or
/// co-submitted in the same [`WriteRequest`].  The embedding is stored in the
/// `vec_nodes_active` virtual table when the `sqlite-vec` feature is enabled;
/// without the feature the insert is silently skipped.
#[derive(Clone, Debug, PartialEq)]
pub struct VecInsert {
    pub chunk_id: String,
    pub embedding: Vec<f32>,
}

/// A mutation to an operational collection submitted as part of a write request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationalWrite {
    Append {
        collection: String,
        record_key: String,
        payload_json: String,
        source_ref: Option<String>,
    },
    Put {
        collection: String,
        record_key: String,
        payload_json: String,
        source_ref: Option<String>,
    },
    Delete {
        collection: String,
        record_key: String,
        source_ref: Option<String>,
    },
}

/// A run to be inserted in a [`WriteRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunInsert {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
    pub upsert: bool,
    pub supersedes_id: Option<String>,
}

/// A step to be inserted in a [`WriteRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepInsert {
    pub id: String,
    pub run_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
    pub upsert: bool,
    pub supersedes_id: Option<String>,
}

/// An action to be inserted in a [`WriteRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionInsert {
    pub id: String,
    pub step_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
    pub upsert: bool,
    pub supersedes_id: Option<String>,
}

// --- Write-request size limits ---
const MAX_NODES: usize = 10_000;
const MAX_EDGES: usize = 10_000;
const MAX_CHUNKS: usize = 50_000;
const MAX_RETIRES: usize = 10_000;
const MAX_RUNTIME_ITEMS: usize = 10_000;
const MAX_OPERATIONAL: usize = 10_000;
const MAX_TOTAL_ITEMS: usize = 100_000;

/// How long `submit` / `touch_last_accessed` wait for the writer thread to reply.
const WRITER_REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// A batch of graph mutations to be applied atomically in a single `SQLite` transaction.
#[derive(Clone, Debug, PartialEq)]
pub struct WriteRequest {
    pub label: String,
    pub nodes: Vec<NodeInsert>,
    pub node_retires: Vec<NodeRetire>,
    pub edges: Vec<EdgeInsert>,
    pub edge_retires: Vec<EdgeRetire>,
    pub chunks: Vec<ChunkInsert>,
    pub runs: Vec<RunInsert>,
    pub steps: Vec<StepInsert>,
    pub actions: Vec<ActionInsert>,
    pub optional_backfills: Vec<OptionalProjectionTask>,
    /// Vector embeddings to persist alongside chunks.  Silently skipped when the
    /// `sqlite-vec` feature is absent.
    pub vec_inserts: Vec<VecInsert>,
    pub operational_writes: Vec<OperationalWrite>,
}

/// Receipt returned after a successful write transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteReceipt {
    pub label: String,
    pub optional_backfill_count: usize,
    pub warnings: Vec<String>,
    pub provenance_warnings: Vec<String>,
}

/// Request to update `last_accessed_at` timestamps for a batch of nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastAccessTouchRequest {
    pub logical_ids: Vec<String>,
    pub touched_at: i64,
    pub source_ref: Option<String>,
}

/// Report from a last-access touch operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastAccessTouchReport {
    pub touched_logical_ids: usize,
    pub touched_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FtsProjectionRow {
    chunk_id: String,
    node_logical_id: String,
    kind: String,
    text_content: String,
}

struct PreparedWrite {
    label: String,
    nodes: Vec<NodeInsert>,
    node_retires: Vec<NodeRetire>,
    edges: Vec<EdgeInsert>,
    edge_retires: Vec<EdgeRetire>,
    chunks: Vec<ChunkInsert>,
    runs: Vec<RunInsert>,
    steps: Vec<StepInsert>,
    actions: Vec<ActionInsert>,
    /// Suppressed when sqlite-vec feature is absent: field is set by `prepare_write` but only
    /// consumed by the cfg-gated apply block.
    #[cfg_attr(not(feature = "sqlite-vec"), allow(dead_code))]
    vec_inserts: Vec<VecInsert>,
    operational_writes: Vec<OperationalWrite>,
    operational_collection_kinds: HashMap<String, OperationalCollectionKind>,
    operational_collection_filter_fields: HashMap<String, Vec<OperationalFilterField>>,
    operational_validation_warnings: Vec<String>,
    /// `node_logical_id` → kind for nodes co-submitted in this request.
    /// Used by `resolve_fts_rows` to avoid a DB round-trip for the common case.
    node_kinds: HashMap<String, String>,
    /// Filled in by `resolve_fts_rows` in the writer thread before BEGIN IMMEDIATE.
    required_fts_rows: Vec<FtsProjectionRow>,
    optional_backfills: Vec<OptionalProjectionTask>,
}

enum WriteMessage {
    Submit {
        prepared: Box<PreparedWrite>,
        reply: Sender<Result<WriteReceipt, EngineError>>,
    },
    TouchLastAccessed {
        request: LastAccessTouchRequest,
        reply: Sender<Result<LastAccessTouchReport, EngineError>>,
    },
}

/// Single-threaded writer that serializes all mutations through one `SQLite` connection.
///
/// On drop, the channel is closed and the writer thread is joined, ensuring all
/// in-flight writes complete and the `SQLite` connection closes cleanly.
#[derive(Debug)]
pub struct WriterActor {
    sender: ManuallyDrop<SyncSender<WriteMessage>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    provenance_mode: ProvenanceMode,
}

impl WriterActor {
    /// # Errors
    /// Returns [`EngineError`] if the writer thread cannot be spawned.
    pub fn start(
        path: impl AsRef<Path>,
        schema_manager: Arc<SchemaManager>,
        provenance_mode: ProvenanceMode,
    ) -> Result<Self, EngineError> {
        let database_path = path.as_ref().to_path_buf();
        let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(256);

        let handle = thread::Builder::new()
            .name("fathomdb-writer".to_owned())
            .spawn(move || writer_loop(&database_path, &schema_manager, receiver))
            .map_err(EngineError::Io)?;

        Ok(Self {
            sender: ManuallyDrop::new(sender),
            thread_handle: Some(handle),
            provenance_mode,
        })
    }

    /// Returns `true` if the writer thread is still running.
    fn is_thread_alive(&self) -> bool {
        self.thread_handle
            .as_ref()
            .is_some_and(|h| !h.is_finished())
    }

    /// Returns `WriterRejected` if the writer thread has exited.
    fn check_thread_alive(&self) -> Result<(), EngineError> {
        if self.is_thread_alive() {
            Ok(())
        } else {
            Err(EngineError::WriterRejected(
                "writer thread has exited".to_owned(),
            ))
        }
    }

    /// # Errors
    /// Returns [`EngineError`] if the write request validation fails, the writer actor has shut
    /// down, or the underlying `SQLite` transaction fails.
    pub fn submit(&self, request: WriteRequest) -> Result<WriteReceipt, EngineError> {
        self.check_thread_alive()?;
        let prepared = prepare_write(request, self.provenance_mode)?;
        let (reply_tx, reply_rx) = mpsc::channel();
        self.sender
            .send(WriteMessage::Submit {
                prepared: Box::new(prepared),
                reply: reply_tx,
            })
            .map_err(|error| EngineError::WriterRejected(error.to_string()))?;

        recv_with_timeout(&reply_rx)
    }

    /// # Errors
    /// Returns [`EngineError`] if validation fails, the writer actor has shut down,
    /// or the underlying `SQLite` transaction fails.
    pub fn touch_last_accessed(
        &self,
        request: LastAccessTouchRequest,
    ) -> Result<LastAccessTouchReport, EngineError> {
        self.check_thread_alive()?;
        prepare_touch_last_accessed(&request, self.provenance_mode)?;
        let (reply_tx, reply_rx) = mpsc::channel();
        self.sender
            .send(WriteMessage::TouchLastAccessed {
                request,
                reply: reply_tx,
            })
            .map_err(|error| EngineError::WriterRejected(error.to_string()))?;

        recv_with_timeout(&reply_rx)
    }
}

#[cfg(not(feature = "tracing"))]
#[allow(clippy::print_stderr)]
fn stderr_panic_notice() {
    eprintln!("fathomdb-writer panicked during shutdown (suppressed: already panicking)");
}

impl Drop for WriterActor {
    fn drop(&mut self) {
        // Phase 1: close the channel so the writer thread's `for msg in receiver`
        // loop exits after finishing any in-progress message.
        // Must happen BEFORE join to avoid deadlock.
        //
        // SAFETY: drop is called exactly once, and no method accesses the sender
        // after drop begins.
        unsafe { ManuallyDrop::drop(&mut self.sender) };

        // Phase 2: join the writer thread to ensure the SQLite connection closes.
        if let Some(handle) = self.thread_handle.take() {
            match handle.join() {
                Ok(()) => {}
                Err(payload) => {
                    if std::thread::panicking() {
                        trace_warn!(
                            "writer thread panicked during shutdown (suppressed: already panicking)"
                        );
                        #[cfg(not(feature = "tracing"))]
                        stderr_panic_notice();
                    } else {
                        std::panic::resume_unwind(payload);
                    }
                }
            }
        }
    }
}

/// Wait for a reply from the writer thread, with a timeout.
fn recv_with_timeout<T>(rx: &mpsc::Receiver<Result<T, EngineError>>) -> Result<T, EngineError> {
    rx.recv_timeout(WRITER_REPLY_TIMEOUT)
        .map_err(|error| {
            EngineError::WriterRejected(match error {
                mpsc::RecvTimeoutError::Timeout => {
                    "write timed out waiting for writer thread reply".to_owned()
                }
                mpsc::RecvTimeoutError::Disconnected => error.to_string(),
            })
        })
        .and_then(|result| result)
}

fn prepare_touch_last_accessed(
    request: &LastAccessTouchRequest,
    mode: ProvenanceMode,
) -> Result<(), EngineError> {
    if request.logical_ids.is_empty() {
        return Err(EngineError::InvalidWrite(
            "touch_last_accessed requires at least one logical_id".to_owned(),
        ));
    }
    for logical_id in &request.logical_ids {
        if logical_id.trim().is_empty() {
            return Err(EngineError::InvalidWrite(
                "touch_last_accessed requires non-empty logical_ids".to_owned(),
            ));
        }
    }
    if mode == ProvenanceMode::Require && request.source_ref.is_none() {
        return Err(EngineError::InvalidWrite(
            "touch_last_accessed requires source_ref when ProvenanceMode::Require is active"
                .to_owned(),
        ));
    }
    Ok(())
}

fn check_require_provenance(request: &WriteRequest) -> Result<(), EngineError> {
    let missing: Vec<String> = request
        .nodes
        .iter()
        .filter(|n| n.source_ref.is_none())
        .map(|n| format!("node '{}'", n.logical_id))
        .chain(
            request
                .node_retires
                .iter()
                .filter(|r| r.source_ref.is_none())
                .map(|r| format!("node retire '{}'", r.logical_id)),
        )
        .chain(
            request
                .edges
                .iter()
                .filter(|e| e.source_ref.is_none())
                .map(|e| format!("edge '{}'", e.logical_id)),
        )
        .chain(
            request
                .edge_retires
                .iter()
                .filter(|r| r.source_ref.is_none())
                .map(|r| format!("edge retire '{}'", r.logical_id)),
        )
        .chain(
            request
                .runs
                .iter()
                .filter(|r| r.source_ref.is_none())
                .map(|r| format!("run '{}'", r.id)),
        )
        .chain(
            request
                .steps
                .iter()
                .filter(|s| s.source_ref.is_none())
                .map(|s| format!("step '{}'", s.id)),
        )
        .chain(
            request
                .actions
                .iter()
                .filter(|a| a.source_ref.is_none())
                .map(|a| format!("action '{}'", a.id)),
        )
        .chain(
            request
                .operational_writes
                .iter()
                .filter(|write| operational_write_source_ref(write).is_none())
                .map(|write| {
                    format!(
                        "operational {} '{}:{}'",
                        operational_write_kind(write),
                        operational_write_collection(write),
                        operational_write_record_key(write)
                    )
                }),
        )
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(EngineError::InvalidWrite(format!(
            "ProvenanceMode::Require: missing source_ref on: {}",
            missing.join(", ")
        )))
    }
}

fn validate_request_size(request: &WriteRequest) -> Result<(), EngineError> {
    if request.nodes.len() > MAX_NODES {
        return Err(EngineError::InvalidWrite(format!(
            "too many nodes: {} exceeds limit of {MAX_NODES}",
            request.nodes.len()
        )));
    }
    if request.edges.len() > MAX_EDGES {
        return Err(EngineError::InvalidWrite(format!(
            "too many edges: {} exceeds limit of {MAX_EDGES}",
            request.edges.len()
        )));
    }
    if request.chunks.len() > MAX_CHUNKS {
        return Err(EngineError::InvalidWrite(format!(
            "too many chunks: {} exceeds limit of {MAX_CHUNKS}",
            request.chunks.len()
        )));
    }
    let retires = request.node_retires.len() + request.edge_retires.len();
    if retires > MAX_RETIRES {
        return Err(EngineError::InvalidWrite(format!(
            "too many retires: {retires} exceeds limit of {MAX_RETIRES}"
        )));
    }
    let runtime_items = request.runs.len() + request.steps.len() + request.actions.len();
    if runtime_items > MAX_RUNTIME_ITEMS {
        return Err(EngineError::InvalidWrite(format!(
            "too many runtime items: {runtime_items} exceeds limit of {MAX_RUNTIME_ITEMS}"
        )));
    }
    if request.operational_writes.len() > MAX_OPERATIONAL {
        return Err(EngineError::InvalidWrite(format!(
            "too many operational writes: {} exceeds limit of {MAX_OPERATIONAL}",
            request.operational_writes.len()
        )));
    }
    let total = request.nodes.len()
        + request.node_retires.len()
        + request.edges.len()
        + request.edge_retires.len()
        + request.chunks.len()
        + request.runs.len()
        + request.steps.len()
        + request.actions.len()
        + request.vec_inserts.len()
        + request.operational_writes.len();
    if total > MAX_TOTAL_ITEMS {
        return Err(EngineError::InvalidWrite(format!(
            "too many total items: {total} exceeds limit of {MAX_TOTAL_ITEMS}"
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn prepare_write(
    request: WriteRequest,
    mode: ProvenanceMode,
) -> Result<PreparedWrite, EngineError> {
    validate_request_size(&request)?;

    // --- ID validation: reject empty IDs ---
    for node in &request.nodes {
        if node.row_id.is_empty() {
            return Err(EngineError::InvalidWrite(
                "NodeInsert has empty row_id".to_owned(),
            ));
        }
        if node.logical_id.is_empty() {
            return Err(EngineError::InvalidWrite(
                "NodeInsert has empty logical_id".to_owned(),
            ));
        }
    }
    for edge in &request.edges {
        if edge.row_id.is_empty() {
            return Err(EngineError::InvalidWrite(
                "EdgeInsert has empty row_id".to_owned(),
            ));
        }
        if edge.logical_id.is_empty() {
            return Err(EngineError::InvalidWrite(
                "EdgeInsert has empty logical_id".to_owned(),
            ));
        }
    }
    for chunk in &request.chunks {
        if chunk.id.is_empty() {
            return Err(EngineError::InvalidWrite(
                "ChunkInsert has empty id".to_owned(),
            ));
        }
        if chunk.text_content.is_empty() {
            return Err(EngineError::InvalidWrite(format!(
                "chunk '{}' has empty text_content; empty chunks are not allowed",
                chunk.id
            )));
        }
    }
    for run in &request.runs {
        if run.id.is_empty() {
            return Err(EngineError::InvalidWrite(
                "RunInsert has empty id".to_owned(),
            ));
        }
    }
    for step in &request.steps {
        if step.id.is_empty() {
            return Err(EngineError::InvalidWrite(
                "StepInsert has empty id".to_owned(),
            ));
        }
    }
    for action in &request.actions {
        if action.id.is_empty() {
            return Err(EngineError::InvalidWrite(
                "ActionInsert has empty id".to_owned(),
            ));
        }
    }
    for vi in &request.vec_inserts {
        if vi.chunk_id.is_empty() {
            return Err(EngineError::InvalidWrite(
                "VecInsert has empty chunk_id".to_owned(),
            ));
        }
        if vi.embedding.is_empty() {
            return Err(EngineError::InvalidWrite(format!(
                "VecInsert for chunk '{}' has empty embedding",
                vi.chunk_id
            )));
        }
    }
    for operational in &request.operational_writes {
        if operational_write_collection(operational).is_empty() {
            return Err(EngineError::InvalidWrite(
                "OperationalWrite has empty collection".to_owned(),
            ));
        }
        if operational_write_record_key(operational).is_empty() {
            return Err(EngineError::InvalidWrite(format!(
                "OperationalWrite for collection '{}' has empty record_key",
                operational_write_collection(operational)
            )));
        }
        match operational {
            OperationalWrite::Append { payload_json, .. }
            | OperationalWrite::Put { payload_json, .. } => {
                if payload_json.is_empty() {
                    return Err(EngineError::InvalidWrite(format!(
                        "OperationalWrite {} '{}:{}' has empty payload_json",
                        operational_write_kind(operational),
                        operational_write_collection(operational),
                        operational_write_record_key(operational)
                    )));
                }
            }
            OperationalWrite::Delete { .. } => {}
        }
    }

    // --- ID validation: reject duplicate row_ids within the request ---
    {
        let mut seen = std::collections::HashSet::new();
        for node in &request.nodes {
            if !seen.insert(node.row_id.as_str()) {
                return Err(EngineError::InvalidWrite(format!(
                    "duplicate row_id '{}' within the same WriteRequest",
                    node.row_id
                )));
            }
        }
        for edge in &request.edges {
            if !seen.insert(edge.row_id.as_str()) {
                return Err(EngineError::InvalidWrite(format!(
                    "duplicate row_id '{}' within the same WriteRequest",
                    edge.row_id
                )));
            }
        }
    }

    // --- ProvenanceMode::Require enforcement ---
    if mode == ProvenanceMode::Require {
        check_require_provenance(&request)?;
    }

    // --- Runtime table upsert validation ---
    for run in &request.runs {
        if run.upsert && run.supersedes_id.is_none() {
            return Err(EngineError::InvalidWrite(format!(
                "run '{}': upsert=true requires supersedes_id to be set",
                run.id
            )));
        }
    }
    for step in &request.steps {
        if step.upsert && step.supersedes_id.is_none() {
            return Err(EngineError::InvalidWrite(format!(
                "step '{}': upsert=true requires supersedes_id to be set",
                step.id
            )));
        }
    }
    for action in &request.actions {
        if action.upsert && action.supersedes_id.is_none() {
            return Err(EngineError::InvalidWrite(format!(
                "action '{}': upsert=true requires supersedes_id to be set",
                action.id
            )));
        }
    }

    let node_kinds = request
        .nodes
        .iter()
        .map(|node| (node.logical_id.clone(), node.kind.clone()))
        .collect::<HashMap<_, _>>();

    Ok(PreparedWrite {
        label: request.label,
        nodes: request.nodes,
        node_retires: request.node_retires,
        edges: request.edges,
        edge_retires: request.edge_retires,
        chunks: request.chunks,
        runs: request.runs,
        steps: request.steps,
        actions: request.actions,
        vec_inserts: request.vec_inserts,
        operational_writes: request.operational_writes,
        operational_collection_kinds: HashMap::new(),
        operational_collection_filter_fields: HashMap::new(),
        operational_validation_warnings: Vec::new(),
        node_kinds,
        required_fts_rows: Vec::new(),
        optional_backfills: request.optional_backfills,
    })
}

fn writer_loop(
    database_path: &Path,
    schema_manager: &Arc<SchemaManager>,
    receiver: mpsc::Receiver<WriteMessage>,
) {
    trace_info!("writer thread started");

    let mut conn = match sqlite::open_connection(database_path) {
        Ok(conn) => conn,
        Err(error) => {
            trace_error!(error = %error, "writer thread: database connection failed");
            reject_all(receiver, &error.to_string());
            return;
        }
    };

    if let Err(error) = schema_manager.bootstrap(&conn) {
        trace_error!(error = %error, "writer thread: schema bootstrap failed");
        reject_all(receiver, &error.to_string());
        return;
    }

    for message in receiver {
        match message {
            WriteMessage::Submit {
                mut prepared,
                reply,
            } => {
                #[cfg(feature = "tracing")]
                let start = std::time::Instant::now();
                let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    resolve_and_apply(&mut conn, &mut prepared)
                }));
                if let Ok(inner) = result {
                    #[allow(unused_variables)]
                    if let Err(error) = &inner {
                        trace_error!(
                            label = %prepared.label,
                            error = %error,
                            "write failed"
                        );
                    } else {
                        trace_info!(
                            label = %prepared.label,
                            nodes = prepared.nodes.len(),
                            edges = prepared.edges.len(),
                            chunks = prepared.chunks.len(),
                            duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                            "write committed"
                        );
                    }
                    let _ = reply.send(inner);
                } else {
                    trace_error!(label = %prepared.label, "writer thread: panic during resolve_and_apply");
                    // Attempt to clean up any open transaction after a panic.
                    let _ = conn.execute_batch("ROLLBACK");
                    let _ = reply.send(Err(EngineError::WriterRejected(
                        "writer thread panic during resolve_and_apply".to_owned(),
                    )));
                }
            }
            WriteMessage::TouchLastAccessed { request, reply } => {
                let result = apply_touch_last_accessed(&mut conn, &request);
                let _ = reply.send(result);
            }
        }
    }

    trace_info!("writer thread shutting down");
}

fn reject_all(receiver: mpsc::Receiver<WriteMessage>, error: &str) {
    for message in receiver {
        match message {
            WriteMessage::Submit { reply, .. } => {
                let _ = reply.send(Err(EngineError::WriterRejected(error.to_string())));
            }
            WriteMessage::TouchLastAccessed { reply, .. } => {
                let _ = reply.send(Err(EngineError::WriterRejected(error.to_string())));
            }
        }
    }
}

/// Resolve FTS projection rows before the write transaction begins.
///
/// For each chunk in the prepared write, determines the node `kind` needed
/// to populate the FTS index. If the node was co-submitted in the same
/// request its kind is taken from `prepared.node_kinds` without a DB query.
/// If the node is pre-existing it is looked up in the database. If the node
/// cannot be found in either place, an `InvalidWrite` error is returned.
fn resolve_fts_rows(
    conn: &rusqlite::Connection,
    prepared: &mut PreparedWrite,
) -> Result<(), EngineError> {
    let retiring_ids: std::collections::HashSet<&str> = prepared
        .node_retires
        .iter()
        .map(|r| r.logical_id.as_str())
        .collect();
    for chunk in &prepared.chunks {
        if retiring_ids.contains(chunk.node_logical_id.as_str()) {
            return Err(EngineError::InvalidWrite(format!(
                "chunk '{}' references node_logical_id '{}' which is being retired in the same \
                 WriteRequest; retire and chunk insertion for the same node must not be combined",
                chunk.id, chunk.node_logical_id
            )));
        }
    }
    for chunk in &prepared.chunks {
        let kind = if let Some(k) = prepared.node_kinds.get(&chunk.node_logical_id) {
            k.clone()
        } else {
            match conn.query_row(
                "SELECT kind FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
                params![chunk.node_logical_id],
                |row| row.get::<_, String>(0),
            ) {
                Ok(kind) => kind,
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    return Err(EngineError::InvalidWrite(format!(
                        "chunk '{}' references node_logical_id '{}' that is not present in this \
                         write request or the database \
                         (v1 limitation: chunks and their nodes must be submitted together or the \
                         node must already exist)",
                        chunk.id, chunk.node_logical_id
                    )));
                }
                Err(e) => return Err(EngineError::Sqlite(e)),
            }
        };
        prepared.required_fts_rows.push(FtsProjectionRow {
            chunk_id: chunk.id.clone(),
            node_logical_id: chunk.node_logical_id.clone(),
            kind,
            text_content: chunk.text_content.clone(),
        });
    }
    trace_debug!(
        fts_rows = prepared.required_fts_rows.len(),
        chunks_processed = prepared.chunks.len(),
        "fts row resolution completed"
    );
    Ok(())
}

fn resolve_operational_writes(
    conn: &rusqlite::Connection,
    prepared: &mut PreparedWrite,
) -> Result<(), EngineError> {
    let mut collection_kinds = HashMap::new();
    let mut collection_filter_fields = HashMap::new();
    let mut collection_validation_contracts = HashMap::new();
    for write in &prepared.operational_writes {
        let collection = operational_write_collection(write);
        if !collection_kinds.contains_key(collection) {
            let maybe_row: Option<(String, Option<i64>, String, String)> = conn
                .query_row(
                    "SELECT kind, disabled_at, filter_fields_json, validation_json FROM operational_collections WHERE name = ?1",
                    params![collection],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(EngineError::Sqlite)?;
            let (kind_text, disabled_at, filter_fields_json, validation_json) = maybe_row
                .ok_or_else(|| {
                    EngineError::InvalidWrite(format!(
                        "operational collection '{collection}' is not registered"
                    ))
                })?;
            if disabled_at.is_some() {
                return Err(EngineError::InvalidWrite(format!(
                    "operational collection '{collection}' is disabled"
                )));
            }
            let kind = OperationalCollectionKind::try_from(kind_text.as_str())
                .map_err(EngineError::InvalidWrite)?;
            let filter_fields = parse_operational_filter_fields(&filter_fields_json)?;
            let validation_contract = parse_operational_validation_contract(&validation_json)
                .map_err(EngineError::InvalidWrite)?;
            collection_kinds.insert(collection.to_owned(), kind);
            collection_filter_fields.insert(collection.to_owned(), filter_fields);
            collection_validation_contracts.insert(collection.to_owned(), validation_contract);
        }

        let kind = collection_kinds.get(collection).copied().ok_or_else(|| {
            EngineError::InvalidWrite("missing operational collection kind".to_owned())
        })?;
        match (kind, write) {
            (OperationalCollectionKind::AppendOnlyLog, OperationalWrite::Append { .. })
            | (
                OperationalCollectionKind::LatestState,
                OperationalWrite::Put { .. } | OperationalWrite::Delete { .. },
            ) => {}
            (OperationalCollectionKind::AppendOnlyLog, _) => {
                return Err(EngineError::InvalidWrite(format!(
                    "operational collection '{collection}' is append_only_log and only accepts Append"
                )));
            }
            (OperationalCollectionKind::LatestState, _) => {
                return Err(EngineError::InvalidWrite(format!(
                    "operational collection '{collection}' is latest_state and only accepts Put/Delete"
                )));
            }
        }
        if let Some(Some(contract)) = collection_validation_contracts.get(collection) {
            let _ = check_operational_write_against_contract(write, contract)?;
        }
    }
    prepared.operational_collection_kinds = collection_kinds;
    prepared.operational_collection_filter_fields = collection_filter_fields;
    Ok(())
}

fn parse_operational_filter_fields(
    filter_fields_json: &str,
) -> Result<Vec<OperationalFilterField>, EngineError> {
    let fields: Vec<OperationalFilterField> =
        serde_json::from_str(filter_fields_json).map_err(|error| {
            EngineError::InvalidWrite(format!("invalid filter_fields_json: {error}"))
        })?;
    let mut seen = std::collections::HashSet::new();
    for field in &fields {
        if field.name.trim().is_empty() {
            return Err(EngineError::InvalidWrite(
                "filter_fields_json field names must not be empty".to_owned(),
            ));
        }
        if !seen.insert(field.name.as_str()) {
            return Err(EngineError::InvalidWrite(format!(
                "filter_fields_json contains duplicate field '{}'",
                field.name
            )));
        }
        if field.modes.is_empty() {
            return Err(EngineError::InvalidWrite(format!(
                "filter_fields_json field '{}' must declare at least one mode",
                field.name
            )));
        }
        if field.modes.contains(&OperationalFilterMode::Prefix)
            && field.field_type != OperationalFilterFieldType::String
        {
            return Err(EngineError::InvalidWrite(format!(
                "filter field '{}' only supports prefix for string types",
                field.name
            )));
        }
    }
    Ok(fields)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OperationalFilterValueRow {
    field_name: String,
    string_value: Option<String>,
    integer_value: Option<i64>,
}

fn extract_operational_filter_values(
    filter_fields: &[OperationalFilterField],
    payload_json: &str,
) -> Vec<OperationalFilterValueRow> {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(payload_json) else {
        return Vec::new();
    };
    let Some(object) = parsed.as_object() else {
        return Vec::new();
    };

    filter_fields
        .iter()
        .filter_map(|field| {
            let value = object.get(&field.name)?;
            match field.field_type {
                OperationalFilterFieldType::String => {
                    value
                        .as_str()
                        .map(|string_value| OperationalFilterValueRow {
                            field_name: field.name.clone(),
                            string_value: Some(string_value.to_owned()),
                            integer_value: None,
                        })
                }
                OperationalFilterFieldType::Integer | OperationalFilterFieldType::Timestamp => {
                    value
                        .as_i64()
                        .map(|integer_value| OperationalFilterValueRow {
                            field_name: field.name.clone(),
                            string_value: None,
                            integer_value: Some(integer_value),
                        })
                }
            }
        })
        .collect()
}

fn resolve_and_apply(
    conn: &mut rusqlite::Connection,
    prepared: &mut PreparedWrite,
) -> Result<WriteReceipt, EngineError> {
    resolve_fts_rows(conn, prepared)?;
    resolve_operational_writes(conn, prepared)?;
    apply_write(conn, prepared)
}

fn apply_touch_last_accessed(
    conn: &mut rusqlite::Connection,
    request: &LastAccessTouchRequest,
) -> Result<LastAccessTouchReport, EngineError> {
    let mut seen = std::collections::HashSet::new();
    let logical_ids = request
        .logical_ids
        .iter()
        .filter(|logical_id| seen.insert(logical_id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    for logical_id in &logical_ids {
        let exists = tx
            .query_row(
                "SELECT 1 FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL LIMIT 1",
                params![logical_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(EngineError::InvalidWrite(format!(
                "touch_last_accessed requires an active node for logical_id '{logical_id}'"
            )));
        }
    }

    {
        let mut upsert_metadata = tx.prepare_cached(
            "INSERT INTO node_access_metadata (logical_id, last_accessed_at, updated_at) \
             VALUES (?1, ?2, ?2) \
             ON CONFLICT(logical_id) DO UPDATE SET \
                 last_accessed_at = excluded.last_accessed_at, \
                 updated_at = excluded.updated_at",
        )?;
        let mut insert_provenance = tx.prepare_cached(
            "INSERT INTO provenance_events (id, event_type, subject, source_ref, metadata_json) \
             VALUES (?1, 'node_last_accessed_touched', ?2, ?3, ?4)",
        )?;
        for logical_id in &logical_ids {
            upsert_metadata.execute(params![logical_id, request.touched_at])?;
            insert_provenance.execute(params![
                new_id(),
                logical_id,
                request.source_ref.as_deref(),
                format!("{{\"touched_at\":{}}}", request.touched_at),
            ])?;
        }
    }

    tx.commit()?;
    Ok(LastAccessTouchReport {
        touched_logical_ids: logical_ids.len(),
        touched_at: request.touched_at,
    })
}

fn ensure_operational_collections_writable(
    tx: &rusqlite::Transaction<'_>,
    prepared: &PreparedWrite,
) -> Result<(), EngineError> {
    for collection in prepared.operational_collection_kinds.keys() {
        let disabled_at: Option<Option<i64>> = tx
            .query_row(
                "SELECT disabled_at FROM operational_collections WHERE name = ?1",
                params![collection],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()?;
        match disabled_at {
            Some(Some(_)) => {
                return Err(EngineError::InvalidWrite(format!(
                    "operational collection '{collection}' is disabled"
                )));
            }
            Some(None) => {}
            None => {
                return Err(EngineError::InvalidWrite(format!(
                    "operational collection '{collection}' is not registered"
                )));
            }
        }
    }
    Ok(())
}

fn validate_operational_writes_against_live_contracts(
    tx: &rusqlite::Transaction<'_>,
    prepared: &PreparedWrite,
) -> Result<Vec<String>, EngineError> {
    let mut collection_validation_contracts =
        HashMap::<String, Option<OperationalValidationContract>>::new();
    for collection in prepared.operational_collection_kinds.keys() {
        let validation_json: String = tx
            .query_row(
                "SELECT validation_json FROM operational_collections WHERE name = ?1",
                params![collection],
                |row| row.get(0),
            )
            .map_err(EngineError::Sqlite)?;
        let validation_contract = parse_operational_validation_contract(&validation_json)
            .map_err(EngineError::InvalidWrite)?;
        collection_validation_contracts.insert(collection.clone(), validation_contract);
    }

    let mut warnings = Vec::new();
    for write in &prepared.operational_writes {
        if let Some(Some(contract)) =
            collection_validation_contracts.get(operational_write_collection(write))
            && let Some(warning) = check_operational_write_against_contract(write, contract)?
        {
            warnings.push(warning);
        }
    }

    Ok(warnings)
}

fn load_live_operational_secondary_indexes(
    tx: &rusqlite::Transaction<'_>,
    prepared: &PreparedWrite,
) -> Result<HashMap<String, Vec<OperationalSecondaryIndexDefinition>>, EngineError> {
    let mut collection_indexes = HashMap::new();
    for (collection, collection_kind) in &prepared.operational_collection_kinds {
        let secondary_indexes_json: String = tx
            .query_row(
                "SELECT secondary_indexes_json FROM operational_collections WHERE name = ?1",
                params![collection],
                |row| row.get(0),
            )
            .map_err(EngineError::Sqlite)?;
        let indexes =
            parse_operational_secondary_indexes_json(&secondary_indexes_json, *collection_kind)
                .map_err(EngineError::InvalidWrite)?;
        collection_indexes.insert(collection.clone(), indexes);
    }
    Ok(collection_indexes)
}

fn check_operational_write_against_contract(
    write: &OperationalWrite,
    contract: &OperationalValidationContract,
) -> Result<Option<String>, EngineError> {
    if contract.mode == OperationalValidationMode::Disabled {
        return Ok(None);
    }

    let (payload_json, collection, record_key) = match write {
        OperationalWrite::Append {
            collection,
            record_key,
            payload_json,
            ..
        }
        | OperationalWrite::Put {
            collection,
            record_key,
            payload_json,
            ..
        } => (
            payload_json.as_str(),
            collection.as_str(),
            record_key.as_str(),
        ),
        OperationalWrite::Delete { .. } => return Ok(None),
    };

    match validate_operational_payload_against_contract(contract, payload_json) {
        Ok(()) => Ok(None),
        Err(message) => match contract.mode {
            OperationalValidationMode::Disabled => Ok(None),
            OperationalValidationMode::ReportOnly => Ok(Some(format!(
                "invalid operational payload for collection '{collection}' {kind} '{record_key}': {message}",
                kind = operational_write_kind(write)
            ))),
            OperationalValidationMode::Enforce => Err(EngineError::InvalidWrite(format!(
                "invalid operational payload for collection '{collection}' {kind} '{record_key}': {message}",
                kind = operational_write_kind(write)
            ))),
        },
    }
}

#[allow(clippy::too_many_lines)]
fn apply_write(
    conn: &mut rusqlite::Connection,
    prepared: &mut PreparedWrite,
) -> Result<WriteReceipt, EngineError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // Node retires: clear rebuildable FTS rows, preserve chunks/vec for possible restore,
    // mark superseded, record audit event.
    {
        let mut del_fts = tx.prepare_cached("DELETE FROM fts_nodes WHERE node_logical_id = ?1")?;
        let mut sup_node = tx.prepare_cached(
            "UPDATE nodes SET superseded_at = unixepoch() \
             WHERE logical_id = ?1 AND superseded_at IS NULL",
        )?;
        let mut ins_event = tx.prepare_cached(
            "INSERT INTO provenance_events (id, event_type, subject, source_ref) \
             VALUES (?1, 'node_retire', ?2, ?3)",
        )?;
        for retire in &prepared.node_retires {
            del_fts.execute(params![retire.logical_id])?;
            sup_node.execute(params![retire.logical_id])?;
            ins_event.execute(params![new_id(), retire.logical_id, retire.source_ref])?;
        }
    }

    // Edge retires: mark superseded, record audit event.
    {
        let mut sup_edge = tx.prepare_cached(
            "UPDATE edges SET superseded_at = unixepoch() \
             WHERE logical_id = ?1 AND superseded_at IS NULL",
        )?;
        let mut ins_event = tx.prepare_cached(
            "INSERT INTO provenance_events (id, event_type, subject, source_ref) \
             VALUES (?1, 'edge_retire', ?2, ?3)",
        )?;
        for retire in &prepared.edge_retires {
            sup_edge.execute(params![retire.logical_id])?;
            ins_event.execute(params![new_id(), retire.logical_id, retire.source_ref])?;
        }
    }

    // Node inserts (with optional upsert + chunk-policy handling).
    {
        let mut del_fts = tx.prepare_cached("DELETE FROM fts_nodes WHERE node_logical_id = ?1")?;
        let mut del_chunks = tx.prepare_cached("DELETE FROM chunks WHERE node_logical_id = ?1")?;
        let mut sup_node = tx.prepare_cached(
            "UPDATE nodes SET superseded_at = unixepoch() \
             WHERE logical_id = ?1 AND superseded_at IS NULL",
        )?;
        let mut ins_node = tx.prepare_cached(
            "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
             VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)",
        )?;
        #[cfg(feature = "sqlite-vec")]
        let vec_del_sql2 = "DELETE FROM vec_nodes_active WHERE chunk_id IN \
                            (SELECT id FROM chunks WHERE node_logical_id = ?1)";
        #[cfg(feature = "sqlite-vec")]
        let mut del_vec = match tx.prepare_cached(vec_del_sql2) {
            Ok(stmt) => Some(stmt),
            Err(ref e) if crate::coordinator::is_vec_table_absent(e) => None,
            Err(e) => return Err(e.into()),
        };
        for node in &prepared.nodes {
            if node.upsert {
                if node.chunk_policy == ChunkPolicy::Replace {
                    #[cfg(feature = "sqlite-vec")]
                    if let Some(ref mut stmt) = del_vec {
                        stmt.execute(params![node.logical_id])?;
                    }
                    del_fts.execute(params![node.logical_id])?;
                    del_chunks.execute(params![node.logical_id])?;
                }
                sup_node.execute(params![node.logical_id])?;
            }
            ins_node.execute(params![
                node.row_id,
                node.logical_id,
                node.kind,
                node.properties,
                node.source_ref,
            ])?;
        }
    }

    // Edge inserts (with optional upsert).
    {
        let mut sup_edge = tx.prepare_cached(
            "UPDATE edges SET superseded_at = unixepoch() \
             WHERE logical_id = ?1 AND superseded_at IS NULL",
        )?;
        let mut ins_edge = tx.prepare_cached(
            "INSERT INTO edges \
             (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at, source_ref) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, unixepoch(), ?7)",
        )?;
        for edge in &prepared.edges {
            if edge.upsert {
                sup_edge.execute(params![edge.logical_id])?;
            }
            ins_edge.execute(params![
                edge.row_id,
                edge.logical_id,
                edge.source_logical_id,
                edge.target_logical_id,
                edge.kind,
                edge.properties,
                edge.source_ref,
            ])?;
        }
    }

    // Chunk inserts.
    {
        let mut ins_chunk = tx.prepare_cached(
            "INSERT INTO chunks (id, node_logical_id, text_content, byte_start, byte_end, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, unixepoch())",
        )?;
        for chunk in &prepared.chunks {
            ins_chunk.execute(params![
                chunk.id,
                chunk.node_logical_id,
                chunk.text_content,
                chunk.byte_start,
                chunk.byte_end,
            ])?;
        }
    }

    // Run inserts (with optional upsert).
    {
        let mut sup_run = tx.prepare_cached(
            "UPDATE runs SET superseded_at = unixepoch() WHERE id = ?1 AND superseded_at IS NULL",
        )?;
        let mut ins_run = tx.prepare_cached(
            "INSERT INTO runs (id, kind, status, properties, created_at, source_ref) \
             VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)",
        )?;
        for run in &prepared.runs {
            if run.upsert
                && let Some(ref prior_id) = run.supersedes_id
            {
                sup_run.execute(params![prior_id])?;
            }
            ins_run.execute(params![
                run.id,
                run.kind,
                run.status,
                run.properties,
                run.source_ref
            ])?;
        }
    }

    // Step inserts (with optional upsert).
    {
        let mut sup_step = tx.prepare_cached(
            "UPDATE steps SET superseded_at = unixepoch() WHERE id = ?1 AND superseded_at IS NULL",
        )?;
        let mut ins_step = tx.prepare_cached(
            "INSERT INTO steps (id, run_id, kind, status, properties, created_at, source_ref) \
             VALUES (?1, ?2, ?3, ?4, ?5, unixepoch(), ?6)",
        )?;
        for step in &prepared.steps {
            if step.upsert
                && let Some(ref prior_id) = step.supersedes_id
            {
                sup_step.execute(params![prior_id])?;
            }
            ins_step.execute(params![
                step.id,
                step.run_id,
                step.kind,
                step.status,
                step.properties,
                step.source_ref,
            ])?;
        }
    }

    // Action inserts (with optional upsert).
    {
        let mut sup_action = tx.prepare_cached(
            "UPDATE actions SET superseded_at = unixepoch() WHERE id = ?1 AND superseded_at IS NULL",
        )?;
        let mut ins_action = tx.prepare_cached(
            "INSERT INTO actions (id, step_id, kind, status, properties, created_at, source_ref) \
             VALUES (?1, ?2, ?3, ?4, ?5, unixepoch(), ?6)",
        )?;
        for action in &prepared.actions {
            if action.upsert
                && let Some(ref prior_id) = action.supersedes_id
            {
                sup_action.execute(params![prior_id])?;
            }
            ins_action.execute(params![
                action.id,
                action.step_id,
                action.kind,
                action.status,
                action.properties,
                action.source_ref,
            ])?;
        }
    }

    // Operational mutation log writes and latest-state current rows.
    {
        ensure_operational_collections_writable(&tx, prepared)?;
        prepared.operational_validation_warnings =
            validate_operational_writes_against_live_contracts(&tx, prepared)?;
        let collection_secondary_indexes = load_live_operational_secondary_indexes(&tx, prepared)?;

        let mut next_mutation_order: i64 = tx.query_row(
            "SELECT COALESCE(MAX(mutation_order), 0) FROM operational_mutations",
            [],
            |row| row.get(0),
        )?;
        let mut ins_mutation = tx.prepare_cached(
            "INSERT INTO operational_mutations \
             (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, unixepoch(), ?7)",
        )?;
        let mut ins_filter_value = tx.prepare_cached(
            "INSERT INTO operational_filter_values \
             (mutation_id, collection_name, field_name, string_value, integer_value) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut upsert_current = tx.prepare_cached(
            "INSERT INTO operational_current \
             (collection_name, record_key, payload_json, updated_at, last_mutation_id) \
             VALUES (?1, ?2, ?3, unixepoch(), ?4) \
             ON CONFLICT(collection_name, record_key) DO UPDATE SET \
                 payload_json = excluded.payload_json, \
                 updated_at = excluded.updated_at, \
                 last_mutation_id = excluded.last_mutation_id",
        )?;
        let mut del_current = tx.prepare_cached(
            "DELETE FROM operational_current WHERE collection_name = ?1 AND record_key = ?2",
        )?;
        let mut del_current_secondary_indexes = tx.prepare_cached(
            "DELETE FROM operational_secondary_index_entries \
             WHERE collection_name = ?1 AND subject_kind = 'current' AND record_key = ?2",
        )?;
        let mut ins_secondary_index = tx.prepare_cached(
            "INSERT INTO operational_secondary_index_entries \
             (collection_name, index_name, subject_kind, mutation_id, record_key, sort_timestamp, \
              slot1_text, slot1_integer, slot2_text, slot2_integer, slot3_text, slot3_integer) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;
        let mut current_row_stmt = tx.prepare_cached(
            "SELECT payload_json, updated_at, last_mutation_id FROM operational_current \
             WHERE collection_name = ?1 AND record_key = ?2",
        )?;

        for write in &prepared.operational_writes {
            let collection = operational_write_collection(write);
            let record_key = operational_write_record_key(write);
            let mutation_id = new_id();
            next_mutation_order += 1;
            let payload_json = operational_write_payload(write);
            ins_mutation.execute(params![
                &mutation_id,
                collection,
                record_key,
                operational_write_kind(write),
                payload_json,
                operational_write_source_ref(write),
                next_mutation_order,
            ])?;
            if let Some(indexes) = collection_secondary_indexes.get(collection) {
                for entry in extract_secondary_index_entries_for_mutation(indexes, payload_json) {
                    ins_secondary_index.execute(params![
                        collection,
                        entry.index_name,
                        "mutation",
                        &mutation_id,
                        record_key,
                        entry.sort_timestamp,
                        entry.slot1_text,
                        entry.slot1_integer,
                        entry.slot2_text,
                        entry.slot2_integer,
                        entry.slot3_text,
                        entry.slot3_integer,
                    ])?;
                }
            }
            if let Some(filter_fields) = prepared
                .operational_collection_filter_fields
                .get(collection)
            {
                for filter_value in extract_operational_filter_values(filter_fields, payload_json) {
                    ins_filter_value.execute(params![
                        &mutation_id,
                        collection,
                        filter_value.field_name,
                        filter_value.string_value,
                        filter_value.integer_value,
                    ])?;
                }
            }

            if prepared.operational_collection_kinds.get(collection)
                == Some(&OperationalCollectionKind::LatestState)
            {
                del_current_secondary_indexes.execute(params![collection, record_key])?;
                match write {
                    OperationalWrite::Put { payload_json, .. } => {
                        upsert_current.execute(params![
                            collection,
                            record_key,
                            payload_json,
                            &mutation_id,
                        ])?;
                        if let Some(indexes) = collection_secondary_indexes.get(collection) {
                            let (current_payload_json, updated_at, last_mutation_id): (
                                String,
                                i64,
                                String,
                            ) = current_row_stmt
                                .query_row(params![collection, record_key], |row| {
                                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                                })?;
                            for entry in extract_secondary_index_entries_for_current(
                                indexes,
                                &current_payload_json,
                                updated_at,
                            ) {
                                ins_secondary_index.execute(params![
                                    collection,
                                    entry.index_name,
                                    "current",
                                    last_mutation_id.as_str(),
                                    record_key,
                                    entry.sort_timestamp,
                                    entry.slot1_text,
                                    entry.slot1_integer,
                                    entry.slot2_text,
                                    entry.slot2_integer,
                                    entry.slot3_text,
                                    entry.slot3_integer,
                                ])?;
                            }
                        }
                    }
                    OperationalWrite::Delete { .. } => {
                        del_current.execute(params![collection, record_key])?;
                    }
                    OperationalWrite::Append { .. } => {}
                }
            }
        }
    }

    // FTS row inserts.
    {
        let mut ins_fts = tx.prepare_cached(
            "INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content) \
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for fts_row in &prepared.required_fts_rows {
            ins_fts.execute(params![
                fts_row.chunk_id,
                fts_row.node_logical_id,
                fts_row.kind,
                fts_row.text_content,
            ])?;
        }
    }

    // Vec inserts (feature-gated; silently skipped when sqlite-vec is absent or table missing).
    #[cfg(feature = "sqlite-vec")]
    {
        match tx
            .prepare_cached("INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES (?1, ?2)")
        {
            Ok(mut ins_vec) => {
                for vi in &prepared.vec_inserts {
                    let bytes: Vec<u8> =
                        vi.embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
                    ins_vec.execute(params![vi.chunk_id, bytes])?;
                }
            }
            Err(ref e) if crate::coordinator::is_vec_table_absent(e) => {
                // vec profile absent: vec inserts are silently skipped.
            }
            Err(e) => return Err(e.into()),
        }
    }

    tx.commit()?;

    let provenance_warnings: Vec<String> = prepared
        .nodes
        .iter()
        .filter(|node| node.source_ref.is_none())
        .map(|node| format!("node '{}' has no source_ref", node.logical_id))
        .chain(
            prepared
                .node_retires
                .iter()
                .filter(|r| r.source_ref.is_none())
                .map(|r| format!("node retire '{}' has no source_ref", r.logical_id)),
        )
        .chain(
            prepared
                .edges
                .iter()
                .filter(|e| e.source_ref.is_none())
                .map(|e| format!("edge '{}' has no source_ref", e.logical_id)),
        )
        .chain(
            prepared
                .edge_retires
                .iter()
                .filter(|r| r.source_ref.is_none())
                .map(|r| format!("edge retire '{}' has no source_ref", r.logical_id)),
        )
        .chain(
            prepared
                .runs
                .iter()
                .filter(|r| r.source_ref.is_none())
                .map(|r| format!("run '{}' has no source_ref", r.id)),
        )
        .chain(
            prepared
                .steps
                .iter()
                .filter(|s| s.source_ref.is_none())
                .map(|s| format!("step '{}' has no source_ref", s.id)),
        )
        .chain(
            prepared
                .actions
                .iter()
                .filter(|a| a.source_ref.is_none())
                .map(|a| format!("action '{}' has no source_ref", a.id)),
        )
        .chain(
            prepared
                .operational_writes
                .iter()
                .filter(|write| operational_write_source_ref(write).is_none())
                .map(|write| {
                    format!(
                        "operational {} '{}:{}' has no source_ref",
                        operational_write_kind(write),
                        operational_write_collection(write),
                        operational_write_record_key(write)
                    )
                }),
        )
        .collect();

    let mut warnings = provenance_warnings.clone();
    warnings.extend(prepared.operational_validation_warnings.clone());

    Ok(WriteReceipt {
        label: prepared.label.clone(),
        optional_backfill_count: prepared.optional_backfills.len(),
        warnings,
        provenance_warnings,
    })
}

fn operational_write_collection(write: &OperationalWrite) -> &str {
    match write {
        OperationalWrite::Append { collection, .. }
        | OperationalWrite::Put { collection, .. }
        | OperationalWrite::Delete { collection, .. } => collection,
    }
}

fn operational_write_record_key(write: &OperationalWrite) -> &str {
    match write {
        OperationalWrite::Append { record_key, .. }
        | OperationalWrite::Put { record_key, .. }
        | OperationalWrite::Delete { record_key, .. } => record_key,
    }
}

fn operational_write_kind(write: &OperationalWrite) -> &'static str {
    match write {
        OperationalWrite::Append { .. } => "append",
        OperationalWrite::Put { .. } => "put",
        OperationalWrite::Delete { .. } => "delete",
    }
}

fn operational_write_payload(write: &OperationalWrite) -> &str {
    match write {
        OperationalWrite::Append { payload_json, .. }
        | OperationalWrite::Put { payload_json, .. } => payload_json,
        OperationalWrite::Delete { .. } => "null",
    }
}

fn operational_write_source_ref(write: &OperationalWrite) -> Option<&str> {
    match write {
        OperationalWrite::Append { source_ref, .. }
        | OperationalWrite::Put { source_ref, .. }
        | OperationalWrite::Delete { source_ref, .. } => source_ref.as_deref(),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use fathomdb_schema::SchemaManager;
    use tempfile::NamedTempFile;

    use super::{apply_write, prepare_write, resolve_operational_writes};
    use crate::{
        ActionInsert, ChunkInsert, ChunkPolicy, EdgeInsert, EdgeRetire, EngineError, NodeInsert,
        NodeRetire, OperationalWrite, OptionalProjectionTask, ProvenanceMode, RunInsert,
        StepInsert, VecInsert, WriteRequest, WriterActor, projection::ProjectionTarget,
    };

    #[test]
    fn writer_executes_runtime_table_rows() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "runtime".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-1".to_owned(),
                    kind: "session".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![StepInsert {
                    id: "step-1".to_owned(),
                    run_id: "run-1".to_owned(),
                    kind: "llm".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                actions: vec![ActionInsert {
                    id: "action-1".to_owned(),
                    step_id: "step-1".to_owned(),
                    kind: "emit".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write receipt");

        assert_eq!(receipt.label, "runtime");
    }

    #[test]
    fn writer_put_operational_write_updates_current_and_mutations() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json) \
             VALUES ('connector_health', 'latest_state', '{}', '{}')",
            [],
        )
        .expect("seed collection");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "node-and-operational".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "lg-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Put {
                    collection: "connector_health".to_owned(),
                    record_key: "gmail".to_owned(),
                    payload_json: r#"{"status":"ok"}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
            })
            .expect("write receipt");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let node_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM nodes WHERE logical_id = 'lg-1'",
                [],
                |row| row.get(0),
            )
            .expect("node count");
        assert_eq!(node_count, 1);
        let mutation_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_mutations WHERE collection_name = 'connector_health' \
                 AND record_key = 'gmail'",
                [],
                |row| row.get(0),
            )
            .expect("mutation count");
        assert_eq!(mutation_count, 1);
        let payload: String = conn
            .query_row(
                "SELECT payload_json FROM operational_current \
                 WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
                |row| row.get(0),
            )
            .expect("current payload");
        assert_eq!(payload, r#"{"status":"ok"}"#);
    }

    #[test]
    fn writer_disabled_validation_mode_allows_invalid_operational_payloads() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json, validation_json) \
             VALUES ('connector_health', 'latest_state', '{}', '{}', ?1)",
            [r#"{"format_version":1,"mode":"disabled","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}"#],
        )
        .expect("seed collection");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "disabled-validation".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Put {
                    collection: "connector_health".to_owned(),
                    record_key: "gmail".to_owned(),
                    payload_json: r#"{"bogus":true}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
            })
            .expect("write receipt");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let payload: String = conn
            .query_row(
                "SELECT payload_json FROM operational_current \
                 WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
                |row| row.get(0),
            )
            .expect("current payload");
        assert_eq!(payload, r#"{"bogus":true}"#);
    }

    #[test]
    fn writer_report_only_validation_allows_invalid_payload_and_emits_warning() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json, validation_json) \
             VALUES ('connector_health', 'latest_state', '{}', '{}', ?1)",
            [r#"{"format_version":1,"mode":"report_only","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}"#],
        )
        .expect("seed collection");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "report-only-validation".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Put {
                    collection: "connector_health".to_owned(),
                    record_key: "gmail".to_owned(),
                    payload_json: r#"{"status":"bogus"}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
            })
            .expect("report_only write should succeed");

        assert_eq!(receipt.provenance_warnings, Vec::<String>::new());
        assert_eq!(receipt.warnings.len(), 1);
        assert!(
            receipt.warnings[0].contains("connector_health"),
            "warning should identify collection"
        );
        assert!(
            receipt.warnings[0].contains("must be one of"),
            "warning should explain validation failure"
        );

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let payload: String = conn
            .query_row(
                "SELECT payload_json FROM operational_current \
                 WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
                |row| row.get(0),
            )
            .expect("current payload");
        assert_eq!(payload, r#"{"status":"bogus"}"#);
    }

    #[test]
    fn writer_rejects_operational_write_for_missing_collection() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "missing-operational-collection".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![OperationalWrite::Put {
                collection: "connector_health".to_owned(),
                record_key: "gmail".to_owned(),
                payload_json: r#"{"status":"ok"}"#.to_owned(),
                source_ref: Some("src-1".to_owned()),
            }],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "missing operational collection must return InvalidWrite"
        );
    }

    #[test]
    fn writer_append_operational_write_records_history_without_current_row() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json) \
             VALUES ('audit_log', 'append_only_log', '{}', '{}')",
            [],
        )
        .expect("seed collection");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "append-operational".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Append {
                    collection: "audit_log".to_owned(),
                    record_key: "evt-1".to_owned(),
                    payload_json: r#"{"type":"sync"}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
            })
            .expect("write receipt");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let mutation: (String, String) = conn
            .query_row(
                "SELECT op_kind, payload_json FROM operational_mutations \
                 WHERE collection_name = 'audit_log' AND record_key = 'evt-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("mutation row");
        assert_eq!(mutation.0, "append");
        assert_eq!(mutation.1, r#"{"type":"sync"}"#);
        let current_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_current \
                 WHERE collection_name = 'audit_log' AND record_key = 'evt-1'",
                [],
                |row| row.get(0),
            )
            .expect("current count");
        assert_eq!(current_count, 0);
    }

    #[test]
    fn writer_enforce_validation_rejects_invalid_append_without_side_effects() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections \
             (name, kind, schema_json, retention_json, filter_fields_json, validation_json) \
             VALUES ('audit_log', 'append_only_log', '{}', '{}', \
                     '[{\"name\":\"status\",\"type\":\"string\",\"modes\":[\"exact\"]}]', ?1)",
            [r#"{"format_version":1,"mode":"enforce","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}"#],
        )
        .expect("seed collection");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let error = writer
            .submit(WriteRequest {
                label: "invalid-append".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Append {
                    collection: "audit_log".to_owned(),
                    record_key: "evt-1".to_owned(),
                    payload_json: r#"{"status":"bogus"}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
            })
            .expect_err("invalid append must reject");
        assert!(matches!(error, EngineError::InvalidWrite(_)));
        assert!(error.to_string().contains("must be one of"));

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let mutation_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_mutations WHERE collection_name = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("mutation count");
        assert_eq!(mutation_count, 0);
        let filter_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_filter_values WHERE collection_name = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("filter count");
        assert_eq!(filter_count, 0);
    }

    #[test]
    fn writer_delete_operational_write_removes_current_row_and_keeps_history() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json) \
             VALUES ('connector_health', 'latest_state', '{}', '{}')",
            [],
        )
        .expect("seed collection");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "put-operational".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Put {
                    collection: "connector_health".to_owned(),
                    record_key: "gmail".to_owned(),
                    payload_json: r#"{"status":"ok"}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
            })
            .expect("put receipt");

        writer
            .submit(WriteRequest {
                label: "delete-operational".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Delete {
                    collection: "connector_health".to_owned(),
                    record_key: "gmail".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                }],
            })
            .expect("delete receipt");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let mutation_kinds: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT op_kind FROM operational_mutations \
                     WHERE collection_name = 'connector_health' AND record_key = 'gmail' \
                     ORDER BY mutation_order ASC",
                )
                .expect("stmt");
            stmt.query_map([], |row| row.get(0))
                .expect("rows")
                .collect::<Result<_, _>>()
                .expect("collect")
        };
        assert_eq!(mutation_kinds, vec!["put".to_owned(), "delete".to_owned()]);
        let current_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_current \
                 WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
                |row| row.get(0),
            )
            .expect("current count");
        assert_eq!(current_count, 0);
    }

    #[test]
    fn writer_delete_bypasses_validation_contract() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json, validation_json) \
             VALUES ('connector_health', 'latest_state', '{}', '{}', ?1)",
            [r#"{"format_version":1,"mode":"enforce","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}"#],
        )
        .expect("seed collection");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "valid-put".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Put {
                    collection: "connector_health".to_owned(),
                    record_key: "gmail".to_owned(),
                    payload_json: r#"{"status":"ok"}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
            })
            .expect("put receipt");
        writer
            .submit(WriteRequest {
                label: "delete-after-put".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Delete {
                    collection: "connector_health".to_owned(),
                    record_key: "gmail".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                }],
            })
            .expect("delete receipt");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let current_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_current \
                 WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
                |row| row.get(0),
            )
            .expect("current count");
        assert_eq!(current_count, 0);
    }

    #[test]
    fn writer_latest_state_secondary_indexes_track_put_and_delete() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections \
             (name, kind, schema_json, retention_json, secondary_indexes_json) \
             VALUES ('connector_health', 'latest_state', '{}', '{}', ?1)",
            [r#"[{"name":"status_current","kind":"latest_state_field","field":"status","value_type":"string"},{"name":"tenant_category","kind":"latest_state_composite","fields":[{"name":"tenant","value_type":"string"},{"name":"category","value_type":"string"}]}]"#],
        )
        .expect("seed collection");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "secondary-index-put".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Put {
                    collection: "connector_health".to_owned(),
                    record_key: "gmail".to_owned(),
                    payload_json: r#"{"status":"degraded","tenant":"acme","category":"mail"}"#
                        .to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
            })
            .expect("put receipt");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let current_entry_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_secondary_index_entries \
                 WHERE collection_name = 'connector_health' AND subject_kind = 'current'",
                [],
                |row| row.get(0),
            )
            .expect("current secondary index count");
        assert_eq!(current_entry_count, 2);
        drop(conn);

        writer
            .submit(WriteRequest {
                label: "secondary-index-delete".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Delete {
                    collection: "connector_health".to_owned(),
                    record_key: "gmail".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                }],
            })
            .expect("delete receipt");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let current_entry_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_secondary_index_entries \
                 WHERE collection_name = 'connector_health' AND subject_kind = 'current'",
                [],
                |row| row.get(0),
            )
            .expect("current secondary index count");
        assert_eq!(current_entry_count, 0);
    }

    #[test]
    fn writer_latest_state_operational_writes_persist_mutation_order() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json) \
             VALUES ('connector_health', 'latest_state', '{}', '{}')",
            [],
        )
        .expect("seed collection");
        drop(conn);

        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "ordered-operational-batch".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![
                    OperationalWrite::Put {
                        collection: "connector_health".to_owned(),
                        record_key: "gmail".to_owned(),
                        payload_json: r#"{"status":"old"}"#.to_owned(),
                        source_ref: Some("src-1".to_owned()),
                    },
                    OperationalWrite::Delete {
                        collection: "connector_health".to_owned(),
                        record_key: "gmail".to_owned(),
                        source_ref: Some("src-2".to_owned()),
                    },
                    OperationalWrite::Put {
                        collection: "connector_health".to_owned(),
                        record_key: "gmail".to_owned(),
                        payload_json: r#"{"status":"new"}"#.to_owned(),
                        source_ref: Some("src-3".to_owned()),
                    },
                ],
            })
            .expect("write receipt");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let rows: Vec<(String, i64)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT op_kind, mutation_order FROM operational_mutations \
                     WHERE collection_name = 'connector_health' AND record_key = 'gmail' \
                     ORDER BY mutation_order ASC",
                )
                .expect("stmt");
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .expect("rows")
                .collect::<Result<_, _>>()
                .expect("collect")
        };
        assert_eq!(
            rows,
            vec![
                ("put".to_owned(), 1),
                ("delete".to_owned(), 2),
                ("put".to_owned(), 3),
            ]
        );
        let payload: String = conn
            .query_row(
                "SELECT payload_json FROM operational_current \
                 WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
                |row| row.get(0),
            )
            .expect("current payload");
        assert_eq!(payload, r#"{"status":"new"}"#);
    }

    #[test]
    fn apply_write_rechecks_collection_disabled_state_inside_transaction() {
        let db = NamedTempFile::new().expect("temporary db");
        let mut conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json) \
             VALUES ('connector_health', 'latest_state', '{}', '{}')",
            [],
        )
        .expect("seed collection");

        let request = WriteRequest {
            label: "disabled-race".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![OperationalWrite::Put {
                collection: "connector_health".to_owned(),
                record_key: "gmail".to_owned(),
                payload_json: r#"{"status":"ok"}"#.to_owned(),
                source_ref: Some("src-1".to_owned()),
            }],
        };
        let mut prepared = prepare_write(request, ProvenanceMode::Warn).expect("prepare");
        resolve_operational_writes(&conn, &mut prepared).expect("preflight resolve");

        conn.execute(
            "UPDATE operational_collections SET disabled_at = 123 WHERE name = 'connector_health'",
            [],
        )
        .expect("disable collection after preflight");

        let error =
            apply_write(&mut conn, &mut prepared).expect_err("disabled collection must reject");
        assert!(matches!(error, EngineError::InvalidWrite(_)));
        assert!(error.to_string().contains("is disabled"));

        let mutation_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_mutations WHERE collection_name = 'connector_health'",
                [],
                |row| row.get(0),
            )
            .expect("mutation count");
        assert_eq!(mutation_count, 0);

        let current_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_current WHERE collection_name = 'connector_health'",
                [],
                |row| row.get(0),
            )
            .expect("current count");
        assert_eq!(current_count, 0);
    }

    #[test]
    fn writer_enforce_validation_rejects_invalid_put_atomically() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json, validation_json) \
             VALUES ('connector_health', 'latest_state', '{}', '{}', ?1)",
            [r#"{"format_version":1,"mode":"enforce","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}"#],
        )
        .expect("seed collection");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let error = writer
            .submit(WriteRequest {
                label: "invalid-put".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "lg-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![OperationalWrite::Put {
                    collection: "connector_health".to_owned(),
                    record_key: "gmail".to_owned(),
                    payload_json: r#"{"status":"bogus"}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
            })
            .expect_err("invalid put must reject");
        assert!(matches!(error, EngineError::InvalidWrite(_)));
        assert!(error.to_string().contains("must be one of"));

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let node_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM nodes WHERE logical_id = 'lg-1'",
                [],
                |row| row.get(0),
            )
            .expect("node count");
        assert_eq!(node_count, 0);
        let mutation_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_mutations WHERE collection_name = 'connector_health'",
                [],
                |row| row.get(0),
            )
            .expect("mutation count");
        assert_eq!(mutation_count, 0);
        let current_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_current WHERE collection_name = 'connector_health'",
                [],
                |row| row.get(0),
            )
            .expect("current count");
        assert_eq!(current_count, 0);
    }

    #[test]
    fn writer_rejects_append_against_latest_state_collection() {
        let db = NamedTempFile::new().expect("temporary db");
        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        SchemaManager::new().bootstrap(&conn).expect("bootstrap");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json) \
             VALUES ('connector_health', 'latest_state', '{}', '{}')",
            [],
        )
        .expect("seed collection");
        drop(conn);
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "bad-append".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![OperationalWrite::Append {
                collection: "connector_health".to_owned(),
                record_key: "gmail".to_owned(),
                payload_json: r#"{"status":"ok"}"#.to_owned(),
                source_ref: Some("src-1".to_owned()),
            }],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "latest_state collection must reject Append"
        );
    }

    #[test]
    fn writer_upsert_supersedes_prior_active_node() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "v1".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "lg-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: r#"{"version":1}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v1 write");

        writer
            .submit(WriteRequest {
                label: "v2".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-2".to_owned(),
                    logical_id: "lg-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: r#"{"version":2}"#.to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: true,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v2 upsert write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let (active_row_id, props): (String, String) = conn
            .query_row(
                "SELECT row_id, properties FROM nodes WHERE logical_id = 'lg-1' AND superseded_at IS NULL",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("active row");
        assert_eq!(active_row_id, "row-2");
        assert!(props.contains("\"version\":2"));

        let superseded: i64 = conn
            .query_row(
                "SELECT count(*) FROM nodes WHERE row_id = 'row-1' AND superseded_at IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("superseded count");
        assert_eq!(superseded, 1);
    }

    #[test]
    fn writer_inserts_edge_between_two_nodes() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "nodes-and-edge".to_owned(),
                nodes: vec![
                    NodeInsert {
                        row_id: "row-meeting".to_owned(),
                        logical_id: "meeting-1".to_owned(),
                        kind: "Meeting".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some("src-1".to_owned()),
                        upsert: false,
                        chunk_policy: ChunkPolicy::Preserve,
                    },
                    NodeInsert {
                        row_id: "row-task".to_owned(),
                        logical_id: "task-1".to_owned(),
                        kind: "Task".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some("src-1".to_owned()),
                        upsert: false,
                        chunk_policy: ChunkPolicy::Preserve,
                    },
                ],
                node_retires: vec![],
                edges: vec![EdgeInsert {
                    row_id: "edge-1".to_owned(),
                    logical_id: "edge-lg-1".to_owned(),
                    source_logical_id: "meeting-1".to_owned(),
                    target_logical_id: "task-1".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                }],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write receipt");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let (src, tgt, kind): (String, String, String) = conn
            .query_row(
                "SELECT source_logical_id, target_logical_id, kind FROM edges WHERE row_id = 'edge-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("edge row");
        assert_eq!(src, "meeting-1");
        assert_eq!(tgt, "task-1");
        assert_eq!(kind, "HAS_TASK");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn writer_upsert_supersedes_prior_active_edge() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        // Write two nodes
        writer
            .submit(WriteRequest {
                label: "nodes".to_owned(),
                nodes: vec![
                    NodeInsert {
                        row_id: "row-a".to_owned(),
                        logical_id: "node-a".to_owned(),
                        kind: "Meeting".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some("src-1".to_owned()),
                        upsert: false,
                        chunk_policy: ChunkPolicy::Preserve,
                    },
                    NodeInsert {
                        row_id: "row-b".to_owned(),
                        logical_id: "node-b".to_owned(),
                        kind: "Task".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some("src-1".to_owned()),
                        upsert: false,
                        chunk_policy: ChunkPolicy::Preserve,
                    },
                ],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("nodes write");

        // Write v1 edge
        writer
            .submit(WriteRequest {
                label: "edge-v1".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![EdgeInsert {
                    row_id: "edge-row-1".to_owned(),
                    logical_id: "edge-lg-1".to_owned(),
                    source_logical_id: "node-a".to_owned(),
                    target_logical_id: "node-b".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: r#"{"weight":1}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                }],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("edge v1 write");

        // Upsert v2 edge
        writer
            .submit(WriteRequest {
                label: "edge-v2".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![EdgeInsert {
                    row_id: "edge-row-2".to_owned(),
                    logical_id: "edge-lg-1".to_owned(),
                    source_logical_id: "node-a".to_owned(),
                    target_logical_id: "node-b".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: r#"{"weight":2}"#.to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: true,
                }],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("edge v2 upsert");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let (active_row_id, props): (String, String) = conn
            .query_row(
                "SELECT row_id, properties FROM edges WHERE logical_id = 'edge-lg-1' AND superseded_at IS NULL",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("active edge");
        assert_eq!(active_row_id, "edge-row-2");
        assert!(props.contains("\"weight\":2"));

        let superseded: i64 = conn
            .query_row(
                "SELECT count(*) FROM edges WHERE row_id = 'edge-row-1' AND superseded_at IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("superseded count");
        assert_eq!(superseded, 1);
    }

    #[test]
    fn writer_fts_rows_are_written_to_database() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "seed".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "logical-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-1".to_owned(),
                    node_logical_id: "logical-1".to_owned(),
                    text_content: "budget discussion".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write receipt");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let (chunk_id, node_logical_id, kind, text_content): (String, String, String, String) =
            conn.query_row(
                "SELECT chunk_id, node_logical_id, kind, text_content \
                 FROM fts_nodes WHERE chunk_id = 'chunk-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("fts row");
        assert_eq!(chunk_id, "chunk-1");
        assert_eq!(node_logical_id, "logical-1");
        assert_eq!(kind, "Meeting");
        assert_eq!(text_content, "budget discussion");
    }

    #[test]
    fn writer_receipt_warns_on_nodes_without_source_ref() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "no-source".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "logical-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write receipt");

        assert_eq!(receipt.provenance_warnings.len(), 1);
        assert!(receipt.provenance_warnings[0].contains("logical-1"));
    }

    #[test]
    fn writer_receipt_no_warnings_when_all_nodes_have_source_ref() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "with-source".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "logical-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write receipt");

        assert!(receipt.provenance_warnings.is_empty());
    }

    #[test]
    fn writer_accepts_chunk_for_pre_existing_node() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        // Request 1: submit node only
        writer
            .submit(WriteRequest {
                label: "r1".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "logical-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("r1 write");

        // Request 2: submit chunk for pre-existing node
        writer
            .submit(WriteRequest {
                label: "r2".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-1".to_owned(),
                    node_logical_id: "logical-1".to_owned(),
                    text_content: "budget discussion".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("r2 write — chunk for pre-existing node");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM fts_nodes WHERE chunk_id = 'chunk-1'",
                [],
                |row| row.get(0),
            )
            .expect("fts count");
        assert_eq!(
            count, 1,
            "FTS row must exist for chunk attached to pre-existing node"
        );
    }

    #[test]
    fn writer_rejects_chunk_for_completely_unknown_node() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "bad".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "chunk-1".to_owned(),
                node_logical_id: "nonexistent".to_owned(),
                text_content: "some text".to_owned(),
                byte_start: None,
                byte_end: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "completely unknown node must return InvalidWrite"
        );
    }

    #[test]
    fn writer_executes_typed_nodes_chunks_and_derived_projections() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "seed".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "logical-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-1".to_owned(),
                    node_logical_id: "logical-1".to_owned(),
                    text_content: "budget discussion".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write receipt");

        assert_eq!(receipt.label, "seed");
    }

    #[test]
    fn writer_node_retire_supersedes_active_node() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "seed".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("seed write");

        writer
            .submit(WriteRequest {
                label: "retire".to_owned(),
                nodes: vec![],
                node_retires: vec![NodeRetire {
                    logical_id: "meeting-1".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                }],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("retire write");

        let conn = rusqlite::Connection::open(db.path()).expect("open");
        let active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE logical_id = 'meeting-1' AND superseded_at IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("count active");
        let historical: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE logical_id = 'meeting-1' AND superseded_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .expect("count historical");

        assert_eq!(active, 0, "active count must be 0 after retire");
        assert_eq!(historical, 1, "historical count must be 1 after retire");
    }

    #[test]
    fn writer_node_retire_preserves_chunks_and_clears_fts() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "seed".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-1".to_owned(),
                    node_logical_id: "meeting-1".to_owned(),
                    text_content: "budget discussion".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("seed write");

        writer
            .submit(WriteRequest {
                label: "retire".to_owned(),
                nodes: vec![],
                node_retires: vec![NodeRetire {
                    logical_id: "meeting-1".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                }],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("retire write");

        let conn = rusqlite::Connection::open(db.path()).expect("open");
        let chunk_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE node_logical_id = 'meeting-1'",
                [],
                |r| r.get(0),
            )
            .expect("chunk count");
        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE node_logical_id = 'meeting-1'",
                [],
                |r| r.get(0),
            )
            .expect("fts count");

        assert_eq!(
            chunk_count, 1,
            "chunks must remain after node retire so restore can re-establish content"
        );
        assert_eq!(fts_count, 0, "fts_nodes must be deleted after node retire");
    }

    #[test]
    fn writer_edge_retire_supersedes_active_edge() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "seed".to_owned(),
                nodes: vec![
                    NodeInsert {
                        row_id: "row-a".to_owned(),
                        logical_id: "node-a".to_owned(),
                        kind: "Meeting".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some("src-1".to_owned()),
                        upsert: false,
                        chunk_policy: ChunkPolicy::Preserve,
                    },
                    NodeInsert {
                        row_id: "row-b".to_owned(),
                        logical_id: "node-b".to_owned(),
                        kind: "Task".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some("src-1".to_owned()),
                        upsert: false,
                        chunk_policy: ChunkPolicy::Preserve,
                    },
                ],
                node_retires: vec![],
                edges: vec![EdgeInsert {
                    row_id: "edge-1".to_owned(),
                    logical_id: "edge-lg-1".to_owned(),
                    source_logical_id: "node-a".to_owned(),
                    target_logical_id: "node-b".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                }],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("seed write");

        writer
            .submit(WriteRequest {
                label: "retire-edge".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![EdgeRetire {
                    logical_id: "edge-lg-1".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                }],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("retire edge write");

        let conn = rusqlite::Connection::open(db.path()).expect("open");
        let active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE logical_id = 'edge-lg-1' AND superseded_at IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("active edge count");

        assert_eq!(active, 0, "active edge count must be 0 after retire");
    }

    #[test]
    fn writer_retire_without_source_ref_emits_provenance_warning() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "seed".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("seed write");

        let receipt = writer
            .submit(WriteRequest {
                label: "retire-no-src".to_owned(),
                nodes: vec![],
                node_retires: vec![NodeRetire {
                    logical_id: "meeting-1".to_owned(),
                    source_ref: None,
                }],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("retire write");

        assert!(
            !receipt.provenance_warnings.is_empty(),
            "retire without source_ref must emit a provenance warning"
        );
    }

    #[test]
    fn writer_upsert_with_chunk_policy_replace_clears_old_chunks() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "v1".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-old".to_owned(),
                    node_logical_id: "meeting-1".to_owned(),
                    text_content: "old text".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v1 write");

        writer
            .submit(WriteRequest {
                label: "v2".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-2".to_owned(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: true,
                    chunk_policy: ChunkPolicy::Replace,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-new".to_owned(),
                    node_logical_id: "meeting-1".to_owned(),
                    text_content: "new text".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v2 write");

        let conn = rusqlite::Connection::open(db.path()).expect("open");
        let old_chunk: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE id = 'chunk-old'",
                [],
                |r| r.get(0),
            )
            .expect("old chunk count");
        let new_chunk: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE id = 'chunk-new'",
                [],
                |r| r.get(0),
            )
            .expect("new chunk count");
        let fts_old: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE node_logical_id = 'meeting-1' AND text_content = 'old text'",
                [],
                |r| r.get(0),
            )
            .expect("old fts count");

        assert_eq!(
            old_chunk, 0,
            "old chunk must be deleted by ChunkPolicy::Replace"
        );
        assert_eq!(new_chunk, 1, "new chunk must exist after replace");
        assert_eq!(
            fts_old, 0,
            "old FTS row must be deleted by ChunkPolicy::Replace"
        );
    }

    #[test]
    fn writer_upsert_with_chunk_policy_preserve_keeps_old_chunks() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "v1".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-old".to_owned(),
                    node_logical_id: "meeting-1".to_owned(),
                    text_content: "old text".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v1 write");

        writer
            .submit(WriteRequest {
                label: "v2-props-only".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-2".to_owned(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: r#"{"status":"updated"}"#.to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: true,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v2 preserve write");

        let conn = rusqlite::Connection::open(db.path()).expect("open");
        let old_chunk: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE id = 'chunk-old'",
                [],
                |r| r.get(0),
            )
            .expect("old chunk count");

        assert_eq!(
            old_chunk, 1,
            "old chunk must be preserved by ChunkPolicy::Preserve"
        );
    }

    #[test]
    fn writer_chunk_policy_replace_without_upsert_is_a_no_op() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "v1".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-existing".to_owned(),
                    node_logical_id: "meeting-1".to_owned(),
                    text_content: "existing text".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v1 write");

        // Insert a second node (not upsert) with ChunkPolicy::Replace — should NOT delete prior chunks
        writer
            .submit(WriteRequest {
                label: "insert-no-upsert".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-2".to_owned(),
                    logical_id: "meeting-2".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Replace,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("insert no-upsert write");

        let conn = rusqlite::Connection::open(db.path()).expect("open");
        let existing_chunk: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE id = 'chunk-existing'",
                [],
                |r| r.get(0),
            )
            .expect("chunk count");

        assert_eq!(
            existing_chunk, 1,
            "ChunkPolicy::Replace without upsert must not delete existing chunks"
        );
    }

    #[test]
    fn writer_run_upsert_supersedes_prior_active_run() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "v1".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-v1".to_owned(),
                    kind: "session".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v1 run write");

        writer
            .submit(WriteRequest {
                label: "v2".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-v2".to_owned(),
                    kind: "session".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: true,
                    supersedes_id: Some("run-v1".to_owned()),
                }],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v2 run write");

        let conn = rusqlite::Connection::open(db.path()).expect("open");
        let v1_historical: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM runs WHERE id = 'run-v1' AND superseded_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .expect("v1 historical count");
        let v2_active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM runs WHERE id = 'run-v2' AND superseded_at IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("v2 active count");

        assert_eq!(v1_historical, 1, "run-v1 must be historical after upsert");
        assert_eq!(v2_active, 1, "run-v2 must be active after upsert");
    }

    #[test]
    fn writer_step_upsert_supersedes_prior_active_step() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "v1".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-1".to_owned(),
                    kind: "session".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![StepInsert {
                    id: "step-v1".to_owned(),
                    run_id: "run-1".to_owned(),
                    kind: "llm".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v1 step write");

        writer
            .submit(WriteRequest {
                label: "v2".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![StepInsert {
                    id: "step-v2".to_owned(),
                    run_id: "run-1".to_owned(),
                    kind: "llm".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: true,
                    supersedes_id: Some("step-v1".to_owned()),
                }],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v2 step write");

        let conn = rusqlite::Connection::open(db.path()).expect("open");
        let v1_historical: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM steps WHERE id = 'step-v1' AND superseded_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .expect("v1 historical count");
        let v2_active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM steps WHERE id = 'step-v2' AND superseded_at IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("v2 active count");

        assert_eq!(v1_historical, 1, "step-v1 must be historical after upsert");
        assert_eq!(v2_active, 1, "step-v2 must be active after upsert");
    }

    #[test]
    fn writer_action_upsert_supersedes_prior_active_action() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "v1".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-1".to_owned(),
                    kind: "session".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![StepInsert {
                    id: "step-1".to_owned(),
                    run_id: "run-1".to_owned(),
                    kind: "llm".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                actions: vec![ActionInsert {
                    id: "action-v1".to_owned(),
                    step_id: "step-1".to_owned(),
                    kind: "emit".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v1 action write");

        writer
            .submit(WriteRequest {
                label: "v2".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![ActionInsert {
                    id: "action-v2".to_owned(),
                    step_id: "step-1".to_owned(),
                    kind: "emit".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: true,
                    supersedes_id: Some("action-v1".to_owned()),
                }],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v2 action write");

        let conn = rusqlite::Connection::open(db.path()).expect("open");
        let v1_historical: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM actions WHERE id = 'action-v1' AND superseded_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .expect("v1 historical count");
        let v2_active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM actions WHERE id = 'action-v2' AND superseded_at IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("v2 active count");

        assert_eq!(
            v1_historical, 1,
            "action-v1 must be historical after upsert"
        );
        assert_eq!(v2_active, 1, "action-v2 must be active after upsert");
    }

    // P0: runtime upsert without supersedes_id must be rejected

    #[test]
    fn writer_run_upsert_without_supersedes_id_returns_invalid_write() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "bad".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![RunInsert {
                id: "run-1".to_owned(),
                kind: "session".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: true,
                supersedes_id: None,
            }],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "run upsert=true without supersedes_id must return InvalidWrite"
        );
    }

    #[test]
    fn writer_step_upsert_without_supersedes_id_returns_invalid_write() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "bad".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![StepInsert {
                id: "step-1".to_owned(),
                run_id: "run-1".to_owned(),
                kind: "llm".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: true,
                supersedes_id: None,
            }],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "step upsert=true without supersedes_id must return InvalidWrite"
        );
    }

    #[test]
    fn writer_action_upsert_without_supersedes_id_returns_invalid_write() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "bad".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![ActionInsert {
                id: "action-1".to_owned(),
                step_id: "step-1".to_owned(),
                kind: "emit".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: true,
                supersedes_id: None,
            }],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "action upsert=true without supersedes_id must return InvalidWrite"
        );
    }

    // P1a/b: provenance warnings for edge inserts and runtime table inserts

    #[test]
    fn writer_edge_insert_without_source_ref_emits_provenance_warning() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![
                    NodeInsert {
                        row_id: "row-a".to_owned(),
                        logical_id: "node-a".to_owned(),
                        kind: "Meeting".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some("src-1".to_owned()),
                        upsert: false,
                        chunk_policy: ChunkPolicy::Preserve,
                    },
                    NodeInsert {
                        row_id: "row-b".to_owned(),
                        logical_id: "node-b".to_owned(),
                        kind: "Task".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some("src-1".to_owned()),
                        upsert: false,
                        chunk_policy: ChunkPolicy::Preserve,
                    },
                ],
                node_retires: vec![],
                edges: vec![EdgeInsert {
                    row_id: "edge-1".to_owned(),
                    logical_id: "edge-lg-1".to_owned(),
                    source_logical_id: "node-a".to_owned(),
                    target_logical_id: "node-b".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                }],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        assert!(
            !receipt.provenance_warnings.is_empty(),
            "edge insert without source_ref must emit a provenance warning"
        );
    }

    #[test]
    fn writer_run_insert_without_source_ref_emits_provenance_warning() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-1".to_owned(),
                    kind: "session".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        assert!(
            !receipt.provenance_warnings.is_empty(),
            "run insert without source_ref must emit a provenance warning"
        );
    }

    // P1c: retire a node AND submit chunks for the same logical_id in one request

    #[test]
    fn writer_retire_node_with_chunk_in_same_request_returns_invalid_write() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        // First seed the node so it exists
        writer
            .submit(WriteRequest {
                label: "seed".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("seed write");

        // Now try to retire it AND add a chunk for it in the same request
        let result = writer.submit(WriteRequest {
            label: "bad".to_owned(),
            nodes: vec![],
            node_retires: vec![NodeRetire {
                logical_id: "meeting-1".to_owned(),
                source_ref: Some("src-2".to_owned()),
            }],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "chunk-bad".to_owned(),
                node_logical_id: "meeting-1".to_owned(),
                text_content: "some text".to_owned(),
                byte_start: None,
                byte_end: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "retiring a node AND adding chunks for it in the same request must return InvalidWrite"
        );
    }

    // --- Item 1: prepare_cached batch insert ---

    #[test]
    fn writer_batch_insert_multiple_nodes() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let nodes: Vec<NodeInsert> = (0..100)
            .map(|i| NodeInsert {
                row_id: format!("row-{i}"),
                logical_id: format!("lg-{i}"),
                kind: "Note".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("batch-src".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            })
            .collect();

        writer
            .submit(WriteRequest {
                label: "batch".to_owned(),
                nodes,
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("batch write");

        let conn = rusqlite::Connection::open(db.path()).expect("open");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
            .expect("count nodes");
        assert_eq!(
            count, 100,
            "all 100 nodes must be present after batch insert"
        );
    }

    // --- Item 2: ID validation ---

    #[test]
    fn prepare_write_rejects_empty_node_row_id() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "test".to_owned(),
            nodes: vec![NodeInsert {
                row_id: String::new(),
                logical_id: "lg-1".to_owned(),
                kind: "Note".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "empty row_id must be rejected"
        );
    }

    #[test]
    fn prepare_write_rejects_empty_node_logical_id() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "test".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-1".to_owned(),
                logical_id: String::new(),
                kind: "Note".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "empty logical_id must be rejected"
        );
    }

    #[test]
    fn prepare_write_rejects_duplicate_row_ids_in_request() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "test".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "lg-1".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
                NodeInsert {
                    row_id: "row-1".to_owned(), // duplicate
                    logical_id: "lg-2".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
            ],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "duplicate row_id within request must be rejected"
        );
    }

    #[test]
    fn prepare_write_rejects_empty_chunk_id() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "test".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-1".to_owned(),
                logical_id: "lg-1".to_owned(),
                kind: "Note".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: String::new(),
                node_logical_id: "lg-1".to_owned(),
                text_content: "some text".to_owned(),
                byte_start: None,
                byte_end: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "empty chunk id must be rejected"
        );
    }

    // --- Item 4: provenance warning coverage tests ---

    #[test]
    fn writer_receipt_warns_on_step_without_source_ref() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        // seed a run first so step FK is satisfied
        writer
            .submit(WriteRequest {
                label: "seed-run".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-1".to_owned(),
                    kind: "session".to_owned(),
                    status: "active".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("seed run");

        let receipt = writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![StepInsert {
                    id: "step-1".to_owned(),
                    run_id: "run-1".to_owned(),
                    kind: "llm_call".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                    supersedes_id: None,
                }],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        assert!(
            !receipt.provenance_warnings.is_empty(),
            "step insert without source_ref must emit a provenance warning"
        );
    }

    #[test]
    fn writer_receipt_warns_on_action_without_source_ref() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        // seed run and step so action FK is satisfied
        writer
            .submit(WriteRequest {
                label: "seed".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-1".to_owned(),
                    kind: "session".to_owned(),
                    status: "active".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![StepInsert {
                    id: "step-1".to_owned(),
                    run_id: "run-1".to_owned(),
                    kind: "llm_call".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("seed");

        let receipt = writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![ActionInsert {
                    id: "action-1".to_owned(),
                    step_id: "step-1".to_owned(),
                    kind: "tool_call".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                    supersedes_id: None,
                }],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        assert!(
            !receipt.provenance_warnings.is_empty(),
            "action insert without source_ref must emit a provenance warning"
        );
    }

    #[test]
    fn writer_receipt_no_warnings_when_all_types_have_source_ref() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-1".to_owned(),
                    kind: "session".to_owned(),
                    status: "active".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![StepInsert {
                    id: "step-1".to_owned(),
                    run_id: "run-1".to_owned(),
                    kind: "llm_call".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                actions: vec![ActionInsert {
                    id: "action-1".to_owned(),
                    step_id: "step-1".to_owned(),
                    kind: "tool_call".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        assert!(
            receipt.provenance_warnings.is_empty(),
            "no warnings expected when all types have source_ref; got: {:?}",
            receipt.provenance_warnings
        );
    }

    // --- Item 4 Task 2: ProvenanceMode tests ---

    #[test]
    fn default_provenance_mode_is_warn() {
        // ProvenanceMode::Warn is the Default; a node without source_ref must warn, not error
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::default(),
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("Warn mode must not reject missing source_ref");

        assert!(
            !receipt.provenance_warnings.is_empty(),
            "Warn mode must emit a warning instead of rejecting"
        );
    }

    #[test]
    fn require_mode_rejects_node_without_source_ref() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Require,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "test".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-1".to_owned(),
                logical_id: "node-1".to_owned(),
                kind: "Note".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "Require mode must reject node without source_ref"
        );
    }

    #[test]
    fn require_mode_accepts_node_with_source_ref() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Require,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "test".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-1".to_owned(),
                logical_id: "node-1".to_owned(),
                kind: "Note".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("src-1".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            result.is_ok(),
            "Require mode must accept node with source_ref"
        );
    }

    #[test]
    fn require_mode_rejects_edge_without_source_ref() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Require,
        )
        .expect("writer");

        // seed nodes first so FK check doesn't interfere (Require mode rejects before DB touch)
        let result = writer.submit(WriteRequest {
            label: "test".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "row-a".to_owned(),
                    logical_id: "node-a".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
                NodeInsert {
                    row_id: "row-b".to_owned(),
                    logical_id: "node-b".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
            ],
            node_retires: vec![],
            edges: vec![EdgeInsert {
                row_id: "edge-row-1".to_owned(),
                logical_id: "edge-1".to_owned(),
                source_logical_id: "node-a".to_owned(),
                target_logical_id: "node-b".to_owned(),
                kind: "LINKS_TO".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
            }],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "Require mode must reject edge without source_ref"
        );
    }

    // --- Item 5: FTS projection coverage tests ---

    #[test]
    fn fts_row_has_correct_kind_from_co_submitted_node() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-1".to_owned(),
                    node_logical_id: "node-1".to_owned(),
                    text_content: "some text".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let kind: String = conn
            .query_row(
                "SELECT kind FROM fts_nodes WHERE chunk_id = 'chunk-1'",
                [],
                |row| row.get(0),
            )
            .expect("fts row");

        assert_eq!(kind, "Meeting");
    }

    #[test]
    fn fts_row_has_correct_text_content() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-1".to_owned(),
                    node_logical_id: "node-1".to_owned(),
                    text_content: "exactly this text".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let text: String = conn
            .query_row(
                "SELECT text_content FROM fts_nodes WHERE chunk_id = 'chunk-1'",
                [],
                |row| row.get(0),
            )
            .expect("fts row");

        assert_eq!(text, "exactly this text");
    }

    #[test]
    fn fts_row_has_correct_kind_from_pre_existing_node() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        // Request 1: node only
        writer
            .submit(WriteRequest {
                label: "r1".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Document".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("r1 write");

        // Request 2: chunk for pre-existing node
        writer
            .submit(WriteRequest {
                label: "r2".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-1".to_owned(),
                    node_logical_id: "node-1".to_owned(),
                    text_content: "some text".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("r2 write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let kind: String = conn
            .query_row(
                "SELECT kind FROM fts_nodes WHERE chunk_id = 'chunk-1'",
                [],
                |row| row.get(0),
            )
            .expect("fts row");

        assert_eq!(kind, "Document");
    }

    #[test]
    fn fts_derives_rows_for_multiple_chunks_per_node() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![
                    ChunkInsert {
                        id: "chunk-a".to_owned(),
                        node_logical_id: "node-1".to_owned(),
                        text_content: "intro".to_owned(),
                        byte_start: None,
                        byte_end: None,
                    },
                    ChunkInsert {
                        id: "chunk-b".to_owned(),
                        node_logical_id: "node-1".to_owned(),
                        text_content: "body".to_owned(),
                        byte_start: None,
                        byte_end: None,
                    },
                    ChunkInsert {
                        id: "chunk-c".to_owned(),
                        node_logical_id: "node-1".to_owned(),
                        text_content: "conclusion".to_owned(),
                        byte_start: None,
                        byte_end: None,
                    },
                ],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE node_logical_id = 'node-1'",
                [],
                |row| row.get(0),
            )
            .expect("fts count");

        assert_eq!(count, 3, "three chunks must produce three FTS rows");
    }

    #[test]
    fn fts_resolves_mixed_fast_and_db_paths() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        // Seed pre-existing node
        writer
            .submit(WriteRequest {
                label: "seed".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-existing".to_owned(),
                    logical_id: "existing-node".to_owned(),
                    kind: "Archive".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("seed");

        // Mixed request: new node (fast path) + chunk for pre-existing node (DB path)
        writer
            .submit(WriteRequest {
                label: "mixed".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-new".to_owned(),
                    logical_id: "new-node".to_owned(),
                    kind: "Inbox".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![
                    ChunkInsert {
                        id: "chunk-fast".to_owned(),
                        node_logical_id: "new-node".to_owned(),
                        text_content: "new content".to_owned(),
                        byte_start: None,
                        byte_end: None,
                    },
                    ChunkInsert {
                        id: "chunk-db".to_owned(),
                        node_logical_id: "existing-node".to_owned(),
                        text_content: "archive content".to_owned(),
                        byte_start: None,
                        byte_end: None,
                    },
                ],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("mixed write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let fast_kind: String = conn
            .query_row(
                "SELECT kind FROM fts_nodes WHERE chunk_id = 'chunk-fast'",
                [],
                |row| row.get(0),
            )
            .expect("fast path fts row");
        let db_kind: String = conn
            .query_row(
                "SELECT kind FROM fts_nodes WHERE chunk_id = 'chunk-db'",
                [],
                |row| row.get(0),
            )
            .expect("db path fts row");

        assert_eq!(fast_kind, "Inbox");
        assert_eq!(db_kind, "Archive");
    }

    #[test]
    fn prepare_write_rejects_empty_chunk_text() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "test".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-1".to_owned(),
                logical_id: "node-1".to_owned(),
                kind: "Note".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "chunk-1".to_owned(),
                node_logical_id: "node-1".to_owned(),
                text_content: String::new(),
                byte_start: None,
                byte_end: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "empty text_content must be rejected"
        );
    }

    #[test]
    fn receipt_reports_zero_backfills_when_none_submitted() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        assert_eq!(receipt.optional_backfill_count, 0);
    }

    #[test]
    fn receipt_reports_correct_backfill_count() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![
                    OptionalProjectionTask {
                        target: ProjectionTarget::Fts,
                        payload: "p1".to_owned(),
                    },
                    OptionalProjectionTask {
                        target: ProjectionTarget::Vec,
                        payload: "p2".to_owned(),
                    },
                    OptionalProjectionTask {
                        target: ProjectionTarget::All,
                        payload: "p3".to_owned(),
                    },
                ],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        assert_eq!(receipt.optional_backfill_count, 3);
    }

    #[test]
    fn backfill_tasks_are_not_executed_during_write() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        // Write a node + chunk. Submit a backfill task targeting FTS.
        // The write path must not create any extra FTS rows beyond the required one.
        writer
            .submit(WriteRequest {
                label: "test".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-1".to_owned(),
                    node_logical_id: "node-1".to_owned(),
                    text_content: "required text".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![OptionalProjectionTask {
                    target: ProjectionTarget::Fts,
                    payload: "backfill-payload".to_owned(),
                }],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE node_logical_id = 'node-1'",
                [],
                |row| row.get(0),
            )
            .expect("fts count");

        assert_eq!(count, 1, "backfill task must not create extra FTS rows");
    }

    #[test]
    fn fts_row_uses_new_kind_after_node_replace() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        // Write original node as "Note"
        writer
            .submit(WriteRequest {
                label: "v1".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-1".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-v1".to_owned(),
                    node_logical_id: "node-1".to_owned(),
                    text_content: "original".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v1 write");

        // Replace with "Meeting" kind + new chunk using ChunkPolicy::Replace
        writer
            .submit(WriteRequest {
                label: "v2".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-2".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: true,
                    chunk_policy: ChunkPolicy::Replace,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-v2".to_owned(),
                    node_logical_id: "node-1".to_owned(),
                    text_content: "updated".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v2 write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");

        // Old FTS row must be gone
        let old_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE chunk_id = 'chunk-v1'",
                [],
                |row| row.get(0),
            )
            .expect("old fts count");
        assert_eq!(old_count, 0, "ChunkPolicy::Replace must remove old FTS row");

        // New FTS row must use the new kind
        let new_kind: String = conn
            .query_row(
                "SELECT kind FROM fts_nodes WHERE chunk_id = 'chunk-v2'",
                [],
                |row| row.get(0),
            )
            .expect("new fts row");
        assert_eq!(new_kind, "Meeting", "FTS row must use updated node kind");
    }

    // --- Item 3: VecInsert tests ---

    #[test]
    fn vec_insert_empty_chunk_id_is_rejected() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");
        let result = writer.submit(WriteRequest {
            label: "vec-test".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![VecInsert {
                chunk_id: String::new(),
                embedding: vec![0.1, 0.2, 0.3],
            }],
            operational_writes: vec![],
        });
        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "empty chunk_id in VecInsert must be rejected"
        );
    }

    #[test]
    fn vec_insert_empty_embedding_is_rejected() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");
        let result = writer.submit(WriteRequest {
            label: "vec-test".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![VecInsert {
                chunk_id: "chunk-1".to_owned(),
                embedding: vec![],
            }],
            operational_writes: vec![],
        });
        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "empty embedding in VecInsert must be rejected"
        );
    }

    #[test]
    fn vec_insert_noop_without_feature() {
        // Without the sqlite-vec feature, a well-formed VecInsert must succeed
        // (no error) but not write to any vec table.
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");
        let result = writer.submit(WriteRequest {
            label: "vec-noop".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![VecInsert {
                chunk_id: "chunk-noop".to_owned(),
                embedding: vec![1.0, 2.0, 3.0],
            }],
            operational_writes: vec![],
        });
        #[cfg(not(feature = "sqlite-vec"))]
        result.expect("noop VecInsert without feature must succeed");
        // The result variable is used above; silence unused warning for cfg-on path.
        #[cfg(feature = "sqlite-vec")]
        let _ = result;
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn node_retire_preserves_vec_rows_for_later_restore() {
        use crate::sqlite::open_connection_with_vec;

        let db = NamedTempFile::new().expect("temporary db");
        let schema_manager = Arc::new(SchemaManager::new());

        {
            let conn = open_connection_with_vec(db.path()).expect("vec connection");
            schema_manager.bootstrap(&conn).expect("bootstrap");
            schema_manager
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 3)
                .expect("ensure profile");
        }

        let writer =
            WriterActor::start(db.path(), Arc::clone(&schema_manager), ProvenanceMode::Warn)
                .expect("writer");

        // Insert node + chunk + vec row
        writer
            .submit(WriteRequest {
                label: "setup".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-retire-vec".to_owned(),
                    logical_id: "node-retire-vec".to_owned(),
                    kind: "Doc".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-retire-vec".to_owned(),
                    node_logical_id: "node-retire-vec".to_owned(),
                    text_content: "text".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![VecInsert {
                    chunk_id: "chunk-retire-vec".to_owned(),
                    embedding: vec![0.1, 0.2, 0.3],
                }],
                operational_writes: vec![],
            })
            .expect("setup write");

        // Retire the node
        writer
            .submit(WriteRequest {
                label: "retire".to_owned(),
                nodes: vec![],
                node_retires: vec![NodeRetire {
                    logical_id: "node-retire-vec".to_owned(),
                    source_ref: Some("src".to_owned()),
                }],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("retire write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vec_nodes_active WHERE chunk_id = 'chunk-retire-vec'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(
            count, 1,
            "vec rows must remain available while the node is retired so restore can re-establish vector behavior"
        );
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn vec_cleanup_on_chunk_replace_removes_old_vec_rows() {
        use crate::sqlite::open_connection_with_vec;

        let db = NamedTempFile::new().expect("temporary db");
        let schema_manager = Arc::new(SchemaManager::new());

        {
            let conn = open_connection_with_vec(db.path()).expect("vec connection");
            schema_manager.bootstrap(&conn).expect("bootstrap");
            schema_manager
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 3)
                .expect("ensure profile");
        }

        let writer =
            WriterActor::start(db.path(), Arc::clone(&schema_manager), ProvenanceMode::Warn)
                .expect("writer");

        // Insert node + chunk-A + vec-A
        writer
            .submit(WriteRequest {
                label: "v1".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-replace-v1".to_owned(),
                    logical_id: "node-replace-vec".to_owned(),
                    kind: "Doc".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-replace-A".to_owned(),
                    node_logical_id: "node-replace-vec".to_owned(),
                    text_content: "version one".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![VecInsert {
                    chunk_id: "chunk-replace-A".to_owned(),
                    embedding: vec![0.1, 0.2, 0.3],
                }],
                operational_writes: vec![],
            })
            .expect("v1 write");

        // Upsert with Replace + chunk-B + vec-B
        writer
            .submit(WriteRequest {
                label: "v2".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-replace-v2".to_owned(),
                    logical_id: "node-replace-vec".to_owned(),
                    kind: "Doc".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src".to_owned()),
                    upsert: true,
                    chunk_policy: ChunkPolicy::Replace,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk-replace-B".to_owned(),
                    node_logical_id: "node-replace-vec".to_owned(),
                    text_content: "version two".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![VecInsert {
                    chunk_id: "chunk-replace-B".to_owned(),
                    embedding: vec![0.4, 0.5, 0.6],
                }],
                operational_writes: vec![],
            })
            .expect("v2 write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let count_a: i64 = conn
            .query_row(
                "SELECT count(*) FROM vec_nodes_active WHERE chunk_id = 'chunk-replace-A'",
                [],
                |row| row.get(0),
            )
            .expect("count A");
        let count_b: i64 = conn
            .query_row(
                "SELECT count(*) FROM vec_nodes_active WHERE chunk_id = 'chunk-replace-B'",
                [],
                |row| row.get(0),
            )
            .expect("count B");
        assert_eq!(
            count_a, 0,
            "old vec row (chunk-A) must be deleted on Replace"
        );
        assert_eq!(
            count_b, 1,
            "new vec row (chunk-B) must be present after Replace"
        );
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn vec_insert_is_persisted_when_feature_enabled() {
        use crate::sqlite::open_connection_with_vec;

        let db = NamedTempFile::new().expect("temporary db");
        let schema_manager = Arc::new(SchemaManager::new());

        // Open a vec-capable connection and bootstrap + ensure profile
        {
            let conn = open_connection_with_vec(db.path()).expect("vec connection");
            schema_manager.bootstrap(&conn).expect("bootstrap");
            schema_manager
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 3)
                .expect("ensure profile");
        }

        let writer =
            WriterActor::start(db.path(), Arc::clone(&schema_manager), ProvenanceMode::Warn)
                .expect("writer");

        writer
            .submit(WriteRequest {
                label: "vec-insert".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![VecInsert {
                    chunk_id: "chunk-vec".to_owned(),
                    embedding: vec![0.1, 0.2, 0.3],
                }],
                operational_writes: vec![],
            })
            .expect("vec insert write");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vec_nodes_active WHERE chunk_id = 'chunk-vec'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1, "VecInsert must persist a row in vec_nodes_active");
    }

    // --- WriteRequest size validation tests ---

    #[test]
    fn write_request_exceeding_node_limit_is_rejected() {
        let nodes: Vec<NodeInsert> = (0..10_001)
            .map(|i| NodeInsert {
                row_id: format!("row-{i}"),
                logical_id: format!("lg-{i}"),
                kind: "Note".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            })
            .collect();

        let request = WriteRequest {
            label: "too-many-nodes".to_owned(),
            nodes,
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        };

        let result = prepare_write(request, ProvenanceMode::Warn)
            .map(|_| ())
            .map_err(|e| format!("{e}"));
        assert!(
            matches!(result, Err(ref msg) if msg.contains("too many nodes")),
            "exceeding node limit must return InvalidWrite: got {result:?}"
        );
    }

    #[test]
    fn write_request_exceeding_total_limit_is_rejected() {
        // Spread items across fields to exceed 100_000 total
        // without exceeding any single per-field limit.
        // nodes(10k) + edges(10k) + chunks(50k) + vec_inserts(20001) + operational(10k) = 100_001
        let request = WriteRequest {
            label: "too-many-total".to_owned(),
            nodes: (0..10_000)
                .map(|i| NodeInsert {
                    row_id: format!("row-{i}"),
                    logical_id: format!("lg-{i}"),
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                })
                .collect(),
            node_retires: vec![],
            edges: (0..10_000)
                .map(|i| EdgeInsert {
                    row_id: format!("edge-row-{i}"),
                    logical_id: format!("edge-lg-{i}"),
                    kind: "link".to_owned(),
                    source_logical_id: format!("lg-{i}"),
                    target_logical_id: format!("lg-{}", i + 1),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                })
                .collect(),
            edge_retires: vec![],
            chunks: (0..50_000)
                .map(|i| ChunkInsert {
                    id: format!("chunk-{i}"),
                    node_logical_id: "lg-0".to_owned(),
                    text_content: "text".to_owned(),
                    byte_start: None,
                    byte_end: None,
                })
                .collect(),
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: (0..20_001)
                .map(|i| VecInsert {
                    chunk_id: format!("vec-chunk-{i}"),
                    embedding: vec![0.1],
                })
                .collect(),
            operational_writes: (0..10_000)
                .map(|i| OperationalWrite::Append {
                    collection: format!("col-{i}"),
                    record_key: format!("key-{i}"),
                    payload_json: "{}".to_owned(),
                    source_ref: None,
                })
                .collect(),
        };

        let result = prepare_write(request, ProvenanceMode::Warn)
            .map(|_| ())
            .map_err(|e| format!("{e}"));
        assert!(
            matches!(result, Err(ref msg) if msg.contains("too many total items")),
            "exceeding total item limit must return InvalidWrite: got {result:?}"
        );
    }

    #[test]
    fn write_request_within_limits_succeeds() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let result = writer.submit(WriteRequest {
            label: "within-limits".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-1".to_owned(),
                logical_id: "lg-1".to_owned(),
                kind: "Note".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        });

        assert!(
            result.is_ok(),
            "write request within limits must succeed: got {result:?}"
        );
    }
}
