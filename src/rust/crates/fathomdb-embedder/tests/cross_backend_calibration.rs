//! FathomDB 0.8.18 Slice 0 U3 — cross-backend vector-equivalence CALIBRATION
//! harness (R-CAL-1..R-CAL-4).
//!
//! A MEASUREMENT INSTRUMENT (calibrate, do NOT enforce) that feeds the 0.8.18
//! #5 D4 tolerance floor (both components: the Phase-1 binary-code flip count
//! and the Phase-2 un-centered L2). It EXTENDS the 0.8.16 same-backend baseline
//! (`candle_onnx_equivalence.rs`) with:
//!
//!   * **R-CAL-1 — strict per-leg PROCESS isolation.** candle selects its
//!     device process-globally from `FATHOMDB_EMBED_DEVICE`, so in-process
//!     per-leg device switching is forbidden. Each `(vendor, device)` leg runs
//!     in its OWN subprocess (the test binary re-invokes itself, running only
//!     `calibration_leg_worker`, with `FATHOMDB_EMBED_DEVICE` set ONCE at child
//!     start). An unavailable backend/device SKIPS CLEANLY: it never fails the
//!     test, and a silent CPU fallback of a `cuda`-requested leg is recorded as
//!     DATA (effective ≠ requested ⇒ the leg is a skip, never mislabeled GPU).
//!   * **R-CAL-2 — reproduce the 0.8.16 CPU baseline.** `Cls` pooling pinned on
//!     BOTH candle + ONNX; candle-CPU↔ONNX-CPU over the 45 probes **hard-asserts
//!     0 sign flips + cosine ≥ 0.99** (a real `assert!`, not printed output).
//!   * **R-CAL-3 — emit both #5 representations per leg-pair.** cosine / L2 /
//!     max-abs Δ / raw-sign flips / **mean-centered** flips (using a REAL
//!     engine-pinned `mean_vec` fixture — mean=0 is raw-sign ONLY, not a proxy)
//!     / P2 un-centered L2.
//!   * **R-CAL-4 — durable results regardless of outcome:** writes
//!     `dev/plans/runs/0.8.18-slice-0-cross-backend-calibration.md`.
//!
//! **Build/run discipline (worktree):** ONLY the CPU legs run here (pure
//! `cargo test` on CPU). The candle-CUDA leg + the ONNX-GPU-EP leg are gated to
//! SKIP cleanly and are selectable by env so the orchestrator can run just that
//! leg on the MAIN tree (`--features …,embed-cuda`, `FATHOMDB_EMBED_DEVICE=cuda:0`).
//! This harness NEVER builds or runs `embed-cuda`.
//!
//! ENV-GATED like the baseline: set `ORT_DYLIB_PATH`, `FATHOMDB_ONNX_MODEL_PATH`,
//! `FATHOMDB_ONNX_TOKENIZER_PATH` (and warm the candle HF cache) to run the ONNX
//! legs; unset ⇒ ONNX legs skip cleanly, candle-only paths still run.
//!
//! ```sh
//! cargo test -p fathomdb-embedder --features default-embedder,onnx-embedder \
//!     --test cross_backend_calibration -- --nocapture
//! ```
//!
//! Compiles to nothing unless BOTH features are on, so the default
//! `cargo clippy/check --workspace --all-targets` sees an empty test crate.
#![cfg(all(feature = "default-embedder", feature = "onnx-embedder"))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use fathomdb_embedder::{CandleBgeEmbedder, OrtBgeEmbedder, OrtPooling, Pooling};
use fathomdb_embedder_api::Embedder;

const PROBES: &str = include_str!("fixtures/candle_onnx_equivalence_probes.txt");
/// REAL engine-pinned mean_vec (little-endian f32[384]) produced by
/// `fathomdb-engine/tests/gen_cross_backend_mean_fixture.rs` (R-CAL-3). mean=0
/// is raw-sign ONLY and is NOT a valid proxy for the engine's centered
/// `embedding_bin`; this is the actual `_fathomdb_embedder_profiles.mean_vec`.
const PINNED_MEAN: &[u8] = include_bytes!("fixtures/cross_backend_pinned_mean.f32");
const DIM: usize = 384;

// ── probe + fixture parsing ─────────────────────────────────────────────

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

fn pinned_mean() -> Vec<f32> {
    assert_eq!(
        PINNED_MEAN.len(),
        DIM * 4,
        "pinned-mean fixture must be {} bytes (f32[384] LE)",
        DIM * 4
    );
    PINNED_MEAN.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn onnx_env() -> Option<(String, String)> {
    let dylib = std::env::var("ORT_DYLIB_PATH").ok()?;
    let model = std::env::var("FATHOMDB_ONNX_MODEL_PATH").ok()?;
    let tok = std::env::var("FATHOMDB_ONNX_TOKENIZER_PATH").ok()?;
    let _ = dylib; // presence-checked; consumed by `ort` load-dynamic at runtime.
    Some((model, tok))
}

// ── leg result (produced by a child process, consumed by the orchestrator) ─

struct LegResult {
    /// True when this leg is NOT a measured run on the requested device: either a
    /// clean skip (env unset / construction failed) OR a fallback (requested
    /// backend unavailable ⇒ effective ≠ requested). A fallback is a skip.
    skipped: bool,
    /// True specifically for the fallback sub-case (requested backend unavailable,
    /// silently downgraded) — distinct from a clean env-unset/construction skip.
    /// Vectors + the effective device are retained as DATA in this case.
    fallback: bool,
    reason: String,
    requested_device: String,
    effective_device: String,
    vectors: Vec<Vec<f32>>,
}

/// Backend token of a device label (`"cuda:0"` → `"cuda"`, `""`/`"cpu"` → `"cpu"`).
fn backend_of(label: &str) -> &str {
    let head = label.split(':').next().unwrap_or("cpu");
    if head.is_empty() {
        "cpu"
    } else {
        head
    }
}

/// Did the leg actually run on the requested backend? (A `cuda`-requested leg
/// that fell back to CPU did NOT — it is an unavailable-backend SKIP.)
fn ran_on_requested(requested: &str, effective: &str) -> bool {
    backend_of(requested) == backend_of(effective)
}

// ── per-leg embedding step (R-CAL-1): ONE process = ONE device binding ─────

/// The per-leg embedding step. Dispatched by env: the orchestrator sets
/// `FATHOMDB_CAL_LEG` (vendor), `FATHOMDB_EMBED_DEVICE` (device, ONCE at process
/// start), and `FATHOMDB_CAL_OUT` (leg-output path) before spawning THIS test
/// in a fresh process. In a normal `cargo test` run `FATHOMDB_CAL_LEG` is unset,
/// so it returns immediately (a clean skip — it is only a worker).
#[test]
fn calibration_leg_worker() {
    let Ok(vendor) = std::env::var("FATHOMDB_CAL_LEG") else {
        // Not spawned as a leg: nothing to do (this is the orchestrator's tool).
        return;
    };
    let out = std::env::var("FATHOMDB_CAL_OUT").expect("FATHOMDB_CAL_OUT must be set for a leg");
    let requested = std::env::var("FATHOMDB_EMBED_DEVICE").unwrap_or_default();

    let set = probes();
    match vendor.as_str() {
        "candle" => {
            // candle reads FATHOMDB_EMBED_DEVICE at construction (process-global,
            // already set by the parent); falls back to CPU + LOUD warn when the
            // requested GPU backend is not compiled in — which we record via
            // `device_label()` (effective ≠ requested ⇒ the orchestrator skips it).
            let emb = CandleBgeEmbedder::new()
                .expect("open CandleBgeEmbedder from the warm HF cache")
                .with_pooling(Pooling::Cls);
            let effective = emb.device_label();
            let vectors: Vec<Vec<f32>> =
                set.iter().map(|p| emb.embed(p).expect("candle embed")).collect();
            let note = fallback_note("candle", &requested, &effective);
            write_leg(&out, &LegOut::classify("candle", &requested, &effective, &vectors, &note));
        }
        "onnx" => {
            let Some((model, tok)) = onnx_env() else {
                write_leg(
                    &out,
                    &LegOut::skipped(
                        "onnx",
                        &requested,
                        "ONNX asset env unset (ORT_DYLIB_PATH / FATHOMDB_ONNX_MODEL_PATH / \
                         FATHOMDB_ONNX_TOKENIZER_PATH)",
                    ),
                );
                return;
            };
            // ONNX resolves its EP at construction from FATHOMDB_EMBED_DEVICE; a
            // non-CPU EP unavailable in this build downgrades to CPU. The
            // effective provider is captured via `effective_provider()`.
            match OrtBgeEmbedder::from_files(Path::new(&model), Path::new(&tok)) {
                Ok(emb) => {
                    let emb = emb.with_pooling(OrtPooling::Cls);
                    let effective = emb.effective_provider().to_string();
                    let vectors: Vec<Vec<f32>> =
                        set.iter().map(|p| emb.embed(p).expect("onnx embed")).collect();
                    let note = fallback_note("onnx", &requested, &effective);
                    write_leg(
                        &out,
                        &LegOut::classify("onnx", &requested, &effective, &vectors, &note),
                    );
                }
                Err(e) => {
                    write_leg(
                        &out,
                        &LegOut::skipped(
                            "onnx",
                            &requested,
                            &format!("OrtBgeEmbedder construction failed: {e:?}"),
                        ),
                    );
                }
            }
        }
        other => panic!("unknown FATHOMDB_CAL_LEG vendor {other:?} (expected candle|onnx)"),
    }
}

/// A leg's payload, serialized to JSON by hand (no serde derive needed).
struct LegOut<'a> {
    skipped: bool,
    fallback: bool,
    reason: &'a str,
    vendor: &'a str,
    requested: &'a str,
    effective: &'a str,
    vectors: &'a [Vec<f32>],
}

const NO_VECTORS: &[Vec<f32>] = &[];

/// Human-readable fallback classification note (used only when effective ≠
/// requested). Names both the requested and the effective backend so the durable
/// leg payload is self-describing.
fn fallback_note(vendor: &str, requested: &str, effective: &str) -> String {
    format!(
        "{vendor} leg: requested backend `{requested}` unavailable in this build/runtime; \
         silently downgraded to effective `{effective}` — classified a SKIP/fallback \
         (silent-GPU guard), NOT a measured run on the requested device"
    )
}

impl<'a> LegOut<'a> {
    /// Classify a leg that PRODUCED vectors on `effective`. A genuine measured RUN
    /// requires the effective backend to match the requested one; otherwise the
    /// requested backend was unavailable and we fell back — which is a SKIP/fallback
    /// (`effective ≠ requested`), never a clean `ran` and never mislabeled as the
    /// requested device, even though the produced vectors are retained as DATA.
    fn classify(
        vendor: &'a str,
        requested: &'a str,
        effective: &'a str,
        vectors: &'a [Vec<f32>],
        fallback_reason: &'a str,
    ) -> Self {
        if ran_on_requested(requested, effective) {
            LegOut {
                skipped: false,
                fallback: false,
                reason: "",
                vendor,
                requested,
                effective,
                vectors,
            }
        } else {
            // effective ≠ requested ⇒ fallback: a SKIP that retains the effective
            // device + vectors as DATA (never a clean `ran`).
            LegOut {
                skipped: true,
                fallback: true,
                reason: fallback_reason,
                vendor,
                requested,
                effective,
                vectors,
            }
        }
    }
    fn skipped(vendor: &'a str, requested: &'a str, reason: &'a str) -> Self {
        LegOut {
            skipped: true,
            fallback: false,
            reason,
            vendor,
            requested,
            effective: "n/a",
            vectors: NO_VECTORS,
        }
    }
}

fn write_leg(path: &str, leg: &LegOut<'_>) {
    // Vectors as JSON arrays of f64 (f32→f64 is exact; serde_json round-trips
    // f64 losslessly), so the orchestrator recovers the exact bytes.
    let mut vecs = String::from("[");
    for (i, v) in leg.vectors.iter().enumerate() {
        if i > 0 {
            vecs.push(',');
        }
        vecs.push('[');
        for (j, x) in v.iter().enumerate() {
            if j > 0 {
                vecs.push(',');
            }
            use std::fmt::Write as _;
            let _ = write!(vecs, "{}", f64::from(*x));
        }
        vecs.push(']');
    }
    vecs.push(']');
    let json = format!(
        "{{\"skipped\":{},\"fallback\":{},\"reason\":{:?},\"vendor\":{:?},\"requested_device\":{:?},\
         \"effective_device\":{:?},\"dim\":{},\"n\":{},\"vectors\":{}}}",
        leg.skipped,
        leg.fallback,
        leg.reason,
        leg.vendor,
        leg.requested,
        leg.effective,
        DIM,
        leg.vectors.len(),
        vecs,
    );
    std::fs::write(path, json).expect("write leg output");
}

// ── orchestrator: spawn one subprocess per leg (R-CAL-1) ───────────────────

static NONCE: AtomicU64 = AtomicU64::new(0);

/// Run a `(vendor, device)` leg in its OWN process (self-re-exec of the test
/// binary, running ONLY `calibration_leg_worker`), with `FATHOMDB_EMBED_DEVICE`
/// set ONCE at child start. Returns the parsed leg result.
fn run_leg(vendor: &str, device: &str) -> LegResult {
    let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
    let safe_dev = device.replace([':', '/'], "_");
    let out: PathBuf = std::env::temp_dir()
        .join(format!("fathomdb_cal_leg_{vendor}_{safe_dev}_{}_{nonce}.json", std::process::id()));
    let exe = std::env::current_exe().expect("current test exe");
    let status = Command::new(exe)
        // Run ONLY the leg worker in the child.
        .args(["calibration_leg_worker", "--exact", "--nocapture", "--test-threads=1"])
        .env("FATHOMDB_CAL_LEG", vendor)
        .env("FATHOMDB_CAL_OUT", &out)
        .env("FATHOMDB_EMBED_DEVICE", device)
        .status()
        .expect("spawn calibration leg");
    assert!(status.success(), "leg {vendor}/{device} process exited with {status}");

    let raw = std::fs::read_to_string(&out)
        .unwrap_or_else(|e| panic!("read leg output {}: {e}", out.display()));
    let _ = std::fs::remove_file(&out);
    parse_leg(&raw)
}

fn parse_leg(raw: &str) -> LegResult {
    let v: serde_json::Value = serde_json::from_str(raw).expect("parse leg JSON");
    let vectors = v["vectors"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    row.as_array()
                        .expect("vector row array")
                        .iter()
                        .map(|x| x.as_f64().expect("f64 component") as f32)
                        .collect::<Vec<f32>>()
                })
                .collect::<Vec<Vec<f32>>>()
        })
        .unwrap_or_default();
    LegResult {
        skipped: v["skipped"].as_bool().unwrap_or(true),
        fallback: v["fallback"].as_bool().unwrap_or(false),
        reason: v["reason"].as_str().unwrap_or("").to_string(),
        requested_device: v["requested_device"].as_str().unwrap_or("").to_string(),
        effective_device: v["effective_device"].as_str().unwrap_or("").to_string(),
        vectors,
    }
}

// ── comparison metrics (pure CPU, no embedder) ─────────────────────────────

struct PairMetrics {
    cosine_mean: f64,
    cosine_min: f64,
    l2_mean: f64,
    l2_max: f64,
    max_abs_delta: f64,
    /// Total raw-sign (m=0) flips over all probes × DIM bits.
    raw_flips_total: usize,
    /// Total mean-centered flips (sign(x − mean_vec)) over all probes × DIM bits.
    mean_centered_flips_total: usize,
    /// Phase-2 un-centered L2, aggregated (mean/max) — same value axis as `l2`.
    p2_l2_mean: f64,
    p2_l2_max: f64,
    n_probes: usize,
}

/// Compare two legs' vectors probe-by-probe. `mean` is the REAL pinned mean_vec.
fn compare_pair(a: &[Vec<f32>], b: &[Vec<f32>], mean: &[f32]) -> PairMetrics {
    assert_eq!(a.len(), b.len(), "leg vector counts must match");
    assert!(!a.is_empty(), "no vectors to compare");
    let mut cos = Vec::with_capacity(a.len());
    let mut l2 = Vec::with_capacity(a.len());
    let mut max_abs = 0.0_f64;
    let mut raw_flips = 0usize;
    let mut mc_flips = 0usize;
    for (va, vb) in a.iter().zip(b.iter()) {
        assert_eq!(va.len(), DIM);
        assert_eq!(vb.len(), DIM);
        let mut dot = 0.0_f64;
        let mut na = 0.0_f64;
        let mut nb = 0.0_f64;
        let mut sq = 0.0_f64;
        for i in 0..DIM {
            let (x, y, m) = (f64::from(va[i]), f64::from(vb[i]), f64::from(mean[i]));
            dot += x * y;
            na += x * x;
            nb += y * y;
            let d = x - y;
            sq += d * d;
            if d.abs() > max_abs {
                max_abs = d.abs();
            }
            // vec_quantize_binary = threshold-at-0 sign: bit set when >= 0.
            if (x >= 0.0) != (y >= 0.0) {
                raw_flips += 1;
            }
            if ((x - m) >= 0.0) != ((y - m) >= 0.0) {
                mc_flips += 1;
            }
        }
        let denom = (na.sqrt() * nb.sqrt()).max(1e-12);
        cos.push(dot / denom);
        l2.push(sq.sqrt());
    }
    let n = a.len();
    let cosine_mean = cos.iter().sum::<f64>() / n as f64;
    let cosine_min = cos.iter().cloned().fold(f64::INFINITY, f64::min);
    let l2_mean = l2.iter().sum::<f64>() / n as f64;
    let l2_max = l2.iter().cloned().fold(0.0_f64, f64::max);
    PairMetrics {
        cosine_mean,
        cosine_min,
        l2_mean,
        l2_max,
        max_abs_delta: max_abs,
        raw_flips_total: raw_flips,
        mean_centered_flips_total: mc_flips,
        // P2 = the un-centered L2 axis (same numbers as l2 here, named for D4).
        p2_l2_mean: l2_mean,
        p2_l2_max: l2_max,
        n_probes: n,
    }
}

/// Measured candle-CUDA leg-pairs (present only when the CUDA leg actually ran
/// on the GPU — i.e. the MAIN-tree `embed-cuda` build). Absent in the worktree.
struct GpuPairs {
    effective: String,
    cpu_vs_cuda: PairMetrics,
    cuda_vs_onnx: PairMetrics,
}

/// One measured leg-pair row for the durable-doc table.
fn measured_row(label: &str, m: &PairMetrics) -> String {
    format!(
        "| {label} | {n} | {cos_mean:.9} | {cos_min:.9} | {l2_mean:.3e} | {l2_max:.3e} | \
         {maxabs:.3e} | **{raw} / {bits}** | **{mc} / {bits}** |",
        n = m.n_probes,
        cos_mean = m.cosine_mean,
        cos_min = m.cosine_min,
        l2_mean = m.l2_mean,
        l2_max = m.l2_max,
        maxabs = m.max_abs_delta,
        raw = m.raw_flips_total,
        mc = m.mean_centered_flips_total,
        bits = m.n_probes * DIM,
    )
}

// ── R-CAL-2: reproduce the 0.8.16 CPU baseline (HARD ASSERT) ───────────────

#[test]
fn cpu_legs_reproduce_0816_baseline() {
    if onnx_env().is_none() {
        eprintln!(
            "SKIP cpu_legs_reproduce_0816_baseline: ONNX asset env unset — set ORT_DYLIB_PATH + \
             FATHOMDB_ONNX_MODEL_PATH + FATHOMDB_ONNX_TOKENIZER_PATH (and warm the candle HF cache)"
        );
        return;
    }

    let candle = run_leg("candle", "cpu");
    let onnx = run_leg("onnx", "cpu");
    assert!(!candle.skipped, "candle-CPU leg must run, got skip: {}", candle.reason);
    assert!(!onnx.skipped, "onnx-CPU leg must run, got skip: {}", onnx.reason);
    assert_eq!(backend_of(&candle.effective_device), "cpu", "candle leg must be on CPU");
    assert_eq!(backend_of(&onnx.effective_device), "cpu", "onnx leg must be on CPU");
    assert_eq!(candle.vectors.len(), 45, "expected 45 probes");

    let mean = pinned_mean();
    let m = compare_pair(&candle.vectors, &onnx.vectors, &mean);

    eprintln!(
        "R-CAL-2 candle-CPU vs ONNX-CPU (CLS pinned both): probes={} cosine_mean={:.9} \
         cosine_min={:.9} sign_flips_total={} of {} bits max_abs_delta={:.3e} l2_mean={:.3e}",
        m.n_probes,
        m.cosine_mean,
        m.cosine_min,
        m.raw_flips_total,
        m.n_probes * DIM,
        m.max_abs_delta,
        m.l2_mean,
    );

    // HARD ASSERT (R-CAL-2): reproduce the 0.8.16 baseline (`70c2dad6`).
    assert_eq!(
        m.raw_flips_total,
        0,
        "candle-CPU↔ONNX-CPU must have 0 sign flips over {} bits (CLS pinned both); got {}",
        m.n_probes * DIM,
        m.raw_flips_total
    );
    assert!(
        m.cosine_mean >= 0.99,
        "mean cosine {:.9} < 0.99 with identical weights — SURFACE this",
        m.cosine_mean
    );
    assert!(m.cosine_min >= 0.99, "min cosine {:.9} < 0.99 — a probe diverged", m.cosine_min);
}

// ── R-CAL-1: unavailable backends skip cleanly; CPU legs run ───────────────

#[test]
fn harness_skips_unavailable_backends_cleanly() {
    // CPU candle leg ALWAYS runs (no ONNX env needed).
    let cpu = run_leg("candle", "cpu");
    assert!(!cpu.skipped, "candle-CPU leg must run");
    assert_eq!(backend_of(&cpu.effective_device), "cpu");
    assert_eq!(cpu.vectors.len(), 45, "CPU leg must produce 45 probe vectors");
    assert!(ran_on_requested("cpu", &cpu.effective_device));

    // candle-CUDA leg. The harness must pass in BOTH valid outcomes and fail only
    // on a genuinely broken classification (a leg claiming it ran on the requested
    // device when effective ≠ requested):
    //   * effective backend ≠ requested (cuda fell back to cpu — the no-CUDA-runtime
    //     case, e.g. this CPU-only worktree build) ⇒ the leg MUST be classified a
    //     clean SKIP/fallback, never a test failure and never mislabeled GPU;
    //   * a VALID cuda runtime is present and effective == cuda (e.g. the MAIN-tree
    //     `embed-cuda` leg or a CUDA host) ⇒ that is a VALID RUN, not a failure.
    let gpu = run_leg("candle", "cuda:0");
    let gpu_backend = backend_of(&gpu.effective_device);
    assert!(
        gpu_backend == "cpu" || gpu_backend == "cuda",
        "candle cuda leg reported an incoherent effective backend: {}",
        gpu.effective_device
    );
    if ran_on_requested("cuda:0", &gpu.effective_device) {
        // Valid CUDA runtime: a measured RUN — never a failure, never a skip.
        assert_eq!(gpu_backend, "cuda", "ran_on_requested(cuda) but effective backend != cuda");
        assert!(
            !gpu.skipped && !gpu.fallback,
            "a candle cuda leg that ran on cuda must NOT be classified skip/fallback (effective={})",
            gpu.effective_device
        );
        assert_eq!(gpu.vectors.len(), 45, "a measured candle cuda leg must produce 45 vectors");
    } else {
        // No CUDA runtime: candle fell back to CPU ⇒ a clean SKIP/fallback (never a
        // test failure and never mislabeled GPU — the silent-GPU guard).
        assert_eq!(
            gpu_backend, "cpu",
            "candle cuda leg without a cuda runtime must report effective cpu, got {}",
            gpu.effective_device
        );
        assert!(
            gpu.skipped && gpu.fallback,
            "a candle cuda leg that fell back to CPU (effective != requested) must be classified a \
             SKIP/fallback, not a clean run"
        );
    }

    // ONNX-GPU-EP leg: only when the ONNX asset env is present. Same both-cases
    // rule: a CUDA EP present + engaged ⇒ a VALID RUN; a CUDA EP absent from this
    // ONNX Runtime build downgrades to CPU at construction ⇒ a clean SKIP/fallback.
    // `effective_provider()` reports the truth, so we fail only on a broken
    // classification.
    if onnx_env().is_some() {
        let ogpu = run_leg("onnx", "cuda");
        if ran_on_requested("cuda", &ogpu.effective_device) {
            // A CUDA EP genuinely engaged: a VALID RUN, not a failure.
            assert_eq!(
                backend_of(&ogpu.effective_device),
                "cuda",
                "ran_on_requested(cuda) but ONNX effective provider != cuda"
            );
            assert!(
                !ogpu.skipped && !ogpu.fallback,
                "an ONNX cuda leg whose CUDA EP engaged must NOT be classified skip/fallback \
                 (effective={})",
                ogpu.effective_device
            );
        } else {
            // Downgraded to CPU (no CUDA EP) or construction failed ⇒ a clean skip.
            assert!(
                ogpu.skipped,
                "an ONNX cuda leg that downgraded to CPU (or failed to construct) must be \
                 classified a SKIP/fallback, effective={}",
                ogpu.effective_device
            );
        }
    }
}

// ── R-CAL-3 + R-CAL-4: emit both #5 representations + durable doc ──────────

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..").canonicalize().expect("repo root")
}

#[test]
fn calibration_reports_p1_flips_and_p2_l2() {
    if onnx_env().is_none() {
        eprintln!(
            "SKIP calibration_reports_p1_flips_and_p2_l2: ONNX asset env unset — the durable doc \
             is written by a run WITH the ONNX assets provisioned (see module docs)"
        );
        return;
    }

    let mean = pinned_mean();
    let candle_cpu = run_leg("candle", "cpu");
    let onnx_cpu = run_leg("onnx", "cpu");
    assert!(!candle_cpu.skipped && !onnx_cpu.skipped, "CPU legs must run for the calibration");

    // Leg-pair 1 (the measured CPU baseline): candle-CPU ↔ ONNX-CPU.
    let m = compare_pair(&candle_cpu.vectors, &onnx_cpu.vectors, &mean);

    // R-CAL-3: BOTH representations present + sane. For the CPU baseline the raw
    // and mean-centered flip counts are both expected 0 (the ~1e-7 f32 round-off
    // never crosses a threshold), and P2 L2 is tiny.
    eprintln!(
        "R-CAL-3 candle-CPU↔ONNX-CPU: cosine_mean={:.9} l2_mean={:.3e} l2_max={:.3e} \
         max_abs_delta={:.3e} raw_flips={} mean_centered_flips={} p2_l2_mean={:.3e}",
        m.cosine_mean,
        m.l2_mean,
        m.l2_max,
        m.max_abs_delta,
        m.raw_flips_total,
        m.mean_centered_flips_total,
        m.p2_l2_mean,
    );
    // The mean-centering must be exercised with the REAL pinned mean (non-zero):
    // assert the fixture is actually a non-degenerate mean, so this is not a
    // vacuous m=0 raw-sign measurement mislabeled as centered.
    let mean_l2: f64 = mean.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>().sqrt();
    assert!(mean_l2 > 1e-3, "pinned mean fixture must be non-degenerate (‖mean‖₂={mean_l2:.6})");

    // Both D4 floor components are emitted (P1 flip counts + P2 L2). For the
    // CPU baseline both flip counts are 0 and P2 L2 is sub-1e-5.
    assert_eq!(m.raw_flips_total, 0, "CPU baseline raw flips must be 0");
    assert_eq!(m.mean_centered_flips_total, 0, "CPU baseline mean-centered flips must be 0");
    assert!(m.p2_l2_max < 1e-4, "CPU baseline P2 L2 max {:.3e} unexpectedly large", m.p2_l2_max);

    // R-CAL-4 candle-CUDA refresh: attempt the CUDA leg through the SAME harness.
    // In the worktree (no `embed-cuda`) it falls back to CPU and is recorded as
    // pending/skip (never a failure); on the MAIN tree (`embed-cuda`) it runs on
    // the GPU and BOTH candle-CUDA leg-pairs are measured + written into the doc
    // with NO orchestrator glue — the orchestrator just re-runs this test on MAIN
    // with the `embed-cuda` feature + `FATHOMDB_EMBED_DEVICE` inherited by the
    // parent (the leg child re-binds cuda:0). This is a CALIBRATION read, never a
    // gate: no assertion is placed on the GPU deltas.
    let candle_cuda = run_leg("candle", "cuda:0");
    let cuda_ran =
        !candle_cuda.skipped && ran_on_requested("cuda:0", &candle_cuda.effective_device);
    let gpu = if cuda_ran {
        let cpu_vs_cuda = compare_pair(&candle_cpu.vectors, &candle_cuda.vectors, &mean);
        let cuda_vs_onnx = compare_pair(&candle_cuda.vectors, &onnx_cpu.vectors, &mean);
        eprintln!(
            "R-CAL-4 candle-CUDA leg MEASURED on {}: CPU↔CUDA raw_flips={} mc_flips={} p2_l2_max={:.3e}; \
             CUDA↔ONNX-CPU raw_flips={} mc_flips={} p2_l2_max={:.3e}",
            candle_cuda.effective_device,
            cpu_vs_cuda.raw_flips_total,
            cpu_vs_cuda.mean_centered_flips_total,
            cpu_vs_cuda.p2_l2_max,
            cuda_vs_onnx.raw_flips_total,
            cuda_vs_onnx.mean_centered_flips_total,
            cuda_vs_onnx.p2_l2_max,
        );
        Some(GpuPairs {
            effective: candle_cuda.effective_device.clone(),
            cpu_vs_cuda,
            cuda_vs_onnx,
        })
    } else {
        eprintln!(
            "R-CAL-4 candle-CUDA leg gated-to-skip (effective={}) — recorded PENDING (run on MAIN)",
            candle_cuda.effective_device
        );
        None
    };

    write_durable_doc(&candle_cpu, &onnx_cpu, &m, mean_l2, gpu.as_ref());
}

/// R-CAL-4: write the durable results doc (mirrors the Slice-15 doc's shape).
fn write_durable_doc(
    candle: &LegResult,
    onnx: &LegResult,
    m: &PairMetrics,
    mean_l2: f64,
    gpu: Option<&GpuPairs>,
) {
    let path = repo_root().join("dev/plans/runs/0.8.18-slice-0-cross-backend-calibration.md");
    // candle-CUDA rows: measured when the GPU leg ran (MAIN tree), else pending.
    let (cpu_vs_cuda_row, cuda_vs_onnx_row, gpu_status) = match gpu {
        Some(g) => (
            measured_row("candle-CPU ↔ candle-CUDA", &g.cpu_vs_cuda),
            measured_row("candle-CUDA ↔ ONNX-CPU", &g.cuda_vs_onnx),
            format!("MEASURED on `{}` (MAIN tree)", g.effective),
        ),
        None => (
            "| candle-CPU ↔ candle-CUDA | — | pending (MAIN tree; `embed-cuda`, \
             `FATHOMDB_EMBED_DEVICE=cuda:0`) | | | | | | |"
                .to_string(),
            "| candle-CUDA ↔ ONNX-CPU | — | pending (MAIN tree) | | | | | | |".to_string(),
            "PENDING (worktree runs CPU legs only; run on MAIN with `embed-cuda`)".to_string(),
        ),
    };
    let doc = format!(
        r#"# 0.8.18 Slice 0 U3 — cross-backend vector-equivalence CALIBRATION (R-CAL-1..R-CAL-4)

Status: **MEASURED + RECORDED** (calibration instrument — measures, does NOT
enforce). Feeds the 0.8.18 #5 **D4 tolerance floor** (both components: the
Phase-1 binary-code flip count + the Phase-2 un-centered L2). Distinct from the
shipped U1 #5 open-time check. Design: `dev/design/0.8.18-slice-0-vector-equivalence-publish-design.md` §U3.

This file is emitted by the harness
`src/rust/crates/fathomdb-embedder/tests/cross_backend_calibration.rs`
(`calibration_reports_p1_flips_and_p2_l2`).

## Architecture (R-CAL-1 — strict per-leg PROCESS isolation)

candle selects its device process-globally from `FATHOMDB_EMBED_DEVICE`, so
in-process per-leg device switching is forbidden. Each `(vendor, device)` leg
runs in its OWN subprocess (the test binary re-invokes itself running only
`calibration_leg_worker`, `FATHOMDB_EMBED_DEVICE` set ONCE at child start). A
leg records its REQUESTED and EFFECTIVE device; a `cuda`-requested leg that fell
back to CPU (no `embed-cuda` build / no CUDA lib) has effective ≠ requested and
is a **clean SKIP**, never mislabeled GPU (candle via `device_label()`, ONNX via
the additive `effective_provider()` accessor).

## Backend matrix

| Role | Embedder | Feature | Pooling | Requested | Effective |
| --- | --- | --- | --- | --- | --- |
| Reference | `CandleBgeEmbedder` | `default-embedder` | CLS | {c_req} | {c_eff} |
| Under test | `OrtBgeEmbedder` | `onnx-embedder` | CLS | {o_req} | {o_eff} |

Both backends load the SAME pinned `BAAI/bge-small-en-v1.5` weights
(HF revision `5c38ec7c405ec4b44b94cc5a9bb96e735b38267a`, dim 384), **CLS pooling
pinned on BOTH** (`CandleBgeEmbedder::new()` defaults to `Mean` — overridden to
`Cls` to match the 0.8.16 baseline `70c2dad6`).

## Probe set + pinned-mean fixture

- Probes: `tests/fixtures/candle_onnx_equivalence_probes.txt` — the fixed,
  order-stable **45-probe** set (shared with the 0.8.16 baseline).
- Pinned mean (R-CAL-3): `tests/fixtures/cross_backend_pinned_mean.f32` — a
  **REAL** engine-pinned `_fathomdb_embedder_profiles.mean_vec` (little-endian
  f32[384]) produced by ingesting 300 docs through a throwaway engine DB past
  the 256-row `MEAN_VEC_PIN_THRESHOLD` (generator
  `fathomdb-engine/tests/gen_cross_backend_mean_fixture.rs`). `‖mean‖₂ = {mean_l2:.6}`
  (non-degenerate). **mean=0 is raw-sign ONLY** and is reported separately as
  such; the mean-centered column uses THIS real mean.

## Measured leg-pairs

Each cell reports the two D4 floor components: **P1** binary-code flip counts
(raw sign, m=0; and mean-centered `sign(x − mean_vec)`) and **P2** the
un-centered L2 (`vec_distance_l2`).

| Leg-pair | probes | cosine mean | cosine min | P2 L2 mean | P2 L2 max | max-abs Δ | **P1 raw-sign flips** | **P1 mean-centered flips** |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| candle-CPU ↔ ONNX-CPU | {n} | {cos_mean:.9} | {cos_min:.9} | {l2_mean:.3e} | {l2_max:.3e} | {maxabs:.3e} | **{raw} / {bits}** | **{mc} / {bits}** |
{cpu_vs_cuda_row}
{cuda_vs_onnx_row}
| ONNX-GPU-EP (D3/U4/L3) | — | pending OOB (CUDA `libonnxruntime.so`) | | | | | | |

**candle-CUDA legs: {gpu_status}.**

## Reading

The candle-CPU ↔ ONNX-CPU baseline reproduces `70c2dad6`: **{raw} sign flips of
{bits} bits** (raw) and **{mc} of {bits}** (mean-centered against the real pinned
mean), cosine ≈ {cos_mean:.6} (min {cos_min:.6}), with only benign f32 round-off
(P2 L2 ≤ {l2_max:.2e}, max-abs Δ ≤ {maxabs:.2e}) — far below the magnitude that
could push a component across a quantization threshold and flip a bit. Both P1
flip counts and the P2 L2 are emitted so the 0.8.18 #5 **D4 floor** can be set on
BOTH components after the GPU legs land.

## Remaining legs (MAIN tree only — GPU discipline)

The candle-CUDA + ONNX-GPU-EP legs are gated-to-skip in the worktree (which never
builds `embed-cuda`) and are run by the orchestrator on the MAIN tree via the
SAME harness:

```sh
# candle-CUDA leg (MAIN tree; nvcc on PATH, CUDA_HOME set):
FATHOMDB_EMBED_DEVICE=cuda:0 cargo test -p fathomdb-embedder \
    --features default-embedder,onnx-embedder,embed-cuda \
    --test cross_backend_calibration -- --nocapture

# ONNX-GPU-EP leg (U4/L3, OOB): point ORT_DYLIB_PATH at a CUDA-enabled
# libonnxruntime.so and set FATHOMDB_EMBED_DEVICE=cuda:0 — effective_provider()
# records whether the CUDA EP actually engaged (guards silent CPU fallback).
```
"#,
        c_req = candle.requested_device,
        c_eff = candle.effective_device,
        o_req = onnx.requested_device,
        o_eff = onnx.effective_device,
        n = m.n_probes,
        cos_mean = m.cosine_mean,
        cos_min = m.cosine_min,
        l2_mean = m.l2_mean,
        l2_max = m.l2_max,
        maxabs = m.max_abs_delta,
        raw = m.raw_flips_total,
        mc = m.mean_centered_flips_total,
        bits = m.n_probes * DIM,
        mean_l2 = mean_l2,
        cpu_vs_cuda_row = cpu_vs_cuda_row,
        cuda_vs_onnx_row = cuda_vs_onnx_row,
        gpu_status = gpu_status,
    );
    std::fs::write(&path, doc)
        .unwrap_or_else(|e| panic!("write durable doc {}: {e}", path.display()));
    eprintln!("R-CAL-4 wrote durable results doc: {}", path.display());
}
