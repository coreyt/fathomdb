//! 0.8.7 Slice 10 — GPU vs CPU embed speedup measurement (R-GPU-3/4/5).
//!
//! Re-embeds a deterministic, frozen synthetic corpus and reports wall-clock and
//! throughput. The device is selected exactly as production code does — through
//! `FATHOMDB_EMBED_DEVICE` resolved at `CandleBgeEmbedder::new()` — so this binary
//! exercises the real seam, not a test-only path.
//!
//! Run the two arms and compare (the corpus + ordering are identical across arms):
//!
//! ```text
//! # CPU baseline (default build)
//! cargo run --release --example gpu_speedup --features default-embedder
//! # CUDA arm (GPU box, MAIN/worktree cargo build only — never `maturin develop`)
//! FATHOMDB_EMBED_DEVICE=cuda:0 \
//!   cargo run --release --example gpu_speedup --features embed-cuda
//! ```
//!
//! Env knobs: `GPU_SPEEDUP_DOCS` (corpus size, default 2000),
//! `GPU_SPEEDUP_BATCH` (batch size for the batched arm, default 64).
//!
//! R-GPU-5 (serialization): this is single-threaded and the PR-9 guard already
//! serializes `embed()`; we additionally assert batched vectors equal the
//! single-call vectors (parity) so a GPU kernel cannot silently diverge here.

use std::time::Instant;

use fathomdb_embedder::CandleBgeEmbedder;
use fathomdb_embedder_api::Embedder;

/// Build a deterministic corpus of `n` documents with varied, realistic lengths
/// (short titles → multi-sentence paragraphs) so padding/truncation are exercised.
/// No RNG: a fixed sentence bank indexed by `i` keeps the corpus byte-identical
/// across the CPU and CUDA arms (a fair re-embed of the SAME frozen corpus).
fn build_corpus(n: usize) -> Vec<String> {
    const BANK: &[&str] = &[
        "FathomDB stores documents, vectors, and a small governed graph in one SQLite file.",
        "The default embedder is BAAI bge-small-en-v1.5 at dimension 384, mean-pooled and L2-normalized.",
        "Retrieval fuses a lexical BM25 arm with a dense 1-bit Hamming arm via reciprocal-rank fusion.",
        "A cross-encoder reranker can rescore the fused pool; alpha blends the lexical and dense scores.",
        "Vectors are quantized to one bit at store time, so the query path stays CPU-only and deterministic.",
        "Embedding on a GPU is a build- and eval-time accelerator; the shipped library query path is unchanged.",
        "The embedder is serialized so only one forward pass touches the shared model at a time.",
        "Cross-vendor backends plug in behind the Embedder trait, not through candle's device enum.",
        "A vector-equivalence probe set guards against silent numeric drift when a database moves machines.",
        "Single-stream GPU inference is roughly two orders of magnitude faster than CPU for this model.",
    ];
    (0..n)
        .map(|i| {
            // Vary length deterministically: 1..=5 sentences, rotating the bank.
            let sentences = 1 + (i % 5);
            let mut doc = String::new();
            for s in 0..sentences {
                if s > 0 {
                    doc.push(' ');
                }
                doc.push_str(BANK[(i + s) % BANK.len()]);
            }
            // Make every doc unique so no caching shortcut can apply.
            doc.push_str(&format!(" [doc #{i}]"));
            doc
        })
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    dot // both inputs are L2-normalized → dot == cosine
}

fn main() {
    let docs: usize =
        std::env::var("GPU_SPEEDUP_DOCS").ok().and_then(|s| s.parse().ok()).unwrap_or(2000);
    let batch: usize =
        std::env::var("GPU_SPEEDUP_BATCH").ok().and_then(|s| s.parse().ok()).unwrap_or(64);
    let device = std::env::var("FATHOMDB_EMBED_DEVICE").unwrap_or_else(|_| "cpu (default)".into());

    eprintln!("== gpu_speedup ==");
    eprintln!("FATHOMDB_EMBED_DEVICE = {device}");
    eprintln!("corpus = {docs} docs, batch = {batch}");

    let corpus = build_corpus(docs);
    let refs: Vec<&str> = corpus.iter().map(String::as_str).collect();

    // Model load happens at construction (warms the device); excluded from timing.
    let load_t = Instant::now();
    let embedder = CandleBgeEmbedder::new().expect("construct embedder (weights must be cached)");
    eprintln!("model load: {:.2?}", load_t.elapsed());

    // --- Arm 1: single embed() per doc (the per-document API path) ---
    let single_t = Instant::now();
    let mut single_vecs = Vec::with_capacity(docs);
    for d in &refs {
        single_vecs.push(embedder.embed(d).expect("embed"));
    }
    let single_elapsed = single_t.elapsed();

    // --- Arm 2: embed_batch() in chunks (the bulk re-embed path) ---
    let batch_t = Instant::now();
    let mut batch_vecs = Vec::with_capacity(docs);
    for chunk in refs.chunks(batch) {
        batch_vecs.extend(embedder.embed_batch(chunk).expect("embed_batch"));
    }
    let batch_elapsed = batch_t.elapsed();

    // R-GPU-5 parity: batched vectors must match single-call vectors (the GPU
    // kernel must not silently diverge from the per-row path).
    let mut min_cos = 1.0_f32;
    for (a, b) in single_vecs.iter().zip(&batch_vecs) {
        min_cos = min_cos.min(cosine(a, b));
    }

    let single_dps = docs as f64 / single_elapsed.as_secs_f64();
    let batch_dps = docs as f64 / batch_elapsed.as_secs_f64();

    println!("---");
    println!("device_env:        {device}");
    println!("docs:              {docs}");
    println!("batch_size:        {batch}");
    println!("single_embed:      {single_elapsed:.3?}  ({single_dps:.1} docs/s)");
    println!("batch_embed:       {batch_elapsed:.3?}  ({batch_dps:.1} docs/s)");
    println!(
        "batch_vs_single:   {:.2}x",
        single_elapsed.as_secs_f64() / batch_elapsed.as_secs_f64()
    );
    println!("min_cosine(parity):{min_cos:.6}  (1.0 == identical)");

    assert!(
        min_cos > 0.999,
        "batched vectors diverged from single-call vectors (min cosine {min_cos}) — \
         a backend numeric divergence, not a speedup result"
    );

    // Optional: dump the first K single-embed vectors (full f32 precision) so a
    // CPU run and a CUDA run can be diffed for CROSS-BACKEND numeric equivalence
    // (the §3 vector-equivalence concern that R-GPU-6 documents the guard for).
    if let Ok(path) = std::env::var("GPU_SPEEDUP_DUMP") {
        use std::fmt::Write as _;
        let k = single_vecs.len().min(16);
        let mut out = String::new();
        for v in &single_vecs[..k] {
            let mut line = String::new();
            for (j, x) in v.iter().enumerate() {
                if j > 0 {
                    line.push(' ');
                }
                let _ = write!(line, "{x:.8}");
            }
            line.push('\n');
            out.push_str(&line);
        }
        std::fs::write(&path, out).expect("write dump");
        eprintln!("dumped {k} vectors to {path}");
    }
}
