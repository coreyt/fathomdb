//! 0.8.0 Slice 5 / G1 — structured `SearchHit` shape.
//!
//! AC-G1-hit-shape, AC-G1-no-eq, AC-G1-dedup-order. Asserts that
//! `SearchResult.results` is `Vec<SearchHit>` (not `Vec<String>`), each hit
//! carries `id == write_cursor`, populated `kind`, populated `body`, a finite
//! `score`, and the correct `branch`; that `SearchResult` no longer derives
//! `Eq` (a `SearchHit` carries `score: f64`); and that dedup-on-body +
//! vector-first ordering is preserved.
//!
//! Uses a deterministic in-process embedder so both retrieval branches
//! exercise without network. No mocking of the database.

use std::sync::Arc;
use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite, SearchHit, SoftFallbackBranch};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

/// Deterministic embedder: every text maps to the same unit vector, so the
/// vector branch always surfaces the (single, when one doc) candidate and the
/// f32 rerank distance is finite.
#[derive(Clone, Debug)]
struct FixedEmbedder;

impl Embedder for FixedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("deterministic", "rev-a", 8)
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; 8];
        v[0] = 1.0;
        Ok(v)
    }
}

fn fixture(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

fn search_after_projection(
    engine: &Engine,
    query: &str,
    min_cursor: u64,
) -> fathomdb_engine::SearchResult {
    let started = Instant::now();
    loop {
        let result = engine.search(query).expect("search");
        if result.projection_cursor >= min_cursor && !result.results.is_empty() {
            return result;
        }
        if started.elapsed() > Duration::from_secs(10) {
            return result;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn ac_g1_hit_shape_text_branch() {
    let (_dir, path) = fixture("g1_hit_shape");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "note".to_string(),
            body: "structured retrieval hit shape document".to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "structured", receipt.cursor);
    assert_eq!(result.results.len(), 1, "expected exactly one hit");
    let hit: &SearchHit = &result.results[0];

    // id == write_cursor (interim identity carrier).
    assert_eq!(hit.id, receipt.cursor, "hit id must be the write_cursor");
    // populated kind + body.
    assert_eq!(hit.kind, "note");
    assert_eq!(hit.body, "structured retrieval hit shape document");
    // finite score.
    assert!(hit.score.is_finite(), "score must be finite, got {}", hit.score);
    // text branch tag (no vector kind configured -> text-only).
    assert_eq!(hit.branch, SoftFallbackBranch::Text);

    opened.engine.close().unwrap();
}

#[test]
fn ac_g1_hit_shape_vector_branch() {
    let (_dir, path) = fixture("g1_hit_shape_vec");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            // FTS query term ("vectorize") is NOT in the body, so the only
            // way this surfaces is the vector branch.
            body: "semantic only payload".to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "vectorize", receipt.cursor);
    assert_eq!(result.results.len(), 1, "expected exactly one vector hit");
    let hit = &result.results[0];
    assert_eq!(hit.id, receipt.cursor);
    assert_eq!(hit.kind, "doc");
    assert_eq!(hit.body, "semantic only payload");
    assert!(hit.score.is_finite());
    assert_eq!(hit.branch, SoftFallbackBranch::Vector);

    opened.engine.close().unwrap();
}

#[test]
fn ac_g1_dedup_on_body_and_vector_first_order() {
    let (_dir, path) = fixture("g1_dedup_order");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    // Two docs: both share the FTS term "hybrid"; both are vector candidates.
    let r1 = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "hybrid retrieval document".to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write 1");
    let r2 = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "another hybrid document".to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write 2");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "hybrid", r2.cursor.max(r1.cursor));

    // Dedup-on-body: a body surfaced by BOTH branches must appear exactly once.
    let mut bodies: Vec<&str> = result.results.iter().map(|h| h.body.as_str()).collect();
    let mut deduped = bodies.clone();
    deduped.sort_unstable();
    deduped.dedup();
    bodies.sort_unstable();
    assert_eq!(bodies, deduped, "results must be deduped on body");

    // Vector-first ordering: the vector branch's hits precede any text-only
    // hits. With this fixed embedder both docs are vector candidates, so every
    // surviving hit is tagged Vector and none is a trailing text-only dup.
    let first_branch = result.results[0].branch;
    assert_eq!(
        first_branch,
        SoftFallbackBranch::Vector,
        "vector-first ordering: leading hit must be from the vector branch"
    );

    // Every hit carries the structured shape.
    for hit in &result.results {
        assert!(hit.id > 0, "id (write_cursor) must be populated");
        assert_eq!(hit.kind, "doc");
        assert!(!hit.body.is_empty());
        assert!(hit.score.is_finite());
    }

    opened.engine.close().unwrap();
}

/// Compile-level proof that `SearchResult` (and `SearchHit`) no longer derive
/// `Eq` is enforced by the `score: f64` field. This runtime check additionally
/// asserts `PartialEq` is retained (results compare by value) — the derive set
/// is `Clone, Debug, PartialEq`, NOT `Eq`.
#[test]
fn ac_g1_no_eq_but_partial_eq() {
    let (_dir, path) = fixture("g1_no_eq");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "note".to_string(),
            body: "equality probe document".to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");
    let a = search_after_projection(&opened.engine, "equality", receipt.cursor);
    let b = a.clone();
    assert_eq!(a, b, "SearchResult must retain PartialEq");
    opened.engine.close().unwrap();
}
