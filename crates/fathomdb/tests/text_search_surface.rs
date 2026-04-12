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
fn text_search_filter_kind_eq_respects_limit_after_fusion() {
    // Phase 2: fusable predicates must be injected into the search CTE so
    // the CTE LIMIT applies AFTER filtering. Seed 20 non-Goal nodes whose
    // chunks contain "budget" and 3 Goal nodes whose chunks contain
    // "budget". A text_search("budget", 5) + filter_kind_eq("Goal") must
    // return exactly 3 hits (all Goal), not 5 raw "budget" hits then
    // filtered down.
    let (_db, engine) = open_engine();

    let mut nodes = Vec::new();
    let mut chunks = Vec::new();
    for i in 0..20 {
        nodes.push(NodeInsert {
            row_id: format!("other-row-{i}"),
            logical_id: format!("other-{i}"),
            kind: "Other".to_owned(),
            properties: r#"{"name":"other node"}"#.to_owned(),
            source_ref: Some("seed".to_owned()),
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        chunks.push(ChunkInsert {
            id: format!("other-chunk-{i}"),
            node_logical_id: format!("other-{i}"),
            text_content: format!("this is about the budget for project {i}"),
            byte_start: None,
            byte_end: None,
            content_hash: None,
        });
    }
    for i in 0..3 {
        nodes.push(NodeInsert {
            row_id: format!("goal-row-{i}"),
            logical_id: format!("goal-{i}"),
            kind: "Goal".to_owned(),
            properties: r#"{"name":"goal node"}"#.to_owned(),
            source_ref: Some("seed".to_owned()),
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        chunks.push(ChunkInsert {
            id: format!("goal-chunk-{i}"),
            node_logical_id: format!("goal-{i}"),
            text_content: format!("the goal is to cut the budget by {i} percent"),
            byte_start: None,
            byte_end: None,
            content_hash: None,
        });
    }

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed".to_owned(),
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
        })
        .expect("seed mixed-kind budget nodes");

    let rows = engine
        .query("Goal")
        .text_search("budget", 5)
        .filter_kind_eq("Goal")
        .execute()
        .expect("fused search executes");

    assert_eq!(
        rows.hits.len(),
        3,
        "fusion must keep all 3 Goal hits despite the 20-node Other lead; got hits: {:#?}",
        rows.hits
            .iter()
            .map(|h| &h.node.logical_id)
            .collect::<Vec<_>>()
    );
    for hit in &rows.hits {
        assert_eq!(hit.node.kind, "Goal");
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn text_search_diacritic_and_stem_matches() {
    // Phase 2 tokenizer migration: unicode61 remove_diacritics 2 + porter
    // stemmer. `cafe` matches `café`, `shipping` matches `ship docs`, and
    // `SHIP` matches `ship docs` (case-insensitive).
    let (_db, engine) = open_engine();
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-tokens".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "cafe-row".to_owned(),
                    logical_id: "cafe".to_owned(),
                    kind: "Note".to_owned(),
                    properties: r#"{"name":"cafe note"}"#.to_owned(),
                    source_ref: Some("seed".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "ship-row".to_owned(),
                    logical_id: "ship".to_owned(),
                    kind: "Note".to_owned(),
                    properties: r#"{"name":"ship note"}"#.to_owned(),
                    source_ref: Some("seed".to_owned()),
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
                    id: "cafe-chunk".to_owned(),
                    node_logical_id: "cafe".to_owned(),
                    text_content: "meeting at the café downtown".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "ship-chunk".to_owned(),
                    node_logical_id: "ship".to_owned(),
                    text_content: "ship docs tomorrow".to_owned(),
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
        .expect("seed tokenizer nodes");

    // Diacritic insensitivity: "cafe" should match "café".
    let cafe_rows = engine
        .query("Note")
        .text_search("cafe", 5)
        .execute()
        .expect("cafe search executes");
    assert!(
        cafe_rows.hits.iter().any(|h| h.node.logical_id == "cafe"),
        "'cafe' must match 'café' via remove_diacritics tokenizer; got {:#?}",
        cafe_rows
            .hits
            .iter()
            .map(|h| &h.node.logical_id)
            .collect::<Vec<_>>()
    );

    // Porter stemming: "shipping" should match "ship docs".
    let shipping_rows = engine
        .query("Note")
        .text_search("shipping", 5)
        .execute()
        .expect("shipping search executes");
    assert!(
        shipping_rows
            .hits
            .iter()
            .any(|h| h.node.logical_id == "ship"),
        "'shipping' must match 'ship docs' via porter stemmer; got {:#?}",
        shipping_rows
            .hits
            .iter()
            .map(|h| &h.node.logical_id)
            .collect::<Vec<_>>()
    );

    // Case-insensitivity: "SHIP" should match "ship docs".
    let upper_rows = engine
        .query("Note")
        .text_search("SHIP", 5)
        .execute()
        .expect("SHIP search executes");
    assert!(
        upper_rows.hits.iter().any(|h| h.node.logical_id == "ship"),
        "'SHIP' must match 'ship docs' (unicode61 lower-casing); got {:#?}",
        upper_rows
            .hits
            .iter()
            .map(|h| &h.node.logical_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn reopen_roundtrip_keeps_fts_integrity() {
    // Phase 2 tokenizer migration rebuilds fts_nodes and fts_node_properties
    // from canonical state. Open, seed, close, reopen, and verify that
    // text search still returns the seeded hits and admin integrity is
    // clean. This exercises the rebuild path on every open where migration
    // 16 has already been applied (it's a no-op) and pins the invariant
    // that reopening a migrated database preserves the FTS projection.
    let db = NamedTempFile::new().expect("temp db");
    {
        let engine = Engine::open(EngineOptions::new(db.path())).expect("open #1");
        seed_goals(&engine);
        let integrity = engine
            .admin()
            .service()
            .check_integrity()
            .expect("integrity #1");
        assert!(
            integrity.physical_ok,
            "physical integrity must hold after seed"
        );
        assert_eq!(
            integrity.missing_fts_rows, 0,
            "no missing fts rows after seed"
        );
    }

    let engine = Engine::open(EngineOptions::new(db.path())).expect("open #2");
    let rows = engine
        .query("Goal")
        .text_search("quarterly", 10)
        .execute()
        .expect("reopened search executes");
    assert!(
        !rows.hits.is_empty(),
        "text search must still find seeded Goal after reopen"
    );

    let integrity = engine
        .admin()
        .service()
        .check_integrity()
        .expect("integrity #2");
    assert!(
        integrity.physical_ok,
        "physical integrity must hold after reopen"
    );
    assert_eq!(
        integrity.missing_fts_rows, 0,
        "fts rows must not go missing across reopen"
    );
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
