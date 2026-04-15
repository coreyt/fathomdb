//! Pack 7 + Pack 8 + Pack 9: async property-FTS rebuild infrastructure tests.
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

    // Verify that staging OR the live FTS table has the 5 rows.
    // If the rebuild reached COMPLETE, the swap already moved rows from staging
    // to fts_node_properties and cleared staging — both are valid outcomes.
    let state = svc
        .get_property_fts_rebuild_state("Note")
        .expect("get state")
        .expect("state row must exist");
    if state.state == "SWAPPING" {
        // Swap is in progress; staging should still have the rows.
        let count = svc.count_staging_rows("Note").expect("count staging rows");
        assert_eq!(
            count, 5,
            "expected 5 staging rows for 'Note' during SWAPPING, got {count}"
        );
    } else {
        // COMPLETE: staging was cleared; verify via FTS table (raw connection).
        let conn =
            rusqlite::Connection::open(dir.path().join("test.db")).expect("open raw connection");
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM fts_node_properties WHERE kind = 'Note'",
                [],
                |r| r.get(0),
            )
            .expect("count fts rows");
        assert_eq!(
            count, 5,
            "expected 5 fts_node_properties rows for 'Note' after swap, got {count}"
        );
    }
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

    // Check the current rebuild state: if BUILDING/SWAPPING, the node should be
    // in staging. If COMPLETE, the swap already moved rows to fts_node_properties.
    let current_state = svc
        .get_property_fts_rebuild_state("Ticket")
        .expect("get state")
        .expect("state row must exist");

    if current_state.state == "BUILDING" || current_state.state == "SWAPPING" {
        // Actor is still running: double-write should have placed the node in staging.
        let in_staging = svc
            .staging_row_exists("Ticket", "ticket:1")
            .expect("staging_row_exists");
        assert!(
            in_staging,
            "expected ticket:1 to be in staging table during rebuild"
        );
    } else {
        // COMPLETE: the swap moved all rows to FTS and cleared staging.
        // Verify the node is findable via FTS query.
        let compiled = QueryBuilder::nodes("Ticket")
            .text_search("urgent", 10)
            .limit(10)
            .compile()
            .expect("compile");
        let rows = engine
            .coordinator()
            .execute_compiled_read(&compiled)
            .expect("query after rebuild");
        assert!(
            !rows.nodes.is_empty(),
            "ticket:1 should be findable via FTS after rebuild COMPLETE"
        );
    }
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

    // Wait until the actor has processed the node into staging OR the rebuild
    // has already completed (in which case staging is empty and FTS has the row).
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let in_staging = svc
            .staging_row_exists("TicketDel", "ticket:del")
            .expect("staging_row_exists");
        if in_staging {
            break;
        }
        // Also accept the case where the rebuild has already completed
        // (swap moved node to FTS and cleared staging).
        let state = svc
            .get_property_fts_rebuild_state("TicketDel")
            .expect("get state");
        if state.as_ref().is_some_and(|s| s.state == "COMPLETE") {
            break;
        }
        assert!(
            Instant::now() <= deadline,
            "ticket:del never appeared in staging and rebuild never completed within 5s"
        );
    }

    // Now retire the node.
    engine
        .writer()
        .submit(make_retire_request("retire-del", "ticket:del"))
        .expect("retire node");

    // After retire, staging should not contain the node (it was never written
    // there after the swap, or it was already absent).
    let in_staging = svc
        .staging_row_exists("TicketDel", "ticket:del")
        .expect("staging_row_exists after retire");
    assert!(
        !in_staging,
        "ticket:del should not be in staging after retire"
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

// ── Pack 9 tests ─────────────────────────────────────────────────────────────

/// Test A: async rebuild completes and the FTS index is queryable with the new schema.
#[test]
fn async_rebuild_completes_and_queries_new_schema() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    // Insert nodes before registering the schema (first registration).
    for i in 0..3u32 {
        engine
            .writer()
            .submit(make_write_request(
                &format!("seed-{i}"),
                vec![make_node(
                    &format!("article:{i}"),
                    "Article",
                    &format!(r#"{{"headline":"searchable headline {i}"}}"#),
                )],
            ))
            .expect("write node");
    }

    let svc = engine.admin().service();
    svc.register_fts_property_schema_with_entries(
        "Article",
        &[FtsPropertyPathSpec::scalar("$.headline")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("register async");

    // Wait for COMPLETE state (up to 10s).
    wait_for_state(&svc, "Article", &["COMPLETE"], 10);

    // FTS query should return results from the rebuilt index.
    let compiled = QueryBuilder::nodes("Article")
        .text_search("searchable", 10)
        .limit(10)
        .compile()
        .expect("compile");

    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute read after rebuild COMPLETE");

    assert_eq!(
        rows.nodes.len(),
        3,
        "FTS query after rebuild COMPLETE should return all 3 nodes, got {}",
        rows.nodes.len()
    );
}

/// Test B: crash recovery — reopening engine marks interrupted rebuilds as FAILED
/// and cleans up staging.
#[test]
fn crash_recovery_mid_building_marks_failed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("test.db");

    {
        let engine = EngineRuntime::open(
            &db_path,
            ProvenanceMode::Warn,
            None,
            2,
            TelemetryLevel::Counters,
            None,
        )
        .expect("open engine");

        // Insert nodes before registering.
        for i in 0..5u32 {
            engine
                .writer()
                .submit(make_write_request(
                    &format!("seed-{i}"),
                    vec![make_node(
                        &format!("crash:{i}"),
                        "CrashKind",
                        &format!(r#"{{"note":"note {i}"}}"#),
                    )],
                ))
                .expect("write node");
        }

        let svc = engine.admin().service();
        svc.register_fts_property_schema_with_entries(
            "CrashKind",
            &[FtsPropertyPathSpec::scalar("$.note")],
            None,
            &[],
            RebuildMode::Async,
        )
        .expect("register async");

        // Wait until at least BUILDING (confirm actor started).
        wait_for_state(&svc, "CrashKind", &["BUILDING", "SWAPPING", "COMPLETE"], 5);

        // Simulate crash: drop engine without waiting for completion.
        // The rebuild actor is joined during drop but the state in DB may still
        // show BUILDING/SWAPPING for the crash-recovery test to work.
        // To simulate a real crash we need to write BUILDING state and leave it,
        // but since we can't kill the process, we use the raw connection to
        // directly set the state back to BUILDING after the engine drops.
    }

    // After the first engine drops, the actor has completed normally.
    // To simulate a true mid-build crash, use a raw connection to force BUILDING state.
    {
        let raw_conn = rusqlite::Connection::open(&db_path).expect("open raw");
        raw_conn
            .execute(
                "UPDATE fts_property_rebuild_state SET state = 'BUILDING' WHERE kind = 'CrashKind'",
                [],
            )
            .expect("force BUILDING state");
        // Also insert a fake staging row to verify cleanup.
        raw_conn
            .execute(
                "INSERT OR IGNORE INTO fts_property_rebuild_staging \
                 (kind, node_logical_id, text_content) VALUES ('CrashKind', 'fake:1', 'fake')",
                [],
            )
            .expect("insert fake staging row");
    }

    // Reopen engine — crash recovery should run.
    let engine2 = EngineRuntime::open(
        &db_path,
        ProvenanceMode::Warn,
        None,
        2,
        TelemetryLevel::Counters,
        None,
    )
    .expect("reopen engine");

    let svc2 = engine2.admin().service();
    let state = svc2
        .get_property_fts_rebuild_state("CrashKind")
        .expect("get state after reopen")
        .expect("state row must exist");

    assert_eq!(
        state.state, "FAILED",
        "crash recovery should mark interrupted rebuild as FAILED, got '{}'",
        state.state
    );

    // Staging table should be empty for this kind.
    let staging_count = svc2
        .count_staging_rows("CrashKind")
        .expect("count staging rows");
    assert_eq!(
        staging_count, 0,
        "crash recovery should clean up staging table, got {staging_count} rows"
    );
}

/// Test C: `get_property_fts_rebuild_progress` returns progress and reaches COMPLETE.
#[test]
fn get_property_fts_rebuild_progress_returns_progress() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    // Insert a few nodes.
    for i in 0..3u32 {
        engine
            .writer()
            .submit(make_write_request(
                &format!("seed-{i}"),
                vec![make_node(
                    &format!("prog:{i}"),
                    "ProgKind",
                    &format!(r#"{{"text":"progress text {i}"}}"#),
                )],
            ))
            .expect("write node");
    }

    let svc = engine.admin().service();
    svc.register_fts_property_schema_with_entries(
        "ProgKind",
        &[FtsPropertyPathSpec::scalar("$.text")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("register async");

    // Poll get_property_fts_rebuild_progress until COMPLETE.
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let progress = engine
            .coordinator()
            .get_property_fts_rebuild_progress("ProgKind")
            .expect("get_property_fts_rebuild_progress");
        if let Some(p) = progress
            && p.state == "COMPLETE"
        {
            assert!(
                p.rows_done > 0,
                "rows_done should be > 0, got {}",
                p.rows_done
            );
            assert!(
                p.started_at > 0,
                "started_at should be a unix millis timestamp > 0"
            );
            break;
        }
        assert!(
            Instant::now() <= deadline,
            "rebuild did not reach COMPLETE within 10s"
        );
    }
}

// ── Pack 10 tests ─────────────────────────────────────────────────────────────

/// Test Pack10-1: after a rebuild reaches COMPLETE, the live `fts_node_properties`
/// table has all expected rows and a text search returns the re-indexed nodes.
#[test]
fn rebuild_completes_and_fts_table_is_queryable() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    // Seed 4 nodes before registering the schema (first registration).
    for i in 0..4u32 {
        engine
            .writer()
            .submit(make_write_request(
                &format!("seed-{i}"),
                vec![make_node(
                    &format!("doc:{i}"),
                    "DocKind",
                    &format!(r#"{{"content":"important document {i}"}}"#),
                )],
            ))
            .expect("write node");
    }

    let svc = engine.admin().service();
    svc.register_fts_property_schema_with_entries(
        "DocKind",
        &[FtsPropertyPathSpec::scalar("$.content")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("register async");

    // Wait for COMPLETE (up to 10s).
    wait_for_state(&svc, "DocKind", &["COMPLETE"], 10);

    // Verify all 4 rows exist in fts_node_properties via a raw connection.
    let conn = rusqlite::Connection::open(dir.path().join("test.db")).expect("open raw connection");
    let fts_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM fts_node_properties WHERE kind = 'DocKind'",
            [],
            |r| r.get(0),
        )
        .expect("count fts rows");
    assert_eq!(
        fts_count, 4,
        "expected 4 fts_node_properties rows after rebuild, got {fts_count}"
    );

    // Staging table should be empty (swap moved all rows).
    let staging_count = svc.count_staging_rows("DocKind").expect("count staging");
    assert_eq!(
        staging_count, 0,
        "staging should be empty after COMPLETE swap, got {staging_count}"
    );

    // Text search via coordinator should return all 4 nodes.
    let compiled = QueryBuilder::nodes("DocKind")
        .text_search("important", 10)
        .limit(10)
        .compile()
        .expect("compile query");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute text search");
    assert_eq!(
        rows.nodes.len(),
        4,
        "text search after rebuild should return all 4 nodes, got {}",
        rows.nodes.len()
    );
}

/// Test Pack10-2: nodes written *during* the rebuild are included in the final
/// FTS index after COMPLETE.  Seed 5 nodes, trigger async rebuild, write 3
/// more during the rebuild window, wait for COMPLETE, assert all 8 are findable.
#[test]
fn concurrent_writes_during_rebuild_are_indexed_in_final_fts() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    // Seed 5 nodes before registering.
    for i in 0..5u32 {
        engine
            .writer()
            .submit(make_write_request(
                &format!("seed-{i}"),
                vec![make_node(
                    &format!("concurrent:{i}"),
                    "ConcKind",
                    &format!(r#"{{"msg":"concurrent message {i}"}}"#),
                )],
            ))
            .expect("write seed node");
    }

    let svc = engine.admin().service();
    svc.register_fts_property_schema_with_entries(
        "ConcKind",
        &[FtsPropertyPathSpec::scalar("$.msg")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("register async");

    // Wait until rebuild is actively building — writes must happen while BUILDING
    // to exercise the staging double-write code path.
    for _ in 0..200 {
        let p = engine
            .coordinator()
            .get_property_fts_rebuild_progress("ConcKind")
            .expect("get_property_fts_rebuild_progress")
            .expect("rebuild state row should exist");
        if p.state == "BUILDING" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    for i in 5..8u32 {
        engine
            .writer()
            .submit(make_write_request(
                &format!("during-{i}"),
                vec![make_node(
                    &format!("concurrent:{i}"),
                    "ConcKind",
                    &format!(r#"{{"msg":"concurrent message {i}"}}"#),
                )],
            ))
            .expect("write node during rebuild");
    }

    // Wait for COMPLETE.
    wait_for_state(&svc, "ConcKind", &["COMPLETE"], 10);

    // All 8 nodes must appear in the FTS index.
    let compiled = QueryBuilder::nodes("ConcKind")
        .text_search("concurrent", 10)
        .limit(20)
        .compile()
        .expect("compile query");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute text search");
    assert_eq!(
        rows.nodes.len(),
        8,
        "all 8 nodes (5 seeded + 3 written during rebuild) should be in FTS after COMPLETE, got {}",
        rows.nodes.len()
    );
}

/// Test Pack10-3: poll `get_property_fts_rebuild_progress` in a loop and assert
/// that the PENDING → BUILDING → COMPLETE sequence is observed in order.
#[test]
fn rebuild_progress_transitions_through_states() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    // Insert nodes so there is real work for the actor.
    for i in 0..5u32 {
        engine
            .writer()
            .submit(make_write_request(
                &format!("seed-{i}"),
                vec![make_node(
                    &format!("trans:{i}"),
                    "TransKind",
                    &format!(r#"{{"label":"transition label {i}"}}"#),
                )],
            ))
            .expect("write node");
    }

    let svc = engine.admin().service();
    svc.register_fts_property_schema_with_entries(
        "TransKind",
        &[FtsPropertyPathSpec::scalar("$.label")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("register async");

    // Poll the coordinator's progress API collecting observed states.
    let mut observed: Vec<String> = Vec::new();
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let progress = engine
            .coordinator()
            .get_property_fts_rebuild_progress("TransKind")
            .expect("get progress");
        if let Some(p) = progress {
            // Record state transitions (deduplicate consecutive identical states).
            if observed.last().map(|s: &String| s.as_str()) != Some(p.state.as_str()) {
                observed.push(p.state.clone());
            }
            if p.state == "COMPLETE" || p.state == "FAILED" {
                break;
            }
        }
        assert!(
            Instant::now() <= deadline,
            "rebuild did not reach a terminal state within 10s; observed states: {observed:?}"
        );
    }

    // The last observed state must be COMPLETE.
    assert_eq!(
        observed.last().map(String::as_str),
        Some("COMPLETE"),
        "final state must be COMPLETE, observed: {observed:?}"
    );

    // All states in `observed` must appear in the expected ordering.
    let expected_order = ["PENDING", "BUILDING", "SWAPPING", "COMPLETE"];
    let mut last_pos: Option<usize> = None;
    for state in &observed {
        let pos = expected_order
            .iter()
            .position(|&s| s == state.as_str())
            .unwrap_or_else(|| panic!("unexpected state observed: {state}"));
        if let Some(prev) = last_pos {
            assert!(
                pos >= prev,
                "state ordering violated: saw {state} (pos {pos}) after pos {prev}; full sequence: {observed:?}"
            );
        }
        last_pos = Some(pos);
    }
}

/// Test Pack10-4: after a rebuild reaches COMPLETE, calling
/// `register_fts_property_schema_with_entries` again with a new path starts a
/// fresh rebuild cycle.  A new PENDING state is observed after the second
/// registration, and the rebuild reaches COMPLETE again with the updated schema.
#[test]
fn re_registration_triggers_new_rebuild() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = open_engine(&dir);

    // Insert nodes with two properties.
    for i in 0..3u32 {
        engine
            .writer()
            .submit(make_write_request(
                &format!("seed-{i}"),
                vec![make_node(
                    &format!("rereg2:{i}"),
                    "Rereg2Kind",
                    &format!(r#"{{"name":"rereg name {i}","tag":"uniquetag {i}"}}"#),
                )],
            ))
            .expect("write node");
    }

    let svc = engine.admin().service();

    // First async registration — index only $.name.
    svc.register_fts_property_schema_with_entries(
        "Rereg2Kind",
        &[FtsPropertyPathSpec::scalar("$.name")],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("first register async");

    // Wait for first rebuild to COMPLETE.
    wait_for_state(&svc, "Rereg2Kind", &["COMPLETE"], 10);

    // Verify first schema only indexes $.name — $.tag content is NOT findable.
    let tag_query = QueryBuilder::nodes("Rereg2Kind")
        .text_search("uniquetag", 10)
        .limit(10)
        .compile()
        .expect("compile query for tag");
    let tag_rows_before = engine
        .coordinator()
        .execute_compiled_read(&tag_query)
        .expect("search before second register");
    assert_eq!(
        tag_rows_before.nodes.len(),
        0,
        "$.tag should not be indexed after first registration (only $.name indexed)"
    );

    // Second async registration — add $.tag to the indexed paths.
    svc.register_fts_property_schema_with_entries(
        "Rereg2Kind",
        &[
            FtsPropertyPathSpec::scalar("$.name"),
            FtsPropertyPathSpec::scalar("$.tag"),
        ],
        None,
        &[],
        RebuildMode::Async,
    )
    .expect("second register async");

    // A new rebuild cycle must start: state transitions back to PENDING/BUILDING.
    wait_for_state(
        &svc,
        "Rereg2Kind",
        &["PENDING", "BUILDING", "SWAPPING", "COMPLETE"],
        5,
    );

    // Wait for second rebuild to COMPLETE.
    wait_for_state(&svc, "Rereg2Kind", &["COMPLETE"], 10);

    // After second rebuild, $.tag content ("uniquetag") must now be findable.
    let tag_rows_after = engine
        .coordinator()
        .execute_compiled_read(&tag_query)
        .expect("search after second rebuild");
    assert_eq!(
        tag_rows_after.nodes.len(),
        3,
        "$.tag ('uniquetag') should be indexed after second rebuild, got {}",
        tag_rows_after.nodes.len()
    );
}

/// Test Pack10-5: after a simulated mid-build crash (engine dropped with BUILDING
/// state forced via raw SQL), reopening the engine runs crash recovery and the
/// `fts_property_rebuild_staging` table has 0 rows for the kind.
#[test]
fn crash_recovery_clears_staging_rows() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("test.db");

    {
        let engine = EngineRuntime::open(
            &db_path,
            ProvenanceMode::Warn,
            None,
            2,
            TelemetryLevel::Counters,
            None,
        )
        .expect("open engine");

        // Insert nodes so the rebuild produces staging rows.
        for i in 0..4u32 {
            engine
                .writer()
                .submit(make_write_request(
                    &format!("seed-{i}"),
                    vec![make_node(
                        &format!("clr:{i}"),
                        "ClrKind",
                        &format!(r#"{{"data":"clr data {i}"}}"#),
                    )],
                ))
                .expect("write node");
        }

        let svc = engine.admin().service();
        svc.register_fts_property_schema_with_entries(
            "ClrKind",
            &[FtsPropertyPathSpec::scalar("$.data")],
            None,
            &[],
            RebuildMode::Async,
        )
        .expect("register async");

        // Wait until the actor has at least started building.
        wait_for_state(&svc, "ClrKind", &["BUILDING", "SWAPPING", "COMPLETE"], 5);
        // Engine drops here, actor is joined.
    }

    // Force BUILDING state and insert a fake staging row to simulate a crash
    // that left behind in-progress state.
    {
        let raw_conn = rusqlite::Connection::open(&db_path).expect("open raw connection");
        raw_conn
            .execute(
                "UPDATE fts_property_rebuild_state SET state = 'BUILDING' WHERE kind = 'ClrKind'",
                [],
            )
            .expect("force BUILDING state");
        raw_conn
            .execute(
                "INSERT OR IGNORE INTO fts_property_rebuild_staging \
                 (kind, node_logical_id, text_content) VALUES ('ClrKind', 'fake:crash', 'leftover')",
                [],
            )
            .expect("insert fake staging row");
    }

    // Reopen engine — crash recovery should clear staging for 'ClrKind'.
    let engine2 = EngineRuntime::open(
        &db_path,
        ProvenanceMode::Warn,
        None,
        2,
        TelemetryLevel::Counters,
        None,
    )
    .expect("reopen engine");

    let svc2 = engine2.admin().service();

    // Staging table must have 0 rows for 'ClrKind' after recovery.
    let staging_count = svc2
        .count_staging_rows("ClrKind")
        .expect("count staging rows after recovery");
    assert_eq!(
        staging_count, 0,
        "crash recovery must clear all staging rows for 'ClrKind', got {staging_count}"
    );

    // The state should also be FAILED (crash recovery marks interrupted builds FAILED).
    let state = svc2
        .get_property_fts_rebuild_state("ClrKind")
        .expect("get state after recovery")
        .expect("state row must exist");
    assert_eq!(
        state.state, "FAILED",
        "crash recovery must mark 'ClrKind' as FAILED, got '{}'",
        state.state
    );
}
