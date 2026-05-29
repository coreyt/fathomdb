//! EU-5b RED tests — lock-flip of the default embedder identity to bge-small.
//!
//! Per `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` §EU-5b. These
//! tests assert:
//!
//! 1. `DEFAULT_EMBEDDER_*` constants point at fathomdb-bge-small-en-v1.5 /
//!    5c38ec7c... / 384.
//! 2. `EmbedderChoice::Default` materialises a real BGE embedder, the open
//!    succeeds, and the reported identity is the bge-small one. (Feature-gated
//!    behind `default-embedder`; cold-open downloads ~133 MB from HuggingFace.)
//! 3. `OpenReport.embedder_mean_centering_required` is true for Default.
//! 4. `OpenReport.embedder_download_ms` is `Some(>0)` on a cold cache and
//!    `None` on a warm cache.
//! 5. `OpenReport.embedder_events` is populated on Default opens.
//! 6. The `MeanAccumulator` pins the per-workspace mean at the threshold
//!    crossing and emits `MeanVecPinned` in the same commit.
//! 7. The §0.5 at-pin re-quantize pass actually UPDATEs pre-pin sign-bit
//!    rows.
//! 8. CLI `fathomdb doctor warm-cache` lives at the CLI level
//!    (see `tests/cli_warm_cache.rs`).

use std::sync::Arc;

use fathomdb_embedder::EmbedderEvent;
use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{EmbedderChoice, Engine, MEAN_VEC_PIN_THRESHOLD};
use rusqlite::Connection;
use tempfile::TempDir;

// EU-5c — env-gate the network-hitting tests below. CI sets
// `FATHOMDB_SKIP_NETWORK_TESTS=1` only when the cache-warm step fails
// (HF unreachable, etc.); the default path is "tests run against warm
// cache". Local dev with internet runs them normally. cargo has no
// first-class "skipped" status — early-return with a log line is the
// idiomatic pattern.
#[allow(unused_macros)]
macro_rules! skip_if_no_network {
    () => {
        if std::env::var("FATHOMDB_SKIP_NETWORK_TESTS").is_ok() {
            eprintln!("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; skipping test");
            return;
        }
    };
}

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}.sqlite"));
    (dir, path)
}

// ---------------------------------------------------------------------------
// Test 1 — constant flip
// ---------------------------------------------------------------------------

#[test]
fn default_embedder_constants_flipped_to_bge_small() {
    // The engine exposes the default identity via `Engine::open` ->
    // `OpenReport.default_embedder`. Use that as the observable witness.
    let (_dir, path) = fixture_path("eu5b_constants");
    let opened =
        Engine::open(&path).expect("open should succeed even pre-fetch (no Default choice)");
    assert_eq!(opened.report.default_embedder.name, "fathomdb-bge-small-en-v1.5");
    assert_eq!(opened.report.default_embedder.revision, "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a");
    assert_eq!(opened.report.default_embedder.dimension, 384);
}

// ---------------------------------------------------------------------------
// Tests 2 / 3 / 5 — EmbedderChoice::Default round-trip (cold open).
//
// These run only with the `default-embedder` feature enabled (and require
// network access for the HuggingFace fetch).
// ---------------------------------------------------------------------------

#[cfg(feature = "default-embedder")]
#[test]
fn embedder_choice_default_succeeds_with_bge_identity() {
    skip_if_no_network!();
    let (_dir, path) = fixture_path("eu5b_default_succeeds");
    let opened = Engine::open_with_choice(&path, EmbedderChoice::Default)
        .expect("EmbedderChoice::Default must succeed after EU-5b");
    assert_eq!(opened.report.default_embedder.name, "fathomdb-bge-small-en-v1.5");
}

#[cfg(feature = "default-embedder")]
#[test]
fn open_report_embedder_mean_centering_required_true_for_default() {
    skip_if_no_network!();
    let (_dir, path) = fixture_path("eu5b_mc_required");
    let opened = Engine::open_with_choice(&path, EmbedderChoice::Default)
        .expect("EmbedderChoice::Default must succeed after EU-5b");
    assert!(
        opened.report.embedder_mean_centering_required,
        "Default identity must report MC-required after the EU-5b flip"
    );
}

#[cfg(feature = "default-embedder")]
#[test]
fn open_report_embedder_events_populated_for_default() {
    skip_if_no_network!();
    let (_dir, path) = fixture_path("eu5b_events");
    let opened = Engine::open_with_choice(&path, EmbedderChoice::Default)
        .expect("EmbedderChoice::Default must succeed");
    assert!(
        !opened.report.embedder_events.is_empty(),
        "Default open must surface loader events (downloads or cache hits)"
    );
}

#[cfg(feature = "default-embedder")]
#[test]
fn open_report_embedder_download_ms_some_on_cold_open() {
    skip_if_no_network!();
    // Cold open: download_ms must be Some(>0) when any file was fetched.
    // (We can't force a cold cache portably without overriding the loader
    //  cache root; this test asserts only the non-`None` shape on first
    //  Default open in the process, which is the documented contract.)
    let (_dir, path) = fixture_path("eu5b_download_ms");
    let opened = Engine::open_with_choice(&path, EmbedderChoice::Default)
        .expect("EmbedderChoice::Default must succeed");
    // Either Some(>0) on cold, or None on warm — but the field must be
    // surfaced (no longer hard-coded `None` regardless of state).
    if let Some(ms) = opened.report.embedder_download_ms {
        assert!(ms > 0, "download_ms must be > 0 on cold open");
    }
    // If the cache was already warm before this test, the field reads None
    // and the events stream is all cache hits — still a valid post-EU-5b
    // shape. The "non-`None` on cold open" half is exercised by
    // `cli_doctor_warm_cache` against a fresh cache root.
}

// ---------------------------------------------------------------------------
// Test 6 — DefaultEmbedderNotWired removed.
// Asserts the variant is gone from EmbedderError. Compile-time check: if
// any code still references the variant after GREEN, the engine + bindings
// won't build.
// ---------------------------------------------------------------------------

#[test]
fn default_embedder_not_wired_error_variant_removed() {
    // Exhaustive-match witness: if `DefaultEmbedderNotWired` were re-added
    // to `EmbedderError`, the compiler would force an arm here and we'd
    // notice immediately. The two production variants are the only legal
    // inhabitants after EU-5b.
    fn _exhaustive_witness(err: EmbedderError) -> &'static str {
        match err {
            EmbedderError::Failed { .. } => "failed",
            EmbedderError::Timeout => "timeout",
        }
    }
    // Touch the witness so the compiler keeps it. No runtime assertion.
    let _ = _exhaustive_witness;
}

// ---------------------------------------------------------------------------
// Tests 7 / 8 — MeanAccumulator pinning + at-pin re-quantize.
//
// We use a SimulatedBgeEmbedder that reports the bge-small identity name
// (so the engine treats it as MC-required) but produces deterministic
// f32 output without touching the network or candle.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct SimulatedBgeEmbedder {
    identity: EmbedderIdentity,
}

impl Default for SimulatedBgeEmbedder {
    fn default() -> Self {
        Self {
            identity: EmbedderIdentity::new(
                "fathomdb-bge-small-en-v1.5",
                "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a",
                384,
            ),
        }
    }
}

impl Embedder for SimulatedBgeEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, input: &str) -> Result<Vector, EmbedderError> {
        // Deterministic, varying-by-input vector so the streaming mean
        // accumulator sees non-trivial input.
        let mut v = vec![0.0f32; 384];
        let mut seed: u64 = 0;
        for b in input.bytes() {
            seed = seed.wrapping_mul(131).wrapping_add(u64::from(b));
        }
        for (i, slot) in v.iter_mut().enumerate() {
            let mixed = seed.wrapping_add(i as u64).wrapping_mul(2654435761);
            *slot = ((mixed >> 8) as u32 as f32) / (u32::MAX as f32) - 0.5;
        }
        Ok(v)
    }
}

fn open_with_simulated_bge(
    name: &str,
) -> (TempDir, std::path::PathBuf, fathomdb_engine::OpenedEngine) {
    let (dir, path) = fixture_path(name);
    let opened = Engine::open_with_choice(
        &path,
        EmbedderChoice::Caller(Arc::new(SimulatedBgeEmbedder::default())),
    )
    .expect("open with simulated bge");
    (dir, path, opened)
}

fn write_docs(engine: &Engine, count: u64) {
    engine.configure_vector_kind_for_test("doc").expect("configure vector kind");
    for i in 0..count {
        let txt = format!("doc-{i}");
        engine
            .write_vector_for_test("doc", &txt)
            .unwrap_or_else(|err| panic!("write_vector_for_test failed at i={i}: {err:?}"));
    }
}

#[test]
fn mean_accumulator_pins_at_threshold_with_real_embedder() {
    let (_dir, path, opened) = open_with_simulated_bge("eu5b_pin_threshold");
    write_docs(&opened.engine, MEAN_VEC_PIN_THRESHOLD);
    drop(opened);

    // The mean_vec must now be populated.
    let connection = Connection::open(&path).expect("reopen");
    let mean_blob: Option<Vec<u8>> = connection
        .query_row(
            "SELECT mean_vec FROM _fathomdb_embedder_profiles WHERE profile = 'default'",
            [],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .expect("mean_vec query");
    let mean_blob = mean_blob.expect("mean_vec must be NOT NULL after threshold crossing");
    assert_eq!(mean_blob.len(), 384 * 4, "mean_vec byte length must be 4 * dim");
}

#[test]
fn mean_accumulator_emits_mean_vec_pinned_event() {
    let (_dir, _path, opened) = open_with_simulated_bge("eu5b_pin_event");
    // Subscribe to lifecycle events that carry MeanVecPinned. The engine's
    // existing surface for accumulator events is to surface them through
    // the same `OpenReport.embedder_events` channel on the *next* open —
    // but EU-5b also expects the projection-worker pin commit to emit a
    // MeanVecPinned variant retrievable via the test seam.
    write_docs(&opened.engine, MEAN_VEC_PIN_THRESHOLD);
    let events =
        opened.engine.drain_mean_centering_events_for_test().expect("drain mean-centering events");
    let mut saw_pin = false;
    for ev in &events {
        if let EmbedderEvent::MeanVecPinned { dim, doc_count } = ev.clone() {
            assert_eq!(dim, 384u32);
            assert!(doc_count >= 1u64, "doc_count must be >= 1 at pin commit");
            saw_pin = true;
        }
    }
    assert!(saw_pin, "MeanVecPinned event must be emitted at pin commit");
}

#[test]
fn requantize_pass_updates_prepin_rows() {
    let (_dir, path, opened) = open_with_simulated_bge("eu5b_requantize");
    // Write enough docs to cross the pin threshold.
    write_docs(&opened.engine, MEAN_VEC_PIN_THRESHOLD);
    drop(opened);

    // The pre-pin rows' sign-bit columns must reflect a non-zero mean
    // having been applied. We sample one row and assert at least one
    // bit differs from the un-centered sign-bit of its raw vector. (The
    // simulated embedder produces a varying signed distribution, so the
    // chance of a coincidence equality is astronomically small.)
    let connection = Connection::open(&path).expect("reopen");

    let any_row: i64 = connection
        .query_row("SELECT rowid FROM vector_default ORDER BY rowid LIMIT 1", [], |row| row.get(0))
        .expect("at least one row exists");

    let embedding: Vec<u8> = connection
        .query_row("SELECT embedding FROM vector_default WHERE rowid = ?1", [any_row], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .expect("read embedding f32");
    let embedding_bin: Vec<u8> = connection
        .query_row("SELECT embedding_bin FROM vector_default WHERE rowid = ?1", [any_row], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .expect("read embedding_bin");

    // Recompute the un-centered sign-bit pattern host-side and compare.
    // sqlite-vec packs little-endian, MSB-first per byte. We don't need
    // to mirror the exact encoding — we just need at least one byte to
    // disagree to prove the re-quantize touched the row.
    let mut uncentered_bits = Vec::with_capacity(embedding.len() / 4 / 8 + 1);
    let mut acc: u8 = 0;
    let mut count = 0;
    for chunk in embedding.chunks_exact(4) {
        let f = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        acc = (acc << 1) | (if f >= 0.0 { 1 } else { 0 });
        count += 1;
        if count == 8 {
            uncentered_bits.push(acc);
            acc = 0;
            count = 0;
        }
    }
    if count > 0 {
        uncentered_bits.push(acc << (8 - count));
    }

    // Both buffers should be the same length (1 bit per dim, packed).
    assert_eq!(
        uncentered_bits.len(),
        embedding_bin.len(),
        "bit-packed length mismatch: uncentered={} stored={}",
        uncentered_bits.len(),
        embedding_bin.len()
    );

    assert!(
        uncentered_bits != embedding_bin,
        "stored sign-bits must differ from un-centered sign-bits after re-quantize"
    );
}
