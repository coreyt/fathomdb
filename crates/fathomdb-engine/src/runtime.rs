use std::path::Path;
use std::sync::Arc;

use fathomdb_schema::SchemaManager;

use crate::{
    AdminHandle, AdminService, EngineError, ExecutionCoordinator, ProvenanceMode, WriterActor,
};

/// Core engine runtime.
///
/// # Drop order invariant
///
/// Fields are ordered so that `coordinator` (reader connections) drops before
/// `writer` (writer thread + connection). This ensures the writer's
/// `sqlite3_close()` is the last connection to the database, which triggers
/// `SQLite`'s automatic passive WAL checkpoint and WAL/shm file cleanup.
/// Do not reorder these fields.
#[derive(Debug)]
pub struct EngineRuntime {
    coordinator: ExecutionCoordinator,
    writer: WriterActor,
    admin: AdminHandle,
}

// Required by #[pyclass(frozen)] — guards against future fields breaking thread safety.
#[allow(clippy::used_underscore_items)]
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<EngineRuntime>();
    }
};

impl EngineRuntime {
    /// # Errors
    /// Returns [`EngineError`] if the database connection cannot be opened, schema bootstrap fails,
    /// or the writer actor cannot be started.
    pub fn open(
        path: impl AsRef<Path>,
        provenance_mode: ProvenanceMode,
        vector_dimension: Option<usize>,
        read_pool_size: usize,
    ) -> Result<Self, EngineError> {
        let schema_manager = Arc::new(SchemaManager::new());
        let coordinator = ExecutionCoordinator::open(
            path.as_ref(),
            Arc::clone(&schema_manager),
            vector_dimension,
            read_pool_size,
        )?;
        let writer =
            WriterActor::start(path.as_ref(), Arc::clone(&schema_manager), provenance_mode)?;
        let admin = AdminHandle::new(AdminService::new(path.as_ref(), schema_manager));

        Ok(Self {
            coordinator,
            writer,
            admin,
        })
    }

    pub fn coordinator(&self) -> &ExecutionCoordinator {
        &self.coordinator
    }

    pub fn writer(&self) -> &WriterActor {
        &self.writer
    }

    pub fn admin(&self) -> &AdminHandle {
        &self.admin
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use fathomdb_query::QueryBuilder;

    use crate::{ChunkInsert, ChunkPolicy, NodeInsert, ProvenanceMode, WriteRequest};

    use super::EngineRuntime;

    /// Issue #30: the engine must support concurrent reads from multiple threads.
    #[test]
    fn concurrent_reads_from_multiple_threads() {
        let dir = tempfile::tempdir().expect("temp dir");
        let runtime = Arc::new(
            EngineRuntime::open(dir.path().join("test.db"), ProvenanceMode::Warn, None, 4)
                .expect("open"),
        );

        runtime
            .writer()
            .submit(WriteRequest {
                label: "seed".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "r1".to_owned(),
                    logical_id: "t:1".to_owned(),
                    kind: "Test".to_owned(),
                    properties: r#"{"v":1}"#.to_owned(),
                    source_ref: Some("test".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "c1".to_owned(),
                    node_logical_id: "t:1".to_owned(),
                    text_content: "hello world".to_owned(),
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

        let compiled = QueryBuilder::nodes("Test")
            .limit(10)
            .compile()
            .expect("compile");

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let rt = Arc::clone(&runtime);
                let q = compiled.clone();
                std::thread::spawn(move || {
                    let rows = rt
                        .coordinator()
                        .execute_compiled_read(&q)
                        .expect("query succeeds");
                    assert_eq!(rows.nodes.len(), 1);
                })
            })
            .collect();

        for h in handles {
            h.join().expect("worker thread panicked");
        }
    }

    /// Verify that dropping `EngineRuntime` joins the writer thread and triggers
    /// `SQLite`'s automatic passive WAL checkpoint (readers drop before writer).
    #[test]
    fn drop_joins_writer_and_checkpoints_wal() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test.db");
        let wal_path = dir.path().join("test.db-wal");

        {
            let runtime =
                EngineRuntime::open(&db_path, ProvenanceMode::Warn, None, 4).expect("open");

            runtime
                .writer()
                .submit(WriteRequest {
                    label: "seed".to_owned(),
                    nodes: vec![NodeInsert {
                        row_id: "r1".to_owned(),
                        logical_id: "t:1".to_owned(),
                        kind: "Test".to_owned(),
                        properties: r#"{"v":1}"#.to_owned(),
                        source_ref: Some("test".to_owned()),
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
        }
        // After drop: WAL should be checkpointed and removed.
        assert!(
            !wal_path.exists(),
            "WAL file should be cleaned up after graceful drop"
        );

        // Reopen and verify data persists.
        let runtime = EngineRuntime::open(&db_path, ProvenanceMode::Warn, None, 4).expect("reopen");
        let compiled = QueryBuilder::nodes("Test")
            .limit(10)
            .compile()
            .expect("compile");
        let rows = runtime
            .coordinator()
            .execute_compiled_read(&compiled)
            .expect("query");
        assert_eq!(rows.nodes.len(), 1);
    }
}
