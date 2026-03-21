use fathomdb::{Engine, EngineOptions, ProjectionTarget, TraverseDirection, WriteRequest};
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
fn writer_and_admin_surface_are_wired() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    let write_request = WriteRequest {
        label: "seed".to_owned(),
        canonical_statements: vec![
            r#"
            INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
            VALUES ('row-1', 'meeting-1', 'Meeting', '{}', unixepoch(), 'source-1')
            "#
            .to_owned(),
            r#"
            INSERT INTO chunks (id, node_logical_id, text_content, created_at)
            VALUES ('chunk-1', 'meeting-1', 'budget discussion', unixepoch())
            "#
            .to_owned(),
        ],
        required_projection_statements: vec![r#"
            INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
            VALUES ('chunk-1', 'meeting-1', 'Meeting', 'budget discussion')
        "#
        .to_owned()],
        optional_backfills: vec![],
    };

    engine.writer().submit(write_request).expect("write completes");

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
