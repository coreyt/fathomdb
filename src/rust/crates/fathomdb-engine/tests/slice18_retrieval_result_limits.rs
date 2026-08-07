//! Slice 18 — public ranked-retrieval limits.
//!
//! These are deliberately real-database tests: the limit must be applied to
//! ranked hits after the FTS/property filters, not merely accepted at an API
//! boundary.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    Engine, EngineError, InitialState, PreparedWrite, ProjectionFts, ProjectionRole,
    ProjectionSpec, ReadView, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use std::collections::BTreeSet;
use std::sync::Arc;
use tempfile::TempDir;

fn db_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join(format!("limits{SQLITE_SUFFIX}"))
}

fn searchable_title() -> ProjectionSpec {
    ProjectionSpec {
        name: "title".to_string(),
        roles: BTreeSet::from([ProjectionRole::Searchable]),
        fts: Some(ProjectionFts { tokenizer: None }),
        vector: None,
        source: None,
    }
}

fn corpus() -> Vec<PreparedWrite> {
    (0..101)
        .map(|n| PreparedWrite::Node {
            kind: "doc".to_string(),
            body: format!(r#"{{"title":"needle result {n}"}}"#),
            source_id: SourceId::new("test:slice18").expect("source id"),
            logical_id: Some(format!("N{n}")),
            state: InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        })
        .collect()
}

#[derive(Clone, Debug)]
struct RankedEmbedder;

impl Embedder for RankedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("slice18-ranked", "rev-a", 8)
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        let rank = text
            .strip_prefix('r')
            .and_then(|suffix| suffix.get(..2))
            .and_then(|digits| digits.parse::<f32>().ok())
            .unwrap_or(0.0);
        let mut vector = vec![0.0; 8];
        vector[0] = 1.0;
        vector[1] = rank * 0.01;
        Ok(vector)
    }
}

#[test]
fn ranked_search_families_default_to_ten_and_honor_requested_k() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir)).expect("open");
    let engine = &opened.engine;
    engine.configure_projections(&[searchable_title()], &[]).expect("configure");
    engine.write(&corpus()).expect("write corpus");
    engine.drain(10_000).expect("drain");

    assert_eq!(engine.search("needle").expect("default hybrid").results.len(), 10);
    assert_eq!(engine.search_text_only("needle").expect("default text").results.len(), 10);
    assert_eq!(
        engine
            .search_projected_text("needle", "title", None, &ReadView::default())
            .expect("default projected")
            .results
            .len(),
        10
    );
    assert_eq!(
        engine.search_expand("needle", None, 0).expect("default expand").search_hits.len(),
        10
    );

    for limit in [5, 20, 50, 100] {
        assert_eq!(engine.search_with_limit("needle", limit).expect("hybrid").results.len(), limit);
        assert_eq!(
            engine.search_text_only_with_limit("needle", limit).expect("text").results.len(),
            limit
        );
        assert_eq!(
            engine
                .search_projected_text_with_limit(
                    "needle",
                    "title",
                    None,
                    &ReadView::default(),
                    limit,
                )
                .expect("projected")
                .results
                .len(),
            limit
        );
        assert_eq!(
            engine
                .search_expand_with_limit("needle", None, 0, limit)
                .expect("expand")
                .search_hits
                .len(),
            limit
        );
    }

    opened.engine.close().expect("close");
}

#[test]
fn ranked_search_limits_reject_out_of_range_requests() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir)).expect("open");
    let engine = &opened.engine;

    for limit in [0, 101] {
        assert!(matches!(
            engine.search_with_limit("needle", limit),
            Err(EngineError::InvalidArgument { .. })
        ));
        assert!(matches!(
            engine.search_text_only_with_limit("needle", limit),
            Err(EngineError::InvalidArgument { .. })
        ));
        assert!(matches!(
            engine.search_projected_text_with_limit(
                "needle",
                "title",
                None,
                &ReadView::default(),
                limit,
            ),
            Err(EngineError::InvalidArgument { .. })
        ));
        assert!(matches!(
            engine.search_expand_with_limit("needle", None, 0, limit),
            Err(EngineError::InvalidArgument { .. })
        ));
    }

    opened.engine.close().expect("close");
}

#[test]
fn vector_rerank_fanout_reaches_requested_twenty_and_fifty() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(RankedEmbedder)).expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    let batch: Vec<_> = (1..=50)
        .map(|rank| PreparedWrite::Node {
            kind: "doc".to_string(),
            body: format!("r{rank:02} vector candidate"),
            source_id: SourceId::new("test:slice18-vector").expect("source id"),
            logical_id: Some(format!("V{rank}")),
            state: InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        })
        .collect();
    engine.write(&batch).expect("write");
    engine.drain(10_000).expect("drain");

    for limit in [20, 50] {
        let result = engine.search_with_limit("querymarker", limit).expect("vector search");
        assert_eq!(result.results.len(), limit, "requested vector depth must not stop at ten");
        assert!(
            result
                .results
                .iter()
                .all(|hit| hit.branch == fathomdb_engine::SoftFallbackBranch::Vector),
            "query deliberately has no FTS match, so every result proves vector fanout"
        );
    }

    opened.engine.close().expect("close");
}

#[test]
fn projected_text_limit_is_applied_after_metadata_filtering() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir)).expect("open");
    let engine = &opened.engine;
    engine.configure_projections(&[searchable_title()], &[]).expect("configure");
    let skipped = (0..10).map(|n| PreparedWrite::Node {
        kind: "skip".to_string(),
        body: format!(r#"{{"title":"needle skipped {n}"}}"#),
        source_id: SourceId::new("test:slice18-projected").expect("source id"),
        logical_id: Some(format!("S{n}")),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    });
    let retained = (0..20).map(|n| PreparedWrite::Node {
        kind: "keep".to_string(),
        body: format!(r#"{{"title":"needle retained {n}"}}"#),
        source_id: SourceId::new("test:slice18-projected").expect("source id"),
        logical_id: Some(format!("K{n}")),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    });
    engine.write(&skipped.chain(retained).collect::<Vec<_>>()).expect("write");
    engine.drain(10_000).expect("drain");

    let mut filter = fathomdb_engine::SearchFilter::default();
    filter.kind = Some("keep".to_string());
    let result = engine
        .search_projected_text_with_limit("needle", "title", Some(filter), &ReadView::default(), 20)
        .expect("projected search");
    assert_eq!(result.results.len(), 20, "filtered candidates must not consume the public budget");
    assert!(result.results.iter().all(|hit| hit.kind == "keep"));

    opened.engine.close().expect("close");
}
