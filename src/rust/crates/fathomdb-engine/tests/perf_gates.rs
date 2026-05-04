use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

const PERF_SAMPLES: usize = 1_000;
const AC020_THREADS: usize = 8;
const AC020_ROUNDS_PER_THREAD: usize = 50;

#[derive(Clone, Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
    delay: Duration,
}

impl DeterministicEmbedder {
    fn new(dimension: u32, delay: Duration) -> Self {
        Self {
            identity: EmbedderIdentity::new("deterministic", "perf-gates", dimension),
            vector: unit_vector(dimension as usize),
            delay,
        }
    }
}

impl Embedder for DeterministicEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        thread::sleep(self.delay);
        Ok(self.vector.clone())
    }
}

#[derive(Clone, Debug)]
struct RoutedEmbedder {
    identity: EmbedderIdentity,
}

impl RoutedEmbedder {
    fn new(dimension: u32) -> Self {
        Self { identity: EmbedderIdentity::new("routed", "perf-gates", dimension) }
    }
}

impl Embedder for RoutedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        let mut vector = vec![0.0_f32; self.identity.dimension as usize];
        let slot = if text.starts_with("semantic-") || text.starts_with("vector-doc-") {
            0
        } else if text.starts_with("hybrid-") || text.starts_with("hybrid doc") {
            1
        } else {
            2
        };
        vector[slot] = 1.0;
        Ok(vector)
    }
}

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

fn percentile_ceil(samples: &[Duration], numerator: usize, denominator: usize) -> Duration {
    assert!(!samples.is_empty());
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() * numerator).div_ceil(denominator)).saturating_sub(1);
    sorted[index]
}

fn unit_vector(dimension: usize) -> Vector {
    let mut values = vec![0.0_f32; dimension];
    if dimension > 0 {
        values[0] = 1.0;
    }
    values
}

fn long_run_enabled() -> bool {
    std::env::var_os("AGENT_LONG").is_some()
}

fn ac020_queries() -> [&'static str; 4] {
    ["semantic-0", "hybrid-0", "semantic-1", "hybrid-1"]
}

fn seed_ac020_fixture(engine: &Engine) {
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    for i in 0..2 {
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("vector-doc-{i}"),
            }])
            .expect("vector-only write");
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("hybrid doc hybrid-{i}"),
            }])
            .expect("hybrid write");
    }
    engine.drain(10_000).expect("drain");
}

fn run_ac020_mix(engine: &Engine) {
    for _ in 0..AC020_ROUNDS_PER_THREAD {
        for query in ac020_queries() {
            let result = engine.search(query).expect("search");
            assert!(!result.results.is_empty(), "read-mix query {query} must yield a result");
        }
    }
}

#[test]
#[ignore = "protocol-incomplete: 1M text-query fixture from dev/acceptance.md is not landed yet"]
fn ac_012_text_query_latency_on_fts5_path() {}

#[test]
#[ignore = "blocked on a protocol-complete vector-latency fixture and retrieval-path evidence"]
fn ac_013_vector_retrieval_latency() {}

#[test]
fn ac_017_vector_projection_freshness_p99_le_five_seconds() {
    let (_dir, path) = fixture_path("projection_freshness");
    let embedder = Arc::new(DeterministicEmbedder::new(384, Duration::from_millis(1)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let mut samples = Vec::with_capacity(PERF_SAMPLES);
    for i in 0..PERF_SAMPLES {
        let commit_started = Instant::now();
        let receipt = opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("projection doc {i}"),
            }])
            .expect("write");

        loop {
            let result = opened.engine.search("projection").expect("search");
            if result.projection_cursor >= receipt.cursor {
                samples.push(commit_started.elapsed());
                break;
            }
            assert!(
                commit_started.elapsed() < Duration::from_secs(5),
                "projection cursor did not reach commit cursor within 5 s for write {}",
                receipt.cursor
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    let p99 = percentile_ceil(&samples, 99, 100);
    assert!(
        p99 <= Duration::from_secs(5),
        "AC-017 failed: p99 freshness {:?} exceeded 5 s over {} samples",
        p99,
        samples.len()
    );
}

#[test]
fn ac_018_drain_of_100_vectors_le_two_seconds() {
    let (_dir, path) = fixture_path("drain_100_vectors");
    let embedder = Arc::new(DeterministicEmbedder::new(384, Duration::from_millis(1)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    for i in 0..100 {
        opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: format!("doc {i}") }])
            .expect("write");
    }

    let started = Instant::now();
    opened.engine.drain(5_000).expect("drain");
    let elapsed = started.elapsed();

    assert!(
        elapsed <= Duration::from_secs(2),
        "AC-018 failed: drain took {:?}, expected <= 2 s",
        elapsed
    );
    assert_eq!(opened.engine.vector_row_count_for_test().expect("vector rows"), 100);
}

#[test]
#[ignore = "blocked on a protocol-complete mixed-retrieval workload that exercises a non-synthetic second branch"]
fn ac_019_mixed_retrieval_stress_workload_tail() {}

#[test]
fn ac_020_reads_do_not_serialize_on_a_single_reader_connection() {
    if !long_run_enabled() {
        return;
    }

    let (_dir, path) = fixture_path("ac020_read_mix");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_ac020_fixture(&opened.engine);

    let sequential_started = Instant::now();
    for _ in 0..AC020_THREADS {
        run_ac020_mix(&opened.engine);
    }
    let sequential = sequential_started.elapsed();

    let engine = Arc::new(opened.engine);
    let barrier = Arc::new(Barrier::new(AC020_THREADS + 1));
    let mut handles = Vec::with_capacity(AC020_THREADS);
    for _ in 0..AC020_THREADS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            run_ac020_mix(&engine);
        }));
    }
    let concurrent_started = Instant::now();
    barrier.wait();
    for handle in handles {
        handle.join().expect("reader thread");
    }
    let concurrent = concurrent_started.elapsed();

    let bound = sequential.mul_f32(1.5 / AC020_THREADS as f32);
    assert!(
        concurrent <= bound,
        "AC-020 failed: concurrent={concurrent:?} bound={bound:?} sequential={sequential:?}"
    );
}

#[test]
#[ignore = "profiling harness: set AC020_PHASE=sequential to opt in"]
fn ac_020_sequential_only() {
    if std::env::var("AC020_PHASE").as_deref() != Ok("sequential") {
        return;
    }

    let (_dir, path) = fixture_path("ac020_sequential_only");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_ac020_fixture(&opened.engine);

    let started = Instant::now();
    for _ in 0..AC020_THREADS {
        run_ac020_mix(&opened.engine);
    }
    let elapsed = started.elapsed();

    eprintln!("AC020_PHASE_SEQUENTIAL_MS={}", elapsed.as_millis());
}

#[test]
#[ignore = "profiling harness: set AC020_PHASE=concurrent to opt in"]
fn ac_020_concurrent_only() {
    if std::env::var("AC020_PHASE").as_deref() != Ok("concurrent") {
        return;
    }

    let (_dir, path) = fixture_path("ac020_concurrent_only");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_ac020_fixture(&opened.engine);

    let engine = Arc::new(opened.engine);
    let barrier = Arc::new(Barrier::new(AC020_THREADS + 1));
    let mut handles = Vec::with_capacity(AC020_THREADS);
    for _ in 0..AC020_THREADS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            run_ac020_mix(&engine);
        }));
    }
    let started = Instant::now();
    barrier.wait();
    for handle in handles {
        handle.join().expect("reader thread");
    }
    let elapsed = started.elapsed();

    eprintln!("AC020_PHASE_CONCURRENT_MS={}", elapsed.as_millis());
}
