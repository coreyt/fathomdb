//! Corpus-Pack 4 vector validation gate.
//!
//! Verifies the vector path is wired end-to-end on production-shaped
//! corpus docs:
//!
//!   1. Ingestion populates `vector_default` (the sqlite-vec table)
//!      with one row per vector-indexed canonical node.
//!   2. `engine.search` returns non-empty results for queries whose
//!      embedding matches an ingested doc.
//!
//! This is a wiring gate, not a recall floor. The real recall@10
//! ≥ 0.90 measurement is owned by the PVQ Pack 2 AC-013b RED test,
//! which runs against the canonical 1M-row fixture under
//! `AGENT_LONG=1` per
//! `dev/notes/ac013-ac019-canonical-scale-policy.md`.
//!
//! A top-K self-recall assertion was prototyped here but moved to the
//! RED test instead — VaryingEmbedder's 6-coordinate hash placement
//! has high mutual-collision rates on natural-language bodies with
//! shared structure (e.g. bahmutov daily-log notes that all start
//! with bullet-list markers), which made statistical self-recall an
//! unreliable wiring gate. The two assertions below are direct
//! contract checks that don't depend on KNN scoring quality.

#[path = "support/corpus_harness.rs"]
mod corpus_harness;

use std::path::Path;

use corpus_harness::{load_subset_or_skip, CorpusFixture};
use rusqlite::Connection;

const PER_SOURCE: usize = 5;
/// Bodies above this size get chunked at ingest; see the chunking
/// discussion in the recall-floor RED test. We constrain to short
/// bodies here so the per-source ingest count and vec0 row count
/// align 1:1 — a chunked long body produces N vec0 rows for one Node
/// write, breaking the count assertion below.
const BODY_LEN_CAP: usize = 600;

fn open_readonly(path: &Path) -> Connection {
    Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only sqlite")
}

#[test]
fn corpus_pack4_vector_path_wired_end_to_end() {
    let Some(all_docs) = load_subset_or_skip(PER_SOURCE) else { return };
    let docs: Vec<_> = all_docs
        .into_iter()
        .filter(|d| {
            let body = d.body.trim();
            !body.is_empty() && body.len() <= BODY_LEN_CAP
        })
        .collect();
    assert!(docs.len() >= 20, "need >=20 short-body docs, got {}", docs.len());

    let fx = CorpusFixture::from_docs("corpus_vector", docs);
    let Some((_dir, engine)) = fx.open_or_skip() else { return };
    let nodes = fx.ingest_into(&engine).nodes;
    assert!(nodes > 0, "ingest wrote 0 nodes");

    // Assertion 1 — engine.search returns non-empty results for a
    // body-derived query. Run this BEFORE closing the engine so we
    // don't have to deal with reopen semantics.
    let mut queried = 0usize;
    let mut non_empty = 0usize;
    for doc in fx.docs() {
        if doc.body.trim().is_empty() {
            continue;
        }
        queried += 1;
        let result = engine
            .search(&doc.body)
            .unwrap_or_else(|e| panic!("search failed for doc {}: {e:?}", doc.doc_id));
        if !result.results.is_empty() {
            non_empty += 1;
        }
    }
    let hit_rate = non_empty as f64 / queried as f64;
    assert!(
        hit_rate >= 0.90,
        "engine.search returned empty for {}/{} queries (hit_rate={:.2}); vector+FTS branches both silent — wiring suspect",
        queried - non_empty,
        queried,
        hit_rate
    );

    // Assertion 2 — vec0 was populated. After drain + close, peek at
    // vector_default directly. We expect at least one vec0 row per
    // short-body ingested doc.
    let db_path = engine.path().to_path_buf();
    engine.drain(15_000).expect("drain");
    engine.close().expect("close");

    let conn = open_readonly(&db_path);
    let vec_count: i64 = conn
        .query_row("SELECT count(*) FROM vector_default", [], |row| row.get(0))
        .expect("count vector_default");
    assert!(
        vec_count >= nodes as i64,
        "vector_default has {vec_count} rows but {nodes} short-body docs were ingested; \
         projection apparently dropped some — vector path is NOT wired end-to-end",
    );

    let kinds_count: i64 = conn
        .query_row("SELECT count(*) FROM _fathomdb_vector_kinds WHERE kind = 'doc'", [], |row| {
            row.get(0)
        })
        .expect("count vector kinds");
    assert_eq!(kinds_count, 1, "configure_vector_kind_for_test('doc') did not register the kind",);
}
