//! 0.8.18 Slice 5 (#5 vector-equivalence probe) — drift guard.
//!
//! The engine compiles the 45-probe fixture in via `include_str!` of a crate-local
//! copy (`src/vector_equivalence_probes.txt`) so the shipped library carries no
//! cross-crate `include_str!` (which would break `cargo publish`). This test pins
//! that crate-local copy byte-identical to the canonical source-of-truth in the
//! embedder crate's test fixtures, so the two never silently diverge.

use std::path::PathBuf;

#[test]
fn engine_probe_fixture_matches_embedder_source_of_truth() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let engine_copy = manifest.join("src/vector_equivalence_probes.txt");
    let canonical =
        manifest.join("../fathomdb-embedder/tests/fixtures/candle_onnx_equivalence_probes.txt");

    let engine_bytes = std::fs::read(&engine_copy)
        .unwrap_or_else(|e| panic!("read engine copy {engine_copy:?}: {e}"));
    let canonical_bytes = std::fs::read(&canonical)
        .unwrap_or_else(|e| panic!("read canonical fixture {canonical:?}: {e}"));

    assert_eq!(
        engine_bytes, canonical_bytes,
        "engine `src/vector_equivalence_probes.txt` must stay byte-identical to the \
         embedder crate's `candle_onnx_equivalence_probes.txt` (drift guard)"
    );

    // Sanity: exactly 45 non-comment, non-empty probes.
    let text = String::from_utf8(canonical_bytes).unwrap();
    let count = text
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.is_empty() && !t.starts_with('#')
        })
        .count();
    assert_eq!(count, 45, "the committed probe set must be exactly 45 probes");
}
