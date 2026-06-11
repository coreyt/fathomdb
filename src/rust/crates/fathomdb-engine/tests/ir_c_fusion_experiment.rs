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
//! IRC_FX_FULL=1 → full corpus + full gold pool at production k=30, restricted to
//! the whole-vs-128/96 decision set (the Option-A deep-K exploratory question);
//! per-class/doc-budget envs still override. Writes a separate `-full.json` report.

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
#[allow(dead_code)] // production AND-compile; kept for future probes
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

/// Re-visited lever (unburied by the OR fix): OR over *content* tokens only —
/// stopwords stripped — to cut the false matches raw-OR picks up on function
/// words. Falls back to raw-OR if the query is all stopwords.
fn compile_content_or(raw: &str) -> String {
    let toks = content_tokens(raw);
    if toks.is_empty() {
        return compile_match_expression_or(raw);
    }
    toks.iter().map(|t| format!("\"{t}\"")).collect::<Vec<_>>().join(" OR ")
}

/// BGE-en-v1.5 retrieval query instruction (model card). Query-side only;
/// passages stay bare. Rejected on whole-doc vectors, re-tested on passages
/// (the granularity the instruction targets).
const BGE_QUERY_INSTRUCTION: &str = "Represent this sentence for searching relevant passages: ";

/// Split a body into overlapping word-window passages (long bodies exceed
/// bge-small's ~512-token window and get mean-pool-diluted). Short bodies pass
/// through as a single chunk; `size = usize::MAX` ⇒ whole-doc (one passage).
fn chunk_words(body: &str, size: usize, stride: usize, max_chunks: usize) -> Vec<String> {
    let words: Vec<&str> = body.split_whitespace().collect();
    if words.len() <= size {
        return vec![body.to_string()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() && chunks.len() < max_chunks {
        let end = (start + size).min(words.len());
        chunks.push(words[start..end].join(" "));
        if end == words.len() {
            break;
        }
        start += stride;
    }
    chunks
}

/// Passage-score aggregation to doc level.
#[derive(Clone, Copy)]
enum Pool {
    Max,  // doc scores as its single best passage
    Mean, // average over all the doc's passages (rewards uniform relevance)
    Top2, // average of the doc's two best passages (max/mean compromise)
}

/// KNN over passage vectors, pooled to ranked doc_ids — already in evaluation
/// (doc_id) space. One pass accumulates sum/count/top-2 per doc.
fn knn_docs_pool(
    qv: &[f32],
    passages: &[(String, Vec<f32>)],
    k: usize,
    pool: Pool,
) -> Vec<String> {
    struct Acc {
        sum: f32,
        n: u32,
        b1: f32,
        b2: f32,
    }
    let mut by_doc: HashMap<&str, Acc> = HashMap::new();
    for (doc_id, pv) in passages {
        let dot: f32 = qv.iter().zip(pv).map(|(a, b)| a * b).sum();
        let e = by_doc
            .entry(doc_id.as_str())
            .or_insert(Acc { sum: 0.0, n: 0, b1: f32::MIN, b2: f32::MIN });
        e.sum += dot;
        e.n += 1;
        if dot > e.b1 {
            e.b2 = e.b1;
            e.b1 = dot;
        } else if dot > e.b2 {
            e.b2 = dot;
        }
    }
    let mut v: Vec<(&str, f32)> = by_doc
        .into_iter()
        .map(|(d, a)| {
            let s = match pool {
                Pool::Max => a.b1,
                Pool::Mean => a.sum / a.n as f32,
                Pool::Top2 => {
                    if a.n >= 2 {
                        (a.b1 + a.b2) / 2.0
                    } else {
                        a.b1
                    }
                }
            };
            (d, s)
        })
        .collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    v.into_iter().take(k).map(|(d, _)| d.to_string()).collect()
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
/// Per-query cache for the geometry × pooling × prefix sweep. Query embeddings
/// (bare + prefixed) and the lexical doc_ids are geometry-independent, so they're
/// computed once; pooling over the per-geometry passage sets is done at eval time.
struct QCache {
    qv_bare: Vec<f32>,      // bare query embedding
    qv_pref: Vec<f32>,      // BGE-query-instruction-prefixed query embedding
    text_ids: Vec<String>,  // content-OR lexical arm, mapped to ranked doc_ids
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
    // Full-corpus mode (`IRC_FX_FULL=1`): seed the entire corpus and evaluate the
    // entire gold pool, to answer the Option-A deep-K exploratory question at scale
    // (the small default is directional only). Defaults flip to "all" but stay
    // overridable. The 1,500-doc / sampled path is unchanged when the flag is off.
    let full_mode = std::env::var_os("IRC_FX_FULL").is_some();
    let n_exact = env_usize("IRC_FX_EXACT", if full_mode { usize::MAX } else { 150 });
    let n_explor = env_usize("IRC_FX_EXPLOR", if full_mode { usize::MAX } else { 80 });
    let n_neg = env_usize("IRC_FX_NEG", if full_mode { usize::MAX } else { 60 });
    let max_docs = env_usize("IRC_FX_MAXDOCS", if full_mode { usize::MAX } else { 1500 });
    eprintln!("FX_MODE full={full_mode}");

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
    // The dense arm is computed entirely harness-side from re-embedded passages,
    // so the engine's per-doc vector projection is dead weight — at full corpus it
    // would embed ~10.5k bodies for vectors we never read. Freeze it; the FTS
    // `search_index` (the only engine artifact the text arm queries) is written
    // synchronously in commit, so no drain is needed (pr_g9 soft_fallback pins the
    // synchronous-FTS contract). Small mode keeps the original write+drain path.
    if full_mode {
        engine.set_projection_scheduler_frozen_for_test(true);
    }
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
            if !full_mode {
                engine.drain(600_000).expect("seed drain");
            }
            written += take;
        }
    }
    eprintln!("FX_SEEDED docs={}", docs.len());

    // Harness-side passage indexes, one per geometry (re-embeds bodies with the
    // same BGE embedder — no engine mean-centering / ANN quantization). "whole"
    // (size=MAX ⇒ one passage/doc) is the whole-doc anchor under the same pooling
    // path. (label, size, stride, max_chunks).
    // Full mode answers the Option-A deep-K question with just two geometries: the
    // whole-doc dense baseline (geom 0) vs 128/96 + max-pool (geom 1, the doc's best
    // exploratory compromise). The 64/256 windows and mean/top2 pooling are already
    // characterized as losing; dropping them keeps full-corpus passage embedding
    // tractable. Small mode keeps the full 4-geometry sweep.
    let geoms: Vec<(&str, usize, usize, usize)> = if full_mode {
        vec![("whole", usize::MAX, 1, 1), ("128/96", 128, 96, 8)]
    } else {
        vec![
            ("whole", usize::MAX, 1, 1),
            ("64/48", 64, 48, 6),
            ("128/96", 128, 96, 8),
            ("256/192", 256, 192, 4),
        ]
    };
    let passage_sets: Vec<(&str, Vec<(String, Vec<f32>)>)> = geoms
        .iter()
        .map(|(label, size, stride, max)| {
            let mut pv: Vec<(String, Vec<f32>)> = Vec::with_capacity(docs.len() * 4);
            for d in &docs {
                for chunk in chunk_words(&d.body, *size, *stride, *max) {
                    pv.push((d.doc_id.clone(), embedder.embed(&chunk).expect("embed chunk")));
                }
            }
            eprintln!("FX_PASSAGES geom={label} n={} (size={size}/stride={stride}/max={max})", pv.len());
            (*label, pv)
        })
        .collect();

    let body_to_doc = build_body_to_doc_id(&docs);
    let deepest = *K_LADDER.iter().max().unwrap();
    engine.set_search_limit_for_test(deepest.max(64));

    // ── Query-side cache: embeddings (bare + prefixed) and the lexical arm. ──
    // The pooled passage retrieval happens per-config at eval time (cheap
    // re-aggregation over the precomputed passage vectors).
    let fts = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open read-only fts conn");
    let cap = 200usize;
    let mut cache: HashMap<String, QCache> = HashMap::with_capacity(gold.queries.len());
    for q in &gold.queries {
        let key = q.query_id.clone().unwrap_or_else(|| q.query.clone());
        let qv_bare = embedder.embed(&q.query).expect("embed query");
        // The BGE query-prefix is definitively rejected (every geometry, both
        // classes); full mode skips the second embed per query and never builds a
        // prefixed config. Small mode still measures it.
        let qv_pref = if full_mode {
            qv_bare.clone()
        } else {
            embedder
                .embed(&format!("{BGE_QUERY_INSTRUCTION}{}", q.query))
                .expect("embed prefixed query")
        };
        let text_bodies =
            fts_bodies(&fts, &compile_content_or(&q.query), "bm25(search_index)", cap);
        let text_ids = map_bodies(&text_bodies, &body_to_doc);
        cache.insert(key, QCache { qv_bare, qv_pref, text_ids });
    }
    eprintln!("FX_RETRIEVED queries={}", cache.len());

    // ── Geometry × pooling × prefix sweep on the chunked dense arm. ──
    // Chunking was the first real dense win; now sweep its knobs and re-test the
    // BGE query-prefix (rejected on whole-doc, but it targets passage granularity).
    struct Cfg {
        name: String,
        wv: f64,
        wt: f64,
        k: f64,
        geom: usize, // index into passage_sets / geoms
        pool: Pool,
        prefix: bool,
    }
    let pool_label = |p: Pool| match p {
        Pool::Max => "max",
        Pool::Mean => "mean",
        Pool::Top2 => "top2",
    };
    let mut configs: Vec<Cfg> = Vec::new();
    if full_mode {
        // Decision set at production k=30: the Option-A deep-K exploratory question.
        // text-only lexical ceiling; whole-doc vs 128/96 dense baseline; and the
        // whole-doc vs chunked hybrid at the shipped 3:1 and at 1:1 (where the
        // directional run located Option-A's deep-K payoff). geom 0=whole, 1=128/96.
        let k = 30.0; // RRF_K (production)
        configs.push(Cfg { name: "text_only_ORc".into(), wv: 0.0, wt: 1.0, k, geom: 0, pool: Pool::Max, prefix: false });
        configs.push(Cfg { name: "v_whole_max".into(), wv: 1.0, wt: 0.0, k, geom: 0, pool: Pool::Max, prefix: false });
        configs.push(Cfg { name: "v_128/96_max".into(), wv: 1.0, wt: 0.0, k, geom: 1, pool: Pool::Max, prefix: false });
        for (gi, label) in [(0usize, "whole"), (1usize, "128/96")] {
            for (wv, wt, w) in [(1.0, 3.0, "1:3"), (1.0, 1.0, "1:1")] {
                configs.push(Cfg { name: format!("h_{label}_{w}"), wv, wt, k, geom: gi, pool: Pool::Max, prefix: false });
            }
        }
    } else {
        // Lexical anchor (wv=0 → vector arm ignored).
        configs.push(Cfg { name: "text_only_ORc".into(), wv: 0.0, wt: 1.0, k: 60.0, geom: 0, pool: Pool::Max, prefix: false });
        // Whole-doc dense anchor (geom 0; pooling is a no-op at one passage/doc).
        for pref in [false, true] {
            let tag = if pref { "pref" } else { "bare" };
            configs.push(Cfg { name: format!("v_whole_{tag}"), wv: 1.0, wt: 0.0, k: 60.0, geom: 0, pool: Pool::Max, prefix: pref });
        }
        // Chunk geometries × pooling × prefix (vector-only).
        for gi in 1..passage_sets.len() {
            for pool in [Pool::Max, Pool::Mean, Pool::Top2] {
                for pref in [false, true] {
                    let tag = if pref { "pref" } else { "bare" };
                    let name = format!("v_{}_{}_{}", geoms[gi].0, pool_label(pool), tag);
                    configs.push(Cfg { name, wv: 1.0, wt: 0.0, k: 60.0, geom: gi, pool, prefix: pref });
                }
            }
        }
        // Curated hybrids (vector pool + content-OR, max-pool) to see if the best
        // dense geometry lifts the fused ceiling.
        for (gi, pref) in [(2usize, false), (2, true), (3, false), (1, false)] {
            let tag = if pref { "pref" } else { "bare" };
            for (wv, wt, w) in [(1.0, 3.0, "1:3"), (1.0, 1.0, "1:1")] {
                let name = format!("h_{}_{}_{}", geoms[gi].0, tag, w);
                configs.push(Cfg { name, wv, wt, k: 60.0, geom: gi, pool: Pool::Max, prefix: pref });
            }
        }
    }

    let mut report = serde_json::Map::new();
    let class_recall = |by_k: &BTreeMap<usize, ir_eval::KResult>, cls: QueryClass, k: usize| -> f64 {
        by_k.get(&k).and_then(|r| r.per_class.get(&cls)).map(|a| a.graded()).unwrap_or(0.0)
    };

    // Vector retrieval depends only on (query, geometry, pool, prefix) — NOT on the
    // fusion weights or k. Memoize the pooled passage KNN so the weight sweep
    // re-fuses from cache instead of re-running the brute-force KNN per config (the
    // dominant cost at full corpus). Deterministic ⇒ byte-identical results, fewer
    // KNN passes. The 0/1 prefix and pool discriminants key the cache.
    let pool_disc = |p: Pool| match p {
        Pool::Max => 0u8,
        Pool::Mean => 1,
        Pool::Top2 => 2,
    };
    let vec_cache: Mutex<HashMap<(String, usize, u8, bool), Vec<String>>> = Mutex::new(HashMap::new());

    eprintln!(
        "\nFX_RESULTS config | exact_fact R@5/10/20/50 | exploratory R@5/10/20/50 | neg_abst"
    );
    for cfg in &configs {
        let by_k = evaluate_gold_set(&gold, &K_LADDER, |q| {
            let key = q.query_id.clone().unwrap_or_else(|| q.query.clone());
            let qc = cache.get(&key).expect("cached qcache");
            // Pooled passage retrieval → ranked doc_ids (memoized); fuse with the
            // lexical arm. Both arms are doc-id space; a zero-weight arm is skipped
            // by fuse_weighted.
            let ckey = (key, cfg.geom, pool_disc(cfg.pool), cfg.prefix);
            let vec_ids = {
                let mut vc = vec_cache.lock().expect("vec_cache poisoned");
                if let Some(v) = vc.get(&ckey) {
                    v.clone()
                } else {
                    let qv = if cfg.prefix { &qc.qv_pref } else { &qc.qv_bare };
                    let v = knn_docs_pool(qv, &passage_sets[cfg.geom].1, cap, cfg.pool);
                    vc.insert(ckey, v.clone());
                    v
                }
            };
            Ok(fuse_weighted(&vec_ids, &qc.text_ids, cfg.wv, cfg.wt, cfg.k))
        })
        .expect("evaluate");

        let ef: Vec<f64> =
            K_LADDER.iter().map(|&k| class_recall(&by_k, QueryClass::ExactFact, k)).collect();
        let ex: Vec<f64> =
            K_LADDER.iter().map(|&k| class_recall(&by_k, QueryClass::Exploratory, k)).collect();
        let (neg_n, neg_abst) =
            by_k.get(&10).map(|r| (r.negative.n, r.negative.abstained)).unwrap_or((0, 0));
        let abst_rate = if neg_n > 0 { neg_abst as f64 / neg_n as f64 } else { 0.0 };
        eprintln!(
            "FX_ROW {:22} | {:.3} {:.3} {:.3} {:.3} | {:.3} {:.3} {:.3} {:.3} | {:.2}",
            cfg.name, ef[0], ef[1], ef[2], ef[3], ex[0], ex[1], ex[2], ex[3], abst_rate
        );
        report.insert(
            cfg.name.clone(),
            json!({
                "w_vec": cfg.wv, "w_text": cfg.wt, "rrf_k": cfg.k,
                "geom": geoms[cfg.geom].0, "pool": pool_label(cfg.pool), "prefix": cfg.prefix,
                "exact_fact": {"r5": ef[0], "r10": ef[1], "r20": ef[2], "r50": ef[3]},
                "exploratory": {"r5": ex[0], "r10": ex[1], "r20": ex[2], "r50": ex[3]},
                "negative_abstain_rate": abst_rate,
            }),
        );
    }

    // ── Complementarity diagnostic (exploratory). ──
    // The hybrid did not beat text-only at deep K; this asks WHY. For each
    // exploratory query take each arm's top-K doc set and compute the ORACLE UNION
    // recall (a gold doc counts if EITHER arm has it in top-K) — the upper bound
    // any fusion could reach. union ≈ text ⇒ the dense arm is genuinely REDUNDANT
    // (Option A cannot help the hybrid at any weight); union ≫ text ⇒ it is
    // COMPLEMENTARY and the gain is there to be captured by re-weighting. We report
    // both dense geometries so we can see whether chunking adds unique coverage that
    // the whole-doc arm does not. Binary presence, ungraded, multi-gold averaged as
    // |G∩topK|/|G|. Reuses the dense+text caches — no new embeds. geom 0=whole,
    // 1=128/96 (full-mode layout). `rescue` = queries with a gold doc the 128/96
    // arm surfaces in top-K that the text arm misses.
    let comp = if full_mode {
        let explor: Vec<&GoldQuery> =
            gold.queries.iter().filter(|q| q.query_class == QueryClass::Exploratory).collect();
        let topk = |ids: &[String], k: usize| -> HashSet<String> { ids.iter().take(k).cloned().collect() };
        let vc = vec_cache.lock().expect("vec_cache poisoned");
        let mut rows = serde_json::Map::new();
        eprintln!(
            "\nFX_COMP exploratory n={} | R@K | text dense_whole dense_128/96 | union_whole union_128/96 | rescue_128/96",
            explor.len()
        );
        for &k in &[10usize, 20, 50] {
            let (mut s_text, mut s_dw, mut s_d1, mut s_uw, mut s_u1) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
            let mut rescue = 0usize;
            let mut counted = 0.0f64;
            for q in &explor {
                let key = q.query_id.clone().unwrap_or_else(|| q.query.clone());
                let g: HashSet<String> = required_doc_ids(q).into_iter().collect();
                if g.is_empty() {
                    continue;
                }
                let t = topk(&cache.get(&key).expect("qc").text_ids, k);
                let dw = vc.get(&(key.clone(), 0usize, 0u8, false)).map(|v| topk(v, k)).unwrap_or_default();
                let d1 = vc.get(&(key.clone(), 1usize, 0u8, false)).map(|v| topk(v, k)).unwrap_or_default();
                let frac = |hit: &HashSet<String>| -> f64 {
                    g.iter().filter(|id| hit.contains(*id)).count() as f64 / g.len() as f64
                };
                s_text += frac(&t);
                s_dw += frac(&dw);
                s_d1 += frac(&d1);
                s_uw += frac(&t.union(&dw).cloned().collect());
                s_u1 += frac(&t.union(&d1).cloned().collect());
                if g.iter().any(|id| d1.contains(id) && !t.contains(id)) {
                    rescue += 1;
                }
                counted += 1.0;
            }
            let n = counted.max(1.0);
            let (text, dw, d1, uw, u1) = (s_text / n, s_dw / n, s_d1 / n, s_uw / n, s_u1 / n);
            eprintln!(
                "FX_COMP R@{:<2} | {:.3} {:.3} {:.3} | {:.3} {:.3} | {} ({:.0}%)",
                k, text, dw, d1, uw, u1, rescue, 100.0 * rescue as f64 / n
            );
            rows.insert(
                format!("r{k}"),
                json!({
                    "text_only": text, "dense_whole": dw, "dense_128_96": d1,
                    "oracle_union_whole": uw, "oracle_union_128_96": u1,
                    "union_headroom_128_96_over_text": u1 - text,
                    "rescue_queries_128_96": rescue,
                }),
            );
        }
        serde_json::Value::Object(rows)
    } else {
        serde_json::Value::Null
    };

    // ── Write report. ── Full mode writes a separate file so the directional
    // small-corpus artifact is preserved.
    let out_name = if full_mode {
        "IR-C-ws1-fusion-experiment-full.json"
    } else {
        "IR-C-ws1-fusion-experiment.json"
    };
    let out = root.join("dev/plans/runs").join(out_name);
    let comment = if full_mode {
        "IR-C WS1 fusion experiment — FULL CORPUS. Harness-side weighted RRF at \
         production k=30 over the full gold pool + full corpus; the Option-A deep-K \
         (R@20/R@50) exploratory question: does 128/96 max-pool passage fan-out beat \
         the whole-doc dense arm, and does it lift the hybrid at 3:1 vs 1:1."
    } else {
        "IR-C WS1 fusion experiment. Harness-side weighted RRF sweep over a sampled \
         exact_fact+exploratory slice; tests whether ordering the text arm by bm25() \
         relevance (vs production write_cursor) and weighting it lifts recall. \
         Small-corpus (directional)."
    };
    let doc = json!({
        "_comment": comment,
        "full_corpus": full_mode,
        "docs_seeded": docs.len(),
        "eval_queries": gold.queries.len(),
        "k_ladder": K_LADDER,
        "configs": serde_json::Value::Object(report),
        "complementarity_exploratory": comp,
    });
    std::fs::write(&out, serde_json::to_string_pretty(&doc).unwrap()).expect("write report");
    eprintln!("FX_WROTE {}", out.display());

    // ── Sanity: the harness anchor must track the engine's real hybrid. ──
    // (Both 'hybrid_current(anchor)' and 'hybrid_wcursor_equal' replicate the
    // production arm ordering; they should land close on exact_fact R@10.)
    eprintln!("FX_NOTE anchor=engine_hybrid vs harness wcursor_equal validate the fusion fidelity");
}
