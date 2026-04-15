//! Pack 7: async property-FTS rebuild infrastructure tests.
#![allow(clippy::expect_used, clippy::panic)]
use std::time::Instant;

use fathomdb_engine::{
    ChunkPolicy, EngineRuntime, FtsPropertyPathSpec, NodeInsert, ProvenanceMode, RebuildMode,
    TelemetryLevel, WriteRequest,
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

fn make_write_request(label: &str, nodes: Vec<NodeInsert>) -> WriteRequest {
    WriteRequest {
        label: label.to_owned(),
        nodes,
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
    }
}

/// Existing eager-mode behavior is preserved: calling with `RebuildMode::Eager`
/// returns the schema record synchronously (same as the old parameter-less behavior).
#[test]
fn eager_register_returns_schema_record() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();

    let record = svc
        .register_fts_property_schema_with_entries(
            "Meeting",
            &[FtsPropertyPathSpec::scalar("$.title")],
            None,
            &[],
            RebuildMode::Eager,
        )
        .expect("register eager");

    assert_eq!(record.kind, "Meeting");
    assert_eq!(record.property_paths, vec!["$.title"]);
}

/// `RebuildMode::Async` register call returns in <500ms (design goal is <100ms; CI budget).
#[test]
fn async_register_is_fast() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();

    let start = Instant::now();
    let record = svc
        .register_fts_property_schema_with_entries(
            "Meeting",
            &[FtsPropertyPathSpec::scalar("$.title")],
            None,
            &[],
            RebuildMode::Async,
        )
        .expect("register async");

    let elapsed = start.elapsed();
    assert_eq!(record.kind, "Meeting");
    assert!(
        elapsed.as_millis() < 500,
        "async register took {}ms, expected <500ms",
        elapsed.as_millis()
    );
}

/// After an async register, rebuild state row exists (state PENDING, BUILDING, SWAPPING, or COMPLETE).
#[test]
fn async_register_creates_rebuild_state_row() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();

    svc.register_fts_property_schema_with_entries(
        "Task",
        &[FtsPropertyPathSpec::scalar("$.name")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("register async");

    // Small sleep to let actor pick up and start processing.
    std::thread::sleep(std::time::Duration::from_millis(200));

    let state = svc
        .get_property_fts_rebuild_state("Task")
        .expect("get state");
    assert!(
        state.is_some(),
        "expected rebuild state row for 'Task' after async register"
    );
}

/// After async rebuild completes (wait for SWAPPING or COMPLETE state),
/// staging table has the expected rows.
#[test]
fn async_rebuild_populates_staging_table() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    // Insert 5 nodes of kind "Note".
    for i in 0..5u32 {
        engine
            .writer()
            .submit(make_write_request(
                &format!("seed-{i}"),
                vec![NodeInsert {
                    row_id: format!("r{i}"),
                    logical_id: format!("note:{i}"),
                    kind: "Note".to_owned(),
                    properties: format!(r#"{{"body":"hello {i}"}}"#),
                    source_ref: Some("test".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                }],
            ))
            .expect("write node");
    }

    let svc = engine.admin().service();
    svc.register_fts_property_schema_with_entries(
        "Note",
        &[FtsPropertyPathSpec::scalar("$.body")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("register async");

    // Wait for rebuild to reach SWAPPING or COMPLETE state (up to 5s).
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let state = svc
            .get_property_fts_rebuild_state("Note")
            .expect("get state");
        let done = state
            .as_ref()
            .is_some_and(|s| s.state == "SWAPPING" || s.state == "COMPLETE");
        if done {
            break;
        }
        assert!(
            Instant::now() <= deadline,
            "rebuild did not reach SWAPPING within 5s, state={:?}",
            svc.get_property_fts_rebuild_state("Note")
        );
    }

    // Verify staging table has the 5 rows.
    let count = svc.count_staging_rows("Note").expect("count staging rows");
    assert_eq!(count, 5, "expected 5 staging rows for 'Note', got {count}");
}

/// Engine shutdown drains and joins the rebuild actor cleanly (no panics, no hangs).
#[test]
fn engine_shutdown_is_clean() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    // Kick off an async rebuild so the actor has work to do.
    // Use a block so svc (Arc<AdminService>) is dropped before engine drops.
    {
        let svc = engine.admin().service();
        svc.register_fts_property_schema_with_entries(
            "Foo",
            &[FtsPropertyPathSpec::scalar("$.x")],
            None,
            &[],
            RebuildMode::Async,
        )
        .expect("register async");
    } // svc dropped here

    // Drop the engine — this should join the rebuild actor cleanly.
    // At this point all SyncSender clones are dropped (svc.rebuild_sender dropped above,
    // engine's _rebuild_sender drops with engine), so the actor thread can exit.
    drop(engine);
    // If we reach here without panic or timeout, the test passes.
}
