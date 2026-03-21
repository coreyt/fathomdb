use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;

use fathomdb_schema::SchemaManager;
use rusqlite::TransactionBehavior;

use crate::{sqlite, projection::ProjectionTarget, EngineError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionalProjectionTask {
    pub target: ProjectionTarget,
    pub payload: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteEnvelope {
    pub label: String,
    pub canonical_statements: Vec<String>,
    pub required_projection_statements: Vec<String>,
    pub optional_backfills: Vec<OptionalProjectionTask>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteReceipt {
    pub label: String,
    pub optional_backfill_count: usize,
}

struct WriteMessage {
    envelope: WriteEnvelope,
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

    pub fn submit(&self, envelope: WriteEnvelope) -> Result<WriteReceipt, EngineError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.sender
            .send(WriteMessage {
                envelope,
                reply: reply_tx,
            })
            .map_err(|error| EngineError::WriterRejected(error.to_string()))?;

        reply_rx
            .recv()
            .map_err(|error| EngineError::WriterRejected(error.to_string()))?
            .map_err(EngineError::WriterRejected)
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
        let result = apply_write(&mut conn, &message.envelope).map_err(|error| error.to_string());
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
    envelope: &WriteEnvelope,
) -> Result<WriteReceipt, rusqlite::Error> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for statement in &envelope.canonical_statements {
        tx.execute_batch(statement)?;
    }
    for statement in &envelope.required_projection_statements {
        tx.execute_batch(statement)?;
    }
    tx.commit()?;

    Ok(WriteReceipt {
        label: envelope.label.clone(),
        optional_backfill_count: envelope.optional_backfills.len(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fathomdb_schema::SchemaManager;
    use tempfile::NamedTempFile;

    use crate::{WriteEnvelope, WriterActor};

    #[test]
    fn writer_executes_canonical_and_projection_statements() {
        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(db.path(), Arc::new(SchemaManager::new())).expect("writer");

        let receipt = writer
            .submit(WriteEnvelope {
                label: "seed".to_owned(),
                canonical_statements: vec![r#"
                    INSERT INTO nodes (row_id, logical_id, kind, properties, created_at)
                    VALUES ('row-1', 'logical-1', 'Meeting', '{}', unixepoch())
                "#
                .to_owned()],
                required_projection_statements: vec![],
                optional_backfills: vec![],
            })
            .expect("write receipt");

        assert_eq!(receipt.label, "seed");
    }
}
