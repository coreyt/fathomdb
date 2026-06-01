//! Corpus-Pack 4 FTS validation gate.
//!
//! For a small deterministic subset of the corpus, pick a salient word
//! from each doc's body and assert that engine.search returns at least
//! one hit. Validates that the FTS5 path is wired end-to-end on
//! production-shaped docs across all 6 source_types.
//!
//! Correctness gate, not perf: runs at default `cargo test` scale and
//! is bounded by the per-source subset size below.

#[path = "support/corpus_harness.rs"]
mod corpus_harness;

use corpus_harness::{salient_word, CorpusFixture};
use std::collections::BTreeMap;

const PER_SOURCE: usize = 5;

#[test]
fn corpus_pack4_fts_returns_hits_for_salient_terms() {
    let fx = CorpusFixture::per_source(PER_SOURCE);
    let Some((_dir, engine)) = fx.open_or_skip() else { return };
    let nodes = fx.ingest_into(&engine).nodes;
    assert!(nodes > 0, "ingest wrote 0 nodes");

    // Pick a salient query word per doc; skip docs where we can't find
    // a long-enough unique-looking word.
    let mut queried = 0usize;
    let mut hit = 0usize;
    let mut miss_by_source: BTreeMap<String, usize> = BTreeMap::new();
    for doc in fx.docs() {
        let Some(term) = salient_word(&doc.body) else { continue };
        queried += 1;
        let result =
            engine.search(&term).unwrap_or_else(|e| panic!("search({term:?}) failed: {e:?}"));
        if !result.results.is_empty() {
            hit += 1;
        } else {
            *miss_by_source.entry(doc.source_type.clone()).or_insert(0) += 1;
        }
    }

    // Require at least 80% hit rate. FTS5 may legitimately miss a few
    // (e.g. ultra-short bodies, words that happen to land on stop-word
    // lists). This bar is generous on purpose: the test is a wiring
    // gate, not a recall measurement.
    assert!(queried >= 20, "need >=20 queried docs to be statistically meaningful, got {queried}");
    let hit_rate = hit as f64 / queried as f64;
    assert!(
        hit_rate >= 0.80,
        "FTS hit rate {:.2} below floor 0.80 — queried {} docs, hit {}, misses by source: {:?}",
        hit_rate,
        queried,
        hit,
        miss_by_source
    );
}
