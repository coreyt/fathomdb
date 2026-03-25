use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;

use fathomdb_schema::SchemaManager;
use rusqlite::{TransactionBehavior, params};

use crate::{EngineError, ids::new_id, projection::ProjectionTarget, sqlite};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionalProjectionTask {
    pub target: ProjectionTarget,
    pub payload: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChunkPolicy {
    #[default]
    Preserve,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRetire {
    pub logical_id: String,
    pub source_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeRetire {
    pub logical_id: String,
    pub source_ref: Option<String>,
}

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteReceipt {
    pub label: String,
    pub optional_backfill_count: usize,
    pub provenance_warnings: Vec<String>,
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
    /// `node_logical_id` → kind for nodes co-submitted in this request.
    /// Used by `resolve_fts_rows` to avoid a DB round-trip for the common case.
    node_kinds: HashMap<String, String>,
    /// Filled in by `resolve_fts_rows` in the writer thread before BEGIN IMMEDIATE.
    required_fts_rows: Vec<FtsProjectionRow>,
    optional_backfills: Vec<OptionalProjectionTask>,
}

struct WriteMessage {
    prepared: PreparedWrite,
    reply: Sender<Result<WriteReceipt, EngineError>>,
}

#[derive(Debug)]
pub struct WriterActor {
    sender: Sender<WriteMessage>,
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
        let (sender, receiver) = mpsc::channel::<WriteMessage>();

        thread::Builder::new()
            .name("fathomdb-writer".to_owned())
            .spawn(move || writer_loop(&database_path, &schema_manager, receiver))
            .map_err(EngineError::Io)?;

        Ok(Self {
            sender,
            provenance_mode,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the write request validation fails, the writer actor has shut
    /// down, or the underlying `SQLite` transaction fails.
    pub fn submit(&self, request: WriteRequest) -> Result<WriteReceipt, EngineError> {
        let prepared = prepare_write(request, self.provenance_mode)?;
        let (reply_tx, reply_rx) = mpsc::channel();
        self.sender
            .send(WriteMessage {
                prepared,
                reply: reply_tx,
            })
            .map_err(|error| EngineError::WriterRejected(error.to_string()))?;

        reply_rx
            .recv()
            .map_err(|error| EngineError::WriterRejected(error.to_string()))
            .and_then(|result| result)
    }
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

#[allow(clippy::too_many_lines)]
fn prepare_write(
    request: WriteRequest,
    mode: ProvenanceMode,
) -> Result<PreparedWrite, EngineError> {
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
    let mut conn = match sqlite::open_connection(database_path) {
        Ok(conn) => conn,
        Err(error) => {
            reject_all(receiver, &error.to_string());
            return;
        }
    };

    if let Err(error) = schema_manager.bootstrap(&conn) {
        reject_all(receiver, &error.to_string());
        return;
    }

    for message in receiver {
        let mut prepared = message.prepared;
        let result = resolve_and_apply(&mut conn, &mut prepared);
        let _ = message.reply.send(result);
    }
}

fn reject_all(receiver: mpsc::Receiver<WriteMessage>, error: &str) {
    for message in receiver {
        let _ = message
            .reply
            .send(Err(EngineError::WriterRejected(error.to_string())));
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
    Ok(())
}

fn resolve_and_apply(
    conn: &mut rusqlite::Connection,
    prepared: &mut PreparedWrite,
) -> Result<WriteReceipt, EngineError> {
    resolve_fts_rows(conn, prepared)?;
    apply_write(conn, prepared).map_err(EngineError::Sqlite)
}

#[allow(clippy::too_many_lines)]
fn apply_write(
    conn: &mut rusqlite::Connection,
    prepared: &PreparedWrite,
) -> Result<WriteReceipt, rusqlite::Error> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // Node retires: clear FTS, clear chunks, mark superseded, record audit event.
    {
        let mut del_fts = tx.prepare_cached("DELETE FROM fts_nodes WHERE node_logical_id = ?1")?;
        let mut del_chunks = tx.prepare_cached("DELETE FROM chunks WHERE node_logical_id = ?1")?;
        let mut sup_node = tx.prepare_cached(
            "UPDATE nodes SET superseded_at = unixepoch() \
             WHERE logical_id = ?1 AND superseded_at IS NULL",
        )?;
        let mut ins_event = tx.prepare_cached(
            "INSERT INTO provenance_events (id, event_type, subject, source_ref) \
             VALUES (?1, 'node_retire', ?2, ?3)",
        )?;
        #[cfg(feature = "sqlite-vec")]
        let vec_del_sql = "DELETE FROM vec_nodes_active WHERE chunk_id IN \
                           (SELECT id FROM chunks WHERE node_logical_id = ?1)";
        #[cfg(feature = "sqlite-vec")]
        let mut del_vec = match tx.prepare_cached(vec_del_sql) {
            Ok(stmt) => Some(stmt),
            Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                if msg.contains("vec_nodes_active") || msg.contains("vec0") =>
            {
                None
            }
            Err(e) => return Err(e),
        };
        for retire in &prepared.node_retires {
            #[cfg(feature = "sqlite-vec")]
            if let Some(ref mut stmt) = del_vec {
                stmt.execute(params![retire.logical_id])?;
            }
            del_fts.execute(params![retire.logical_id])?;
            del_chunks.execute(params![retire.logical_id])?;
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
            Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                if msg.contains("vec_nodes_active") || msg.contains("vec0") =>
            {
                None
            }
            Err(e) => return Err(e),
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
            Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                if msg.contains("vec_nodes_active") || msg.contains("vec0") =>
            {
                // vec profile absent: vec inserts are silently skipped.
            }
            Err(e) => return Err(e),
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
        .collect();

    Ok(WriteReceipt {
        label: prepared.label.clone(),
        optional_backfill_count: prepared.optional_backfills.len(),
        provenance_warnings,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use fathomdb_schema::SchemaManager;
    use tempfile::NamedTempFile;

    use crate::{
        ActionInsert, ChunkInsert, ChunkPolicy, EdgeInsert, EdgeRetire, EngineError, NodeInsert,
        NodeRetire, OptionalProjectionTask, ProvenanceMode, RunInsert, StepInsert, VecInsert,
        WriteRequest, WriterActor, projection::ProjectionTarget,
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
            })
            .expect("write receipt");

        assert_eq!(receipt.label, "runtime");
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
    fn writer_node_retire_cleans_chunks_and_fts() {
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

        assert_eq!(chunk_count, 0, "chunks must be deleted after node retire");
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
        });
        #[cfg(not(feature = "sqlite-vec"))]
        result.expect("noop VecInsert without feature must succeed");
        // The result variable is used above; silence unused warning for cfg-on path.
        #[cfg(feature = "sqlite-vec")]
        let _ = result;
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn vec_cleanup_on_node_retire_removes_vec_rows() {
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
        assert_eq!(count, 0, "vec rows must be deleted when node is retired");
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
}
