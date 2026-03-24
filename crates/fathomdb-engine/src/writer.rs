use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;

use fathomdb_schema::SchemaManager;
use rusqlite::{TransactionBehavior, params};

use crate::{EngineError, projection::ProjectionTarget, sqlite};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionalProjectionTask {
    pub target: ProjectionTarget,
    pub payload: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeInsert {
    pub row_id: String,
    pub logical_id: String,
    pub kind: String,
    pub properties: String,
    pub source_ref: Option<String>,
    /// When true the writer supersedes the current active row for this logical_id
    /// before inserting this new version. The supersession and insert are atomic.
    pub upsert: bool,
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
    /// When true the writer supersedes the current active edge for this logical_id
    /// before inserting this new version. The supersession and insert are atomic.
    pub upsert: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkInsert {
    pub id: String,
    pub node_logical_id: String,
    pub text_content: String,
    pub byte_start: Option<i64>,
    pub byte_end: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunInsert {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepInsert {
    pub id: String,
    pub run_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionInsert {
    pub id: String,
    pub step_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteRequest {
    pub label: String,
    pub nodes: Vec<NodeInsert>,
    pub edges: Vec<EdgeInsert>,
    pub chunks: Vec<ChunkInsert>,
    pub runs: Vec<RunInsert>,
    pub steps: Vec<StepInsert>,
    pub actions: Vec<ActionInsert>,
    pub optional_backfills: Vec<OptionalProjectionTask>,
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
    edges: Vec<EdgeInsert>,
    chunks: Vec<ChunkInsert>,
    runs: Vec<RunInsert>,
    steps: Vec<StepInsert>,
    actions: Vec<ActionInsert>,
    /// node_logical_id → kind for nodes co-submitted in this request.
    /// Used by resolve_fts_rows to avoid a DB round-trip for the common case.
    node_kinds: HashMap<String, String>,
    /// Filled in by resolve_fts_rows in the writer thread before BEGIN IMMEDIATE.
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
}

impl WriterActor {
    pub fn start(
        path: impl AsRef<Path>,
        schema_manager: Arc<SchemaManager>,
    ) -> Result<Self, EngineError> {
        let database_path = path.as_ref().to_path_buf();
        let (sender, receiver) = mpsc::channel::<WriteMessage>();

        thread::Builder::new()
            .name("fathomdb-writer".to_owned())
            .spawn(move || writer_loop(database_path, schema_manager, receiver))
            .map_err(EngineError::Io)?;

        Ok(Self { sender })
    }

    pub fn submit(&self, request: WriteRequest) -> Result<WriteReceipt, EngineError> {
        let prepared = prepare_write(request);
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

fn prepare_write(request: WriteRequest) -> PreparedWrite {
    let node_kinds = request
        .nodes
        .iter()
        .map(|node| (node.logical_id.clone(), node.kind.clone()))
        .collect::<HashMap<_, _>>();

    PreparedWrite {
        label: request.label,
        nodes: request.nodes,
        edges: request.edges,
        chunks: request.chunks,
        runs: request.runs,
        steps: request.steps,
        actions: request.actions,
        node_kinds,
        required_fts_rows: Vec::new(),
        optional_backfills: request.optional_backfills,
    }
}

fn writer_loop(
    database_path: PathBuf,
    schema_manager: Arc<SchemaManager>,
    receiver: mpsc::Receiver<WriteMessage>,
) {
    let mut conn = match sqlite::open_connection(&database_path) {
        Ok(conn) => conn,
        Err(error) => {
            reject_all(receiver, error.to_string());
            return;
        }
    };

    if let Err(error) = schema_manager.bootstrap(&conn) {
        reject_all(receiver, error.to_string());
        return;
    }

    for message in receiver {
        let mut prepared = message.prepared;
        let result = resolve_and_apply(&mut conn, &mut prepared);
        let _ = message.reply.send(result);
    }
}

fn reject_all(receiver: mpsc::Receiver<WriteMessage>, error: String) {
    for message in receiver {
        let _ = message
            .reply
            .send(Err(EngineError::WriterRejected(error.clone())));
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

fn apply_write(
    conn: &mut rusqlite::Connection,
    prepared: &PreparedWrite,
) -> Result<WriteReceipt, rusqlite::Error> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for node in &prepared.nodes {
        if node.upsert {
            tx.execute(
                "UPDATE nodes SET superseded_at = unixepoch() \
                 WHERE logical_id = ?1 AND superseded_at IS NULL",
                params![node.logical_id],
            )?;
        }
        tx.execute(
            "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
             VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)",
            params![
                node.row_id,
                node.logical_id,
                node.kind,
                node.properties,
                node.source_ref,
            ],
        )?;
    }
    for edge in &prepared.edges {
        if edge.upsert {
            tx.execute(
                "UPDATE edges SET superseded_at = unixepoch() \
                 WHERE logical_id = ?1 AND superseded_at IS NULL",
                params![edge.logical_id],
            )?;
        }
        tx.execute(
            "INSERT INTO edges \
             (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at, source_ref) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, unixepoch(), ?7)",
            params![
                edge.row_id,
                edge.logical_id,
                edge.source_logical_id,
                edge.target_logical_id,
                edge.kind,
                edge.properties,
                edge.source_ref,
            ],
        )?;
    }
    for chunk in &prepared.chunks {
        tx.execute(
            "INSERT INTO chunks (id, node_logical_id, text_content, byte_start, byte_end, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, unixepoch())",
            params![
                chunk.id,
                chunk.node_logical_id,
                chunk.text_content,
                chunk.byte_start,
                chunk.byte_end,
            ],
        )?;
    }
    for run in &prepared.runs {
        tx.execute(
            "INSERT INTO runs (id, kind, status, properties, created_at, source_ref) \
             VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)",
            params![run.id, run.kind, run.status, run.properties, run.source_ref],
        )?;
    }
    for step in &prepared.steps {
        tx.execute(
            "INSERT INTO steps (id, run_id, kind, status, properties, created_at, source_ref) \
             VALUES (?1, ?2, ?3, ?4, ?5, unixepoch(), ?6)",
            params![
                step.id,
                step.run_id,
                step.kind,
                step.status,
                step.properties,
                step.source_ref,
            ],
        )?;
    }
    for action in &prepared.actions {
        tx.execute(
            "INSERT INTO actions (id, step_id, kind, status, properties, created_at, source_ref) \
             VALUES (?1, ?2, ?3, ?4, ?5, unixepoch(), ?6)",
            params![
                action.id,
                action.step_id,
                action.kind,
                action.status,
                action.properties,
                action.source_ref,
            ],
        )?;
    }
    for fts_row in &prepared.required_fts_rows {
        tx.execute(
            "INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                fts_row.chunk_id,
                fts_row.node_logical_id,
                fts_row.kind,
                fts_row.text_content,
            ],
        )?;
    }
    tx.commit()?;

    let provenance_warnings: Vec<String> = prepared
        .nodes
        .iter()
        .filter(|node| node.source_ref.is_none())
        .map(|node| format!("node '{}' has no source_ref", node.logical_id))
        .collect();

    Ok(WriteReceipt {
        label: prepared.label.clone(),
        optional_backfill_count: prepared.optional_backfills.len(),
        provenance_warnings,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fathomdb_schema::SchemaManager;
    use tempfile::NamedTempFile;

    use crate::{
        ActionInsert, ChunkInsert, EdgeInsert, EngineError, NodeInsert, RunInsert, StepInsert,
        WriteRequest, WriterActor,
    };

    #[test]
    fn writer_executes_runtime_table_rows() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

        let receipt = writer
            .submit(WriteRequest {
                label: "runtime".to_owned(),
                nodes: vec![],
                edges: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-1".to_owned(),
                    kind: "session".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
                steps: vec![StepInsert {
                    id: "step-1".to_owned(),
                    run_id: "run-1".to_owned(),
                    kind: "llm".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
                actions: vec![ActionInsert {
                    id: "action-1".to_owned(),
                    step_id: "step-1".to_owned(),
                    kind: "emit".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
                optional_backfills: vec![],
            })
            .expect("write receipt");

        assert_eq!(receipt.label, "runtime");
    }

    #[test]
    fn writer_upsert_supersedes_prior_active_node() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

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
                }],
                edges: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
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
                }],
                edges: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
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
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

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
                    },
                    NodeInsert {
                        row_id: "row-task".to_owned(),
                        logical_id: "task-1".to_owned(),
                        kind: "Task".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some("src-1".to_owned()),
                        upsert: false,
                    },
                ],
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
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
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
    fn writer_upsert_supersedes_prior_active_edge() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

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
                    },
                    NodeInsert {
                        row_id: "row-b".to_owned(),
                        logical_id: "node-b".to_owned(),
                        kind: "Task".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some("src-1".to_owned()),
                        upsert: false,
                    },
                ],
                edges: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
            })
            .expect("nodes write");

        // Write v1 edge
        writer
            .submit(WriteRequest {
                label: "edge-v1".to_owned(),
                nodes: vec![],
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
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
            })
            .expect("edge v1 write");

        // Upsert v2 edge
        writer
            .submit(WriteRequest {
                label: "edge-v2".to_owned(),
                nodes: vec![],
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
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
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
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

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
                }],
                edges: vec![],
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
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

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
                }],
                edges: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
            })
            .expect("write receipt");

        assert_eq!(receipt.provenance_warnings.len(), 1);
        assert!(receipt.provenance_warnings[0].contains("logical-1"));
    }

    #[test]
    fn writer_receipt_no_warnings_when_all_nodes_have_source_ref() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

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
                }],
                edges: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
            })
            .expect("write receipt");

        assert!(receipt.provenance_warnings.is_empty());
    }

    #[test]
    fn writer_accepts_chunk_for_pre_existing_node() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

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
                }],
                edges: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
            })
            .expect("r1 write");

        // Request 2: submit chunk for pre-existing node
        writer
            .submit(WriteRequest {
                label: "r2".to_owned(),
                nodes: vec![],
                edges: vec![],
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
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

        let result = writer.submit(WriteRequest {
            label: "bad".to_owned(),
            nodes: vec![],
            edges: vec![],
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
        });

        assert!(
            matches!(result, Err(EngineError::InvalidWrite(_))),
            "completely unknown node must return InvalidWrite"
        );
    }

    #[test]
    fn writer_executes_typed_nodes_chunks_and_derived_projections() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

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
                }],
                edges: vec![],
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
            })
            .expect("write receipt");

        assert_eq!(receipt.label, "seed");
    }
}
