use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;

use fathomdb_schema::SchemaManager;
use rusqlite::{params, TransactionBehavior};

use crate::{sqlite, projection::ProjectionTarget, EngineError};

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
pub struct WriteRequest {
    pub label: String,
    pub nodes: Vec<NodeInsert>,
    pub chunks: Vec<ChunkInsert>,
    pub optional_backfills: Vec<OptionalProjectionTask>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteReceipt {
    pub label: String,
    pub optional_backfill_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FtsProjectionRow {
    chunk_id: String,
    node_logical_id: String,
    kind: String,
    text_content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedWrite {
    label: String,
    nodes: Vec<NodeInsert>,
    chunks: Vec<ChunkInsert>,
    required_fts_rows: Vec<FtsProjectionRow>,
    optional_backfills: Vec<OptionalProjectionTask>,
}

struct WriteMessage {
    prepared: PreparedWrite,
    reply: Sender<Result<WriteReceipt, String>>,
}

#[derive(Debug)]
pub struct WriterActor {
    sender: Sender<WriteMessage>,
}

impl WriterActor {
    pub fn start(path: impl AsRef<Path>, schema_manager: Arc<SchemaManager>) -> Result<Self, EngineError> {
        let database_path = path.as_ref().to_path_buf();
        let (sender, receiver) = mpsc::channel::<WriteMessage>();

        thread::Builder::new()
            .name("fathomdb-writer".to_owned())
            .spawn(move || writer_loop(database_path, schema_manager, receiver))
            .map_err(EngineError::Io)?;

        Ok(Self { sender })
    }

    pub fn submit(&self, request: WriteRequest) -> Result<WriteReceipt, EngineError> {
        let prepared = prepare_write(request)?;
        let (reply_tx, reply_rx) = mpsc::channel();
        self.sender
            .send(WriteMessage {
                prepared,
                reply: reply_tx,
            })
            .map_err(|error| EngineError::WriterRejected(error.to_string()))?;

        reply_rx
            .recv()
            .map_err(|error| EngineError::WriterRejected(error.to_string()))?
            .map_err(EngineError::WriterRejected)
    }
}

fn prepare_write(request: WriteRequest) -> Result<PreparedWrite, EngineError> {
    let node_kinds = request
        .nodes
        .iter()
        .map(|node| (node.logical_id.clone(), node.kind.clone()))
        .collect::<HashMap<_, _>>();
    let mut required_fts_rows = Vec::with_capacity(request.chunks.len());

    for chunk in &request.chunks {
        let Some(kind) = node_kinds.get(&chunk.node_logical_id) else {
            return Err(EngineError::InvalidWrite(format!(
                "chunk '{}' references node_logical_id '{}' that is not present in the same write request",
                chunk.id, chunk.node_logical_id
            )));
        };
        required_fts_rows.push(FtsProjectionRow {
            chunk_id: chunk.id.clone(),
            node_logical_id: chunk.node_logical_id.clone(),
            kind: kind.clone(),
            text_content: chunk.text_content.clone(),
        });
    }

    Ok(PreparedWrite {
        label: request.label,
        nodes: request.nodes,
        chunks: request.chunks,
        required_fts_rows,
        optional_backfills: request.optional_backfills,
    })
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
        let result = apply_write(&mut conn, &message.prepared).map_err(|error| error.to_string());
        let _ = message.reply.send(result);
    }
}

fn reject_all(receiver: mpsc::Receiver<WriteMessage>, error: String) {
    for message in receiver {
        let _ = message.reply.send(Err(error.clone()));
    }
}

fn apply_write(
    conn: &mut rusqlite::Connection,
    prepared: &PreparedWrite,
) -> Result<WriteReceipt, rusqlite::Error> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for node in &prepared.nodes {
        tx.execute(
            r#"
            INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
            VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)
            "#,
            params![
                node.row_id,
                node.logical_id,
                node.kind,
                node.properties,
                node.source_ref,
            ],
        )?;
    }
    for chunk in &prepared.chunks {
        tx.execute(
            r#"
            INSERT INTO chunks (id, node_logical_id, text_content, byte_start, byte_end, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, unixepoch())
            "#,
            params![
                chunk.id,
                chunk.node_logical_id,
                chunk.text_content,
                chunk.byte_start,
                chunk.byte_end,
            ],
        )?;
    }
    for fts_row in &prepared.required_fts_rows {
        tx.execute(
            r#"
            INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                fts_row.chunk_id,
                fts_row.node_logical_id,
                fts_row.kind,
                fts_row.text_content,
            ],
        )?;
    }
    tx.commit()?;

    Ok(WriteReceipt {
        label: prepared.label.clone(),
        optional_backfill_count: prepared.optional_backfills.len(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fathomdb_schema::SchemaManager;
    use tempfile::NamedTempFile;

    use crate::{ChunkInsert, NodeInsert, WriteRequest, WriterActor};

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
                }],
                chunks: vec![ChunkInsert {
                    id: "chunk-1".to_owned(),
                    node_logical_id: "logical-1".to_owned(),
                    text_content: "budget discussion".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                optional_backfills: vec![],
            })
            .expect("write receipt");

        assert_eq!(receipt.label, "seed");
    }
}
