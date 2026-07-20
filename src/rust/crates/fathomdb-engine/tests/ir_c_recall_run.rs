//! IR-C — real-corpus Evidence Recall@K run over the RESOLVED gold set.
//!
//! Drives the IR-B measure (`support/ir_eval.rs`: `run_experiment` /
//! Evidence Recall@K over the K-ladder, per-class + negative buckets) against
//! the REAL `Engine::search` path with the shipped BGE embedder, using the
//! gold set resolved from the dataset QA by
//! `tests/corpus/scripts/build_ir_gold.py` (4,597 queries, pinned to the frozen
//! 0.8.x-B snapshot).
//!
//! This is the runner the IR-B scaffold deferred to the corpus freeze
//! (`dev/plans/runs/IR-B-deferred-on-corpus-freeze.md`). It mirrors eu8's
//! real-BGE ingest (own TempDir/WAL, batched seed+drain) but scores via the
//! consensus Evidence Recall@K math rather than eu8's bespoke binary metrics.
//!
//! ## Gating + run (opt-in; default `cargo test` SKIPs)
//! Requires `--features default-embedder` AND `IRC_RUN=1`. Graceful skip when
//! the feature/env/gold/corpus are absent, so CI stays green.
//!
//!   # fast SMALL-CORPUS smoke (sampled queries + their evidence + distractors):
//!   IRC_RUN=1 IRC_SMOKE=1 cargo test --release -p fathomdb-engine \
//!     --features default-embedder --test ir_c_recall_run -- --nocapture
//!
//!   # full headline run (embeds the whole corpus — the multi-hour job):
//!   IRC_RUN=1 cargo test --release -p fathomdb-engine \
//!     --features default-embedder --test ir_c_recall_run -- --nocapture
//!
//! Env knobs:
//!   IRC_GOLD      gold file under data/corpus-data/eval/ir_gold/ (default all.gold.json)
//!   IRC_SMOKE     convenience: defaults IRC_SAMPLE=150, IRC_MAX_DOCS=600
//!   IRC_SAMPLE    evaluate only the first N gold queries (0 = all)
//!   IRC_MAX_DOCS  cap the seeded doc universe (0 = full corpus). Evidence docs
//!                 for the evaluated queries are ALWAYS seeded; the remainder of
//!                 the budget is filled with corpus distractors.

#![cfg(feature = "default-embedder")]

#[path = "support/corpus_subset.rs"]
mod corpus_subset;
#[path = "support/ir_eval.rs"]
mod ir_eval;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use corpus_subset::{load_chain_docs, load_subset_or_skip, repo_root, Doc, VECTOR_KIND};
use fathomdb_embedder::CandleBgeEmbedder;
use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{EmbedderChoice, Engine, PreparedWrite};
use ir_eval::{
    experiment_to_json, load_gold_set, required_doc_ids, run_experiment, validate_gold_set,
    GoldQuery, GoldSet, QueryClass, K_LADDER, RUNNABLE_NOW_MODES,
};
use serde_json::json;
use tempfile::TempDir;

// ── Serialized real BGE embedder (mirrors eu8's SerializedBge) ──────────────
// The engine's projection pool calls embed() concurrently on one shared
// instance; candle guidance is to guard a shared model for concurrent CPU
// inference. Measurement-fidelity wrapper only.
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

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Build the body->doc_id map (first-occurrence rule for duplicate bodies, as
/// eu8): a retrieved body maps to the first-ingested doc_id; later collisions
/// only push recall DOWN, never up.
fn build_body_to_doc_id_map(docs: &[Doc]) -> (HashMap<String, String>, usize) {
    let mut map = HashMap::with_capacity(docs.len());
    let mut dups = 0usize;
    for d in docs {
        if map.contains_key(&d.body) {
            dups += 1;
        } else {
            map.insert(d.body.clone(), d.doc_id.clone());
        }
    }
    (map, dups)
}

#[test]
fn ir_c_recall_run() {
    if std::env::var_os("IRC_RUN").is_none() {
        eprintln!("[skip] IRC_RUN not set; IR-C real-corpus recall run is opt-in");
        return;
    }
    if std::env::var_os("FATHOMDB_SKIP_NETWORK_TESTS").is_some() {
        eprintln!("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; embedder weights unavailable");
        return;
    }
    let smoke = std::env::var_os("IRC_SMOKE").is_some();
    let sample = env_usize("IRC_SAMPLE", if smoke { 150 } else { 0 });
    let max_docs = env_usize("IRC_MAX_DOCS", if smoke { 600 } else { 0 });
    let gold_file = std::env::var("IRC_GOLD").unwrap_or_else(|_| "all.gold.json".to_string());

    // ── Load + validate the resolved gold (pinned to the frozen snapshot). ──
    let Some(root) = repo_root() else {
        eprintln!("[skip] repo_root() not found");
        return;
    };
    let gold_path = root.join("data/corpus-data/eval/ir_gold").join(&gold_file);
    if !gold_path.exists() {
        eprintln!(
            "[skip] {} absent (gitignored — run tests/corpus/scripts/build_ir_gold.py)",
            gold_path.display()
        );
        return;
    }
    let full_gold = load_gold_set(&gold_path).expect("load resolved gold");
    let issues = validate_gold_set(&full_gold);
    assert!(issues.is_empty(), "resolved gold invalid: {issues:?}");

    // Deterministic STRIDED sample for the smoke; all for full. The gold is
    // sorted by query_id, so the per-source blocks (enronqa/qaconv/qmsum) are
    // contiguous — a strided pick spans all sources and therefore all classes
    // (exact_fact + exploratory + negative), where `take(N)` would draw only the
    // first block and hide the harder classes.
    let queries: Vec<GoldQuery> = if sample > 0 && sample < full_gold.queries.len() {
        let total = full_gold.queries.len();
        (0..sample).map(|i| full_gold.queries[i * total / sample].clone()).collect()
    } else {
        full_gold.queries.clone()
    };
    eprintln!(
        "IRC_SETUP gold={gold_file} smoke={smoke} queries_evaluated={} (of {}) max_docs={}",
        queries.len(),
        full_gold.queries.len(),
        if max_docs == 0 { "full".to_string() } else { max_docs.to_string() }
    );

    // ── Build the doc universe: evidence docs for the evaluated queries
    //    (ALWAYS seeded) + corpus distractors up to the budget. ──
    let evidence_ids: HashSet<String> = queries.iter().flat_map(|q| required_doc_ids(q)).collect();
    let Some(mut docs) = load_chain_docs(&evidence_ids) else {
        eprintln!("[skip] corpus absent; cannot load evidence docs");
        return;
    };
    let mut have: HashSet<String> = docs.iter().map(|d| d.doc_id.clone()).collect();

    // Fill the rest of the budget (or the full corpus) with distractors.
    let want_total = if max_docs == 0 { usize::MAX } else { max_docs.max(docs.len()) };
    if have.len() < want_total {
        if let Some(subset) = load_subset_or_skip(usize::MAX) {
            for d in subset {
                if docs.len() >= want_total {
                    break;
                }
                if have.insert(d.doc_id.clone()) {
                    docs.push(d);
                }
            }
        }
    }
    if docs.is_empty() {
        eprintln!("[skip] loaded 0 docs");
        return;
    }

    // Evaluate only queries whose required evidence is fully present in the
    // seeded universe (a missing-evidence query is an automatic miss that would
    // unfairly depress recall — report how many we excluded for transparency).
    // Negative (abstention) queries have no required evidence and always stay.
    let present: HashSet<String> = docs.iter().map(|d| d.doc_id.clone()).collect();
    let mut excluded_missing_evidence = 0usize;
    let eval_queries: Vec<GoldQuery> = queries
        .into_iter()
        .filter(|q| {
            if q.query_class == QueryClass::Negative {
                return true;
            }
            let req = required_doc_ids(q);
            let ok = req.iter().all(|id| present.contains(id));
            if !ok {
                excluded_missing_evidence += 1;
            }
            ok
        })
        .collect();
    eprintln!(
        "IRC_CORPUS docs_to_seed={} evidence_docs={} eval_queries={} excluded_missing_evidence={}",
        docs.len(),
        evidence_ids.len(),
        eval_queries.len(),
        excluded_missing_evidence
    );
    if eval_queries.is_empty() {
        eprintln!("[skip] no evaluable queries after evidence-presence filter");
        return;
    }
    let gold = GoldSet {
        corpus_hash: full_gold.corpus_hash.clone(),
        qrels_version: full_gold.qrels_version.clone(),
        note: full_gold.note.clone(),
        queries: eval_queries,
    };

    // ── Real BGE embedder + own engine (own TempDir/WAL). ──
    let embedder = Arc::new(SerializedBge::new(
        CandleBgeEmbedder::new().expect("construct real bge embedder"),
    ));
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("ir_c.sqlite");
    let opened = Engine::open_with_choice(
        &path,
        EmbedderChoice::Caller(embedder.clone() as Arc<dyn Embedder>),
    )
    .expect("open with real bge embedder");
    assert_eq!(
        opened.report.default_embedder.name, "fathomdb-bge-small-en-v1.5",
        "IR-C must run against the real bge-small identity"
    );
    let engine = opened.engine;
    engine.configure_vector_kind_for_test(VECTOR_KIND).expect("configure vector kind");

    // Seed bodies in 256-doc batches with a generous per-batch drain (candle
    // embed is the bottleneck; IR scores against labelled doc_ids and never
    // traverses edges, so seeding nodes/bodies is sufficient).
    {
        const BATCH: usize = 256;
        let started = Instant::now();
        let mut last = Instant::now();
        let mut written = 0usize;
        while written < docs.len() {
            let take = BATCH.min(docs.len() - written);
            let batch: Vec<PreparedWrite> = docs[written..written + take]
                .iter()
                .map(|d| PreparedWrite::Node {
                    kind: VECTOR_KIND.to_string(),
                    body: d.body.clone(),
                    source_id: fathomdb_engine::SourceId::new(d.doc_id.clone())
                        .expect("test source id"),
                    logical_id: None,
                    state: fathomdb_engine::InitialState::Active,
                    reason: None,
                })
                .collect();
            engine.write(&batch).expect("seed write");
            engine.drain(600_000).expect("seed drain (batch)");
            written += take;
            if last.elapsed() >= Duration::from_secs(30) {
                let rate = written as f64 / started.elapsed().as_secs_f64().max(1e-3);
                eprintln!(
                    "IRC_SEED_PROGRESS seeded={written}/{} rate_docs_per_s={rate:.2}",
                    docs.len()
                );
                last = Instant::now();
            }
        }
        eprintln!("IRC_SEEDED nodes={} in {:.1}s", docs.len(), started.elapsed().as_secs_f64());
    }

    let (body_to_doc_id, duplicate_bodies) = build_body_to_doc_id_map(&docs);

    // ── Run the mode×K×class Evidence Recall@K experiment. ──
    let result = run_experiment(&engine, &gold, &body_to_doc_id, &RUNNABLE_NOW_MODES, &K_LADDER)
        .expect("run_experiment");

    // Headline = RrfHybrid @ K=10.
    let headline =
        result.per_mode.get(&ir_eval::RetrievalMode::RrfHybrid).and_then(|by_k| by_k.get(&10));
    if let Some(h) = headline {
        eprintln!(
            "IRC_HEADLINE{} rrf_hybrid@10 strict={:.4} graded={:.4} n={} \
             negative(n={}, fpr={:.4})",
            if smoke { " (SMALL-CORPUS SMOKE)" } else { " (FULL CORPUS)" },
            h.overall.strict(),
            h.overall.graded(),
            h.overall.n,
            h.negative.n,
            h.negative.false_positive_rate(),
        );
        for (cls, agg) in &h.per_class {
            eprintln!(
                "IRC_CLASS class={} n={} strict={:.4} graded={:.4}",
                cls.label(),
                agg.n,
                agg.strict(),
                agg.graded()
            );
        }
    }

    // ── Write the structured report (the shape IR-2/HITL will consume). ──
    let report = json!({
        "_comment": "IR-C Evidence Recall@K over the resolved gold set. SMOKE \
                     (sampled queries + capped corpus) is directional only; the \
                     headline number requires the full-corpus run (IRC_SMOKE unset).",
        "run_kind": if smoke { "small_corpus_smoke" } else { "full_corpus" },
        "gold_file": gold_file,
        "queries_evaluated": gold.queries.len(),
        "queries_excluded_missing_evidence": excluded_missing_evidence,
        "docs_seeded": docs.len(),
        "duplicate_bodies": duplicate_bodies,
        "experiment": experiment_to_json(&gold, &result),
    });
    let suffix = if smoke { "smoke" } else { "full" };
    let out = root.join(format!("dev/plans/runs/IR-C-recall-{suffix}.json"));
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).expect("write report");
    eprintln!("IRC_WROTE {}", out.display());

    // ── Structural sanity (NOT a relevance gate): every aggregate is a
    //    well-formed fraction, and the run actually retrieved something. ──
    for by_k in result.per_mode.values() {
        for &k in &K_LADDER {
            let r = &by_k[&k];
            for v in [r.overall.strict(), r.overall.graded(), r.overall.supporting()] {
                assert!((0.0..=1.0).contains(&v), "aggregate {v} out of [0,1] at K={k}");
            }
        }
    }
    if let Some(h) = headline {
        assert!(
            h.overall.graded() > 0.0,
            "headline graded recall@10 is 0 — the real BGE path should retrieve SOME \
             labelled-relevant doc; a flat zero signals a harness/engine wiring bug"
        );
    }
}
