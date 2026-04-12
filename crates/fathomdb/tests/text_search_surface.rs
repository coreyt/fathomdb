#![allow(clippy::expect_used, clippy::missing_panics_doc)]

//! Phase 1 integration tests for the adaptive text search surface.

use fathomdb::{
    ChunkInsert, ChunkPolicy, Engine, EngineOptions, FtsPropertyPathSpec, HitAttribution,
    NodeInsert, SearchHitSource, SearchMatchMode, WriteRequest,
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
fn strict_hit_does_not_trigger_relaxed_branch() {
    // Phase 3: when the strict branch returns at least one hit, the relaxed
    // fallback branch MUST NOT run. "budget meeting" matches a seeded chunk
    // directly, so strict finds hits and relaxed stays dormant.
    let (_db, engine) = open_engine();
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-budget".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "budget-row".to_owned(),
                logical_id: "budget".to_owned(),
                kind: "Goal".to_owned(),
                properties: r#"{"name":"budget goal"}"#.to_owned(),
                source_ref: Some("seed".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "budget-chunk".to_owned(),
                node_logical_id: "budget".to_owned(),
                text_content: "budget meeting quarterly review notes".to_owned(),
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
        .expect("seed budget node");

    let rows = engine
        .query("Goal")
        .text_search("budget meeting", 10)
        .execute()
        .expect("search executes");

    assert!(!rows.hits.is_empty(), "strict should find hits");
    assert!(
        !rows.fallback_used,
        "relaxed branch must not fire on strict hit"
    );
    assert_eq!(rows.relaxed_hit_count, 0);
    assert_eq!(rows.strict_hit_count, rows.hits.len());
    assert!(!rows.was_degraded);
    for hit in &rows.hits {
        assert!(matches!(hit.match_mode, SearchMatchMode::Strict));
    }
}

#[test]
fn strict_miss_triggers_relaxed_branch_and_returns_relaxed_hits() {
    // Phase 3: when strict returns zero hits, the coordinator runs the
    // relaxed (per-term OR) branch. "budget nonexistentterm" fails the
    // implicit AND under strict, but the relaxed branch matches "budget".
    let (_db, engine) = open_engine();
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-budget".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "budget-row".to_owned(),
                logical_id: "budget".to_owned(),
                kind: "Goal".to_owned(),
                properties: r#"{"name":"budget goal"}"#.to_owned(),
                source_ref: Some("seed".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "budget-chunk".to_owned(),
                node_logical_id: "budget".to_owned(),
                text_content: "budget meeting quarterly review notes".to_owned(),
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
        .expect("seed budget node");

    let rows = engine
        .query("Goal")
        .text_search("budget zzznonexistentterm", 10)
        .execute()
        .expect("search executes");

    assert!(
        rows.fallback_used,
        "relaxed branch must fire on strict miss"
    );
    assert!(
        !rows.hits.is_empty(),
        "relaxed branch must contribute at least one hit"
    );
    assert!(rows.relaxed_hit_count > 0);
    assert_eq!(rows.strict_hit_count, 0);
    assert!(!rows.was_degraded, "3-term plan should fit the cap");
    assert!(
        rows.hits
            .iter()
            .any(|h| matches!(h.match_mode, SearchMatchMode::Relaxed))
    );
}

#[test]
fn relaxed_branch_marks_was_degraded_when_cap_truncated_the_plan() {
    // Phase 3: a 5-term strict-miss query must fire the relaxed branch AND
    // mark was_degraded on the resulting SearchRows, because derive_relaxed
    // truncates at RELAXED_BRANCH_CAP = 4 alternatives.
    let (_db, engine) = open_engine();
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-budget".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "budget-row".to_owned(),
                logical_id: "budget".to_owned(),
                kind: "Goal".to_owned(),
                properties: r#"{"name":"budget goal"}"#.to_owned(),
                source_ref: Some("seed".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "budget-chunk".to_owned(),
                node_logical_id: "budget".to_owned(),
                text_content: "budget meeting quarterly review notes".to_owned(),
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
        .expect("seed budget node");

    // 5 terms, strict fails (zzznope is nowhere), relaxed fires and
    // truncates the alternatives list to 4 -> was_degraded = true.
    let rows = engine
        .query("Goal")
        .text_search("budget alpha bravo charlie zzznope", 10)
        .execute()
        .expect("search executes");

    assert!(rows.fallback_used);
    assert!(
        rows.was_degraded,
        "5-term relaxed plan must be marked degraded"
    );
    assert!(rows.relaxed_hit_count > 0);
}

#[test]
fn relaxed_branch_does_not_mark_was_degraded_when_plan_fits_cap() {
    // Phase 3: a 3-term strict-miss query fires the relaxed branch but the
    // derived OR fits under the 4-alternative cap, so was_degraded stays
    // false.
    let (_db, engine) = open_engine();
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-budget".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "budget-row".to_owned(),
                logical_id: "budget".to_owned(),
                kind: "Goal".to_owned(),
                properties: r#"{"name":"budget goal"}"#.to_owned(),
                source_ref: Some("seed".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "budget-chunk".to_owned(),
                node_logical_id: "budget".to_owned(),
                text_content: "budget meeting quarterly review notes".to_owned(),
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
        .expect("seed budget node");

    let rows = engine
        .query("Goal")
        .text_search("budget alpha zzznope", 10)
        .execute()
        .expect("search executes");

    assert!(rows.fallback_used);
    assert!(!rows.was_degraded, "3-term relaxed plan fits the cap");
    assert!(rows.relaxed_hit_count > 0);
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
    assert!(!rows.hits.is_empty(), "expected at least one Goal hit");
    for hit in &rows.hits {
        assert_eq!(hit.node.kind, "Goal");
    }
}

// --- Phase 4 integration tests -----------------------------------------

fn submit_simple_node(engine: &Engine, row_id: &str, logical_id: &str, kind: &str, props: &str) {
    engine
        .writer()
        .submit(WriteRequest {
            label: "phase4-seed".to_owned(),
            nodes: vec![NodeInsert {
                row_id: row_id.to_owned(),
                logical_id: logical_id.to_owned(),
                kind: kind.to_owned(),
                properties: props.to_owned(),
                source_ref: Some("phase4".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
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
        })
        .expect("submit");
}

#[test]
fn leaf_separator_is_hard_phrase_break_under_unicode61_porter() {
    // Phase 4: the recursive extractor concatenates scalar leaves with a
    // separator that must act as a hard phrase break under FTS5's
    // `porter unicode61 remove_diacritics 2` tokenizer. Register a
    // recursive schema and insert a node whose two leaves end in
    // "alpha" and start with "beta" respectively. A phrase query for
    // "alpha beta" (straddling the separator) must return zero hits,
    // while a phrase query for each token individually still hits.
    let (_db, engine) = open_engine();
    engine
        .register_fts_property_schema_with_entries(
            "Note",
            &[FtsPropertyPathSpec::recursive("$.body")],
            None,
            &[],
        )
        .expect("register recursive schema");

    submit_simple_node(
        &engine,
        "note-1-row",
        "note-1",
        "Note",
        r#"{"body":{"a":"leading alpha","b":"beta trailing"}}"#,
    );

    // Individual tokens must still hit.
    let rows = engine
        .query("Note")
        .text_search("alpha", 10)
        .execute()
        .expect("search alpha");
    assert!(!rows.hits.is_empty(), "individual token must hit");

    let rows = engine
        .query("Note")
        .text_search("beta", 10)
        .execute()
        .expect("search beta");
    assert!(!rows.hits.is_empty(), "individual token must hit");

    // The phrase "alpha beta" straddles the leaf separator and must NOT
    // match the concatenated blob.
    let rows = engine
        .query("Note")
        .text_search("\"alpha beta\"", 10)
        .execute()
        .expect("phrase search");
    assert!(
        rows.hits.is_empty(),
        "phrase straddling leaf separator must not match (got {} hits)",
        rows.hits.len()
    );
}

#[test]
fn recursive_schema_registration_triggers_eager_rebuild() {
    // Phase 4: when a scalar-only schema is later replaced by one that
    // adds a recursive path, the property FTS rows for that kind must be
    // rebuilt in the same transaction as the schema update — without any
    // additional node writes — and the position map for the recursive
    // leaves must be populated.
    let (_db, engine) = open_engine();

    // 1. Register scalar-only schema for "Note" and seed a node whose
    //    scalar-indexed property is discoverable via text search.
    engine
        .register_fts_property_schema("Note", &["$.title".to_owned()], None)
        .expect("register scalar schema");
    submit_simple_node(
        &engine,
        "note-1-row",
        "note-1",
        "Note",
        r#"{"title":"scalar-only-title","payload":{"inner":"recursive-only-word"}}"#,
    );

    let rows = engine
        .query("Note")
        .text_search("scalar-only-title", 10)
        .execute()
        .expect("scalar search");
    assert!(!rows.hits.is_empty(), "scalar-only schema must index title");

    let rows = engine
        .query("Note")
        .text_search("recursive-only-word", 10)
        .execute()
        .expect("search inner");
    assert!(
        rows.hits.is_empty(),
        "inner payload must NOT be indexed before recursive registration"
    );

    // Position map must be empty for a scalar-only schema.
    let conn = rusqlite::Connection::open(engine.coordinator().database_path())
        .expect("open for assertion");
    let pos_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM fts_node_property_positions WHERE kind = 'Note'",
            [],
            |row| row.get(0),
        )
        .expect("pos count");
    assert_eq!(pos_count, 0);
    drop(conn);

    // 2. Now register a schema with a recursive path for the same kind.
    //    No further node writes occur — the eager rebuild must make the
    //    inner payload discoverable.
    engine
        .register_fts_property_schema_with_entries(
            "Note",
            &[
                FtsPropertyPathSpec::scalar("$.title"),
                FtsPropertyPathSpec::recursive("$.payload"),
            ],
            None,
            &[],
        )
        .expect("register recursive schema");

    let rows = engine
        .query("Note")
        .text_search("recursive-only-word", 10)
        .execute()
        .expect("search after rebuild");
    assert!(
        !rows.hits.is_empty(),
        "eager rebuild must index inner payload without a write"
    );

    // Scalar search must still work.
    let rows = engine
        .query("Note")
        .text_search("scalar-only-title", 10)
        .execute()
        .expect("search title after rebuild");
    assert!(!rows.hits.is_empty());

    // Position map must now have rows for the recursive leaves.
    let conn = rusqlite::Connection::open(engine.coordinator().database_path())
        .expect("open for assertion");
    let pos_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM fts_node_property_positions WHERE kind = 'Note'",
            [],
            |row| row.get(0),
        )
        .expect("pos count");
    assert!(pos_count > 0, "position map must have rows after rebuild");
}

#[test]
fn recursive_schema_registration_is_transactional() {
    // Positive variant of the transaction test: register a recursive
    // schema over an already-seeded kind and assert that at a single
    // consistent read point both `fts_node_properties` and
    // `fts_node_property_positions` are updated in lockstep. The read
    // happens after the registration commit, so if the update had NOT
    // been transactional we could observe either table out of sync.
    let (_db, engine) = open_engine();

    engine
        .register_fts_property_schema("Doc", &["$.title".to_owned()], None)
        .expect("register initial schema");
    submit_simple_node(
        &engine,
        "doc-1-row",
        "doc-1",
        "Doc",
        r#"{"title":"hello","body":{"p1":"alpha","p2":"bravo"}}"#,
    );
    submit_simple_node(
        &engine,
        "doc-2-row",
        "doc-2",
        "Doc",
        r#"{"title":"world","body":{"p1":"charlie","p2":"delta"}}"#,
    );

    engine
        .register_fts_property_schema_with_entries(
            "Doc",
            &[
                FtsPropertyPathSpec::scalar("$.title"),
                FtsPropertyPathSpec::recursive("$.body"),
            ],
            None,
            &[],
        )
        .expect("eager recursive registration");

    let conn = rusqlite::Connection::open(engine.coordinator().database_path())
        .expect("open for assertion");

    // Every active Doc node must have a property FTS row.
    let prop_rows: i64 = conn
        .query_row(
            "SELECT count(*) FROM fts_node_properties WHERE kind = 'Doc'",
            [],
            |row| row.get(0),
        )
        .expect("prop count");
    assert_eq!(prop_rows, 2, "eager rebuild must emit one row per node");

    // Each Doc node must have exactly 2 position-map rows (p1 + p2 leaves).
    let pos_rows: i64 = conn
        .query_row(
            "SELECT count(*) FROM fts_node_property_positions WHERE kind = 'Doc'",
            [],
            |row| row.get(0),
        )
        .expect("pos count");
    assert_eq!(
        pos_rows, 4,
        "2 nodes × 2 recursive leaves = 4 position rows"
    );

    // Spot-check that each position-map entry points at a real leaf value.
    let text_doc1: String = conn
        .query_row(
            "SELECT text_content FROM fts_node_properties WHERE node_logical_id = 'doc-1'",
            [],
            |row| row.get(0),
        )
        .expect("doc-1 text");
    assert!(text_doc1.contains("alpha"));
    assert!(text_doc1.contains("bravo"));
}

#[test]
fn rebuild_from_canonical_regenerates_position_map() {
    // Phase 4: integrity repair / projection rebuild must regenerate the
    // position map from canonical state. Open → seed with recursive
    // schema → close → reopen → rebuild_projections(Fts) → assert
    // fts_node_property_positions matches the expected rebuilt state.
    let db = NamedTempFile::new().expect("temp db");
    {
        let engine = Engine::open(EngineOptions::new(db.path())).expect("open #1");
        engine
            .register_fts_property_schema_with_entries(
                "Doc",
                &[FtsPropertyPathSpec::recursive("$.body")],
                None,
                &[],
            )
            .expect("register recursive schema");
        submit_simple_node(
            &engine,
            "doc-1-row",
            "doc-1",
            "Doc",
            r#"{"body":{"p1":"alpha","p2":"bravo"}}"#,
        );
    }

    let engine = Engine::open(EngineOptions::new(db.path())).expect("open #2");
    // Drop the position-map rows to simulate drift; the rebuild must
    // regenerate them.
    {
        let conn = rusqlite::Connection::open(engine.coordinator().database_path())
            .expect("open for drift");
        conn.execute("DELETE FROM fts_node_property_positions", [])
            .expect("delete positions");
    }

    engine
        .admin()
        .service()
        .rebuild_projections(fathomdb::ProjectionTarget::Fts)
        .expect("rebuild projections");

    let conn = rusqlite::Connection::open(engine.coordinator().database_path())
        .expect("open for assertion");
    let pos_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM fts_node_property_positions WHERE kind = 'Doc'",
            [],
            |row| row.get(0),
        )
        .expect("pos count");
    assert_eq!(
        pos_count, 2,
        "projection rebuild must regenerate position map rows"
    );
}

// ---------------------------------------------------------------------------
// Phase 5: opt-in match-attribution tests.
// ---------------------------------------------------------------------------

fn register_recursive_payload_schema(engine: &Engine) {
    engine
        .register_fts_property_schema_with_entries(
            "Note",
            &[FtsPropertyPathSpec::recursive("$.payload")],
            None,
            &[],
        )
        .expect("register recursive schema");
}

#[test]
fn default_text_search_does_not_read_position_map_and_sets_attribution_none() {
    // Zero-cost proof: without `.with_match_attribution()`, every hit must
    // carry `attribution == None`. This is the default path and the Phase 4
    // position map must not contribute to the result.
    let (_db, engine) = open_engine();
    register_recursive_payload_schema(&engine);
    submit_simple_node(
        &engine,
        "note-default-row",
        "note-default",
        "Note",
        r#"{"payload":{"body":"shipping quarterly docs"}}"#,
    );

    let rows = engine
        .query("Note")
        .text_search("quarterly", 10)
        .execute()
        .expect("default search");

    assert!(!rows.hits.is_empty(), "expected at least one hit");
    for hit in &rows.hits {
        assert!(
            hit.attribution.is_none(),
            "default path must leave attribution None, got {:?}",
            hit.attribution
        );
    }
}

#[test]
fn attribution_resolves_stemmed_match_to_original_leaf() {
    // The porter stemmer collapses `ship` and `shipping` to the same stem.
    // FTS5 still records the original-text byte offset of `shipping`, so
    // the binary search into the position map lands on the leaf that
    // contains it.
    let (_db, engine) = open_engine();
    register_recursive_payload_schema(&engine);
    submit_simple_node(
        &engine,
        "note-stem-row",
        "note-stem",
        "Note",
        r#"{"payload":{"body":"shipping quarterly docs"}}"#,
    );

    let rows = engine
        .query("Note")
        .text_search("ship", 10)
        .with_match_attribution()
        .execute()
        .expect("attributed search");

    assert!(!rows.hits.is_empty());
    let hit = &rows.hits[0];
    let att = hit
        .attribution
        .as_ref()
        .expect("attribution populated when requested");
    assert_eq!(
        att.matched_paths,
        vec!["$.payload.body".to_owned()],
        "stemmed match must resolve to the originating leaf",
    );
}

#[test]
fn attribution_resolves_phrase_within_single_leaf() {
    let (_db, engine) = open_engine();
    register_recursive_payload_schema(&engine);
    submit_simple_node(
        &engine,
        "note-phrase-row",
        "note-phrase",
        "Note",
        r#"{"payload":{"body":"shipping quarterly docs"}}"#,
    );

    let rows = engine
        .query("Note")
        .text_search("\"quarterly docs\"", 10)
        .with_match_attribution()
        .execute()
        .expect("phrase search");

    assert!(!rows.hits.is_empty());
    let hit = &rows.hits[0];
    let att = hit.attribution.as_ref().expect("attribution populated");
    assert_eq!(att.matched_paths, vec!["$.payload.body".to_owned()]);
}

#[test]
fn attribution_phrase_does_not_straddle_leaves() {
    // Re-assert the Phase 4 leaf-separator invariant from the attribution
    // side: a phrase query "alpha beta" straddling two leaves returns no
    // hits, but an AND query (unquoted) returns a hit whose attribution
    // lists both leaves.
    let (_db, engine) = open_engine();
    register_recursive_payload_schema(&engine);
    submit_simple_node(
        &engine,
        "note-straddle-row",
        "note-straddle",
        "Note",
        r#"{"payload":{"a":"leading alpha","b":"beta trailing"}}"#,
    );

    // Phrase query must not match across the leaf separator.
    let rows = engine
        .query("Note")
        .text_search("\"alpha beta\"", 10)
        .with_match_attribution()
        .execute()
        .expect("phrase search");
    assert!(
        rows.hits.is_empty(),
        "phrase must not straddle leaf separator"
    );

    // The AND form should still return a hit whose attribution lists both
    // leaves (in first-match-offset order).
    let rows = engine
        .query("Note")
        .text_search("alpha beta", 10)
        .with_match_attribution()
        .execute()
        .expect("AND search");
    assert!(!rows.hits.is_empty(), "AND form must still match");
    let hit = &rows.hits[0];
    let att = hit.attribution.as_ref().expect("attribution populated");
    assert!(
        att.matched_paths.contains(&"$.payload.a".to_owned()),
        "expected $.payload.a in {:?}",
        att.matched_paths,
    );
    assert!(
        att.matched_paths.contains(&"$.payload.b".to_owned()),
        "expected $.payload.b in {:?}",
        att.matched_paths,
    );
    // First-match-offset order: $.payload.a (leading alpha) precedes
    // $.payload.b (beta trailing) in the blob.
    let idx_a = att
        .matched_paths
        .iter()
        .position(|p| p == "$.payload.a")
        .expect("a present");
    let idx_b = att
        .matched_paths
        .iter()
        .position(|p| p == "$.payload.b")
        .expect("b present");
    assert!(
        idx_a < idx_b,
        "first-match order: a must precede b, got {:?}",
        att.matched_paths,
    );
}

#[test]
fn attribution_ignores_not_clauses() {
    // A `NOT` clause contributes no positive match positions, so the
    // attribution vector only records the positive term's leaf.
    //
    // NOTE (P5-2 review): the stronger invariant would seed the NOT target
    // in a *second* indexed leaf and assert only the positive leaf is
    // attributed. Under FTS5's full-document NOT semantics, any row whose
    // indexed text contains the NOT term is rejected outright — so
    // seeding `archive` into `$.payload.notes` (also recursively indexed)
    // would simply drop the row and the test would be vacuous. We keep
    // this weaker check and rely on the "NOT clauses contribute no
    // matched_paths" invariant being enforced at the offset-resolution
    // level rather than via cross-leaf construction.
    let (_db, engine) = open_engine();
    register_recursive_payload_schema(&engine);
    submit_simple_node(
        &engine,
        "note-not-row",
        "note-not",
        "Note",
        r#"{"payload":{"title":"budget plan","notes":"unrelated text"}}"#,
    );

    let rows = engine
        .query("Note")
        .text_search("budget NOT archive", 10)
        .with_match_attribution()
        .execute()
        .expect("NOT search");

    assert!(!rows.hits.is_empty());
    let hit = &rows.hits[0];
    let att = hit.attribution.as_ref().expect("attribution populated");
    assert_eq!(
        att.matched_paths,
        vec!["$.payload.title".to_owned()],
        "NOT clause must not contribute paths",
    );
}

#[test]
fn attribution_multi_term_and_across_leaves_returns_multiple_paths() {
    let (_db, engine) = open_engine();
    register_recursive_payload_schema(&engine);
    submit_simple_node(
        &engine,
        "note-multi-row",
        "note-multi",
        "Note",
        // Keys are walked in alphabetical order by the recursive extractor,
        // so $.payload.aaa precedes $.payload.bbb in the blob. Put "budget"
        // in the earlier leaf and "archive" in the later one.
        r#"{"payload":{"aaa":"budget plan","bbb":"archive folder"}}"#,
    );

    let rows = engine
        .query("Note")
        .text_search("budget archive", 10)
        .with_match_attribution()
        .execute()
        .expect("multi-term AND search");

    assert!(!rows.hits.is_empty());
    let hit = &rows.hits[0];
    let att = hit.attribution.as_ref().expect("attribution populated");
    assert!(att.matched_paths.contains(&"$.payload.aaa".to_owned()));
    assert!(att.matched_paths.contains(&"$.payload.bbb".to_owned()));
    let idx_a = att
        .matched_paths
        .iter()
        .position(|p| p == "$.payload.aaa")
        .expect("aaa");
    let idx_b = att
        .matched_paths
        .iter()
        .position(|p| p == "$.payload.bbb")
        .expect("bbb");
    assert!(
        idx_a < idx_b,
        "first-match order: aaa must precede bbb, got {:?}",
        att.matched_paths,
    );
}

#[test]
fn attribution_works_under_relaxed_branch() {
    // Strict fails (the second term does not appear), but relaxed recovers
    // via the OR branch. The recovered hit must carry populated attribution
    // and be tagged Relaxed.
    let (_db, engine) = open_engine();
    register_recursive_payload_schema(&engine);
    submit_simple_node(
        &engine,
        "note-relaxed-row",
        "note-relaxed",
        "Note",
        r#"{"payload":{"body":"budget meeting notes"}}"#,
    );

    let rows = engine
        .query("Note")
        .text_search("budget zzznonexistentterm", 10)
        .with_match_attribution()
        .execute()
        .expect("relaxed search");

    assert!(rows.fallback_used, "relaxed must fire on strict miss");
    assert!(!rows.hits.is_empty());
    let hit = rows
        .hits
        .iter()
        .find(|h| matches!(h.match_mode, SearchMatchMode::Relaxed))
        .expect("at least one relaxed hit");
    let att = hit
        .attribution
        .as_ref()
        .expect("attribution populated on relaxed hit");
    assert_eq!(att.matched_paths, vec!["$.payload.body".to_owned()]);
}

#[test]
fn attribution_empty_for_chunk_only_hit() {
    // Chunk hits have no leaf structure — with attribution on, they carry
    // an empty `matched_paths` vector (not `None`), so callers can
    // distinguish "asked for and this hit doesn't qualify" from "not
    // asked for."
    let (_db, engine) = open_engine();
    // Do NOT register a property FTS schema — the Goal kind has no
    // recursive/property index, so the only search surface is the chunk
    // index.
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-chunk".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "chunk-hit-row".to_owned(),
                logical_id: "chunk-hit".to_owned(),
                kind: "Goal".to_owned(),
                properties: r#"{"name":"ignored"}"#.to_owned(),
                source_ref: Some("seed".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "chunk-hit-chunk".to_owned(),
                node_logical_id: "chunk-hit".to_owned(),
                text_content: "unique-chunk-sentinel phrase in this chunk".to_owned(),
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
        .expect("seed chunk-only node");

    let rows = engine
        .query("Goal")
        .text_search("unique-chunk-sentinel", 10)
        .with_match_attribution()
        .execute()
        .expect("chunk search");

    assert!(!rows.hits.is_empty());
    let hit = &rows.hits[0];
    assert!(matches!(hit.source, SearchHitSource::Chunk));
    assert_eq!(
        hit.attribution,
        Some(HitAttribution {
            matched_paths: Vec::new(),
        }),
        "chunk hit must carry present-but-empty attribution",
    );
}

#[test]
fn attribution_populated_for_every_hit_when_flag_on() {
    // With attribution on, every hit — chunk or property — carries
    // `attribution.is_some()`. The dedup step keeps one hit per logical_id
    // preferring chunk over property, so we seed two distinct nodes: one
    // whose match lives in a chunk and one whose match lives in a
    // recursive property leaf. Both nodes survive dedup and both hits
    // must have populated attribution.
    let (_db, engine) = open_engine();
    register_recursive_payload_schema(&engine);
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-mixed".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "prop-only-row".to_owned(),
                    logical_id: "prop-only".to_owned(),
                    kind: "Note".to_owned(),
                    properties: r#"{"payload":{"body":"budget summary only"}}"#.to_owned(),
                    source_ref: Some("seed".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "chunk-only-row".to_owned(),
                    logical_id: "chunk-only".to_owned(),
                    kind: "Note".to_owned(),
                    // No `payload`, so the recursive schema extracts
                    // nothing — the only way this node matches "budget"
                    // is via its chunk text.
                    properties: r#"{"title":"ignored-scalar"}"#.to_owned(),
                    source_ref: Some("seed".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "chunk-only-chunk".to_owned(),
                node_logical_id: "chunk-only".to_owned(),
                text_content: "the quarterly budget summary for the team".to_owned(),
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
        .expect("seed mixed nodes");

    let rows = engine
        .query("Note")
        .text_search("budget", 10)
        .with_match_attribution()
        .execute()
        .expect("search");

    assert!(
        rows.hits.len() >= 2,
        "expected both hits, got {:#?}",
        rows.hits
    );
    let mut saw_property_path = false;
    let mut saw_chunk_empty = false;
    for hit in &rows.hits {
        assert!(
            hit.attribution.is_some(),
            "every hit must have attribution when the flag is on",
        );
        match hit.source {
            SearchHitSource::Property => {
                let att = hit.attribution.as_ref().expect("attribution some");
                assert_eq!(att.matched_paths, vec!["$.payload.body".to_owned()]);
                saw_property_path = true;
            }
            SearchHitSource::Chunk => {
                let att = hit.attribution.as_ref().expect("attribution some");
                assert!(
                    att.matched_paths.is_empty(),
                    "chunk hit attribution must be empty, got {:?}",
                    att.matched_paths,
                );
                saw_chunk_empty = true;
            }
            SearchHitSource::Vector => {}
        }
    }
    assert!(saw_property_path, "must see at least one property hit");
    assert!(saw_chunk_empty, "must see at least one chunk hit");
}

// --- Phase 6 integration tests: fallback_search ------------------------

fn seed_budget_goal(engine: &Engine) {
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-budget".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "budget-alpha-row".to_owned(),
                    logical_id: "budget-alpha".to_owned(),
                    kind: "Goal".to_owned(),
                    properties: r#"{"name":"budget alpha goal"}"#.to_owned(),
                    source_ref: Some("seed".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "budget-bravo-row".to_owned(),
                    logical_id: "budget-bravo".to_owned(),
                    kind: "Goal".to_owned(),
                    properties: r#"{"name":"budget bravo goal"}"#.to_owned(),
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
                    id: "budget-alpha-chunk".to_owned(),
                    node_logical_id: "budget-alpha".to_owned(),
                    text_content: "alpha budget quarterly review notes".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "budget-bravo-chunk".to_owned(),
                    node_logical_id: "budget-bravo".to_owned(),
                    text_content: "bravo budget annual summary notes".to_owned(),
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
        .expect("seed budget nodes");
}

#[test]
fn fallback_search_strict_only_returns_same_shape_as_two_shape_path() {
    let (_db, engine) = open_engine();
    seed_budget_goal(&engine);

    let strict_only = engine
        .fallback_search("budget", None::<String>, 10)
        .filter_kind_eq("Goal")
        .execute()
        .expect("strict-only fallback");

    assert!(!strict_only.hits.is_empty(), "expected at least one hit");
    assert!(
        strict_only
            .hits
            .iter()
            .all(|h| matches!(h.match_mode, SearchMatchMode::Strict)),
        "strict-only must return only Strict hits",
    );
    assert_eq!(strict_only.strict_hit_count, strict_only.hits.len());
    assert_eq!(strict_only.relaxed_hit_count, 0);
    assert!(!strict_only.fallback_used);
    assert!(!strict_only.was_degraded);

    let two_shape = engine
        .fallback_search("budget", Some("budget OR nonexistent"), 10)
        .filter_kind_eq("Goal")
        .execute()
        .expect("two-shape fallback with non-firing relaxed");

    // With strict finding hits, relaxed must not run — result must match
    // the strict-only form field-by-field.
    assert_eq!(strict_only, two_shape);
}

#[test]
fn fallback_search_two_shape_reuses_adaptive_merge_rules() {
    let (_db, engine) = open_engine();
    seed_budget_goal(&engine);

    // Strict ANDs two nonexistent terms => zero hits; relaxed "budget OR
    // nothing" matches the seeded nodes via the OR branch.
    let rows = engine
        .fallback_search(
            "zzznonexistent1 zzznonexistent2",
            Some("budget OR nothing"),
            10,
        )
        .filter_kind_eq("Goal")
        .execute()
        .expect("two-shape fallback executes");

    assert!(rows.fallback_used, "relaxed must fire on strict miss");
    assert!(!rows.hits.is_empty());
    assert_eq!(rows.strict_hit_count, 0);
    assert_eq!(rows.relaxed_hit_count, rows.hits.len());
    assert!(!rows.was_degraded);
    for hit in &rows.hits {
        assert!(
            matches!(hit.match_mode, SearchMatchMode::Relaxed),
            "every hit must be tagged Relaxed",
        );
    }
}

#[test]
fn fallback_search_populates_per_block_counts() {
    // With FALLBACK_TRIGGER_K = 1, relaxed only fires when strict returns
    // zero. To exercise the merge path with BOTH blocks present, drive the
    // shared merge helper via the strict-miss case and assert block
    // ordering + counts on the resulting SearchRows.
    let (_db, engine) = open_engine();
    seed_budget_goal(&engine);

    // Strict miss => relaxed fires; relaxed matches both seeded nodes.
    let rows = engine
        .fallback_search("zzznope1 zzznope2", Some("budget OR alpha OR bravo"), 10)
        .filter_kind_eq("Goal")
        .execute()
        .expect("merge path executes");

    assert!(rows.fallback_used);
    assert!(rows.hits.len() >= 2, "expected both seeded nodes");
    // Strict block is empty so all hits are Relaxed.
    assert_eq!(rows.strict_hit_count, 0);
    assert_eq!(rows.relaxed_hit_count, rows.hits.len());
    // Relaxed hits must be ordered by score descending.
    for pair in rows.hits.windows(2) {
        assert!(
            pair[0].score >= pair[1].score,
            "relaxed block must be score-desc ordered",
        );
    }
}

#[test]
fn fallback_search_respects_filter_kind_eq() {
    let (_db, engine) = open_engine();
    seed_budget_goal(&engine);
    // Seed a non-Goal node with a matching chunk to make sure
    // filter_kind_eq excludes it.
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-other".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "note-budget-row".to_owned(),
                logical_id: "note-budget".to_owned(),
                kind: "Note".to_owned(),
                properties: r#"{"title":"budget note"}"#.to_owned(),
                source_ref: Some("seed".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "note-budget-chunk".to_owned(),
                node_logical_id: "note-budget".to_owned(),
                text_content: "budget thoughts note".to_owned(),
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
        .expect("seed note");

    let rows = engine
        .fallback_search("budget", Some("budget"), 10)
        .filter_kind_eq("Goal")
        .execute()
        .expect("filtered fallback executes");

    assert!(!rows.hits.is_empty());
    assert!(rows.hits.len() <= 10);
    for hit in &rows.hits {
        assert_eq!(
            hit.node.kind, "Goal",
            "filter_kind_eq must exclude non-Goal"
        );
    }
}

#[test]
fn fallback_search_with_match_attribution_populates_leaves() {
    let (_db, engine) = open_engine();
    register_recursive_payload_schema(&engine);
    submit_simple_node(
        &engine,
        "note-att-row",
        "note-att",
        "Note",
        r#"{"payload":{"body":"budget quarterly notes"}}"#,
    );

    let rows = engine
        .fallback_search("budget", Some("budget OR nothing"), 10)
        .filter_kind_eq("Note")
        .with_match_attribution()
        .execute()
        .expect("fallback attribution search");

    assert!(!rows.hits.is_empty());
    let hit = rows
        .hits
        .iter()
        .find(|h| matches!(h.source, SearchHitSource::Property))
        .expect("expected a property hit");
    let att = hit
        .attribution
        .as_ref()
        .expect("attribution must be populated");
    assert_eq!(att.matched_paths, vec!["$.payload.body".to_owned()]);
}

#[test]
fn fallback_search_strict_only_matches_two_shape_when_relaxed_never_fires() {
    let (_db, engine) = open_engine();
    seed_budget_goal(&engine);

    let strict_only = engine
        .fallback_search("budget", None::<String>, 10)
        .filter_kind_eq("Goal")
        .execute()
        .expect("strict-only");
    let two_shape = engine
        .fallback_search("budget", Some("budget OR zzznothing"), 10)
        .filter_kind_eq("Goal")
        .execute()
        .expect("two-shape non-firing relaxed");

    // Field-by-field equality.
    assert_eq!(strict_only.hits, two_shape.hits);
    assert_eq!(strict_only.strict_hit_count, two_shape.strict_hit_count);
    assert_eq!(strict_only.relaxed_hit_count, two_shape.relaxed_hit_count);
    assert_eq!(strict_only.fallback_used, two_shape.fallback_used);
    assert_eq!(strict_only.was_degraded, two_shape.was_degraded);
    assert_eq!(strict_only, two_shape);
}

#[test]
fn fallback_search_does_not_apply_relaxed_branch_cap() {
    let (_db, engine) = open_engine();
    // Seed six distinct terms a..f so the relaxed branch can match them.
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-terms".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "terms-row".to_owned(),
                logical_id: "terms".to_owned(),
                kind: "Goal".to_owned(),
                properties: r#"{"name":"terms goal"}"#.to_owned(),
                source_ref: Some("seed".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "terms-chunk".to_owned(),
                node_logical_id: "terms".to_owned(),
                text_content: "alpha bravo charlie delta echo foxtrot".to_owned(),
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
        .expect("seed terms");

    // 6-term relaxed shape — exceeds RELAXED_BRANCH_CAP = 4. `derive_relaxed`
    // would truncate and set was_degraded; fallback_search must NOT.
    let rows = engine
        .fallback_search(
            "nonexistent_strict",
            Some("alpha OR bravo OR charlie OR delta OR echo OR foxtrot"),
            10,
        )
        .filter_kind_eq("Goal")
        .execute()
        .expect("6-term relaxed executes");

    assert!(rows.fallback_used);
    assert!(!rows.hits.is_empty());
    assert!(
        !rows.was_degraded,
        "caller-provided relaxed shape must NOT be subject to the 4-alternative cap",
    );
}

// --- Pack FX review-findings tests -------------------------------------

#[test]
fn property_fts_rebuilds_after_crash_recovery_state() {
    // P2-1 regression: verify the property-FTS rebuild guard catches the
    // crash-recovery state in which `fts_property_schemas` is non-empty
    // but `fts_node_properties` was left empty (e.g. migration 16 applied
    // in a prior open but the rebuild did not commit).
    let db = NamedTempFile::new().expect("temporary db");
    let db_path = db.path().to_path_buf();

    {
        let engine = Engine::open(EngineOptions::new(&db_path)).expect("first open");
        register_recursive_payload_schema(&engine);
        submit_simple_node(
            &engine,
            "note-crash-row",
            "note-crash",
            "Note",
            r#"{"payload":{"body":"quarterly budget notes"}}"#,
        );
        let rows = engine
            .query("Note")
            .text_search("budget", 10)
            .execute()
            .expect("initial search");
        assert!(!rows.hits.is_empty(), "initial search must see the node");
        drop(engine);
    }

    // Simulate crash-recovery: delete all fts_node_properties rows via a
    // direct rusqlite connection, leaving fts_property_schemas intact.
    {
        let conn = rusqlite::Connection::open(&db_path).expect("raw conn");
        conn.execute("DELETE FROM fts_node_properties", [])
            .expect("delete fts rows");
        conn.execute("DELETE FROM fts_node_property_positions", [])
            .expect("delete positions");
    }

    // Re-open the engine — the open-time guard must repopulate the index.
    let engine = Engine::open(EngineOptions::new(&db_path)).expect("second open");
    let rows = engine
        .query("Note")
        .text_search("budget", 10)
        .execute()
        .expect("post-recovery search");
    assert!(
        !rows.hits.is_empty(),
        "open-time rebuild must have repopulated property FTS",
    );
}

#[test]
fn eager_rebuild_does_not_duplicate_sibling_kind_rows() {
    // P4-1 regression: registering a new recursive schema on kind B must
    // not re-insert FTS rows for kind A (whose schema is untouched).
    let (_db, engine) = open_engine();

    // Kind A: scalar-only schema.
    engine
        .register_fts_property_schema_with_entries(
            "AlphaKind",
            &[FtsPropertyPathSpec::scalar("$.title")],
            None,
            &[],
        )
        .expect("register alpha");
    // Kind B: initial recursive schema.
    engine
        .register_fts_property_schema_with_entries(
            "BetaKind",
            &[FtsPropertyPathSpec::recursive("$.body")],
            None,
            &[],
        )
        .expect("register beta initial");

    for i in 0..3 {
        submit_simple_node(
            &engine,
            &format!("alpha-{i}-row"),
            &format!("alpha-{i}"),
            "AlphaKind",
            &format!(r#"{{"title":"alpha target {i}"}}"#),
        );
        submit_simple_node(
            &engine,
            &format!("beta-{i}-row"),
            &format!("beta-{i}"),
            "BetaKind",
            &format!(r#"{{"body":{{"text":"beta target {i}"}}}}"#),
        );
    }

    let alpha_before = engine
        .query("AlphaKind")
        .text_search("target", 10)
        .execute()
        .expect("alpha search");
    assert!(!alpha_before.hits.is_empty(), "alpha must have hits");
    let alpha_hit_count = alpha_before.hits.len();
    let beta_before = engine
        .query("BetaKind")
        .text_search("target", 10)
        .execute()
        .expect("beta search");
    assert!(!beta_before.hits.is_empty(), "beta must have hits");

    // Count raw fts_node_properties rows for AlphaKind pre-rebuild.
    let db_path = engine.coordinator().database_path().to_path_buf();
    let count_alpha_rows = || -> i64 {
        let conn = rusqlite::Connection::open(&db_path).expect("raw conn");
        conn.query_row(
            "SELECT COUNT(*) FROM fts_node_properties WHERE kind = ?1",
            ["AlphaKind"],
            |r| r.get(0),
        )
        .expect("count query")
    };
    let alpha_rows_before = count_alpha_rows();

    // Register a NEW recursive schema on BetaKind — triggers eager rebuild.
    engine
        .register_fts_property_schema_with_entries(
            "BetaKind",
            &[FtsPropertyPathSpec::recursive("$.body")],
            Some(" | "),
            &[],
        )
        .expect("re-register beta");

    let alpha_rows_after = count_alpha_rows();
    assert_eq!(
        alpha_rows_before, alpha_rows_after,
        "AlphaKind fts rows must not be duplicated by a BetaKind rebuild",
    );

    let alpha_after = engine
        .query("AlphaKind")
        .text_search("target", 10)
        .execute()
        .expect("alpha post-rebuild");
    assert_eq!(
        alpha_after.hits.len(),
        alpha_hit_count,
        "alpha hit count must survive sibling-kind rebuild unchanged",
    );

    let beta_after = engine
        .query("BetaKind")
        .text_search("target", 10)
        .execute()
        .expect("beta post-rebuild");
    assert!(
        !beta_after.hits.is_empty(),
        "beta must still have hits after rebuild with new separator",
    );
}

#[test]
fn text_search_empty_query_returns_empty_search_rows() {
    // P1-1: an empty or whitespace-only query parses to TextQuery::Empty,
    // which would otherwise yield a raw FTS5 syntax error. The coordinator
    // must short-circuit to an empty SearchRows instead.
    let (_db, engine) = open_engine();
    seed_goals(&engine);

    let rows = engine
        .query("Goal")
        .text_search("", 10)
        .execute()
        .expect("empty query must not error");
    assert!(rows.hits.is_empty());
    assert_eq!(rows.strict_hit_count, 0);
    assert_eq!(rows.relaxed_hit_count, 0);
    assert!(!rows.fallback_used);
    assert!(!rows.was_degraded);

    let rows_ws = engine
        .query("Goal")
        .text_search("   ", 10)
        .execute()
        .expect("whitespace-only query must not error");
    assert!(rows_ws.hits.is_empty());
    assert_eq!(rows_ws.strict_hit_count, 0);
    assert_eq!(rows_ws.relaxed_hit_count, 0);
    assert!(!rows_ws.fallback_used);
    assert!(!rows_ws.was_degraded);
}

#[test]
fn strict_hit_with_many_terms_leaves_was_degraded_false() {
    // P3-1: a 5+-term implicit-AND strict hit must not set `was_degraded`,
    // because the relaxed branch never runs when strict is non-empty.
    let (_db, engine) = open_engine();
    engine
        .register_fts_property_schema(
            "Goal",
            &["$.name".to_owned(), "$.description".to_owned()],
            None,
        )
        .expect("register schema");
    submit_simple_node(
        &engine,
        "goal-many-row",
        "goal-many",
        "Goal",
        r#"{"name":"alpha beta gamma","description":"delta epsilon review"}"#,
    );

    let rows = engine
        .query("Goal")
        .text_search("alpha beta gamma delta epsilon", 10)
        .execute()
        .expect("5-term strict search");
    assert!(!rows.hits.is_empty(), "expected a strict match");
    assert!(!rows.fallback_used, "relaxed must not fire on strict hit");
    assert!(
        !rows.was_degraded,
        "was_degraded must be false on strict hit"
    );
    assert_eq!(rows.relaxed_hit_count, 0);
    for hit in &rows.hits {
        assert!(matches!(hit.match_mode, SearchMatchMode::Strict));
    }
}

#[test]
fn exclude_paths_suppresses_subtree() {
    // P4-2: exact-path match in `exclude_paths` on an object node
    // effectively suppresses the subtree rooted there.
    let (_db, engine) = open_engine();
    engine
        .register_fts_property_schema_with_entries(
            "Note",
            &[FtsPropertyPathSpec::recursive("$.payload")],
            None,
            &["$.payload.priv".to_owned()],
        )
        .expect("register recursive with excludes");
    submit_simple_node(
        &engine,
        "note-excl-row",
        "note-excl",
        "Note",
        r#"{"payload":{"pub":{"a":"alpha","b":"bravo"},"priv":{"x":"xray","y":"yankee"}}}"#,
    );

    let rows_alpha = engine
        .query("Note")
        .text_search("alpha", 10)
        .execute()
        .expect("alpha search");
    assert!(!rows_alpha.hits.is_empty(), "alpha must be indexed");

    let rows_xray = engine
        .query("Note")
        .text_search("xray", 10)
        .execute()
        .expect("xray search");
    assert!(
        rows_xray.hits.is_empty(),
        "xray must be excluded via $.payload.priv",
    );

    let rows_yankee = engine
        .query("Note")
        .text_search("yankee", 10)
        .execute()
        .expect("yankee search");
    assert!(
        rows_yankee.hits.is_empty(),
        "yankee must be excluded via $.payload.priv",
    );
}

#[test]
fn fallback_search_strict_only_matches_text_search_strict_only() {
    // P6-4: the adaptive text_search path and the narrow fallback_search
    // helper must produce field-by-field identical SearchRows when neither
    // path fires its relaxed branch.
    let (_db, engine) = open_engine();
    seed_budget_goal(&engine);

    let a = engine
        .query("Goal")
        .text_search("budget", 10)
        .execute()
        .expect("adaptive strict-only");
    let b = engine
        .fallback_search("budget", None::<&str>, 10)
        .filter_kind_eq("Goal")
        .execute()
        .expect("fallback strict-only");

    assert_eq!(a.hits.len(), b.hits.len(), "hit counts must match");
    assert_eq!(a.strict_hit_count, b.strict_hit_count);
    assert_eq!(a.relaxed_hit_count, b.relaxed_hit_count);
    assert_eq!(a.fallback_used, b.fallback_used);
    assert_eq!(a.was_degraded, b.was_degraded);

    // Pair hits by logical_id for field-by-field equality.
    for a_hit in &a.hits {
        let b_hit = b
            .hits
            .iter()
            .find(|h| h.node.logical_id == a_hit.node.logical_id)
            .expect("matching hit in fallback result");
        assert!(
            (a_hit.score - b_hit.score).abs() < f64::EPSILON,
            "scores must match: adaptive={} fallback={}",
            a_hit.score,
            b_hit.score,
        );
        assert_eq!(a_hit.source, b_hit.source, "sources must match");
        assert_eq!(a_hit.match_mode, b_hit.match_mode);
        assert_eq!(a_hit.snippet, b_hit.snippet);
        assert_eq!(a_hit.written_at, b_hit.written_at);
        assert_eq!(a_hit.projection_row_id, b_hit.projection_row_id);
    }
}

#[test]
fn text_search_dedups_same_node_across_chunk_and_property() {
    // P1-2 verification: a node whose content matches BOTH a chunk and a
    // recursive property leaf must surface as a single hit (dedup by
    // logical_id). Source priority is chunk > property in the Phase 3
    // dedup pass, so the surviving hit must be a chunk hit.
    let (_db, engine) = open_engine();
    register_recursive_payload_schema(&engine);
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-dual".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "note-dual-row".to_owned(),
                logical_id: "note-dual".to_owned(),
                kind: "Note".to_owned(),
                properties: r#"{"payload":{"body":"the dualmatch term appears here"}}"#.to_owned(),
                source_ref: Some("seed".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "note-dual-chunk".to_owned(),
                node_logical_id: "note-dual".to_owned(),
                text_content: "the dualmatch term also appears in this chunk".to_owned(),
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
        .expect("seed dual-match node");

    let rows = engine
        .query("Note")
        .text_search("dualmatch", 10)
        .execute()
        .expect("dedup search");
    assert_eq!(
        rows.hits.len(),
        1,
        "same logical_id must appear exactly once across chunk+property",
    );
    assert!(matches!(rows.hits[0].source, SearchHitSource::Chunk));
}
