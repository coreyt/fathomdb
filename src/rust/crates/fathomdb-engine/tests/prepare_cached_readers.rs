use std::sync::Arc;
use std::thread;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

const SEARCH_ITERATIONS: usize = 100;

/// 3-dimensional embedder that routes by query prefix, matching ac_020 fixture.
#[derive(Clone, Debug)]
struct RoutedEmbedder3 {
    identity: EmbedderIdentity,
}

impl RoutedEmbedder3 {
    fn new() -> Self {
        Self { identity: EmbedderIdentity::new("routed3", "prepare-cached-readers", 3) }
    }
}

impl Embedder for RoutedEmbedder3 {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; 3];
        let slot = if text.starts_with("semantic-") {
            0
        } else if text.starts_with("hybrid-") {
            1
        } else {
            2
        };
        v[slot] = 1.0;
        Ok(v)
    }
}

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

fn seed_fixture(engine: &Engine) {
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    for i in 0..2 {
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("semantic-{i} text"),
            }])
            .expect("write");
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("hybrid-{i} text"),
            }])
            .expect("write");
    }
    engine.drain(10_000).expect("drain");
}

/// Verify that the 4 read-path search statements use `prepare_cached` so each
/// reader connection compiles each SQL at most once.
///
/// Baseline (before E.1): `tx.prepare()` on every search → 4 fresh prepares
/// per call → 100 iterations × 3 active statements = 300 cache misses (fails).
///
/// After E.1: `prepare_cached` → 1 miss per unique SQL per reader connection
/// borrowed. Single-threaded test uses ≤ 1 reader → ≤ 4 total misses (passes).
/// The bound 32 (8 readers × 4 stmts) covers the multi-reader case.
#[test]
fn prepare_cached_readers_statement_cache_populated_after_100_searches() {
    let (_dir, path) = fixture_path("prepare_cached_readers");
    let embedder = Arc::new(RoutedEmbedder3::new());
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_fixture(&opened.engine);

    opened.engine.reset_search_prepare_count_for_test();

    for _ in 0..SEARCH_ITERATIONS {
        let result = opened.engine.search("semantic-0").expect("search");
        assert!(!result.results.is_empty(), "search must return results");
    }

    let misses = opened.engine.search_prepare_count_for_test();
    assert!(
        misses <= 32,
        "expected ≤ 32 total prepare cache misses (8 readers × 4 stmts); got {misses}. \
         Baseline (no caching) would be ~300 misses for 100 iterations."
    );
}

/// Same invariant, exercised with all 4 query types from the AC-020 mix to
/// confirm caching holds regardless of which vector-path branches activate.
#[test]
fn prepare_cached_readers_all_query_types_cached() {
    let (_dir, path) = fixture_path("prepare_cached_readers_mix");
    let embedder = Arc::new(RoutedEmbedder3::new());
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_fixture(&opened.engine);

    opened.engine.reset_search_prepare_count_for_test();

    let queries = ["semantic-0", "hybrid-0", "semantic-1", "hybrid-1"];
    for _ in 0..25 {
        for q in queries {
            opened.engine.search(q).expect("search");
        }
    }

    let misses = opened.engine.search_prepare_count_for_test();
    assert!(
        misses <= 32,
        "expected ≤ 32 prepare cache misses across 100 searches (4 queries × 25 iters); \
         got {misses}"
    );
}

/// Concurrent variant: 8 threads each run 50 searches. Total cache misses must
/// be ≤ 32 (8 reader connections × 4 statements), matching READER_POOL_SIZE.
#[test]
fn prepare_cached_readers_concurrent_pool_miss_bound() {
    const THREADS: usize = 8;
    const ROUNDS: usize = 50;

    let (_dir, path) = fixture_path("prepare_cached_concurrent");
    let embedder = Arc::new(RoutedEmbedder3::new());
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_fixture(&opened.engine);

    let engine = Arc::new(opened.engine);
    engine.reset_search_prepare_count_for_test();

    let barrier = Arc::new(std::sync::Barrier::new(THREADS + 1));
    let mut handles = Vec::with_capacity(THREADS);
    for _ in 0..THREADS {
        let eng = Arc::clone(&engine);
        let bar = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            bar.wait();
            for _ in 0..ROUNDS {
                eng.search("semantic-0").expect("search");
            }
        }));
    }
    barrier.wait();
    for h in handles {
        h.join().expect("thread");
    }

    let misses = engine.search_prepare_count_for_test();
    // Each of the 8 reader connections may cold-start its cache → 8 × 3 active
    // stmts = 24 expected, ≤ 32 to account for pool reuse ordering.
    assert!(misses <= 32, "expected ≤ 32 cache misses ({THREADS} readers × 4 stmts); got {misses}");
}

/// Regression: once the cache is warm, subsequent searches must produce zero
/// new prepare misses, proving the cache stays hot indefinitely.
#[test]
fn prepare_cached_readers_miss_count_saturates() {
    let (_dir, path) = fixture_path("prepare_cached_saturation");
    let embedder = Arc::new(RoutedEmbedder3::new());
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_fixture(&opened.engine);

    // Warm-up phase: cache fills after the first few searches.
    for _ in 0..10 {
        opened.engine.search("semantic-0").expect("search");
    }

    // Post-warm-up: counter reset; all subsequent searches must be cache hits.
    opened.engine.reset_search_prepare_count_for_test();
    for _ in 0..1_000 {
        opened.engine.search("semantic-0").expect("search");
    }
    let post_warmup_misses = opened.engine.search_prepare_count_for_test();

    assert_eq!(
        post_warmup_misses, 0,
        "after cache warm-up, 1000 searches must produce 0 new prepare misses; \
         got {post_warmup_misses}"
    );
}
