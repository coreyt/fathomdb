//! 0.7.2 PR-2b RED -> GREEN — production mean-recompute engine slice.
//!
//! Covers the two recompute triggers (automatic in-ingest drift detector +
//! the `doctor recompute-mean` verb's `Engine::recompute_mean`), the
//! N>=200k dynamic cap (via the lowerable test seam), crash-atomicity of the
//! recompute tx, the `EmbedderEvent` surface (trigger / dim / doc_count /
//! deferred drift cos), no-drift stability, and the drift EFFICACY test that
//! PR-2a could not do (a SYNTHETIC topic-drift corpus where topic-B recall
//! improves after recompute).
//!
//! All tests use a deterministic in-process embedder that reports the
//! bge-small identity (so the engine treats it as mean-centering-required)
//! without candle or the network, so the suite runs under a plain
//! `cargo test -p fathomdb-engine`.

use std::collections::HashSet;
use std::sync::Arc;

use fathomdb_embedder::{EmbedderEvent, MeanRecomputeTrigger};
use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{EmbedderChoice, Engine, PreparedWrite, MEAN_VEC_PIN_THRESHOLD};
use rusqlite::Connection;
use tempfile::TempDir;

const DIM: u32 = 384;
const BGE_NAME: &str = "fathomdb-bge-small-en-v1.5";
const BGE_REV: &str = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";

// ── Deterministic bge-identity embedder (no candle, no network) ─────────
#[derive(Clone, Debug)]
struct SimulatedBgeEmbedder {
    identity: EmbedderIdentity,
}

impl Default for SimulatedBgeEmbedder {
    fn default() -> Self {
        Self { identity: EmbedderIdentity::new(BGE_NAME, BGE_REV, DIM) }
    }
}

impl Embedder for SimulatedBgeEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, input: &str) -> Result<Vector, EmbedderError> {
        Ok(topic_vector(input))
    }
}

fn hash64(input: &str) -> u64 {
    let mut seed: u64 = 0xcbf29ce484222325;
    for b in input.bytes() {
        seed ^= u64::from(b);
        seed = seed.wrapping_mul(0x100000001b3);
    }
    seed
}

/// Topic-aware unit vector. A body prefixed `A:` clusters around a fixed
/// topic-A direction, `B:` around an (almost) orthogonal topic-B direction,
/// each with per-doc noise; anything else is generic. This makes a
/// topic-A-skewed pinned mean systematically wrong for topic-B docs, which
/// is exactly the failure PR-2b's recompute must repair.
fn topic_vector(input: &str) -> Vector {
    let (topic, rest) = match input.split_once(':') {
        Some(("A", rest)) => (Some(false), rest),
        Some(("B", rest)) => (Some(true), rest),
        _ => (None, input),
    };
    let seed = hash64(rest);
    let mut v = vec![0.0f32; DIM as usize];
    // Per-doc signal (mean-zero), small enough that a topic-mismatched mean
    // offset swamps it (collapsing sign bits -> the PR-2a recall failure),
    // but recoverable once the CORRECT corpus mean removes the common-mode
    // topic offset.
    for (i, slot) in v.iter_mut().enumerate() {
        let mixed = seed.wrapping_add(i as u64).wrapping_mul(2654435761);
        *slot = (((mixed >> 8) as u32 as f32) / (u32::MAX as f32) - 0.5) * 0.7;
    }
    // Topic direction: a moderate per-block DC offset. The two topics' means
    // differ sharply (low cosine -> drives the detector). 2*offset (0.8)
    // exceeds the per-doc signal half-range (0.35), so centering a B doc by
    // an A-skewed mean pushes the whole B-block to one sign (loss of
    // per-doc discrimination); the correct B-dominated mean restores it.
    if let Some(is_b) = topic {
        let half = (DIM / 2) as usize;
        for (i, slot) in v.iter_mut().enumerate() {
            let in_b_block = i >= half;
            if in_b_block == is_b {
                *slot += 0.4;
            } else {
                *slot -= 0.4;
            }
        }
    }
    // Unit-normalize (bge vectors are unit-norm).
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
    for slot in &mut v {
        *slot /= norm;
    }
    v
}

// ── Harness helpers ─────────────────────────────────────────────────────

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}.sqlite"));
    (dir, path)
}

fn open_caller(
    path: &std::path::Path,
    embedder: Arc<dyn Embedder>,
) -> fathomdb_engine::OpenedEngine {
    Engine::open_with_choice(path, EmbedderChoice::Caller(embedder)).expect("open")
}

/// Write `count` `doc` nodes (bodies built by `body`) through the PRODUCTION
/// path in batches of `batch`, draining after each batch.
fn write_docs<F: Fn(usize) -> String>(engine: &Engine, count: usize, batch: usize, body: F) {
    let mut written = 0usize;
    while written < count {
        let take = batch.min(count - written);
        let nodes: Vec<PreparedWrite> = (0..take)
            .map(|i| PreparedWrite::Node {
                kind: "doc".to_string(),
                body: body(written + i),
                source_id: None,
            })
            .collect();
        engine.write(&nodes).expect("production write");
        written += take;
        engine.drain(60_000).expect("drain");
    }
}

fn read_mean_vec(path: &std::path::Path) -> Option<Vec<u8>> {
    let conn = Connection::open(path).expect("reopen");
    conn.query_row(
        "SELECT mean_vec FROM _fathomdb_embedder_profiles WHERE profile = 'default'",
        [],
        |row| row.get::<_, Option<Vec<u8>>>(0),
    )
    .expect("mean_vec query")
}

fn decode_f32(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn subtract(v: &[f32], mean: &[f32]) -> Vec<f32> {
    v.iter().zip(mean).map(|(a, b)| a - b).collect()
}

fn quantize_binary(conn: &Connection, vec: &[f32]) -> Vec<u8> {
    let json = serde_json::to_string(vec).expect("json");
    conn.query_row("SELECT vec_quantize_binary(vec_f32(?1))", [json], |r| r.get::<_, Vec<u8>>(0))
        .expect("vec_quantize_binary")
}

/// Closed-form full-corpus mean over the stored un-centered f32 BLOBs,
/// computed independently of the engine.
fn closed_form_mean(conn: &Connection) -> Vec<f32> {
    let mut stmt = conn.prepare("SELECT embedding FROM vector_default ORDER BY rowid").unwrap();
    let rows: Vec<Vec<u8>> =
        stmt.query_map([], |r| r.get::<_, Vec<u8>>(0)).unwrap().filter_map(Result::ok).collect();
    let mut sum = vec![0.0f64; DIM as usize];
    for blob in &rows {
        for (s, x) in sum.iter_mut().zip(decode_f32(blob)) {
            *s += f64::from(x);
        }
    }
    let n = rows.len().max(1) as f64;
    sum.iter().map(|s| (s / n) as f32).collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| f64::from(*x) * f64::from(*y)).sum();
    let na: f64 = a.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>().sqrt();
    (dot / (na * nb).max(1e-12)) as f32
}

// ── Tests ───────────────────────────────────────────────────────────────

/// (0) 0.7.2 PR-2bc S2 GUARD — the AUTOMATIC in-ingest drift detector is
/// CARVED OUT (deferred to 0.8.x). On a synthetic topic pivot (pin a
/// topic-A-skewed mean, then flood topic-B docs well past the old debounce
/// window) the engine must NOT auto-recompute mid-ingest: no
/// `MeanVecRecomputed { DriftAuto }`, no `MeanRecomputeDeferred`, and the
/// pinned `mean_vec` must be byte-identical before and after the B-flood.
/// The manual `doctor recompute-mean` path is unaffected (covered elsewhere).
///
/// RED on pre-carve-out code: the auto-detector fires on this exact pivot
/// (it is the old `drift_recompute_improves_topic_b_recall` Part 1 scenario),
/// staging a `MeanVecRecomputed { DriftAuto }` event and overwriting the
/// pinned mean — so the no-auto-event assertion (and the unchanged-mean
/// assertion) fail. GREEN after the detector is removed.
#[test]
fn topic_pivot_does_not_auto_recompute_mid_ingest() {
    let (_dir, path) = fixture_path("pr2bc_no_auto_drift");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    // Pin a topic-A-skewed mean, then snapshot the pinned mean.
    write_docs(&engine, MEAN_VEC_PIN_THRESHOLD as usize, 64, |i| format!("A:{i}"));
    let _ = engine.drain_embedder_events();
    let mean_after_pin = read_mean_vec(&path).expect("mean pinned after topic-A");

    // Flood topic-B far past the old debounce floor (256). Pre-carve-out this
    // is exactly what tripped the auto drift detector.
    write_docs(&engine, 600, 64, |i| format!("B:{i}"));
    let events = engine.drain_embedder_events().expect("drain events");

    // No automatic recompute event of any kind may be emitted mid-ingest.
    let auto_recomputed = events.iter().any(|e| {
        matches!(
            e,
            EmbedderEvent::MeanVecRecomputed { trigger: MeanRecomputeTrigger::DriftAuto, .. }
        )
    });
    assert!(!auto_recomputed, "auto drift detector must NOT fire mid-ingest; events={events:?}");
    let any_recomputed =
        events.iter().any(|e| matches!(e, EmbedderEvent::MeanVecRecomputed { .. }));
    assert!(
        !any_recomputed,
        "no MeanVecRecomputed may be emitted during ingest; events={events:?}"
    );

    // The pinned mean must be untouched by the topic-B flood.
    let mean_after_flood = read_mean_vec(&path).expect("mean still pinned after topic-B flood");
    assert_eq!(
        mean_after_pin, mean_after_flood,
        "pinned mean must be unchanged after the topic-B flood (no auto-recompute)"
    );
    engine.close().expect("close");
}

/// (1) Mechanical: `recompute_mean` produces the full-corpus mean (within
/// fp tolerance of an independent closed-form mean), re-quantizes every row,
/// and the pinned mean + stored sign-bits are mutually consistent after.
#[test]
fn manual_recompute_matches_closed_form_and_requantizes_all() {
    let (_dir, path) = fixture_path("pr2b_mechanical");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    // Pin a topic-A-skewed mean, then add topic-B docs (no auto recompute
    // forced; we drive the manual path explicitly below).
    write_docs(&engine, MEAN_VEC_PIN_THRESHOLD as usize, 64, |i| format!("A:{i}"));
    write_docs(&engine, 64, 64, |i| format!("B:{i}"));
    let _ = engine.drain_embedder_events();

    let report = engine.recompute_mean().expect("manual recompute");
    assert!(report.mean_was_pinned, "recompute must observe the prior pin");
    assert_eq!(report.dim, DIM);
    let total = engine.vector_row_count_for_test().expect("count");
    assert_eq!(
        report.doc_count_requantized, total,
        "recompute must re-quantize every row, got {} of {total}",
        report.doc_count_requantized
    );
    engine.close().expect("close");

    let conn = Connection::open(&path).expect("reopen");
    let pinned = decode_f32(&read_mean_vec(&path).expect("mean pinned"));
    let want = closed_form_mean(&conn);
    let cos = cosine(&pinned, &want);
    assert!(cos > 0.9999, "pinned mean must match closed-form full-corpus mean, cos={cos}");

    // Every row's stored sign-bits must equal the centering under the new mean.
    let mut stmt =
        conn.prepare("SELECT rowid, embedding, embedding_bin FROM vector_default").unwrap();
    let rows: Vec<(i64, Vec<u8>, Vec<u8>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(rows.len() as u64 == total);
    for (rid, emb, bin) in &rows {
        let want_bits = quantize_binary(&conn, &subtract(&decode_f32(emb), &pinned));
        assert_eq!(bin, &want_bits, "row {rid} sign-bits inconsistent with re-pinned mean");
    }
}

/// (2) Crash-atomicity: a fault between the `mean_vec` UPDATE and the
/// re-quantize completion rolls back fully — no half-recentered corpus.
#[test]
fn recompute_fault_rolls_back_fully() {
    let (_dir, path) = fixture_path("pr2b_atomicity");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    write_docs(&engine, MEAN_VEC_PIN_THRESHOLD as usize, 64, |i| format!("A:{i}"));
    write_docs(&engine, 64, 64, |i| format!("B:{i}"));
    let _ = engine.drain_embedder_events();

    let mean_before = read_mean_vec(&path).expect("mean pinned before");

    engine.force_next_recompute_failure_for_test();
    let err = engine.recompute_mean();
    assert!(err.is_err(), "injected fault must surface as an error, got {err:?}");
    engine.close().expect("close");

    let mean_after = read_mean_vec(&path).expect("mean still pinned after rollback");
    assert_eq!(mean_before, mean_after, "mean_vec must be unchanged after a rolled-back recompute");

    // Rows must still be consistent with the ORIGINAL mean (no partial recenter).
    let conn = Connection::open(&path).expect("reopen");
    let pinned = decode_f32(&mean_after);
    let mut stmt =
        conn.prepare("SELECT rowid, embedding, embedding_bin FROM vector_default").unwrap();
    let rows: Vec<(i64, Vec<u8>, Vec<u8>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    for (rid, emb, bin) in &rows {
        let want_bits = quantize_binary(&conn, &subtract(&decode_f32(emb), &pinned));
        assert_eq!(bin, &want_bits, "row {rid} must remain centered under the ORIGINAL mean");
    }
}

/// (3) Drift efficacy — the test PR-2a could not do. Build a synthetic
/// topic-drift corpus: pin a topic-A-skewed mean, then ingest many topic-B
/// docs. Assert (a) the auto detector fires (MeanVecRecomputed) AND
/// (b) topic-B recall@10 (engine path, target-excluded GT) improves after
/// recompute vs the wrong-mean baseline measured at the moment of pin.
#[test]
fn drift_recompute_improves_topic_b_recall() {
    // Part 1 — the auto detector fires on a topic pivot (A then B).
    {
        let (_dir, path) = fixture_path("pr2b_efficacy_auto");
        let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
        let engine = opened.engine;
        engine.configure_vector_kind_for_test("doc").expect("vector kind");
        write_docs(&engine, MEAN_VEC_PIN_THRESHOLD as usize, 64, |i| format!("A:{i}"));
        let _ = engine.drain_embedder_events();
        write_docs(&engine, 600, 64, |i| format!("B:{i}"));
        let events = engine.drain_embedder_events().expect("drain events");
        engine.close().expect("close");
        let recomputed = events.iter().any(|e| {
            matches!(
                e,
                EmbedderEvent::MeanVecRecomputed { trigger: MeanRecomputeTrigger::DriftAuto, .. }
            )
        });
        assert!(recomputed, "auto drift detector must fire on a topic pivot; events={events:?}");
    }

    // Part 2 — EFFICACY on a FIXED drifted corpus: with the A-skewed mean
    // still pinned, topic-B recall is depressed; a manual recompute (the
    // same core the auto path uses) repairs it. The corpus is held constant
    // across the two measurements so the mean is the only variable. The auto
    // cap seam is set so the drift detector does NOT silently recompute
    // before we take the wrong-mean baseline.
    let (_dir, path) = fixture_path("pr2b_efficacy_fixed");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    // Suppress the auto path entirely (cap = 0) so the A-skewed mean persists.
    engine.set_mean_recompute_dynamic_max_for_test(0);

    write_docs(&engine, MEAN_VEC_PIN_THRESHOLD as usize, 64, |i| format!("A:{i}"));
    // Many more B docs than A so the corpus is B-dominated: the A-skewed mean
    // is now wrong for the majority topic, depressing B sign-bit candidates.
    write_docs(&engine, 800, 64, |i| format!("B:{i}"));
    let _ = engine.drain_embedder_events();

    let embedder = SimulatedBgeEmbedder::default();
    let b_queries: Vec<String> = (0..60).map(|i| format!("B:{i}")).collect();
    let recall_before = measure_recall(&engine, &embedder, &b_queries);

    let report = engine.recompute_mean().expect("manual recompute");
    assert!(report.mean_was_pinned && report.drift_cos_before < 0.95);
    let recall_after = measure_recall(&engine, &embedder, &b_queries);
    engine.close().expect("close");

    assert!(
        recall_after > recall_before + 0.02,
        "topic-B recall@10 must improve measurably after recompute: before={recall_before:.3} after={recall_after:.3}"
    );
}

/// recall@10 over a query set, target-excluded, comparing the engine's
/// production path (sign-bit K + f32 rerank) against a brute-force f32 GT.
fn measure_recall(engine: &Engine, embedder: &dyn Embedder, queries: &[String]) -> f64 {
    // Snapshot all stored un-centered vectors + their bodies for GT.
    let conn = Connection::open(engine.path()).expect("reopen for GT");
    let mut stmt = conn
        .prepare(
            "SELECT canonical_nodes.body, vector_default.embedding
             FROM vector_default JOIN canonical_nodes
               ON canonical_nodes.write_cursor = vector_default.rowid
             ORDER BY vector_default.rowid",
        )
        .expect("prep gt");
    let docs: Vec<(String, Vec<f32>)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))
        .expect("gt query")
        .filter_map(Result::ok)
        .map(|(b, blob)| (b, decode_f32(&blob)))
        .collect();

    let mut per_query = Vec::with_capacity(queries.len());
    for q in queries {
        let qv = embedder.embed(q).expect("embed query");
        let mut idx: Vec<usize> = (0..docs.len()).collect();
        let dist = |v: &[f32]| -> f32 { v.iter().zip(&qv).map(|(a, b)| (a - b) * (a - b)).sum() };
        idx.sort_by(|&a, &b| {
            dist(&docs[a].1).partial_cmp(&dist(&docs[b].1)).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut gt: Vec<&String> = Vec::new();
        for &i in idx.iter() {
            if &docs[i].0 == q {
                continue; // target exclusion
            }
            gt.push(&docs[i].0);
            if gt.len() == 10 {
                break;
            }
        }
        let gt_set: HashSet<&String> = gt.into_iter().collect();
        let prod = engine.search(q).expect("prod search").results;
        let hits = prod.iter().filter(|b| *b != q).filter(|b| gt_set.contains(b)).count();
        per_query.push(hits as f64 / 10.0);
    }
    per_query.iter().sum::<f64>() / per_query.len().max(1) as f64
}

/// (4) No-drift stability: a homogeneous corpus does NOT trigger spurious
/// recomputes (threshold + debounce are not over-sensitive).
#[test]
fn homogeneous_corpus_does_not_recompute() {
    let (_dir, path) = fixture_path("pr2b_stable");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    // Keep ingesting the SAME topic well past the pin + several debounce
    // windows.
    write_docs(&engine, MEAN_VEC_PIN_THRESHOLD as usize + 1200, 64, |i| format!("A:{i}"));
    let events = engine.drain_embedder_events().expect("drain");
    engine.close().expect("close");

    let recomputes =
        events.iter().filter(|e| matches!(e, EmbedderEvent::MeanVecRecomputed { .. })).count();
    assert_eq!(recomputes, 0, "homogeneous corpus must not auto-recompute; events={events:?}");
}

/// (5) N>=cap: at/above the (lowered) cap the auto path is SUPPRESSED and
/// emits MeanRecomputeDeferred carrying the drift cos; `recompute_mean`
/// still recomputes regardless of the cap.
#[test]
fn cap_suppresses_auto_but_not_manual() {
    let (_dir, path) = fixture_path("pr2b_cap");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    // Lower the cap so the workspace is already "above 200k" by row count.
    engine.set_mean_recompute_dynamic_max_for_test(MEAN_VEC_PIN_THRESHOLD);

    write_docs(&engine, MEAN_VEC_PIN_THRESHOLD as usize, 64, |i| format!("A:{i}"));
    let _ = engine.drain_embedder_events();
    // Now drift with topic B past the debounce floor; the auto path is capped.
    write_docs(&engine, 400, 64, |i| format!("B:{i}"));
    let events = engine.drain_embedder_events().expect("drain");

    let deferred: Vec<&EmbedderEvent> = events
        .iter()
        .filter(|e| matches!(e, EmbedderEvent::MeanRecomputeDeferred { .. }))
        .collect();
    let auto_recomputes =
        events.iter().filter(|e| matches!(e, EmbedderEvent::MeanVecRecomputed { .. })).count();
    assert_eq!(auto_recomputes, 0, "auto recompute must be suppressed above the cap");
    assert!(!deferred.is_empty(), "a deferred notification must be surfaced; events={events:?}");
    let cos = deferred[0].deferred_drift_cos().expect("deferred drift cos");
    assert!((-1.0..=1.0).contains(&cos), "deferred drift cos must be a valid cosine, got {cos}");

    // The doctor verb is exempt: it recomputes even above the cap.
    let report = engine.recompute_mean().expect("manual recompute exempt from cap");
    assert!(report.doc_count_requantized >= MEAN_VEC_PIN_THRESHOLD);
    let after = engine.drain_embedder_events().expect("drain manual");
    assert!(
        after.iter().any(|e| matches!(
            e,
            EmbedderEvent::MeanVecRecomputed { trigger: MeanRecomputeTrigger::Manual, .. }
        )),
        "manual recompute must emit MeanVecRecomputed{{Manual}}; events={after:?}"
    );
    engine.close().expect("close");
}

/// (6) Events: MeanVecRecomputed carries the correct trigger/dim/doc_count
/// and is published only after the recompute is durable; deferred event
/// carries a drift cos.
#[test]
fn recompute_event_fields_and_post_commit_publish() {
    let (_dir, path) = fixture_path("pr2b_events");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    write_docs(&engine, MEAN_VEC_PIN_THRESHOLD as usize, 64, |i| format!("A:{i}"));
    write_docs(&engine, 64, 64, |i| format!("B:{i}"));
    let _ = engine.drain_embedder_events();

    let report = engine.recompute_mean().expect("manual recompute");
    let events = engine.drain_embedder_events().expect("drain");
    let recomputed: Vec<&EmbedderEvent> =
        events.iter().filter(|e| matches!(e, EmbedderEvent::MeanVecRecomputed { .. })).collect();
    assert_eq!(recomputed.len(), 1, "exactly one MeanVecRecomputed, got {events:?}");
    match recomputed[0] {
        EmbedderEvent::MeanVecRecomputed { dim, doc_count, trigger } => {
            assert_eq!(*dim, DIM);
            assert_eq!(*doc_count, report.doc_count_requantized);
            assert_eq!(*trigger, MeanRecomputeTrigger::Manual);
        }
        other => panic!("expected MeanVecRecomputed, got {other:?}"),
    }
    // Drained once already -> empty now (single delivery, durable channel).
    assert!(engine.drain_embedder_events().expect("drain2").is_empty());
    engine.close().expect("close");
}

/// A non-mean-centering caller embedder (distinct identity name), used to
/// drive the `recompute_mean` rejection path.
#[derive(Clone, Debug)]
struct NonMcEmbedder {
    identity: EmbedderIdentity,
}

impl Default for NonMcEmbedder {
    fn default() -> Self {
        Self { identity: EmbedderIdentity::new("fathomdb-noop", "0.6.0-scaffold", DIM) }
    }
}

impl Embedder for NonMcEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }
    fn embed(&self, input: &str) -> Result<Vector, EmbedderError> {
        Ok(topic_vector(input))
    }
}

/// (7) `recompute_mean` errors cleanly on a non-MC identity rather than
/// corrupting an un-centered workspace (drives the CLI non-clean exit path).
#[test]
fn recompute_rejects_non_mc_identity() {
    let (_dir, path) = fixture_path("pr2b_noop");
    let opened = open_caller(&path, Arc::new(NonMcEmbedder::default()));
    let engine = opened.engine;
    let err = engine.recompute_mean();
    assert!(err.is_err(), "non-MC identity must not be recomputable, got {err:?}");
}
