#![allow(clippy::expect_used)]

mod helpers;

use fathomdb::{
    ChunkInsert, ChunkPolicy, EdgeInsert, Engine, EngineOptions, LastAccessTouchRequest,
    NodeInsert, NodeRetire, ProvenanceMode, TraverseDirection, WriteRequest, new_row_id,
};
use rusqlite::Connection;
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn open_engine_with_provenance(mode: ProvenanceMode) -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let mut options = EngineOptions::new(db.path());
    options.provenance_mode = mode;
    let engine = Engine::open(options).expect("engine opens");
    (db, engine)
}

fn seed_meeting(engine: &Engine) {
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-touch".to_owned(),
            nodes: vec![NodeInsert {
                row_id: new_row_id(),
                logical_id: "meeting-1".to_owned(),
                kind: "Meeting".to_owned(),
                properties: r#"{"title":"Budget review"}"#.to_owned(),
                source_ref: Some("source:meeting".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Replace,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "chunk-meeting-1".to_owned(),
                node_logical_id: "meeting-1".to_owned(),
                text_content: "budget review agenda".to_owned(),
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
}

fn seed_graph(engine: &Engine) {
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-touch-graph".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: r#"{"title":"Budget review"}"#.to_owned(),
                    source_ref: Some("source:meeting".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Replace,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "task-1".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"title":"Draft memo"}"#.to_owned(),
                    source_ref: Some("source:task".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![EdgeInsert {
                row_id: new_row_id(),
                logical_id: "edge-1".to_owned(),
                source_logical_id: "meeting-1".to_owned(),
                target_logical_id: "task-1".to_owned(),
                kind: "HAS_TASK".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("source:edge".to_owned()),
                upsert: false,
            }],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "chunk-meeting-1".to_owned(),
                node_logical_id: "meeting-1".to_owned(),
                text_content: "budget review agenda".to_owned(),
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
        .expect("seed graph");
}

#[test]
fn touch_last_accessed_updates_metadata_without_node_supersession_churn() {
    let (db, engine) = open_engine();
    seed_meeting(&engine);

    let report = engine
        .touch_last_accessed(LastAccessTouchRequest {
            logical_ids: vec!["meeting-1".to_owned(), "meeting-1".to_owned()],
            touched_at: 1_711_843_200,
            source_ref: Some("source:touch".to_owned()),
        })
        .expect("touch succeeds");

    assert_eq!(report.touched_logical_ids, 1);
    assert_eq!(report.touched_at, 1_711_843_200);
    assert_eq!(helpers::count_rows(db.path(), "nodes"), 1);

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
    assert_eq!(rows.nodes[0].last_accessed_at, Some(1_711_843_200));

    let conn = Connection::open(db.path()).expect("open db");
    let provenance_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM provenance_events WHERE event_type = 'node_last_accessed_touched'",
            [],
            |row| row.get(0),
        )
        .expect("provenance count");
    assert_eq!(provenance_count, 1);
}

#[test]
fn touch_last_accessed_rejects_unknown_logical_ids_atomically() {
    let (db, engine) = open_engine();
    seed_meeting(&engine);

    let error = engine
        .touch_last_accessed(LastAccessTouchRequest {
            logical_ids: vec!["meeting-1".to_owned(), "missing".to_owned()],
            touched_at: 1_711_843_200,
            source_ref: Some("source:touch".to_owned()),
        })
        .expect_err("unknown logical_id must fail");

    let message = error.to_string();
    assert!(message.contains("missing"));

    let conn = Connection::open(db.path()).expect("open db");
    let metadata_rows: i64 = conn
        .query_row("SELECT count(*) FROM node_access_metadata", [], |row| {
            row.get(0)
        })
        .expect("count metadata rows");
    assert_eq!(metadata_rows, 0);
}

#[test]
fn touch_last_accessed_is_visible_on_grouped_query_results() {
    let (_db, engine) = open_engine();
    seed_graph(&engine);

    engine
        .touch_last_accessed(LastAccessTouchRequest {
            logical_ids: vec!["meeting-1".to_owned(), "task-1".to_owned()],
            touched_at: 1_711_843_299,
            source_ref: Some("source:touch".to_owned()),
        })
        .expect("touch succeeds");

    let compiled = engine
        .query("Meeting")
        .filter_logical_id_eq("meeting-1")
        .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1)
        .compile_grouped()
        .expect("grouped query compiles");
    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots[0].last_accessed_at, Some(1_711_843_299));
    assert_eq!(
        rows.expansions[0].roots[0].nodes[0].last_accessed_at,
        Some(1_711_843_299)
    );
}

#[test]
fn purge_logical_id_removes_last_access_metadata() {
    let (db, engine) = open_engine();
    seed_meeting(&engine);

    engine
        .touch_last_accessed(LastAccessTouchRequest {
            logical_ids: vec!["meeting-1".to_owned()],
            touched_at: 1_711_843_200,
            source_ref: Some("source:touch".to_owned()),
        })
        .expect("touch succeeds");
    engine
        .writer()
        .submit(WriteRequest {
            label: "retire".to_owned(),
            nodes: vec![],
            node_retires: vec![NodeRetire {
                logical_id: "meeting-1".to_owned(),
                source_ref: Some("source:retire".to_owned()),
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

    engine
        .purge_logical_id("meeting-1")
        .expect("purge logical id");

    let conn = Connection::open(db.path()).expect("open db");
    let metadata_rows: i64 = conn
        .query_row(
            "SELECT count(*) FROM node_access_metadata WHERE logical_id = 'meeting-1'",
            [],
            |row| row.get(0),
        )
        .expect("count metadata rows");
    assert_eq!(metadata_rows, 0);
}

#[test]
fn restore_logical_id_preserves_last_access_metadata() {
    let (_db, engine) = open_engine();
    seed_meeting(&engine);

    engine
        .touch_last_accessed(LastAccessTouchRequest {
            logical_ids: vec!["meeting-1".to_owned()],
            touched_at: 1_711_843_200,
            source_ref: Some("source:touch".to_owned()),
        })
        .expect("touch succeeds");
    engine
        .writer()
        .submit(WriteRequest {
            label: "retire".to_owned(),
            nodes: vec![],
            node_retires: vec![NodeRetire {
                logical_id: "meeting-1".to_owned(),
                source_ref: Some("source:retire".to_owned()),
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

    engine
        .restore_logical_id("meeting-1")
        .expect("restore logical id");

    let compiled = engine
        .query("Meeting")
        .filter_logical_id_eq("meeting-1")
        .compile()
        .expect("query compiles");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read executes");
    assert_eq!(rows.nodes[0].last_accessed_at, Some(1_711_843_200));
}

#[test]
fn touch_last_accessed_survives_engine_reopen() {
    let (db, engine) = open_engine();
    seed_meeting(&engine);

    engine
        .touch_last_accessed(LastAccessTouchRequest {
            logical_ids: vec!["meeting-1".to_owned()],
            touched_at: 1_711_843_245,
            source_ref: Some("source:touch".to_owned()),
        })
        .expect("touch succeeds");
    drop(engine);

    let reopened = Engine::open(EngineOptions::new(db.path())).expect("engine reopens");
    let compiled = reopened
        .query("Meeting")
        .filter_logical_id_eq("meeting-1")
        .compile()
        .expect("query compiles");
    let rows = reopened
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read executes");
    assert_eq!(rows.nodes[0].last_accessed_at, Some(1_711_843_245));
}

#[test]
fn excise_source_removes_last_access_metadata_when_no_active_node_remains() {
    let (db, engine) = open_engine();
    seed_meeting(&engine);

    engine
        .touch_last_accessed(LastAccessTouchRequest {
            logical_ids: vec!["meeting-1".to_owned()],
            touched_at: 1_711_843_200,
            source_ref: Some("source:touch".to_owned()),
        })
        .expect("touch succeeds");

    engine
        .admin()
        .service()
        .excise_source("source:meeting")
        .expect("excise succeeds");

    let conn = Connection::open(db.path()).expect("open db");
    let metadata_rows: i64 = conn
        .query_row(
            "SELECT count(*) FROM node_access_metadata WHERE logical_id = 'meeting-1'",
            [],
            |row| row.get(0),
        )
        .expect("count metadata rows");
    assert_eq!(metadata_rows, 0);
}

#[test]
fn check_semantics_detects_orphaned_last_access_metadata_rows() {
    let (db, engine) = open_engine();
    seed_meeting(&engine);

    let conn = Connection::open(db.path()).expect("open db");
    conn.execute(
        "INSERT INTO node_access_metadata (logical_id, last_accessed_at, updated_at) VALUES ('missing', 10, 10)",
        [],
    )
    .expect("insert orphaned metadata");

    let report = engine
        .admin()
        .service()
        .check_semantics()
        .expect("semantics");
    assert_eq!(report.orphaned_last_access_metadata_rows, 1);
}

#[test]
fn touch_last_accessed_requires_source_ref_when_provenance_is_required() {
    let (_db, engine) = open_engine_with_provenance(ProvenanceMode::Require);
    seed_meeting(&engine);

    let error = engine
        .touch_last_accessed(LastAccessTouchRequest {
            logical_ids: vec!["meeting-1".to_owned()],
            touched_at: 1_711_843_200,
            source_ref: None,
        })
        .expect_err("missing source_ref must fail");

    assert!(error.to_string().contains("source_ref"));
}
