use std::sync::{Arc, Barrier, Mutex};
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
                source_id: None,
            }])
            .expect("vector-only write");
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("hybrid doc hybrid-{i}"),
                source_id: None,
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

// ── AC-012 / AC-013 / AC-019 retrieval perf-gate fixtures ───────────────────
//
// Per `dev/plans/0.6.0-Phase-9-Pack-D-retrieval-perf-fixtures.md` and
// ADR-0.6.0-{text-query,retrieval}-latency-gates the canonical fixture
// scale is 1,000,000 chunk rows / 768-d vectors. Seeding 1M rows under
// AGENT_LONG=1 on the aarch64 dev runner is hours-of-wall-clock; the
// canonical x86_64 tier-1 CI runner sets `AC_FULL_SCALE=1` to honor
// the 1M scale. AGENT_LONG=1 on the dev runner uses the env-tunable
// scale below (AC-007a/b runner-pin precedent) and asserts the same
// budget. See `dev/test-plan.md` § Current Perf Attribution and
// `dev/notes/performance-whitepaper-notes.md` for measured medians.
const AC012_DEFAULT_N: usize = 100_000;
const AC013_DEFAULT_N: usize = 50_000;
const AC019_THREADS: usize = 8;
const AC019_QUERIES_PER_THREAD: usize = 250;
const AC012_BUDGET_P50: Duration = Duration::from_millis(20);
const AC012_BUDGET_P99: Duration = Duration::from_millis(150);
const AC013_BUDGET_P50: Duration = Duration::from_millis(50);
const AC013_BUDGET_P99: Duration = Duration::from_millis(200);
const AC019_STRESS_FLOOR: Duration = Duration::from_millis(150);
const AC019_STRESS_MULT: u32 = 10;
const RETRIEVAL_VECTOR_DIM: u32 = 768;

fn env_usize(key: &str, default_full: usize, default_short: usize) -> usize {
    if let Ok(raw) = std::env::var(key) {
        if let Ok(parsed) = raw.parse::<usize>() {
            return parsed;
        }
    }
    if std::env::var_os("AC_FULL_SCALE").is_some() {
        default_full
    } else {
        default_short
    }
}

fn ac012_corpus_n() -> usize {
    env_usize("AC012_CORPUS_N", 1_000_000, AC012_DEFAULT_N)
}

fn ac013_corpus_n() -> usize {
    env_usize("AC013_CORPUS_N", 1_000_000, AC013_DEFAULT_N)
}

/// Deterministic seeded LCG. Generates reproducible token streams so the
/// AC-012 corpus and the held-out query set are both byte-stable across
/// runs (per ADR-0.6.0-text-query-latency-gates "synthetic-English-like
/// text with a Zipfian token-frequency distribution").
struct SeededRng {
    state: u64,
}

impl SeededRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1) }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state
    }

    fn next_in(&mut self, bound: usize) -> usize {
        (self.next_u64() as usize) % bound
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }
}

/// Vocabulary of synthetic English-like tokens. Size ~1024 keeps the
/// FTS index dense; tokens are short, ASCII, lowercase, deterministic.
fn perf_vocab() -> Vec<String> {
    let mut out = Vec::with_capacity(1024);
    for i in 0..1024 {
        let a = (b'a' + ((i / 26 / 26) % 26) as u8) as char;
        let b = (b'a' + ((i / 26) % 26) as u8) as char;
        let c = (b'a' + (i % 26) as u8) as char;
        out.push(format!("{a}{b}{c}{i:04}"));
    }
    out
}

/// Sample a token index under a Zipfian-ish distribution with shape s=1.0,
/// using inverse-CDF on a precomputed cumulative weight table. Returns an
/// index into the vocabulary in [0, vocab_size).
fn zipf_index(rng: &mut SeededRng, cumulative: &[f64]) -> usize {
    let r = rng.next_f64() * cumulative[cumulative.len() - 1];
    match cumulative.binary_search_by(|w| w.partial_cmp(&r).unwrap_or(std::cmp::Ordering::Equal)) {
        Ok(idx) => idx,
        Err(idx) => idx.min(cumulative.len() - 1),
    }
}

fn zipf_cumulative(vocab_size: usize) -> Vec<f64> {
    let mut cumulative = Vec::with_capacity(vocab_size);
    let mut acc = 0.0_f64;
    for k in 1..=vocab_size {
        acc += 1.0_f64 / k as f64;
        cumulative.push(acc);
    }
    cumulative
}

/// Generate one synthetic chunk body of approximately `target_bytes`
/// (~500 B per ADR) using the Zipfian sampler. Tokens are space-joined.
fn synth_chunk_body(rng: &mut SeededRng, vocab: &[String], cumulative: &[f64]) -> String {
    // Tokens average ~7 chars + 1 separator, target ~500 B => ~63 tokens.
    let mut body = String::with_capacity(512);
    let token_count = 55 + rng.next_in(20);
    for i in 0..token_count {
        if i > 0 {
            body.push(' ');
        }
        body.push_str(&vocab[zipf_index(rng, cumulative)]);
    }
    body
}

/// Choose a held-out query token from the 50th–90th percentile
/// term-frequency band per ADR-0.6.0-text-query-latency-gates. Vocab is
/// indexed by descending frequency rank (rank 0 most frequent), so the
/// band maps to indices [0.10*vocab, 0.50*vocab) (frequency rank space).
/// The Zipfian sampler yields rank 0 most often, so this band carves out
/// the body of the distribution.
fn ac012_query_token_band(vocab: &[String]) -> Vec<String> {
    let lo = vocab.len() / 10;
    let hi = vocab.len() / 2;
    vocab[lo..hi].to_vec()
}

/// AC-012 deterministic seeder: 1 chunk body per write batch (4096 nodes
/// per `engine.write()`). Returns elapsed seed time for diagnostics.
fn seed_ac012_corpus(engine: &Engine, n: usize) -> Duration {
    const BATCH: usize = 4096;
    let vocab = perf_vocab();
    let cumulative = zipf_cumulative(vocab.len());
    let mut rng = SeededRng::new(0x0AC0_12C0_12C0);
    let started = Instant::now();
    let mut written = 0usize;
    while written < n {
        let take = BATCH.min(n - written);
        let mut batch = Vec::with_capacity(take);
        for _ in 0..take {
            batch.push(PreparedWrite::Node {
                kind: "doc".to_string(),
                body: synth_chunk_body(&mut rng, &vocab, &cumulative),
                source_id: None,
            });
        }
        engine.write(&batch).expect("ac-012 seed write");
        written += take;
    }
    // No vector kind configured -> projection runtime still drains FTS index.
    engine.drain(600_000).expect("ac-012 drain");
    started.elapsed()
}

/// Deterministic varying-vector embedder. Projects an input token's
/// stable hash onto a single coordinate per call so vec0 ANN search
/// returns distinct k=10 neighbors (a constant-vector embedder would
/// collapse all distances to 0). Reproducible byte-for-byte across runs.
#[derive(Clone, Debug)]
struct VaryingEmbedder {
    identity: EmbedderIdentity,
    dim: u32,
}

impl VaryingEmbedder {
    fn new(dim: u32) -> Self {
        Self { identity: EmbedderIdentity::new("varying", "perf-gates", dim), dim }
    }

    fn vector_for(&self, text: &str) -> Vector {
        let dim = self.dim as usize;
        let mut v = vec![0.0_f32; dim];
        // FNV-1a 64-bit on the input; spread across coordinates with
        // deterministic small magnitudes so distance is meaningful.
        let mut h: u64 = 0xcbf29ce484222325;
        for &b in text.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        // Place mass on a small handful of coordinates derived from h.
        for k in 0..6 {
            let coord = ((h >> (k * 8)) as usize) % dim;
            let sign = if (h >> (k * 8 + 7)) & 1 == 0 { 1.0 } else { -1.0 };
            v[coord] += sign * 0.5_f32;
        }
        // Normalize-ish so all vectors have similar magnitude.
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
        for x in &mut v {
            *x /= norm;
        }
        v
    }
}

impl Embedder for VaryingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        Ok(self.vector_for(text))
    }
}

/// AC-013 deterministic seeder: N vector rows with varying embeddings.
/// Returns elapsed seed time. Bodies double as both FTS5 documents and
/// the input to the embedder, so the same fixture supports AC-019's
/// FTS+vector mixed workload.
fn seed_ac013_corpus(engine: &Engine, n: usize) -> Duration {
    const BATCH: usize = 1024;
    let vocab = perf_vocab();
    let cumulative = zipf_cumulative(vocab.len());
    let mut rng = SeededRng::new(0x0AC0_13D0_13D0);
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    let started = Instant::now();
    let mut written = 0usize;
    while written < n {
        let take = BATCH.min(n - written);
        let mut batch = Vec::with_capacity(take);
        for _ in 0..take {
            batch.push(PreparedWrite::Node {
                kind: "doc".to_string(),
                body: synth_chunk_body(&mut rng, &vocab, &cumulative),
                source_id: None,
            });
        }
        engine.write(&batch).expect("ac-013 seed write");
        written += take;
    }
    engine.drain(1_800_000).expect("ac-013 drain");
    started.elapsed()
}

/// Build a held-out reproducible query set drawn from the same
/// distribution as the indexed corpus (ADR-0.6.0-retrieval-latency-gates
/// "Query vectors drawn from a held-out slice of the same distribution").
fn ac013_query_bodies(count: usize) -> Vec<String> {
    let vocab = perf_vocab();
    let cumulative = zipf_cumulative(vocab.len());
    // Use a different seed than the corpus so the query set is held out
    // (not byte-equal to seeded chunks but drawn from same distribution).
    let mut rng = SeededRng::new(0x0AC0_130D_EC0D_E000);
    (0..count).map(|_| synth_chunk_body(&mut rng, &vocab, &cumulative)).collect()
}

/// Bounded-size histogram for AC-019 tail-latency capture. Single
/// power-of-two bucketing in microseconds; avoids unbounded
/// `Vec<Duration>` per the Pack D plan (`Histogram::record` style).
struct LatencyHistogram {
    /// Bucket i spans [2^i us, 2^(i+1) us). 32 buckets covers 1 us .. ~71 minutes.
    buckets: [u64; 32],
    count: u64,
}

impl LatencyHistogram {
    fn new() -> Self {
        Self { buckets: [0; 32], count: 0 }
    }

    fn record(&mut self, d: Duration) {
        let us = d.as_micros().max(1) as u64;
        let bucket = (63 - us.leading_zeros()) as usize;
        let bucket = bucket.min(self.buckets.len() - 1);
        self.buckets[bucket] += 1;
        self.count += 1;
    }

    fn merge(&mut self, other: &LatencyHistogram) {
        for i in 0..self.buckets.len() {
            self.buckets[i] += other.buckets[i];
        }
        self.count += other.count;
    }

    /// Return the upper bound of the bucket containing the requested
    /// percentile (numerator/denominator). Conservative ceiling.
    fn percentile_ceil(&self, numerator: u64, denominator: u64) -> Duration {
        if self.count == 0 {
            return Duration::ZERO;
        }
        let target = (self.count * numerator).div_ceil(denominator);
        let mut acc: u64 = 0;
        for (i, count) in self.buckets.iter().enumerate() {
            acc += count;
            if acc >= target {
                let upper_us = 1u64 << (i + 1);
                return Duration::from_micros(upper_us);
            }
        }
        let i = self.buckets.len() - 1;
        Duration::from_micros(1u64 << (i + 1))
    }
}

#[test]
fn ac_012_text_query_latency_on_fts5_path() {
    if !long_run_enabled() {
        return;
    }

    let n = ac012_corpus_n();
    let (_dir, path) = fixture_path("ac012_text_query");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let seed_elapsed = seed_ac012_corpus(&opened.engine, n);

    // Build the held-out query token band (50th–90th percentile term
    // frequency per the ADR). Reproducible: seeded from the vocab order.
    let vocab = perf_vocab();
    let band = ac012_query_token_band(&vocab);
    let mut rng = SeededRng::new(0x0AC0_120D_EC0D_E000);
    let queries: Vec<String> =
        (0..PERF_SAMPLES).map(|_| band[rng.next_in(band.len())].clone()).collect();

    // Warmup: full pass discarded, per ADR ("run the full query suite
    // once and discard; measure on the second pass").
    for q in &queries {
        let _ = opened.engine.search(q).expect("warmup search");
    }

    // Measurement pass.
    let mut samples = Vec::with_capacity(PERF_SAMPLES);
    for q in &queries {
        let started = Instant::now();
        let _ = opened.engine.search(q).expect("measure search");
        samples.push(started.elapsed());
    }

    let p50 = percentile_ceil(&samples, 50, 100);
    let p99 = percentile_ceil(&samples, 99, 100);
    eprintln!(
        "AC012_NUMBERS n={n} samples={s} seed_ms={seed} p50_ms={p50} p99_ms={p99}",
        s = samples.len(),
        seed = seed_elapsed.as_millis(),
        p50 = p50.as_millis(),
        p99 = p99.as_millis(),
    );

    assert!(
        p50 <= AC012_BUDGET_P50,
        "AC-012 failed: p50={p50:?} > budget {budget:?} at n={n}",
        budget = AC012_BUDGET_P50,
    );
    assert!(
        p99 <= AC012_BUDGET_P99,
        "AC-012 failed: p99={p99:?} > budget {budget:?} at n={n}",
        budget = AC012_BUDGET_P99,
    );
}

#[test]
fn ac_013_vector_retrieval_latency() {
    if !long_run_enabled() {
        return;
    }

    let n = ac013_corpus_n();
    let (_dir, path) = fixture_path("ac013_vector_retrieval");
    let embedder = Arc::new(VaryingEmbedder::new(RETRIEVAL_VECTOR_DIM));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    let seed_elapsed = seed_ac013_corpus(&opened.engine, n);

    let queries = ac013_query_bodies(PERF_SAMPLES);

    for q in &queries {
        let _ = opened.engine.search(q).expect("warmup search");
    }

    let mut samples = Vec::with_capacity(PERF_SAMPLES);
    for q in &queries {
        let started = Instant::now();
        let _ = opened.engine.search(q).expect("measure search");
        samples.push(started.elapsed());
    }

    let p50 = percentile_ceil(&samples, 50, 100);
    let p99 = percentile_ceil(&samples, 99, 100);
    eprintln!(
        "AC013_NUMBERS n={n} samples={s} seed_ms={seed} p50_ms={p50} p99_ms={p99}",
        s = samples.len(),
        seed = seed_elapsed.as_millis(),
        p50 = p50.as_millis(),
        p99 = p99.as_millis(),
    );

    assert!(
        p50 <= AC013_BUDGET_P50,
        "AC-013 failed: p50={p50:?} > budget {budget:?} at n={n}",
        budget = AC013_BUDGET_P50,
    );
    assert!(
        p99 <= AC013_BUDGET_P99,
        "AC-013 failed: p99={p99:?} > budget {budget:?} at n={n}",
        budget = AC013_BUDGET_P99,
    );
}

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
                source_id: None,
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
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("doc {i}"),
                source_id: None,
            }])
            .expect("write");
    }

    let started = Instant::now();
    opened.engine.drain(5_000).expect("drain");
    let elapsed = started.elapsed();

    eprintln!("AC018_NUMBERS drain_ms={}", elapsed.as_millis());

    assert!(
        elapsed <= Duration::from_secs(2),
        "AC-018 failed: drain took {:?}, expected <= 2 s",
        elapsed
    );
    assert_eq!(opened.engine.vector_row_count_for_test().expect("vector rows"), 100);
}

#[test]
fn ac_019_mixed_retrieval_stress_workload_tail() {
    if !long_run_enabled() {
        return;
    }

    let n = ac013_corpus_n();
    let (_dir, path) = fixture_path("ac019_mixed_retrieval");
    let embedder = Arc::new(VaryingEmbedder::new(RETRIEVAL_VECTOR_DIM));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    let seed_elapsed = seed_ac013_corpus(&opened.engine, n);

    // Baseline pass — re-run AC-013's protocol immediately preceding
    // the stress pass per acceptance.md AC-019 ("baseline_p99 is
    // captured by re-running AC-013's protocol immediately preceding
    // this AC in the same CI job").
    let queries = ac013_query_bodies(PERF_SAMPLES);
    for q in &queries {
        let _ = opened.engine.search(q).expect("baseline warmup");
    }
    let mut baseline = Vec::with_capacity(PERF_SAMPLES);
    for q in &queries {
        let started = Instant::now();
        let _ = opened.engine.search(q).expect("baseline measure");
        baseline.push(started.elapsed());
    }
    let baseline_p99 = percentile_ceil(&baseline, 99, 100);

    // Stress pass — N concurrent reader threads, mixed FTS5 + vector
    // + canonical reads. The single embedder-bearing `search()` path
    // exercises both vector ANN and FTS5 MATCH per call (see
    // `read_search_in_tx` in fathomdb-engine/src/lib.rs); mixing
    // distinct query bodies across threads keeps the working set
    // realistic.
    let engine = Arc::new(opened.engine);
    let barrier = Arc::new(Barrier::new(AC019_THREADS + 1));
    let histograms: Arc<Mutex<Vec<LatencyHistogram>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::with_capacity(AC019_THREADS);
    let stress_queries: Arc<Vec<String>> =
        Arc::new(ac013_query_bodies(AC019_QUERIES_PER_THREAD * AC019_THREADS));
    for tid in 0..AC019_THREADS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        let queries = Arc::clone(&stress_queries);
        let sink = Arc::clone(&histograms);
        handles.push(thread::spawn(move || {
            let mut hist = LatencyHistogram::new();
            let base = tid * AC019_QUERIES_PER_THREAD;
            barrier.wait();
            for i in 0..AC019_QUERIES_PER_THREAD {
                let q = &queries[(base + i) % queries.len()];
                let started = Instant::now();
                let _ = engine.search(q).expect("stress search");
                hist.record(started.elapsed());
            }
            sink.lock().unwrap().push(hist);
        }));
    }
    let stress_started = Instant::now();
    barrier.wait();
    for h in handles {
        h.join().expect("stress thread");
    }
    let stress_elapsed = stress_started.elapsed();

    let mut combined = LatencyHistogram::new();
    for hist in histograms.lock().unwrap().iter() {
        combined.merge(hist);
    }
    let stress_p99 = combined.percentile_ceil(99, 100);
    let bound = std::cmp::max(baseline_p99 * AC019_STRESS_MULT, AC019_STRESS_FLOOR);

    eprintln!(
        "AC019_NUMBERS n={n} threads={t} per_thread={p} stress_ms={se} \
         seed_ms={seed} baseline_p99_ms={bp} stress_p99_ms={sp} bound_ms={bm}",
        t = AC019_THREADS,
        p = AC019_QUERIES_PER_THREAD,
        se = stress_elapsed.as_millis(),
        seed = seed_elapsed.as_millis(),
        bp = baseline_p99.as_millis(),
        sp = stress_p99.as_millis(),
        bm = bound.as_millis(),
    );

    assert!(
        stress_p99 <= bound,
        "AC-019 failed: stress p99={stress_p99:?} > bound {bound:?} \
         (baseline_p99={baseline_p99:?}, mult={AC019_STRESS_MULT}x, floor={floor:?})",
        floor = AC019_STRESS_FLOOR,
    );
}

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
    eprintln!(
        "AC020_NUMBERS sequential_ms={} concurrent_ms={} bound_ms={}",
        sequential.as_millis(),
        concurrent.as_millis(),
        bound.as_millis(),
    );
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

// ── G.3.5 cache-pressure telemetry ───────────────────────────────────────────

/// Pack 6.G G.3.5 — read-only screening test that captures per-worker
/// `SQLITE_DBSTATUS_CACHE_HIT` / `_CACHE_MISS` / `_CACHE_USED` deltas
/// across one AC-020 concurrent body. Writes a sidecar JSON to the
/// path given by `G3_5_OUTPUT_PATH` env var so the orchestrator can
/// assemble the final per-phase JSON without re-running.
///
/// Run with:
///   `G3_5_OUTPUT_PATH=/tmp/foo.json cargo test --release \
///    -p fathomdb-engine --test perf_gates -- --ignored \
///    g3_5_cache_pressure_telemetry --nocapture`
#[cfg(debug_assertions)]
#[test]
#[ignore = "G.3.5 diagnostic: read-only cache-pressure telemetry"]
fn g3_5_cache_pressure_telemetry() {
    let output_path = std::env::var("G3_5_OUTPUT_PATH")
        .expect("G3_5_OUTPUT_PATH env var required for the G.3.5 sidecar JSON");

    let (_dir, path) = fixture_path("g3_5_cache_pressure_telemetry");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_ac020_fixture(&opened.engine);
    let worker_count = opened.engine.reader_worker_count_for_test();

    // Warmup: 16 dispatched searches so the round-robin reaches every
    // worker at least twice and the page cache reaches steady state on
    // the seeded fixture before the pre snapshot.
    for _ in 0..16 {
        let _ = opened.engine.search("semantic-0").expect("warmup search");
    }

    let pre = opened.engine.cache_status_per_worker_for_test("pre");
    assert_eq!(pre.len(), worker_count);

    // Run the AC-020 concurrent body once (8 threads x 50 rounds x 4
    // queries = 1600 dispatched searches). Same shape as
    // `ac_020_concurrent_only` but inlined so we don't depend on env-
    // gated test ordering.
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
    let concurrent_ms = started.elapsed().as_millis() as u64;

    let post = engine.cache_status_per_worker_for_test("post");
    assert_eq!(post.len(), worker_count);

    // Build per-worker telemetry as JSON-encoded bytes by hand so we
    // do not pull serde_json into the test crate. Field order matches
    // §6 of the G.3.5 prompt.
    let mut per_worker = String::from("[");
    for (idx, (p, q)) in pre.iter().zip(post.iter()).enumerate() {
        let delta_hit = i64::from(q.cache_hit) - i64::from(p.cache_hit);
        let delta_miss = i64::from(q.cache_miss) - i64::from(p.cache_miss);
        let delta_total = delta_hit + delta_miss;
        let delta_miss_rate =
            if delta_total > 0 { (delta_miss as f64) / (delta_total as f64) } else { 0.0 };
        // SQLite default cache_size is -2000 (KiB) => 2 MiB per
        // connection. No production override is in place on the F.0
        // reader connections (only `journal_mode=WAL` and `query_only=ON`
        // PRAGMAs run at open time), so the limit assumed here is the
        // canonical default.
        let cache_size_limit_bytes: f64 = 2.0 * 1024.0 * 1024.0;
        let pct = (q.cache_used_bytes as f64) / cache_size_limit_bytes;
        if idx > 0 {
            per_worker.push(',');
        }
        per_worker.push_str(&format!(
            "{{\"worker_idx\":{wi},\"pre_hit\":{ph},\"pre_miss\":{pm},\"pre_used_bytes\":{pu},\
\"post_hit\":{qh},\"post_miss\":{qm},\"post_used_bytes\":{qu},\"delta_hit\":{dh},\
\"delta_miss\":{dm},\"delta_total\":{dt},\"delta_miss_rate\":{dmr:.6},\
\"cache_used_post_pct_of_limit\":{pct:.6}}}",
            wi = idx,
            ph = p.cache_hit,
            pm = p.cache_miss,
            pu = p.cache_used_bytes,
            qh = q.cache_hit,
            qm = q.cache_miss,
            qu = q.cache_used_bytes,
            dh = delta_hit,
            dm = delta_miss,
            dt = delta_total,
            dmr = delta_miss_rate,
            pct = pct,
        ));
    }
    per_worker.push(']');

    let body = format!(
        "{{\"worker_count\":{wc},\"concurrent_ms\":{cm},\"cache_size_limit_bytes_assumed\":{lim},\
\"cache_size_limit_source\":\"sqlite default (-2000 KiB = 2 MiB per connection); no PRAGMA cache_size override on F.0 reader open path\",\
\"per_worker_telemetry\":{pw}}}",
        wc = worker_count,
        cm = concurrent_ms,
        lim = 2 * 1024 * 1024,
        pw = per_worker,
    );

    eprintln!("G3_5_TELEMETRY_JSON={body}");
    std::fs::write(&output_path, body).expect("write G.3.5 sidecar JSON");
}

// ── A.3 secondary diagnostics ────────────────────────────────────────────────

const A3_EVIDENCE_DIR: &str = "dev/plan/runs/A3-evidence";

fn a3_evidence_path(name: &str) -> std::path::PathBuf {
    // Resolve relative to repo root (two levels up from tests/).
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest.ancestors().nth(4).expect("repo root").to_path_buf();
    let dir = repo_root.join(A3_EVIDENCE_DIR);
    std::fs::create_dir_all(&dir).expect("create evidence dir");
    dir.join(name)
}

/// A.3.2 — In-process timing counters for the concurrent read path.
/// Measures total wall time per `Engine::search()`. Since `RoutedEmbedder` has no
/// delay, search_us ≈ borrow_wait + read_search_in_tx. Splitting those requires
/// production hooks; counters_collection_status is `partial`.
#[test]
#[ignore = "A.3 diagnostic: set AC020_PHASE=concurrent to opt in"]
fn ac_a3_counters_concurrent() {
    if std::env::var("AC020_PHASE").as_deref() != Ok("concurrent") {
        return;
    }

    let (_dir, path) = fixture_path("a3_counters_concurrent");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    seed_ac020_fixture(&opened.engine);

    let engine = Arc::new(opened.engine);
    let barrier = Arc::new(Barrier::new(AC020_THREADS + 1));
    let all_search_ms: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let all_embed_ms: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::with_capacity(AC020_THREADS);
    for _ in 0..AC020_THREADS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        let search_sink = Arc::clone(&all_search_ms);
        let embed_sink = Arc::clone(&all_embed_ms);
        let embedder = embedder.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            let mut local_search = Vec::new();
            let mut local_embed = Vec::new();
            for _ in 0..AC020_ROUNDS_PER_THREAD {
                for query in ac020_queries() {
                    let t_embed = Instant::now();
                    let _ = embedder.embed(query);
                    local_embed.push(t_embed.elapsed().as_micros() as u64);

                    let t_search = Instant::now();
                    engine.search(query).expect("search");
                    local_search.push(t_search.elapsed().as_micros() as u64);
                }
            }
            search_sink.lock().unwrap().extend(local_search);
            embed_sink.lock().unwrap().extend(local_embed);
        }));
    }
    barrier.wait();
    for h in handles {
        h.join().expect("thread");
    }

    let search_us = all_search_ms.lock().unwrap();
    let embed_us = all_embed_ms.lock().unwrap();
    let queries_total = search_us.len() as u64;
    let search_total_us: u64 = search_us.iter().sum();
    let embed_total_us: u64 = embed_us.iter().sum();
    // proxy: borrow+read ≈ search - embed (embed is ~0 µs for RoutedEmbedder)
    let proxy_read_total_us = search_total_us.saturating_sub(embed_total_us);

    let search_per_query_us = search_total_us.checked_div(queries_total).unwrap_or(0);
    let embed_per_query_us = embed_total_us.checked_div(queries_total).unwrap_or(0);
    let proxy_per_query_us = proxy_read_total_us.checked_div(queries_total).unwrap_or(0);

    // 4 SQL statements per search (vec0 match, canonical lookup, soft-fallback probe, fts match)
    // — constant by code inspection of read_search_in_tx.
    let prepares_per_search: u64 = 4;

    let json = format!(
        r#"{{
  "reader_borrow_ms_total": "n/a: requires production hook",
  "reader_borrow_ms_per_query": "n/a: requires production hook",
  "embedder_us_total": {embed_total_us},
  "embedder_us_per_query": {embed_per_query_us},
  "search_us_total": {search_total_us},
  "search_us_per_query": {search_per_query_us},
  "proxy_borrow_plus_read_us_total": {proxy_read_total_us},
  "proxy_borrow_plus_read_us_per_query": {proxy_per_query_us},
  "prepares_per_search": {prepares_per_search},
  "queries_total": {queries_total},
  "counters_collection_status": "partial: borrow_wait and read_search_in_tx split requires production hooks; search_us covers both",
  "note": "embed is RoutedEmbedder (instant), so proxy_borrow_plus_read_us ≈ read_search_in_tx_us + borrow_wait_us"
}}"#
    );

    let out_path = a3_evidence_path("counters.json");
    std::fs::write(&out_path, &json).expect("write counters.json");
    eprintln!("A3_COUNTERS written to {}", out_path.display());
    eprintln!("  queries_total={queries_total}");
    eprintln!("  search_us_total={search_total_us}  per_query={search_per_query_us}");
    eprintln!("  embed_us_total={embed_total_us}  per_query={embed_per_query_us}");
    eprintln!("  proxy_read_us_total={proxy_read_total_us}  per_query={proxy_per_query_us}");
}

/// A.3.3 — EXPLAIN QUERY PLAN for the four read-path SQL statements.
#[test]
#[ignore = "A.3 diagnostic: opt-in with AC020_PHASE=concurrent"]
fn ac_a3_explain_query_plan() {
    if std::env::var("AC020_PHASE").as_deref() != Ok("concurrent") {
        return;
    }

    let (_dir, path) = fixture_path("a3_explain");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_ac020_fixture(&opened.engine);
    // Engine must stay alive while we open a raw connection (WAL, shared cache).
    let db_path = opened.engine.path().to_path_buf();

    // Open a raw rusqlite connection — sqlite_vec auto-extension is process-global
    // after the first Engine::open, so vec0 virtual tables are accessible.
    let conn = rusqlite::Connection::open(&db_path).expect("raw conn");
    conn.pragma_update(None, "query_only", "ON").ok();

    // (label, sql-with-literal-placeholders-for-EXPLAIN, explain-literal-substituted)
    // EXPLAIN QUERY PLAN requires parameter binding even though it doesn't execute.
    // Use rusqlite::params! with one dummy value per ?1 slot.
    let statements: &[(&str, &str, &str)] = &[
        (
            "vec0_match",
            "SELECT rowid FROM vector_default WHERE embedding MATCH vec_f32(?1) ORDER BY distance LIMIT 10",
            "SELECT rowid FROM vector_default WHERE embedding MATCH vec_f32('[1.0,0.0,0.0]') ORDER BY distance LIMIT 10",
        ),
        (
            "canonical_lookup",
            "SELECT body FROM canonical_nodes WHERE write_cursor = ?1 LIMIT 1",
            "SELECT body FROM canonical_nodes WHERE write_cursor = 1 LIMIT 1",
        ),
        (
            "soft_fallback_probe",
            "SELECT 1
             FROM search_index
             JOIN _fathomdb_vector_kinds ON _fathomdb_vector_kinds.kind = search_index.kind
             LEFT JOIN _fathomdb_projection_terminal
               ON _fathomdb_projection_terminal.write_cursor = search_index.write_cursor
             WHERE search_index MATCH ?1
              AND _fathomdb_projection_terminal.write_cursor IS NULL
             LIMIT 1",
            "SELECT 1
             FROM search_index
             JOIN _fathomdb_vector_kinds ON _fathomdb_vector_kinds.kind = search_index.kind
             LEFT JOIN _fathomdb_projection_terminal
               ON _fathomdb_projection_terminal.write_cursor = search_index.write_cursor
             WHERE search_index MATCH 'dummy'
              AND _fathomdb_projection_terminal.write_cursor IS NULL
             LIMIT 1",
        ),
        (
            "fts_match",
            "SELECT body FROM search_index WHERE search_index MATCH ?1 ORDER BY write_cursor",
            "SELECT body FROM search_index WHERE search_index MATCH 'dummy' ORDER BY write_cursor",
        ),
    ];

    let mut out = String::new();
    let mut regression = false;

    for (label, _parametric_sql, explain_sql) in statements {
        out.push_str(&format!("=== {label} ===\n"));
        let explain = format!("EXPLAIN QUERY PLAN {explain_sql}");
        let mut stmt = conn.prepare(&explain).expect("prepare explain");
        let rows: Vec<String> = stmt
            .query_map([], |row| {
                let detail: String = row.get(3)?;
                Ok(detail)
            })
            .expect("query_map")
            .flatten()
            .collect();
        for row in &rows {
            out.push_str(&format!("  {row}\n"));
            // Flag SCAN on canonical_nodes or search_index without SEARCH — potential regression.
            if row.contains("SCAN") && !row.contains("vec0") && !row.contains("fts5") {
                regression = true;
                out.push_str("  *** REGRESSION CANDIDATE: unexpected SCAN ***\n");
            }
        }
        out.push('\n');
    }

    out.push_str(&format!("regression_observed: {regression}\n"));

    let out_path = a3_evidence_path("explain-query-plan.txt");
    std::fs::write(&out_path, &out).expect("write explain-query-plan.txt");
    eprintln!("A3_EXPLAIN written to {}", out_path.display());
    eprintln!("{out}");
}

/// A.3.4 — sqlite3_threadsafe integer + PRAGMA compile_options.
///
/// Also probes the reader-connection pragma profile (cache_size, mmap_size,
/// page_size, synchronous, journal_mode, query_only).
#[test]
#[ignore = "A.3 diagnostic: opt-in with AC020_PHASE=concurrent"]
fn ac_a3_threadsafe_and_compile_options() {
    if std::env::var("AC020_PHASE").as_deref() != Ok("concurrent") {
        return;
    }

    let (_dir, path) = fixture_path("a3_threadsafe");
    // Open via Engine to register extension and create schema.
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let db_path = opened.engine.path().to_path_buf();
    drop(opened);

    let conn = rusqlite::Connection::open(&db_path).expect("conn");

    // A.3.4a — THREADSAFE
    let threadsafe_val: i32 = unsafe { rusqlite::ffi::sqlite3_threadsafe() };
    std::fs::write(a3_evidence_path("threadsafe.txt"), format!("{threadsafe_val}\n"))
        .expect("write threadsafe.txt");
    eprintln!("A3_THREADSAFE={threadsafe_val}");

    // A.3.4b — compile_options
    let mut stmt = conn.prepare("PRAGMA compile_options").expect("prepare compile_options");
    let opts: Vec<String> =
        stmt.query_map([], |r| r.get::<_, String>(0)).expect("query_map").flatten().collect();
    let opts_text = opts.join("\n") + "\n";
    std::fs::write(a3_evidence_path("compile_options.txt"), &opts_text)
        .expect("write compile_options.txt");
    eprintln!("A3_COMPILE_OPTIONS ({} lines):\n{opts_text}", opts.len());

    // A.3.4c — reader pragma profile (WAL + query_only reader mimicking production)
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.pragma_update(None, "query_only", "ON").ok();
    let journal_mode: String =
        conn.pragma_query_value(None, "journal_mode", |r| r.get(0)).unwrap_or_default();
    let query_only: i64 = conn.pragma_query_value(None, "query_only", |r| r.get(0)).unwrap_or(0);
    let cache_size: i64 = conn.pragma_query_value(None, "cache_size", |r| r.get(0)).unwrap_or(0);
    let mmap_size: i64 = conn.pragma_query_value(None, "mmap_size", |r| r.get(0)).unwrap_or(0);
    let page_size: i64 = conn.pragma_query_value(None, "page_size", |r| r.get(0)).unwrap_or(0);
    let synchronous: i64 = conn.pragma_query_value(None, "synchronous", |r| r.get(0)).unwrap_or(0);

    let pragma_json = format!(
        r#"{{
  "journal_mode": "{journal_mode}",
  "query_only": {query_only},
  "cache_size": {cache_size},
  "mmap_size": {mmap_size},
  "page_size": {page_size},
  "synchronous": {synchronous}
}}"#
    );
    std::fs::write(a3_evidence_path("reader_pragmas.json"), &pragma_json)
        .expect("write reader_pragmas.json");
    eprintln!("A3_READER_PRAGMAS: {pragma_json}");
}
