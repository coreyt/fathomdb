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
                logical_id: None,
            }])
            .expect("vector-only write");
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("hybrid doc hybrid-{i}"),
                source_id: None,
                logical_id: None,
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
/// AC-076 (0.8.0 Slice 40) binding-tier ceiling for AC-012 text-query
/// latency, mirroring `AC013_GATE_N`. At/below this N the 20/150 ms budget
/// is asserted as a hard release gate; above it, latency is measured and
/// REPORTED (AC012_TIER_INFO) but not asserted. Slice 6 proved the latency
/// is O(N) FTS-scan cost, NOT the porter tokenizer (engine A/B: porter ≈
/// unicode61 within noise), structurally identical to the AC-013 O(N)
/// bit-KNN scan that HITL tiered (AC-072). The 100k/1M tiers are tracked
/// post-1.0 targets. See ADR-0.7.0-text-query-latency-gates-revised and
/// dev/plans/runs/0.8.0-slice-6-tokenizer-experiment-20260607T003001Z.md.
const AC012_GATE_N: usize = 10_000;
// AC-013 / AC-019 default (short) scale = the BINDING budget tier. Per
// ADR-0.7.0-text-query-latency-gates-revised (tiered budget, HITL 2026-06-01)
// the 80/300 ms budget is enforced as a release gate only at the 10k tier for
// 0.x / 1.x; the 100k and 1M tiers are tracked targets for post-1.0 (pre-2.1)
// ANN-index work, where the vec0 bit-KNN's O(N) linear scan is addressed. The
// measured 0.7.2 PR-3 numbers backing this are in
// dev/plans/runs/0.7.2-PR-3-perf-data.md.
const AC013_DEFAULT_N: usize = 10_000;
/// Binding-tier ceiling: at/below this N the 80/300 ms budget is asserted as a
/// hard gate; above it, latency is measured and REPORTED (AC013_TIER_INFO /
/// AC019_TIER_INFO) but not asserted, per the tiered ADR.
const AC013_GATE_N: usize = 10_000;
const AC019_THREADS: usize = 8;
const AC019_QUERIES_PER_THREAD: usize = 250;
const AC012_BUDGET_P50: Duration = Duration::from_millis(20);
const AC012_BUDGET_P99: Duration = Duration::from_millis(150);
// Pack 2 RED re-pin (dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md
// § Pack 2.1): canonical-CI N=1M observed p50=2048 ms / p99=2327 ms on the
// f32-brute-force read path (dev/plans/runs/0.7.0-PERF-EXP-W4.1-ac013-
// canonical-output.json). HITL-locked target post bit-quant + rerank is
// ≤ 80 / 300 ms — RED at canonical scale until P2-IMPL lands.
const AC013_BUDGET_P50: Duration = Duration::from_millis(80);
const AC013_BUDGET_P99: Duration = Duration::from_millis(300);
// Recall floor for AC-013b: HITL-locked ≥ 0.90 at k=10 vs f32 ground truth
// (dev/plans/0.7.0-HITL-recommendations.md:89; ADR-0.7.0-vector-binary-
// quant.md § 2 point 4).
const AC013B_RECALL_FLOOR: f64 = 0.90;
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

/// Embedder/partition dimension for the AC-013 / AC-019 latency fixtures.
/// Defaults to the legacy `RETRIEVAL_VECTOR_DIM` (768) for unchanged
/// committed behaviour, but is env-tunable for the 0.7.2 PR-3 local
/// canonical run: the production default embedder (bge-small) is 384-d, and
/// the bit-KNN scan + f32 rerank cost scales ~linearly with dim, so a
/// 768-d synthetic fixture overstates the production-faithful latency by
/// ~2×. Setting AC013_VECTOR_DIM=384 measures at the shipped dimension.
/// (The recall fixture stays pinned at `RETRIEVAL_VECTOR_DIM` — its
/// isotropic-noise-floor analysis is dimension-specific.)
fn retrieval_vector_dim() -> u32 {
    std::env::var("AC013_VECTOR_DIM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(RETRIEVAL_VECTOR_DIM)
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
                logical_id: None,
            });
        }
        engine.write(&batch).expect("ac-012 seed write");
        written += take;
    }
    // No vector kind configured -> projection runtime still drains FTS index.
    engine.drain(600_000).expect("ac-012 drain");
    started.elapsed()
}

/// Deterministic dense isotropic embedder. Seeds an xorshift64 PRNG
/// from FNV-1a of the input and fills every coordinate with a
/// uniformly-distributed value in [-1, 1), then L2-normalizes.
///
/// **Why dense, not sparse.** A prior version of this struct placed
/// mass on only 6 of `dim` coordinates (FNV-driven slot picks). That
/// is pathological for binary quantization: `vec_quantize_binary`
/// takes the sign bit of every coordinate, so for a dim-768 sparse
/// vector with 6 non-zero coords, ~762 of 768 sign bits encode the
/// IEEE sign of an exact 0.0 (positive). The bit-distance between
/// any two corpus vectors then carries only ~6 bits of signal — bit-
/// KNN top-K=64 returns near-random candidates and AC-013b recall@10
/// collapsed to 0.157 on N=10K once the unrelated batch-collapse bug
/// was fixed. See `dev/plans/runs/STATUS-perf-vector-quant.md`
/// "Fixture-replacement evaluation" for the full analysis.
///
/// **Why isotropic.** Per-coordinate variance is uniform across
/// dim, which is the regime where Charikar's `1 − θ/π` SimHash
/// collision-probability bound is tight. Real embeddings are
/// anisotropic (Ethayarajh 2019; Gao 2019 "cone effect") and will
/// score *lower* than this fixture under sign-bit quantization —
/// see the e5-base-v2 reference at 74.8% NDCG@10 retention. So this
/// fixture's recall@10 is a NECESSARY-but-not-SUFFICIENT condition
/// for the AC-013b 0.90 HITL-locked floor against real embeddings;
/// real-embedding validation is deferred to 0.7.1 EMBEDDER-UNDEFER
/// per `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md`.
#[derive(Clone, Debug)]
struct VaryingEmbedder {
    identity: EmbedderIdentity,
    dim: u32,
}

impl VaryingEmbedder {
    fn new(dim: u32) -> Self {
        Self { identity: EmbedderIdentity::new("varying", "perf-gates-dense", dim), dim }
    }

    fn vector_for(&self, text: &str) -> Vector {
        let dim = self.dim as usize;
        // FNV-1a 64-bit on the input — same hash as the prior sparse
        // version, so seed determinism is unchanged.
        let mut h: u64 = 0xcbf29ce484222325;
        for &b in text.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        // Avoid the all-zero xorshift fixed point for empty input.
        if h == 0 {
            h = 0xdeadbeef_cafebabe;
        }
        let mut state = h;
        let mut v = Vec::with_capacity(dim);
        for _ in 0..dim {
            // xorshift64 — full-period, zero-pole-avoidance above.
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // Map u64 → [-1, 1) via the i32 cast of the low 32 bits.
            let x = ((state as u32) as i32 as f32) / (i32::MAX as f32 + 1.0);
            v.push(x);
        }
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
                logical_id: None,
            });
        }
        engine.write(&batch).expect("ac-013 seed write");
        written += take;
    }
    // Final drain budget is env-tunable for the local canonical run: seeding
    // 1M synthetic rows through the per-row projection path can exceed the
    // historical 30-min cap on a contended dev box (0.7.2 PR-3 observed an
    // `Err(Scheduler)` = wait_for_idle timeout, NOT a wedge, at the 1.8M ms
    // default). Canonical CI keeps the default; a local maintainer running
    // N=1M sets AC013_DRAIN_TIMEOUT_MS to budget the longer drain.
    let drain_ms: u64 = std::env::var("AC013_DRAIN_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_800_000);
    engine.drain(drain_ms).expect("ac-013 drain");
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

    // AC-076 tiered budget: binding gate only at the 10k tier (0.x/1.x); 100k
    // and 1M are tracked post-1.0 targets (O(N) FTS MATCH scan + bm25 over the
    // matched-row set; the tokenizer is exonerated — Slice 6). Mirrors the
    // AC-013 AC013_GATE_N branch below. See
    // ADR-0.7.0-text-query-latency-gates-revised + the Slice 6 experiment.
    if n <= AC012_GATE_N {
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
    } else {
        eprintln!(
            "AC012_TIER_INFO n={n} p50_ms={} p99_ms={} budget_p50_ms={} budget_p99_ms={} \
             binding=false (tracked post-1.0 target per tiered ADR; gated only at N<={})",
            p50.as_millis(),
            p99.as_millis(),
            AC012_BUDGET_P50.as_millis(),
            AC012_BUDGET_P99.as_millis(),
            AC012_GATE_N,
        );
    }
}

#[test]
fn ac_013_vector_retrieval_latency() {
    if !long_run_enabled() {
        return;
    }

    let n = ac013_corpus_n();
    let (_dir, path) = fixture_path("ac013_vector_retrieval");
    let embedder = Arc::new(VaryingEmbedder::new(retrieval_vector_dim()));
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

    // Tiered budget: binding gate only at the 10k tier (0.x/1.x); 100k and 1M
    // are tracked post-1.0 targets (O(N) bit-KNN scan; ANN-index work). See
    // ADR-0.7.0-text-query-latency-gates-revised + 0.7.2-PR-3-perf-data.md.
    if n <= AC013_GATE_N {
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
    } else {
        eprintln!(
            "AC013_TIER_INFO n={n} p50_ms={} p99_ms={} budget_p50_ms={} budget_p99_ms={} \
             binding=false (tracked post-1.0 target per tiered ADR; gated only at N<={})",
            p50.as_millis(),
            p99.as_millis(),
            AC013_BUDGET_P50.as_millis(),
            AC013_BUDGET_P99.as_millis(),
            AC013_GATE_N,
        );
    }
}

#[test]
fn ac_013b_recall_at_10_floor() {
    // AC-075 (0.8.0 Slice 40): this synthetic gate is REPORT-ONLY. It
    // measures bit-KNN + f32-rerank quantization fidelity against the
    // brute-force f32 top-10 on the isotropic `VaryingEmbedder` and prints
    // `RECALL_FIDELITY_INFO`; it no longer hard-asserts the 0.90 floor
    // (isotropic noise is the worst case for sign-bit ANN, ~0.73–0.89 < 0.90,
    // a fixture property not a product defect). The asserting recall verdict
    // is now `eu7_real_corpus_ac.rs` on the REAL embedder, measured on the
    // VECTOR STAGE in isolation (◆ B-1 correction): ANN+ vector top-10 vs the
    // exact-f32 VECTOR top-10 ground truth, recall@10 = 0.937. The name is
    // kept for history; the floor constant + its sentinel
    // `ac_013b_floor_matches_adr` are retained (see AC013B_RECALL_FLOOR).
    //
    // 0.7.2 reframe: the corrected ANN / quantization-FIDELITY measurement
    // on the real default embedder (bge-small, candle; EU-7 corpus N=7667)
    // is recall@10 = 0.937 (CI 0.913-0.957, sigma 0.0116) — full CI above
    // 0.90. The earlier 0.828 was a conservative-measurement artifact
    // (exclude-after + body-string GT); see ADR-0.7.0-vector-binary-quant.md
    // § 2 point 4 and dev/plans/runs/0.7.2-PR-2c-recall-rootcause.md. This
    // is ANN fidelity (GT = exact f32 top-10 over the SAME model), NOT
    // IR-relevance — the separate EU-8 IR ceiling (0.571) is not a gate.
    if !long_run_enabled() {
        return;
    }

    let n = ac013_corpus_n();
    let (_dir, path) = fixture_path("ac013b_recall");
    let embedder = Arc::new(VaryingEmbedder::new(RETRIEVAL_VECTOR_DIM));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    let _seed_elapsed = seed_ac013_corpus(&opened.engine, n);

    let queries = ac013_query_bodies(PERF_SAMPLES);

    // Raw read-only connection for the f32 ground-truth pass. sqlite_vec
    // is process-global after Engine::open, so vec0 vtabs are reachable.
    // Mirrors the SQL at src/rust/crates/fathomdb-engine/src/lib.rs:2317-
    // 2342 (rowid lookup against vector_default, body fetch against
    // canonical_nodes by write_cursor).
    let db_path = opened.engine.path().to_path_buf();
    let conn = rusqlite::Connection::open(&db_path).expect("raw ground-truth conn");
    conn.pragma_update(None, "query_only", "ON").ok();

    let mut total_hits = 0usize;
    let mut total_queries = 0usize;

    for q in &queries {
        let vector = embedder.embed(q).expect("embed");
        let vector_json = serde_json::to_string(&vector).expect("json");

        let mut gt_rowid_stmt = conn
            .prepare(
                "SELECT rowid
                 FROM vector_default
                 WHERE embedding MATCH vec_f32(?1)
                 ORDER BY distance
                 LIMIT 10",
            )
            .expect("prepare gt rowid");
        let gt_rowids: Vec<i64> = gt_rowid_stmt
            .query_map([&vector_json], |row| row.get::<_, i64>(0))
            .expect("gt rowid query")
            .filter_map(|r| r.ok())
            .collect();

        let mut body_stmt = conn
            .prepare("SELECT body FROM canonical_nodes WHERE write_cursor = ?1 LIMIT 1")
            .expect("prepare body");
        let mut gt_bodies = Vec::with_capacity(gt_rowids.len());
        for rowid in &gt_rowids {
            if let Ok(body) = body_stmt.query_row([rowid], |row| row.get::<_, String>(0)) {
                gt_bodies.push(body);
            }
        }

        let prod: Vec<String> = opened
            .engine
            .search(q)
            .expect("measure search")
            .results
            .iter()
            .map(|h| h.body.clone())
            .collect();

        let gt_set: std::collections::HashSet<&String> = gt_bodies.iter().collect();
        let hits = prod.iter().filter(|b| gt_set.contains(b)).count();
        total_hits += hits;
        total_queries += 1;
    }

    let recall = total_hits as f64 / (10.0 * total_queries.max(1) as f64);
    eprintln!("RECALL_NUMBERS n={n} samples={s} recall_at_10={recall:.4}", s = total_queries,);

    // AC-075 (0.8.0 Slice 40) — REPORT-ONLY. This synthetic isotropic
    // `VaryingEmbedder` recall is a quantization-FIDELITY signal, not a
    // product recall floor: isotropic random vectors are the noise-limited
    // worst case for sign-bit ANN, so this number (~0.73–0.89 depending on
    // N) does not — and was never meant to — clear the 0.90 product floor.
    // The asserting recall verdict moved to the REAL-embedder
    // `eu7_real_corpus_ac.rs`, measured on the VECTOR STAGE in isolation
    // (◆ B-1): ANN+ vector top-10 vs exact-f32 VECTOR top-10, recall@10 =
    // 0.937. The `AC013B_RECALL_FLOOR` constant and its sentinel
    // `ac_013b_floor_matches_adr` are retained as the documented floor the
    // eu7 verdict enforces. See ADR-0.7.0-vector-binary-quant.md § 2 point 4.
    eprintln!(
        "RECALL_FIDELITY_INFO n={n} recall_at_10={recall:.4} product_floor={floor:.2} \
         binding=false (synthetic isotropic quantization-fidelity signal; the asserting \
         recall verdict is eu7_real_corpus_ac.rs on the real embedder, vector stage — AC-075)",
        floor = AC013B_RECALL_FLOOR,
    );
}

/// Fast, non-gated sentinel: the recall-floor constant must stay in lockstep
/// with the value documented in ADR-0.7.0-vector-binary-quant.md § 2 point 4
/// (HITL-locked ≥ 0.90 recall@10 vs f32 brute-force ground truth). The ADR § 2
/// point 4 amendment cites the corrected ANN-fidelity anchor 0.937 but keeps
/// the floor itself at 0.90 (conservative; R-2sigma = 0.914). If anyone changes
/// AC013B_RECALL_FLOOR without re-ratifying the ADR (or vice versa), this test
/// catches the drift. It does NOT seed a corpus, so it always runs.
#[test]
fn ac_013b_floor_matches_adr() {
    assert_eq!(
        AC013B_RECALL_FLOOR, 0.90,
        "AC013B_RECALL_FLOOR must remain 0.90 to match the HITL-locked value in \
         ADR-0.7.0-vector-binary-quant.md § 2 point 4; changing it requires \
         re-ratifying that ADR section."
    );
}

/// CI "not broken" smoke for the AC-013 two-phase vector read path.
///
/// The canonical AC-013 / AC-013b / AC-019 gates are `AGENT_LONG`-gated and
/// run only as a local once-per-release exercise: the real-corpus +
/// real-embedder N=1M measurement is infeasible on the 4-core canonical
/// runner (~166 h of serialized bge seed at the PR-9-measured 1.67 docs/s
/// vs the 240 min workflow timeout — see
/// `dev/plans/runs/0.7.2-PR-3-output.json` and
/// `dev/notes/ac013-ac019-canonical-scale-policy.md`). So in normal CI those
/// tests early-return and nothing exercises the bit-KNN + f32 rerank read
/// path. This fast, always-on test fills that gap: it confirms the whole
/// write → projection → embed → sign-bit quantize → vec0 → two-phase
/// bit-KNN + f32 rerank → canonical-body fetch pipeline is wired and
/// returns the exact nearest neighbour, WITHOUT asserting any latency or
/// recall budget (those belong to the local canonical gates).
///
/// **FTS-isolated by construction.** The query is a token that appears in NO
/// seeded document, so the FTS5 stage of `search()` matches zero rows and any
/// result can ONLY have come from the vector (bit-KNN + f32 rerank) stage. A
/// naive body-as-query form would NOT isolate the vector path: `search()`
/// appends FTS5 hits after vector hits (even when the vector stage returns
/// nothing) and the query is compiled to a quoted FTS phrase, so an exact body
/// match can be returned via the text path alone — the smoke would pass even if
/// bit-KNN/rerank were dead (codex BLOCK, 2026-06-01; this is the fix).
///
/// **Deterministic, non-flaky correctness.** `SMOKE_N < TOP_K_BIT_CANDIDATES`
/// (64 < 192), so the bit-KNN candidate stage returns *every* row and the f32
/// rerank is therefore exact — the true f32-nearest body (computed in-test) is
/// guaranteed rank 1, with none of the candidate-set lossiness that makes
/// recall < 1 at scale. Uses the synthetic `VaryingEmbedder` (whose identity
/// does not request mean-centering, so the query vector is byte-identical to
/// the stored vectors), so it needs neither the `default-embedder` feature nor
/// the on-disk corpus and stays green on every CI runner.
#[test]
fn ac_013_vector_read_path_smoke() {
    // < TOP_K_BIT_CANDIDATES (192): every row is a bit-KNN candidate -> exact rerank.
    const SMOKE_N: usize = 64;
    // A single FTS token present in NO seeded body (bodies are space-joined
    // `perf_vocab` tokens like "abc0001"). FTS5 MATCH on it returns zero rows.
    const VECTOR_PROBE_QUERY: &str = "zzvectorpathprobezz";

    let (_dir, path) = fixture_path("ac013_read_path_smoke");
    let embedder = Arc::new(VaryingEmbedder::new(RETRIEVAL_VECTOR_DIM));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let vocab = perf_vocab();
    let cumulative = zipf_cumulative(vocab.len());
    let mut rng = SeededRng::new(0x0005_0CE0_0AC0_13D0);
    let bodies: Vec<String> =
        (0..SMOKE_N).map(|_| synth_chunk_body(&mut rng, &vocab, &cumulative)).collect();
    let batch: Vec<PreparedWrite> = bodies
        .iter()
        .map(|b| PreparedWrite::Node {
            kind: "doc".to_string(),
            body: b.clone(),
            source_id: None,
            logical_id: None,
        })
        .collect();
    opened.engine.write(&batch).expect("smoke seed write");
    opened.engine.drain(60_000).expect("smoke drain");

    // Projection completed for every seeded row.
    assert_eq!(
        opened.engine.vector_row_count_for_test().expect("vector rows") as usize,
        SMOKE_N,
        "projection must index every seeded row before the read-path check"
    );

    // True f32-nearest seeded body to the probe query's embedding (in-test).
    let qv = embedder.embed(VECTOR_PROBE_QUERY).expect("embed probe");
    let body_vecs: Vec<Vector> = bodies.iter().map(|b| embedder.embed(b).expect("embed")).collect();
    let dist = |v: &[f32]| -> f32 { v.iter().zip(&qv).map(|(a, b)| (a - b) * (a - b)).sum() };
    let nearest_idx = (0..SMOKE_N)
        .min_by(|&a, &b| {
            dist(&body_vecs[a])
                .partial_cmp(&dist(&body_vecs[b]))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("nearest");
    let nearest = &bodies[nearest_idx];

    // FTS5 matches zero rows for the probe token, so these results come SOLELY
    // from bit-KNN + f32 rerank. If that stage is broken and returns nothing,
    // `search()` returns empty here and the test fails (the property the old
    // body-as-query form could not guarantee).
    let result = opened.engine.search(VECTOR_PROBE_QUERY).expect("smoke search");
    let result_bodies: Vec<String> = result.results.iter().map(|h| h.body.clone()).collect();
    assert!(
        !result_bodies.is_empty(),
        "vector read path returned no results for an FTS-absent query — bit-KNN/rerank is broken"
    );
    assert!(
        result_bodies.iter().all(|b| bodies.contains(b)),
        "vector results must be seeded corpus bodies; got {:?}",
        result_bodies
    );
    // SMOKE_N < K => exact rerank => the true f32-nearest is rank 1.
    assert_eq!(
        result_bodies.first(),
        Some(nearest),
        "exact f32-nearest must rank 1 through bit-KNN + f32 rerank (SMOKE_N<K => exact rerank); \
         got top-{} = {:?}",
        result_bodies.len(),
        result_bodies.first(),
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
                logical_id: None,
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
                logical_id: None,
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
    let embedder = Arc::new(VaryingEmbedder::new(retrieval_vector_dim()));
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

    // REPORT-ONLY on the synthetic fixture (HITL 2026-06-01). The synthetic
    // VaryingEmbedder cannot meet AC-019's `max(baseline_p99*10, 150ms)` bound,
    // and this is a property of the synthetic DATA, not the box: its embed is
    // instant, so the single-thread baseline (~16-28 ms) is unrealistically
    // fast, which makes the 10x bound far tighter than production while the
    // isotropic vectors give no concurrency relief — the absolute 8-thread tail
    // (~520 ms @384d / ~1050 ms @768d at N=10k) is comparable to the real path
    // but the bound is not meetable. The VERDICT-QUALITY AC-019 signal is the
    // real-corpus harness `eu7_real_corpus_ac.rs` (bge-small, dim 384), which
    // PASSES at the 10k tier (baseline_p99 40 ms, stress_p99 343 ms < bound
    // 405 ms); per `dev/notes/ac013-ac019-canonical-scale-policy.md` synthetic
    // dev-box numbers are scouting, not verdicts. So this test measures and
    // REPORTS but does not assert. Full data: dev/plans/runs/0.7.2-PR-3-perf-data.md.
    let synthetic_passes = stress_p99 <= bound;
    eprintln!(
        "AC019_REPORT_ONLY n={n} stress_p99_ms={} bound_ms={} synthetic_meets_bound={} \
         (report-only; verdict is the real-corpus eu7 harness — synthetic isotropic data \
         cannot meet the baseline-relative bound, see ADR-0.7.0-text-query-latency-gates-revised)",
        stress_p99.as_millis(),
        bound.as_millis(),
        synthetic_passes,
    );
}

#[test]
fn ac_020_reads_do_not_serialize_on_a_single_reader_connection() {
    if !long_run_enabled() {
        return;
    }

    let (_dir, path) = fixture_path("ac020_read_mix");
    let embedder = Arc::new(RoutedEmbedder::new(8));
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
    let embedder = Arc::new(RoutedEmbedder::new(8));
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
    let embedder = Arc::new(RoutedEmbedder::new(8));
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
    let embedder = Arc::new(RoutedEmbedder::new(8));
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

const A3_EVIDENCE_DIR: &str = "dev/plans/runs/A3-evidence";

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
    let embedder = Arc::new(RoutedEmbedder::new(8));
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
    let embedder = Arc::new(RoutedEmbedder::new(8));
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
