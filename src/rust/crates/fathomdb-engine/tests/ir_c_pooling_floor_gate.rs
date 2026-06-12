//! IR-C — Phase 0 pooling change × the 1-bit binary recall-floor GATE.
//!
//! `dev/notes/IR-C-embedder-options-research.md`: bge-small-en-v1.5 is CLS-pooled
//! but FathomDB mean-pools it. Switching to CLS may fix retrieval relevance, BUT
//! pooling changes the embedding-space geometry that sign-bit (1-bit) quantization
//! is sensitive to — so CLS must clear the SAME binary recall@10 floor (≥0.90,
//! AC-013/eu7) that mean-pool does, or it can't be adopted as-is.
//!
//! This harness measures that floor for BOTH pooling modes in a single embed pass
//! (via `CandleBgeEmbedder::embed_dual_for_test` — one forward → both vectors),
//! faithfully to the production vector stage: mean-center → sign-bit quantize →
//! Hamming top-K=192 candidates → f32 (centered) cosine rerank → top-10, scored as
//! recall@10 vs the exact-f32 (centered) cosine top-10 ground truth.
//!
//! Gated `IRC_RUN=1` + `--features default-embedder`; skips without the corpus.
//! Pure quant/recall helpers below run in the DEFAULT `cargo test` pass.

#[path = "support/corpus_subset.rs"]
mod corpus_subset;
#[path = "support/ir_eval.rs"]
mod ir_eval;

// ── Pure pipeline helpers (unit-tested, no feature needed) ──────────────────

/// Pack sign bits: bit i = 1 iff v[i] >= 0 (production sign-bit convention).
fn sign_bits(v: &[f32]) -> Vec<u64> {
    let mut bits = vec![0u64; v.len().div_ceil(64)];
    for (i, &x) in v.iter().enumerate() {
        if x >= 0.0 {
            bits[i / 64] |= 1u64 << (i % 64);
        }
    }
    bits
}

fn hamming(a: &[u64], b: &[u64]) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

fn sub(a: &[f32], m: &[f32]) -> Vec<f32> {
    a.iter().zip(m).map(|(x, y)| x - y).collect()
}

fn mean_vec(vs: &[Vec<f32>]) -> Vec<f32> {
    let d = vs.first().map(|v| v.len()).unwrap_or(0);
    let mut m = vec![0.0f64; d];
    for v in vs {
        for (i, &x) in v.iter().enumerate() {
            m[i] += x as f64;
        }
    }
    let n = vs.len().max(1) as f64;
    m.into_iter().map(|x| (x / n) as f32).collect()
}

/// Indices of the top-`k` by `score` (descending).
fn topk_desc(n: usize, k: usize, score: impl Fn(usize) -> f32) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| score(b).partial_cmp(&score(a)).unwrap_or(std::cmp::Ordering::Equal));
    idx.truncate(k);
    idx
}

/// recall@k = |truth ∩ got| / |truth|.
fn recall_at_k(truth: &[usize], got: &[usize]) -> f64 {
    if truth.is_empty() {
        return 1.0;
    }
    let g: std::collections::HashSet<usize> = got.iter().copied().collect();
    truth.iter().filter(|t| g.contains(t)).count() as f64 / truth.len() as f64
}

/// The vector-stage recall@10 floor for one set of (already-pooled) doc + query
/// vectors: center by the doc mean, then for each query compare the quantized
/// pipeline's top-`k_eval` to the exact-f32 top-`k_eval`.
fn measure_floor(
    docs: &[Vec<f32>],
    queries: &[Vec<f32>],
    k_cand: usize,
    k_eval: usize,
) -> (f64, f64) {
    let mean = mean_vec(docs);
    let cdocs: Vec<Vec<f32>> = docs.iter().map(|d| sub(d, &mean)).collect();
    let dbits: Vec<Vec<u64>> = cdocs.iter().map(|d| sign_bits(d)).collect();
    let n = cdocs.len();

    let mut sum = 0.0;
    let mut min = 1.0f64;
    for q in queries {
        let cq = sub(q, &mean);
        // Ground truth: exact f32 cosine top-k_eval.
        let gt = topk_desc(n, k_eval, |i| cosine(&cq, &cdocs[i]));
        // Quantized: Hamming top-k_cand (ascending dist = descending -dist), then
        // f32 cosine rerank to top-k_eval.
        let qbits = sign_bits(&cq);
        let cand = topk_desc(n, k_cand, |i| -(hamming(&qbits, &dbits[i]) as f32));
        let got = {
            let mut c = cand;
            c.sort_by(|&a, &b| {
                cosine(&cq, &cdocs[b])
                    .partial_cmp(&cosine(&cq, &cdocs[a]))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            c.truncate(k_eval);
            c
        };
        let r = recall_at_k(&gt, &got);
        sum += r;
        min = min.min(r);
    }
    (sum / queries.len().max(1) as f64, min)
}

// ── The gate run (IRC_RUN + default-embedder; skips without corpus) ─────────

#[cfg(feature = "default-embedder")]
#[test]
fn ir_c_pooling_floor_gate() {
    use corpus_subset::{load_subset_or_skip, repo_root};
    use fathomdb_embedder::CandleBgeEmbedder;
    use ir_eval::load_gold_set;

    fn env_usize(key: &str, default: usize) -> usize {
        std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    if std::env::var_os("IRC_RUN").is_none() {
        eprintln!("[skip] IRC_RUN not set; pooling floor gate is opt-in");
        return;
    }
    let Some(root) = repo_root() else {
        eprintln!("[skip] repo_root() not found");
        return;
    };
    let max_docs = env_usize("IRC_GATE_MAXDOCS", usize::MAX);
    let n_queries = env_usize("IRC_GATE_QUERIES", 200);
    const K_CAND: usize = 192; // production rerank fanout
    const K_EVAL: usize = 10;
    const FLOOR: f64 = 0.90;

    let Some(mut docs) = load_subset_or_skip(usize::MAX) else { return };
    if docs.len() > max_docs {
        docs.truncate(max_docs);
    }
    // Query set: a deterministic stride sample of gold query TEXTS (real queries,
    // not corpus bodies, so there is no self-match in the ground truth).
    let gold_path = root.join("data/corpus-data/eval/ir_gold/all.gold.json");
    let query_texts: Vec<String> = match gold_path.exists() {
        true => {
            let g = load_gold_set(&gold_path).expect("load gold");
            let pool: Vec<&ir_eval::GoldQuery> = g
                .queries
                .iter()
                .filter(|q| q.query_class != ir_eval::QueryClass::Negative)
                .collect();
            let take = n_queries.min(pool.len());
            (0..take).map(|i| pool[i * pool.len() / take].query.clone()).collect()
        }
        false => {
            eprintln!("[skip] gold absent");
            return;
        }
    };
    eprintln!("GATE_SETUP docs={} queries={} k_cand={K_CAND}", docs.len(), query_texts.len());

    let emb = CandleBgeEmbedder::new().expect("bge embedder");
    let t0 = std::time::Instant::now();
    // ONE forward per text → both pooled vectors.
    let mut mean_docs = Vec::with_capacity(docs.len());
    let mut cls_docs = Vec::with_capacity(docs.len());
    for (i, d) in docs.iter().enumerate() {
        let (m, c) = emb.embed_dual_for_test(&d.body).expect("embed doc");
        mean_docs.push(m);
        cls_docs.push(c);
        if (i + 1) % 2000 == 0 {
            eprintln!(
                "GATE_PROGRESS {}/{} docs ({:.0}s)",
                i + 1,
                docs.len(),
                t0.elapsed().as_secs_f64()
            );
        }
    }
    let (mut mean_q, mut cls_q) = (Vec::new(), Vec::new());
    for t in &query_texts {
        let (m, c) = emb.embed_dual_for_test(t).expect("embed query");
        mean_q.push(m);
        cls_q.push(c);
    }
    eprintln!(
        "GATE_EMBEDDED {} docs + {} queries in {:.0}s",
        docs.len(),
        query_texts.len(),
        t0.elapsed().as_secs_f64()
    );

    let (mean_r, mean_min) = measure_floor(&mean_docs, &mean_q, K_CAND, K_EVAL);
    let (cls_r, cls_min) = measure_floor(&cls_docs, &cls_q, K_CAND, K_EVAL);

    eprintln!("GATE_RESULT floor={FLOOR}");
    eprintln!(
        "GATE_RESULT pooling=mean recall@10={mean_r:.4} (min/query {mean_min:.3}) {}",
        if mean_r >= FLOOR { "PASS" } else { "BELOW" }
    );
    eprintln!(
        "GATE_RESULT pooling=cls  recall@10={cls_r:.4} (min/query {cls_min:.3}) {}",
        if cls_r >= FLOOR { "PASS" } else { "BELOW" }
    );

    let out = root.join("dev/plans/runs/IR-C-pooling-floor-gate.json");
    let doc = serde_json::json!({
        "_comment": "IR-C Phase 0 pooling × 1-bit binary recall@10 floor gate. \
                     Faithful vector stage: mean-center -> sign-bit -> Hamming K=192 \
                     -> f32 rerank vs exact-f32 top-10. CLS must clear the same 0.90 \
                     floor as mean to be adoptable.",
        "n_docs": docs.len(), "n_queries": query_texts.len(),
        "k_cand": K_CAND, "k_eval": K_EVAL, "floor": FLOOR,
        "mean_pool": {"recall_at_10": mean_r, "min_per_query": mean_min, "pass": mean_r >= FLOOR},
        "cls_pool":  {"recall_at_10": cls_r,  "min_per_query": cls_min,  "pass": cls_r >= FLOOR},
    });
    std::fs::write(&out, serde_json::to_string_pretty(&doc).unwrap()).expect("write gate report");
    eprintln!("GATE_WROTE {}", out.display());
}

// ── Unit tests (default pass: no feature, no corpus) ────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_bits_and_hamming() {
        let a = sign_bits(&[0.5, -0.5, 0.0, -1.0]); // bits: 1,0,1,0
        let b = sign_bits(&[0.5, 0.5, 0.0, -1.0]); // bits: 1,1,1,0
        assert_eq!(hamming(&a, &a), 0);
        assert_eq!(hamming(&a, &b), 1); // differ at index 1
    }

    #[test]
    fn cosine_basic() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn recall_and_topk() {
        let scores = [0.1f32, 0.9, 0.5, 0.8];
        assert_eq!(topk_desc(4, 2, |i| scores[i]), vec![1, 3]); // 0.9, 0.8
        assert_eq!(recall_at_k(&[1, 3], &[1, 3]), 1.0);
        assert_eq!(recall_at_k(&[1, 3], &[1, 2]), 0.5);
        assert_eq!(recall_at_k(&[1, 3], &[2, 0]), 0.0);
    }

    #[test]
    fn floor_is_perfect_when_quant_separates_clusters() {
        // Two well-separated clusters → sign-bits + cosine agree perfectly, so the
        // quantized top-k matches the exact-f32 top-k (recall 1.0). Sanity that the
        // pipeline wiring (center → bits → hamming cand → rerank) is coherent.
        let mut docs = Vec::new();
        for _ in 0..20 {
            docs.push(vec![1.0, 1.0, -1.0, -1.0]);
        }
        for _ in 0..20 {
            docs.push(vec![-1.0, -1.0, 1.0, 1.0]);
        }
        let queries = vec![vec![0.9, 1.1, -1.0, -0.8]]; // clearly cluster A
        let (r, min) = measure_floor(&docs, &queries, 8, 5);
        assert_eq!(r, 1.0);
        assert_eq!(min, 1.0);
    }
}
