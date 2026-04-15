//! Pack 7 + Pack 8: async property-FTS rebuild infrastructure tests.
#![allow(clippy::expect_used, clippy::panic)]
use std::time::Instant;

use fathomdb_engine::{
    ChunkPolicy, EngineRuntime, FtsPropertyPathSpec, NodeInsert, NodeRetire, ProvenanceMode,
    RebuildMode, TelemetryLevel, WriteRequest,
};
use fathomdb_query::QueryBuilder;

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

// ── Pack 8 tests ─────────────────────────────────────────────────────────────

fn make_retire_request(label: &str, logical_id: &str) -> WriteRequest {
    WriteRequest {
        label: label.to_owned(),
        nodes: vec![],
        node_retires: vec![NodeRetire {
            logical_id: logical_id.to_owned(),
            source_ref: Some("test".to_owned()),
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
    }
}

fn make_node(id: &str, kind: &str, props: &str) -> NodeInsert {
    NodeInsert {
        row_id: format!("row-{id}"),
        logical_id: id.to_owned(),
        kind: kind.to_owned(),
        properties: props.to_owned(),
        source_ref: Some("test".to_owned()),
        upsert: false,
        chunk_policy: ChunkPolicy::Preserve,
        content_ref: None,
    }
}

/// Wait for the rebuild state of `kind` to reach one of the target states.
/// Returns the state string when found within the deadline.
fn wait_for_state(
    svc: &fathomdb_engine::AdminService,
    kind: &str,
    targets: &[&str],
    deadline_secs: u64,
) -> String {
    let deadline = Instant::now() + std::time::Duration::from_secs(deadline_secs);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let state = svc
            .get_property_fts_rebuild_state(kind)
            .expect("get_property_fts_rebuild_state");
        if let Some(row) = state
            && targets.iter().any(|t| row.state == *t)
        {
            return row.state.clone();
        }
        assert!(
            Instant::now() <= deadline,
            "timed out waiting for kind={kind} to reach state in {targets:?}"
        );
    }
}

/// Test A: writing a node of a kind that is currently rebuilding populates
/// `fts_property_rebuild_staging` with a row for that node.
#[test]
fn write_during_rebuild_populates_staging() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    let svc = engine.admin().service();
    // Register under Async so a rebuild starts.
    svc.register_fts_property_schema_with_entries(
        "Ticket",
        &[FtsPropertyPathSpec::scalar("$.title")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("register async");

    // Wait until the actor has moved the state to BUILDING.
    wait_for_state(&svc, "Ticket", &["BUILDING", "SWAPPING", "COMPLETE"], 5);

    // Now write a new node of that kind.
    let new_node = make_node("ticket:1", "Ticket", r#"{"title":"urgent bug"}"#);
    engine
        .writer()
        .submit(make_write_request("w1", vec![new_node]))
        .expect("write node");

    // The staging table should contain a row for this node.
    let count = svc
        .count_staging_rows("Ticket")
        .expect("count staging rows");
    assert!(
        count >= 1,
        "expected at least 1 staging row for 'Ticket', got {count}"
    );

    // Verify the specific node is in staging.
    let in_staging = svc
        .staging_row_exists("Ticket", "ticket:1")
        .expect("staging_row_exists");
    assert!(in_staging, "expected ticket:1 to be in staging table");
}

/// Test B: deleting a node during rebuild removes it from both the live FTS
/// table and the staging table.
#[test]
fn delete_during_rebuild_removes_from_both_tables() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    // Insert a node before registering the schema (so it gets pre-seeded by
    // the rebuild actor into staging as part of the normal batch walk).
    let node = make_node("ticket:del", "TicketDel", r#"{"title":"to be deleted"}"#);
    engine
        .writer()
        .submit(make_write_request("seed", vec![node]))
        .expect("write seed node");

    let svc = engine.admin().service();
    svc.register_fts_property_schema_with_entries(
        "TicketDel",
        &[FtsPropertyPathSpec::scalar("$.title")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("register async");

    // Wait until staging has the node (actor has processed it).
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let in_staging = svc
            .staging_row_exists("TicketDel", "ticket:del")
            .expect("staging_row_exists");
        if in_staging {
            break;
        }
        assert!(
            Instant::now() <= deadline,
            "ticket:del never appeared in staging within 5s"
        );
    }

    // Now retire the node.
    engine
        .writer()
        .submit(make_retire_request("retire-del", "ticket:del"))
        .expect("retire node");

    // Both live FTS and staging should be gone.
    let in_staging = svc
        .staging_row_exists("TicketDel", "ticket:del")
        .expect("staging_row_exists after retire");
    assert!(
        !in_staging,
        "ticket:del should be removed from staging after retire"
    );

    // Also check fts_node_properties directly via a raw connection.
    let conn = rusqlite::Connection::open(dir.path().join("test.db")).expect("open raw connection");
    let live_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM fts_node_properties WHERE node_logical_id = 'ticket:del'",
            [],
            |r| r.get(0),
        )
        .expect("count live fts");
    assert_eq!(
        live_count, 0,
        "ticket:del should be removed from fts_node_properties after retire"
    );
}

/// Test C: first registration under Async — the coordinator's property-FTS
/// query uses JSON scan fallback when `is_first_registration=1` and state is
/// PENDING/BUILDING (no FTS5 rows exist yet).
#[test]
fn read_during_first_registration_uses_scan_fallback() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    // Insert nodes before registering FTS schema (is_first_registration=1).
    for i in 0..3u32 {
        let node = make_node(
            &format!("note:{i}"),
            "ScanNote",
            &format!(r#"{{"body":"findme note {i}"}}"#),
        );
        engine
            .writer()
            .submit(make_write_request(&format!("seed-{i}"), vec![node]))
            .expect("write node");
    }

    let svc = engine.admin().service();
    svc.register_fts_property_schema_with_entries(
        "ScanNote",
        &[FtsPropertyPathSpec::scalar("$.body")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("register async");

    // Check that the rebuild state is first_registration=true.
    let state = svc
        .get_property_fts_rebuild_state("ScanNote")
        .expect("get state")
        .expect("state must exist");
    assert!(
        state.is_first_registration,
        "expected is_first_registration=true for first async registration"
    );

    // While state is still PENDING/BUILDING, execute a property-FTS query via
    // the coordinator. The scan fallback should return results even though the
    // FTS5 table has no rows for this kind yet.
    let compiled = QueryBuilder::nodes("ScanNote")
        .text_search("findme", 10)
        .limit(10)
        .compile()
        .expect("compiled query");

    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute read during first-registration rebuild");

    assert!(
        !rows.nodes.is_empty(),
        "scan fallback should return nodes during first-registration rebuild, got 0 results"
    );
    assert_eq!(
        rows.nodes.len(),
        3,
        "scan fallback should return all 3 matching nodes"
    );
}

/// Test D: re-registration under Async — queries during PENDING/BUILDING use
/// the existing live FTS table (no scan fallback), so results are returned.
#[test]
fn read_during_re_registration_uses_live_fts_table() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    let svc = engine.admin().service();

    // First registration under Eager (schema must be registered BEFORE inserting
    // nodes so the writer populates fts_node_properties during the inserts).
    svc.register_fts_property_schema_with_entries(
        "ReregKind",
        &[FtsPropertyPathSpec::scalar("$.title")],
        None,
        &[],
        RebuildMode::Eager,
    )
    .expect("register eager");

    // Insert nodes AFTER schema is registered so the writer populates
    // fts_node_properties for each node at write time.
    for i in 0..3u32 {
        let node = make_node(
            &format!("rereg:{i}"),
            "ReregKind",
            &format!(r#"{{"title":"rereg node {i}"}}"#),
        );
        engine
            .writer()
            .submit(make_write_request(&format!("seed-{i}"), vec![node]))
            .expect("write node");
    }

    // Now re-register under Async — this is a re-registration (is_first_registration=0).
    svc.register_fts_property_schema_with_entries(
        "ReregKind",
        &[FtsPropertyPathSpec::scalar("$.title")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("re-register async");

    // The state row should show is_first_registration=false.
    let state = svc
        .get_property_fts_rebuild_state("ReregKind")
        .expect("get state")
        .expect("state must exist");
    assert!(
        !state.is_first_registration,
        "expected is_first_registration=false for re-registration"
    );

    // Query via the coordinator during PENDING/BUILDING. The existing live FTS
    // rows (from the eager registration) should be used — no scan fallback.
    let compiled = QueryBuilder::nodes("ReregKind")
        .text_search("rereg", 10)
        .limit(10)
        .compile()
        .expect("compiled query");

    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute read during re-registration rebuild");

    assert_eq!(
        rows.nodes.len(),
        3,
        "re-registration should use live FTS table and return all 3 nodes"
    );
}
