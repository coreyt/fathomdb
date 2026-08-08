//! Slice 23 — direct text-only result-prefix stability.
//!
//! The fixture deliberately duplicates an eleventh node body's FTS match on
//! an edge.  Before Slice 23, the caller limit capped node FTS while edge FTS
//! remained unbounded, so the duplicate only contributed to the larger call's
//! RRF input and changed its first ten results.

use fathomdb_embedder::NoopEmbedder;
use fathomdb_engine::{EmbedderChoice, Engine, InitialState, PreparedWrite, ReadView, SourceId};
use fathomdb_schema::SQLITE_SUFFIX;
use std::sync::Arc;
use tempfile::TempDir;

const QUERY: &str = "s23prefix";

fn db_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join(format!("slice23{SQLITE_SUFFIX}"))
}

fn open(dir: &TempDir) -> fathomdb_engine::OpenedEngine {
    Engine::open_with_choice(
        db_path(dir),
        EmbedderChoice::Caller(Arc::new(NoopEmbedder::default())),
    )
    .expect("open")
}

fn node(rank: usize) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: format!("{QUERY} matching document {rank:02}"),
        source_id: SourceId::new("test:slice23").expect("source id"),
        logical_id: Some(format!("N{rank:02}")),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn duplicate_edge() -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: "N00".to_string(),
        to: "N01".to_string(),
        source_id: SourceId::new("test:slice23").expect("source id"),
        logical_id: Some("E10".to_string()),
        // Must be byte-identical to node 10: fusion deduplicates on `body`.
        body: Some(format!("{QUERY} matching document 10")),
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

fn assert_ordered_prefix(
    small: &fathomdb_engine::SearchResult,
    large: &fathomdb_engine::SearchResult,
) {
    assert_eq!(small.results.len(), 10, "small direct-text result must honor limit=10");
    assert_eq!(large.results.len(), 50, "large direct-text result must honor limit=50");
    let actual: Vec<_> = small.results.iter().map(|hit| (&hit.id, hit.score)).collect();
    let expected: Vec<_> =
        large.results.iter().take(small.results.len()).map(|hit| (&hit.id, hit.score)).collect();
    assert_eq!(
        actual, expected,
        "small direct-text result must be the ordered large-result prefix"
    );
}

#[test]
fn direct_text_limits_are_prefix_stable_with_duplicate_edge_body() {
    let dir = TempDir::new().expect("tempdir");
    let opened = open(&dir);
    let engine = &opened.engine;
    let mut writes: Vec<_> = (0..50).map(node).collect();
    writes.push(duplicate_edge());
    engine.write(&writes).expect("write adversarial corpus");
    engine.drain(10_000).expect("drain");

    let small = engine.search_text_only_with_limit(QUERY, 10).expect("small direct text search");
    let large = engine.search_text_only_with_limit(QUERY, 50).expect("large direct text search");
    assert_ordered_prefix(&small, &large);

    opened.engine.close().expect("close");
}

#[test]
fn direct_text_view_limits_are_prefix_stable_at_a_fixed_validity_time() {
    let dir = TempDir::new().expect("tempdir");
    let opened = open(&dir);
    let engine = &opened.engine;
    let mut writes: Vec<_> = (0..50).map(node).collect();
    writes.push(duplicate_edge());
    engine.write(&writes).expect("write adversarial corpus");
    engine.drain(10_000).expect("drain");

    let view = ReadView { valid_as_of: Some(1_700_000_000), ..ReadView::default() };
    let small = engine
        .search_text_only_view_with_limit(QUERY, &view, 10)
        .expect("small direct text view search");
    let large = engine
        .search_text_only_view_with_limit(QUERY, &view, 50)
        .expect("large direct text view search");
    assert_ordered_prefix(&small, &large);

    opened.engine.close().expect("close");
}
