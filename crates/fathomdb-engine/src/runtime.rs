use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;

use fathomdb_schema::SchemaManager;

use crate::{
    AdminHandle, AdminService, EngineError, ExecutionCoordinator, ProvenanceMode, QueryEmbedder,
    VectorProjectionActor, WriterActor,
    database_lock::DatabaseLock,
    rebuild_actor::{RebuildActor, RebuildRequest, recover_interrupted_rebuilds},
    telemetry::{TelemetryCounters, TelemetryLevel, TelemetrySnapshot},
};

/// Core engine runtime.
///
/// # Drop order invariant
///
/// Fields are ordered so that `coordinator` (reader connections) drops before
/// `writer` (writer thread + connection).  This ensures the writer's
/// `sqlite3_close()` is the last connection to the database, which triggers
/// `SQLite`'s automatic passive WAL checkpoint and WAL/shm file cleanup.
/// `_vector_actor` drops after `writer` and before `_rebuild` so the vector
/// projection thread stops submitting writer messages before the writer
/// channel closes (actually the writer drops first because we hold the
/// writer in an `Arc` shared with the admin service, and the Arc survives
/// through admin; effectively the admin handle drops before the vector
/// actor's join).
/// `_rebuild` drops before `_lock` so the rebuild thread's connection closes
/// before the exclusive file lock is released.
/// `_lock` drops last so the exclusive file lock is released only after all
/// connections are closed.  Do not reorder these fields.
///
/// `telemetry` holds shared counters and has no drop-order concern (atomics).
#[derive(Debug)]
pub struct EngineRuntime {
    telemetry: Arc<TelemetryCounters>,
    coordinator: ExecutionCoordinator,
    admin: AdminHandle,
    writer: Arc<WriterActor>,
    /// Background worker for `vector_projection_work`.  Held between
    /// `writer` and `_rebuild` in drop order.
    _vector_actor: VectorProjectionActor,
    /// Sender side of the rebuild channel.  Dropped before `_rebuild` so the
    /// rebuild thread's loop exits before we join it.
    _rebuild_sender: mpsc::SyncSender<RebuildRequest>,
    _rebuild: RebuildActor,
    _lock: DatabaseLock,
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
        telemetry_level: TelemetryLevel,
        query_embedder: Option<Arc<dyn QueryEmbedder>>,
    ) -> Result<Self, EngineError> {
        let lock = DatabaseLock::acquire(path.as_ref())?;

        if read_pool_size == 0 {
            return Err(EngineError::InvalidConfig(
                "read_pool_size must be >= 1, got 0".to_owned(),
            ));
        }

        trace_info!(
            path = %path.as_ref().display(),
            provenance_mode = ?provenance_mode,
            vector_dimension = ?vector_dimension,
            read_pool_size,
            telemetry_level = ?telemetry_level,
            "engine opening"
        );
        let _ = telemetry_level; // Used by trace_info! when tracing feature is active
        let telemetry = Arc::new(TelemetryCounters::default());
        let schema_manager = Arc::new(SchemaManager::new());
        let coordinator = ExecutionCoordinator::open(
            path.as_ref(),
            Arc::clone(&schema_manager),
            vector_dimension,
            read_pool_size,
            Arc::clone(&telemetry),
            query_embedder,
        )?;
        // Ensure the sqlite-vec auto-extension is registered globally BEFORE
        // the writer thread opens its connection, so it can create/insert
        // into `vec_<kind>` virtual tables for vector projection apply
        // flows.  Registration is idempotent at the SQLite level.
        #[cfg(feature = "sqlite-vec")]
        {
            let _prime = crate::sqlite::open_connection_with_vec(path.as_ref())?;
        }

        let writer = Arc::new(WriterActor::start(
            path.as_ref(),
            Arc::clone(&schema_manager),
            provenance_mode,
            Arc::clone(&telemetry),
        )?);

        // Crash recovery: mark any interrupted rebuilds (PENDING/BUILDING/SWAPPING)
        // as FAILED and clean up their staging rows.  Must run before the
        // RebuildActor starts so the actor never sees stale non-terminal state.
        {
            let recovery_conn = crate::sqlite::open_connection(path.as_ref())?;
            recover_interrupted_rebuilds(&recovery_conn)?;
        }

        // Rebuild actor: create channel, start thread, pass sender to AdminService.
        let (rebuild_sender, rebuild_receiver) = mpsc::sync_channel::<RebuildRequest>(64);
        let rebuild_actor =
            RebuildActor::start(path.as_ref(), Arc::clone(&schema_manager), rebuild_receiver)?;
        let admin = AdminHandle::new(AdminService::new_with_engine(
            path.as_ref(),
            schema_manager,
            rebuild_sender.clone(),
            Arc::clone(&writer),
        ));
        let vector_actor = VectorProjectionActor::start(writer.as_ref())?;

        trace_info!(path = %path.as_ref().display(), "engine opened");
        Ok(Self {
            telemetry,
            coordinator,
            admin,
            writer,
            _vector_actor: vector_actor,
            _rebuild_sender: rebuild_sender,
            _rebuild: rebuild_actor,
            _lock: lock,
        })
    }

    pub fn coordinator(&self) -> &ExecutionCoordinator {
        &self.coordinator
    }

    pub fn writer(&self) -> &WriterActor {
        &self.writer
    }

    /// Cloneable shared handle to the writer actor.
    #[must_use]
    pub fn writer_arc(&self) -> Arc<WriterActor> {
        Arc::clone(&self.writer)
    }

    pub fn admin(&self) -> &AdminHandle {
        &self.admin
    }

    /// Shared telemetry counters for incrementing from the public API layer.
    pub fn telemetry(&self) -> &Arc<TelemetryCounters> {
        &self.telemetry
    }

    /// Read all telemetry counters and aggregate `SQLite` cache status across
    /// the reader pool.
    #[must_use]
    pub fn telemetry_snapshot(&self) -> TelemetrySnapshot {
        let mut snapshot = self.telemetry.snapshot();
        snapshot.sqlite_cache = self.coordinator.aggregate_cache_status();
        snapshot
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use fathomdb_query::QueryBuilder;

    use crate::{
        ChunkInsert, ChunkPolicy, NodeInsert, ProvenanceMode, TelemetryLevel, WriteRequest,
    };

    use super::EngineRuntime;

    /// Issue #30: the engine must support concurrent reads from multiple threads.
    #[test]
    fn concurrent_reads_from_multiple_threads() {
        let dir = tempfile::tempdir().expect("temp dir");
        let runtime = Arc::new(
            EngineRuntime::open(
                dir.path().join("test.db"),
                ProvenanceMode::Warn,
                None,
                4,
                TelemetryLevel::Counters,
                None,
            )
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
                    content_ref: None,
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
                    content_hash: None,
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

    #[test]
    fn open_same_database_twice_returns_database_locked() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test.db");

        let _first = EngineRuntime::open(
            &db_path,
            ProvenanceMode::Warn,
            None,
            4,
            TelemetryLevel::Counters,
            None,
        )
        .expect("open");
        let second = EngineRuntime::open(
            &db_path,
            ProvenanceMode::Warn,
            None,
            4,
            TelemetryLevel::Counters,
            None,
        );

        assert!(second.is_err(), "second open must fail");
        let err = second.expect_err("second open must fail");
        assert!(
            matches!(err, crate::EngineError::DatabaseLocked(_)),
            "expected DatabaseLocked, got: {err:?}"
        );
        assert!(
            err.to_string().contains("already in use"),
            "error must mention 'already in use': {err}"
        );
    }

    #[test]
    fn database_reopens_after_drop() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test.db");

        {
            let _runtime = EngineRuntime::open(
                &db_path,
                ProvenanceMode::Warn,
                None,
                4,
                TelemetryLevel::Counters,
                None,
            )
            .expect("first open");
        }

        let runtime = EngineRuntime::open(
            &db_path,
            ProvenanceMode::Warn,
            None,
            4,
            TelemetryLevel::Counters,
            None,
        )
        .expect("reopen");
        let compiled = QueryBuilder::nodes("Test")
            .limit(10)
            .compile()
            .expect("compile");
        let rows = runtime
            .coordinator()
            .execute_compiled_read(&compiled)
            .expect("query");
        assert!(rows.nodes.is_empty());
    }

    #[test]
    fn lock_error_includes_pid() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test.db");

        let _first = EngineRuntime::open(
            &db_path,
            ProvenanceMode::Warn,
            None,
            4,
            TelemetryLevel::Counters,
            None,
        )
        .expect("open");
        let err = EngineRuntime::open(
            &db_path,
            ProvenanceMode::Warn,
            None,
            4,
            TelemetryLevel::Counters,
            None,
        )
        .expect_err("second open must fail");

        let msg = err.to_string();
        assert!(
            msg.contains("already in use"),
            "error must mention 'already in use': {msg}"
        );
        // PID is best-effort; on Windows exclusive locks prevent reading the
        // lock file from a second handle.
        if cfg!(unix) {
            let our_pid = std::process::id().to_string();
            assert!(
                msg.contains(&our_pid),
                "error must contain holder pid {our_pid}: {msg}"
            );
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
            let runtime = EngineRuntime::open(
                &db_path,
                ProvenanceMode::Warn,
                None,
                4,
                TelemetryLevel::Counters,
                None,
            )
            .expect("open");

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
                        content_ref: None,
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
        let runtime = EngineRuntime::open(
            &db_path,
            ProvenanceMode::Warn,
            None,
            4,
            TelemetryLevel::Counters,
            None,
        )
        .expect("reopen");
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

    /// Helper: create a seeded runtime with one node and one chunk.
    fn seeded_runtime() -> (tempfile::TempDir, EngineRuntime) {
        let dir = tempfile::tempdir().expect("temp dir");
        let runtime = EngineRuntime::open(
            dir.path().join("test.db"),
            ProvenanceMode::Warn,
            None,
            4,
            TelemetryLevel::Counters,
            None,
        )
        .expect("open");

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
                    content_ref: None,
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
                    content_hash: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("seed write");

        (dir, runtime)
    }

    #[test]
    fn telemetry_counts_queries() {
        let (_dir, runtime) = seeded_runtime();
        let compiled = QueryBuilder::nodes("Test")
            .limit(10)
            .compile()
            .expect("compile");

        for _ in 0..3 {
            runtime
                .coordinator()
                .execute_compiled_read(&compiled)
                .expect("query");
        }

        let snap = runtime.telemetry_snapshot();
        assert!(
            snap.queries_total >= 3,
            "expected at least 3 queries, got {}",
            snap.queries_total,
        );
    }

    #[test]
    fn telemetry_counts_writes() {
        let (_dir, runtime) = seeded_runtime();

        // seeded_runtime already submitted 1 write
        runtime
            .writer()
            .submit(WriteRequest {
                label: "second".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "r2".to_owned(),
                    logical_id: "t:2".to_owned(),
                    kind: "Test".to_owned(),
                    properties: r#"{"v":2}"#.to_owned(),
                    source_ref: Some("test".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
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
            .expect("second write");

        let snap = runtime.telemetry_snapshot();
        assert!(
            snap.writes_total >= 2,
            "expected at least 2 writes, got {}",
            snap.writes_total,
        );
    }

    #[test]
    fn telemetry_counts_write_rows() {
        let (_dir, runtime) = seeded_runtime();
        // The seed write has 1 node + 1 chunk = 2 rows
        let snap = runtime.telemetry_snapshot();
        assert!(
            snap.write_rows_total >= 2,
            "expected at least 2 write rows, got {}",
            snap.write_rows_total,
        );
    }

    #[test]
    fn telemetry_snapshot_includes_cache_status() {
        let (_dir, runtime) = seeded_runtime();
        let compiled = QueryBuilder::nodes("Test")
            .limit(10)
            .compile()
            .expect("compile");

        // Run several queries to exercise the page cache.
        for _ in 0..5 {
            runtime
                .coordinator()
                .execute_compiled_read(&compiled)
                .expect("query");
        }

        let snap = runtime.telemetry_snapshot();
        assert!(
            snap.sqlite_cache.cache_hits + snap.sqlite_cache.cache_misses > 0,
            "expected cache activity, got hits={} misses={}",
            snap.sqlite_cache.cache_hits,
            snap.sqlite_cache.cache_misses,
        );
    }
}
