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

use std::sync::{Arc, Once};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, EngineError};
use fathomdb_schema::{migrate_with_steps, MIGRATIONS};
use tempfile::TempDir;

const DIM: usize = 384;
const PROBE_IDENTITY_NAME: &str = "fathomdb-probe-test";
const PROBE_IDENTITY_REV: &str = "veq-slice5";

// ---- fix-1: mean-centering (MC) identity for the production P1 branch --------
// The default bge identity is the ONLY one for which
// `identity_requires_mean_centering` is true; these tests exercise the
// MC-required-WITH-pin P1 branch (DEFECT #5 / CONCERN #9), the real production
// path, which non-bge identities never reach.
const BGE_NAME: &str = "fathomdb-bge-small-en-v1.5";
const BGE_REV: &str = "veq-slice5-mc";

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

/// fix-1 DEFECT #1 — a caller embedder that PANICS in `embed`. SAME identity as
/// the seeded references, so `check_embedder_profile` passes and the panic is the
/// ONLY thing that can be caught: the probe must fail-SAFE (refuse dense), never
/// wedge open and never fail-open (silently serve an un-verifiable arm).
#[derive(Debug)]
struct PanicEmbedder;
impl Embedder for PanicEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(PROBE_IDENTITY_NAME, PROBE_IDENTITY_REV, DIM as u32)
    }
    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        panic!("probe embedder deliberately panics");
    }
}

/// fix-1 DEFECT #1 — a caller embedder that ERRORS in `embed`. SAME identity as
/// the references; the probe must fail-SAFE (refuse dense).
#[derive(Debug)]
struct ErrorEmbedder;
impl Embedder for ErrorEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(PROBE_IDENTITY_NAME, PROBE_IDENTITY_REV, DIM as u32)
    }
    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        Err(EmbedderError::Failed { message: "deterministic probe embed failure".to_string() })
    }
}

/// fix-1 (DEFECT #5 / CONCERN #9) — faithful bge-identity backend (the only
/// identity whose `identity_requires_mean_centering` is true). `embed ==
/// reference_vector` (all components in `[0.5, 1.5)`, strictly positive).
#[derive(Debug)]
struct BgeRefEmbedder;
impl Embedder for BgeRefEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(BGE_NAME, BGE_REV, DIM as u32)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        Ok(reference_vector(text))
    }
}

/// fix-1 (DEFECT #5 / CONCERN #9) — bge-identity backend that REFLECTS every
/// component about the pinned mean `1.0`: `e = 2.0 - r`. Because `r ∈ [0.5, 1.5)`,
/// `e ∈ (0.5, 1.5]` — so the RAW sign is unchanged (0 un-centered flips), but the
/// MEAN-CENTERED sign `sign(e − 1) = −sign(r − 1)` flips on every component whose
/// `r ≠ 1.0`. This isolates the MC-required-with-pin P1 branch: it trips ONLY when
/// centering is actually applied (with a pin ⇒ flips ≫ 0; without a pin ⇒ flips=0).
#[derive(Debug)]
struct BgeMeanReflectEmbedder;
impl Embedder for BgeMeanReflectEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new(BGE_NAME, BGE_REV, DIM as u32)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        Ok(reference_vector(text).into_iter().map(|x| 2.0 - x).collect())
    }
}

fn db_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("veq.sqlite")
}

/// Register sqlite-vec once per test binary (needed to build a raw v18 DB whose
/// step-9 migration creates a vec0 table).
fn register_sqlite_vec_once() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| unsafe {
        let entrypoint: unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *const std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int = std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
        rusqlite::ffi::sqlite3_auto_extension(Some(entrypoint));
    });
}

/// The all-`1.0` mean vector, LE-f32 encoded (matches the engine's `mean_vec`
/// blob shape: `4 * dim` bytes). Chosen so the centered reference `r − 1.0` is a
/// MIX of positive and negative components (`r ∈ [0.5, 1.5)`), i.e. the centered
/// P1 bits are genuinely mean-dependent, not trivially all-ones.
fn all_ones_mean_blob() -> Vec<u8> {
    let mut blob = Vec::with_capacity(DIM * 4);
    for _ in 0..DIM {
        blob.extend_from_slice(&1.0f32.to_le_bytes());
    }
    blob
}

/// Directly pin (or, with `None`, un-pin) `_fathomdb_embedder_profiles.mean_vec`
/// for the default profile via a raw connection while the engine is closed. There
/// is no public test seam to pin the mean without writing ≥256 vector rows; a
/// direct UPDATE reproduces the pinned state deterministically.
fn set_pinned_mean(path: &std::path::Path, mean: Option<Vec<u8>>) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "UPDATE _fathomdb_embedder_profiles SET mean_vec = ?1 WHERE profile = 'default'",
        rusqlite::params![mean],
    )
    .expect("pin/un-pin mean_vec");
}

/// Seed a bge-identity workspace (registered vector kind + 45 UN-centered f32
/// references) with a pinned all-`1.0` mean, leaving the DB ready for an MC-branch
/// CHECK on the next open. Returns with `dense_disabled == false`.
fn seed_bge_references_with_pinned_mean(path: &std::path::Path) {
    // Session 1 — create the profile (bge identity) + register the vector kind.
    let opened = Engine::open_with_embedder_for_test(path, Arc::new(BgeRefEmbedder))
        .expect("bge session 1 open");
    opened.engine.configure_vector_kind_for_test("note").expect("register vector kind");
    opened.engine.close().expect("close bge session 1");

    // Pin the mean BEFORE the references are captured, so the MC gate engages.
    set_pinned_mean(path, Some(all_ones_mean_blob()));

    // Session 2 — kind registered + references empty ⇒ persist UN-centered refs.
    let opened = Engine::open_with_embedder_for_test(path, Arc::new(BgeRefEmbedder))
        .expect("bge session 2 open persists references");
    assert!(!opened.report.dense_disabled, "first registration is never degraded");
    opened.engine.close().expect("close bge session 2");
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

// ---- fix-1 DEFECT #1 — fail-SAFE (never fail-open) on an un-verifiable arm ----

/// A probe embedder that PANICS at CHECK time ⇒ open SUCCEEDS (no wedge), but the
/// dense arm is REFUSED (`dense_disabled=true`), a dense query raises
/// `VectorEquivalenceMismatch`, and the text-only/FTS path still serves. Before
/// fix-1 this failed open (dense served on an un-verifiable arm).
#[test]
fn panicking_embedder_at_check_fails_safe_not_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(PanicEmbedder))
        .expect("a panicking probe embedder must NOT wedge open (open still succeeds)");
    let engine = opened.engine;

    assert!(
        opened.report.dense_disabled,
        "an un-verifiable (panicking) embedder must fail SAFE: dense refused"
    );
    assert!(opened.report.dense_disabled_reason.is_some(), "a refusal reason is surfaced");
    match engine.search("memory") {
        Err(EngineError::VectorEquivalenceMismatch { .. }) => {}
        other => panic!("dense query must refuse with VectorEquivalenceMismatch, got {other:?}"),
    }
    assert!(engine.search_text_only("memory").is_ok(), "FTS-only path must still serve");
    engine.close().unwrap();
}

/// A probe embedder that ERRORS at CHECK time ⇒ same fail-SAFE contract as the
/// panicking one (open succeeds, dense refused, FTS serves).
#[test]
fn erroring_embedder_at_check_fails_safe_not_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(ErrorEmbedder))
        .expect("an erroring probe embedder must NOT wedge open");
    let engine = opened.engine;

    assert!(
        opened.report.dense_disabled,
        "an un-verifiable (erroring) embedder must fail SAFE: dense refused"
    );
    match engine.search("memory") {
        Err(EngineError::VectorEquivalenceMismatch { .. }) => {}
        other => panic!("dense query must refuse with VectorEquivalenceMismatch, got {other:?}"),
    }
    assert!(engine.search_text_only("memory").is_ok(), "FTS-only path must still serve");
    engine.close().unwrap();
}

/// POPULATION path (first registration): if the embedder cannot produce the
/// reference vectors, NO baseline can be established ⇒ fail-SAFE (dense refused),
/// and NO partial references are persisted. Before fix-1 this fail-opened (dense
/// served with an empty/partial baseline).
#[test]
fn population_failure_fails_safe_and_persists_no_partial_baseline() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);

    // Session 1 — register the vector kind with a faithful backend (probe inert).
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(RefEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("note").expect("register vector kind");
    opened.engine.close().unwrap();

    // Session 2 — kind registered + references empty ⇒ POPULATION runs, but the
    // panicking backend cannot produce any reference ⇒ fail-SAFE.
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(PanicEmbedder))
        .expect("population failure must NOT wedge open");
    assert!(
        opened.report.dense_disabled,
        "a baseline that cannot be established must fail SAFE: dense refused"
    );
    match opened.engine.search("q") {
        Err(EngineError::VectorEquivalenceMismatch { .. }) => {}
        other => panic!("dense query must refuse, got {other:?}"),
    }
    opened.engine.close().unwrap();

    // No partial baseline was persisted (atomic population; rollback on failure).
    let conn = rusqlite::Connection::open(&path).unwrap();
    let rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM _fathomdb_embed_probe", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 0, "a failed population must persist NO probe rows (no partial baseline)");
}

// ---- fix-1 DEFECT #4 hole (a) — post-open registration: safe in-session + ------
// ---- baseline established at the NEXT open (NOT in the write path) ------------

/// A vector kind registered AFTER open serves SAFELY in the registering session
/// (the serving backend IS the backend that built the vectors — nothing to
/// diverge from), and the baseline is established at the NEXT open (identity-gated
/// population), catching all forward drift. The baseline is deliberately NOT
/// captured in the write path: a write must never block on the embedder (the
/// async-projection invariant — `ac_029` + the PR-9 embed watchdog). This closes
/// hole (a) to the same degree as the accepted v18→v19 upgrade residual.
#[test]
fn post_open_registration_serves_in_session_and_baselines_at_next_open() {
    use fathomdb_engine::PreparedWrite;

    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);

    // Session 1 — fresh open with NO vector kind ⇒ the probe is inert. Register a
    // kind post-open and write under it; the write does NOT block on the embedder
    // (no probe embeds on the write path) and the dense arm is NOT degraded.
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(RefEmbedder)).expect("open");
    let engine = opened.engine;
    assert!(!opened.report.dense_disabled, "a vector-less open is not degraded");
    engine.configure_vector_kind_for_test("note").expect("register vector kind post-open");
    engine
        .write(&[PreparedWrite::Node {
            kind: "note".to_string(),
            body: "post-open registration body".to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write must not block on / be gated by the probe");
    assert!(!engine.dense_disabled(), "in-session serving is safe (same live backend)");
    // In-session serving is not refused (the registering backend is serving).
    assert!(engine.search("post-open").is_ok(), "in-session dense query is served");
    engine.close().unwrap();

    // No baseline was written in the registering session (write-path is embed-free).
    let conn = rusqlite::Connection::open(&path).unwrap();
    let pre: i64 =
        conn.query_row("SELECT COUNT(*) FROM _fathomdb_embed_probe", [], |r| r.get(0)).unwrap();
    assert_eq!(pre, 0, "the write path must not embed/establish the baseline");
    drop(conn);

    // Session 2 (next open) — the kind now exists at open ⇒ population establishes
    // the 45-probe baseline from the identity-matched embedder (hole (a) closed).
    let reopened =
        Engine::open_with_embedder_for_test(&path, Arc::new(RefEmbedder)).expect("reopen");
    assert!(!reopened.report.dense_disabled, "baseline establishment at reopen is not degraded");
    reopened.engine.close().unwrap();

    let conn = rusqlite::Connection::open(&path).unwrap();
    let rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM _fathomdb_embed_probe", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 45, "the next open must establish the 45-probe baseline (identity-gated)");
    drop(conn);

    // Session 3 — a divergent same-identity backend is now caught against the
    // baseline (forward drift detection works after the reopen baseline).
    let divergent = Engine::open_with_embedder_for_test(&path, Arc::new(DivergentEmbedder))
        .expect("divergent reopen (degraded)");
    assert!(divergent.report.dense_disabled, "forward drift is caught after the reopen baseline");
    divergent.engine.close().unwrap();
}

// ---- fix-1 DEFECT #5 / CONCERN #9 — MC-required-WITH-pin P1 branch ------------

/// The default bge identity WITH a pinned mean is the real production P1 path
/// (`identity_requires_mean_centering ∧ mean_pinned`). A same-backend reopen must
/// NOT trip (0 flips through the mean-centered comparison).
#[test]
fn mc_required_with_pin_same_backend_does_not_trip() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_bge_references_with_pinned_mean(&path);

    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(BgeRefEmbedder))
        .expect("bge same-backend reopen");
    assert!(
        !opened.report.dense_disabled,
        "the MC-with-pin same-backend path must be 0 flips / 0 L2 (dense served)"
    );
    match opened.engine.search("q") {
        Ok(_) | Err(EngineError::EmbedderNotConfigured) => {}
        Err(EngineError::VectorEquivalenceMismatch { .. }) => {
            panic!("the same-backend MC path must NOT refuse dense")
        }
        Err(other) => panic!("unexpected error: {other:?}"),
    }
    opened.engine.close().unwrap();
}

/// The MC-required-WITH-pin P1 branch DOES trip on a mean-centered sign flip: the
/// reflect backend (`e = 2 − r`) has ZERO raw-sign flips but FULL mean-centered
/// flips, so it trips ONLY because centering is applied with the pinned mean. The
/// no-pin contrast (below) proves the pin is what caught it.
#[test]
fn mc_required_with_pin_centered_flip_trips_dense() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_bge_references_with_pinned_mean(&path);

    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(BgeMeanReflectEmbedder))
        .expect("bge reflect reopen (degraded)");
    assert!(
        opened.report.dense_disabled,
        "a mean-centered sign flip must degrade the open on the MC-with-pin path"
    );
    let reason = opened.report.dense_disabled_reason.clone().expect("reason present");
    assert!(
        !reason.contains("flips=0 "),
        "centering must be applied (flips > 0) with the pinned mean, reason was: {reason}"
    );
    opened.engine.close().unwrap();
}

/// Contrast that PROVES centering is the cause: the SAME reflect backend, with the
/// mean UN-pinned, takes the un-centered path — 0 raw flips — so P1 does NOT trip
/// (only P2 does, `flips=0`). With the pin (test above) P1 trips (`flips > 0`).
/// Same backend, opposite P1 outcome ⇒ the MC-with-pin branch genuinely centers.
#[test]
fn mc_reflect_without_pin_takes_uncentered_path_zero_flips() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_bge_references_with_pinned_mean(&path);

    // Un-pin the mean: the bge identity now falls back to the un-centered path.
    set_pinned_mean(&path, None);

    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(BgeMeanReflectEmbedder))
        .expect("bge reflect reopen (un-pinned)");
    // P2 still trips (L2 large) ⇒ degraded, but P1 sees ZERO raw-sign flips.
    assert!(opened.report.dense_disabled, "the reflect backend still trips P2 (L2) un-centered");
    let reason = opened.report.dense_disabled_reason.clone().expect("reason present");
    assert!(
        reason.contains("flips=0 "),
        "un-centered, the reflect backend must show 0 raw-sign flips, reason was: {reason}"
    );
    opened.engine.close().unwrap();
}

// ---- fix-1 CONCERN #6 — v18→v19 upgrade with pre-existing kind + pinned mean --

/// Build a genuine v18 DB with a pre-existing vector kind + a pinned mean, then
/// open it through the engine (which upgrades 18→head). The baseline must be
/// established at that first upgraded open FROM the identity-matched embedder (gated by
/// `check_embedder_profile`), and the subsequent check must behave: same backend ⇒
/// served; divergent backend ⇒ dense refused (fail-SAFE). The residual (a
/// same-identity backend that diverged BEFORE upgrade is not retroactively caught)
/// is documented in the design; #5 is additive-only.
#[test]
fn upgrade_from_v18_with_kind_and_pinned_mean_establishes_baseline_and_checks() {
    register_sqlite_vec_once();
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);

    // --- Build a v18 DB (migrations 1..=18) then seed the pre-existing state ---
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let steps_to_18: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id <= 18).cloned().collect();
        // Fresh DB (user_version 0) ⇒ steps 1..=18 run contiguously (step 1 creates
        // `_fathomdb_embedder_profiles`; the mean_vec column + `_fathomdb_vector_kinds`
        // land at earlier steps). Profile row is inserted AFTER migrating, mirroring
        // the engine's order (migrate, then check_embedder_profile inserts it).
        migrate_with_steps(&conn, &steps_to_18).expect("migrate to v18");
        let ver: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(ver, 18, "precondition: DB is at v18");

        conn.execute(
            "INSERT INTO _fathomdb_embedder_profiles(profile, name, revision, dimension, mean_vec)
             VALUES('default', ?1, ?2, ?3, ?4)",
            rusqlite::params![BGE_NAME, BGE_REV, DIM as u32, all_ones_mean_blob()],
        )
        .expect("seed bge profile with a pinned mean");
        conn.execute(
            "INSERT INTO _fathomdb_vector_kinds(kind, profile, created_at) VALUES('note','default',0)",
            [],
        )
        .expect("seed pre-existing vector kind");

        // No probe references yet — exactly the post-upgrade state (the migration
        // creates the empty table at v19; the engine populates it at open).
        drop(conn);
    }

    // --- First v19 open: upgrade + establish the baseline (identity-gated) ------
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(BgeRefEmbedder))
        .expect("open must upgrade v18→v19 and establish the baseline");
    assert!(
        !opened.report.dense_disabled,
        "establishing the baseline at the upgrade open is never degraded"
    );
    assert_eq!(opened.report.schema_version_before, 18, "upgrade started at v18");
    assert_eq!(opened.report.schema_version_after, 22, "upgrade reached head (v22)");
    opened.engine.close().unwrap();

    let conn = rusqlite::Connection::open(&path).unwrap();
    let rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM _fathomdb_embed_probe", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 45, "the upgrade open must establish the 45-probe baseline");
    drop(conn);

    // --- Subsequent checks behave: same backend served, divergent refused -------
    let same = Engine::open_with_embedder_for_test(&path, Arc::new(BgeRefEmbedder))
        .expect("same-backend reopen");
    assert!(!same.report.dense_disabled, "same identity-matched backend ⇒ dense served");
    same.engine.close().unwrap();

    let divergent = Engine::open_with_embedder_for_test(&path, Arc::new(BgeMeanReflectEmbedder))
        .expect("divergent reopen (degraded)");
    assert!(
        divergent.report.dense_disabled,
        "a mean-centered divergent backend ⇒ dense refused (fail-SAFE)"
    );
    divergent.engine.close().unwrap();
}

// ---- fix-2 DEFECT #1 residual — the STORED baseline must be COMPLETE -----------
// `COUNT(*) > 0` is not proof of a trustworthy baseline. A partially populated or
// externally-tampered `_fathomdb_embed_probe` table (44 of 45 rows, a foreign
// substituted probe whose text+vector are self-consistent, or a re-attributed
// embedder identity) previously verified only the rows present / re-embedded a
// tampered `probe_text` against its OWN reference, and so served dense with
// `dense_disabled=false` — fail-OPEN. These reopen with the FAITHFUL backend (no
// embed drift) so the ONLY thing under test is the completeness validation: each
// must degrade the open, refuse the dense arm with `VectorEquivalenceMismatch`, and
// keep the text-only/FTS path serviceable.

/// LE-f32 encoding of a vector (matches the engine's `reference_vec` blob shape).
fn encode_le_f32(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

/// Assert the fail-SAFE contract after a baseline corruption: faithful reopen is
/// DEGRADED (not failed), a dense query refuses with the typed error, and the
/// text-only path still serves.
fn assert_degraded_but_fts_serves(path: &std::path::Path) {
    let opened = Engine::open_with_embedder_for_test(path, Arc::new(RefEmbedder))
        .expect("a corrupt/partial baseline must NOT wedge open (open still succeeds)");
    let engine = opened.engine;
    assert!(
        opened.report.dense_disabled,
        "an incomplete/tampered stored baseline must fail SAFE: dense refused"
    );
    assert!(opened.report.dense_disabled_reason.is_some(), "a refusal reason is surfaced");
    match engine.search("memory") {
        Err(EngineError::VectorEquivalenceMismatch { .. }) => {}
        other => panic!("dense query must refuse with VectorEquivalenceMismatch, got {other:?}"),
    }
    assert!(engine.search_text_only("memory").is_ok(), "FTS-only path must still serve");
    engine.close().unwrap();
}

/// Partial baseline (44 of 45 rows): DELETE one committed probe. Faithful reopen
/// must refuse dense (count mismatch), not verify only the 44 rows present.
#[test]
fn partial_baseline_missing_one_row_fails_safe_not_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    // Corrupt the STORED baseline: drop the last committed probe (44 of 45).
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("DELETE FROM _fathomdb_embed_probe WHERE probe_ordinal = 44", [])
            .expect("delete one probe row");
        let rows: i64 =
            conn.query_row("SELECT COUNT(*) FROM _fathomdb_embed_probe", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 44, "precondition: the stored baseline is now partial (44 of 45)");
    }

    assert_degraded_but_fts_serves(&path);
}

/// Substituted foreign probe (self-consistent text+vector): REPLACE one committed
/// probe's text with a NON-committed string AND its reference with that string's
/// faithful embedding. The old code re-embedded the stored text and compared it to
/// its OWN (matching) reference — so this fail-OPENED (0 flips / 0 L2). The
/// fixture-text validation now refuses it.
#[test]
fn substituted_probe_text_verifying_against_itself_fails_safe_not_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    {
        let foreign = "this-is-not-a-committed-probe";
        let foreign_vec = encode_le_f32(&reference_vector(foreign));
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE _fathomdb_embed_probe SET probe_text = ?1, reference_vec = ?2 \
             WHERE probe_ordinal = 5",
            rusqlite::params![foreign, foreign_vec],
        )
        .expect("substitute a self-consistent foreign probe");
    }

    assert_degraded_but_fts_serves(&path);
}

/// Re-attributed embedder identity: REWRITE the stored `embedder_name` to a foreign
/// value on every row. The old code ignored the identity columns at check time, so
/// this fail-OPENED. The identity validation now refuses it.
#[test]
fn re_attributed_embedder_identity_fails_safe_not_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("UPDATE _fathomdb_embed_probe SET embedder_name = 'a-different-embedder'", [])
            .expect("re-attribute the stored embedder identity");
    }

    assert_degraded_but_fts_serves(&path);
}

/// Non-contiguous ordinals WITHOUT a row-count change: move `probe_ordinal` 44 to a
/// gap value (45). Count stays 45 but the ordinals are no longer 0..=44. The old
/// code re-embedded each stored text and served; the contiguity validation refuses.
#[test]
fn non_contiguous_ordinals_fail_safe_not_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE _fathomdb_embed_probe SET probe_ordinal = 45 WHERE probe_ordinal = 44",
            [],
        )
        .expect("introduce an ordinal gap");
        let rows: i64 =
            conn.query_row("SELECT COUNT(*) FROM _fathomdb_embed_probe", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 45, "precondition: the row COUNT is still 45 (only the ordinal moved)");
    }

    assert_degraded_but_fts_serves(&path);
}

/// fix-3 — MANGLED/TRUNCATED reference blob (length != `4 * dim`): TRUNCATE one
/// committed probe's `reference_vec` by ONE byte so its stored length is
/// `4*dim - 1` while the row count, ordinals, text, and identity all stay intact.
/// `probe_check_against_baseline` validates each `reference_vec` is a well-formed
/// `4 * dim` f32 blob BEFORE it is decoded; a mangled blob must fail SAFE (dense
/// refused), never reach the decoder with a malformed length. This is the sibling
/// of the partial-row / substituted-probe / re-attributed-identity / non-contiguous
/// cases for the blob-length branch, which had no dedicated coverage.
///
/// The one-byte (non-multiple-of-4) truncation is deliberate and makes this test
/// NON-VACUOUS for THIS branch specifically. A whole-f32 (4-byte) truncation stays
/// a multiple of 4, so `decode_vector_blob` still decodes it and the downstream P1
/// flip-counter catches the packed-bit length delta anyway — the length branch
/// would be redundant for that input. A one-byte truncation is NOT a multiple of 4:
/// with the length branch REMOVED it reaches `decode_vector_blob`, whose
/// `chunks_exact(4)` would silently drop the trailing bytes (release) and whose
/// `debug_assert!(len % 4 == 0)` panics (debug/test) — proving the branch is the
/// load-bearing guard. Verified RED with the branch bypassed, GREEN with it present.
#[test]
fn mangled_reference_blob_wrong_length_fails_safe_not_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    seed_references(&path);

    // Corrupt the STORED baseline: truncate ONE reference_vec by a single byte so
    // its length is 4*dim - 1 (not a multiple of 4). Count/ordinals/text/identity
    // are untouched, so the ONLY thing under test is the blob well-formedness
    // (length) validation.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let full: Vec<u8> = conn
            .query_row(
                "SELECT reference_vec FROM _fathomdb_embed_probe WHERE probe_ordinal = 7",
                [],
                |r| r.get(0),
            )
            .expect("read the reference blob to mangle");
        assert_eq!(full.len(), DIM * 4, "precondition: the stored blob is a full 4*dim f32 blob");
        let truncated = full[..full.len() - 1].to_vec(); // drop one byte ⇒ 4*dim - 1 (non-%4)
        conn.execute(
            "UPDATE _fathomdb_embed_probe SET reference_vec = ?1 WHERE probe_ordinal = 7",
            rusqlite::params![truncated],
        )
        .expect("truncate one reference blob");
        let len: i64 = conn
            .query_row(
                "SELECT LENGTH(reference_vec) FROM _fathomdb_embed_probe WHERE probe_ordinal = 7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            len,
            (DIM * 4 - 1) as i64,
            "precondition: the stored blob is now the wrong length (not a multiple of 4)"
        );
    }

    assert_degraded_but_fts_serves(&path);
}
