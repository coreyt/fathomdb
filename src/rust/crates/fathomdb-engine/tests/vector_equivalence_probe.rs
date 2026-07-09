//! 0.8.18 Slice 5 (#5 vector-equivalence probe KEYSTONE) — the shipped open-time
//! self-check that re-embeds the 45 committed probes and refuses dense retrieval
//! on divergence beyond the frozen D4 floor.
//!
//! Authority: `dev/design/0.8.18-slice-0-vector-equivalence-publish-design.md`
//! §U1 (R-VEQ-1..R-VEQ-6) + `dev/adr/ADR-0.8.18-vector-equivalence-self-check.md`.
//!
//! FROZEN D4 floor (Steward, from U3): **P1** mean-centered `embedding_bin`
//! sign-flip count floor = 0 (exact); **P2** un-centered L2 ε = 1e-5.
//!
//! Test embedders share ONE identity (so `check_embedder_profile` passes — the #5
//! probe is ADDITIVE to the identity gate, R-VEQ-5) but differ in the vectors they
//! produce, which is exactly the drift #5 exists to catch. All test identities are
//! non-bge, so `identity_requires_mean_centering` is false and the probe uses the
//! un-centered (raw-sign) representation on BOTH sides (R-VEQ-3c non-MC branch).

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, EngineError};
use tempfile::TempDir;

const DIM: usize = 384;
const PROBE_IDENTITY_NAME: &str = "fathomdb-probe-test";
const PROBE_IDENTITY_REV: &str = "veq-slice5";

/// Deterministic per-text reference vector, every component in `[0.5, 1.5]` (all
/// strictly positive, so the 1-bit sign quantization is all-ones and is robustly
/// away from the zero threshold — a small perturbation never flips a sign).
fn reference_vector(text: &str) -> Vec<f32> {
    let mut out = Vec::with_capacity(DIM);
    // Simple deterministic FNV-1a-ish per-index hash of the text bytes.
    for i in 0..DIM {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ (i as u64).wrapping_mul(0x0100_0000_01b3);
        for b in text.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        let frac = (h % 1000) as f32 / 1000.0; // [0, 1)
        out.push(0.5 + frac); // [0.5, 1.5)
    }
    out
}

/// The faithful reference backend: `embed == reference_vector`.
#[derive(Debug)]
struct RefEmbedder;
impl Embedder for RefEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(PROBE_IDENTITY_NAME, PROBE_IDENTITY_REV, DIM as u32)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        Ok(reference_vector(text))
    }
}

/// Deliberately-divergent backend, SAME identity: negates every component. Every
/// sign flips (all 384 bits per probe) AND the un-centered L2 is large — trips
/// BOTH P1 and P2.
#[derive(Debug)]
struct DivergentEmbedder;
impl Embedder for DivergentEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(PROBE_IDENTITY_NAME, PROBE_IDENTITY_REV, DIM as u32)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        Ok(reference_vector(text).into_iter().map(|x| -x).collect())
    }
}

/// Same-backend float noise, SAME identity: adds a deterministic per-component
/// perturbation of magnitude 1e-7, so the TOTAL un-centered L2 ≈ sqrt(384)·1e-7 ≈
/// 2e-6 < 1e-5, and no sign flips (components stay ≥ 0.5). Trips NEITHER check.
#[derive(Debug)]
struct NoiseEmbedder;
impl Embedder for NoiseEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(PROBE_IDENTITY_NAME, PROBE_IDENTITY_REV, DIM as u32)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        Ok(reference_vector(text)
            .into_iter()
            .enumerate()
            .map(|(i, x)| x + if i % 2 == 0 { 1e-7 } else { -1e-7 })
            .collect())
    }
}

/// P2-only divergence, SAME identity: adds a constant 1e-3 to every component. No
/// sign flips (all components stay ≥ 0.5, so P1 flip count = 0), but the
/// un-centered L2 = sqrt(384)·1e-3 ≈ 2e-2 ≫ 1e-5 — trips P2 ALONE. Proves the
/// Phase-2 L2 axis is asserted independently of the Phase-1 bit axis.
#[derive(Debug)]
struct P2OnlyEmbedder;
impl Embedder for P2OnlyEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(PROBE_IDENTITY_NAME, PROBE_IDENTITY_REV, DIM as u32)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        Ok(reference_vector(text).into_iter().map(|x| x + 1e-3).collect())
    }
}

fn db_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("veq.sqlite")
}

/// Register a vector kind then persist the 45 UN-centered f32 references with the
/// reference backend, leaving the DB ready for a divergence CHECK on the next
/// open. Two opens: session 1 registers the kind (the probe is gated on a
/// registered vector kind, so it is inert here); session 2 sees the kind and
/// persists the references. Both leave `dense_disabled == false`.
fn seed_references(path: &std::path::Path) {
    // Session 1 — register the vector kind (probe inert: no kind at open yet).
    let opened =
        Engine::open_with_embedder_for_test(path, Arc::new(RefEmbedder)).expect("session 1 open");
    opened.engine.configure_vector_kind_for_test("note").expect("register vector kind");
    assert!(!opened.report.dense_disabled, "session 1 is never degraded");
    opened.engine.close().expect("close session 1");

    // Session 2 — kind now registered + references empty ⇒ persist references.
    let opened = Engine::open_with_embedder_for_test(path, Arc::new(RefEmbedder))
        .expect("session 2 open persists references");
    assert!(!opened.report.dense_disabled, "first registration is never degraded");
    opened.engine.close().expect("close session 2");
}

// ---- R-VEQ-1 — probe set persisted at first vector-kind registration ---------

#[test]
fn probe_set_persisted_at_first_vector_kind_registration() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    // Session 1 — register the vector kind (probe gated on a registered kind).
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(RefEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("note").expect("register vector kind");
    // No references persisted yet — no kind existed at THIS open.
    opened.engine.close().unwrap();
    let conn0 = rusqlite::Connection::open(&path).unwrap();
    let pre: i64 =
        conn0.query_row("SELECT COUNT(*) FROM _fathomdb_embed_probe", [], |r| r.get(0)).unwrap();
    assert_eq!(pre, 0, "no references persisted before a vector kind exists at open");
    drop(conn0);

    // Session 2 — kind now registered ⇒ the 45 UN-centered f32 references persist.
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(RefEmbedder)).expect("reopen");
    opened.engine.close().unwrap();

    let conn = rusqlite::Connection::open(&path).unwrap();
    let rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM _fathomdb_embed_probe", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 45, "exactly 45 probes must be persisted at first registration");

    let (name, rev, dim): (String, String, i64) = conn
        .query_row(
            "SELECT embedder_name, embedder_revision, dim FROM _fathomdb_embed_probe WHERE probe_ordinal = 0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(name, PROBE_IDENTITY_NAME);
    assert_eq!(rev, PROBE_IDENTITY_REV);
    assert_eq!(dim, DIM as i64);

    // reference_vec is 4*dim little-endian f32 bytes (UN-centered); NEVER a bit
    // blob (would be dim/8 = 48 bytes).
    let ref_len: i64 = conn
        .query_row(
            "SELECT LENGTH(reference_vec) FROM _fathomdb_embed_probe WHERE probe_ordinal = 0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ref_len, (DIM * 4) as i64, "reference must be 4*dim f32 bytes, never packed bits");
}

// ---- R-VEQ-2 — two-sided: divergent trips, float-noise does not --------------

#[test]
fn divergent_backend_trips_dense_refusal() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    // Reopen with a DIVERGENT backend of the SAME identity.
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(DivergentEmbedder))
        .expect("open must SUCCEED (degraded), never fail");
    let engine = opened.engine;

    // Degraded-open, not open-failure (R-VEQ-4).
    assert!(opened.report.dense_disabled, "divergent backend must degrade the open");

    // Every vector-dependent arm refuses with the typed query-time error.
    match engine.search("memory") {
        Err(EngineError::VectorEquivalenceMismatch { .. }) => {}
        other => panic!("hybrid search must refuse with VectorEquivalenceMismatch, got {other:?}"),
    }

    // FTS-only path still serves.
    let fts = engine.search_text_only("memory").expect("FTS-only path must stay serviceable");
    let _ = fts; // may be empty (no rows written) — the point is it does not refuse.
    engine.close().unwrap();
}

#[test]
fn same_backend_float_noise_does_not_trip() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    // Reopen with a ≤1e-6-total-L2 float-noise backend of the SAME identity.
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(NoiseEmbedder))
        .expect("open with float-noise backend");
    let engine = opened.engine;

    assert!(!opened.report.dense_disabled, "sub-epsilon float noise must NOT degrade the open");
    // Dense arm served (does not refuse). Result may be empty (no rows) — the
    // contract is "no VectorEquivalenceMismatch".
    match engine.search("memory") {
        Ok(_) => {}
        Err(EngineError::VectorEquivalenceMismatch { .. }) => {
            panic!("float noise within the D4 floor must NOT refuse dense")
        }
        Err(other) => panic!("unexpected error: {other:?}"),
    }
    engine.close().unwrap();
}

// ---- R-VEQ-3a (P1) — mean-centered flip count ------------------------------

#[test]
fn probe_p1_flip_count_matches_embedding_bin() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    // Full-negation backend: reference is all-positive (bits all 1), reembed is
    // all-negative (bits all 0), so EVERY bit flips: 384 bits × 45 probes = 17280.
    // The exact count proves the probe computes `embedding_bin` via the SAME
    // `vec_quantize_binary(sign(x))` path and counts flips exactly (P1 floor = 0).
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(DivergentEmbedder)).unwrap();
    assert!(opened.report.dense_disabled);
    let reason = opened.report.dense_disabled_reason.clone().expect("degraded reason present");
    assert!(
        reason.contains("flips=17280"),
        "P1 must count exactly 384*45=17280 sign flips, reason was: {reason}"
    );
    opened.engine.close().unwrap();
}

// ---- R-VEQ-3b (P2) — un-centered L2 within epsilon --------------------------

#[test]
fn probe_p2_l2_within_epsilon() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    // +1e-3 constant: NO sign flips (P1 flips = 0) but L2 ≈ 2e-2 > 1e-5 — P2 trips
    // ALONE, proving the Phase-2 L2 axis is asserted independently of P1.
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(P2OnlyEmbedder)).unwrap();
    assert!(opened.report.dense_disabled, "P2 L2 over epsilon must degrade the open");
    let reason = opened.report.dense_disabled_reason.clone().expect("reason present");
    assert!(
        reason.contains("flips=0"),
        "P2-only divergence must NOT flip any P1 bit, reason was: {reason}"
    );
    opened.engine.close().unwrap();
}

// ---- R-VEQ-3c — centering gate (non-MC/no-pin ⇒ un-centered both sides) ------

#[test]
fn probe_respects_mean_centering_gate_non_mc_path() {
    // Non-bge identity ⇒ `identity_requires_mean_centering` is false ⇒ the probe
    // uses the raw-sign (un-centered) representation on BOTH sides. The exact
    // 17280-flip result in `probe_p1_flip_count_matches_embedding_bin` is only
    // reachable on the un-centered path (raw sign of all-positive vs all-negative);
    // here we additionally assert the gate does not spuriously trip on the
    // faithful identical backend under the same non-MC path.
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(RefEmbedder)).unwrap();
    assert!(
        !opened.report.dense_disabled,
        "identical backend on the non-MC un-centered path must be 0 flips / 0 L2"
    );
    opened.engine.close().unwrap();
}

// ---- R-VEQ-4 — degraded-open + choke-point coverage + FTS serviceable --------

#[test]
fn divergent_open_is_degraded_not_failed() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);
    // open() returns Ok, not Err — the refusal is query-time, not open-time.
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(DivergentEmbedder))
        .expect("degraded open must SUCCEED");
    assert!(opened.report.dense_disabled);
    assert!(opened.engine.dense_disabled(), "engine accessor mirrors the report");
    opened.engine.close().unwrap();
}

#[test]
fn every_vector_dependent_arm_refuses_with_typed_error() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(DivergentEmbedder)).unwrap();
    let engine = opened.engine;

    let is_veq = |r: &Result<_, EngineError>| {
        matches!(r, Err(EngineError::VectorEquivalenceMismatch { .. }))
    };

    assert!(is_veq(&engine.search("q")), "search must refuse");
    assert!(is_veq(&engine.search_filtered("q", None)), "search_filtered must refuse");
    assert!(
        is_veq(&engine.search_reranked("q", None, 5, false, 1.0, 5)),
        "search_reranked (CE) must refuse"
    );
    assert!(
        is_veq(&engine.search_explained("q", None, 5, false, 1.0, 5)),
        "search_explained (explain/rerank) must refuse"
    );
    assert!(
        is_veq(&engine.search_reranked("q", None, 0, true, 0.3, 0)),
        "graph-arm (use_graph_arm=true) must refuse"
    );
    // searchExpand rides the same choke point via search_inner.
    assert!(
        matches!(
            engine.search_expand("q", None, 1),
            Err(EngineError::VectorEquivalenceMismatch { .. })
        ),
        "search_expand must refuse"
    );
    engine.close().unwrap();
}

#[test]
fn fts_only_path_still_serves_when_degraded() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(DivergentEmbedder)).unwrap();
    let engine = opened.engine;
    // The explicit text-only route never routes through the vector choke point,
    // so it returns Ok even while every dense arm refuses.
    assert!(engine.search_text_only("anything").is_ok(), "FTS-only path must serve when degraded");
    assert!(engine.search("anything").is_err(), "the dense arm still refuses");
    engine.close().unwrap();
}

// ---- R-VEQ-6 — degraded observability + telemetry counter + reopen-sticky ----

#[test]
fn open_report_surfaces_dense_disabled_and_counter_and_reopen_stays_degraded() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    // First divergent reopen: report surfaces dense_disabled + reason; counter
    // increments per refused query.
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(DivergentEmbedder)).unwrap();
    let engine = opened.engine;
    assert!(opened.report.dense_disabled);
    assert!(opened.report.dense_disabled_reason.is_some(), "reason surfaced on the report");
    assert_eq!(engine.vector_equivalence_refusal_count(), 0, "counter starts at 0");
    let _ = engine.search("q");
    let _ = engine.search("q2");
    assert_eq!(engine.vector_equivalence_refusal_count(), 2, "counter increments per refusal");
    engine.close().unwrap();

    // Reopen AGAIN with the divergent backend: state re-derived, still degraded
    // (never silently re-enables dense).
    let reopened = Engine::open_with_embedder_for_test(&path, Arc::new(DivergentEmbedder)).unwrap();
    assert!(reopened.report.dense_disabled, "reopen with a divergent backend stays degraded");
    reopened.engine.close().unwrap();

    // Reopen with the FAITHFUL backend: the check clears (dense re-enabled).
    let healthy = Engine::open_with_embedder_for_test(&path, Arc::new(RefEmbedder)).unwrap();
    assert!(!healthy.report.dense_disabled, "reopen with a matching backend clears the degrade");
    healthy.engine.close().unwrap();
}
