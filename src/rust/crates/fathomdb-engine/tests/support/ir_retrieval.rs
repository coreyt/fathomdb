//! Shared harness-side retrieval seams for the IR-C experiments + diagnostics.
//!
//! Extracted VERBATIM from `ir_c_fusion_experiment.rs` so the fusion experiment
//! and the gold-diagnostics harness (`ir_c_gold_diagnostics.rs`) compute the
//! lexical arm (content-OR + FTS5 `bm25()`) and the dense arm (chunked passage
//! KNN) with byte-identical logic — otherwise their ranks would not be
//! comparable. Keeping it in one place is the regression guard: the fusion
//! experiment compiling + passing against these proves the extraction is faithful.
//!
//! Pure / engine-file-only (no embedder dependency), so a lexical-only consumer
//! can use these WITHOUT the `default-embedder` feature.

#![allow(dead_code)] // each includer uses a different subset; cargo lints includes in isolation

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

// ── Text-arm query compilation ──────────────────────────────────────────────

/// Inlined copy of `fathomdb_query::compile_text_query` (not a dev-dependency):
/// whitespace-split, quote each token, AND-join — byte-identical to production.
pub fn compile_match_expression(raw: &str) -> String {
    compile_with_op(raw, " AND ")
}

/// Bag-of-words OR semantics — standard BM25 query handling, where any token may
/// match and `bm25()` ranks by overlap (how the same-dataset EnronQA/QAConv BM25
/// baselines are run; the production AND-join near-zeroes recall on NL questions).
pub fn compile_match_expression_or(raw: &str) -> String {
    compile_with_op(raw, " OR ")
}

pub fn compile_with_op(raw: &str, op: &str) -> String {
    raw.split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(op)
}

/// OR over *content* tokens only — stopwords stripped — to cut the false matches
/// raw-OR picks up on function words. Falls back to raw-OR if the query is all
/// stopwords.
pub fn compile_content_or(raw: &str) -> String {
    let toks = content_tokens(raw);
    if toks.is_empty() {
        return compile_match_expression_or(raw);
    }
    toks.iter().map(|t| format!("\"{t}\"")).collect::<Vec<_>>().join(" OR ")
}

/// Minimal stopword set so content-token coverage isn't inflated by function
/// words (the OR query still matches on them, but `bm25`'s IDF + this coverage
/// guard both discount them).
pub const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "was", "were", "what", "when", "where", "who", "whom", "which",
    "how", "why", "did", "does", "do", "is", "of", "to", "in", "on", "at", "by", "an", "a", "it",
    "its", "this", "that", "these", "those", "with", "from", "as", "be", "or", "if", "about",
    "into", "over", "than", "then", "they", "them", "their", "you", "your", "we", "our", "i",
];

pub fn tokenize_set(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_string())
        .collect()
}

/// Content tokens of a query: tokenized, ≥3 chars, stopwords removed. The
/// coverage denominator (the "M" in N-of-M) and the `idf_overlap` numerator set.
pub fn content_tokens(query: &str) -> HashSet<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    tokenize_set(query).into_iter().filter(|t| !stop.contains(t.as_str())).collect()
}

// ── Dense arm: passage chunking + pooled KNN ────────────────────────────────

/// Byte (start, end) of each whitespace-delimited word in `body`. Lets a chunk
/// report its char span in the ORIGINAL body for the passage↔evidence-span
/// overlap metric (WI-3b).
pub fn word_spans(body: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in body.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                spans.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        spans.push((s, body.len()));
    }
    spans
}

/// Like [`chunk_words`] but each chunk also carries its `(char_start, char_end)`
/// byte range in the original body. The chunk TEXT is still the normalized
/// single-space join (what gets embedded); the offsets index the source body so
/// a passage can be compared to an evidence span. Whole-doc (`size = usize::MAX`
/// or short body) ⇒ one chunk spanning `0..body.len()`.
pub fn chunk_words_offsets(
    body: &str,
    size: usize,
    stride: usize,
    max_chunks: usize,
) -> Vec<(String, usize, usize)> {
    let spans = word_spans(body);
    if spans.len() <= size {
        return vec![(body.to_string(), 0, body.len())];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < spans.len() && chunks.len() < max_chunks {
        let end = (start + size).min(spans.len());
        let text = spans[start..end].iter().map(|&(s, e)| &body[s..e]).collect::<Vec<_>>().join(" ");
        chunks.push((text, spans[start].0, spans[end - 1].1));
        if end == spans.len() {
            break;
        }
        start += stride;
    }
    chunks
}

/// Split a body into overlapping word-window passages (long bodies exceed
/// bge-small's ~512-token window and get mean-pool-diluted). Short bodies pass
/// through as a single chunk; `size = usize::MAX` ⇒ whole-doc (one passage).
/// Text-only view of [`chunk_words_offsets`] (byte-identical to the prior impl).
pub fn chunk_words(body: &str, size: usize, stride: usize, max_chunks: usize) -> Vec<String> {
    chunk_words_offsets(body, size, stride, max_chunks).into_iter().map(|(t, _, _)| t).collect()
}

/// Passage-score aggregation to doc level.
#[derive(Clone, Copy)]
pub enum Pool {
    Max,  // doc scores as its single best passage
    Mean, // average over all the doc's passages (rewards uniform relevance)
    Top2, // average of the doc's two best passages (max/mean compromise)
}

/// KNN over passage vectors, pooled to ranked doc_ids — already in evaluation
/// (doc_id) space. One pass accumulates sum/count/top-2 per doc.
pub fn knn_docs_pool(
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

// ── FTS read seam + body↔doc_id mapping ─────────────────────────────────────

/// Read-only FTS query against the engine's sqlite file, ordered by `order_sql`.
pub fn fts_bodies(conn: &Connection, match_expr: &str, order_sql: &str, cap: usize) -> Vec<String> {
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

/// Map ranked bodies → ranked doc_ids via a body→doc_id table; unmapped bodies
/// are dropped (preserves rank).
pub fn map_bodies(bodies: &[String], m: &HashMap<String, String>) -> Vec<String> {
    bodies.iter().filter_map(|b| m.get(b).cloned()).collect()
}
