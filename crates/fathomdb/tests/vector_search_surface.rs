#![allow(clippy::expect_used, clippy::missing_panics_doc, deprecated)]

//! Phase 11 integration tests for the tethered `VectorSearchBuilder`
//! surface. These exercise the standalone vector retrieval path through
//! `engine.query(kind).vector_search(query, limit).execute()`.
//!
//! The seeding/execution tests that require real vector KNN scans are
//! gated per-test behind `#[cfg(feature = "sqlite-vec")]`. The
//! capability-miss degradation test runs unconditionally (it relies on
//! the absence of `vec_nodes_active` at query time, which happens both
//! when the feature is off and when the engine is opened without a
//! vector dimension).

#[cfg(feature = "sqlite-vec")]
use fathomdb::{
    ChunkInsert, ChunkPolicy, NodeInsert, RetrievalModality, SearchHitSource, VecInsert,
    WriteRequest,
};
use fathomdb::{Engine, EngineOptions};
use tempfile::NamedTempFile;

#[cfg(feature = "sqlite-vec")]
const DIM: usize = 4;

/// Capability-miss: when the engine is opened without a vector dimension
/// the `vec_nodes_active` virtual table is not created, so any vector
/// search must surface as an empty `SearchRows` with `was_degraded = true`
/// rather than propagating a SQL error. This holds both when the
/// `sqlite-vec` feature is disabled (the extension is not loaded at all)
/// and when it is enabled but no dimension was set at `Engine::open` time,
/// so the test runs unconditionally.
#[test]
fn vector_search_capability_miss_returns_empty_degraded() {
    let db = NamedTempFile::new().expect("temporary db");
    // Intentionally do NOT set vector_dimension, so vec_nodes_active is
    // never created regardless of whether the sqlite-vec feature is on.
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    let rows = engine
        .query("Document")
        .vector_search("[0.1, 0.2, 0.3, 0.4]", 5)
        .execute()
        .expect("vector_search capability miss must not error");

    assert!(rows.hits.is_empty());
    assert!(
        rows.was_degraded,
        "capability miss must surface as was_degraded=true"
    );
    assert_eq!(rows.vector_hit_count, 0);
    assert_eq!(rows.strict_hit_count, 0);
    assert_eq!(rows.relaxed_hit_count, 0);
    assert!(!rows.fallback_used);
}

#[cfg(feature = "sqlite-vec")]
fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let mut opts = EngineOptions::new(db.path());
    opts.vector_dimension = Some(DIM);
    let engine = Engine::open(opts).expect("engine opens with vec");
    assert!(
        engine.coordinator().vector_enabled(),
        "vector must be enabled after setting dimension"
    );
    (db, engine)
}

#[cfg(feature = "sqlite-vec")]
#[allow(clippy::too_many_lines)]
fn seed_docs(engine: &Engine) {
    // Three Documents + one Goal, each with a distinct 4-dim embedding.
    // We use unit-length-ish vectors differing across dimensions so the
    // vec0 distance ordering is unambiguous.
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-docs".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "doc-near-row".to_owned(),
                    logical_id: "doc-near".to_owned(),
                    kind: "Document".to_owned(),
                    properties: r#"{"title":"Ship quarterly docs","status":"active"}"#.to_owned(),
                    source_ref: Some("seed".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "doc-mid-row".to_owned(),
                    logical_id: "doc-mid".to_owned(),
                    kind: "Document".to_owned(),
                    properties: r#"{"title":"Draft roadmap","status":"active"}"#.to_owned(),
                    source_ref: Some("seed".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "doc-far-row".to_owned(),
                    logical_id: "doc-far".to_owned(),
                    kind: "Document".to_owned(),
                    properties: r#"{"title":"Archive old plans","status":"archived"}"#.to_owned(),
                    source_ref: Some("seed".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "goal-quarterly-row".to_owned(),
                    logical_id: "goal-quarterly".to_owned(),
                    kind: "Goal".to_owned(),
                    properties: r#"{"name":"Ship quarterly docs"}"#.to_owned(),
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
                    id: "chunk-doc-near".to_owned(),
                    node_logical_id: "doc-near".to_owned(),
                    text_content: "quarterly docs".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "chunk-doc-mid".to_owned(),
                    node_logical_id: "doc-mid".to_owned(),
                    text_content: "draft roadmap".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "chunk-doc-far".to_owned(),
                    node_logical_id: "doc-far".to_owned(),
                    text_content: "archive old plans".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "chunk-goal-quarterly".to_owned(),
                    node_logical_id: "goal-quarterly".to_owned(),
                    text_content: "quarterly goal".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
            ],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![
                VecInsert {
                    chunk_id: "chunk-doc-near".to_owned(),
                    embedding: vec![1.0, 0.0, 0.0, 0.0],
                },
                VecInsert {
                    chunk_id: "chunk-doc-mid".to_owned(),
                    embedding: vec![0.5, 0.5, 0.0, 0.0],
                },
                VecInsert {
                    chunk_id: "chunk-doc-far".to_owned(),
                    embedding: vec![0.0, 0.0, 1.0, 0.0],
                },
                VecInsert {
                    chunk_id: "chunk-goal-quarterly".to_owned(),
                    embedding: vec![0.9, 0.1, 0.0, 0.0],
                },
            ],
            operational_writes: vec![],
        })
        .expect("seed docs");
}

#[cfg(feature = "sqlite-vec")]
#[test]
fn vector_search_execute_returns_search_rows_with_vector_fields() {
    let (_db, engine) = open_engine();
    seed_docs(&engine);

    let rows = engine
        .query("Document")
        .vector_search("[1.0, 0.0, 0.0, 0.0]", 5)
        .execute()
        .expect("vector search executes");

    assert!(!rows.hits.is_empty(), "expected at least one hit");
    assert_eq!(rows.vector_hit_count, rows.hits.len());
    assert_eq!(rows.strict_hit_count, 0);
    assert_eq!(rows.relaxed_hit_count, 0);
    assert!(!rows.fallback_used);
    assert!(!rows.was_degraded);

    for hit in &rows.hits {
        assert_eq!(hit.modality, RetrievalModality::Vector);
        assert_eq!(hit.source, SearchHitSource::Vector);
        assert!(
            hit.match_mode.is_none(),
            "vector hits must not carry a strict/relaxed match_mode"
        );
        assert!(hit.vector_distance.is_some());
        assert!(hit.snippet.is_none(), "vector hits have no snippet");
        assert!(hit.attribution.is_none(), "default path: no attribution");
        assert!(hit.written_at > 0, "written_at must be populated");
    }
}

#[cfg(feature = "sqlite-vec")]
#[test]
fn vector_search_score_is_negated_distance() {
    let (_db, engine) = open_engine();
    seed_docs(&engine);

    let rows = engine
        .query("Document")
        .vector_search("[1.0, 0.0, 0.0, 0.0]", 5)
        .execute()
        .expect("vector search executes");

    assert!(!rows.hits.is_empty());
    for hit in &rows.hits {
        let distance = hit.vector_distance.expect("distance present");
        // score == -distance exactly (no normalization)
        assert!(
            (hit.score - (-distance)).abs() < f64::EPSILON,
            "score ({}) must equal -distance ({})",
            hit.score,
            -distance
        );
    }
}

#[cfg(feature = "sqlite-vec")]
#[test]
fn vector_search_ordering_is_score_descending() {
    let (_db, engine) = open_engine();
    seed_docs(&engine);

    let rows = engine
        .query("Document")
        .vector_search("[1.0, 0.0, 0.0, 0.0]", 10)
        .execute()
        .expect("vector search executes");

    assert!(
        rows.hits.len() >= 2,
        "need at least two hits to verify ordering, got {}",
        rows.hits.len()
    );
    for pair in rows.hits.windows(2) {
        assert!(
            pair[0].score >= pair[1].score,
            "hits must be score-descending: {} < {}",
            pair[0].score,
            pair[1].score
        );
    }

    // First hit should be doc-near since its embedding exactly matches the query.
    assert_eq!(rows.hits[0].node.logical_id, "doc-near");
}

#[cfg(feature = "sqlite-vec")]
#[test]
fn vector_search_filter_kind_eq_fuses() {
    let (_db, engine) = open_engine();
    seed_docs(&engine);

    // Without the filter, Documents and Goal compete for the top slots.
    let unfiltered = engine
        .query("Document")
        .vector_search("[1.0, 0.0, 0.0, 0.0]", 10)
        .execute()
        .expect("unfiltered executes");
    assert!(
        unfiltered.hits.iter().all(|h| h.node.kind == "Document"),
        "engine.query(\"Document\") already narrows by root kind"
    );

    // Filter across the whole corpus by switching root kind first is not
    // supported; the fusion test uses a secondary predicate instead.
    let filtered = engine
        .query("Document")
        .vector_search("[1.0, 0.0, 0.0, 0.0]", 10)
        .filter_source_ref_eq("seed")
        .execute()
        .expect("filtered executes");
    assert!(!filtered.hits.is_empty());
    for hit in &filtered.hits {
        assert_eq!(hit.node.kind, "Document");
    }
}

#[cfg(feature = "sqlite-vec")]
#[test]
fn vector_search_with_match_attribution_sets_some_empty_matched_paths() {
    let (_db, engine) = open_engine();
    seed_docs(&engine);

    let rows = engine
        .query("Document")
        .vector_search("[1.0, 0.0, 0.0, 0.0]", 5)
        .with_match_attribution()
        .execute()
        .expect("vector search executes");

    assert!(!rows.hits.is_empty());
    for hit in &rows.hits {
        let attribution = hit
            .attribution
            .as_ref()
            .expect("attribution flag should populate Some(...)");
        assert!(
            attribution.matched_paths.is_empty(),
            "vector attribution must carry an empty matched_paths list per addendum 1"
        );
    }
}

#[cfg(feature = "sqlite-vec")]
#[test]
fn vector_search_without_match_attribution_sets_none() {
    let (_db, engine) = open_engine();
    seed_docs(&engine);

    let rows = engine
        .query("Document")
        .vector_search("[1.0, 0.0, 0.0, 0.0]", 5)
        .execute()
        .expect("vector search executes");

    assert!(!rows.hits.is_empty());
    for hit in &rows.hits {
        assert!(hit.attribution.is_none());
    }
}

#[cfg(feature = "sqlite-vec")]
#[test]
fn vector_search_limit_zero_returns_empty() {
    let (_db, engine) = open_engine();
    seed_docs(&engine);

    let rows = engine
        .query("Document")
        .vector_search("[1.0, 0.0, 0.0, 0.0]", 0)
        .execute()
        .expect("vector search executes");

    assert!(rows.hits.is_empty());
    assert_eq!(rows.vector_hit_count, 0);
    assert_eq!(rows.strict_hit_count, 0);
    assert_eq!(rows.relaxed_hit_count, 0);
    assert!(!rows.fallback_used);
    assert!(!rows.was_degraded);
}

#[cfg(feature = "sqlite-vec")]
#[test]
fn vector_search_filter_json_text_eq_is_residual() {
    let (_db, engine) = open_engine();
    seed_docs(&engine);

    // doc-far has status=archived; exclude it via residual JSON filter.
    let rows = engine
        .query("Document")
        .vector_search("[1.0, 0.0, 0.0, 0.0]", 10)
        .filter_json_text_eq("$.status", "active")
        .execute()
        .expect("vector search executes");

    assert!(!rows.hits.is_empty());
    for hit in &rows.hits {
        assert_ne!(hit.node.logical_id, "doc-far");
    }
    assert!(rows.hits.iter().any(|h| h.node.logical_id == "doc-near"));
}
