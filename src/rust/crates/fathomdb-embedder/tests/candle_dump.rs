//! PR-2c root-cause DIAGNOSTIC (not a CI test): dump real `CandleBgeEmbedder`
//! vectors for a set of identical input strings so they can be compared against
//! the offline HF-transformers pipeline. Answers: do candle and HF produce the
//! same embeddings for identical text? (If yes, the offline->real recall gap is
//! the retrieval pipeline, not the embeddings.)
//!
//! Reads /tmp/agree/in_NNN.txt (one input per file, whole-file = one string),
//! writes /tmp/agree/candle_NNN.txt (384 space-separated f32). Gated on the
//! `loader-test-hooks` feature AND env `EU_DUMP=1` so it never runs in normal
//! suites. Run:
//!   EU_DUMP=1 cargo test -p fathomdb-embedder --features loader-test-hooks \
//!     --release --test candle_dump -- --nocapture
#![cfg(feature = "loader-test-hooks")]

use std::fs;
use std::path::PathBuf;

use fathomdb_embedder::CandleBgeEmbedder;
use fathomdb_embedder_api::Embedder;

#[test]
fn dump_candle_vectors_for_agreement_check() {
    if std::env::var_os("EU_DUMP").is_none() {
        eprintln!("EU_DUMP not set; skipping candle dump diagnostic");
        return;
    }
    let dir = PathBuf::from("/tmp/agree");
    let mut inputs: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("read /tmp/agree")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            n.starts_with("in_") && n.ends_with(".txt")
        })
        .collect();
    inputs.sort();
    assert!(!inputs.is_empty(), "no /tmp/agree/in_*.txt inputs");

    let e = CandleBgeEmbedder::new().expect("CandleBgeEmbedder::new");
    eprintln!("EU_DUMP embedder identity = {:?}", e.identity());

    for p in &inputs {
        let text = fs::read_to_string(p).expect("read input");
        let v = e.embed(&text).expect("embed");
        let stem = p.file_name().unwrap().to_str().unwrap();
        let n = stem.trim_start_matches("in_").trim_end_matches(".txt");
        let out = dir.join(format!("candle_{n}.txt"));
        let body: String = v.iter().map(|x| format!("{x:.8}")).collect::<Vec<_>>().join(" ");
        fs::write(&out, body).expect("write candle vec");
        let norm: f32 = v.iter().map(|x| (x as &f32) * x).sum::<f32>().sqrt();
        eprintln!(
            "EU_DUMP in_{n}: chars={} dim={} norm={norm:.5} -> {}",
            text.len(),
            v.len(),
            out.display()
        );
    }
    eprintln!("EU_DUMP done: {} vectors written", inputs.len());
}
