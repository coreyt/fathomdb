//! EU-8 -- IR (relevance-judged) recall harness for 0.7.2.
//!
//! Adds an **IR recall** signal that runs ALONGSIDE (and orthogonal to)
//! the ANN recall measured by `eu7_real_corpus_ac.rs`. Where ANN recall
//! scores the quantized retrieval against the embedder's OWN exact-f32
//! nearest neighbours (a self-referential fidelity signal subject to the
//! quirks PR-2c found), IR recall scores `engine.search()` against the
//! EXTERNALLY-LABELLED relevant doc_ids carried in each chain's
//! `ground_truth_queries` field. It is therefore immune to the ANN
//! harness's target-self-retrieval exclusion and duplicate-body GT
//! quirks (see `dev/plans/runs/0.7.2-PR-2c-recall-rootcause.md`).
//!
//! ## Orthogonality (the hard constraint)
//!
//! This harness is fully independent of `eu7_real_corpus_ac.rs`: separate
//! test fn, separate `TempDir`/`Engine`, separate JSON output file, ZERO
//! imports from the EU-7 file, no shared mutable global/static state.
//! `Engine::search(&self, ...)` is read-only on the embedder / quant /
//! mean-centering pipeline (it only READS the already-pinned mean; see
//! `lib.rs::search_inner`), so running this changes no ANN behaviour. The
//! only support change is the ADDITIVE `extract_ground_truth_queries` /
//! `IRQuery` in `support/corpus_subset.rs`.
//!
//! ## Real-embedder requirement (design correction)
//!
//! `engine.search()` only returns MEANINGFUL results if the corpus was
//! embedded by the REAL candle embedder during ingest (the synthetic
//! `VaryingEmbedder` produces hash-placement vectors with no semantic
//! relevance to the natural-language chain queries). So EU-8 DOES require
//! `--features default-embedder` and a real seed+embed, exactly like
//! EU-7. "Orthogonal / no embedder touching" means EU-8 does NOT MODIFY
//! the embedder, quantization, mean-centering, or the ANN harness -- NOT
//! that it avoids embedding. The design note's "does NOT require
//! default-embedder" claim was wrong and has been corrected.
//!
//! ## Gating + run
//!
//! Requires the `default-embedder` Cargo feature AND `AGENT_LONG=1`
//! (graceful skip otherwise, and graceful skip if chains/corpus absent).
//!
//!   AGENT_LONG=1 cargo test --release -p fathomdb-engine \
//!     --features default-embedder --test eu8_ir_validation -- --nocapture
//!
//! Tunables (env):
//!   EU8_MAX_CHAINS   number of chain JSONs to load (default 200 = all).
//!                    For a SMALL-CORPUS SMOKE, set this low (e.g. 10).
//!   EU8_SMOKE        if set, seed ONLY the chain docs (a few hundred docs)
//!                    instead of the full 7,667-doc corpus -- fast, low CPU.
//!   EU8_BOOTSTRAP    bootstrap resamples (default 1000).

#![cfg(feature = "default-embedder")]

#[path = "support/corpus_subset.rs"]
mod corpus_subset;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use corpus_subset::{
    extract_ground_truth_queries, load_chain_docs, load_chains_or_skip, repo_root, Doc, IRQuery,
    CORPUS_DIM, VECTOR_KIND,
};
use fathomdb_embedder::CandleBgeEmbedder;
use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{EmbedderChoice, Engine, PreparedWrite};
use serde_json::json;
use tempfile::TempDir;

const K: usize = 10;
const BOOTSTRAP_SEED: u64 = 0x0E88_B007_574A_9001; // EU-8 bootstrap (distinct from EU-7)
                                                   // Loose sanity floor: with the real BGE embedder over the chain docs, the
                                                   // labelled-relevant docs should land in the top-10 well above chance. This
                                                   // is scouting, NOT a production floor.
const SANITY_FLOOR: f64 = 0.25;

// ── Serialized real BGE embedder (mirrors EU-7's SerializedBge) ─────────
// The engine's projection pool calls embed() concurrently on one shared
// instance; candle guidance is to guard a shared model for concurrent CPU
// inference. Measurement-fidelity wrapper only: vectors/identity/mean
// behaviour are byte-identical, only concurrency is constrained.
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

// ── Deterministic RNG (SplitMix64) for the bootstrap ────────────────────
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

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

// ── Per-query IR result ─────────────────────────────────────────────────
#[derive(Clone, Debug)]
struct IRQueryResult {
    hits: usize,
    recall_at_k: f64,
    precision_at_k: f64,
    reciprocal_rank: f64,
    ndcg_at_k: f64,
}

// ── Aggregate IR result ─────────────────────────────────────────────────
struct IRRecallResult {
    mean_recall_at_k: f64,
    ci_lo: f64,
    ci_hi: f64,
    sigma: f64,
    mean_precision_at_k: f64,
    mean_mrr: f64,
    mean_ndcg: f64,
    query_count: usize,
    queries_with_zero_hits: usize,
    unmapped_retrieval_results: usize,
    per_relation_type: BTreeMap<String, AggBucket>,
    per_chain_shape: BTreeMap<String, AggBucket>,
}

#[derive(Default, Clone)]
struct AggBucket {
    count: usize,
    recall_sum: f64,
    precision_sum: f64,
    mrr_sum: f64,
    ndcg_sum: f64,
}
impl AggBucket {
    fn add(&mut self, r: &IRQueryResult) {
        self.count += 1;
        self.recall_sum += r.recall_at_k;
        self.precision_sum += r.precision_at_k;
        self.mrr_sum += r.reciprocal_rank;
        self.ndcg_sum += r.ndcg_at_k;
    }
    fn json(&self) -> serde_json::Value {
        let n = self.count.max(1) as f64;
        json!({
            "query_count": self.count,
            "recall_at_10": round4(self.recall_sum / n),
            "precision_at_10": round4(self.precision_sum / n),
            "mrr": round4(self.mrr_sum / n),
            "ndcg_at_10": round4(self.ndcg_sum / n),
        })
    }
}

// ── body -> doc_id mapping (engine.search returns bodies) ───────────────
//
// First-occurrence rule for duplicate bodies: when two docs share an
// identical body, the FIRST (by the order of `docs`, which `ingest` writes
// in) wins the mapping; later collisions are counted as `duplicate_bodies`
// for transparent diagnostics. Conservative: a retrieved body that maps to
// the first-occurrence doc_id may "miss" a later duplicate's expected id,
// pushing recall DOWN, never up.
fn build_body_to_doc_id_map(docs: &[Doc]) -> (HashMap<String, String>, usize) {
    let mut map: HashMap<String, String> = HashMap::with_capacity(docs.len());
    let mut duplicate_bodies = 0usize;
    for d in docs {
        if map.contains_key(&d.body) {
            duplicate_bodies += 1;
        } else {
            map.insert(d.body.clone(), d.doc_id.clone());
        }
    }
    (map, duplicate_bodies)
}

/// Map a list of retrieved bodies back to doc_ids. Unmapped bodies (no
/// matching ingested doc) are dropped from the id list and counted so the
/// caller can report `unmapped_retrieval_results`. Order is preserved
/// (rank matters for MRR/NDCG).
fn map_bodies_to_doc_ids(bodies: &[String], map: &HashMap<String, String>) -> (Vec<String>, usize) {
    let mut ids = Vec::with_capacity(bodies.len());
    let mut unmapped = 0usize;
    for b in bodies {
        match map.get(b) {
            Some(id) => ids.push(id.clone()),
            None => unmapped += 1,
        }
    }
    (ids, unmapped)
}

// ── Per-query metrics (binary relevance) ────────────────────────────────
//
// NO TARGET EXCLUSION (unlike EU-7 ANN): chain queries reference multiple
// specific relevant docs, not a single self-source, so every expected id
// is a valid relevant target.
//   recall@k    = hits / |expected|        (capped: hits<=|expected|)
//   precision@k = hits / min(k, |retrieved|)
//   MRR         = 1 / rank_of_first_relevant   (0 if none in top-k)
//   NDCG@k      = DCG / iDCG, binary relevance
fn compute_ir_metrics(
    expected: &HashSet<String>,
    retrieved_ids: &[String],
    k: usize,
) -> IRQueryResult {
    let top: Vec<String> = retrieved_ids.iter().take(k).cloned().collect();
    let hits = top.iter().filter(|id| expected.contains(*id)).count();

    let recall_at_k = if expected.is_empty() {
        0.0
    } else {
        // Cap at the number of relevant docs retrievable in k slots.
        let denom = expected.len();
        (hits as f64 / denom as f64).min(1.0)
    };
    let precision_at_k = if top.is_empty() { 0.0 } else { hits as f64 / top.len().min(k) as f64 };

    let reciprocal_rank = top
        .iter()
        .position(|id| expected.contains(id))
        .map(|pos| 1.0 / (pos as f64 + 1.0))
        .unwrap_or(0.0);

    let ndcg_at_k = compute_ndcg(expected, &top, k);

    IRQueryResult { hits, recall_at_k, precision_at_k, reciprocal_rank, ndcg_at_k }
}

/// NDCG@k with binary relevance: gain = 1 if retrieved id is relevant.
/// DCG = sum_i rel_i / log2(i + 2); iDCG is the DCG of the ideal ranking
/// (all relevant docs first), capped at min(k, |expected|) ones.
fn compute_ndcg(expected: &HashSet<String>, top: &[String], k: usize) -> f64 {
    let mut dcg = 0.0;
    for (i, id) in top.iter().take(k).enumerate() {
        if expected.contains(id) {
            dcg += 1.0 / ((i as f64) + 2.0).log2();
        }
    }
    let ideal_hits = expected.len().min(k);
    if ideal_hits == 0 {
        return 0.0;
    }
    let mut idcg = 0.0;
    for i in 0..ideal_hits {
        idcg += 1.0 / ((i as f64) + 2.0).log2();
    }
    if idcg == 0.0 {
        0.0
    } else {
        dcg / idcg
    }
}

/// Percentile bootstrap over per-query recall (reuses EU-7's method:
/// resample-with-replacement, 1000 resamples, 2.5/97.5 percentiles, sigma
/// of the resample means). Implemented locally (no import from EU-7).
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

/// Run every IR query through `engine.search()`, map bodies->doc_ids,
/// compute per-query metrics, and aggregate (overall + per-relation-type
/// + per-chain-shape + diagnostics).
fn measure_ir_recall(
    engine: &Engine,
    body_to_doc_id: &HashMap<String, String>,
    ir_queries: &[IRQuery],
    resamples: usize,
) -> IRRecallResult {
    let mut per_query_recall = Vec::with_capacity(ir_queries.len());
    let mut precision_sum = 0.0;
    let mut mrr_sum = 0.0;
    let mut ndcg_sum = 0.0;
    let mut zero_hits = 0usize;
    let mut total_unmapped = 0usize;
    let mut per_relation: BTreeMap<String, AggBucket> = BTreeMap::new();
    let mut per_shape: BTreeMap<String, AggBucket> = BTreeMap::new();

    for q in ir_queries {
        let bodies: Vec<String> = engine
            .search(&q.text)
            .expect("ir search")
            .results
            .iter()
            .map(|h| h.body.clone())
            .collect();
        let (ids, unmapped) = map_bodies_to_doc_ids(&bodies, body_to_doc_id);
        total_unmapped += unmapped;
        let r = compute_ir_metrics(&q.expected_doc_ids, &ids, K);
        if r.hits == 0 {
            zero_hits += 1;
        }
        per_query_recall.push(r.recall_at_k);
        precision_sum += r.precision_at_k;
        mrr_sum += r.reciprocal_rank;
        ndcg_sum += r.ndcg_at_k;
        per_relation.entry(q.relation_type.clone()).or_default().add(&r);
        per_shape.entry(q.chain_shape.clone()).or_default().add(&r);
    }

    let n = per_query_recall.len().max(1) as f64;
    let mean_recall_at_k = per_query_recall.iter().sum::<f64>() / n;
    let (ci_lo, ci_hi, sigma) = bootstrap_ci(&per_query_recall, resamples);

    IRRecallResult {
        mean_recall_at_k,
        ci_lo,
        ci_hi,
        sigma,
        mean_precision_at_k: precision_sum / n,
        mean_mrr: mrr_sum / n,
        mean_ndcg: ndcg_sum / n,
        query_count: ir_queries.len(),
        queries_with_zero_hits: zero_hits,
        unmapped_retrieval_results: total_unmapped,
        per_relation_type: per_relation,
        per_chain_shape: per_shape,
    }
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

#[allow(clippy::too_many_arguments)]
fn write_ir_measurements_json(
    result: &IRRecallResult,
    chains_loaded: usize,
    corpus_docs: usize,
    duplicate_bodies: usize,
    expected_id_coverage: (usize, usize),
    bootstrap: usize,
    smoke: bool,
) {
    let Some(root) = repo_root() else {
        eprintln!("[warn] repo_root() not found; skipping IR measurements JSON write");
        return;
    };
    let out_path = root.join("dev/plans/runs/0.7.1-EU-8-measurements.json");
    let per_relation: serde_json::Map<String, serde_json::Value> =
        result.per_relation_type.iter().map(|(k, v)| (k.clone(), v.json())).collect();
    let per_shape: serde_json::Map<String, serde_json::Value> =
        result.per_chain_shape.iter().map(|(k, v)| (k.clone(), v.json())).collect();

    let doc = json!({
        "_comment": "EU-8 IR (relevance-judged) recall measurements. Orthogonal to \
                     the ANN recall in 0.7.1-EU-7-measurements.json: scores \
                     engine.search() against externally-labelled relevant doc_ids \
                     from chain ground_truth_queries (NOT the embedder's self KNN). \
                     Regenerable: AGENT_LONG=1 cargo test --release -p fathomdb-engine \
                     --features default-embedder --test eu8_ir_validation. \
                     EU8_SMOKE=1 seeds only chain docs (small smoke); unset = full corpus.",
        "config": {
            "embedder": "fathomdb-bge-small-en-v1.5",
            "dimension": CORPUS_DIM,
            "chains_loaded": chains_loaded,
            "query_count": result.query_count,
            "corpus_docs": corpus_docs,
            "ground_truth_source": "chain.ground_truth_queries.expected_top_k_doc_ids (labelled)",
            "metrics": ["recall@10", "precision@10", "MRR", "NDCG@10"],
            "k": K,
            "target_exclusion": false,
            "bootstrap_resamples": bootstrap,
            "bootstrap_seed_hex": format!("{BOOTSTRAP_SEED:#x}"),
            "smoke_chain_docs_only": smoke,
        },
        "aggregate": {
            "recall_at_10": round4(result.mean_recall_at_k),
            "ci_lo": round4(result.ci_lo),
            "ci_hi": round4(result.ci_hi),
            "sigma": round4(result.sigma),
            "precision_at_10": round4(result.mean_precision_at_k),
            "mrr": round4(result.mean_mrr),
            "ndcg_at_10": round4(result.mean_ndcg),
            "query_count": result.query_count,
            "queries_with_zero_hits": result.queries_with_zero_hits,
        },
        "per_relation_type": per_relation,
        "per_chain_shape": per_shape,
        "diagnostics": {
            "duplicate_bodies": duplicate_bodies,
            "unmapped_retrieval_results": result.unmapped_retrieval_results,
            "queries_with_zero_hits": result.queries_with_zero_hits,
            "expected_doc_ids_present_in_corpus": expected_id_coverage.0,
            "expected_doc_ids_total": expected_id_coverage.1,
        },
    });
    std::fs::write(&out_path, serde_json::to_string_pretty(&doc).unwrap())
        .expect("write ir measurements json");
    eprintln!("EU8_WROTE {}", out_path.display());
}

// ── Driver ──────────────────────────────────────────────────────────────

#[test]
fn eu8_ir_validation() {
    if std::env::var_os("AGENT_LONG").is_none() {
        eprintln!("[skip] AGENT_LONG not set; EU-8 IR-recall measurement is opt-in");
        return;
    }
    if std::env::var("FATHOMDB_SKIP_NETWORK_TESTS").is_ok() {
        eprintln!("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; embedder cache unavailable");
        return;
    }

    let max_chains = env_usize("EU8_MAX_CHAINS", 200);
    let bootstrap = env_usize("EU8_BOOTSTRAP", 1000);
    let smoke = std::env::var_os("EU8_SMOKE").is_some();

    let Some(chains) = load_chains_or_skip(max_chains) else {
        eprintln!("[skip] chains absent; cannot run EU-8 IR-recall measurement");
        return;
    };
    let ir_queries = extract_ground_truth_queries(&chains);
    if ir_queries.is_empty() {
        eprintln!("[skip] no ground_truth_queries parsed from {} chains", chains.len());
        return;
    }
    eprintln!(
        "EU8_SETUP chains_loaded={} ir_queries={} smoke_chain_docs_only={smoke} bootstrap={bootstrap}",
        chains.len(),
        ir_queries.len()
    );

    // Build the corpus to seed. In SMOKE mode (default for the dev box
    // contention guard) seed ONLY the chain docs -- they are the relevant
    // universe for the chain queries, so IR recall is measurable on them.
    // Without EU8_SMOKE, seed the full corpus (the documented full-verdict
    // run; do NOT launch on a busy box).
    let wanted: HashSet<String> = chains.iter().flat_map(|c| c.doc_ids.iter().cloned()).collect();
    let docs: Vec<Doc> = if smoke {
        match load_chain_docs(&wanted) {
            Some(d) => d,
            None => {
                eprintln!("[skip] corpus absent; cannot load chain docs for EU-8 smoke");
                return;
            }
        }
    } else {
        match corpus_subset::load_subset_or_skip(usize::MAX) {
            Some(d) => d,
            None => {
                eprintln!("[skip] corpus absent; cannot run EU-8 full measurement");
                return;
            }
        }
    };
    if docs.is_empty() {
        eprintln!("[skip] EU-8 loaded 0 docs");
        return;
    }
    eprintln!("EU8_CORPUS docs_to_seed={}", docs.len());

    // Coverage diagnostic: how many labelled expected ids are actually
    // present in the seeded corpus (an upper bound on achievable recall).
    let present_ids: HashSet<String> = docs.iter().map(|d| d.doc_id.clone()).collect();
    let all_expected: HashSet<String> =
        ir_queries.iter().flat_map(|q| q.expected_doc_ids.iter().cloned()).collect();
    let present_expected = all_expected.iter().filter(|id| present_ids.contains(*id)).count();
    eprintln!("EU8_COVERAGE expected_ids_present={}/{}", present_expected, all_expected.len());

    // ── Real BGE embedder + own engine (own TempDir/WAL; orthogonal). ──
    let embedder = Arc::new(SerializedBge::new(
        CandleBgeEmbedder::new().expect("construct real bge embedder"),
    ));
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("eu8_ir.sqlite");
    let opened = Engine::open_with_choice(
        &path,
        EmbedderChoice::Caller(embedder.clone() as Arc<dyn Embedder>),
    )
    .expect("open with real bge embedder");
    assert_eq!(
        opened.report.default_embedder.name, "fathomdb-bge-small-en-v1.5",
        "EU-8 must run against the real bge-small identity"
    );
    let engine = opened.engine;
    engine.configure_vector_kind_for_test(VECTOR_KIND).expect("configure vector kind");

    // Seed doc bodies in 256-doc batches with a per-batch 600s drain (mirrors
    // eu7's seed_slice). The shared `ingest()` helper drains only 30s, which is
    // fine for the smoke but trips `Scheduler` on the full ~7,667-doc corpus
    // (candle embed ~50 min >> 30s). IR recall scores against labelled
    // expected_doc_ids and never traverses edges, so seeding nodes (bodies)
    // only is sufficient; body->doc_id mapping is in-harness.
    {
        const BATCH: usize = 256;
        let started = Instant::now();
        let mut last_report = Instant::now();
        let mut written = 0usize;
        while written < docs.len() {
            let take = BATCH.min(docs.len() - written);
            let batch: Vec<PreparedWrite> = docs[written..written + take]
                .iter()
                .map(|d| PreparedWrite::Node {
                    kind: VECTOR_KIND.to_string(),
                    body: d.body.clone(),
                    source_id: Some(d.doc_id.clone()),
                })
                .collect();
            engine.write(&batch).expect("seed write");
            engine.drain(600_000).expect("seed drain (batch)");
            written += take;
            if last_report.elapsed() >= Duration::from_secs(30) {
                let rate = written as f64 / started.elapsed().as_secs_f64().max(1e-3);
                eprintln!(
                    "EU8_SEED_PROGRESS seeded={written}/{} rate_docs_per_s={rate:.1}",
                    docs.len()
                );
                last_report = Instant::now();
            }
        }
    }
    let nodes = docs.len();
    eprintln!("EU8_SEEDED nodes={nodes}");

    // body -> doc_id mapping (first-occurrence rule + dup count).
    let (body_to_doc_id, duplicate_bodies) = build_body_to_doc_id_map(&docs);
    eprintln!(
        "EU8_MAP body_to_doc_id_entries={} duplicate_bodies={duplicate_bodies}",
        body_to_doc_id.len()
    );

    let result = measure_ir_recall(&engine, &body_to_doc_id, &ir_queries, bootstrap);

    eprintln!(
        "EU8_NUMBERS{label} recall_at_10={:.4} ci_lo={:.4} ci_hi={:.4} sigma={:.4} \
         precision_at_10={:.4} mrr={:.4} ndcg_at_10={:.4} query_count={} \
         zero_hit_queries={} unmapped_results={} duplicate_bodies={duplicate_bodies}",
        result.mean_recall_at_k,
        result.ci_lo,
        result.ci_hi,
        result.sigma,
        result.mean_precision_at_k,
        result.mean_mrr,
        result.mean_ndcg,
        result.query_count,
        result.queries_with_zero_hits,
        result.unmapped_retrieval_results,
        label = if smoke { " (SMALL-CORPUS SMOKE)" } else { " (FULL CORPUS)" }
    );
    for (rel, b) in &result.per_relation_type {
        let n = b.count.max(1) as f64;
        eprintln!(
            "EU8_PER_RELATION rel={rel} n={} recall={:.4} precision={:.4} mrr={:.4} ndcg={:.4}",
            b.count,
            b.recall_sum / n,
            b.precision_sum / n,
            b.mrr_sum / n,
            b.ndcg_sum / n
        );
    }
    for (shape, b) in &result.per_chain_shape {
        let n = b.count.max(1) as f64;
        eprintln!(
            "EU8_PER_SHAPE shape={shape} n={} recall={:.4} precision={:.4} mrr={:.4} ndcg={:.4}",
            b.count,
            b.recall_sum / n,
            b.precision_sum / n,
            b.mrr_sum / n,
            b.ndcg_sum / n
        );
    }

    write_ir_measurements_json(
        &result,
        chains.len(),
        docs.len(),
        duplicate_bodies,
        (present_expected, all_expected.len()),
        bootstrap,
        smoke,
    );

    // Loose sanity assert (scouting, not a production floor): the real BGE
    // embedder should land labelled-relevant docs in the top-10 well above
    // chance. A value far below this signals a wiring bug.
    assert!(
        result.mean_recall_at_k >= SANITY_FLOOR,
        "EU-8 IR sanity: recall@10 {:.4} < sanity floor {SANITY_FLOOR} \
         (real BGE should retrieve labelled-relevant chain docs well above chance; \
         a value this low signals a harness/engine wiring bug, not a relevance gap)",
        result.mean_recall_at_k,
    );
}
