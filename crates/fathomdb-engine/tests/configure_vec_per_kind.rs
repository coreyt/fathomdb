//! Pack C: per-kind `configure_vec_kind` + `get_vec_index_status` admin API.
#![allow(clippy::expect_used, clippy::panic)]

use fathomdb_engine::{
    ChunkInsert, ChunkPolicy, EngineRuntime, NodeInsert, ProvenanceMode, TelemetryLevel,
    VectorSource, WriteRequest,
};

fn open_engine(dir: &tempfile::TempDir) -> EngineRuntime {
    EngineRuntime::open(
        dir.path().join("test.db"),
        ProvenanceMode::Warn,
        None,
        2,
        TelemetryLevel::Counters,
        None,
    )
    .expect("open engine")
}

fn make_write_request(
    label: &str,
    nodes: Vec<NodeInsert>,
    chunks: Vec<ChunkInsert>,
) -> WriteRequest {
    WriteRequest {
        label: label.to_owned(),
        nodes,
        node_retires: vec![],
        edges: vec![],
        edge_retires: vec![],
        chunks,
        runs: vec![],
        steps: vec![],
        actions: vec![],
        optional_backfills: vec![],
        vec_inserts: vec![],
        operational_writes: vec![],
    }
}

fn seed_active_profile(db_path: &std::path::Path, dimensions: i64) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("open raw");
    conn.execute(
        "INSERT INTO vector_embedding_profiles \
         (profile_name, model_identity, model_version, dimensions, normalization_policy, max_tokens, active, activated_at, created_at) \
         VALUES ('test-profile', 'test/model', 'v1', ?1, 'l2', 512, 1, unixepoch(), unixepoch())",
        rusqlite::params![dimensions],
    )
    .expect("seed profile");
    conn.last_insert_rowid()
}

fn seed_kind_with_chunks(engine: &EngineRuntime, kind: &str, count: u32) {
    for i in 0..count {
        let logical_id = format!("{kind}:{i}");
        let chunk_id = format!("chunk-{kind}-{i}");
        engine
            .writer()
            .submit(make_write_request(
                &format!("seed-{kind}-{i}"),
                vec![NodeInsert {
                    row_id: format!("row-{kind}-{i}"),
                    logical_id: logical_id.clone(),
                    kind: kind.to_owned(),
                    properties: format!(r#"{{"index":{i}}}"#),
                    source_ref: Some("test".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                }],
                vec![ChunkInsert {
                    id: chunk_id,
                    node_logical_id: logical_id,
                    text_content: format!("chunk body {i}"),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                }],
            ))
            .expect("write node+chunk");
    }
}

#[test]
fn test_configure_vec_requires_active_embedding_profile() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();

    let err = svc
        .configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect_err("must error without active profile");
    let msg = format!("{err}");
    assert!(
        msg.contains("embedding") || msg.contains("profile"),
        "unexpected error: {msg}"
    );
}

#[test]
fn test_configure_vec_creates_per_kind_table_and_backfill_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let profile_id = seed_active_profile(&db_path, 384);
    seed_kind_with_chunks(&engine, "KnowledgeItem", 3);

    let svc = engine.admin().service();
    let outcome = svc
        .configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure_vec_kind");

    assert_eq!(outcome.kind, "KnowledgeItem");
    assert_eq!(outcome.enqueued_backfill_rows, 3);
    assert!(!outcome.was_already_enabled);

    // Per-kind vec table exists.
    let conn = rusqlite::Connection::open(&db_path).expect("reopen");
    let table = fathomdb_schema::vec_kind_table_name("KnowledgeItem");
    assert!(
        table.starts_with("vec_knowledgeitem_"),
        "expected per-kind vec table to be prefixed with sanitized kind slug, got {table}"
    );
    let exists: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = ?1",
            rusqlite::params![&table],
            |r| r.get(0),
        )
        .expect("query sqlite_master");
    assert_eq!(exists, 1, "expected {table} to exist");

    // Work rows: 3, priority < 1000, pending, correct profile id.
    let (count, max_priority): (i64, i64) = conn
        .query_row(
            "SELECT count(*), COALESCE(MAX(priority), 0) FROM vector_projection_work \
             WHERE kind = 'KnowledgeItem' AND state = 'pending' AND embedding_profile_id = ?1",
            rusqlite::params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("query work");
    assert_eq!(count, 3);
    assert!(max_priority < 1000, "backfill priority must be < 1000");
}

#[test]
fn test_configure_vec_is_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");
    let _profile_id = seed_active_profile(&db_path, 384);
    seed_kind_with_chunks(&engine, "KnowledgeItem", 3);
    let svc = engine.admin().service();

    let first = svc
        .configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure first");
    assert_eq!(first.enqueued_backfill_rows, 3);
    assert!(!first.was_already_enabled);

    let second = svc
        .configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure second");
    assert_eq!(second.enqueued_backfill_rows, 0);
    assert!(second.was_already_enabled);

    let conn = rusqlite::Connection::open(&db_path).expect("reopen");
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM vector_projection_work WHERE kind = 'KnowledgeItem' AND state = 'pending'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(count, 3, "work row count must stay at 3 after reconfigure");
}

#[test]
fn test_get_vec_index_status_unconfigured_kind() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();

    let status = svc
        .get_vec_index_status("Unknown")
        .expect("status unconfigured");
    assert_eq!(status.kind, "Unknown");
    assert!(!status.enabled);
    assert_eq!(status.state, "unconfigured");
    assert_eq!(status.pending_backfill, 0);
    assert_eq!(status.pending_incremental, 0);
    assert!(status.last_error.is_none());
}

#[test]
fn test_get_vec_index_status_after_configure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");
    let _profile_id = seed_active_profile(&db_path, 384);
    seed_kind_with_chunks(&engine, "KnowledgeItem", 3);
    let svc = engine.admin().service();

    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure");

    let status = svc
        .get_vec_index_status("KnowledgeItem")
        .expect("status after configure");
    assert!(status.enabled);
    assert_eq!(status.pending_backfill, 3);
    assert_eq!(status.pending_incremental, 0);
    assert_eq!(status.embedding_identity.as_deref(), Some("test/model"));
}
