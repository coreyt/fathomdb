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
//!
//! ## fix-1 — the THREAT MODEL, measured rather than asserted
//!
//! codex §9 round 2 raised a `[P1]`: the cached marker is a SHA-256 over
//! deterministic, publicly derivable DB and build inputs, so an actor with write
//! access to the SQLite file can compute the current digest, write it into
//! `_fathomdb_open_state`, and force the early `Ok(())` before the 45-probe
//! verification runs. That is true, and
//! `a_marker_matching_the_current_state_serves_a_drifted_backend` proves it.
//!
//! `a_forged_stored_baseline_defeats_the_probe_even_when_it_fully_runs` shows the
//! SAME actor defeats the SAME arm through the **pre-slice** path — forge
//! `_fathomdb_embed_probe.reference_vec` to the drifted backend's own output and
//! the probe runs all 45 embeds and PASSES. The check body exercised there is
//! byte-identical to the one shipped at the slice baseline `fe85337f` (the TC-68
//! diff adds only the cache gate and the recording call), and the marker is
//! deleted first so the cache demonstrably plays no part.
//!
//! So the equivalence probe is a **correctness self-check against backend drift,
//! not tamper evidence** — it never was, and there is no secret in an embedded
//! local-first engine with which to make the marker unforgeable.
//!
//! ## fix-2 — the same actor, but NOT the same cost
//!
//! fix-1 concluded from the pair above that "the cache adds no new attack surface".
//! The codex re-review rejected that and was right: forging the marker needs only a
//! publicly computable digest (usually the one already in the row), while
//! re-baselining needs the target backend's 45 exact embeddings encoded into every
//! row — a materially higher bar. **The cache IS a cheaper bypass.** The claim is
//! struck; see §8.4 of the design note.
//!
//! What bounds the finding is the ruled residual, not the marker:
//! `residual_same_identity_backend_drift_is_not_caught_on_a_cached_open` shows an
//! **honestly recorded** marker already serves a drifted backend with NO forgery at
//! all, so forgery adds capability only where no valid marker exists for the current
//! fingerprint (§8.5). And it is bounded in time too:
//! `a_replayed_verdict_marker_is_defeated_by_a_mutated_baseline` and
//! `a_replayed_verdict_marker_is_defeated_by_a_rewritten_pinned_mean` show a known
//! digest stops working at the next change to any fingerprint input.

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

/// The `_fathomdb_open_state` key under which the engine records the fingerprint
/// of the last open at which the probe RAN and PASSED (mirrors
/// `vector_equivalence_probe.rs`). fix-1 needs to name it: the threat-model tests
/// read, delete and re-install that exact row the way an external writer would.
const VEQ_VERDICT_CACHE_KEY: &str = "vector_equivalence_verified_fingerprint";

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

/// LE-f32 encode a vector into the engine's `reference_vec` blob shape.
fn encode_vector_blob(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// fix-1 — read the cached verdict marker exactly as an external actor would.
fn read_verdict_marker(path: &std::path::Path) -> Option<String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(
        "SELECT value FROM _fathomdb_open_state WHERE key = ?1",
        [VEQ_VERDICT_CACHE_KEY],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

/// fix-1 — write an arbitrary value into the marker row: the forgery itself.
fn install_verdict_marker(path: &std::path::Path, value: &str) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "INSERT INTO _fathomdb_open_state(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![VEQ_VERDICT_CACHE_KEY, value],
    )
    .expect("install the verdict marker");
}

/// fix-1 — drop the marker, so the next open runs the pre-slice check path.
fn clear_verdict_marker(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute("DELETE FROM _fathomdb_open_state WHERE key = ?1", [VEQ_VERDICT_CACHE_KEY])
        .expect("clear the verdict marker");
}

/// fix-1 — rewrite EVERY stored `reference_vec` to `f(probe_text)`, leaving the
/// ordinal, the text, the identity triple and the blob LENGTH untouched. This is
/// the pre-slice forgery: the 0.8.18 fix-2 completeness validation pins each row's
/// shape but never its blob CONTENT, so a hostile writer can re-baseline the probe
/// against whatever their backend currently emits.
fn forge_stored_baseline(path: &std::path::Path, f: impl Fn(&str) -> Vec<f32>) {
    let conn = rusqlite::Connection::open(path).unwrap();
    let rows: Vec<(i64, String, usize)> = {
        let mut stmt = conn
            .prepare(
                "SELECT probe_ordinal, probe_text, length(reference_vec) \
                 FROM _fathomdb_embed_probe ORDER BY probe_ordinal",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as usize)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        rows
    };
    assert_eq!(rows.len() as u64, PROBE_COUNT, "the baseline must be the full committed set");
    for (ordinal, text, blob_len) in rows {
        let blob = encode_vector_blob(&f(&text));
        assert_eq!(blob.len(), blob_len, "the forgery must preserve the blob length");
        conn.execute(
            "UPDATE _fathomdb_embed_probe SET reference_vec = ?1 WHERE probe_ordinal = ?2",
            rusqlite::params![blob, ordinal],
        )
        .expect("forge one reference row");
    }
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
///
/// **fix-2 — this is also the BOUND on the forged-marker finding (§8.5 of the design
/// note).** There is no forgery anywhere in this test: the marker is the one the
/// ENGINE ITSELF recorded at the honest population open, asserted below to be
/// present beforehand and byte-unchanged afterwards. A same-identity backend swap
/// moves no fingerprint input, so that honest marker still matches and the drifted
/// backend is served anyway. Whatever forging a marker buys an attacker, it does not
/// buy *this* — the ruled trade already grants it, free. Forgery adds capability
/// only on an open where no valid marker exists for the CURRENT fingerprint.
#[test]
fn residual_same_identity_backend_drift_is_not_caught_on_a_cached_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();
    seed_verified_workspace(&path, &calls);

    // fix-2: the marker in play is the ENGINE'S OWN, written by the honest
    // population open above. Nothing in this test writes or computes a digest.
    let honest_marker =
        read_verdict_marker(&path).expect("the honest population open records its own verdict");

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
    assert_eq!(
        read_verdict_marker(&path).as_deref(),
        Some(honest_marker.as_str()),
        "fix-2 (§8.5): the drifted backend was served off the ENGINE'S OWN marker, \
         unchanged — no forgery was involved, so a forged marker buys nothing here"
    );
}

// ---- fix-1 (codex §9 round 2 [P1]) — the THREAT MODEL, measured ------------
//
// Transcript:
// `dev/plans/runs/codex/0.8.20/slice-22-review-round2-targeted-20260729T004445Z.log`.
//
// The finding is correct as stated: a writer who can compute the current digest
// forces the probe to be skipped. These four tests establish what that is worth —
// that the SAME actor already defeated the SAME arm before the cache existed, and
// that a known digest expires at the next change to any fingerprint input.
//
// fix-2: same actor, NOT same cost — the marker route is the cheaper one, and that
// concession is §8.4 of the design note. The bound on the finding is the ruled
// residual above, not these tests.

/// **THE HYPOTHESIS TEST.** Is the pre-slice design tamper-evident against a hostile
/// writer? No — and this is the measurement that says so.
///
/// **fix-2 — read the scope of this measurement carefully.** It establishes that the
/// same actor had a route that defeats the probe while it runs in full. It does NOT
/// establish that the two routes cost the same: this one needs the drifted backend's
/// 45 exact embeddings, the marker route needs a publicly computable digest. fix-1
/// drew "the cache adds no new attack surface" from this test; that inference was
/// wrong and is struck (§8.4).
///
/// The 0.8.18 probe re-embeds the 45 committed probes and compares them to the
/// STORED references. Those references live in an ordinary table in the same file.
/// An actor with write access re-baselines them to their drifted backend's own
/// output — same ordinals, same texts, same identity triple, same blob lengths, so
/// the fix-2 completeness validation sees nothing — and the probe then verifies the
/// drifted backend against itself and PASSES.
///
/// The cache is removed from the picture entirely: the marker is DELETED before
/// the open, and the test asserts all 45 embeds actually happen. What runs is the
/// check body as shipped at the slice baseline `fe85337f` (the TC-68 diff over that
/// function adds only the cache gate and the `record_probe_verification` call).
///
/// The control leg proves the forgery is what does it: the identical drifted
/// backend against an UN-forged baseline is caught and refuses dense.
#[test]
fn a_forged_stored_baseline_defeats_the_probe_even_when_it_fully_runs() {
    // -- control: drift alone, with the cache out of the way, IS caught. --------
    let control_dir = TempDir::new().unwrap();
    let control_path = db_path(&control_dir);
    let control_calls = counter();
    seed_verified_workspace(&control_path, &control_calls);
    clear_verdict_marker(&control_path);
    let (control_embeds, control_degraded) = open_count_close(
        &control_path,
        Arc::new(CountingDivergentEmbedder { calls: control_calls.clone() }),
        &control_calls,
    );
    assert_eq!(control_embeds, PROBE_COUNT, "control: the probe must actually run");
    assert!(control_degraded, "control: un-forged baseline + drifted backend ⇒ dense REFUSED");

    // -- the forgery: re-baseline the references to the drifted backend's output.
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();
    seed_verified_workspace(&path, &calls);
    forge_stored_baseline(&path, |text| reference_vector(text).into_iter().map(|x| -x).collect());
    clear_verdict_marker(&path);
    assert_eq!(read_verdict_marker(&path), None, "the cache must play NO part in this leg");

    let before = calls.load(Ordering::SeqCst);
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingDivergentEmbedder { calls: calls.clone() }),
    )
    .expect("open must succeed");
    let embeds = calls.load(Ordering::SeqCst) - before;

    assert_eq!(embeds, PROBE_COUNT, "the FULL pre-slice check ran — nothing was skipped");
    assert!(
        !opened.report.dense_disabled,
        "PRE-EXISTING: a forged stored baseline defeats the probe on the pre-slice path — \
         the probe is a correctness self-check against backend drift, NOT an integrity \
         boundary against an actor with write access to the database file"
    );
    if let Err(EngineError::VectorEquivalenceMismatch { .. }) = opened.engine.search("memory") {
        panic!("the dense arm must be SERVING — that is the point of this measurement");
    }
    opened.engine.close().unwrap();
}

/// **THE FINDING, as an executable fact.** A marker that matches the current state
/// — however it got there — skips the probe, so a drifted backend is served. On the
/// SAME workspace, with the marker absent, that drift is caught. The marker is
/// therefore not an engine-authenticated attestation; it is a statement that the
/// fingerprint inputs are unchanged since *some* engine recorded a pass.
///
/// Paired with `a_forged_stored_baseline_defeats_the_probe_even_when_it_fully_runs`:
/// both routes are open to exactly the same actor and one of them predates the
/// cache — but **this one is the cheaper route** (fix-2, §8.4), which is precisely
/// why the finding is conceded rather than argued away. Its *bound* is
/// `residual_same_identity_backend_drift_is_not_caught_on_a_cached_open`: note that
/// the digest installed here is the one the engine itself wrote moments earlier, and
/// that leaving it in place would have served the same drifted backend with no
/// forgery at all.
#[test]
fn a_marker_matching_the_current_state_serves_a_drifted_backend() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();
    seed_verified_workspace(&path, &calls);

    // The digest for THIS state. An external actor derives it from the same
    // public inputs (identity, pinned mean, fixture, floors, stored baseline); the
    // test simply reads the row, which is the same value by construction.
    let digest = read_verdict_marker(&path).expect("a verified workspace carries a marker");

    // Without it, the drift is caught — and the failing open clears the marker
    // again without touching any fingerprint input, so `digest` stays current.
    clear_verdict_marker(&path);
    let (caught_embeds, caught_degraded) = open_count_close(
        &path,
        Arc::new(CountingDivergentEmbedder { calls: calls.clone() }),
        &calls,
    );
    assert_eq!(caught_embeds, PROBE_COUNT, "no marker ⇒ the probe runs");
    assert!(caught_degraded, "no marker ⇒ the drifted backend is CAUGHT");
    assert_eq!(read_verdict_marker(&path), None, "a failing verdict is never cached");

    // Write the digest back — the forgery — and the same drifted backend is served.
    install_verdict_marker(&path, &digest);
    let (forged_embeds, forged_degraded) = open_count_close(
        &path,
        Arc::new(CountingDivergentEmbedder { calls: calls.clone() }),
        &calls,
    );
    assert_eq!(forged_embeds, 0, "a matching marker skips the 45-probe verification");
    assert!(
        !forged_degraded,
        "codex §9 round-2 [P1]: a marker matching the current state serves a drifted \
         backend — the marker proves the fingerprint inputs are unchanged since SOME \
         engine recorded a pass, not that THIS engine verified THIS backend"
    );
}

/// What forging buys is bounded in time: a digest is valid only for the state it
/// was computed over. Re-installing a known digest after mutating a reference blob
/// does not keep the probe skipped — the fingerprint moved, so the probe re-runs
/// and catches the mutation.
#[test]
fn a_replayed_verdict_marker_is_defeated_by_a_mutated_baseline() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();
    seed_verified_workspace(&path, &calls);
    let digest = read_verdict_marker(&path).expect("a verified workspace carries a marker");

    // Mutate ONE reference (length preserved) and re-assert the known digest.
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
            .flat_map(|c| (-f32::from_le_bytes([c[0], c[1], c[2], c[3]])).to_le_bytes())
            .collect();
        conn.execute(
            "UPDATE _fathomdb_embed_probe SET reference_vec = ?1 WHERE probe_ordinal = 0",
            rusqlite::params![flipped],
        )
        .expect("mutate reference 0");
    }
    install_verdict_marker(&path, &digest);

    let (embeds, degraded) =
        open_count_close(&path, Arc::new(CountingRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(embeds, PROBE_COUNT, "the stale digest cannot keep the probe skipped");
    assert!(degraded, "…and the re-run catches the mutated reference (fail-safe, R-VEQ-4)");
}

/// The same bound on the other end-to-end-reachable fingerprint input: replaying a
/// known digest across a rewritten pinned `mean_vec` does not buy a skip either.
#[test]
fn a_replayed_verdict_marker_is_defeated_by_a_rewritten_pinned_mean() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let calls = counter();

    {
        let opened = Engine::open_with_embedder_for_test(
            &path,
            Arc::new(CountingBgeRefEmbedder { calls: calls.clone() }),
        )
        .expect("bge enrolment open");
        opened.engine.configure_vector_kind_for_test("note").expect("enrol vector kind");
        opened.engine.close().expect("close");
    }
    set_pinned_mean(&path, mean_blob(1.0));
    let (population, _) =
        open_count_close(&path, Arc::new(CountingBgeRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(population, 2 * PROBE_COUNT, "population open under a pinned mean");
    let digest = read_verdict_marker(&path).expect("a verified workspace carries a marker");

    set_pinned_mean(&path, mean_blob(0.9));
    install_verdict_marker(&path, &digest);

    let (embeds, degraded) =
        open_count_close(&path, Arc::new(CountingBgeRefEmbedder { calls: calls.clone() }), &calls);
    assert_eq!(embeds, PROBE_COUNT, "a rewritten mean invalidates the replayed digest");
    assert!(!degraded, "…and a faithful backend still passes the forced re-run");
}
