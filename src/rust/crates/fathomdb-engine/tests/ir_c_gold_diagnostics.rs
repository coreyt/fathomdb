//! IR-C — gold test-query-quality diagnostics: the LEXICAL tier (WI-1L).
//!
//! Plan: `dev/plans/IR-C-test-query-quality-instrumentation-plan.md`. Quantifies
//! how lexically *easy* each gold query is — i.e. how much of the "hybrid is
//! lexical-bound" result is a property of the test set vs the retriever — using
//! a MODEL-FREE pass over the frozen corpus (seed + FTS5 `bm25()`, NO embedding),
//! so it is cheap and CI-able. The model-dependent DENSE tier (dense_gold_rank,
//! lexical/semantic/hard buckets, span-overlap) is WI-1D and lands separately.
//!
//! Per positive query it records:
//!   - `bm25_gold_rank` — rank of the best gold doc under content-OR + `bm25()`
//!     over the whole corpus (`null` past the scan cap). rank 1 ⇒ trivially lexical.
//!   - `idf_overlap`    — IDF-weighted query∩gold-doc content-token coverage
//!     (raw coverage saturates on long docs; IDF-weight measures the *rare* terms).
//!   - `gold_doc_tokens`, `gold_locator_kind` — difficulty context.
//!
//! Writes the `lexical` section of `…/ir_gold/all.gold.diagnostics.json`, pinned
//! to `corpus_hash` (refuses on snapshot mismatch). The retrieval seams are the
//! SAME ones the fusion experiment uses (`support/ir_retrieval.rs`) so ranks are
//! comparable.
//!
//! The full-corpus run is gated `IRC_RUN=1` and skips when the (gitignored) corpus
//! is absent; the unit tests below run in the DEFAULT `cargo test` pass (no
//! feature, no corpus) and pin the metric math.

#[path = "support/corpus_subset.rs"]
mod corpus_subset;
#[path = "support/ir_eval.rs"]
mod ir_eval;
#[path = "support/ir_retrieval.rs"]
mod ir_retrieval;

use std::collections::{BTreeSet, HashMap, HashSet};

use corpus_subset::{ingest, load_subset_or_skip, repo_root, Doc, VECTOR_KIND};
use fathomdb_engine::PreparedWrite;
use ir_eval::{
    load_gold_set, required_doc_ids, validate_gold_set, QueryClass, UNPINNED_PLACEHOLDER,
};
use ir_retrieval::{compile_content_or, content_tokens, fts_bodies, map_bodies, tokenize_set};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

/// How deep the content-OR `bm25()` scan goes before a gold doc counts as
/// "unranked" (`bm25_gold_rank = null`). Deep enough that anything past it is
/// lexically hopeless regardless of the exact value.
const RANK_CAP: usize = 1000;

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

// ── Pure metric helpers (unit-tested below) ─────────────────────────────────

/// 1-based rank of the best (earliest) gold doc in a ranked doc_id list; `None`
/// if no gold doc appears.
fn bm25_gold_rank(ranked: &[String], gold: &BTreeSet<String>) -> Option<usize> {
    ranked.iter().position(|d| gold.contains(d)).map(|p| p + 1)
}

/// BM25 IDF of a term: `ln((N − df + 0.5)/(df + 0.5) + 1)`. Rarer ⇒ higher.
fn idf(df: usize, n_docs: usize) -> f64 {
    let n = n_docs as f64;
    let df = df as f64;
    ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
}

/// IDF-weighted coverage of the query's content tokens by the gold doc:
/// `Σ_{t∈Q∩D} idf(t) / Σ_{t∈Q} idf(t)` ∈ [0,1]. Empty query ⇒ 0.0.
fn idf_overlap(
    q_tokens: &HashSet<String>,
    doc_tokens: &HashSet<String>,
    df: &HashMap<String, usize>,
    n_docs: usize,
) -> f64 {
    let idf_of = |t: &str| idf(df.get(t).copied().unwrap_or(0), n_docs);
    let denom: f64 = q_tokens.iter().map(|t| idf_of(t)).sum();
    if denom <= 0.0 {
        return 0.0;
    }
    let numer: f64 =
        q_tokens.iter().filter(|t| doc_tokens.contains(t.as_str())).map(|t| idf_of(t)).sum();
    numer / denom
}

/// Guard: the gold's pin must match the frozen snapshot and not be the
/// fixture placeholder — otherwise we'd diagnose a corpus the gold wasn't built
/// against. Returns the validated hash to stamp into the sidecar.
fn check_corpus_hash(gold_hash: &str, snapshot_hash: &str) -> Result<String, String> {
    if gold_hash.trim().is_empty() {
        return Err("gold corpus_hash is empty".into());
    }
    if gold_hash == UNPINNED_PLACEHOLDER {
        return Err("gold corpus_hash is the unpinned fixture placeholder".into());
    }
    if snapshot_hash.trim().is_empty() {
        return Err("snapshot corpus_hash is empty".into());
    }
    if gold_hash != snapshot_hash {
        return Err(format!("gold corpus_hash {gold_hash} != snapshot {snapshot_hash}"));
    }
    Ok(gold_hash.to_string())
}

// ── Sidecar assembly (unit-tested for shape) ────────────────────────────────

struct LexRecord {
    query_id: String,
    query_class: String,
    source: Option<String>,
    bm25_gold_rank: Option<usize>,
    idf_overlap: f64,
    gold_doc_tokens: usize,
    gold_locator_kind: String,
}

fn median(mut v: Vec<usize>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] as f64 } else { (v[n / 2 - 1] + v[n / 2]) as f64 / 2.0 })
}

/// Per-class + overall summary over a slice of records.
fn summarize(recs: &[&LexRecord]) -> Value {
    let n = recs.len();
    if n == 0 {
        return json!({ "n": 0 });
    }
    let rank1 = recs.iter().filter(|r| r.bm25_gold_rank == Some(1)).count();
    let found: Vec<usize> = recs.iter().filter_map(|r| r.bm25_gold_rank).collect();
    let mean_overlap: f64 = recs.iter().map(|r| r.idf_overlap).sum::<f64>() / n as f64;
    json!({
        "n": n,
        "bm25_rank1_frac": rank1 as f64 / n as f64,
        "bm25_found_frac": found.len() as f64 / n as f64,
        "median_bm25_gold_rank": median(found),
        "mean_idf_overlap": (mean_overlap * 1e4).round() / 1e4,
    })
}

fn build_lexical_sidecar(
    corpus_hash: &str,
    qrels_version: &str,
    scope: &str,
    n_docs: usize,
    recs: &[LexRecord],
) -> Value {
    let per_query: serde_json::Map<String, Value> = recs
        .iter()
        .map(|r| {
            (
                r.query_id.clone(),
                json!({
                    "query_class": r.query_class,
                    "source": r.source,
                    "bm25_gold_rank": r.bm25_gold_rank,
                    "idf_overlap": (r.idf_overlap * 1e4).round() / 1e4,
                    "gold_doc_tokens": r.gold_doc_tokens,
                    "gold_locator_kind": r.gold_locator_kind,
                }),
            )
        })
        .collect();

    // Overall + per-class summaries.
    let mut summary = serde_json::Map::new();
    summary.insert("overall".into(), summarize(&recs.iter().collect::<Vec<_>>()));
    let mut by_class: serde_json::Map<String, Value> = serde_json::Map::new();
    for class in ["exact_fact", "exploratory"] {
        let sel: Vec<&LexRecord> = recs.iter().filter(|r| r.query_class == class).collect();
        if !sel.is_empty() {
            by_class.insert(class.into(), summarize(&sel));
        }
    }
    summary.insert("per_class".into(), Value::Object(by_class));

    json!({
        "corpus_hash": corpus_hash,
        "qrels_version": qrels_version,
        "_comment": "IR-C gold diagnostics — LEXICAL tier (model-free, full-corpus). \
                     bm25_gold_rank/idf_overlap measure per-query lexical easiness; the \
                     dense tier (buckets, span overlap) is WI-1D. Pinned to corpus_hash.",
        "lexical": {
            "scope": scope,
            "n_docs": n_docs,
            "summary": Value::Object(summary),
            "per_query": Value::Object(per_query),
        },
    })
}

// ── Dense tier (WI-1D/WI-3b): ranks, buckets, passage↔span overlap ──────────
// Pure helpers (unit-tested); the embedding loop that feeds them is feature-gated.

/// Classify a query by which arm can find its gold doc within `cap`:
/// `lexical` (bm25 reaches it), else `semantic` (only the chunked dense arm
/// reaches it — the stratum that justifies a vector arm), else `hard`.
fn bucket(bm25_rank: Option<usize>, dense_rank: Option<usize>, cap: usize) -> &'static str {
    let within = |r: Option<usize>| matches!(r, Some(x) if x <= cap);
    if within(bm25_rank) {
        "lexical"
    } else if within(dense_rank) {
        "semantic"
    } else {
        "hard"
    }
}

/// Char-span IoU of two `[start, end)` byte ranges. Disjoint ⇒ 0.0.
fn span_iou(a: (usize, usize), b: (usize, usize)) -> f64 {
    let inter = a.1.min(b.1).saturating_sub(a.0.max(b.0)) as f64;
    let la = a.1.saturating_sub(a.0) as f64;
    let lb = b.1.saturating_sub(b.0) as f64;
    let union = la + lb - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Max-pool passage→doc ranking that also tracks each doc's best-scoring
/// passage span (for the span-overlap metric). Returns `(doc_id, best_span)` in
/// descending doc-score order. Same max-pool the experiment's `v_*_max` uses.
fn maxpool_ranking(
    qv: &[f32],
    passages: &[(String, usize, usize, Vec<f32>)],
) -> Vec<(String, (usize, usize))> {
    struct B {
        score: f32,
        span: (usize, usize),
    }
    let mut by: HashMap<&str, B> = HashMap::new();
    for (doc, s, e, v) in passages {
        let dot: f32 = qv.iter().zip(v).map(|(a, b)| a * b).sum();
        let entry = by.entry(doc.as_str()).or_insert(B { score: f32::MIN, span: (0, 0) });
        if dot > entry.score {
            entry.score = dot;
            entry.span = (*s, *e);
        }
    }
    let mut v: Vec<(&str, f32, (usize, usize))> =
        by.into_iter().map(|(d, b)| (d, b.score, b.span)).collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    v.into_iter().map(|(d, _, sp)| (d.to_string(), sp)).collect()
}

/// Rank (1-based) of the best gold doc in a doc ranking, plus that doc's best
/// passage span. `(None, None)` if no gold doc appears.
fn dense_gold_rank_and_span(
    ranking: &[(String, (usize, usize))],
    gold: &BTreeSet<String>,
) -> (Option<usize>, Option<(usize, usize)>) {
    for (i, (doc, span)) in ranking.iter().enumerate() {
        if gold.contains(doc) {
            return (Some(i + 1), Some(*span));
        }
    }
    (None, None)
}

struct DenseRecord {
    query_id: String,
    dense_gold_rank_whole: Option<usize>,
    dense_gold_rank_128_96: Option<usize>,
    bucket: &'static str,
    passage_evidence_iou: Option<f64>,
}

fn build_dense_section(
    identity: &str,
    scope: &str,
    bucket_cap: usize,
    recs: &[DenseRecord],
) -> Value {
    let per_query: serde_json::Map<String, Value> = recs
        .iter()
        .map(|r| {
            (
                r.query_id.clone(),
                json!({
                    "dense_gold_rank_whole": r.dense_gold_rank_whole,
                    "dense_gold_rank_128_96": r.dense_gold_rank_128_96,
                    "bucket": r.bucket,
                    "passage_evidence_iou": r.passage_evidence_iou.map(|x| (x * 1e4).round() / 1e4),
                }),
            )
        })
        .collect();
    let count = |b: &str| recs.iter().filter(|r| r.bucket == b).count();
    let ious: Vec<f64> = recs.iter().filter_map(|r| r.passage_evidence_iou).collect();
    let mean_iou = if ious.is_empty() {
        Value::Null
    } else {
        json!((ious.iter().sum::<f64>() / ious.len() as f64 * 1e4).round() / 1e4)
    };
    json!({
        "embedder_identity": identity,
        "scope": scope,
        "bucket_cap": bucket_cap,
        "summary": {
            "n": recs.len(),
            "bucket_counts": {
                "lexical": count("lexical"),
                "semantic": count("semantic"),
                "hard": count("hard"),
            },
            "span_locator_queries": ious.len(),
            "mean_passage_evidence_iou": mean_iou,
        },
        "per_query": Value::Object(per_query),
    })
}

/// Embed whole + 128/96 passages with the real BGE model and compute the dense
/// section: per query, `dense_gold_rank_{whole,128_96}`, the lexical/semantic/
/// hard `bucket` (using the lexical bm25 ranks), and — for span-locator queries
/// — the IoU of the gold doc's best passage with the evidence span. Tagged with
/// the embedder identity + scope (model-dependent, NOT pinned to corpus_hash).
#[cfg(feature = "default-embedder")]
fn compute_dense_section(
    docs: &[Doc],
    gold: &ir_eval::GoldSet,
    bm25_rank_by_qid: &HashMap<String, Option<usize>>,
    bucket_cap: usize,
    scope: &str,
) -> Value {
    use fathomdb_embedder::{CandleBgeEmbedder, NomicEmbedder, Pooling};
    use fathomdb_embedder_api::Embedder;
    use ir_retrieval::chunk_words_offsets;

    // Model A/B. Default = bge (with IRC_DIAG_POOLING=cls / IRC_DIAG_PREFIX knobs).
    // IRC_DIAG_MODEL=nomic uses nomic-embed-text-v1.5 (768-d), which REQUIRES task
    // prefixes: "search_document: " on passages, "search_query: " on queries.
    let model = std::env::var("IRC_DIAG_MODEL").unwrap_or_else(|_| "bge".to_string());
    let (emb, passage_prefix, query_prefix): (Box<dyn Embedder>, String, String) = if model
        == "nomic"
    {
        let dir = std::path::PathBuf::from(
            std::env::var("IRC_NOMIC_DIR")
                .unwrap_or_else(|_| "/root/.cache/fathomdb/embedders/nomic-v1.5".to_string()),
        );
        let e = NomicEmbedder::from_dir(&dir).expect("load nomic");
        (Box::new(e), "search_document: ".to_string(), "search_query: ".to_string())
    } else {
        let pooling = if std::env::var("IRC_DIAG_POOLING").as_deref() == Ok("cls") {
            Pooling::Cls
        } else {
            Pooling::Mean
        };
        const BGE_QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";
        let qp = if std::env::var_os("IRC_DIAG_PREFIX").is_some() {
            BGE_QUERY_PREFIX.to_string()
        } else {
            String::new()
        };
        (
            Box::new(CandleBgeEmbedder::new().expect("bge embedder").with_pooling(pooling)),
            String::new(),
            qp,
        )
    };
    let identity = format!("{:?}", emb.identity());
    // Prepend a prefix only when non-empty (avoids a useless alloc for bare bge).
    let pfx = |prefix: &str, text: &str| -> std::borrow::Cow<'static, str> {
        if prefix.is_empty() {
            std::borrow::Cow::Owned(text.to_string())
        } else {
            std::borrow::Cow::Owned(format!("{prefix}{text}"))
        }
    };
    eprintln!("DIAG_DENSE model={model} identity={identity} q_prefix={query_prefix:?}");
    let t0 = std::time::Instant::now();
    // The bucket (the headline) needs only the 128/96 dense rank;
    // dense_gold_rank_whole is a reference field. IRC_DIAG_SKIP_WHOLE drops the
    // whole-doc geometry — by far the slowest part (full 512-token passes on the
    // ~6k long docs) — to fit a window. The persistent-box run keeps it (default).
    let skip_whole = std::env::var_os("IRC_DIAG_SKIP_WHOLE").is_some();
    eprintln!(
        "DIAG_DENSE embedding {}128/96 passages over {} docs…",
        if skip_whole { "" } else { "whole + " },
        docs.len()
    );
    let mut whole: Vec<(String, usize, usize, Vec<f32>)> = Vec::new();
    if !skip_whole {
        whole.reserve(docs.len());
        for (i, d) in docs.iter().enumerate() {
            whole.push((
                d.doc_id.clone(),
                0usize,
                d.body.len(),
                emb.embed(&pfx(&passage_prefix, &d.body)).expect("embed whole"),
            ));
            if (i + 1) % 2000 == 0 {
                eprintln!(
                    "DIAG_DENSE whole_progress {}/{} ({:.0}s)",
                    i + 1,
                    docs.len(),
                    t0.elapsed().as_secs_f64()
                );
            }
        }
    }
    let mut p128: Vec<(String, usize, usize, Vec<f32>)> = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        for (t, s, e) in chunk_words_offsets(&d.body, 128, 96, 8) {
            p128.push((
                d.doc_id.clone(),
                s,
                e,
                emb.embed(&pfx(&passage_prefix, &t)).expect("embed chunk"),
            ));
        }
        if (i + 1) % 2000 == 0 {
            eprintln!(
                "DIAG_DENSE p128_progress {}/{} docs, {} passages ({:.0}s)",
                i + 1,
                docs.len(),
                p128.len(),
                t0.elapsed().as_secs_f64()
            );
        }
    }
    eprintln!(
        "DIAG_DENSE embedded whole={} p128={} in {:.0}s",
        whole.len(),
        p128.len(),
        t0.elapsed().as_secs_f64()
    );

    let present: HashSet<&str> = docs.iter().map(|d| d.doc_id.as_str()).collect();
    eprintln!("DIAG_DENSE ranking {} queries…", gold.queries.len());
    let mut recs: Vec<DenseRecord> = Vec::new();
    for q in &gold.queries {
        if q.query_class == QueryClass::Negative {
            continue;
        }
        if recs.len() % 1000 == 0 && recs.len() > 0 {
            eprintln!(
                "DIAG_DENSE rank_progress {} queries ({:.0}s)",
                recs.len(),
                t0.elapsed().as_secs_f64()
            );
        }
        let gold_ids = required_doc_ids(q);
        if gold_ids.is_empty() || !gold_ids.iter().any(|d| present.contains(d.as_str())) {
            continue;
        }
        let qid = q.query_id.clone().unwrap_or_else(|| q.query.clone());
        let qv = emb.embed(&pfx(&query_prefix, &q.query)).expect("embed query");
        let rank_whole = dense_gold_rank_and_span(&maxpool_ranking(&qv, &whole), &gold_ids).0;
        let (rank_128, best_span) =
            dense_gold_rank_and_span(&maxpool_ranking(&qv, &p128), &gold_ids);
        let bm = bm25_rank_by_qid.get(&qid).copied().flatten();
        let bkt = bucket(bm, rank_128, bucket_cap);
        // IoU only when the gold doc carries an evidence span (WI-3a).
        let iou = best_span.and_then(|ps| {
            let spans: Vec<(usize, usize)> = q
                .required_evidence
                .iter()
                .filter_map(|e| e.locator.as_ref())
                .flat_map(|l| l.spans.iter().flatten())
                .map(|s| (s.start, s.end))
                .collect();
            if spans.is_empty() {
                None
            } else {
                Some(spans.iter().map(|&es| span_iou(ps, es)).fold(0.0_f64, f64::max))
            }
        });
        recs.push(DenseRecord {
            query_id: qid,
            dense_gold_rank_whole: rank_whole,
            dense_gold_rank_128_96: rank_128,
            bucket: bkt,
            passage_evidence_iou: iou,
        });
    }
    let mut section = build_dense_section(&identity, scope, bucket_cap, &recs);
    section["model"] = json!(model);
    section["identity"] = json!(identity);
    section["passage_prefix"] = json!(passage_prefix);
    section["query_prefix"] = json!(query_prefix);
    section
}

// ── Full-corpus run (IRC_RUN-gated; skips without the corpus) ────────────────

#[test]
fn ir_c_gold_diagnostics() {
    if std::env::var_os("IRC_RUN").is_none() {
        eprintln!("[skip] IRC_RUN not set; IR-C gold diagnostics is opt-in");
        return;
    }
    let Some(root) = repo_root() else {
        eprintln!("[skip] repo_root() not found");
        return;
    };
    let gold_path = root.join("data/corpus-data/eval/ir_gold/all.gold.json");
    if !gold_path.exists() {
        eprintln!("[skip] {} absent (gitignored)", gold_path.display());
        return;
    }
    let gold = load_gold_set(&gold_path).expect("load gold");
    assert!(validate_gold_set(&gold).is_empty(), "gold invalid");

    // Pin guard: the gold must agree with the frozen snapshot.
    let snap_path = root.join("tests/corpus/snapshot.json");
    let snapshot_hash = std::fs::read_to_string(&snap_path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("corpus_hash").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    let corpus_hash = match check_corpus_hash(&gold.corpus_hash, &snapshot_hash) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("[skip] corpus_hash guard: {e}");
            return;
        }
    };

    // Whole frozen corpus (per_source = MAX) → bm25 ranks are over all of it.
    let max_docs = env_usize("IRC_DIAG_MAXDOCS", usize::MAX);
    let Some(mut docs) = load_subset_or_skip(usize::MAX) else {
        return; // load_subset_or_skip already emitted SKIP
    };
    if docs.len() > max_docs {
        docs.truncate(max_docs);
    }
    let scope =
        if max_docs == usize::MAX { "full".to_string() } else { format!("slice@{}", docs.len()) };
    eprintln!("DIAG_CORPUS docs={} scope={scope} queries={}", docs.len(), gold.queries.len());

    // Seed FTS only: freeze the vector projection (we never read vectors in the
    // lexical pass) and write nodes WITHOUT draining — the FTS `search_index` is
    // committed synchronously, and draining would block on the frozen scheduler.
    let (dir, engine) = corpus_subset::fixture_engine();
    engine.set_projection_scheduler_frozen_for_test(true);
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
                    source_id: fathomdb_engine::SourceId::new(d.doc_id.clone())
                        .expect("test source id"),
                    logical_id: None,
                    state: fathomdb_engine::InitialState::Active,
                    reason: None,
                    valid_from: None,
                    valid_until: None,
                })
                .collect();
            engine.write(&batch).expect("seed write");
            written += take;
        }
    }
    let db = dir.path().join("corpus.sqlite");
    let conn = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open read-only fts conn");

    // Corpus token stats: per-doc body tokens + document-frequency map.
    let doc_tokens: HashMap<String, HashSet<String>> =
        docs.iter().map(|d| (d.doc_id.clone(), tokenize_set(&d.body))).collect();
    let doc_word_len: HashMap<String, usize> =
        docs.iter().map(|d| (d.doc_id.clone(), d.body.split_whitespace().count())).collect();
    let body_to_doc: HashMap<String, String> =
        docs.iter().map(|d| (d.body.clone(), d.doc_id.clone())).collect();
    let mut df: HashMap<String, usize> = HashMap::new();
    for toks in doc_tokens.values() {
        for t in toks {
            *df.entry(t.clone()).or_insert(0) += 1;
        }
    }
    let n_docs = docs.len();
    let present: HashSet<&str> = docs.iter().map(|d| d.doc_id.as_str()).collect();

    let mut recs: Vec<LexRecord> = Vec::new();
    for q in &gold.queries {
        if q.query_class == QueryClass::Negative {
            continue; // lexical-easiness is a positive-query notion
        }
        let gold_ids = required_doc_ids(q);
        // Skip queries whose evidence isn't in the seeded corpus.
        if gold_ids.is_empty() || !gold_ids.iter().any(|d| present.contains(d.as_str())) {
            continue;
        }
        let query_id = q.query_id.clone().unwrap_or_else(|| q.query.clone());
        let ranked = map_bodies(
            &fts_bodies(&conn, &compile_content_or(&q.query), "bm25(search_index)", RANK_CAP),
            &body_to_doc,
        );
        let rank = bm25_gold_rank(&ranked, &gold_ids);

        // Pick the gold doc to characterise: the one bm25 actually surfaced, else
        // the first present required doc (deterministic via the BTreeSet order).
        let gold_doc = ranked
            .iter()
            .find(|d| gold_ids.contains(*d))
            .cloned()
            .or_else(|| gold_ids.iter().find(|d| present.contains(d.as_str())).cloned())
            .expect("a present gold doc");
        let q_tokens = content_tokens(&q.query);
        let empty = HashSet::new();
        let overlap =
            idf_overlap(&q_tokens, doc_tokens.get(&gold_doc).unwrap_or(&empty), &df, n_docs);
        let loc_kind = if q
            .required_evidence
            .iter()
            .any(|e| e.locator.as_ref().map(|l| l.kind == "span").unwrap_or(false))
        {
            "span"
        } else {
            "whole_body"
        };

        recs.push(LexRecord {
            query_id,
            query_class: q.query_class.label().to_string(),
            source: q.source.clone(),
            bm25_gold_rank: rank,
            idf_overlap: overlap,
            gold_doc_tokens: doc_word_len.get(&gold_doc).copied().unwrap_or(0),
            gold_locator_kind: loc_kind.to_string(),
        });
    }
    eprintln!("DIAG_RECORDS evaluated={}", recs.len());

    #[allow(unused_mut)] // mutated only under the default-embedder dense path
    let mut sidecar =
        build_lexical_sidecar(&corpus_hash, &gold.qrels_version, &scope, n_docs, &recs);
    // Surface the headline so the run is useful even before the file is read.
    eprintln!("DIAG_SUMMARY {}", serde_json::to_string(&sidecar["lexical"]["summary"]).unwrap());

    // Dense tier (WI-1D/WI-3b): opt-in via IRC_DIAG_DENSE + the embedder feature
    // (it embeds the whole corpus). Merges a scope+identity-tagged `dense` section
    // alongside the lexical one — never clobbers it.
    #[cfg(feature = "default-embedder")]
    if std::env::var_os("IRC_DIAG_DENSE").is_some() {
        let bucket_cap = env_usize("IRC_DIAG_BUCKET_CAP", 50);
        let bm25_rank_by_qid: HashMap<String, Option<usize>> =
            recs.iter().map(|r| (r.query_id.clone(), r.bm25_gold_rank)).collect();
        let dense = compute_dense_section(&docs, &gold, &bm25_rank_by_qid, bucket_cap, &scope);
        eprintln!("DIAG_DENSE_SUMMARY {}", serde_json::to_string(&dense["summary"]).unwrap());
        sidecar.as_object_mut().expect("sidecar object").insert("dense".into(), dense);
    }

    let out = root.join("data/corpus-data/eval/ir_gold/all.gold.diagnostics.json");
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&out, serde_json::to_string_pretty(&sidecar).unwrap()).expect("write sidecar");
    eprintln!("DIAG_WROTE {}", out.display());
}

// ── Unit tests (default pass: no feature, no corpus) ────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    fn gold(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bm25_gold_rank_positions() {
        let ranked = ids(&["x", "g", "y", "g2"]);
        assert_eq!(bm25_gold_rank(&ranked, &gold(&["g"])), Some(2));
        // earliest gold doc wins.
        assert_eq!(bm25_gold_rank(&ranked, &gold(&["g2", "g"])), Some(2));
        // absent ⇒ None.
        assert_eq!(bm25_gold_rank(&ranked, &gold(&["zzz"])), None);
    }

    #[test]
    fn idf_overlap_weights_rare_terms() {
        let n = 100;
        let mut df = HashMap::new();
        df.insert("rare".to_string(), 1); // high idf
        df.insert("common".to_string(), 90); // low idf
        let q: HashSet<String> = ["rare", "common"].iter().map(|s| s.to_string()).collect();
        let doc_rare: HashSet<String> = ["rare"].iter().map(|s| s.to_string()).collect();
        let doc_common: HashSet<String> = ["common"].iter().map(|s| s.to_string()).collect();

        let cover_rare = idf_overlap(&q, &doc_rare, &df, n);
        let cover_common = idf_overlap(&q, &doc_common, &df, n);
        // Covering the rare term is worth far more than covering the common one.
        assert!(cover_rare > 0.5, "rare-term coverage {cover_rare} should dominate");
        assert!(cover_common < 0.5, "common-term coverage {cover_common} should be minor");
        assert!(cover_rare > cover_common);
        // Empty query ⇒ 0, never NaN.
        assert_eq!(idf_overlap(&HashSet::new(), &doc_rare, &df, n), 0.0);
    }

    #[test]
    fn check_corpus_hash_guard() {
        assert!(check_corpus_hash("abc", "abc").is_ok());
        assert!(check_corpus_hash("abc", "def").is_err()); // mismatch
        assert!(check_corpus_hash("", "abc").is_err()); // empty gold
        assert!(check_corpus_hash("abc", "").is_err()); // empty snapshot
        assert!(check_corpus_hash(UNPINNED_PLACEHOLDER, UNPINNED_PLACEHOLDER).is_err());
        // placeholder
    }

    #[test]
    fn lexical_sidecar_shape_and_summary() {
        let recs = vec![
            LexRecord {
                query_id: "qmsum:1".into(),
                query_class: "exploratory".into(),
                source: Some("qmsum".into()),
                bm25_gold_rank: Some(1),
                idf_overlap: 0.9,
                gold_doc_tokens: 5000,
                gold_locator_kind: "whole_body".into(),
            },
            LexRecord {
                query_id: "enronqa:2".into(),
                query_class: "exact_fact".into(),
                source: Some("enronqa".into()),
                bm25_gold_rank: None,
                idf_overlap: 0.1,
                gold_doc_tokens: 200,
                gold_locator_kind: "span".into(),
            },
        ];
        let v = build_lexical_sidecar("hash123", "ir-c-reused-v2", "full", 10506, &recs);
        assert_eq!(v["corpus_hash"], "hash123");
        assert_eq!(v["lexical"]["scope"], "full");
        assert_eq!(v["lexical"]["n_docs"], 10506);
        // per_query keyed by query_id.
        assert_eq!(v["lexical"]["per_query"]["qmsum:1"]["bm25_gold_rank"], 1);
        assert!(v["lexical"]["per_query"]["enronqa:2"]["bm25_gold_rank"].is_null());
        // overall: 1 of 2 at rank 1, 1 of 2 found.
        assert_eq!(v["lexical"]["summary"]["overall"]["n"], 2);
        assert_eq!(v["lexical"]["summary"]["overall"]["bm25_rank1_frac"], 0.5);
        assert_eq!(v["lexical"]["summary"]["overall"]["bm25_found_frac"], 0.5);
        // per-class split present.
        assert_eq!(v["lexical"]["summary"]["per_class"]["exploratory"]["n"], 1);
        assert_eq!(v["lexical"]["summary"]["per_class"]["exact_fact"]["n"], 1);
    }

    #[test]
    fn bm25_ranks_lexical_match_first() {
        // FTS integration: the content-overlapping doc must rank first under
        // content-OR + bm25(); a no-overlap query finds nothing.
        let docs = vec![
            doc("d-A", "the quarterly budget review meeting decisions and approvals"),
            doc("d-B", "unrelated gardening tips composting and seasonal recipes"),
            doc("d-C", "weather forecast clouds rain and afternoon sunshine"),
        ];
        let (_dir, conn) = fts_conn(&docs);
        let body_to_doc: HashMap<String, String> =
            docs.iter().map(|d| (d.body.clone(), d.doc_id.clone())).collect();

        let ranked = map_bodies(
            &fts_bodies(
                &conn,
                &compile_content_or("budget review decisions"),
                "bm25(search_index)",
                50,
            ),
            &body_to_doc,
        );
        assert_eq!(bm25_gold_rank(&ranked, &gold(&["d-A"])), Some(1), "lexical match ranks first");

        let none = map_bodies(
            &fts_bodies(
                &conn,
                &compile_content_or("xylophone zeppelin quokka"),
                "bm25(search_index)",
                50,
            ),
            &body_to_doc,
        );
        assert_eq!(bm25_gold_rank(&none, &gold(&["d-A"])), None, "no overlap ⇒ unranked");
    }

    // ── Dense-tier pure helpers (WI-1D/WI-3b) ──

    #[test]
    fn bucket_rule_lexical_semantic_hard() {
        assert_eq!(bucket(Some(3), Some(2), 50), "lexical"); // bm25 within cap wins
        assert_eq!(bucket(Some(60), Some(10), 50), "semantic"); // bm25 too deep, dense reaches
        assert_eq!(bucket(None, Some(5), 50), "semantic");
        assert_eq!(bucket(Some(100), None, 50), "hard");
        assert_eq!(bucket(None, None, 50), "hard");
        assert_eq!(bucket(Some(51), Some(51), 50), "hard"); // both past cap
    }

    #[test]
    fn span_iou_cases() {
        assert_eq!(span_iou((0, 10), (0, 10)), 1.0);
        assert_eq!(span_iou((0, 10), (20, 30)), 0.0); // disjoint
        assert_eq!(span_iou((0, 10), (10, 20)), 0.0); // touching, no overlap
                                                      // [0,10) ∩ [5,15) = 5; ∪ = 15 → 1/3.
        assert!((span_iou((0, 10), (5, 15)) - 5.0 / 15.0).abs() < 1e-9);
    }

    #[test]
    fn maxpool_ranking_ranks_and_tracks_best_span() {
        let q = vec![1.0_f32, 0.0];
        let passages = vec![
            ("A".to_string(), 0usize, 5usize, vec![0.1_f32, 0.9]), // weak A passage
            ("A".to_string(), 5usize, 10usize, vec![0.9_f32, 0.1]), // strong A passage → best span
            ("B".to_string(), 0usize, 4usize, vec![0.5_f32, 0.5]),
        ];
        let ranking = maxpool_ranking(&q, &passages);
        assert_eq!(ranking[0].0, "A", "A's best passage (0.9) outranks B (0.5)");
        assert_eq!(ranking[0].1, (5, 10), "best passage span is tracked");

        let (rank, span) = dense_gold_rank_and_span(&ranking, &gold(&["A"]));
        assert_eq!((rank, span), (Some(1), Some((5, 10))));
        assert_eq!(dense_gold_rank_and_span(&ranking, &gold(&["B"])).0, Some(2));
        assert_eq!(dense_gold_rank_and_span(&ranking, &gold(&["zzz"])), (None, None));
    }

    #[test]
    fn dense_section_shape_scope_and_buckets() {
        let recs = vec![
            DenseRecord {
                query_id: "a".into(),
                dense_gold_rank_whole: Some(14),
                dense_gold_rank_128_96: Some(6),
                bucket: "semantic",
                passage_evidence_iou: Some(0.5),
            },
            DenseRecord {
                query_id: "b".into(),
                dense_gold_rank_whole: Some(2),
                dense_gold_rank_128_96: Some(1),
                bucket: "lexical",
                passage_evidence_iou: None,
            },
        ];
        let v = build_dense_section("bge-small/v1", "slice@1200", 50, &recs);
        assert_eq!(v["scope"], "slice@1200"); // reduced runs never claim "full"
        assert_eq!(v["embedder_identity"], "bge-small/v1");
        assert_eq!(v["bucket_cap"], 50);
        assert_eq!(v["summary"]["bucket_counts"]["semantic"], 1);
        assert_eq!(v["summary"]["bucket_counts"]["lexical"], 1);
        assert_eq!(v["summary"]["bucket_counts"]["hard"], 0);
        assert_eq!(v["summary"]["span_locator_queries"], 1);
        assert_eq!(v["summary"]["mean_passage_evidence_iou"], 0.5);
        assert_eq!(v["per_query"]["a"]["dense_gold_rank_128_96"], 6);
        assert!(v["per_query"]["b"]["passage_evidence_iou"].is_null());
    }

    #[test]
    fn chunk_words_offsets_slice_back_to_text() {
        let body = "alpha beta gamma delta epsilon zeta";
        // Whole-doc ⇒ one chunk spanning the body.
        assert_eq!(
            ir_retrieval::chunk_words_offsets(body, usize::MAX, 1, 1),
            vec![(body.to_string(), 0, body.len())]
        );
        // 2-word windows: each chunk's byte range slices back to its (single-
        // spaced) text, and the text view matches plain chunk_words.
        let chunks = ir_retrieval::chunk_words_offsets(body, 2, 2, 3);
        for (text, s, e) in &chunks {
            assert_eq!(&body[*s..*e], text, "offsets must slice back to the chunk text");
        }
        assert_eq!(chunks[0], ("alpha beta".to_string(), 0, 10));
        let text_only: Vec<String> = chunks.iter().map(|(t, _, _)| t.clone()).collect();
        assert_eq!(text_only, ir_retrieval::chunk_words(body, 2, 2, 3));
    }

    fn doc(doc_id: &str, body: &str) -> Doc {
        Doc {
            doc_id: doc_id.to_string(),
            source_type: "doc".to_string(),
            title: None,
            body: body.to_string(),
            parent_doc_id: None,
            tags: vec![],
            relation_hint: None,
        }
    }

    /// Seed a tiny corpus through the synthetic-embedder engine and hand back a
    /// read-only FTS connection (the engine writes `search_index` synchronously).
    fn fts_conn(docs: &[Doc]) -> (tempfile::TempDir, Connection) {
        let (dir, engine) = corpus_subset::fixture_engine();
        ingest(&engine, docs);
        let db = dir.path().join("corpus.sqlite"); // fixture_engine's db filename
        let conn =
            Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY).expect("ro conn");
        (dir, conn)
    }
}
