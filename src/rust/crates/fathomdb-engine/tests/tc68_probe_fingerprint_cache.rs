//! 0.8.20 Slice 22 (R-20-VC) **TC-68** — the 0.8.18 vector-equivalence probe is
//! CACHED against an embedder-identity fingerprint, so `Engine::open` stops
//! paying the 45-probe re-embed on every single open.
//!
//! Authority: `dev/plans/plan-0.8.20.md` §3 row `R-20-VC` (TC-68) and
//! `dev/design/0.8.20-tc68-equivalence-probe-fingerprint-cache.md`. The probe
//! itself is `dev/design/0.8.18-slice-5-vector-equivalence-probe.md` +
//! `dev/adr/ADR-0.8.18-vector-equivalence-self-check.md` (ACCEPTED, HITL-signed).
//!
//! ## MEASURED baseline (this file's characterization tests pin it)
//!
//! Taken at `94bb33ef` with a counting embedder, snapshotting `embed` calls
//! across `Engine::open`:
//!
//! | workspace state                   | open 1 | open 2 | open 3 |
//! |-----------------------------------|--------|--------|--------|
//! | zero enrolled vector kinds        | 0      | 0      | —      |
//! | ONE enrolled kind                 | **90** | **45** | **45** |
//! | SIX enrolled kinds                | **90** | **45** | **45** |
//!
//! So both figures in circulation are true, of different opens: **90** is the
//! one-time POPULATION open (45 embeds to persist the baseline + 45 more for the
//! fix-2 "confirm the just-written baseline" check), and **45** is EVERY open
//! thereafter, forever. Cost is already independent of the enrolled-kind count
//! (the probe gate is `SELECT EXISTS(...)`, not a count, and the probe body never
//! iterates kinds) — so a test that varies the kind count is VACUOUS. The real
//! cost is the per-open 45, and that is what TC-68 removes.
//!
//! ## The three acceptance points
//!
//! 1. a second/subsequent open whose fingerprint is UNCHANGED does **zero** probe
//!    embeds;
//! 2. an open whose fingerprint CHANGED re-runs the **full** probe;
//! 3. fail-SAFE (`R-VEQ-4`, HITL-signed) is untouched — an unreadable/garbled
//!    cache falls back to RUNNING the probe (never to trusting it), and a
//!    divergence still yields `dense_disabled`.
//!
//! ## The residual, asserted rather than buried
//!
//! Caching against a fingerprint means SAME-identity backend drift (candle
//! CPU↔CUDA, a library/driver change) is no longer caught on every open. That is
//! the ruled trade, and
//! `residual_same_identity_backend_drift_is_not_caught_on_a_cached_open` states it
//! as an executable fact.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, EngineError};
use tempfile::TempDir;

const DIM: usize = 384;
const PROBE_IDENTITY_NAME: &str = "fathomdb-probe-test";
const PROBE_IDENTITY_REV: &str = "veq-tc68";

/// The ONLY identity for which `identity_requires_mean_centering` is true, so it
/// is the only way to reach the production P1 mean-centred branch — and therefore
/// the only END-TO-END way to change the probe's `mean_vec` fingerprint input.
const BGE_NAME: &str = "fathomdb-bge-small-en-v1.5";
const BGE_REV: &str = "veq-tc68-mc";

/// The full committed probe fixture size (`src/vector_equivalence_probes.txt`).
const PROBE_COUNT: u64 = 45;

/// Deterministic per-text reference vector, every component in `[0.5, 1.5)` —
/// strictly positive, so the raw 1-bit sign quantization is all-ones and sits
/// robustly away from the zero threshold. Mirrors `vector_equivalence_probe.rs`.
fn reference_vector(text: &str) -> Vec<f32> {
    let mut out = Vec::with_capacity(DIM);
    for i in 0..DIM {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ (i as u64).wrapping_mul(0x0100_0000_01b3);
        for b in text.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        let frac = (h % 1000) as f32 / 1000.0;
        out.push(0.5 + frac);
    }
    out
}

/// Faithful backend that COUNTS every `embed` call. The count is the whole point
/// of this file: TC-68 is a cost claim, and only an actual call count can falsify
/// it (a log line or a wall-clock timing cannot).
#[derive(Debug)]
struct CountingRefEmbedder {
    calls: Arc<AtomicU64>,
}
impl Embedder for CountingRefEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(PROBE_IDENTITY_NAME, PROBE_IDENTITY_REV, DIM as u32)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(reference_vector(text))
    }
}

/// SAME identity, deliberately divergent (negates every component): every P1 sign
/// flips AND the P2 un-centred L2 is large. This is the "same-identity backend
/// drift" the 0.8.18 probe exists to catch.
#[derive(Debug)]
struct CountingDivergentEmbedder {
    calls: Arc<AtomicU64>,
}
impl Embedder for CountingDivergentEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(PROBE_IDENTITY_NAME, PROBE_IDENTITY_REV, DIM as u32)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(reference_vector(text).into_iter().map(|x| -x).collect())
    }
}

/// Faithful bge-identity backend (mean-centring REQUIRED), counting.
#[derive(Debug)]
struct CountingBgeRefEmbedder {
    calls: Arc<AtomicU64>,
}
impl Embedder for CountingBgeRefEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(BGE_NAME, BGE_REV, DIM as u32)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(reference_vector(text))
    }
}

fn db_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("tc68.sqlite")
}

fn counter() -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(0))
}

/// A constant mean vector, LE-f32 encoded (`4 * dim` bytes — the engine's
/// `mean_vec` blob shape).
fn mean_blob(value: f32) -> Vec<u8> {
    let mut blob = Vec::with_capacity(DIM * 4);
    for _ in 0..DIM {
        blob.extend_from_slice(&value.to_le_bytes());
    }
    blob
}

/// Pin `_fathomdb_embedder_profiles.mean_vec` for the default profile via a raw
/// connection while the engine is closed (no public seam pins a mean without
/// writing ≥256 vector rows).
fn set_pinned_mean(path: &std::path::Path, mean: Vec<u8>) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "UPDATE _fathomdb_embedder_profiles SET mean_vec = ?1 WHERE profile = 'default'",
        rusqlite::params![mean],
    )
    .expect("pin mean_vec");
}

/// Open, snapshot the embed-call delta across the open, close. Returns
/// `(embeds_during_open, dense_disabled)`.
fn open_count_close(
    path: &std::path::Path,
    embedder: Arc<dyn Embedder>,
    calls: &Arc<AtomicU64>,
) -> (u64, bool) {
    let before = calls.load(Ordering::SeqCst);
    let opened = Engine::open_with_embedder_for_test(path, embedder).expect("open must succeed");
    let embeds = calls.load(Ordering::SeqCst) - before;
    let dense_disabled = opened.report.dense_disabled;
    opened.engine.close().expect("close");
    (embeds, dense_disabled)
}

/// Enrol a vector kind under the non-bge probe identity. The probe is gated on a
/// vector kind existing AT open, so this session is probe-inert.
fn enrol_kind(path: &std::path::Path, calls: &Arc<AtomicU64>, kind: &str) {
    let opened = Engine::open_with_embedder_for_test(
        path,
        Arc::new(CountingRefEmbedder { calls: calls.clone() }),
    )
    .expect("enrolment open");
    opened.engine.configure_vector_kind_for_test(kind).expect("enrol vector kind");
    opened.engine.close().expect("close enrolment session");
}

/// Bring a non-bge workspace to the state "baseline persisted AND verified" —
/// i.e. exactly the state from which every later open should be free.
/// Session 1 enrols the kind (probe inert); session 2 populates + confirms (90).
fn seed_verified_workspace(path: &std::path::Path, calls: &Arc<AtomicU64>) {
    enrol_kind(path, calls, "note");
    let (embeds, degraded) =
        open_count_close(path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), calls);
    assert_eq!(embeds, 2 * PROBE_COUNT, "the POPULATION open is 45 persist + 45 confirm");
    assert!(!degraded, "a faithful population open is never degraded");
}

// ---- characterization: the MEASURED baseline (§ module docs) ----------------

/// A workspace with NO enrolled vector kind pays nothing, on any open. Already
/// true at `94bb33ef`; pinned so the cache cannot regress it.
#[test]
fn measured_zero_enrolled_kinds_costs_zero_probe_embeds() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();

    let (first, _) =
        open_count_close(&path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), &calls);
    let (second, _) =
        open_count_close(&path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), &calls);

    assert_eq!(first, 0, "no vector kind ⇒ no dense arm to guard ⇒ no probe embeds");
    assert_eq!(second, 0, "…and the same on reopen");
}

/// The POPULATION open costs 90 (45 persist + 45 confirm) and — the point — that
/// figure does NOT move with the number of enrolled kinds. The plan's DoD sentence
/// ("open cost independent of enrolled-kind count") is therefore already satisfied
/// VACUOUSLY at `94bb33ef`; this test exists to say so out loud so nobody mistakes
/// it for the TC-68 acceptance signal. The real acceptance is
/// `verified_reopen_performs_zero_probe_embeds`.
#[test]
fn measured_population_open_cost_is_flat_in_the_enrolled_kind_count() {
    let one_dir = TempDir::new().unwrap();
    let one_path = db_path(&one_dir);
    let one_calls = counter();
    enrol_kind(&one_path, &one_calls, "note");
    let (one_kind_open, _) = open_count_close(
        &one_path,
        Arc::new(CountingRefEmbedder { calls: one_calls.clone() }),
        &one_calls,
    );

    let many_dir = TempDir::new().unwrap();
    let many_path = db_path(&many_dir);
    let many_calls = counter();
    {
        let opened = Engine::open_with_embedder_for_test(
            &many_path,
            Arc::new(CountingRefEmbedder { calls: many_calls.clone() }),
        )
        .expect("enrolment open");
        for kind in ["note", "email", "article", "paper", "meeting", "todo"] {
            opened.engine.configure_vector_kind_for_test(kind).expect("enrol vector kind");
        }
        opened.engine.close().expect("close");
    }
    let (six_kind_open, _) = open_count_close(
        &many_path,
        Arc::new(CountingRefEmbedder { calls: many_calls.clone() }),
        &many_calls,
    );

    assert_eq!(one_kind_open, 2 * PROBE_COUNT, "population open = 45 persist + 45 confirm");
    assert_eq!(six_kind_open, one_kind_open, "probe cost never scaled with the kind count");
}

// ---- acceptance 1 — an unchanged fingerprint costs ZERO --------------------

/// **THE TC-68 SIGNAL.** Once the probe has verified this workspace, every later
/// open whose fingerprint is unchanged must do NO probe embeds at all.
/// At `94bb33ef` this is 45 per open, forever.
#[test]
fn verified_reopen_performs_zero_probe_embeds() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();
    seed_verified_workspace(&path, &calls);

    let (third, third_degraded) =
        open_count_close(&path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), &calls);
    let (fourth, fourth_degraded) =
        open_count_close(&path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), &calls);

    assert_eq!(third, 0, "a verified workspace must not re-embed the probe set on reopen");
    assert_eq!(fourth, 0, "…and the cached verdict must persist, not be one-shot");
    assert!(!third_degraded, "the cached verdict is PASS, so dense stays enabled");
    assert!(!fourth_degraded, "…on every subsequent open too");
}

// ---- acceptance 2 — a CHANGED fingerprint re-runs the full probe -----------

/// The pinned `mean_vec` is a fingerprint input because the probe's P1 check
/// quantizes against the LIVE mean (`vec_quantize_binary(sign(x − mean_vec))`), so
/// rewriting the mean changes the verdict's meaning. Unchanged ⇒ 0 embeds;
/// rewritten ⇒ the FULL 45-probe re-run.
#[test]
fn a_rewritten_pinned_mean_reruns_the_full_probe() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();

    // Session 1 — create the bge profile + enrol the kind (probe inert).
    {
        let opened = Engine::open_with_embedder_for_test(
            &path,
            Arc::new(CountingBgeRefEmbedder { calls: calls.clone() }),
        )
        .expect("bge enrolment open");
        opened.engine.configure_vector_kind_for_test("note").expect("enrol vector kind");
        opened.engine.close().expect("close");
    }
    // Pin the mean BEFORE the baseline is captured so the MC gate engages.
    set_pinned_mean(&path, mean_blob(1.0));

    // Session 2 — POPULATION (45 persist + 45 confirm) under mean = 1.0.
    let (population, _) =
        open_count_close(&path, Arc::new(CountingBgeRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(population, 2 * PROBE_COUNT, "population open under a pinned mean");

    // Session 3 — nothing changed ⇒ the cached verdict answers.
    let (cached, cached_degraded) =
        open_count_close(&path, Arc::new(CountingBgeRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(cached, 0, "unchanged fingerprint ⇒ zero probe embeds");
    assert!(!cached_degraded);

    // Rewrite the pinned mean. The stored UN-centred references are untouched, so
    // a faithful backend still passes — but the P1 verdict is now computed against
    // a DIFFERENT mean, so the cached verdict is stale and must not be trusted.
    set_pinned_mean(&path, mean_blob(0.9));

    let (after_mean_change, after_degraded) =
        open_count_close(&path, Arc::new(CountingBgeRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(
        after_mean_change, PROBE_COUNT,
        "a rewritten mean_vec must re-run the FULL probe, not reuse the cached verdict"
    );
    assert!(!after_degraded, "a faithful backend still passes the re-run");

    // …and the re-run's verdict is itself cached under the new mean.
    let (recached, _) =
        open_count_close(&path, Arc::new(CountingBgeRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(recached, 0, "the fresh verdict must be cached under the new fingerprint");
}

/// The stored baseline is the other end-to-end-reachable fingerprint input. A
/// tampered `reference_vec` (length preserved, so the shipped completeness check
/// cannot see it) must invalidate the cached verdict, re-run the probe, and be
/// CAUGHT — this is the 0.8.18 fix-2 external-tamper closure, which the cache must
/// not re-open.
#[test]
fn a_tampered_reference_baseline_reruns_the_probe_and_refuses_dense() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();
    seed_verified_workspace(&path, &calls);

    let (cached, _) =
        open_count_close(&path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(cached, 0, "verified workspace reopens free");

    // Flip the sign of every component of ONE stored reference (same byte length,
    // so the row-shape completeness validation still passes).
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let blob: Vec<u8> = conn
            .query_row(
                "SELECT reference_vec FROM _fathomdb_embed_probe WHERE probe_ordinal = 0",
                [],
                |r| r.get(0),
            )
            .expect("read reference 0");
        let flipped: Vec<u8> = blob
            .chunks_exact(4)
            .flat_map(|c| {
                let v = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                (-v).to_le_bytes()
            })
            .collect();
        assert_eq!(flipped.len(), blob.len(), "tamper must preserve the blob length");
        conn.execute(
            "UPDATE _fathomdb_embed_probe SET reference_vec = ?1 WHERE probe_ordinal = 0",
            rusqlite::params![flipped],
        )
        .expect("tamper reference 0");
    }

    let before = calls.load(Ordering::SeqCst);
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingRefEmbedder { calls: calls.clone() }),
    )
    .expect("open must SUCCEED (degraded), never fail");
    let embeds = calls.load(Ordering::SeqCst) - before;
    assert_eq!(embeds, PROBE_COUNT, "a mutated baseline must force a full re-run");
    assert!(
        opened.report.dense_disabled,
        "the tampered reference diverges ⇒ dense must be REFUSED (fail-safe, R-VEQ-4)"
    );
    match opened.engine.search("memory") {
        Err(EngineError::VectorEquivalenceMismatch { .. }) => {}
        other => panic!("dense arm must refuse with VectorEquivalenceMismatch, got {other:?}"),
    }
    opened.engine.close().unwrap();
}

// ---- acceptance 3 — fail-SAFE: a bad cache RUNS the probe, never trusts it --

/// `R-VEQ-4` (HITL-signed) in the cache path: a cache entry that cannot be read as
/// a valid verdict must fall back to RUNNING the probe. It must never be treated
/// as a pass, and it must never degrade the open by itself.
#[test]
fn an_unreadable_cache_entry_falls_back_to_running_the_probe() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();
    seed_verified_workspace(&path, &calls);

    let (cached, _) =
        open_count_close(&path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(cached, 0, "verified workspace reopens free");

    // Garble every `_fathomdb_open_state` value that is not a known non-probe
    // marker: whatever key TC-68 chose, its cached verdict is now unreadable.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE _fathomdb_open_state SET value = 'not-a-fingerprint'
             WHERE key NOT IN ('projection_cursor',
                               'search_index_tokenizer_reproject_complete',
                               'tc33_edge_vector_prune_complete',
                               'tc33_reserved_write_cursor')",
            [],
        )
        .expect("garble the cached verdict");
    }

    let (after_garble, degraded) =
        open_count_close(&path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(
        after_garble, PROBE_COUNT,
        "an unreadable cached verdict must RUN the probe, never be trusted"
    );
    assert!(!degraded, "…and running it on a faithful backend passes, so dense stays enabled");
}

/// The same fail-safe from the other side: a cache entry that has been DELETED
/// outright (or was never written — e.g. a workspace upgraded from before TC-68)
/// simply re-runs the probe.
#[test]
fn a_missing_cache_entry_falls_back_to_running_the_probe() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();
    seed_verified_workspace(&path, &calls);

    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "DELETE FROM _fathomdb_open_state
             WHERE key NOT IN ('projection_cursor',
                               'search_index_tokenizer_reproject_complete',
                               'tc33_edge_vector_prune_complete',
                               'tc33_reserved_write_cursor')",
            [],
        )
        .expect("delete the cached verdict");
    }

    let (after_delete, degraded) =
        open_count_close(&path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(after_delete, PROBE_COUNT, "no cached verdict ⇒ run the probe");
    assert!(!degraded);

    let (recached, _) =
        open_count_close(&path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(recached, 0, "…and the fresh verdict is cached again");
}

/// A divergence found on a fingerprint-changed open still yields `dense_disabled`
/// — the cache narrows WHEN the probe runs, never WHAT it concludes when it does.
#[test]
fn a_divergence_on_a_rerun_still_disables_dense() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();
    seed_verified_workspace(&path, &calls);

    // Invalidate the cached verdict the same way a pre-TC-68 workspace would look.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "DELETE FROM _fathomdb_open_state
             WHERE key NOT IN ('projection_cursor',
                               'search_index_tokenizer_reproject_complete',
                               'tc33_edge_vector_prune_complete',
                               'tc33_reserved_write_cursor')",
            [],
        )
        .expect("delete the cached verdict");
    }

    let (embeds, degraded) = open_count_close(
        &path,
        Arc::new(CountingDivergentEmbedder { calls: calls.clone() }),
        &calls,
    );
    assert_eq!(embeds, PROBE_COUNT, "the probe must actually run");
    assert!(degraded, "divergence on a re-run must still refuse the dense arm");

    // And a FAILED verdict is never cached as a pass: the next open re-runs.
    let (rerun, rerun_degraded) = open_count_close(
        &path,
        Arc::new(CountingDivergentEmbedder { calls: calls.clone() }),
        &calls,
    );
    assert_eq!(rerun, PROBE_COUNT, "a failing verdict must never be cached");
    assert!(rerun_degraded);
}

// ---- the RULED RESIDUAL, stated as an executable fact ----------------------

/// **THE NARROWING TC-68 BUYS.** Before TC-68, a same-identity backend that
/// drifted (candle CPU↔CUDA, a library/driver change) was caught at the NEXT open,
/// because the probe re-ran every time. With the verdict cached against a
/// fingerprint that such a drift does not move, it is no longer caught per-open —
/// it is caught only when some fingerprint input changes.
///
/// This is the ruled trade (`dev/plans/plan-0.8.20.md` §3 R-20-VC), and it is a
/// real narrowing of an HITL-signed guarantee
/// (`dev/adr/ADR-0.8.18-vector-equivalence-self-check.md`). It is asserted here so
/// it is reviewable and cannot be quietly reinterpreted as "preserved".
///
/// At `94bb33ef` this test FAILS — the drift *is* caught. That failure is the
/// honest statement of what changes.
#[test]
fn residual_same_identity_backend_drift_is_not_caught_on_a_cached_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();
    seed_verified_workspace(&path, &calls);

    // Same identity, same pinned mean (none), same baseline, same fixture — only
    // the BACKEND changed, which no fingerprint input can see.
    let (embeds, degraded) = open_count_close(
        &path,
        Arc::new(CountingDivergentEmbedder { calls: calls.clone() }),
        &calls,
    );

    assert_eq!(embeds, 0, "the cached verdict answers, so the drifted backend is never asked");
    assert!(
        !degraded,
        "RESIDUAL: same-identity backend drift is NOT caught on a cached open — \
         it is caught only at the next open whose fingerprint changed"
    );
}
