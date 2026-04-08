#![allow(clippy::expect_used)]

mod helpers;

use fathomdb::{
    ActionInsert, ChunkInsert, ChunkPolicy, EdgeInsert, EdgeRetire, Engine, EngineOptions,
    NodeInsert, NodeRetire, ProjectionTarget, RunInsert, StepInsert, TraverseDirection,
    WriteRequest, new_row_id,
};
use rusqlite::Connection;
use tempfile::NamedTempFile;

#[test]
fn engine_bootstraps_and_compiles_queries() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    let compiled = engine
        .query("Meeting")
        .text_search("budget", 5)
        .traverse(TraverseDirection::Out, "HAS_TASK", 2)
        .limit(5)
        .compile()
        .expect("query compiles");

    assert!(compiled.sql.contains("WITH RECURSIVE"));
}

#[test]
fn coordinator_executes_compiled_text_query_and_decodes_rows() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request(r#"{"status":"active"}"#))
        .expect("write completes");

    let compiled = engine
        .query("Meeting")
        .text_search("budget", 5)
        .limit(5)
        .compile()
        .expect("query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read executes");

    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].row_id, "row-1");
    assert_eq!(rows.nodes[0].logical_id, "meeting-1");
    assert_eq!(rows.nodes[0].kind, "Meeting");
    assert_eq!(rows.nodes[0].properties, r#"{"status":"active"}"#);
}

#[test]
fn writer_and_admin_surface_are_wired() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request("{}"))
        .expect("write completes");

    let integrity = engine
        .admin()
        .service()
        .check_integrity()
        .expect("integrity report");
    assert!(integrity.physical_ok);

    let repair = engine
        .admin()
        .service()
        .rebuild_projections(ProjectionTarget::Fts)
        .expect("projection rebuild");
    assert_eq!(repair.targets, vec![ProjectionTarget::Fts]);
}

#[test]
fn typed_write_request_persists_nodes_chunks_and_derived_fts() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    let write_request = meeting_write_request(r#"{"status":"active"}"#);

    let receipt = engine
        .writer()
        .submit(write_request)
        .expect("write completes");
    assert_eq!(receipt.label, "seed");
    assert_eq!(receipt.optional_backfill_count, 0);

    let compiled = engine
        .query("Meeting")
        .text_search("budget", 5)
        .limit(5)
        .compile()
        .expect("query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read executes");

    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "meeting-1");
    assert_eq!(rows.nodes[0].properties, r#"{"status":"active"}"#);

    let integrity = engine
        .admin()
        .service()
        .check_integrity()
        .expect("integrity report");
    assert_eq!(integrity.missing_fts_rows, 0);
}

#[test]
fn trace_report_includes_logical_ids() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request(r#"{"status":"active"}"#))
        .expect("write completes");

    let report = engine
        .admin()
        .service()
        .trace_source("source-1")
        .expect("trace");

    assert_eq!(report.node_rows, 1);
    assert_eq!(report.node_logical_ids, vec!["meeting-1"]);
}

#[test]
fn engine_restore_logical_id_reactivates_retired_object() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request(r#"{"status":"active"}"#))
        .expect("seed write");
    engine
        .writer()
        .submit(WriteRequest {
            label: "retire".to_owned(),
            nodes: vec![],
            node_retires: vec![NodeRetire {
                logical_id: "meeting-1".to_owned(),
                source_ref: Some("forget-1".to_owned()),
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

    let report = engine
        .restore_logical_id("meeting-1")
        .expect("restore logical id");
    assert_eq!(report.logical_id, "meeting-1");
    assert_eq!(report.restored_node_rows, 1);
    assert_eq!(report.restored_chunk_rows, 1);

    let compiled = engine
        .query("Meeting")
        .text_search("budget", 5)
        .limit(5)
        .compile()
        .expect("query compiles");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read executes");
    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "meeting-1");
}

#[test]
fn engine_purge_logical_id_removes_retired_object() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request(r#"{"status":"active"}"#))
        .expect("seed write");
    engine
        .writer()
        .submit(WriteRequest {
            label: "retire".to_owned(),
            nodes: vec![],
            node_retires: vec![NodeRetire {
                logical_id: "meeting-1".to_owned(),
                source_ref: Some("forget-1".to_owned()),
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

    let report = engine
        .purge_logical_id("meeting-1")
        .expect("purge logical id");
    assert_eq!(report.logical_id, "meeting-1");
    assert_eq!(report.deleted_node_rows, 1);
    assert_eq!(report.deleted_chunk_rows, 1);

    let compiled = engine
        .query("Meeting")
        .text_search("budget", 5)
        .limit(5)
        .compile()
        .expect("query compiles");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read executes");
    assert!(rows.nodes.is_empty());
}

#[test]
fn excise_single_version_cleans_fts() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request(r#"{"status":"active"}"#))
        .expect("write completes");

    let before = engine
        .admin()
        .service()
        .check_integrity()
        .expect("pre-excise integrity");
    assert_eq!(before.missing_fts_rows, 0);

    engine
        .admin()
        .service()
        .excise_source("source-1")
        .expect("excise");

    let after = engine
        .admin()
        .service()
        .check_integrity()
        .expect("post-excise integrity");
    assert_eq!(
        after.missing_fts_rows, 0,
        "FTS should be clean after excise"
    );
}

#[test]
fn upsert_write_promotes_new_version_and_read_returns_it() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request(r#"{"version":1}"#))
        .expect("v1 write");

    engine
        .writer()
        .submit(WriteRequest {
            label: "v2".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-2".to_owned(),
                logical_id: "meeting-1".to_owned(),
                kind: "Meeting".to_owned(),
                properties: r#"{"version":2}"#.to_owned(),
                source_ref: Some("source-2".to_owned()),
                upsert: true,
                chunk_policy: ChunkPolicy::Preserve,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "chunk-2".to_owned(),
                node_logical_id: "meeting-1".to_owned(),
                text_content: "second version discussion".to_owned(),
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
        .expect("v2 upsert write");

    let compiled = engine
        .query("Meeting")
        .filter_logical_id_eq("meeting-1")
        .compile()
        .expect("query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read executes");

    assert_eq!(rows.nodes.len(), 1);
    assert!(
        rows.nodes[0].properties.contains("\"version\":2"),
        "read should return the upserted v2 row"
    );
}

#[test]
fn runtime_table_write_is_traced_by_source_ref() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(WriteRequest {
            label: "session".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-1".to_owned(),
                logical_id: "meeting-1".to_owned(),
                kind: "Meeting".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("source-1".to_owned()),
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
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("source-1".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            steps: vec![StepInsert {
                id: "step-1".to_owned(),
                run_id: "run-1".to_owned(),
                kind: "llm".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("source-1".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            actions: vec![ActionInsert {
                id: "action-1".to_owned(),
                step_id: "step-1".to_owned(),
                kind: "emit".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("source-1".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("write completes");

    let report = engine
        .admin()
        .service()
        .trace_source("source-1")
        .expect("trace");

    assert_eq!(report.node_rows, 1);
    assert_eq!(report.action_rows, 1);
    assert_eq!(report.node_logical_ids, vec!["meeting-1"]);
    assert_eq!(report.action_ids, vec!["action-1"]);
}

#[test]
fn traversal_query_returns_connected_node_via_typed_writes() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(WriteRequest {
            label: "graph".to_owned(),
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
        .expect("write nodes and edge");

    let compiled = engine
        .query("Meeting")
        .traverse(TraverseDirection::Out, "HAS_TASK", 1)
        .compile()
        .expect("traversal query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("traversal executes");

    let logical_ids: Vec<&str> = rows.nodes.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(
        logical_ids.contains(&"task-1"),
        "traversal must return the connected task node; got: {logical_ids:?}"
    );
}

// ── Layer 1: Physical Storage ────────────────────────────────────────────────

#[test]
fn schema_version_persists_across_reopen() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    drop(engine);

    let versions_first: Vec<i64> = {
        let conn = Connection::open(db.path()).expect("open db");
        let mut stmt = conn
            .prepare("SELECT version FROM fathom_schema_migrations ORDER BY applied_at")
            .expect("prepare");
        stmt.query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect()
    };

    // Reopen — must not re-apply migrations.
    let _engine2 = Engine::open(EngineOptions::new(db.path())).expect("reopen");

    let versions_second: Vec<i64> = {
        let conn = Connection::open(db.path()).expect("open db");
        let mut stmt = conn
            .prepare("SELECT version FROM fathom_schema_migrations ORDER BY applied_at")
            .expect("prepare");
        stmt.query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect()
    };

    assert!(
        !versions_first.is_empty(),
        "at least one migration must have been applied"
    );
    assert_eq!(
        versions_first, versions_second,
        "reopen must not add or reorder migration rows"
    );
}

#[test]
fn migration_ordering_is_deterministic() {
    let db = NamedTempFile::new().expect("temporary db");
    let _engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    let versions: Vec<i64> = {
        let conn = Connection::open(db.path()).expect("open db");
        let mut stmt = conn
            .prepare("SELECT version FROM fathom_schema_migrations ORDER BY applied_at")
            .expect("prepare");
        stmt.query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect()
    };

    assert!(!versions.is_empty(), "at least one migration applied");
    let mut sorted = versions.clone();
    sorted.sort_unstable();
    assert_eq!(
        versions, sorted,
        "migration versions must be applied in ascending order"
    );
}

#[test]
fn startup_pragma_journal_mode_is_wal() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    assert_eq!(
        engine
            .coordinator()
            .raw_pragma("journal_mode")
            .expect("pragma"),
        "wal"
    );
}

#[test]
fn startup_pragma_foreign_keys_is_on() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    assert_eq!(
        engine
            .coordinator()
            .raw_pragma("foreign_keys")
            .expect("pragma"),
        "1"
    );
}

#[test]
fn startup_pragma_busy_timeout_is_set() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    let timeout: i64 = engine
        .coordinator()
        .raw_pragma("busy_timeout")
        .expect("pragma")
        .parse()
        .expect("integer");
    assert!(
        timeout >= 5000,
        "busy_timeout must be at least 5000 ms, got {timeout}"
    );
}

#[test]
fn startup_pragma_synchronous_is_not_full() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    // SQLite reports 0=OFF 1=NORMAL 2=FULL 3=EXTRA; we require not FULL.
    assert_ne!(
        engine
            .coordinator()
            .raw_pragma("synchronous")
            .expect("pragma"),
        "2",
        "synchronous must not be FULL"
    );
}

#[test]
fn wal_checkpoint_does_not_lose_committed_data() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    for i in 0..5_i32 {
        engine
            .writer()
            .submit(WriteRequest {
                label: format!("wal-{i}"),
                nodes: vec![NodeInsert {
                    row_id: format!("wal-row-{i}"),
                    logical_id: format!("wal-node-{i}"),
                    kind: "Doc".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some(format!("wal-src-{i}")),
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
    }

    helpers::exec_sql(db.path(), "PRAGMA wal_checkpoint(FULL)");
    drop(engine);

    let engine2 = Engine::open(EngineOptions::new(db.path())).expect("reopen");
    let compiled = engine2.query("Doc").compile().expect("compiles");
    let rows = engine2
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read");
    assert_eq!(
        rows.nodes.len(),
        5,
        "all 5 nodes must survive WAL checkpoint + reopen"
    );
}

#[test]
fn wal_mode_allows_concurrent_readers() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request("{}"))
        .expect("initial write");

    // Open a second raw connection and hold an open read transaction.
    let reader = Connection::open(db.path()).expect("reader connection");
    reader.execute_batch("BEGIN").expect("begin read tx");

    // A write must succeed even with an active reader (WAL allows this).
    let result = engine.writer().submit(WriteRequest {
        label: "concurrent".to_owned(),
        nodes: vec![NodeInsert {
            row_id: "wal-concurrent-row".to_owned(),
            logical_id: "wal-concurrent".to_owned(),
            kind: "Meeting".to_owned(),
            properties: "{}".to_owned(),
            source_ref: Some("src-wal".to_owned()),
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

    reader.execute_batch("COMMIT").expect("commit read tx");
    result.expect("write must succeed while reader holds open transaction");
}

// ── Layer 2: Engine Invariants ────────────────────────────────────────────────

#[test]
fn node_insert_writes_all_fields_to_nodes_table() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(WriteRequest {
            label: "field-check".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "exact-row-id".to_owned(),
                logical_id: "exact-logical-id".to_owned(),
                kind: "ExactKind".to_owned(),
                properties: r#"{"x":1}"#.to_owned(),
                source_ref: Some("exact-source".to_owned()),
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

    let f = helpers::node_fields(db.path(), "exact-logical-id");
    assert_eq!(f.row_id, "exact-row-id");
    assert_eq!(f.logical_id, "exact-logical-id");
    assert_eq!(f.kind, "ExactKind");
    assert_eq!(f.properties, r#"{"x":1}"#);
    assert_eq!(f.source_ref.as_deref(), Some("exact-source"));
    assert!(
        f.created_at > 0,
        "created_at must be a non-zero unix timestamp"
    );
    assert!(
        f.superseded_at.is_none(),
        "newly inserted node must have superseded_at = NULL"
    );
}

#[test]
fn chunk_insert_writes_to_chunks_table() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(WriteRequest {
            label: "chunk-field-check".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "node-row".to_owned(),
                logical_id: "node-lg".to_owned(),
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
                id: "exact-chunk-id".to_owned(),
                node_logical_id: "node-lg".to_owned(),
                text_content: "exact chunk text".to_owned(),
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

    let f = helpers::chunk_fields(db.path(), "exact-chunk-id");
    assert_eq!(f.id, "exact-chunk-id");
    assert_eq!(f.node_logical_id, "node-lg");
    assert_eq!(f.text_content, "exact chunk text");
    assert!(
        f.created_at > 0,
        "created_at must be a non-zero unix timestamp"
    );
}

#[test]
fn chunk_policy_replace_is_atomic() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    // Write original node + chunk A.
    engine
        .writer()
        .submit(WriteRequest {
            label: "v1".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "replace-row-1".to_owned(),
                logical_id: "replace-node".to_owned(),
                kind: "Doc".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("src-v1".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "chunk-a".to_owned(),
                node_logical_id: "replace-node".to_owned(),
                text_content: "original text".to_owned(),
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

    // Upsert with ChunkPolicy::Replace — atomically swaps chunk A for chunk B.
    engine
        .writer()
        .submit(WriteRequest {
            label: "v2".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "replace-row-2".to_owned(),
                logical_id: "replace-node".to_owned(),
                kind: "Doc".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("src-v2".to_owned()),
                upsert: true,
                chunk_policy: ChunkPolicy::Replace,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "chunk-b".to_owned(),
                node_logical_id: "replace-node".to_owned(),
                text_content: "replacement text".to_owned(),
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
        .expect("v2 replace write");

    // Verify atomic swap: A gone, B present, exactly one FTS row.
    assert_eq!(
        helpers::chunk_count(db.path(), "replace-node"),
        1,
        "exactly one chunk must remain after Replace"
    );
    let b = helpers::chunk_fields(db.path(), "chunk-b");
    assert_eq!(b.text_content, "replacement text");
    assert_eq!(
        helpers::fts_row_count(db.path(), "replace-node"),
        1,
        "FTS must reflect new chunk only"
    );
}

#[test]
fn execute_compiled_read_returns_empty_for_no_match() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request("{}"))
        .expect("write Meeting");

    let compiled = engine.query("Task").compile().expect("query compiles");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read");
    assert!(
        rows.nodes.is_empty(),
        "query for Task must return no rows when only Meeting nodes exist"
    );
}

#[test]
fn execute_compiled_read_only_returns_active_rows() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request(r#"{"v":1}"#))
        .expect("v1 write");

    engine
        .writer()
        .submit(WriteRequest {
            label: "v2".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-2".to_owned(),
                logical_id: "meeting-1".to_owned(),
                kind: "Meeting".to_owned(),
                properties: r#"{"v":2}"#.to_owned(),
                source_ref: Some("source-2".to_owned()),
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
        .expect("v2 upsert");

    let compiled = engine
        .query("Meeting")
        .filter_logical_id_eq("meeting-1")
        .compile()
        .expect("compiles");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read");

    assert_eq!(
        rows.nodes.len(),
        1,
        "only the active (v2) row must be returned"
    );
    assert_eq!(rows.nodes[0].row_id, "row-2");
}

#[test]
fn traversal_does_not_follow_retired_edges() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    // Use distinct kinds so that "Sink" node is only reachable via the edge,
    // not as a query seed.
    engine
        .writer()
        .submit(WriteRequest {
            label: "setup".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "row-src".to_owned(),
                    logical_id: "node-src".to_owned(),
                    kind: "Source".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
                NodeInsert {
                    row_id: "row-sink".to_owned(),
                    logical_id: "node-sink".to_owned(),
                    kind: "Sink".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
            ],
            node_retires: vec![],
            edges: vec![EdgeInsert {
                row_id: "edge-row-1".to_owned(),
                logical_id: "edge-lg-1".to_owned(),
                source_logical_id: "node-src".to_owned(),
                target_logical_id: "node-sink".to_owned(),
                kind: "FLOW".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("src".to_owned()),
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
        .expect("setup write");

    engine
        .writer()
        .submit(WriteRequest {
            label: "retire-edge".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![EdgeRetire {
                logical_id: "edge-lg-1".to_owned(),
                source_ref: Some("src".to_owned()),
            }],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("retire edge");

    // Query seeds only on "Source" kind; Sink is only reachable via the retired edge.
    let compiled = engine
        .query("Source")
        .traverse(TraverseDirection::Out, "FLOW", 1)
        .compile()
        .expect("compiles");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read");

    let ids: Vec<&str> = rows.nodes.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(
        !ids.contains(&"node-sink"),
        "traversal must not follow a retired edge; got: {ids:?}"
    );
}

#[test]
fn traversal_follows_logical_id_through_superseded_node() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    // Use distinct kinds: "Root" seeds the traversal; "Leaf" is only reachable via edge.
    engine
        .writer()
        .submit(WriteRequest {
            label: "setup".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "row-root-v1".to_owned(),
                    logical_id: "node-root".to_owned(),
                    kind: "Root".to_owned(),
                    properties: r#"{"v":1}"#.to_owned(),
                    source_ref: Some("src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
                NodeInsert {
                    row_id: "row-leaf".to_owned(),
                    logical_id: "node-leaf".to_owned(),
                    kind: "Leaf".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
            ],
            node_retires: vec![],
            edges: vec![EdgeInsert {
                row_id: "edge-row-1".to_owned(),
                logical_id: "edge-lg-1".to_owned(),
                source_logical_id: "node-root".to_owned(),
                target_logical_id: "node-leaf".to_owned(),
                kind: "BRANCH".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("src".to_owned()),
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
        .expect("setup write");

    // Supersede node-root → v2.
    engine
        .writer()
        .submit(WriteRequest {
            label: "upsert-root".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-root-v2".to_owned(),
                logical_id: "node-root".to_owned(),
                kind: "Root".to_owned(),
                properties: r#"{"v":2}"#.to_owned(),
                source_ref: Some("src2".to_owned()),
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
        .expect("supersede root");

    // Query seeds on "Root" and traverses BRANCH edges — node-leaf is only reachable via edge.
    let compiled = engine
        .query("Root")
        .traverse(TraverseDirection::Out, "BRANCH", 1)
        .compile()
        .expect("compiles");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read");

    let ids: Vec<&str> = rows.nodes.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(
        ids.contains(&"node-leaf"),
        "traversal must still reach node-leaf after node-root is superseded; got: {ids:?}"
    );

    // The active version of node-root in results must be v2.
    if let Some(r) = rows.nodes.iter().find(|n| n.logical_id == "node-root") {
        assert_eq!(
            r.row_id, "row-root-v2",
            "traversal must return the active (v2) row for node-root"
        );
    }
}

#[test]
fn new_row_id_is_valid_as_node_insert_row_id() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    let generated = new_row_id();
    assert!(
        !generated.is_empty(),
        "new_row_id must return a non-empty string"
    );

    engine
        .writer()
        .submit(WriteRequest {
            label: "id-gen-test".to_owned(),
            nodes: vec![NodeInsert {
                row_id: generated.clone(),
                logical_id: "id-gen-node".to_owned(),
                kind: "Doc".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("src".to_owned()),
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
        .expect("write with generated row_id must succeed");

    let f = helpers::node_fields(db.path(), "id-gen-node");
    assert_eq!(f.row_id, generated);
}

#[test]
fn retire_node_leaves_dangling_edge_detected_by_check_semantics() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    // Write node-A, node-B, and an edge A→B.
    engine
        .writer()
        .submit(WriteRequest {
            label: "setup".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "row-A".to_owned(),
                    logical_id: "node-A".to_owned(),
                    kind: "Source".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
                NodeInsert {
                    row_id: "row-B".to_owned(),
                    logical_id: "node-B".to_owned(),
                    kind: "Target".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
            ],
            node_retires: vec![],
            edges: vec![EdgeInsert {
                row_id: "edge-row-1".to_owned(),
                logical_id: "edge-A-B".to_owned(),
                source_logical_id: "node-A".to_owned(),
                target_logical_id: "node-B".to_owned(),
                kind: "LINKS_TO".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("src".to_owned()),
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
        .expect("setup write");

    // Retire node-A — edge A→B now dangles.
    engine
        .writer()
        .submit(WriteRequest {
            label: "retire-A".to_owned(),
            nodes: vec![],
            node_retires: vec![NodeRetire {
                logical_id: "node-A".to_owned(),
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

    let report = engine
        .admin()
        .service()
        .check_semantics()
        .expect("semantics check");

    assert!(
        report.dangling_edges >= 1,
        "retiring node-A must leave a dangling edge; got dangling_edges={}",
        report.dangling_edges
    );
    assert!(
        report.warnings.iter().any(|w| w.contains("edge")),
        "warnings must mention dangling edges; got: {:?}",
        report.warnings
    );
}

#[test]
fn retire_only_version_reports_orphaned_supersession_chain() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request(r#"{"version":1}"#))
        .expect("seed write");

    engine
        .writer()
        .submit(WriteRequest {
            label: "retire".to_owned(),
            nodes: vec![],
            node_retires: vec![NodeRetire {
                logical_id: "meeting-1".to_owned(),
                source_ref: Some("source-retire".to_owned()),
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

    let report = engine
        .admin()
        .service()
        .check_semantics()
        .expect("semantics check");
    assert!(
        report.orphaned_supersession_chains >= 1,
        "retiring the only version should surface an orphaned supersession chain"
    );
}

#[test]
fn excise_source_is_idempotent() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request(r#"{"status":"active"}"#))
        .expect("seed write");

    let first = engine
        .admin()
        .service()
        .excise_source("source-1")
        .expect("first excise");
    let historical_after_first = helpers::historical_count(db.path(), "nodes", "meeting-1");
    let second = engine
        .admin()
        .service()
        .excise_source("source-1")
        .expect("second excise");
    let historical_after_second = helpers::historical_count(db.path(), "nodes", "meeting-1");

    assert_eq!(first.node_rows, 1, "first excise must supersede one row");
    assert_eq!(
        second.node_rows, first.node_rows,
        "trace counts are source-scoped totals and remain stable"
    );
    assert_eq!(helpers::active_count(db.path(), "nodes", "meeting-1"), 0);
    assert_eq!(
        historical_after_second, historical_after_first,
        "second excise must not mutate supersession state"
    );
}

#[test]
fn excise_source_does_not_affect_other_sources() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "row-src-a".to_owned(),
                    logical_id: "node-src-a".to_owned(),
                    kind: "Doc".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("source-a".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
                NodeInsert {
                    row_id: "row-src-b".to_owned(),
                    logical_id: "node-src-b".to_owned(),
                    kind: "Doc".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("source-b".to_owned()),
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
        .expect("seed write");

    engine
        .admin()
        .service()
        .excise_source("source-a")
        .expect("excise source-a");

    assert_eq!(helpers::active_count(db.path(), "nodes", "node-src-a"), 0);
    assert_eq!(helpers::active_count(db.path(), "nodes", "node-src-b"), 1);
}

#[test]
fn retire_node_records_provenance_event() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    // Insert a node.
    engine
        .writer()
        .submit(WriteRequest {
            label: "setup".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "row-prov-1".to_owned(),
                logical_id: "prov-node-1".to_owned(),
                kind: "Doc".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("src-prov".to_owned()),
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
        .expect("setup write");

    // Retire the node.
    engine
        .writer()
        .submit(WriteRequest {
            label: "retire".to_owned(),
            nodes: vec![],
            node_retires: vec![NodeRetire {
                logical_id: "prov-node-1".to_owned(),
                source_ref: Some("src-retire".to_owned()),
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

    let events = engine
        .coordinator()
        .query_provenance_events("prov-node-1")
        .expect("query provenance events");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "node_retire");
    assert_eq!(events[0].subject, "prov-node-1");
}

#[test]
fn excise_source_records_provenance_event() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    engine
        .writer()
        .submit(meeting_write_request(r#"{"v":1}"#))
        .expect("setup write");

    engine
        .admin()
        .service()
        .excise_source("source-1")
        .expect("excise");

    let events = engine
        .coordinator()
        .query_provenance_events("source-1")
        .expect("query provenance events");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "excise_source");
    assert_eq!(events[0].subject, "source-1");
}

#[test]
fn provenance_events_are_isolated_per_subject() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    // Insert two nodes.
    engine
        .writer()
        .submit(WriteRequest {
            label: "setup".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "row-iso-A".to_owned(),
                    logical_id: "iso-node-A".to_owned(),
                    kind: "Doc".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                },
                NodeInsert {
                    row_id: "row-iso-B".to_owned(),
                    logical_id: "iso-node-B".to_owned(),
                    kind: "Doc".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src".to_owned()),
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
        .expect("setup write");

    // Retire both nodes in one request.
    engine
        .writer()
        .submit(WriteRequest {
            label: "retire-both".to_owned(),
            nodes: vec![],
            node_retires: vec![
                NodeRetire {
                    logical_id: "iso-node-A".to_owned(),
                    source_ref: Some("src".to_owned()),
                },
                NodeRetire {
                    logical_id: "iso-node-B".to_owned(),
                    source_ref: Some("src".to_owned()),
                },
            ],
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

    let events_a = engine
        .coordinator()
        .query_provenance_events("iso-node-A")
        .expect("query A");
    let events_b = engine
        .coordinator()
        .query_provenance_events("iso-node-B")
        .expect("query B");

    assert_eq!(events_a.len(), 1, "node-A must have exactly one event");
    assert_eq!(events_b.len(), 1, "node-B must have exactly one event");
    assert_eq!(events_a[0].subject, "iso-node-A");
    assert_eq!(events_b[0].subject, "iso-node-B");
}

fn meeting_write_request(properties: &str) -> WriteRequest {
    WriteRequest {
        label: "seed".to_owned(),
        nodes: vec![NodeInsert {
            row_id: "row-1".to_owned(),
            logical_id: "meeting-1".to_owned(),
            kind: "Meeting".to_owned(),
            properties: properties.to_owned(),
            source_ref: Some("source-1".to_owned()),
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
    }
}

#[test]
fn engine_rejects_zero_pool_size() {
    let db = NamedTempFile::new().expect("temporary db");
    let mut opts = EngineOptions::new(db.path());
    opts.read_pool_size = Some(0);
    let result = Engine::open(opts);
    assert!(result.is_err(), "pool_size=0 should return Err, not panic");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("read_pool_size"),
        "error should mention read_pool_size, got: {err}"
    );
}
