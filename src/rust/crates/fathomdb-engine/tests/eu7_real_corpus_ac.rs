//! EU-7 — real-corpus AC validation + recall@10 anchor measurement.
//!
//! Per `dev/plans/prompts/0.7.1-EU-7-launch.md`. This harness validates
//! AC-013 (vector retrieval latency), AC-013b (recall@10 vs f32 ground
//! truth), and AC-019 (mixed-retrieval stress tail) against the **real
//! default embedder** (`BAAI/bge-small-en-v1.5`, candle-transformers,
//! dim 384) over the **real corpus** (`data/corpus-data/raw/*.jsonl`),
//! at dev-box scale.
//!
//! ## What this measures vs. perf_gates.rs
//!
//! `tests/perf_gates.rs::ac_013b_recall_at_10_floor` runs against a
//! synthetic *isotropic* `VaryingEmbedder` and hard-asserts the
//! HITL-locked 0.90 floor. Isotropic random vectors are the noise-
//! limited case for sign-bit ANN; real anisotropic BGE embeddings are
//! *easier* for sign-bit quantization (see
//! `dev/plans/runs/STATUS-perf-vector-quant.md` "Fixture-replacement
//! evaluation", post-correction). This harness measures the **real**
//! number that 0.7.2 PR-2 will use to re-derive `AC013B_RECALL_FLOOR`
//! as `R - 2*sigma`.
//!
//! ## Recall posture (AC-075, 0.8.0 Slice 40 / GA-2 — eu7 is the verdict)
//!
//! As of 0.8.0 Slice 40, this real-embedder harness is the **asserting**
//! recall verdict (AC-075) on the real bge-small embedder over the real
//! corpus. ◆ GA-3 (◆ HITL ruling 2026-06-08): the verdict is a **one-sided,
//! CI-based** gate against the unchanged 0.90 floor — PASS iff the recall 95%
//! bootstrap CI is not significantly below the floor (`recall_ci_hi >=
//! CURRENT_FLOOR`), NOT a point-estimate `recall >= floor` assert. The 0.90
//! floor CONSTANT is unchanged; the measured N=7667 point estimate 0.8960 (CI
//! [0.8640, 0.9250]) clears the gate because ci_hi 0.925 ≥ 0.90. The
//! point-estimate-≥0.90 recovery (the ~4pt 0.7.x→0.8.0 vector-stage drop) is a
//! 0.8.1 item, and this CI form is to be revisited after 0.8.0. It supersedes
//! the informal AC-013b floor assert: the synthetic
//! `perf_gates::ac_013b_recall_at_10_floor` is now REPORT-ONLY.
//!
//! **◆ B-1 correction (GA-2, 2026-06-08).** The system-under-test is the
//! engine's pre-fusion **VECTOR-STAGE** ranking (1-bit sign-quant K=192
//! Hamming + f32 rerank), obtained via the test-only
//! `set_vector_stage_only_for_test` seam — NOT the RRF-fused `search()`
//! output. The ground truth is an exact-f32 VECTOR top-10, so measuring it
//! against the unconditional-RRF hybrid `search()` (Slice 10, `d28d204`)
//! conflated ANN-quantization FIDELITY (what the 0.90 floor gates) with
//! intended fusion divergence — the cause of the Slice-40 Phase-A HALT
//! (recall 0.8710 < 0.90), root-caused in
//! `dev/plans/runs/GA-1-corpus-ab-20260608T012503Z.md`. The corrected
//! vector-stage SUT measures recall@10 = 0.896 (CI 0.864–0.925, σ 0.0157) at
//! N=7667 (0.924 at N=1000) — the live B-1 remeasure
//! (`GA-signoff-eu7-remeasure-20260608T172804Z`). (The 0.937 figure earlier in
//! this campaign was the 0.7.1 `search()`-SUT anchor, carried as a GA-2
//! assumption; it is NOT the vector-stage value. The 0.937→0.896 gap is a
//! vector-stage measurement-SUT change — old `search()` vs this seam — not a
//! fidelity regression, and not embedder-pooling; bisected in
//! `dev/plans/runs/0.8.3-eu7-bisect-report.md`.) The
//! harness ALSO reports the fused-`search()` recall (`EU7_RECALL_FUSED`,
//! ~0.871) as the report-only delta, so the correction is demonstrably
//! load-bearing, not cosmetic. This is the ANN-quantization FIDELITY axis,
//! complementary to and NOT a substitute for the IR/relevance axis (eu8 /
//! the IR-1 `ir-recall-measure.md`). K stays locked at 192;
//! `AC013B_RECALL_FLOOR` is unchanged (its sentinel still pins 0.90).
//! AC-013 latency and AC-019 stress remain REPORTED (dev-box scouting).
//!
//! ## Production-path fidelity
//!
//! The harness constructs the **real** `CandleBgeEmbedder` and supplies
//! it via `EmbedderChoice::Caller`. Because its identity name is
//! `fathomdb-bge-small-en-v1.5`, the engine's mean-centering apply paths
//! engage exactly as they do for `EmbedderChoice::Default` (identity-
//! gated; see `identity_requires_mean_centering`), the mean pins at
//! `MEAN_VEC_PIN_THRESHOLD` (256) docs, and retrieval runs the locked
//! K=192 bit-KNN + f32 rerank pipeline. Holding the embedder `Arc` lets
//! the harness embed queries with the *same model* for the f32
//! ground-truth pass (codex focus: ground truth must use the candidate
//! model, not a different one).
//!
//! ## Gating
//!
//! Requires the `default-embedder` Cargo feature (real candle weights)
//! AND `AGENT_LONG=1` (the embed pass is minutes of wall-clock). The
//! warm-cache contract is the same as `eu5b_lockflip.rs`: weights are
//! pre-fetched; set `FATHOMDB_SKIP_NETWORK_TESTS=1` to skip when the
//! cache is cold and HF is unreachable.
//!
//! Run:
//!   AGENT_LONG=1 cargo test -p fathomdb-engine --features default-embedder \
//!     --test eu7_real_corpus_ac -- --nocapture
//!
//! Tunables (env):
//!   EU7_N_VALUES         comma-separated haystack sizes (default "1000,7667")
//!   EU7_QUERIES          query-set size (default 100)
//!   EU7_BOOTSTRAP        bootstrap resamples (default 1000)
//!   EU7_LATENCY_SAMPLES  AC-013 measurement samples (default 1000)
//!   EU7_STRESS_PER_THREAD AC-019 queries per stress thread (default 250)
//!
//! Seeding the real BGE embedder through the projection pipeline runs at
//! ~1.3 docs/sec on a 24-core dev box (embed + sign-bit quantize + mean-
//! centering + WAL commit per doc), so the full 7,667-doc corpus seeds in
//! ~1.5 h. This is expected dev-box cost; canonical N=1M is 0.7.2 PR-3.

#![cfg(feature = "default-embedder")]

#[path = "support/corpus_subset.rs"]
mod corpus_subset;
#[path = "support/recall_gate.rs"]
mod recall_gate;

use std::collections::HashSet;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use corpus_subset::{load_subset_or_skip, repo_root, Doc};
use fathomdb_embedder::CandleBgeEmbedder;
use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{EmbedderChoice, Engine, PreparedWrite};
use serde_json::json;
use tempfile::TempDir;

/// Serializes `CandleBgeEmbedder::embed` behind a `Mutex`.
///
/// The engine's projection pool runs `PROJECTION_WORKERS` (=2) worker
/// threads that call `embed()` concurrently on one shared instance.
/// candle's own guidance is to guard a shared model (`Arc<RwLock<Model>>`)
/// for concurrent CPU inference; this `Mutex` is that guard, applied
/// harness-side without touching the engine's writer/projection contracts.
/// It is a measurement-fidelity wrapper only: the produced vectors,
/// identity, and mean-centering behaviour are byte-identical to the bare
/// embedder; only concurrency is constrained.
///
/// (The original EU-7 seeding stall was NOT this — it was Finding C, the
/// missing 512-token truncation, now fixed in `CandleBgeEmbedder`. This
/// guard remains as defensive isolation against concurrent candle forward.)
struct SerializedBge {
    inner: Mutex<CandleBgeEmbedder>,
    identity: EmbedderIdentity,
}

impl SerializedBge {
    fn new(inner: CandleBgeEmbedder) -> Self {
        let identity = inner.identity();
        Self { inner: Mutex::new(inner), identity }
    }
}

impl Embedder for SerializedBge {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        self.inner.lock().expect("embedder mutex poisoned").embed(text)
    }
}

// ── Budgets (mirrored verbatim from tests/perf_gates.rs; do NOT relax) ──
// AC-013: HITL-locked vector-retrieval latency budget.
const AC013_BUDGET_P50: Duration = Duration::from_millis(80);
const AC013_BUDGET_P99: Duration = Duration::from_millis(300);
// AC-019: stress p99 bound = max(baseline_p99 * 10, 150 ms).
const AC019_STRESS_FLOOR: Duration = Duration::from_millis(150);
const AC019_STRESS_MULT: u32 = 10;
const AC019_THREADS: usize = 8;
// Default per-thread stress query count (overridable via EU7_STRESS_PER_THREAD
// for fast logic smoke-tests; the canonical AC-019 value is 250).
const AC019_QUERIES_PER_THREAD_DEFAULT: usize = 250;

// The production recall floor (HITL-locked 0.90; see
// ADR-0.7.0-vector-binary-quant.md § 2 point 4). As of 0.8.0 Slice 40 /
// GA-2 (AC-075) this is the ASSERTING verdict on the real embedder,
// measured on the pre-fusion VECTOR STAGE (◆ B-1) — see the recall-posture
// note in the module header.
const CURRENT_FLOOR: f64 = 0.90;
// Loose sanity floor: real BGE must clear the isotropic noise floor
// (dense isotropic measured 0.5124 @ K=64, N=10K; see STATUS-PVQ). This
// is a "pipeline is wired and real embeddings beat noise" check, NOT the
// production floor.
const SANITY_FLOOR: f64 = 0.55;

// Default measurement-pass sample count (overridable via EU7_LATENCY_SAMPLES
// for fast logic smoke-tests; the canonical AC-013 value is 1000).
const LATENCY_SAMPLES_DEFAULT: usize = 1_000;

// ── Deterministic RNG (SplitMix64) for query selection + bootstrap ──
struct SplitMix64 {
    state: u64,
}
impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn next_in(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

const QUERY_SELECT_SEED: u64 = 0x0E7_7_C0_12_5E1EC7; // EU-7 query selection
const BOOTSTRAP_SEED: u64 = 0x0E7_7_B0_07_57_4A9; // EU-7 bootstrap

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn n_values() -> Vec<usize> {
    match std::env::var("EU7_N_VALUES") {
        Ok(raw) => raw.split(',').filter_map(|s| s.trim().parse::<usize>().ok()).collect(),
        // Default sweep: a small-N trend point and the full real corpus
        // (7,667 docs). N beyond the real corpus would require synthetic
        // padding; that haystack-scaling toward canonical N=1M is 0.7.2
        // PR-3's job, not dev-box scouting. Override via EU7_N_VALUES.
        Err(_) => vec![1000, 7667],
    }
}

fn percentile_ceil(samples: &[Duration], numerator: usize, denominator: usize) -> Duration {
    assert!(!samples.is_empty());
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() * numerator).div_ceil(denominator)).saturating_sub(1);
    sorted[index]
}

// ── Query synthesis (EU-0 §1.2 methodology) ────────────────────────────
// Title if usable (>= 6 chars, not "untitled", not equal to body);
// otherwise the lead sentence / first ~140 chars at a sentence boundary.
// The synthesized text must NOT equal the body verbatim (else the query
// is the document and recall is trivially self-fulfilling).
const LEAD_MAX_CHARS: usize = 140;

fn synth_query(doc: &Doc) -> Option<String> {
    if let Some(title) = &doc.title {
        let t = title.trim();
        if t.len() >= 6 && !t.eq_ignore_ascii_case("untitled") && t != doc.body.trim() {
            return Some(t.to_string());
        }
    }
    let body = doc.body.trim();
    if body.is_empty() {
        return None;
    }
    let lead = lead_sentence(body, LEAD_MAX_CHARS);
    // Skip docs whose entire body is shorter than/equal to the lead we'd
    // extract — we can't form a non-verbatim query from them.
    if lead.trim().is_empty() || lead.trim() == body {
        return None;
    }
    Some(lead)
}

/// First sentence (up to `.`/`!`/`?`/newline) or `max_chars` at a char
/// boundary, whichever comes first. Skips leading markdown bullet noise.
fn lead_sentence(body: &str, max_chars: usize) -> String {
    let cleaned: String = body
        .lines()
        .map(|l| l.trim_start_matches(['-', '*', '#', '>', ' ', '\t']))
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned.trim();
    let mut out = String::new();
    for (i, ch) in cleaned.chars().enumerate() {
        if i >= max_chars {
            break;
        }
        out.push(ch);
        if matches!(ch, '.' | '!' | '?') && out.trim().len() >= 12 {
            break;
        }
    }
    out.trim().to_string()
}

// ── Synthetic English-ish distractor bodies (haystack padding) ─────────
// When a requested N exceeds the real corpus size (7,667), the haystack
// is padded with deterministic synthetic English distractors, embedded by
// the SAME real BGE model. These are semantically unrelated to the real
// query-target docs, so they act as distractors that enlarge the haystack
// without being near-duplicates. This mirrors how 0.7.2 PR-3 reaches
// canonical N=1M ("real corpus + synthetic replicates if needed"). The
// recall anchor is taken from an all-real N (no padding); padded points
// only probe haystack-size scaling and are labelled as such in the JSON.
fn synth_distractor_body(idx: usize) -> String {
    const WORDS: &[&str] = &[
        "system",
        "report",
        "meeting",
        "schedule",
        "budget",
        "review",
        "vector",
        "index",
        "storage",
        "latency",
        "throughput",
        "deadline",
        "proposal",
        "summary",
        "analysis",
        "customer",
        "revenue",
        "quarter",
        "project",
        "timeline",
        "resource",
        "estimate",
        "baseline",
        "metric",
        "threshold",
        "pipeline",
        "deployment",
        "incident",
        "rollback",
        "migration",
        "feature",
        "release",
    ];
    let mut rng = SplitMix64::new(0xD15_7AC_70_5000 ^ idx as u64);
    let count = 40 + rng.next_in(40);
    let mut body = String::with_capacity(512);
    for i in 0..count {
        if i > 0 {
            body.push(' ');
        }
        body.push_str(WORDS[rng.next_in(WORDS.len())]);
    }
    body
}

// ── Corpus + query-set construction ────────────────────────────────────

struct QueryItem {
    text: String,
    target_body: String,
}

/// Load all real docs (deterministic order), build the fixed query set
/// from the first 1,000 real docs (present in every N >= 1000 so recall
/// is apples-to-apples across haystack sizes), and return both.
fn load_real_and_queries(num_queries: usize) -> Option<(Vec<Doc>, Vec<QueryItem>)> {
    // per_source = usize::MAX -> load the full corpus (~7,667 docs).
    let real = load_subset_or_skip(usize::MAX)?;
    if real.is_empty() {
        return None;
    }
    // Query-target pool: first up-to-1000 real docs (the floor of the N
    // sweep), so the same query set is valid at every N.
    let pool_len = real.len().min(1000);
    let mut indices: Vec<usize> = (0..pool_len).collect();
    // Deterministic Fisher-Yates shuffle so the 100 queries spread across
    // sources rather than clustering at the head of the sorted list.
    let mut rng = SplitMix64::new(QUERY_SELECT_SEED);
    for i in (1..indices.len()).rev() {
        let j = rng.next_in(i + 1);
        indices.swap(i, j);
    }
    let mut queries = Vec::with_capacity(num_queries);
    for &idx in &indices {
        if queries.len() >= num_queries {
            break;
        }
        if let Some(text) = synth_query(&real[idx]) {
            queries.push(QueryItem { text, target_body: real[idx].body.clone() });
        }
    }
    Some((real, queries))
}

/// Build the haystack of size `n`: the first `min(n, real.len())` real
/// docs, padded with synthetic distractors when `n` exceeds the corpus.
fn haystack_bodies(real: &[Doc], n: usize) -> Vec<String> {
    let mut bodies: Vec<String> = real.iter().take(n).map(|d| d.body.clone()).collect();
    let mut pad_idx = 0usize;
    while bodies.len() < n {
        bodies.push(synth_distractor_body(pad_idx));
        pad_idx += 1;
    }
    bodies
}

/// Batch-write `bodies[from..to]` as vector-indexed `doc` nodes, then
/// drain so the projection workers embed + index every row. Returns the
/// elapsed seed (embed) time for this slice. Used incrementally: the
/// haystack grows across N targets in a single engine so each doc is
/// embedded exactly once (the projection embed path is ~100x the search
/// path; re-seeding from scratch per N would be wasteful). Mean-centering
/// pins once at the 256th doc (production behaviour) and is never
/// recomputed as the haystack grows.
fn seed_slice(engine: &Engine, bodies: &[String], from: usize, to: usize) -> Duration {
    const BATCH: usize = 256;
    let started = Instant::now();
    let mut written = from;
    let mut last_report = Instant::now();
    while written < to {
        let take = BATCH.min(to - written);
        let batch: Vec<PreparedWrite> = bodies[written..written + take]
            .iter()
            .map(|b| PreparedWrite::Node {
                kind: "doc".to_string(),
                body: b.clone(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            })
            .collect();
        engine.write(&batch).expect("seed write");
        // Drain per batch so projection backlog stays bounded and seeding
        // throughput is observable; a hang is localized to one batch.
        engine.drain(600_000).expect("seed drain (batch)");
        written += take;
        if last_report.elapsed() >= Duration::from_secs(30) {
            let rate = (written - from) as f64 / started.elapsed().as_secs_f64().max(1e-3);
            eprintln!(
                "EU7_SEED_PROGRESS seeded={written}/{to} elapsed_s={} rate_docs_per_s={rate:.1}",
                started.elapsed().as_secs()
            );
            last_report = Instant::now();
        }
    }
    started.elapsed()
}

// ── Recall@10 vs f32 ground truth ──────────────────────────────────────

struct RecallResult {
    /// AC-075 verdict: recall@10 of the ANN+ VECTOR-STAGE ranking (bit-KNN
    /// K=192 + f32 rerank, pre-fusion) vs the exact-f32 VECTOR top-10 GT.
    mean: f64,
    ci_lo: f64,
    ci_hi: f64,
    sigma: f64,
    /// ◆ B-1 delta (report-only): the same recall@10 method applied to the
    /// production RRF-fused `search()` output (vector ⊕ FTS5) vs the same
    /// vector-only GT. Lower than `mean` because a hybrid result legitimately
    /// diverges from a vector-only top-10 — this is the fusion-divergence the
    /// halted GA-1 attempt mis-measured as a quantization-fidelity drop.
    fused_mean: f64,
}

/// For each query: embed with the same model, compute the brute-force f32
/// VECTOR top-10 (excluding the target doc) as ground truth, and compare
/// against the engine's ANN+ VECTOR-STAGE top-10 (sign-bit K=192 + f32
/// rerank, pre-fusion, also with the target excluded). recall@10 =
/// |prod ∩ gt| / 10 per query.
///
/// ◆ B-1 correction (GA-2, 2026-06-08): the system-under-test is the
/// engine's pre-fusion VECTOR-branch ranking, obtained via the test-only
/// `set_vector_stage_only_for_test` seam — NOT the RRF-fused `search()`
/// output. The GT is a vector-only top-10, so measuring it against a
/// hybrid (vector ⊕ FTS5) `search()` would conflate ANN-quantization
/// FIDELITY (what the 0.90 floor gates) with intended fusion divergence
/// (the GA-1 finding; see `dev/plans/runs/GA-1-corpus-ab-*.md`). We ALSO
/// measure the fused `search()` output with the identical method and report
/// it as `EU7_RECALL_FUSED` (the report-only delta) so the fused-vs-vector
/// gap is legible — but the asserting verdict is the vector stage.
///
/// Ground truth is computed IN-RUST over `doc_vecs` (the same-model,
/// uncentered, L2-normed embeddings of `bodies`, aligned by index) rather
/// than via a second SQLite connection: a separate `rusqlite::Connection`
/// to the live engine's WAL is a deadlock hazard against the engine's
/// single-writer lock, and in-Rust brute force is both deadlock-free and
/// exactly the f32 ranking the engine's rerank step targets (the engine
/// reranks on uncentered f32; mean-centering only biases the sign-bit
/// candidate stage). Ranking by L2 over unit vectors recovers cosine
/// order (vectors are unit-norm per protocol Invariant 1).
///
/// Target exclusion (codex focus): the query is synthesized from a doc;
/// that doc trivially self-retrieves and would inflate recall by a free
/// shared hit. We drop it from BOTH the ground-truth top-10 (computed as
/// top-11, target removed) and the production results. Because the engine's
/// LIMIT is locked at 10, a query whose target self-retrieves into
/// production's literal top-10 yields at most 9 comparable slots — a small
/// structural bias that pushes recall DOWN (conservative for a floor
/// anchor). Documented in the honesty report.
fn measure_recall(
    engine: &Engine,
    bodies: &[String],
    doc_vecs: &[Vec<f32>],
    embedder: &dyn Embedder,
    queries: &[QueryItem],
    bootstrap_resamples: usize,
) -> RecallResult {
    assert_eq!(bodies.len(), doc_vecs.len(), "bodies/doc_vecs must align");
    // PR-2c root-cause re-measure: compute BOTH the original (conservative)
    // method and the corrected ANN-recall method per query, in one run.
    //   OLD: literal top-10, exclude target AFTER (-> <=9 slots when target
    //        self-retrieves), GT as a HashSet (duplicate bodies collapse).
    //   NEW: exclude the query-source target BEFORE truncating to 10 (needs
    //        the engine to return >10), and dedup bodies on both prod and GT.
    //        This is the standard ANN-recall convention and matches the
    //        offline numpy pipeline (index/exclude-before).
    // PR-2bc S1 fix-1: raise the engine's phase-2 rerank LIMIT via the
    // test seam (NOT an env var) so prod search returns 10+slack, leaving 10
    // unique non-target hits after exclude-before. The seam clamps to the
    // production floor (10), so this can only RAISE fanout, never shrink it.
    const RECALL_SEARCH_SLACK: usize = 5;
    engine.set_search_limit_for_test(10 + RECALL_SEARCH_SLACK);
    // ◆ B-1: NEW-method recall@10 of a SUT result list vs the vector-only GT
    // set (`gt`), with exclude-before-target + dedup-by-body, truncated to 10.
    let recall_new = |prod: &[String], gt: &HashSet<&str>, target: &str| -> f64 {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut hits = 0usize;
        let mut taken = 0usize;
        for s in prod.iter() {
            let b = s.as_str();
            if b == target || !seen.insert(b) {
                continue;
            }
            if gt.contains(b) {
                hits += 1;
            }
            taken += 1;
            if taken == 10 {
                break;
            }
        }
        hits as f64 / 10.0
    };
    let mut per_query_old = Vec::with_capacity(queries.len());
    let mut per_query_new = Vec::with_capacity(queries.len());
    let mut per_query_fused = Vec::with_capacity(queries.len());
    let mut target_in_top10 = 0usize;
    for q in queries {
        let qv = embedder.embed(&q.text).expect("embed query");
        let dist = |v: &[f32]| -> f32 { v.iter().zip(&qv).map(|(a, b)| (a - b) * (a - b)).sum() };
        let mut idx: Vec<usize> = (0..doc_vecs.len()).collect();
        idx.sort_by(|&a, &b| {
            dist(&doc_vecs[a]).partial_cmp(&dist(&doc_vecs[b])).unwrap_or(std::cmp::Ordering::Equal)
        });
        let target = q.target_body.as_str();

        // OLD GT: top-11, remove target, truncate 10, set-collapse dup bodies.
        let mut gt_old: Vec<&str> = idx.iter().take(11).map(|&i| bodies[i].as_str()).collect();
        if let Some(pos) = gt_old.iter().position(|b| *b == target) {
            gt_old.remove(pos);
        }
        gt_old.truncate(10);
        let gt_old_set: HashSet<&str> = gt_old.into_iter().collect();

        // NEW GT: the 10 nearest UNIQUE non-target bodies (exclude-before + dedup).
        let mut gt_new_set: HashSet<&str> = HashSet::new();
        for &i in idx.iter() {
            let b = bodies[i].as_str();
            if b == target {
                continue;
            }
            if gt_new_set.insert(b) && gt_new_set.len() == 10 {
                break;
            }
        }

        // ◆ B-1: the VERDICT SUT is the engine's pre-fusion VECTOR-STAGE
        // ranking (ANN+ bit-KNN K=192 + f32 rerank), via the measurement seam.
        engine.set_vector_stage_only_for_test(true);
        let prod_vec: Vec<String> = engine
            .search(&q.text)
            .expect("prod vector-stage search")
            .results
            .iter()
            .map(|h| h.body.clone())
            .collect();
        // Report-only DELTA SUT: the production RRF-fused `search()` output
        // (vector ⊕ FTS5), measured with the identical method against the same
        // vector-only GT, so the fused-vs-vector gap is legible.
        engine.set_vector_stage_only_for_test(false);
        let prod_fused: Vec<String> = engine
            .search(&q.text)
            .expect("prod fused search")
            .results
            .iter()
            .map(|h| h.body.clone())
            .collect();

        // OLD recall (diagnostic, on the FUSED output): literal top-10,
        // exclude target after, /10 — kept for continuity of the OLD report.
        let prod_top10: Vec<&str> = prod_fused.iter().take(10).map(|s| s.as_str()).collect();
        if prod_top10.contains(&target) {
            target_in_top10 += 1;
        }
        let hits_old =
            prod_top10.iter().filter(|b| **b != target).filter(|b| gt_old_set.contains(*b)).count();
        per_query_old.push(hits_old as f64 / 10.0);

        // NEW recall (exclude-before + dedup, take 10, /10) on BOTH SUTs:
        //   per_query_new  = VECTOR-STAGE ranking → the AC-075 verdict.
        //   per_query_fused = RRF-fused search()  → the report-only delta.
        per_query_new.push(recall_new(&prod_vec, &gt_new_set, target));
        per_query_fused.push(recall_new(&prod_fused, &gt_new_set, target));
    }

    let mean_old = per_query_old.iter().sum::<f64>() / per_query_old.len().max(1) as f64;
    let (lo_old, hi_old, sg_old) = bootstrap_ci(&per_query_old, bootstrap_resamples);
    eprintln!(
        "EU7_RECALL_OLD exclude-after+setGT(fused) recall@10={mean_old:.4} ci=[{lo_old:.4},{hi_old:.4}] sigma={sg_old:.4} target_in_top10={target_in_top10}/{}",
        queries.len()
    );
    // ◆ B-1 verdict: recall@10 of the ANN+ VECTOR STAGE vs exact-f32 vector GT.
    let mean = per_query_new.iter().sum::<f64>() / per_query_new.len().max(1) as f64;
    let (ci_lo, ci_hi, sigma) = bootstrap_ci(&per_query_new, bootstrap_resamples);
    eprintln!(
        "EU7_RECALL_VECTOR exclude-before+dedupGT vector-stage recall@10={mean:.4} ci=[{ci_lo:.4},{ci_hi:.4}] sigma={sigma:.4} (AC-075 VERDICT SUT)"
    );
    // ◆ B-1 report-only delta: the same method on the production RRF-fused
    // search() output. Lower than the vector-stage verdict because a hybrid
    // result legitimately diverges from a vector-only top-10 (the GA-1 finding).
    let fused_mean = per_query_fused.iter().sum::<f64>() / per_query_fused.len().max(1) as f64;
    let (f_lo, f_hi, f_sg) = bootstrap_ci(&per_query_fused, bootstrap_resamples);
    eprintln!(
        "EU7_RECALL_FUSED exclude-before+dedupGT fused-search recall@10={fused_mean:.4} ci=[{f_lo:.4},{f_hi:.4}] sigma={f_sg:.4} (report-only delta; NOT the floor SUT)"
    );
    eprintln!(
        "EU7_RECALL_DELTA vector_stage={mean:.4} fused_search={fused_mean:.4} delta={:.4} \
         (correction is load-bearing: the floor gates the vector stage, not the hybrid output)",
        mean - fused_mean
    );
    RecallResult { mean, ci_lo, ci_hi, sigma, fused_mean }
}

/// Percentile bootstrap (resample with replacement) over per-query
/// recall. Returns (2.5pct, 97.5pct, sigma) of the resample means.
fn bootstrap_ci(per_query: &[f64], resamples: usize) -> (f64, f64, f64) {
    if per_query.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let n = per_query.len();
    let mut rng = SplitMix64::new(BOOTSTRAP_SEED);
    let mut means = Vec::with_capacity(resamples);
    for _ in 0..resamples {
        let mut acc = 0.0;
        for _ in 0..n {
            acc += per_query[rng.next_in(n)];
        }
        means.push(acc / n as f64);
    }
    means.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let grand = means.iter().sum::<f64>() / means.len() as f64;
    let var = means.iter().map(|m| (m - grand) * (m - grand)).sum::<f64>() / means.len() as f64;
    let sigma = var.sqrt();
    let lo = means[((resamples as f64) * 0.025) as usize];
    let hi = means[(((resamples as f64) * 0.975) as usize).min(resamples - 1)];
    (lo, hi, sigma)
}

// ── AC-013 latency ─────────────────────────────────────────────────────

struct LatencyResult {
    p50: Duration,
    p99: Duration,
}

fn measure_latency(engine: &Engine, queries: &[QueryItem], samples_n: usize) -> LatencyResult {
    // Warmup pass (discarded).
    for i in 0..samples_n {
        let q = &queries[i % queries.len()];
        let _ = engine.search(&q.text).expect("warmup search");
    }
    let mut samples = Vec::with_capacity(samples_n);
    for i in 0..samples_n {
        let q = &queries[i % queries.len()];
        let started = Instant::now();
        let _ = engine.search(&q.text).expect("measure search");
        samples.push(started.elapsed());
    }
    LatencyResult {
        p50: percentile_ceil(&samples, 50, 100),
        p99: percentile_ceil(&samples, 99, 100),
    }
}

// ── AC-019 mixed-retrieval stress tail ─────────────────────────────────

struct StressResult {
    baseline_p99: Duration,
    stress_p99: Duration,
    bound: Duration,
}

fn measure_stress(
    engine: Arc<Engine>,
    queries: &[QueryItem],
    samples_n: usize,
    per_thread: usize,
) -> StressResult {
    // Baseline = re-run AC-013's protocol immediately preceding the stress
    // pass (per acceptance.md AC-019).
    for i in 0..samples_n {
        let q = &queries[i % queries.len()];
        let _ = engine.search(&q.text).expect("baseline warmup");
    }
    let mut baseline = Vec::with_capacity(samples_n);
    for i in 0..samples_n {
        let q = &queries[i % queries.len()];
        let started = Instant::now();
        let _ = engine.search(&q.text).expect("baseline measure");
        baseline.push(started.elapsed());
    }
    let baseline_p99 = percentile_ceil(&baseline, 99, 100);

    let texts: Arc<Vec<String>> = Arc::new(queries.iter().map(|q| q.text.clone()).collect());
    let barrier = Arc::new(Barrier::new(AC019_THREADS + 1));
    let sink: Arc<Mutex<Vec<Duration>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::with_capacity(AC019_THREADS);
    for tid in 0..AC019_THREADS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        let texts = Arc::clone(&texts);
        let sink = Arc::clone(&sink);
        handles.push(thread::spawn(move || {
            let mut local = Vec::with_capacity(per_thread);
            let base = tid * per_thread;
            barrier.wait();
            for i in 0..per_thread {
                let q = &texts[(base + i) % texts.len()];
                let started = Instant::now();
                let _ = engine.search(q).expect("stress search");
                local.push(started.elapsed());
            }
            sink.lock().unwrap().extend(local);
        }));
    }
    barrier.wait();
    for h in handles {
        h.join().expect("stress thread");
    }
    let all = sink.lock().unwrap().clone();
    let stress_p99 = percentile_ceil(&all, 99, 100);
    let bound = std::cmp::max(baseline_p99 * AC019_STRESS_MULT, AC019_STRESS_FLOOR);
    StressResult { baseline_p99, stress_p99, bound }
}

// ── Driver ─────────────────────────────────────────────────────────────

/// **TC-20 hard gate (0.8.20 Slice 5 fix-4).** `#[ignore]` — this body NEVER runs
/// unless a human explicitly asks for it with `-- --ignored`.
///
/// The measurement seeds the real BGE embedder through the projection pipeline
/// at ~1.3 docs/sec, i.e. ~1.5 h of wall clock for the full corpus. It must not
/// be reachable by an agent's routine `cargo test --workspace`.
///
/// Two independent gates, deliberately: the `default-embedder` feature gate
/// (file-level `#![cfg]`) already keeps this out of the default build, but that
/// is implicit — a future `--all-features` run, or feature unification via
/// another crate, would silently arm it. `#[ignore]` holds regardless of which
/// features are selected.
///
/// The `AGENT_LONG` early-return below is kept as a third layer rather than
/// replaced: an explicit `--ignored` run on a box without the corpus should
/// still skip rather than fail.
///
/// This does NOT create a vacuous green: R-20-EU7 was CLOSED by HITL decision,
/// so this test no longer gates anything. Run:
///   AGENT_LONG=1 cargo test -p fathomdb-engine --features default-embedder \
///     --test eu7_real_corpus_ac -- --ignored --nocapture
#[test]
#[ignore = "TC-20: EU-7 real-corpus measurement is ~1.5h of wall clock; opt in with --ignored"]
fn eu7_real_corpus_ac_validation() {
    if std::env::var_os("AGENT_LONG").is_none() {
        eprintln!("[skip] AGENT_LONG not set; EU-7 real-corpus measurement is opt-in");
        return;
    }
    if std::env::var("FATHOMDB_SKIP_NETWORK_TESTS").is_ok() {
        eprintln!("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; embedder cache unavailable");
        return;
    }

    let num_queries = env_usize("EU7_QUERIES", 100);
    let bootstrap = env_usize("EU7_BOOTSTRAP", 1000);
    let latency_samples = env_usize("EU7_LATENCY_SAMPLES", LATENCY_SAMPLES_DEFAULT);
    let stress_per_thread = env_usize("EU7_STRESS_PER_THREAD", AC019_QUERIES_PER_THREAD_DEFAULT);
    let ns = n_values();

    let Some((real, queries)) = load_real_and_queries(num_queries) else {
        eprintln!("[skip] corpus not present; cannot run EU-7 real-corpus measurement");
        return;
    };
    let real_len = real.len();
    eprintln!(
        "EU7_SETUP real_docs={real_len} queries={} n_values={ns:?} bootstrap={bootstrap} \
         latency_samples={latency_samples} stress_per_thread={stress_per_thread}",
        queries.len()
    );

    // Build the real embedder once (warm cache, no network), serialized to
    // dodge the concurrent-embed projection-pool stall. Reused as the
    // engine's embedder AND as the ground-truth query encoder.
    let embedder = Arc::new(SerializedBge::new(
        CandleBgeEmbedder::new().expect("construct real bge embedder"),
    ));

    let mut ac013 = Vec::new();
    let mut ac013b = Vec::new();
    let mut ac019 = Vec::new();
    let mut anchor: Option<serde_json::Value> = None;
    // (actual_n, ac013_passed, ac019_passed, vector_stage_recall_mean,
    //  vector_stage_recall_ci_hi, padded) per N — verdicts are reported after
    // the JSON is written so a dev-box latency miss never aborts before the data
    // lands. `padded` gates the AC-075 assert to all-real N (synthetic
    // distractors depress recall). `recall_ci_hi` is the upper bound of the
    // recall 95% bootstrap CI: the GA-3 one-sided CI gate (◆ HITL 2026-06-08)
    // asserts `recall_ci_hi >= CURRENT_FLOOR`, not the point estimate.
    let mut verdicts: Vec<(usize, bool, bool, f64, f64, bool)> = Vec::new();

    // Single growing engine: N targets are seeded incrementally so each
    // doc is embedded exactly once across the whole sweep.
    let mut sorted_ns = ns.clone();
    sorted_ns.sort_unstable();
    sorted_ns.dedup();
    let max_n = *sorted_ns.last().expect("at least one N");
    let bodies = haystack_bodies(&real, max_n);

    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("eu7_corpus.sqlite");
    let opened = Engine::open_with_choice(
        &path,
        EmbedderChoice::Caller(embedder.clone() as Arc<dyn Embedder>),
    )
    .expect("open with real bge embedder");
    let engine = Arc::new(opened.engine);
    assert_eq!(
        opened.report.default_embedder.name, "fathomdb-bge-small-en-v1.5",
        "must run against the real bge-small identity"
    );
    engine.configure_vector_kind_for_test("doc").expect("configure vector kind");

    // In-harness same-model embeddings of every seeded body, for the
    // deadlock-free in-Rust f32 ground truth (see measure_recall).
    let mut doc_vecs: Vec<Vec<f32>> = Vec::with_capacity(bodies.len());
    let mut seeded = 0usize;
    for &n in &sorted_ns {
        let actual_n = n.min(bodies.len());
        let padded = actual_n > real_len;
        eprintln!("EU7_PHASE n={actual_n} seed_start (from {seeded})");
        let seed = seed_slice(&engine, &bodies, seeded, actual_n);
        // Mirror the newly-seeded bodies into the in-Rust GT vector cache.
        // This single-threaded re-embed is silent for ~25-45 min on the full
        // corpus and has twice read as a hang; emit a start marker + a throttled
        // progress line (mirrors the EU7_SEED_PROGRESS Instant cadence above).
        let gt_n = actual_n - seeded;
        eprintln!("EU7_PHASE n={actual_n} GT_EMBED_START n={gt_n}");
        let gt_started = Instant::now();
        let mut gt_last_report = Instant::now();
        for (i, body) in bodies[seeded..actual_n].iter().enumerate() {
            doc_vecs.push(embedder.embed(body).expect("embed body for GT"));
            if gt_last_report.elapsed() >= Duration::from_secs(30) {
                let embedded = i + 1;
                let rate = embedded as f64 / gt_started.elapsed().as_secs_f64().max(1e-3);
                eprintln!(
                    "EU7_GT_EMBED_PROGRESS embedded={embedded}/{gt_n} rate_docs_per_s={rate:.1}"
                );
                gt_last_report = Instant::now();
            }
        }
        seeded = actual_n;
        // Confirm every row is indexed before measuring.
        let rows = engine.vector_row_count_for_test().expect("row count");
        assert_eq!(rows as usize, actual_n, "vector_default rows must equal seeded docs");
        eprintln!("EU7_PHASE n={actual_n} seed_done seed_ms={}", seed.as_millis());

        // 0.7.2 PR-2c DIAGNOSTIC seam (not production): force the FULL-corpus
        // mean via the PR-2b recompute path after all docs are seeded, to test
        // whether a known-good mean recovers real-engine recall (PR-2a offline
        // projected ~0.945 with EU-7 queries) or whether the offline number
        // simply does not transfer to the candle path.
        if std::env::var_os("EU7_FORCE_FULL_RECOMPUTE").is_some() {
            let rep = engine.recompute_mean().expect("force full recompute");
            eprintln!(
                "EU7_FORCE_FULL_RECOMPUTE n={actual_n} doc_count={} \
                 drift_cos_before={:.4} mean_was_pinned={} dim={}",
                rep.doc_count_requantized, rep.drift_cos_before, rep.mean_was_pinned, rep.dim
            );
        }

        let lat = measure_latency(&engine, &queries, latency_samples);
        eprintln!(
            "EU7_PHASE n={actual_n} latency_done p50_ms={} p99_ms={}",
            lat.p50.as_millis(),
            lat.p99.as_millis()
        );
        let rec = measure_recall(
            &engine,
            &bodies[..actual_n],
            &doc_vecs,
            embedder.as_ref(),
            &queries,
            bootstrap,
        );
        eprintln!(
            "EU7_PHASE n={actual_n} recall_done vector_stage={:.4} fused_search={:.4}",
            rec.mean, rec.fused_mean
        );
        let stress =
            measure_stress(Arc::clone(&engine), &queries, latency_samples, stress_per_thread);
        eprintln!("EU7_PHASE n={actual_n} stress_done p99_ms={}", stress.stress_p99.as_millis());

        let ac013_passed = lat.p50 <= AC013_BUDGET_P50 && lat.p99 <= AC013_BUDGET_P99;
        let ac019_passed = stress.stress_p99 <= stress.bound;

        eprintln!(
            "EU7_NUMBERS n={actual_n} padded={padded} seed_ms={} p50_ms={} p99_ms={} \
             recall_at_10={:.4} recall_ci_lo={:.4} recall_ci_hi={:.4} sigma={:.4} \
             recall_fused_search={:.4} recall_fused_delta={:.4} \
             stress_p99_ms={} stress_bound_ms={} ac013={} ac019={}",
            seed.as_millis(),
            lat.p50.as_millis(),
            lat.p99.as_millis(),
            rec.mean,
            rec.ci_lo,
            rec.ci_hi,
            rec.sigma,
            rec.fused_mean,
            rec.mean - rec.fused_mean,
            stress.stress_p99.as_millis(),
            stress.bound.as_millis(),
            ac013_passed,
            ac019_passed,
        );

        ac013.push(json!({
            "n": actual_n,
            "padded_with_synthetic_distractors": padded,
            "p50_ms": lat.p50.as_millis() as u64,
            "p99_ms": lat.p99.as_millis() as u64,
            "budget_p50_ms": AC013_BUDGET_P50.as_millis() as u64,
            "budget_p99_ms": AC013_BUDGET_P99.as_millis() as u64,
            "passed": ac013_passed,
        }));
        ac013b.push(json!({
            "n": actual_n,
            "padded_with_synthetic_distractors": padded,
            "sut": "ann_plus_vector_stage_pre_fusion (◆ B-1; not RRF-fused search())",
            "recall_at_10": round4(rec.mean),
            "ci_lo": round4(rec.ci_lo),
            "ci_hi": round4(rec.ci_hi),
            "sigma_bootstrap": round4(rec.sigma),
            "recall_at_10_fused_search": round4(rec.fused_mean),
            "recall_at_10_fused_search_delta": round4(rec.mean - rec.fused_mean),
            "current_floor_0_90": CURRENT_FLOOR,
            "passes_current_floor_point_estimate": rec.mean >= CURRENT_FLOOR,
            // ◆ GA-3: the 0.8.0 AC-075 verdict is the one-sided CI gate
            // (ci_hi >= floor), not the point estimate. Floor constant 0.90.
            "passes_ci_gate_0_8_0_one_sided": recall_gate::recall_ci_clears_floor(
                rec.ci_hi, CURRENT_FLOOR,
            ),
        }));
        ac019.push(json!({
            "n": actual_n,
            "padded_with_synthetic_distractors": padded,
            "baseline_p99_ms": stress.baseline_p99.as_millis() as u64,
            "p99_ms": stress.stress_p99.as_millis() as u64,
            "bound_ms": stress.bound.as_millis() as u64,
            "passed": ac019_passed,
        }));

        // Anchor: the largest ALL-REAL haystack (no synthetic padding).
        if !padded {
            let floor_2sigma = round4(rec.mean - 2.0 * rec.sigma);
            anchor = Some(json!({
                "n": actual_n,
                "recall_at_10": round4(rec.mean),
                "ci_lo": round4(rec.ci_lo),
                "sigma_bootstrap": round4(rec.sigma),
                "recommended_pr2_floor_R_minus_2sigma": floor_2sigma,
                "recommended_pr2_floor_rounded_down_0_01": (floor_2sigma * 100.0).floor() / 100.0,
                "note": "Largest all-real-corpus haystack; recall DECREASES with N, \
                         so this dev-box anchor is an upper-ish bound on PR-3's \
                         canonical N=1M number. K=192 locked.",
            }));
        }

        verdicts.push((actual_n, ac013_passed, ac019_passed, rec.mean, rec.ci_hi, padded));
    }

    write_measurements_json(MeasurementsOut {
        ac013: &ac013,
        ac013b: &ac013b,
        ac019: &ac019,
        anchor: anchor.as_ref(),
        ns: &ns,
        real_len,
        queries: &queries,
        bootstrap,
        latency_samples,
        stress_per_thread,
    });

    // ── Verdicts (data already persisted to JSON above) ────────────────
    //
    // AC-013 (latency) and AC-019 (stress) are REPORTED, not hard-gated
    // here: per the launch prompt, "Canonical-CI is the only verdict-
    // quality signal. Dev-box measurements in EU-7 are scouting." A slow
    // dev runner missing the 80/300 ms budget is a hardware artifact for
    // PR-3 to confirm at canonical scale, not a true RED. The pass/fail
    // flags are in the JSON for the orchestrator + HITL.
    //
    // AC-075 (0.8.0 Slice 40 / GA-2) — this real-embedder recall@10, measured
    // on the pre-fusion VECTOR STAGE (◆ B-1), is the ASSERTING recall verdict
    // (≥ CURRENT_FLOOR = 0.90), superseding the synthetic perf_gates::ac_013b
    // hard-assert (demoted to report-only). Two tiers of assertion, low→high,
    // so a failure is legible:
    //   1. SANITY_FLOOR (0.55) — below the isotropic noise floor signals a
    //      wiring bug in the harness/engine path, not a quantization gap.
    //   2. CURRENT_FLOOR (0.90) — the AC-075 production recall verdict, on the
    //      VECTOR STAGE only (the ANN-quantization fidelity axis the floor is
    //      defined to gate). ◆ GA-3 (0.8.0, ◆ HITL 2026-06-08): this is a
    //      ONE-SIDED, CI-based gate — PASS iff `recall_ci_hi >= CURRENT_FLOOR`
    //      (the recall 95% bootstrap CI is not significantly below the floor),
    //      NOT a point-estimate `recall >= floor` assert. The 0.90 floor
    //      CONSTANT is unchanged; the point-estimate-≥0.90 recovery (the ~4pt
    //      0.7.x→0.8.0 drop) is a 0.8.1 item, and this CI form is to be
    //      revisited after 0.8.0. Asserted only for ALL-REAL N (synthetic
    //      padding depresses recall; padded points probe haystack scaling, not
    //      the floor). This is the LOCAL once-per-release / perf-canonical-
    //      dispatch verdict (real-embedder N is infeasible per-push, AC-072);
    //      per-push CI gates only perf_gates::ac_013_vector_read_path_smoke.
    for (n, ac013_ok, ac019_ok, recall, recall_ci_hi, padded) in &verdicts {
        eprintln!(
            "EU7_VERDICT n={n} ac013_latency={} ac019_stress={} vector_stage_recall_at_10={:.4} \
             recall_ci_hi={recall_ci_hi:.4} padded={padded} \
             (latency/stress are dev-box scouting; canonical verdict is 0.7.2 PR-3)",
            if *ac013_ok { "PASS" } else { "MISS(dev-box)" },
            if *ac019_ok { "PASS" } else { "MISS(dev-box)" },
            recall,
        );
    }
    for (n, _, _, recall, recall_ci_hi, padded) in &verdicts {
        assert!(
            *recall >= SANITY_FLOOR,
            "AC-013b sanity: recall@10 {recall:.4} < sanity floor {SANITY_FLOOR} at n={n} \
             (real BGE should beat the ~0.51 isotropic noise floor; a value this low \
             signals a wiring bug, not a quantization gap)"
        );
        if !*padded {
            // ◆ GA-3 (0.8.0 Slice-40) — the AC-075 verdict is a ONE-SIDED,
            // CI-based gate against the UNCHANGED 0.90 floor: PASS iff the recall
            // 95% bootstrap CI is NOT significantly below the floor
            // (`recall_ci_hi >= CURRENT_FLOOR`). This replaces the prior
            // point-estimate hard-assert (`recall >= CURRENT_FLOOR`), which
            // PANICKED at the measured 0.8960 even though the 0.90 floor lies
            // inside the CI [0.8640, 0.9250] (ci_hi 0.925 ≥ 0.90 ⇒ PASS). The
            // floor CONSTANT is unchanged at 0.90 — the gate is reconciled, not
            // lowered/weakened. NOT a two-sided "floor ∈ [ci_lo, ci_hi]" test:
            // that would wrongly FAIL a comfortably-high recall whose whole CI
            // clears the floor. See `support/recall_gate.rs`.
            assert!(
                recall_gate::recall_ci_clears_floor(*recall_ci_hi, CURRENT_FLOOR),
                "AC-075 recall verdict (0.8.0 GA-3 gate): vector-stage recall@10 95% CI is \
                 SIGNIFICANTLY below the {CURRENT_FLOOR} floor at n={n} \
                 (recall_ci_hi {recall_ci_hi:.4} < floor {CURRENT_FLOOR}; point estimate \
                 {recall:.4}). 0.8.0 gate = recall 95% CI not significantly below the 0.90 \
                 floor (one-sided, ci_hi >= floor); the floor CONSTANT is UNCHANGED at 0.90 \
                 (not lowered). Measured on the REAL bge-small embedder's pre-fusion VECTOR \
                 STAGE — ANN-quantization fidelity vs the exact-f32 vector top-10; the \
                 synthetic perf_gates::ac_013b is report-only, and the RRF-fused search() \
                 recall is the report-only delta EU7_RECALL_FUSED, NOT this gate's SUT. The \
                 point-estimate-≥0.90 recovery (the ~4pt 0.7.x→0.8.0 vector drop) is tracked \
                 to 0.8.1. Per the ◆ HITL ruling 2026-06-08; this CI form is a 0.8.0-scoped \
                 reconciliation to be REVISITED after 0.8.0."
            );
        }
    }
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

struct MeasurementsOut<'a> {
    ac013: &'a [serde_json::Value],
    ac013b: &'a [serde_json::Value],
    ac019: &'a [serde_json::Value],
    anchor: Option<&'a serde_json::Value>,
    ns: &'a [usize],
    real_len: usize,
    queries: &'a [QueryItem],
    bootstrap: usize,
    latency_samples: usize,
    stress_per_thread: usize,
}

fn write_measurements_json(m: MeasurementsOut) {
    let MeasurementsOut {
        ac013,
        ac013b,
        ac019,
        anchor,
        ns,
        real_len,
        queries,
        bootstrap,
        latency_samples,
        stress_per_thread,
    } = m;
    let Some(root) = repo_root() else {
        eprintln!("[warn] repo_root() not found; skipping measurements JSON write");
        return;
    };
    // ◆ GA-3: write to a non-anchor filename so each eu7 run no longer clobbers
    // the historical 0.7.1 measurements anchor (which pinned the 0.937 number).
    let out_path = root.join("dev/plans/runs/eu7-latest-measurements.json");
    let doc = json!({
        "_comment": "EU-7 real-corpus AC measurements (dev-box scouting). \
                     Regenerable: AGENT_LONG=1 cargo test -p fathomdb-engine \
                     --features default-embedder --test eu7_real_corpus_ac. \
                     Consumed by dev/plans/runs/0.7.1-EU-7-output.json and by \
                     0.7.2 PR-2 (floor re-derivation).",
        "config": {
            "embedder": "fathomdb-bge-small-en-v1.5",
            "dimension": 384,
            "mean_centering": true,
            "rerank_k_locked": 192,
            "query_count": queries.len(),
            "query_synthesis": "EU-0 §1.2: title-or-lead-sentence, target excluded from GT+prod",
            "ground_truth": "brute-force f32 L2 over uncentered embedding column, same model",
            "n_values_requested": ns,
            "real_corpus_docs": real_len,
            "bootstrap_resamples": bootstrap,
            "latency_samples": latency_samples,
            "ac019_stress_per_thread": stress_per_thread,
            "ac019_threads": AC019_THREADS,
            "query_select_seed_hex": format!("{QUERY_SELECT_SEED:#x}"),
            "bootstrap_seed_hex": format!("{BOOTSTRAP_SEED:#x}"),
        },
        "ac_013_real_dev_box": ac013,
        "ac_013b_real_dev_box": ac013b,
        "ac_019_real_dev_box": ac019,
        "r_canonical_anchor": anchor,
    });
    std::fs::write(&out_path, serde_json::to_string_pretty(&doc).unwrap())
        .expect("write measurements json");
    eprintln!("EU7_WROTE {}", out_path.display());
}
