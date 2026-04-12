#![allow(clippy::expect_used, clippy::missing_panics_doc)]

//! Phase 1 integration tests for the adaptive text search surface.

use fathomdb::{
    ChunkInsert, ChunkPolicy, Engine, EngineOptions, NodeInsert, SearchHitSource, SearchMatchMode,
    WriteRequest,
};
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn seed_goals(engine: &Engine) {
    // Register a property FTS schema so the property-indexed branch of the
    // search UNION has data to exercise.
    engine
        .register_fts_property_schema(
            "Goal",
            &["$.name".to_owned(), "$.description".to_owned()],
            None,
        )
        .expect("register property schema");

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-goals".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "goal-quarterly-row".to_owned(),
                    logical_id: "goal-quarterly".to_owned(),
                    kind: "Goal".to_owned(),
                    properties: r#"{"name":"Ship quarterly docs","description":"Publish the quarterly planning docs for the platform team."}"#.to_owned(),
                    source_ref: Some("seed-goals".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "goal-hiring-row".to_owned(),
                    logical_id: "goal-hiring".to_owned(),
                    kind: "Goal".to_owned(),
                    properties: r#"{"name":"Hire a staff engineer","description":"Fill the open staff engineering role this half."}"#.to_owned(),
                    source_ref: Some("seed-goals".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "goal-migration-row".to_owned(),
                    logical_id: "goal-migration".to_owned(),
                    kind: "Goal".to_owned(),
                    properties: r#"{"name":"Finish database migration","description":"Cut over reads and writes to the new storage engine."}"#.to_owned(),
                    source_ref: Some("seed-goals".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![
                ChunkInsert {
                    id: "goal-quarterly-chunk".to_owned(),
                    node_logical_id: "goal-quarterly".to_owned(),
                    text_content: "Our quarterly planning docs outline roadmap commitments for the next three months.".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "goal-hiring-chunk".to_owned(),
                    node_logical_id: "goal-hiring".to_owned(),
                    text_content: "Recruit and onboard a senior staff engineer to lead infrastructure work.".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "goal-migration-chunk".to_owned(),
                    node_logical_id: "goal-migration".to_owned(),
                    text_content: "Complete the storage engine migration with zero downtime.".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
            ],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("seed goals");
}

#[test]
fn text_search_execute_returns_search_rows_with_populated_fields() {
    let (_db, engine) = open_engine();
    seed_goals(&engine);

    let rows = engine
        .query("Goal")
        .text_search("quarterly", 10)
        .execute()
        .expect("search executes");

    assert!(!rows.hits.is_empty(), "expected at least one hit");
    assert_eq!(rows.strict_hit_count, rows.hits.len());
    assert_eq!(rows.relaxed_hit_count, 0);
    assert!(!rows.fallback_used);
    assert!(!rows.was_degraded);

    let hit = rows
        .hits
        .iter()
        .find(|h| h.node.logical_id == "goal-quarterly")
        .expect("goal-quarterly hit");

    assert!(hit.score > 0.0, "score must be flipped bm25 (positive)");
    assert!(matches!(hit.match_mode, SearchMatchMode::Strict));
    assert!(matches!(
        hit.source,
        SearchHitSource::Chunk | SearchHitSource::Property,
    ));
    assert!(hit.projection_row_id.is_some());
    assert!(hit.attribution.is_none());
    assert!(hit.written_at > 0, "written_at must be populated");
    // The snippet should be populated on at least one hit.
    assert!(
        rows.hits.iter().any(|h| h.snippet.is_some()),
        "at least one hit must have a snippet"
    );

    // written_at should match the nodes.created_at row. Compare against the
    // raw sqlite column to pin the active-version semantic.
    let conn = rusqlite::Connection::open(engine.coordinator().database_path())
        .expect("open db for assertion");
    let created_at: i64 = conn
        .query_row(
            "SELECT created_at FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
            rusqlite::params!["goal-quarterly"],
            |row| row.get(0),
        )
        .expect("fetch created_at");
    assert_eq!(hit.written_at, created_at);
}

#[test]
fn text_search_zero_hits_returns_empty_search_rows() {
    let (_db, engine) = open_engine();
    seed_goals(&engine);

    let rows = engine
        .query("Goal")
        .text_search("zzznothingmatcheszzz", 10)
        .execute()
        .expect("search executes");

    assert!(rows.hits.is_empty());
    assert_eq!(rows.strict_hit_count, 0);
    assert_eq!(rows.relaxed_hit_count, 0);
    assert!(!rows.fallback_used);
    assert!(!rows.was_degraded);
}

#[test]
fn node_query_execute_still_returns_query_rows() {
    let (_db, engine) = open_engine();
    seed_goals(&engine);

    let rows = engine.query("Goal").execute().expect("flat query executes");
    // Compile-time proof: the return type is QueryRows (not SearchRows).
    let _: &fathomdb::QueryRows = &rows;
    assert!(!rows.nodes.is_empty());
}

#[test]
fn text_search_with_filter_kind_eq_chains() {
    let (_db, engine) = open_engine();
    seed_goals(&engine);

    // filter_kind_eq on TextSearchBuilder must compile and execute. Filter
    // fusion is deferred to Phase 2 — the filter is applied in the outer
    // WHERE which may narrow results but must not error.
    let rows = engine
        .query("Goal")
        .text_search("engineer", 5)
        .filter_kind_eq("Goal")
        .execute()
        .expect("filtered search executes");

    assert_eq!(rows.strict_hit_count, rows.hits.len());
    for hit in &rows.hits {
        assert_eq!(hit.node.kind, "Goal");
    }
}
