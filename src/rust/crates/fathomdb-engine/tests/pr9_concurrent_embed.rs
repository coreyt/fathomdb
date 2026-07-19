//! PR-9 item 1 — sustained real-corpus seed through the production
//! projection path (now engine-serialized).
//!
//! `PROJECTION_WORKERS = 2` workers drive the real `CandleBgeEmbedder`. The
//! PR-9 pre-flight established that letting the two workers call the shared
//! `BertModel::forward` *concurrently* does not wedge or corrupt. PR-9 still
//! serializes the embed engine-side (`lib.rs::run_projection_job`'s
//! `embed_serialize` guard, commit/IO stays parallel) — for SAFETY with
//! arbitrary caller-supplied embedders, not throughput (candle fans every
//! forward onto one process-wide rayon pool, so serialization is
//! throughput-neutral on the candle default). The fast mechanism test for
//! that guard is `pr9_embed_serialization.rs`.
//!
//! This test is the end-to-end *guard + measurement*: it seeds ≥10K real
//! docs through the serialized production path and asserts:
//!   * `drain` returns (the serialized seed completes in bounded time),
//!   * every seeded row produced a vector (no drops),
//!   * the projection reaches `UpToDate`, and
//!   * stored vectors are finite + unit-norm, with a direct spot-check that
//!     the first rows' stored vectors equal a single-threaded re-embed.
//!
//! It also logs the serialized embed rate (`PR9_PROGRESS` / `PR9_DRAINED`)
//! for the record. NOTE: run in `--release`; in a debug build a 512-token
//! candle forward is ~14x slower (PR-9 micro-benchmark), so a 10K seed of
//! long corpus docs takes hours in debug. Lower `PR9_SEED_N` for a quick
//! debug end-to-end.
//!
//! Opt-in (real candle weights, slow ~45 min at 10K): requires the
//! `default-embedder` feature AND `AGENT_LONG=1`:
//!   AGENT_LONG=1 cargo test -p fathomdb-engine --features default-embedder \
//!     --test pr9_concurrent_embed -- --nocapture
//! Override the seed size with `PR9_SEED_N` (default 10000).

#![cfg(feature = "default-embedder")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fathomdb_embedder::CandleBgeEmbedder;
use fathomdb_embedder_api::Embedder;
use fathomdb_engine::lifecycle::ProjectionStatus;
use fathomdb_engine::{EmbedderChoice, Engine, PreparedWrite};
use tempfile::TempDir;

#[path = "support/corpus_subset.rs"]
mod corpus_subset;
use corpus_subset::load_subset_or_skip;

const DEFAULT_SEED_N: usize = 10_000;
const WRITE_BATCH: usize = 256;
// Generous ceiling so the (real, serialized) seed completes rather than
// being cut off: release embeds are ~14ms short / ~960ms for a 512-token doc
// (PR-9 micro-benchmark), so a 10K corpus-mix seed is well under this. A
// genuine wedge would surface as drain RETURNING Err, not as a timeout.
const DRAIN_TIMEOUT_MS: u64 = 90 * 60 * 1000;

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb).max(1e-12)
}

#[test]
fn sustained_seed_serialized_path_completes_and_is_correct() {
    if std::env::var_os("AGENT_LONG").is_none() {
        eprintln!("[skip] AGENT_LONG not set; PR-9 sustained-seed measurement is opt-in");
        return;
    }
    if std::env::var("FATHOMDB_SKIP_NETWORK_TESTS").is_ok() {
        eprintln!("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; embedder cache unavailable");
        return;
    }
    let Some(docs) = load_subset_or_skip(usize::MAX) else {
        eprintln!("[skip] corpus not present; cannot run PR-9 concurrent-embed seed");
        return;
    };

    // Build the ≥N-body haystack by cycling the real corpus (duplicates are
    // fine: they still drive the full serialized embed→commit pipeline and let
    // us spot-check determinism).
    let target_n = env_usize("PR9_SEED_N", DEFAULT_SEED_N);
    let real_bodies: Vec<String> =
        docs.iter().map(|d| d.body.clone()).filter(|b| !b.trim().is_empty()).collect();
    assert!(!real_bodies.is_empty(), "corpus yielded no non-empty bodies");
    let bodies: Vec<String> =
        (0..target_n).map(|i| real_bodies[i % real_bodies.len()].clone()).collect();
    eprintln!("PR9_SETUP target_n={target_n} real_docs={} (cycled to fill)", real_bodies.len());

    // Bare CandleBgeEmbedder — NO harness-side SerializedBge wrapper: embed
    // serialization is now the engine's job (`embed_serialize`), so this
    // exercises the real production path. Reused below as the single-threaded
    // ground-truth encoder for the determinism spot-check.
    let embedder: Arc<dyn Embedder> =
        Arc::new(CandleBgeEmbedder::new().expect("construct real bge embedder"));

    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("pr9_concurrent.sqlite");
    let opened = Engine::open_with_choice(&path, EmbedderChoice::Caller(embedder.clone()))
        .expect("open with real bge embedder");
    let engine = Arc::new(opened.engine);
    assert_eq!(
        opened.report.default_embedder.name, "fathomdb-bge-small-en-v1.5",
        "must run against the real bge-small identity"
    );
    engine.configure_vector_kind_for_test("doc").expect("configure vector kind");

    // Write the whole haystack WITHOUT draining between batches so the
    // projection workers stay fed throughout (embeds run one at a time behind
    // embed_serialize) — a single end-of-seed drain proves the pool reaches
    // idle.
    let started = Instant::now();
    let mut written = 0usize;
    while written < bodies.len() {
        let take = WRITE_BATCH.min(bodies.len() - written);
        let batch: Vec<PreparedWrite> = bodies[written..written + take]
            .iter()
            .map(|b| PreparedWrite::Node {
                kind: "doc".to_string(),
                body: b.clone(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
            })
            .collect();
        engine.write(&batch).expect("seed write");
        written += take;
    }
    eprintln!("PR9_WROTE n={written} enqueue_s={}", started.elapsed().as_secs());

    // Progress monitor: sample committed vector row count via the engine's
    // own seam every 20s while `drain` blocks. This is the wedge-vs-slow
    // discriminator — a healthy seed climbs steadily to `written`; a true
    // wedge plateaus (workers park, CPU → 0).
    // We log each sample + the per-interval delta; a zero delta for several
    // consecutive intervals is the wedge signature.
    let monitor_engine = engine.clone();
    let monitor_done = Arc::new(AtomicBool::new(false));
    let monitor_flag = monitor_done.clone();
    let monitor = thread::spawn(move || {
        let started = Instant::now();
        let mut prev = 0u64;
        let mut stalls = 0u32;
        while !monitor_flag.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(20));
            let count = monitor_engine.vector_row_count_for_test().unwrap_or(u64::MAX);
            let delta = count.saturating_sub(prev);
            if delta == 0 {
                stalls += 1;
            } else {
                stalls = 0;
            }
            eprintln!(
                "PR9_PROGRESS elapsed_s={} vector_rows={count} delta={delta} stalls={stalls}",
                started.elapsed().as_secs()
            );
            prev = count;
        }
    });

    let drain_started = Instant::now();
    let drained = engine.drain(DRAIN_TIMEOUT_MS);
    monitor_done.store(true, Ordering::Relaxed);
    let _ = monitor.join();
    eprintln!("PR9_DRAINED ok={} drain_s={}", drained.is_ok(), drain_started.elapsed().as_secs());
    assert!(
        drained.is_ok(),
        "serialized seed must complete drain in bounded time; got {drained:?}"
    );

    // Completion: the projection must reach UpToDate and every row vectorized.
    assert_eq!(
        engine.projection_status_for_test("doc").expect("projection status"),
        ProjectionStatus::UpToDate,
        "projection must reach UpToDate after the concurrent seed"
    );
    let row_count = engine.vector_row_count_for_test().expect("vector row count");
    assert_eq!(
        row_count as usize, written,
        "every seeded doc must produce exactly one vector row (no concurrency drops)"
    );

    // Corruption guard: every stored vector must be finite + unit-norm. A
    // forward()-state data race would yield NaN/Inf or off-norm vectors.
    let dim = opened.report.default_embedder.dimension as usize;
    let mut checked = 0u64;
    for rowid in 1..=row_count as i64 {
        let blob = engine.read_vector_blob_for_test(rowid).expect("read vector blob");
        let v: Vec<f32> =
            blob.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        assert_eq!(v.len(), dim, "row {rowid} vector has wrong dimension");
        assert!(v.iter().all(|x| x.is_finite()), "row {rowid} vector has non-finite values");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-3,
            "row {rowid} vector not unit-norm (got {norm}); concurrency corruption suspected"
        );
        checked += 1;
    }
    eprintln!("PR9_NORM_OK checked={checked} dim={dim}");

    // Determinism spot-check: the first rows' stored vectors must equal a
    // single-threaded re-embed of the same body (rowids are assigned in
    // write order, so rowid i ↔ bodies[i-1]). A race that swapped/garbled a
    // forward pass would break the cosine match.
    let spot = (row_count as usize).min(16);
    for i in 0..spot {
        let rowid = (i + 1) as i64;
        let blob = engine.read_vector_blob_for_test(rowid).expect("read vector blob");
        let stored: Vec<f32> =
            blob.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let expected = embedder.embed(&bodies[i]).expect("re-embed for spot-check");
        let cos = cosine(&stored, &expected);
        assert!(
            cos > 0.9999,
            "row {rowid} stored vector diverges from single-threaded re-embed (cos={cos}); \
             concurrent-embed produced a wrong vector"
        );
    }
    eprintln!("PR9_SPOTCHECK_OK rows={spot} verdict=SERIALIZED_SEED_COMPLETE_AND_CORRECT");
}
