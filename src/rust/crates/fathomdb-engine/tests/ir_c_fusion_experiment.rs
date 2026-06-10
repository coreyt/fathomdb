//! IR-C Workstream 1 — RRF fusion experiment (text-arm ordering + arm weighting).
//!
//! Tests the WS1 hypothesis from `dev/plans/runs/ir-improvement-orchestration-prompt.md`
//! AND a sharper root-cause found while verifying the code: the production search
//! path orders the FTS/text branch by `write_cursor` (insertion order), NOT by
//! `bm25()` relevance (`fathomdb-engine/src/lib.rs:3968` —
//! `... ORDER BY write_cursor`), even though the `bm25()` score is selected. So the
//! "BM25 arm" of the hybrid never ranks by lexical relevance — which the same-dataset
//! literature says is exactly what EnronQA/QAConv reward (BM25 R@5 0.80–0.875).
//!
//! This experiment runs ENTIRELY in the harness — **no production-code change**:
//!   - vector arm: the existing `set_vector_stage_only_for_test` seam;
//!   - text arm: a read-only FTS5 query against the engine's own sqlite file,
//!     ordered EITHER by `bm25()` (relevance) OR `write_cursor` (to replicate the
//!     production arm in isolation);
//!   - fusion: a local weighted RRF (faithful to `fuse_rrf`: dedup on body,
//!     vector-first tiebreak) swept over arm weights + RRF k.
//! Retrieval happens once per query; the weight/order sweep re-fuses from cache, so
//! it is nearly free. Scored with the SAME `evaluate_gold_set` metric machinery as
//! the headline runner, so numbers are directly comparable.
//!
//! Validation anchor: the `hybrid_current` config (write_cursor text + equal RRF)
//! is cross-checked against the engine's real `RrfHybrid` on the same slice; if they
//! match, the harness fusion is faithful and the `bm25`-ordered numbers are trusted.
//!
//! Gated: `--features default-embedder` + `IRC_RUN=1` (graceful skip otherwise).
//!   IRC_RUN=1 IRC_FX=1 cargo test --release -p fathomdb-engine \
//!     --features default-embedder --test ir_c_fusion_experiment -- --nocapture
//!
//! Env: IRC_FX_EXACT (default 150) / IRC_FX_EXPLOR (default 80) sampled queries per
//! class; IRC_FX_MAXDOCS (default 1500) seeded doc budget (evidence always seeded).

#![cfg(feature = "default-embedder")]

#[path = "support/corpus_subset.rs"]
mod corpus_subset;
#[path = "support/ir_eval.rs"]
mod ir_eval;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use corpus_subset::{load_chain_docs, load_subset_or_skip, repo_root, Doc, VECTOR_KIND};
use fathomdb_embedder::CandleBgeEmbedder;
use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{EmbedderChoice, Engine, PreparedWrite};
use ir_eval::{
    evaluate_gold_set, load_gold_set, required_doc_ids, validate_gold_set, GoldQuery, GoldSet,
    QueryClass, K_LADDER,
};
use rusqlite::{Connection, OpenFlags};
use serde_json::json;
use tempfile::TempDir;

// ── Real BGE embedder, serialized (mirrors the headline runner). ────────────
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

/// Inlined copy of `fathomdb_query::compile_text_query` (not a dev-dependency):
/// whitespace-split, quote each token, AND-join — byte-identical to production.
fn compile_match_expression(raw: &str) -> String {
    compile_with_op(raw, " AND ")
}

/// WS4 candidate: bag-of-words OR semantics — standard BM25 query handling, where
/// any token may match and `bm25()` ranks by overlap. This is how the same-dataset
/// BM25 baselines (EnronQA/QAConv) are run; the production AND-join requires EVERY
/// token present, which near-zeroes recall on natural-language questions.
fn compile_match_expression_or(raw: &str) -> String {
    compile_with_op(raw, " OR ")
}

fn compile_with_op(raw: &str, op: &str) -> String {
    raw.split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(op)
}

/// Local weighted RRF, faithful to `fuse_rrf`: contribution `w / (k + rank1)`,
/// dedup keyed on body, vector-first tiebreak, deterministic sort. Returns fused
/// bodies in rank order. An arm passed empty contributes nothing (arm-only modes).
fn fuse_weighted(
    vec_bodies: &[String],
    text_bodies: &[String],
    w_vec: f64,
    w_text: f64,
    k: f64,
) -> Vec<String> {
    struct E {
        body: String,
        score: f64,
        in_vec: bool,
        order: usize,
    }
    let mut entries: Vec<E> = Vec::new();
    let mut acc = |body: &str, rank0: usize, w: f64, in_vec: bool, entries: &mut Vec<E>| {
        let contrib = w * (1.0 / (k + (rank0 as f64 + 1.0)));
        if let Some(e) = entries.iter_mut().find(|e| e.body == body) {
            e.score += contrib;
        } else {
            let order = entries.len();
            entries.push(E { body: body.to_string(), score: contrib, in_vec, order });
        }
    };
    if w_vec != 0.0 {
        for (r, b) in vec_bodies.iter().enumerate() {
            acc(b, r, w_vec, true, &mut entries);
        }
    }
    if w_text != 0.0 {
        for (r, b) in text_bodies.iter().enumerate() {
            acc(b, r, w_text, false, &mut entries);
        }
    }
    entries.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.in_vec.cmp(&a.in_vec))
            .then_with(|| a.order.cmp(&b.order))
    });
    entries.into_iter().map(|e| e.body).collect()
}

/// Cached per-query retrieval arms (retrieved once, re-fused many times).
struct Arms {
    vector: Vec<String>,        // vector-stage-only ranked bodies
    text_bm25: Vec<String>,     // AND-of-tokens (production compile) + bm25() order
    text_wcursor: Vec<String>,  // AND-of-tokens + write_cursor order (production arm)
    text_or: Vec<String>,       // WS4 fix: OR-of-tokens (bag-of-words) + bm25() order
    text_or_cov: Vec<usize>,    // # content query-tokens present in each text_or body
    content_len: usize,         // # content query-tokens (the N-of-M denominator)
    engine_hybrid: Vec<String>, // engine's real RrfHybrid (validation anchor)
}

/// Guarded text arm: keep OR candidates covering ≥ `cov` of the query's content
/// tokens (N-of-M), preserving bm25() order. `cov <= 0` = no guard (pure OR).
/// Empty content tokens ⇒ no guard (can't measure coverage).
fn guard_coverage(arms: &Arms, cov: f64) -> Vec<String> {
    if cov <= 0.0 || arms.content_len == 0 {
        return arms.text_or.clone();
    }
    let need = ((cov * arms.content_len as f64).ceil() as usize).max(1);
    arms.text_or
        .iter()
        .zip(arms.text_or_cov.iter())
        .filter(|(_, &c)| c >= need)
        .map(|(b, _)| b.clone())
        .collect()
}

/// Read-only FTS query against the engine's sqlite file, ordered by `order_sql`.
fn fts_bodies(conn: &Connection, match_expr: &str, order_sql: &str, cap: usize) -> Vec<String> {
    if match_expr.is_empty() {
        return Vec::new();
    }
    let sql = format!(
        "SELECT body FROM search_index WHERE search_index MATCH ?1 ORDER BY {order_sql} LIMIT {cap}"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else { return Vec::new() };
    let rows = stmt.query_map([match_expr], |row| row.get::<_, String>(0));
    match rows {
        Ok(it) => it.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

fn build_body_to_doc_id(docs: &[Doc]) -> HashMap<String, String> {
    let mut m = HashMap::with_capacity(docs.len());
    for d in docs {
        m.entry(d.body.clone()).or_insert_with(|| d.doc_id.clone());
    }
    m
}

fn map_bodies(bodies: &[String], m: &HashMap<String, String>) -> Vec<String> {
    bodies.iter().filter_map(|b| m.get(b).cloned()).collect()
}

/// Minimal stopword set so content-token coverage isn't inflated by function
/// words (the OR query still matches on them, but bm25's IDF + this coverage
/// guard both discount them).
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "was", "were", "what", "when", "where", "who", "whom", "which",
    "how", "why", "did", "does", "do", "is", "of", "to", "in", "on", "at", "by", "an", "a", "it",
    "its", "this", "that", "these", "those", "with", "from", "as", "be", "or", "if", "about",
    "into", "over", "than", "then", "they", "them", "their", "you", "your", "we", "our", "i",
];

fn tokenize_set(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_string())
        .collect()
}

/// Content tokens of a query: tokenized, ≥3 chars, stopwords removed. The
/// coverage denominator (the "M" in N-of-M).
fn content_tokens(query: &str) -> HashSet<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    tokenize_set(query).into_iter().filter(|t| !stop.contains(t.as_str())).collect()
}

#[test]
fn ir_c_fusion_experiment() {
    if std::env::var_os("IRC_RUN").is_none() {
        eprintln!("[skip] IRC_RUN not set; IR-C fusion experiment is opt-in");
        return;
    }
    if std::env::var_os("FATHOMDB_SKIP_NETWORK_TESTS").is_some() {
        eprintln!("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; embedder weights unavailable");
        return;
    }
    let n_exact = env_usize("IRC_FX_EXACT", 150);
    let n_explor = env_usize("IRC_FX_EXPLOR", 80);
    let n_neg = env_usize("IRC_FX_NEG", 60);
    let max_docs = env_usize("IRC_FX_MAXDOCS", 1500);

    let Some(root) = repo_root() else {
        eprintln!("[skip] repo_root() not found");
        return;
    };
    let gold_path = root.join("data/corpus-data/eval/ir_gold/all.gold.json");
    if !gold_path.exists() {
        eprintln!("[skip] {} absent (gitignored)", gold_path.display());
        return;
    }
    let full = load_gold_set(&gold_path).expect("load gold");
    assert!(validate_gold_set(&full).is_empty(), "gold invalid");

    // Strided per-class sample (deterministic, spans each class's id range).
    let pick = |class: QueryClass, n: usize| -> Vec<GoldQuery> {
        let pool: Vec<&GoldQuery> = full.queries.iter().filter(|q| q.query_class == class).collect();
        if pool.is_empty() || n == 0 {
            return Vec::new();
        }
        let n = n.min(pool.len());
        (0..n).map(|i| pool[i * pool.len() / n].clone()).collect()
    };
    let mut queries = pick(QueryClass::ExactFact, n_exact);
    queries.extend(pick(QueryClass::Exploratory, n_explor));
    queries.extend(pick(QueryClass::Negative, n_neg));
    eprintln!(
        "FX_SETUP exact_fact={} exploratory={} negative={} max_docs={max_docs}",
        queries.iter().filter(|q| q.query_class == QueryClass::ExactFact).count(),
        queries.iter().filter(|q| q.query_class == QueryClass::Exploratory).count(),
        queries.iter().filter(|q| q.query_class == QueryClass::Negative).count(),
    );

    // Doc universe: evidence docs (always) + distractors up to the budget.
    let evidence: HashSet<String> = queries.iter().flat_map(|q| required_doc_ids(q)).collect();
    let Some(mut docs) = load_chain_docs(&evidence) else {
        eprintln!("[skip] corpus absent");
        return;
    };
    let mut have: HashSet<String> = docs.iter().map(|d| d.doc_id.clone()).collect();
    if have.len() < max_docs {
        if let Some(extra) = load_subset_or_skip(usize::MAX) {
            for d in extra {
                if docs.len() >= max_docs {
                    break;
                }
                if have.insert(d.doc_id.clone()) {
                    docs.push(d);
                }
            }
        }
    }
    // Keep only queries whose (single) required evidence is present.
    let present: HashSet<String> = docs.iter().map(|d| d.doc_id.clone()).collect();
    let eval_queries: Vec<GoldQuery> = queries
        .into_iter()
        .filter(|q| required_doc_ids(q).iter().all(|id| present.contains(id)))
        .collect();
    eprintln!("FX_CORPUS docs={} eval_queries={}", docs.len(), eval_queries.len());
    if eval_queries.is_empty() {
        eprintln!("[skip] no evaluable queries");
        return;
    }
    let gold = GoldSet {
        corpus_hash: full.corpus_hash.clone(),
        qrels_version: full.qrels_version.clone(),
        note: full.note.clone(),
        queries: eval_queries,
    };

    // ── Engine + seed (mirrors the headline runner). ──
    let embedder =
        Arc::new(SerializedBge::new(CandleBgeEmbedder::new().expect("bge embedder")));
    let dir = TempDir::new().expect("tempdir");
    let db_path = dir.path().join("ir_c_fx.sqlite");
    let opened = Engine::open_with_choice(
        &db_path,
        EmbedderChoice::Caller(embedder.clone() as Arc<dyn Embedder>),
    )
    .expect("open engine");
    let engine = opened.engine;
    engine.configure_vector_kind_for_test(VECTOR_KIND).expect("configure vector kind");
    {
        const BATCH: usize = 256;
        let mut written = 0usize;
        while written < docs.len() {
            let take = BATCH.min(docs.len() - written);
            let batch: Vec<PreparedWrite> = docs[written..written + take]
                .iter()
                .map(|d| PreparedWrite::Node {
                    kind: VECTOR_KIND.to_string(),
                    body: d.body.clone(),
                    source_id: Some(d.doc_id.clone()),
                    logical_id: None,
                })
                .collect();
            engine.write(&batch).expect("seed write");
            engine.drain(600_000).expect("seed drain");
            written += take;
        }
    }
    eprintln!("FX_SEEDED docs={}", docs.len());

    let body_to_doc = build_body_to_doc_id(&docs);
    let deepest = *K_LADDER.iter().max().unwrap();
    engine.set_search_limit_for_test(deepest.max(64));

    // ── Retrieve each arm ONCE per query (cache for the free sweep). ──
    let fts = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open read-only fts conn");
    let cap = 200usize;
    let mut cache: HashMap<String, Arms> = HashMap::with_capacity(gold.queries.len());
    for q in &gold.queries {
        let key = q.query_id.clone().unwrap_or_else(|| q.query.clone());
        // vector-stage-only arm
        engine.set_vector_stage_only_for_test(true);
        let vector: Vec<String> = engine
            .search(&q.query)
            .expect("vector search")
            .results
            .into_iter()
            .map(|h| h.body)
            .collect();
        engine.set_vector_stage_only_for_test(false);
        // engine's real hybrid (validation anchor)
        let engine_hybrid: Vec<String> =
            engine.search(&q.query).expect("hybrid search").results.into_iter().map(|h| h.body).collect();
        // text arms (read-only FTS, two orderings)
        let expr = compile_match_expression(&q.query);
        let or_expr = compile_match_expression_or(&q.query);
        let text_bm25 = fts_bodies(&fts, &expr, "bm25(search_index)", cap);
        let text_wcursor = fts_bodies(&fts, &expr, "write_cursor", cap);
        let text_or = fts_bodies(&fts, &or_expr, "bm25(search_index)", cap);
        // Pre-compute content-token coverage per OR candidate (for the N-of-M guard).
        let content = content_tokens(&q.query);
        let content_len = content.len();
        let text_or_cov: Vec<usize> = text_or
            .iter()
            .map(|b| {
                let bt = tokenize_set(b);
                content.iter().filter(|t| bt.contains(*t)).count()
            })
            .collect();
        cache.insert(
            key,
            Arms { vector, text_bm25, text_wcursor, text_or, text_or_cov, content_len, engine_hybrid },
        );
    }
    eprintln!("FX_RETRIEVED queries={}", cache.len());

    // ── Configs: (name, w_vec, w_text, k, ord, cov, abstain). ──
    // ord: "none"(vector) | "engine"(anchor) | "bm25"(AND) | "bm25or"(OR).
    // cov: N-of-M content-token coverage guard on the OR arm (0 = none).
    // abstain: when guarded text is empty, return NOTHING (confidence gate) so the
    //          pipeline can abstain on no-answer (negative) queries.
    let configs: Vec<(&str, f64, f64, f64, &str, f64, bool)> = vec![
        ("vector_only", 1.0, 0.0, 60.0, "none", 0.0, false),
        ("hybrid_current(anchor)", 0.0, 0.0, 60.0, "engine", 0.0, false),
        ("bm25_only_AND", 0.0, 1.0, 60.0, "bm25", 0.0, false),
        // OR baselines (high recall, but no abstention → high negative FPR):
        ("bm25_only_OR", 0.0, 1.0, 60.0, "bm25or", 0.0, false),
        ("hybrid_OR_2x", 1.0, 2.0, 60.0, "bm25or", 0.0, false),
        ("hybrid_OR_3x", 1.0, 3.0, 60.0, "bm25or", 0.0, false),
        // GUARDED: OR + N-of-M coverage + abstention gate:
        ("bm25_OR_cov50", 0.0, 1.0, 60.0, "bm25or", 0.50, true),
        ("bm25_OR_cov67", 0.0, 1.0, 60.0, "bm25or", 0.67, true),
        ("hybrid_OR_3x_gate50", 1.0, 3.0, 60.0, "bm25or", 0.50, true),
        ("hybrid_OR_3x_gate67", 1.0, 3.0, 60.0, "bm25or", 0.67, true),
        ("hybrid_OR_3x_gate100", 1.0, 3.0, 60.0, "bm25or", 1.0, true),
    ];

    let mut report = serde_json::Map::new();
    let class_recall = |by_k: &BTreeMap<usize, ir_eval::KResult>, cls: QueryClass, k: usize| -> f64 {
        by_k.get(&k).and_then(|r| r.per_class.get(&cls)).map(|a| a.graded()).unwrap_or(0.0)
    };

    eprintln!(
        "\nFX_RESULTS config | exact_fact R@5/10/20/50 | exploratory R@10 | neg_abstain(want↑ for safety)"
    );
    for (name, w_vec, w_text, k, ord, cov, abstain) in &configs {
        let by_k = evaluate_gold_set(&gold, &K_LADDER, |q| {
            let key = q.query_id.clone().unwrap_or_else(|| q.query.clone());
            let arms = cache.get(&key).expect("cached arms");
            let fused: Vec<String> = match *ord {
                "engine" => arms.engine_hybrid.clone(),
                "none" => fuse_weighted(&arms.vector, &[], *w_vec, 0.0, *k),
                "bm25" => fuse_weighted(&arms.vector, &arms.text_bm25, *w_vec, *w_text, *k),
                "wcursor" => fuse_weighted(&arms.vector, &arms.text_wcursor, *w_vec, *w_text, *k),
                "bm25or" => {
                    let guarded = guard_coverage(arms, *cov);
                    if *abstain && guarded.is_empty() {
                        Vec::new() // confidence gate → abstain
                    } else {
                        fuse_weighted(&arms.vector, &guarded, *w_vec, *w_text, *k)
                    }
                }
                _ => unreachable!(),
            };
            Ok(map_bodies(&fused, &body_to_doc))
        })
        .expect("evaluate");

        let ef: Vec<f64> =
            K_LADDER.iter().map(|&k| class_recall(&by_k, QueryClass::ExactFact, k)).collect();
        let ex10 = class_recall(&by_k, QueryClass::Exploratory, 10);
        // Negative-class abstention at K=10 (correct = returned nothing).
        let (neg_n, neg_abst) =
            by_k.get(&10).map(|r| (r.negative.n, r.negative.abstained)).unwrap_or((0, 0));
        let abst_rate = if neg_n > 0 { neg_abst as f64 / neg_n as f64 } else { 0.0 };
        eprintln!(
            "FX_ROW {:22} | {:.3} {:.3} {:.3} {:.3} | {:.3} | {:.2} ({}/{})",
            name, ef[0], ef[1], ef[2], ef[3], ex10, abst_rate, neg_abst, neg_n
        );
        report.insert(
            (*name).to_string(),
            json!({
                "w_vec": w_vec, "w_text": w_text, "rrf_k": k, "text_order": ord,
                "coverage_guard": cov, "abstain_gate": abstain,
                "exact_fact": {"r5": ef[0], "r10": ef[1], "r20": ef[2], "r50": ef[3]},
                "exploratory_r10": ex10,
                "negative_abstain_rate": abst_rate,
            }),
        );
    }

    // ── Write report. ──
    let out = root.join("dev/plans/runs/IR-C-ws1-fusion-experiment.json");
    let doc = json!({
        "_comment": "IR-C WS1 fusion experiment. Harness-side weighted RRF sweep over \
                     a sampled exact_fact+exploratory slice; tests whether ordering the \
                     text arm by bm25() relevance (vs production write_cursor) and \
                     weighting it lifts recall. Small-corpus (directional).",
        "docs_seeded": docs.len(),
        "eval_queries": gold.queries.len(),
        "k_ladder": K_LADDER,
        "configs": serde_json::Value::Object(report),
    });
    std::fs::write(&out, serde_json::to_string_pretty(&doc).unwrap()).expect("write report");
    eprintln!("FX_WROTE {}", out.display());

    // ── Sanity: the harness anchor must track the engine's real hybrid. ──
    // (Both 'hybrid_current(anchor)' and 'hybrid_wcursor_equal' replicate the
    // production arm ordering; they should land close on exact_fact R@10.)
    eprintln!("FX_NOTE anchor=engine_hybrid vs harness wcursor_equal validate the fusion fidelity");
}
