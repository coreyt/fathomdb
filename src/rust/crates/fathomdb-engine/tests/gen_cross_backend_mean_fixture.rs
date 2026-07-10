//! 0.8.18 U3 (R-CAL-3) — GENERATOR for the REAL engine-pinned mean fixture.
//!
//! The cross-backend calibration harness
//! (`fathomdb-embedder/tests/cross_backend_calibration.rs`) measures the
//! Phase-1 binary-code flip count at the engine's **mean-centered**
//! representation `vec_quantize_binary(sign(x − mean_vec))`. A mean of ZERO is
//! raw-sign ONLY and is NOT a proxy for the engine's centered `embedding_bin`
//! (codex round-1 U3-a). This generator therefore captures a REAL pinned
//! `_fathomdb_embedder_profiles.mean_vec` produced by the engine's own
//! mean-pin mechanism and commits it as a small fixture the harness reads.
//!
//! Mechanism (engine `lib.rs`): ingesting ≥ `MEAN_VEC_PIN_THRESHOLD` (256)
//! vector rows through the production write→embed path pins the corpus mean
//! (`UPDATE _fathomdb_embedder_profiles SET mean_vec = …` + a re-quantize pass)
//! for the mean-centering-required default `fathomdb-bge-small-en-v1.5`
//! identity. We ingest 300 varied real-text docs with the SHIPPED
//! `CandleBgeEmbedder`, pinned to **CLS pooling** (matching the calibration
//! legs, which pin CLS on both backends per R-CAL-2), read the pinned blob, and
//! write it to the embedder crate's fixtures dir.
//!
//! This is a `#[ignore]`d ONE-OFF: run it explicitly to (re)produce the
//! committed fixture; it needs the warm candle HF cache + the `default-embedder`
//! feature and is NOT part of the normal `cargo test` closure.
//!
//! ```sh
//! FATHOMDB_EMBED_DEVICE=cpu cargo test -p fathomdb-engine \
//!     --features default-embedder --test gen_cross_backend_mean_fixture \
//!     -- --ignored --exact generate_cross_backend_pinned_mean_fixture --nocapture
//! ```
#![cfg(feature = "default-embedder")]

use std::sync::Arc;

use fathomdb_embedder::{CandleBgeEmbedder, Pooling};
use fathomdb_embedder_api::Embedder;
use fathomdb_engine::{EmbedderChoice, Engine, PreparedWrite, MEAN_VEC_PIN_THRESHOLD};
use rusqlite::Connection;
use tempfile::TempDir;

const DIM: usize = 384;
/// Ingest comfortably past the 256-row pin threshold so the pin fires.
const N_DOCS: usize = 300;

/// A pool of varied real-text sentence templates. Cycled with a per-doc index
/// so every ingested body is distinct — the pinned mean is then a plausible
/// corpus mean of the bge-small embedding space (not degenerate).
const TEMPLATES: &[&str] = &[
    "The quick brown fox jumps over the lazy dog near the riverbank.",
    "Quarterly revenue grew twelve percent after the product launch.",
    "She tuned the guitar before the concert began downtown.",
    "Photosynthesis converts sunlight into chemical energy in plants.",
    "The committee postponed the vote until the following Tuesday.",
    "A gentle rain fell across the valley through the afternoon.",
    "Distributed systems must tolerate partial network failures.",
    "He whisked the eggs and folded in the melted dark chocolate.",
    "The museum unveiled a restored painting from the Renaissance.",
    "Migratory birds navigate using the Earth's magnetic field.",
    "The startup raised a seed round to build developer tooling.",
    "Glaciers have retreated noticeably over the past few decades.",
    "The novel explores memory, loss, and the passage of time.",
    "Engineers reinforced the bridge after the seismic survey.",
    "Fresh basil and ripe tomatoes make a simple summer salad.",
    "The telescope captured a faint galaxy billions of light-years away.",
    "Interest rates influence borrowing costs across the economy.",
    "The marathon route winds through five historic neighborhoods.",
    "Coral reefs support a quarter of all marine species.",
    "The lecture covered gradient descent and backpropagation.",
    "A power outage delayed the evening train by an hour.",
    "The chef reduced the sauce until it coated the spoon.",
    "Volunteers planted native saplings along the eroded slope.",
    "The spacecraft entered orbit after a seven-month journey.",
    "Local farmers rotate crops to preserve the soil nutrients.",
    "The orchestra rehearsed the symphony's demanding final movement.",
    "Encryption protects data in transit and at rest.",
    "The hikers reached the summit just before the fog rolled in.",
    "The library digitized thousands of fragile historical documents.",
    "A balanced diet includes vegetables, grains, and lean protein.",
];

fn body(i: usize) -> String {
    let t = TEMPLATES[i % TEMPLATES.len()];
    format!("{t} (document #{i})")
}

fn write_docs(engine: &Engine, count: usize, batch: usize) {
    let mut written = 0usize;
    while written < count {
        let take = batch.min(count - written);
        let nodes: Vec<PreparedWrite> = (0..take)
            .map(|i| PreparedWrite::Node {
                kind: "doc".to_string(),
                body: body(written + i),
                source_id: None,
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
            })
            .collect();
        engine.write(&nodes).expect("production write");
        written += take;
        engine.drain(120_000).expect("drain");
    }
}

/// Repo-root-relative path to the embedder crate's fixtures dir. The engine
/// crate manifest is `<repo>/src/rust/crates/fathomdb-engine`.
fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fathomdb-embedder/tests/fixtures")
        .canonicalize()
        .expect("embedder fixtures dir must exist")
}

#[test]
#[ignore = "one-off generator: (re)produces the committed cross-backend pinned-mean fixture"]
fn generate_cross_backend_pinned_mean_fixture() {
    // Deterministic CPU: the fixture must be a CPU-pinned mean (the worktree
    // never builds embed-cuda). Set once at process start.
    std::env::set_var("FATHOMDB_EMBED_DEVICE", "cpu");

    let embedder = CandleBgeEmbedder::new()
        .expect("open CandleBgeEmbedder from the warm HF cache")
        .with_pooling(Pooling::Cls);
    let identity = embedder.identity();
    assert_eq!(identity.dimension as usize, DIM, "bge-small dim must be 384");
    let embedder: Arc<dyn Embedder> = Arc::new(embedder);

    let dir = TempDir::new().expect("tempdir");
    let db_path = dir.path().join("mean_fixture.sqlite");
    let opened =
        Engine::open_with_choice(&db_path, EmbedderChoice::Caller(embedder)).expect("open");
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("register vector kind");

    write_docs(&engine, N_DOCS, 64);
    let _ = engine.drain_embedder_events();

    // Read the REAL pinned mean the engine wrote (little-endian f32[384]).
    let conn = Connection::open(&db_path).expect("reopen db");
    let mean_blob: Vec<u8> = conn
        .query_row(
            "SELECT mean_vec FROM _fathomdb_embedder_profiles WHERE profile = 'default'",
            [],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .expect("mean_vec query")
        .expect("mean_vec must be PINNED after ingesting >= 256 vector rows");
    assert_eq!(
        mean_blob.len(),
        DIM * 4,
        "pinned mean_vec must be {} bytes (384 f32 little-endian), got {}",
        DIM * 4,
        mean_blob.len()
    );

    // Sanity: a real bge corpus mean is NOT the zero vector.
    let mean_f32: Vec<f32> =
        mean_blob.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    let l2: f64 = mean_f32.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>().sqrt();
    assert!(l2 > 1e-3, "pinned mean must be non-degenerate (‖mean‖₂={l2:.6})");

    let fixtures = fixtures_dir();
    let bin_path = fixtures.join("cross_backend_pinned_mean.f32");
    std::fs::write(&bin_path, &mean_blob).expect("write mean fixture bytes");

    let meta = format!(
        "{{\n  \"description\": \"REAL engine-pinned mean_vec (0.8.18 U3 R-CAL-3). Little-endian f32[384] in cross_backend_pinned_mean.f32.\",\n  \"embedder_name\": {:?},\n  \"embedder_revision\": {:?},\n  \"dim\": {DIM},\n  \"pooling\": \"cls\",\n  \"device\": \"cpu\",\n  \"docs_ingested\": {N_DOCS},\n  \"pin_threshold\": {MEAN_VEC_PIN_THRESHOLD},\n  \"mean_l2_norm\": {l2:.9}\n}}\n",
        identity.name, identity.revision,
    );
    let meta_path = fixtures.join("cross_backend_pinned_mean.json");
    std::fs::write(&meta_path, meta).expect("write mean fixture metadata");

    eprintln!(
        "GEN cross_backend_pinned_mean: wrote {} bytes to {} (‖mean‖₂={l2:.6}, {N_DOCS} docs)",
        mean_blob.len(),
        bin_path.display()
    );
}
