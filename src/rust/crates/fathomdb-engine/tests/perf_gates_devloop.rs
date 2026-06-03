//! Dev-loop perf gates (0.7.2 PR-6).
//!
//! A fast (≤30 s warm, synthetic default) subset of the canonical AC-013 /
//! AC-013b / AC-019 gates. It exercises the SAME production read path
//! (`Engine::search` → FTS5 + two-phase bit-KNN + f32 rerank) at a small
//! N (~1000) via PR-5's [`CorpusFixture`], so developers get a perf + recall
//! signal in seconds rather than the minutes the `AGENT_LONG`-gated canonical
//! gates take.
//!
//! ## Two-tier model (see `dev/design/perf-gates.md`)
//!
//! - **Devloop (this file):** always-runs in `cargo test` — NOT gated behind
//!   `AGENT_LONG`. It is the inner-loop signal.
//! - **Canonical (`perf_gates.rs`):** `AGENT_LONG`-gated ship verdict at the
//!   tiered 10k budget (PR-3 ADR amend).
//!
//! ## Gate disposition (HITL-locked 2026-06-01)
//!
//! Per HITL, **perf signals NOTIFY, structural invariants BLOCK**:
//!
//! - **Structural invariants** (`assert_vec0_row_count_matches_ingest`,
//!   `assert_fts_index_populated`) — **hard assert**. These catch the
//!   batch-collapse regression (`4a95cfd`) directly: a collapsed batch leaves
//!   `vector_default` short of the ingested doc count and the assert RED-fails.
//! - **Soft latency budget** (p50 ≤ 50 ms / p99 ≤ 150 ms at N≈1000, synthetic
//!   path) and the **recall floor** (≥ 0.85, real path) — **notify-only**.
//!   Exceeding them prints a loud `DEVLOOP_PERF_WARN … status=OVER|UNDER` line
//!   but does NOT fail the test (small-N sample noise should not flap the inner
//!   loop; PR-7 owns turning the trend into a CI gate).
//! - **One hard catastrophic latency ceiling** (10× the soft budget:
//!   p50 > 500 ms / p99 > 1500 ms; synthetic path) — **hard assert**. This
//!   catches the projection-scanner throughput regression (`53a270d`), whose
//!   symptom is an orders-of-magnitude p50/p99 inflation that clears the
//!   ceiling while routine noise never does.
//!
//! Both named regressions therefore RED-fail `cargo test` (batch-collapse via
//! the structural assert; catastrophic scanner inflation via the ceiling),
//! while ordinary small-N latency wobble only notifies.
//!
//! ## Embedder
//!
//! Synthetic [`VaryingEmbedder`] is the always-runs default (instant embed, so
//! the measured latency isolates RETRIEVAL — the same reason canonical AC-013
//! uses it). It is the only path bound by the ≤30 s budget, and it carries the
//! LATENCY gates (synthetic recall is report-only — its sparse vectors quantize
//! poorly, ~0.35 @ N≈1000).
//!
//! The real BGE embedder is opt-in via `DEVLOOP_REAL_EMBEDDER=1` AND the
//! `default-embedder` feature; without the feature the fixture SKIPs
//! gracefully. It carries the RECALL gate (0.85 floor WARN) — its dense vectors
//! make ANN-fidelity meaningful. Its latency is report-only. Cold-cache is
//! allowed (the first run warms PR-5's on-disk doc-body cache and is slow); a
//! warm re-run skips re-embedding doc bodies. Held-out QUERY texts are not in
//! the doc-body cache, so the per-test warmup pass embeds them live once
//! (then the in-memory cache serves the measure pass) and the recall
//! ground-truth pass embeds them again via a fresh embedder — so the real
//! path's WALL time is candle-bound and not ≤30 s regardless of the doc cache.
//! It is an occasional end-to-end exercise, not the inner-loop signal. See
//! `dev/design/perf-gates.md`.

#![allow(dead_code)]

#[path = "support/corpus_harness.rs"]
mod corpus_harness;

use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use corpus_harness::{CorpusFixture, HeldOutQuery, VaryingEmbedder, CORPUS_DIM};
use fathomdb_embedder_api::Embedder;
use fathomdb_engine::Engine;

// ── HITL-locked devloop budget (2026-06-01) ───────────────────────────────
//
// N≈1000 (`CorpusFixture::medium`). Soft budget ≈ 2–3× the canonical 10k tier
// (p50 80 / p99 300) at 10× smaller N: loose enough that ~10× small-N sample
// noise does not flap, tight enough that the scanner-throughput regression
// (orders-of-magnitude inflation) trips. Recall floor 0.85 (the canonical 0.90
// is the N=7667 ANN-fidelity anchor; small-N fidelity is noisier).

/// Soft p50 budget — WARN only.
const DEVLOOP_BUDGET_P50: Duration = Duration::from_millis(50);
/// Soft p99 budget — WARN only.
const DEVLOOP_BUDGET_P99: Duration = Duration::from_millis(150);
/// Soft recall@10 floor — WARN only.
const DEVLOOP_RECALL_FLOOR: f64 = 0.85;
/// Catastrophic ceiling multiplier over the soft budget — HARD assert.
/// 10× headroom: an orders-of-magnitude scanner regression clears it; routine
/// small-N noise never does.
const DEVLOOP_CATASTROPHIC_MULT: u32 = 10;

/// Held-out query sample size for the latency + recall passes.
const DEVLOOP_SAMPLES: usize = 100;
/// Fixed seed so the query set (and therefore the measurement) is reproducible
/// run-to-run for trend tracking.
const DEVLOOP_SEED: u64 = 0x0DEF_1009_0DEF_1009;

// AC-019 devloop stress shape — modest, report-only.
const DEVLOOP_AC019_THREADS: usize = 4;
const DEVLOOP_AC019_QUERIES_PER_THREAD: usize = 50;

// ── Embedder selection ─────────────────────────────────────────────────────

/// Opt-in real-embedder path: `DEVLOOP_REAL_EMBEDDER=1`. Requires the
/// `default-embedder` feature too (the fixture SKIPs otherwise). Default is
/// synthetic, the always-runs inner-loop signal.
fn real_embedder_requested() -> bool {
    matches!(std::env::var("DEVLOOP_REAL_EMBEDDER"), Ok(v) if v == "1" || v.eq_ignore_ascii_case("true"))
}

/// Build the N≈1000 devloop fixture. Synthetic by default; real BGE when
/// opted in (and available). Returns the fixture plus a label for the
/// `DEVLOOP_NUMBERS embedder=` field.
fn devloop_fixture() -> (CorpusFixture, &'static str) {
    if real_embedder_requested() {
        (CorpusFixture::medium().with_real_embedder(), "real")
    } else {
        (CorpusFixture::medium().with_synthetic_embedder(), "synthetic")
    }
}

/// Reconstruct an embedder that produces byte-identical vectors to the one the
/// fixture used, for the f32 brute-force recall ground truth. Synthetic
/// vectors depend only on `dim` + text, so `VaryingEmbedder::new` matches the
/// fixture's caching wrapper exactly.
#[cfg(feature = "default-embedder")]
fn ground_truth_embedder(real: bool) -> Arc<dyn Embedder> {
    if real {
        Arc::new(fathomdb_embedder::CandleBgeEmbedder::new().expect("construct real bge embedder"))
    } else {
        Arc::new(VaryingEmbedder::new(CORPUS_DIM))
    }
}

#[cfg(not(feature = "default-embedder"))]
fn ground_truth_embedder(_real: bool) -> Arc<dyn Embedder> {
    // Real path is unreachable here — the fixture SKIPs without the feature.
    Arc::new(VaryingEmbedder::new(CORPUS_DIM))
}

// ── Small local stats helper (each test file is its own crate) ─────────────

/// Inclusive (ceil) percentile over a latency sample, mirroring
/// `perf_gates.rs::percentile_ceil`'s contract for parity of reported numbers.
fn percentile_ceil(samples: &[Duration], numerator: usize, denominator: usize) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    // index = ceil(p/100 * n) - 1, clamped into range.
    let rank = (sorted.len() * numerator).div_ceil(denominator);
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

// ── Reporting (the stable `DEVLOOP_NUMBERS` contract PR-7 consumes) ─────────

/// Cache disposition string for the `cache=` field, derived from the ingest
/// report. `hit` = warm; `cold`/`<reason>` = a loud miss; `na` = synthetic
/// (the cache is irrelevant — synthetic embed is instant).
fn cache_field(report: &corpus_harness::IngestReport, embedder: &str) -> String {
    if embedder == "synthetic" {
        return "na".to_string();
    }
    match &report.cache_miss_reason {
        None => "hit".to_string(),
        Some(reason) => reason.replace(' ', "_"),
    }
}

/// Emit the stable, parseable trend line. `recall` is `None` for the
/// latency-only AC-013 pass.
fn emit_numbers(
    ac: &str,
    n: usize,
    samples: usize,
    (p50, p99): (Duration, Duration),
    recall: Option<f64>,
    cache: &str,
    embedder: &str,
) {
    let recall_field = match recall {
        Some(r) => format!("{r:.4}"),
        None => "NA".to_string(),
    };
    eprintln!(
        "DEVLOOP_NUMBERS ac={ac} n={n} samples={samples} p50_ms={p50} p99_ms={p99} \
         recall_at_10={recall_field} cache={cache} embedder={embedder}",
        p50 = p50.as_millis(),
        p99 = p99.as_millis(),
    );
}

/// Notify-only soft-budget check for latency. Prints a loud WARN per breached
/// metric; never fails.
fn warn_if_over_latency(ac: &str, p50: Duration, p99: Duration) {
    if p50 > DEVLOOP_BUDGET_P50 {
        eprintln!(
            "DEVLOOP_PERF_WARN ac={ac} metric=p50 value_ms={} budget_ms={} status=OVER",
            p50.as_millis(),
            DEVLOOP_BUDGET_P50.as_millis(),
        );
    }
    if p99 > DEVLOOP_BUDGET_P99 {
        eprintln!(
            "DEVLOOP_PERF_WARN ac={ac} metric=p99 value_ms={} budget_ms={} status=OVER",
            p99.as_millis(),
            DEVLOOP_BUDGET_P99.as_millis(),
        );
    }
}

/// Notify-only soft-floor check for recall. Prints a loud WARN if under; never
/// fails.
fn warn_if_under_recall(ac: &str, recall: f64) {
    if recall < DEVLOOP_RECALL_FLOOR {
        eprintln!(
            "DEVLOOP_PERF_WARN ac={ac} metric=recall_at_10 value={recall:.4} floor={floor:.2} status=UNDER",
            floor = DEVLOOP_RECALL_FLOOR,
        );
    }
}

/// The ONE hard latency gate: a catastrophic ceiling at 10× the soft budget.
/// Routine small-N noise never clears it; the scanner-throughput regression
/// (orders-of-magnitude inflation) does. This is the RED-shows hook for
/// `53a270d`.
fn enforce_catastrophic_ceiling(ac: &str, p50: Duration, p99: Duration) {
    let ceil_p50 = DEVLOOP_BUDGET_P50 * DEVLOOP_CATASTROPHIC_MULT;
    let ceil_p99 = DEVLOOP_BUDGET_P99 * DEVLOOP_CATASTROPHIC_MULT;
    assert!(
        p50 <= ceil_p50,
        "DEVLOOP CATASTROPHIC ({ac}): p50={p50:?} > {ceil_p50:?} (10× soft budget) — \
         an orders-of-magnitude latency regression (e.g. projection-scanner throughput, \
         see 53a270d), not sample noise"
    );
    assert!(
        p99 <= ceil_p99,
        "DEVLOOP CATASTROPHIC ({ac}): p99={p99:?} > {ceil_p99:?} (10× soft budget) — \
         an orders-of-magnitude latency regression, not sample noise"
    );
}

// ── Shared setup ───────────────────────────────────────────────────────────

/// Open the engine, ingest, drain, and hard-assert the structural invariants
/// (batch-collapse RED-shows here). Returns the opened engine, the held-out
/// query set, the ingest report, and the embedder label.
fn setup() -> Option<(
    tempfile::TempDir,
    Engine,
    Vec<HeldOutQuery>,
    corpus_harness::IngestReport,
    &'static str,
)> {
    let (fx, embedder) = devloop_fixture();
    let (dir, engine) = fx.open_or_skip()?;
    let report = fx.ingest_into(&engine);
    engine.drain(15_000).expect("drain after ingest");

    // STRUCTURAL invariants — HARD assert (batch-collapse `4a95cfd` RED-shows
    // here: a collapsed batch leaves vector_default short of the ingested doc
    // count).
    fx.assert_vec0_row_count_matches_ingest(&engine);
    fx.assert_fts_index_populated(&engine);

    let queries = fx.query_set(DEVLOOP_SAMPLES, DEVLOOP_SEED);
    assert!(!queries.is_empty(), "devloop query_set produced no queries");
    Some((dir, engine, queries, report, embedder))
}

// ── AC-013 devloop — vector retrieval latency ──────────────────────────────

#[test]
fn ac_013_devloop() {
    let Some((_dir, engine, queries, report, embedder)) = setup() else {
        return;
    };

    // Warmup pass (discarded), then measure — mirrors the canonical protocol.
    for q in &queries {
        let _ = engine.search(&q.text).expect("warmup search");
    }
    let mut samples = Vec::with_capacity(queries.len());
    for q in &queries {
        let started = Instant::now();
        let _ = engine.search(&q.text).expect("measure search");
        samples.push(started.elapsed());
    }

    let p50 = percentile_ceil(&samples, 50, 100);
    let p99 = percentile_ceil(&samples, 99, 100);

    emit_numbers(
        "013",
        report.nodes,
        samples.len(),
        (p50, p99),
        None,
        &cache_field(&report, embedder),
        embedder,
    );

    // Latency gates apply on the SYNTHETIC path only — there `search` embed is
    // instant, so the measured latency isolates RETRIEVAL (the same reason
    // canonical AC-013 uses the synthetic embedder). On the real path the
    // numbers are report-only: the warmup pass primes the in-memory query
    // cache so the measure pass is embed-cache-warm, but tying retrieval gates
    // to a candle-dependent run would conflate embed and retrieval cost. Real
    // is the RECALL signal (see ac_013b_devloop); synthetic is the LATENCY one.
    if embedder == "synthetic" {
        warn_if_over_latency("013", p50, p99); // notify-only
        enforce_catastrophic_ceiling("013", p50, p99); // hard (scanner regression RED-shows here)
    } else {
        eprintln!(
            "DEVLOOP_PERF_INFO ac=013 metric=latency disposition=report_only embedder={embedder}"
        );
    }
}

// ── AC-013b devloop — recall@10 ANN fidelity (notify-only) ─────────────────

#[test]
fn ac_013b_devloop() {
    let Some((_dir, engine, queries, report, embedder)) = setup() else {
        return;
    };

    // f32 brute-force ground truth over the SAME model (ANN fidelity, NOT
    // IR-relevance) — mirrors the canonical AC-013b SQL (lib.rs:2317-2342):
    // rowid lookup against vector_default, body fetch against canonical_nodes
    // by write_cursor. The recall metric is `prod top-10 ∩ f32-GT top-10 / 10`.
    let gt_embedder = ground_truth_embedder(embedder == "real");
    let db_path = engine.path().to_path_buf();
    let conn = rusqlite::Connection::open(&db_path).expect("raw ground-truth conn");
    conn.pragma_update(None, "query_only", "ON").ok();

    let mut total_hits = 0usize;
    let mut total_queries = 0usize;
    for q in &queries {
        let vector = gt_embedder.embed(&q.text).expect("embed gt");
        let vector_json = serde_json::to_string(&vector).expect("json");

        let mut gt_rowid_stmt = conn
            .prepare(
                "SELECT rowid FROM vector_default WHERE embedding MATCH vec_f32(?1) \
                 ORDER BY distance LIMIT 10",
            )
            .expect("prepare gt rowid");
        let gt_rowids: Vec<i64> = gt_rowid_stmt
            .query_map([&vector_json], |row| row.get::<_, i64>(0))
            .expect("gt rowid query")
            .filter_map(Result::ok)
            .collect();

        let mut body_stmt = conn
            .prepare("SELECT body FROM canonical_nodes WHERE write_cursor = ?1 LIMIT 1")
            .expect("prepare body");
        let mut gt_bodies = Vec::with_capacity(gt_rowids.len());
        for rowid in &gt_rowids {
            if let Ok(body) = body_stmt.query_row([rowid], |row| row.get::<_, String>(0)) {
                gt_bodies.push(body);
            }
        }

        let prod: Vec<String> = engine
            .search(&q.text)
            .expect("measure search")
            .results
            .iter()
            .map(|h| h.body.clone())
            .collect();
        let gt_set: std::collections::HashSet<&String> = gt_bodies.iter().collect();
        total_hits += prod.iter().filter(|b| gt_set.contains(b)).count();
        total_queries += 1;
    }

    let recall = total_hits as f64 / (10.0 * total_queries.max(1) as f64);
    // recall is a quality signal, not a latency one — pass NA latency fields.
    emit_numbers(
        "013b",
        report.nodes,
        total_queries,
        (Duration::ZERO, Duration::ZERO),
        Some(recall),
        &cache_field(&report, embedder),
        embedder,
    );

    // The 0.85 floor is a REAL-embedder (dense BGE) ANN-fidelity number. The
    // synthetic `VaryingEmbedder` places only 6 non-zero coords in 768 dims, so
    // sign-bit quantization cannot separate documents and measured recall vs
    // exact-f32 is ~0.35 at N≈1000 — a property of the synthetic DATA, not a
    // regression (the same reason AC-019 is report-only on synthetic). So:
    //   - synthetic → REPORT-ONLY (emit the number for PR-7 trend tracking; no
    //     WARN, which would otherwise fire every run and train devs to ignore
    //     it). A recall *regression* still shows as a downward trend PR-7 flags.
    //   - real      → notify-only WARN against the 0.85 floor.
    // Structural invariants (asserted in setup()) remain the hard recall-path
    // gate — they RED-fail if the vector path drops rows (batch-collapse).
    if embedder == "real" {
        warn_if_under_recall("013b", recall);
    } else {
        eprintln!("DEVLOOP_PERF_INFO ac=013b metric=recall_at_10 disposition=report_only embedder=synthetic");
    }
}

// ── AC-019 devloop — mixed retrieval stress tail (report-only) ─────────────

#[test]
fn ac_019_devloop() {
    let Some((_dir, engine, queries, report, embedder)) = setup() else {
        return;
    };

    // Baseline pass (AC-013 protocol) immediately preceding the stress pass.
    for q in &queries {
        let _ = engine.search(&q.text).expect("baseline warmup");
    }
    let mut baseline = Vec::with_capacity(queries.len());
    for q in &queries {
        let started = Instant::now();
        let _ = engine.search(&q.text).expect("baseline measure");
        baseline.push(started.elapsed());
    }
    let baseline_p99 = percentile_ceil(&baseline, 99, 100);

    // Modest concurrent stress: DEVLOOP_AC019_THREADS readers, mixed FTS5 +
    // vector + canonical reads via the single `search()` path.
    let query_texts: Arc<Vec<String>> = Arc::new(queries.iter().map(|q| q.text.clone()).collect());
    let engine = Arc::new(engine);
    let barrier = Arc::new(Barrier::new(DEVLOOP_AC019_THREADS + 1));
    let sink: Arc<Mutex<Vec<Duration>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::with_capacity(DEVLOOP_AC019_THREADS);
    for tid in 0..DEVLOOP_AC019_THREADS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        let qs = Arc::clone(&query_texts);
        let sink = Arc::clone(&sink);
        handles.push(thread::spawn(move || {
            let mut local = Vec::with_capacity(DEVLOOP_AC019_QUERIES_PER_THREAD);
            let base = tid * DEVLOOP_AC019_QUERIES_PER_THREAD;
            barrier.wait();
            for i in 0..DEVLOOP_AC019_QUERIES_PER_THREAD {
                let q = &qs[(base + i) % qs.len()];
                let started = Instant::now();
                let _ = engine.search(q).expect("stress search");
                local.push(started.elapsed());
            }
            sink.lock().unwrap().extend(local);
        }));
    }
    let stress_started = Instant::now();
    barrier.wait();
    for h in handles {
        h.join().expect("stress thread");
    }
    let stress_elapsed = stress_started.elapsed();

    let stress_samples = std::mem::take(&mut *sink.lock().unwrap());
    let n_stress = stress_samples.len();
    let stress_p50 = percentile_ceil(&stress_samples, 50, 100);
    let stress_p99 = percentile_ceil(&stress_samples, 99, 100);

    // REPORT-ONLY (mirrors the canonical synthetic AC-019 disposition, HITL
    // 2026-06-01): synthetic isotropic data cannot meet the baseline-relative
    // `max(baseline_p99*N, floor)` bound, so the devloop does not assert it.
    // The verdict-quality AC-019 signal is the real-corpus canonical gate.
    //
    // The first line is the SAME keyed `DEVLOOP_NUMBERS` schema as AC-013 /
    // AC-013b (PR-7's stable contract): p50_ms/p99_ms carry the stress-tail
    // percentiles; recall_at_10=NA. The AC-019-specific detail (baseline,
    // thread shape, wall time, disposition) follows on a separate line so the
    // common contract stays uniform across all three ACs.
    emit_numbers(
        "019",
        report.nodes,
        n_stress,
        (stress_p50, stress_p99),
        None,
        &cache_field(&report, embedder),
        embedder,
    );
    eprintln!(
        "DEVLOOP_AC019_DETAIL ac=019 threads={threads} per_thread={per} stress_ms={se} \
         baseline_p99_ms={bp} stress_p50_ms={sp50} stress_p99_ms={sp99} disposition=report_only",
        threads = DEVLOOP_AC019_THREADS,
        per = DEVLOOP_AC019_QUERIES_PER_THREAD,
        se = stress_elapsed.as_millis(),
        bp = baseline_p99.as_millis(),
        sp50 = stress_p50.as_millis(),
        sp99 = stress_p99.as_millis(),
    );
}
