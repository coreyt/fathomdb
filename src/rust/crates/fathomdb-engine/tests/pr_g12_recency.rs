//! Slice 10 / G12-recency (recency half only).
//!
//! A `write_cursor`-derived recency reweight applied **AFTER** bit-KNN (never a
//! vec0 predicate), gated behind a **dedicated recency flag, off by default**
//! (NOT `fusion_mode`). Off => order is pure RRF. On => an equal-RRF tie breaks
//! toward the more-recent (higher `write_cursor`) hit. Importance (G12 M-half)
//! and F9 confidence are deferred. Plus a reweight-latency gate.

use std::sync::Arc;
use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    apply_recency_reweight, Engine, IdSpace, PreparedWrite, SearchHit, SoftFallbackBranch,
};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn hit(id: u64, body: &str, score: f64) -> SearchHit {
    SearchHit {
        // C-2 (0.8.19): recency reweight keys on `write_cursor`, not `id`.
        id: IdSpace::content(id.to_string()),
        write_cursor: id,
        kind: "doc".to_string(),
        body: body.to_string(),
        score,
        branch: SoftFallbackBranch::Vector,
        source_id: None,
        ce_score: None,
    }
}

#[test]
fn recency_disabled_is_a_no_op() {
    // Two hits, equal RRF score; lower-id first in the input.
    let hits = vec![hit(1, "older", 0.01), hit(2, "newer", 0.01)];
    let out = apply_recency_reweight(hits.clone(), false);
    assert_eq!(out, hits, "flag off => order + scores unchanged (pure RRF)");
}

#[test]
fn recency_enabled_breaks_equal_rrf_tie_toward_recent() {
    // Equal base score: recency must surface the higher-cursor ("newer") hit.
    let hits = vec![hit(1, "older", 0.01), hit(2, "newer", 0.01)];
    let out = apply_recency_reweight(hits, true);
    assert_eq!(out[0].body, "newer", "more-recent (higher write_cursor) wins the tie");
    assert_eq!(out[1].body, "older");
    assert!(out[0].score >= out[1].score, "reweighted list stays sorted by score desc");
}

#[test]
fn recency_does_not_override_a_clear_rrf_signal() {
    // "strong" has a clearly higher RRF score than the more-recent "recent";
    // the conservative recency weight must not flip a clear signal.
    let hits = vec![hit(9, "recent", 0.01), hit(1, "strong", 0.02)];
    let out = apply_recency_reweight(hits, true);
    assert_eq!(out[0].body, "strong", "recency is a near-tie nudge, not an override");
}

#[test]
fn recency_reweight_latency_gate() {
    let hits: Vec<SearchHit> =
        (0..10).map(|i| hit(i as u64, &format!("body-{i}"), 0.02 - (i as f64) * 0.001)).collect();
    let started = Instant::now();
    for _ in 0..1000 {
        let _ = apply_recency_reweight(hits.clone(), true);
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(200),
        "reweighting a top-10 set 1000x must be cheap, took {elapsed:?}"
    );
}

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

#[test]
fn recency_flag_off_by_default_e2e() {
    let (_dir, path) = fixture("g12_default_off");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    for body in ["recency alpha", "recency beta"] {
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: body.to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
            }])
            .expect("write");
    }
    opened.engine.drain(10_000).expect("drain");

    // Default (flag off) is pure RRF.
    let baseline = opened.engine.search("recency").expect("search");
    assert!(!baseline.results.is_empty());

    // Enabling the dedicated recency flag keeps the same result SET and a
    // score-desc-sorted list (the conservative reweight never drops a hit).
    opened.engine.set_recency_reweight_enabled_for_test(true);
    let reweighted = opened.engine.search("recency").expect("search");
    let base_bodies: std::collections::BTreeSet<&str> =
        baseline.results.iter().map(|h| h.body.as_str()).collect();
    let rw_bodies: std::collections::BTreeSet<&str> =
        reweighted.results.iter().map(|h| h.body.as_str()).collect();
    assert_eq!(base_bodies, rw_bodies, "recency reweight preserves the result set");
    for w in reweighted.results.windows(2) {
        assert!(w[0].score >= w[1].score, "reweighted list stays sorted by score desc");
    }

    opened.engine.close().unwrap();
}
