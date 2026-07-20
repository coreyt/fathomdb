//! EU-5f RED tests — mean-centering pin on the PRODUCTION write path +
//! projection fault-isolation.
//!
//! Per `dev/plans/runs/0.7.1-EU-7-findings.md`. Two engine defects:
//!
//! Finding B — mean-centering (locked ON) only pins via the synchronous
//! `write_vector_for_test` test seam. The production `engine.write` ->
//! projection path (`run_projection_job` + `commit_projection_outcomes`)
//! never feeds the mean accumulator and never pins, so real ingests leave
//! `_fathomdb_embedder_profiles.mean_vec` NULL and sign-bits uncentered.
//!
//! Finding A — `run_projection_jobs` has no panic/fault guard, so a worker
//! that faults inside `embed()` never decrements `active_jobs` and `drain`
//! wedges into `EngineError::Scheduler`.
//!
//! These tests drive the PRODUCTION path only (`engine.write` + `drain`),
//! never `write_vector_for_test`. They use a deterministic in-process
//! embedder that reports the bge-small identity name (so the engine treats
//! it as mean-centering-required) without touching candle or the network —
//! so the suite runs under a plain `cargo test -p fathomdb-engine`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use fathomdb_embedder::EmbedderEvent;
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
        Ok(deterministic_vector(input))
    }
}

/// Deterministic per-input vector with a non-trivial signed distribution so
/// the corpus mean is non-zero and centering flips a meaningful number of
/// sign bits.
fn deterministic_vector(input: &str) -> Vector {
    let mut seed: u64 = 0xcbf29ce484222325;
    for b in input.bytes() {
        seed ^= u64::from(b);
        seed = seed.wrapping_mul(0x100000001b3);
    }
    let mut v = vec![0.0f32; DIM as usize];
    for (i, slot) in v.iter_mut().enumerate() {
        let mixed = seed.wrapping_add(i as u64).wrapping_mul(2654435761);
        // Skew positive so the corpus mean is clearly non-zero -> centering
        // changes the sign of many coordinates.
        *slot = ((mixed >> 8) as u32 as f32) / (u32::MAX as f32) - 0.35;
    }
    v
}

/// Embedder that reports the bge identity but panics on the Nth `embed`
/// call — exercises Finding A (a faulting projection worker must not wedge
/// `drain`).
#[derive(Debug)]
struct PanickingBgeEmbedder {
    identity: EmbedderIdentity,
    calls: AtomicU64,
    panic_at: u64,
}

impl PanickingBgeEmbedder {
    fn new(panic_at: u64) -> Self {
        Self {
            identity: EmbedderIdentity::new(BGE_NAME, BGE_REV, DIM),
            calls: AtomicU64::new(0),
            panic_at,
        }
    }
}

impl Embedder for PanickingBgeEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, input: &str) -> Result<Vector, EmbedderError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if n == self.panic_at {
            panic!("PanickingBgeEmbedder: induced panic on embed call {n}");
        }
        Ok(deterministic_vector(input))
    }
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

/// Write `count` distinct `doc` nodes through the PRODUCTION path in
/// batches of `batch`, draining after each batch.
fn write_docs_production(engine: &Engine, start: usize, count: usize, batch: usize) {
    let mut written = 0usize;
    while written < count {
        let take = batch.min(count - written);
        let nodes: Vec<PreparedWrite> = (0..take)
            .map(|i| PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("doc-{}", start + written + i),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
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

fn first_rowid(conn: &Connection) -> i64 {
    conn.query_row("SELECT rowid FROM vector_default ORDER BY rowid LIMIT 1", [], |r| r.get(0))
        .expect("first rowid")
}

/// The canonical sign-bit packing sqlite-vec produces for `vec`, obtained
/// from the process-global `vec_quantize_binary` itself (no host-side
/// packing assumptions). sqlite-vec is loaded process-globally after any
/// `Engine::open`, so a bare rusqlite connection can call it.
fn quantize_binary(conn: &Connection, vec: &[f32]) -> Vec<u8> {
    let json = serde_json::to_string(vec).expect("json");
    conn.query_row("SELECT vec_quantize_binary(vec_f32(?1))", [json], |r| r.get::<_, Vec<u8>>(0))
        .expect("vec_quantize_binary")
}

fn decode_f32(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn decode_mean(blob: &[u8]) -> Vec<f32> {
    decode_f32(blob)
}

fn subtract(v: &[f32], mean: &[f32]) -> Vec<f32> {
    v.iter().zip(mean).map(|(a, b)| a - b).collect()
}

// ── Tests ───────────────────────────────────────────────────────────────

/// (1) Writing >= MEAN_VEC_PIN_THRESHOLD docs through the production path
/// pins the corpus mean and OpenReport reports it on reopen.
#[test]
fn production_write_pins_mean_vec() {
    let (_dir, path) = fixture_path("eu5f_pin");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    write_docs_production(&engine, 0, MEAN_VEC_PIN_THRESHOLD as usize, 64);
    engine.close().expect("close");

    let mean_blob = read_mean_vec(&path);
    assert!(
        mean_blob.is_some(),
        "production engine.write of {} docs must pin mean_vec (Finding B)",
        MEAN_VEC_PIN_THRESHOLD
    );
    assert_eq!(mean_blob.unwrap().len(), DIM as usize * 4, "mean_vec is dim f32");

    // OpenReport surfaces the pinned state on reopen.
    let reopened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    assert!(
        reopened.report.embedder_mean_vec_pinned,
        "OpenReport.embedder_mean_vec_pinned must be true after production pin"
    );
}

/// (2) Pre-pin rows are re-quantized with the pinned mean: an early row's
/// stored sign-bits become the CENTERED quantization (and differ from the
/// uncentered one).
#[test]
fn prepin_rows_requantized() {
    let (_dir, path) = fixture_path("eu5f_requantize");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    // Cross the threshold so the early rows get re-quantized at pin time.
    write_docs_production(&engine, 0, MEAN_VEC_PIN_THRESHOLD as usize + 8, 64);
    engine.close().expect("close");

    let conn = Connection::open(&path).expect("reopen");
    let mean = decode_mean(&read_mean_vec(&path).expect("mean pinned"));
    let rid = first_rowid(&conn);
    let embedding = decode_f32(
        &conn
            .query_row("SELECT embedding FROM vector_default WHERE rowid=?1", [rid], |r| {
                r.get::<_, Vec<u8>>(0)
            })
            .expect("embedding"),
    );
    let stored_bin: Vec<u8> = conn
        .query_row("SELECT embedding_bin FROM vector_default WHERE rowid=?1", [rid], |r| {
            r.get::<_, Vec<u8>>(0)
        })
        .expect("embedding_bin");

    let want_centered = quantize_binary(&conn, &subtract(&embedding, &mean));
    let uncentered = quantize_binary(&conn, &embedding);
    assert_ne!(want_centered, uncentered, "fixture must make centering observable");
    assert_eq!(
        stored_bin, want_centered,
        "pre-pin row must be re-quantized with the pinned mean (Finding B)"
    );
}

/// (3) Rows written AFTER the pin are centered at insert time.
#[test]
fn postpin_rows_centered() {
    let (_dir, path) = fixture_path("eu5f_postpin");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    // First cross the threshold, then write a clearly-post-pin doc.
    write_docs_production(&engine, 0, MEAN_VEC_PIN_THRESHOLD as usize, 64);
    write_docs_production(&engine, 100_000, 1, 1); // distinct body, post-pin
    engine.close().expect("close");

    let conn = Connection::open(&path).expect("reopen");
    let mean = decode_mean(&read_mean_vec(&path).expect("mean pinned"));
    let last_rid: i64 = conn
        .query_row("SELECT rowid FROM vector_default ORDER BY rowid DESC LIMIT 1", [], |r| r.get(0))
        .expect("last rowid");
    let embedding = decode_f32(
        &conn
            .query_row("SELECT embedding FROM vector_default WHERE rowid=?1", [last_rid], |r| {
                r.get::<_, Vec<u8>>(0)
            })
            .expect("embedding"),
    );
    let stored_bin: Vec<u8> = conn
        .query_row("SELECT embedding_bin FROM vector_default WHERE rowid=?1", [last_rid], |r| {
            r.get::<_, Vec<u8>>(0)
        })
        .expect("embedding_bin");
    assert_eq!(
        stored_bin,
        quantize_binary(&conn, &subtract(&embedding, &mean)),
        "post-pin row must be centered at insert time"
    );
}

/// (4) The pin happens exactly once under many small interleaved batches,
/// and NO row survives uncentered.
#[test]
fn pin_once_no_uncentered_survivors() {
    let (_dir, path) = fixture_path("eu5f_pin_once");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    // Many small batches to maximize 2-worker interleaving across the pin.
    write_docs_production(&engine, 0, 600, 8);
    let events = engine.drain_mean_centering_events_for_test().expect("drain events");
    let pins = events.iter().filter(|e| matches!(e, EmbedderEvent::MeanVecPinned { .. })).count();
    engine.close().expect("close");

    assert_eq!(pins, 1, "exactly one MeanVecPinned across the run, got {pins}");

    // Every row must equal its centered quantization (no uncentered survivors).
    let conn = Connection::open(&path).expect("reopen");
    let mean = decode_mean(&read_mean_vec(&path).expect("mean pinned"));
    let mut stmt =
        conn.prepare("SELECT rowid, embedding, embedding_bin FROM vector_default").expect("prep");
    let rows: Vec<(i64, Vec<u8>, Vec<u8>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .filter_map(Result::ok)
        .collect();
    assert!(rows.len() >= 600, "all docs indexed");
    for (rid, emb, bin) in &rows {
        let want = quantize_binary(&conn, &subtract(&decode_f32(emb), &mean));
        assert_eq!(bin, &want, "row {rid} must be centered (no uncentered survivor)");
    }
}

/// (5) Finding A — a panicking embedder must not wedge `drain` into a
/// permanent `EngineError::Scheduler`. After the fix, the faulted job is
/// recorded and `drain` returns.
#[test]
fn panicking_embedder_does_not_wedge_drain() {
    let (_dir, path) = fixture_path("eu5f_panic");
    let opened = open_caller(&path, Arc::new(PanickingBgeEmbedder::new(3)));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    let nodes: Vec<PreparedWrite> = (0..8)
        .map(|i| PreparedWrite::Node {
            kind: "doc".to_string(),
            body: format!("panic-doc-{i}"),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        })
        .collect();
    engine.write(&nodes).expect("write");
    // With the fault-isolation guard, the panicked job is recorded and the
    // scheduler reaches idle, so drain returns Ok. Without it, active_jobs
    // stays elevated and drain times out into EngineError::Scheduler.
    let drained = engine.drain(15_000);
    assert!(
        drained.is_ok(),
        "drain must not wedge on a panicking embedder (Finding A); got {drained:?}"
    );
}

/// (6) Recovery — a workspace with >= threshold rows but mean_vec NULL
/// (e.g. crash before the pin committed) must re-pin on the next open.
#[test]
fn recovery_pins_on_open_when_mean_null() {
    let (_dir, path) = fixture_path("eu5f_recovery");
    let opened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    write_docs_production(&engine, 0, MEAN_VEC_PIN_THRESHOLD as usize + 4, 64);
    engine.close().expect("close");

    // Simulate a crash-before-pin: blank the pinned mean out of band.
    {
        let conn = Connection::open(&path).expect("reopen raw");
        conn.execute(
            "UPDATE _fathomdb_embedder_profiles SET mean_vec = NULL WHERE profile = 'default'",
            [],
        )
        .expect("blank mean");
    }
    assert!(read_mean_vec(&path).is_none(), "precondition: mean blanked");

    // Reopen: recovery must re-pin because rows >= threshold and mean NULL.
    let reopened = open_caller(&path, Arc::new(SimulatedBgeEmbedder::default()));
    assert!(
        reopened.report.embedder_mean_vec_pinned,
        "open must recover-pin when rows >= threshold and mean_vec NULL (Hazard 4)"
    );
    assert!(read_mean_vec(&path).is_some(), "mean_vec must be repinned after recovery open");
}
