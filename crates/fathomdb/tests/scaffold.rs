use fathomdb::{
    ChunkInsert, Engine, EngineOptions, NodeInsert, ProjectionTarget, TraverseDirection,
    WriteRequest,
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

    let dispatched = engine
        .coordinator()
        .dispatch_compiled_read(&compiled)
        .expect("read dispatched");

    assert!(dispatched.sql.contains("WITH RECURSIVE"));
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

fn meeting_write_request(properties: &str) -> WriteRequest {
    WriteRequest {
        label: "seed".to_owned(),
        nodes: vec![NodeInsert {
            row_id: "row-1".to_owned(),
            logical_id: "meeting-1".to_owned(),
            kind: "Meeting".to_owned(),
            properties: properties.to_owned(),
            source_ref: Some("source-1".to_owned()),
        }],
        chunks: vec![ChunkInsert {
            id: "chunk-1".to_owned(),
            node_logical_id: "meeting-1".to_owned(),
            text_content: "budget discussion".to_owned(),
            byte_start: None,
            byte_end: None,
        }],
        optional_backfills: vec![],
    }
}
