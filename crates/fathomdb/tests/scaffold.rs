#![allow(clippy::expect_used)]

use fathomdb::{
    ActionInsert, ChunkInsert, ChunkPolicy, EdgeInsert, Engine, EngineOptions, NodeInsert,
    ProjectionTarget, RunInsert, StepInsert, TraverseDirection, WriteRequest,
};
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

    let receipt = engine.writer().submit(write_request).expect("write completes");
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
    assert_eq!(after.missing_fts_rows, 0, "FTS should be clean after excise");
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
    }
}
