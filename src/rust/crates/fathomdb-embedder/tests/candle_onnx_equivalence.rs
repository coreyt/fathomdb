//! FathomDB 0.8.16 Slice 15 — candle<->ONNX numeric-equivalence measurement
//! (R-ONNX-3).
//!
//! Embeds a FIXED, order-stable probe set (tests/fixtures/) with BOTH backends
//! on the CPU (same-backend fidelity per policy `649a8d45`):
//!   - candle reference: `CandleBgeEmbedder` with CLS pooling (the shipped
//!     reference, feature `default-embedder`);
//!   - ONNX: `OrtBgeEmbedder` on the CPU execution provider with CLS pooling
//!     (feature `onnx-embedder`).
//!
//! Both load the SAME pinned bge-small-en-v1.5 weights, so agreement should be
//! very high; a gross divergence signals an export/pooling bug to SURFACE (not
//! hide).
//!
//! Per vector then aggregated (mean/median/p95/max): (a) cosine similarity,
//! (b) L2 distance, (c) max-abs element delta, and (d) the LOAD-BEARING 1-bit
//! sign-quantization FLIP RATE — the fraction of the 384 sign bits that differ
//! between the candle and ONNX vectors (the in-library query path is a 1-bit
//! Hamming comparison, so this is the number that actually threatens fidelity).
//!
//! R-ONNX-3 is MEASURED + RECORDED, NOT enforced (ADR-0.8.16 §3 / design §5):
//! the tight 0.8.18-style tolerance is NOT gated here. A LOOSE sanity assertion
//! (mean cosine >= 0.99, given identical weights) catches a broken export
//! without pretending to be the 0.8.18 threshold.
//!
//! ENV-GATED (not `#[ignore]`) so CI without the provisioned assets skips
//! cleanly. Set `ORT_DYLIB_PATH`, `FATHOMDB_ONNX_MODEL_PATH`,
//! `FATHOMDB_ONNX_TOKENIZER_PATH` (and have the candle HF cache present) to run
//! it for real. See `dev/tools/onnx/README.md`. Run with:
//!
//! ```sh
//! cargo test -p fathomdb-embedder --features default-embedder,onnx-embedder \
//!     --test candle_onnx_equivalence -- --nocapture
//! ```
//!
//! The whole file compiles to nothing unless BOTH features are on, so the
//! default `cargo clippy/check --workspace --all-targets` (no features) sees an
//! empty test crate.
#![cfg(all(feature = "default-embedder", feature = "onnx-embedder"))]

use std::path::Path;

use fathomdb_embedder::{CandleBgeEmbedder, OrtBgeEmbedder, OrtPooling, Pooling};
use fathomdb_embedder_api::Embedder;

const PROBES: &str = include_str!("fixtures/candle_onnx_equivalence_probes.txt");
const DIM: usize = 384;

/// Parse the fixed probe set: one probe per line, skipping blank lines and
/// comment lines (first non-whitespace char is '#'). Order-stable.
fn probes() -> Vec<String> {
    PROBES
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.is_empty() && !t.starts_with('#')
        })
        .map(|l| l.to_string())
        .collect()
}

/// Per-probe comparison metrics between two L2-normalized vectors.
struct Metrics {
    cosine: f64,
    l2: f64,
    max_abs_delta: f64,
    /// Fraction of the `DIM` sign bits that differ (1-bit sign quantization).
    sign_flip_rate: f64,
    /// Absolute count of differing sign bits (0..=DIM).
    sign_flips: usize,
}

fn compare(a: &[f32], b: &[f32]) -> Metrics {
    assert_eq!(a.len(), DIM);
    assert_eq!(b.len(), DIM);
    let mut dot = 0.0_f64;
    let mut na = 0.0_f64;
    let mut nb = 0.0_f64;
    let mut sq = 0.0_f64;
    let mut max_abs = 0.0_f64;
    let mut flips = 0usize;
    for i in 0..DIM {
        let (x, y) = (a[i] as f64, b[i] as f64);
        dot += x * y;
        na += x * x;
        nb += y * y;
        let d = x - y;
        sq += d * d;
        if d.abs() > max_abs {
            max_abs = d.abs();
        }
        // 1-bit sign quantization: bit set when component >= 0.
        if (x >= 0.0) != (y >= 0.0) {
            flips += 1;
        }
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-12);
    Metrics {
        cosine: dot / denom,
        l2: sq.sqrt(),
        max_abs_delta: max_abs,
        sign_flip_rate: flips as f64 / DIM as f64,
        sign_flips: flips,
    }
}

/// Aggregate stats over a series of f64 samples.
struct Agg {
    mean: f64,
    median: f64,
    p95: f64,
    max: f64,
    min: f64,
}

fn aggregate(mut xs: Vec<f64>) -> Agg {
    assert!(!xs.is_empty());
    let n = xs.len();
    let mean = xs.iter().sum::<f64>() / n as f64;
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = if n % 2 == 1 { xs[n / 2] } else { (xs[n / 2 - 1] + xs[n / 2]) / 2.0 };
    // Nearest-rank p95.
    let idx = (((95.0 / 100.0) * n as f64).ceil() as usize).saturating_sub(1).min(n - 1);
    let p95 = xs[idx];
    Agg { mean, median, p95, max: xs[n - 1], min: xs[0] }
}

#[test]
fn candle_onnx_equivalence_measurement() {
    // Env gate: skip cleanly when the ONNX asset / dylib are not provisioned
    // (e.g. CI). All three ONNX env vars must be set; the candle HF cache is
    // read by `CandleBgeEmbedder::new()` (local-only, no network on a warm
    // cache).
    let (Ok(_dylib), Ok(model), Ok(tok)) = (
        std::env::var("ORT_DYLIB_PATH"),
        std::env::var("FATHOMDB_ONNX_MODEL_PATH"),
        std::env::var("FATHOMDB_ONNX_TOKENIZER_PATH"),
    ) else {
        eprintln!(
            "SKIP candle_onnx_equivalence_measurement: set ORT_DYLIB_PATH + \
             FATHOMDB_ONNX_MODEL_PATH + FATHOMDB_ONNX_TOKENIZER_PATH (and warm the candle HF \
             cache) to run the R-ONNX-3 equivalence measurement (see dev/tools/onnx/README.md)"
        );
        return;
    };

    // CPU same-backend baseline (policy 649a8d45): force both backends onto CPU
    // deterministically, independent of the ambient device knob.
    std::env::set_var("FATHOMDB_EMBED_DEVICE", "cpu");

    let candle = CandleBgeEmbedder::new()
        .expect("open CandleBgeEmbedder from the warm HF cache")
        .with_pooling(Pooling::Cls);
    let onnx = OrtBgeEmbedder::from_files(Path::new(&model), Path::new(&tok))
        .expect("open OrtBgeEmbedder on the CPU EP from the provisioned asset")
        .with_pooling(OrtPooling::Cls);

    let set = probes();
    assert!(
        set.len() >= 30,
        "probe set must exercise >=30 inputs, got {} — check the fixture",
        set.len()
    );

    let mut cos = Vec::with_capacity(set.len());
    let mut l2 = Vec::with_capacity(set.len());
    let mut maxd = Vec::with_capacity(set.len());
    let mut flip = Vec::with_capacity(set.len());
    let mut total_flips = 0usize;
    let mut worst: Option<(usize, Metrics)> = None;

    for (i, p) in set.iter().enumerate() {
        let cv = candle.embed(p).expect("candle embed");
        let ov = onnx.embed(p).expect("onnx embed");
        let m = compare(&cv, &ov);
        cos.push(m.cosine);
        l2.push(m.l2);
        maxd.push(m.max_abs_delta);
        flip.push(m.sign_flip_rate);
        total_flips += m.sign_flips;
        let take = match &worst {
            None => true,
            Some((_, w)) => m.sign_flips > w.sign_flips,
        };
        if take {
            worst = Some((i, m));
        }
    }

    let a_cos = aggregate(cos);
    let a_l2 = aggregate(l2);
    let a_maxd = aggregate(maxd);
    let a_flip = aggregate(flip);
    let n = set.len();

    // Machine-parseable summary block (transcribed into
    // dev/plans/runs/0.8.16-slice-15-candle-onnx-equivalence.md + output.json).
    eprintln!("=== R-ONNX-3 candle<->ONNX equivalence (CPU vs CPU, CLS pooling) ===");
    eprintln!("R_ONNX_3 probes_n={n}");
    eprintln!(
        "R_ONNX_3 cosine mean={:.9} median={:.9} p95={:.9} min={:.9} max={:.9}",
        a_cos.mean, a_cos.median, a_cos.p95, a_cos.min, a_cos.max
    );
    eprintln!(
        "R_ONNX_3 l2 mean={:.9} median={:.9} p95={:.9} min={:.9} max={:.9}",
        a_l2.mean, a_l2.median, a_l2.p95, a_l2.min, a_l2.max
    );
    eprintln!(
        "R_ONNX_3 max_abs_delta mean={:.9} median={:.9} p95={:.9} min={:.9} max={:.9}",
        a_maxd.mean, a_maxd.median, a_maxd.p95, a_maxd.min, a_maxd.max
    );
    eprintln!(
        "R_ONNX_3 sign_flip_rate mean={:.9} median={:.9} p95={:.9} min={:.9} max={:.9}",
        a_flip.mean, a_flip.median, a_flip.p95, a_flip.min, a_flip.max
    );
    eprintln!(
        "R_ONNX_3 sign_flips_total={total_flips} of {} bits ({} probes x {DIM} dims)",
        n * DIM,
        n
    );
    if let Some((i, w)) = &worst {
        eprintln!(
            "R_ONNX_3 worst_probe idx={i} sign_flips={} rate={:.9} cosine={:.9} \
             max_abs_delta={:.9} text={:?}",
            w.sign_flips, w.sign_flip_rate, w.cosine, w.max_abs_delta, set[*i]
        );
    }

    // LOOSE sanity assertion ONLY (R-ONNX-3 is documented-not-enforced): given
    // identical weights the mean cosine must be extremely high. This catches a
    // broken export/pooling mismatch; it deliberately does NOT gate on a tight
    // 0.8.18-style flip-rate tolerance (that calibration is 0.8.18 #5's job,
    // against the numbers recorded above).
    assert!(
        a_cos.mean >= 0.99,
        "mean cosine {:.9} < 0.99 with identical weights — likely an export/pooling bug; \
         SURFACE this, do not loosen the check",
        a_cos.mean
    );
}
