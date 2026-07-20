//! 0.8.20 Slice 10a (TC-31) — `source_id` must be readable on EVERY search-hit
//! path, not just the graph arm.
//!
//! **The defect.** 0.8.20 made provenance structurally mandatory on write
//! (`SourceId` newtype on every `PreparedWrite`) and shipped `erase_source` as
//! the GDPR erasure verb. But `SearchHit.source_id` was populated by the graph
//! arm ONLY: every text/BM25F, vector and edge-FTS hit hard-coded
//! `source_id: None`. A caller who received a text or vector hit therefore had
//! no way to obtain the `source_id` that `erase_source` consumes — the erasure
//! verb shipped with its argument unreachable. 0.8.19 also stopped surfacing
//! `write_cursor` through the SDK bindings, so there was no fallback route from
//! a hit back to its source document either.
//!
//! **Test-design contract** (inherited from Slice 5,
//! `erasure_projection_registry.rs` module docs, design
//! `0.8.20-slice0-erasure-design.md` §3 Rule 1): every erasure witness here
//! asserts on RAW TABLE CONTENTS after `close()` + a fresh `rusqlite` open. A
//! `search()`-based assertion is INVALID as an erasure witness — both
//! `search_index_v2` read paths discard candidates lacking a live
//! `canonical_nodes` row, so a search for the erased text passes on the BROKEN
//! code. The leak is data-at-rest and never surfaces in results.
//!
//! Each test walks the full user-facing contract for one arm:
//!   write(known source_id) → retrieve via THAT arm → read `hit.source_id` →
//!   feed exactly that value to `erase_source` → raw-assert the content is gone.
//!
//! **Synthetic passages (`rerank_passages`) are deliberately not covered.** That
//! `SearchHit` is a private adapter shape: `rerank_passages` takes caller-supplied
//! `(ordinal, body, score)` tuples that have no canonical row and no parent node
//! reachable from the function (it is a pure function with no database handle),
//! and it projects back to `(id, score, ce_score)` — the `SearchHit` never
//! escapes to a caller. There is therefore no user-facing `source_id` to
//! populate on that path, and inventing one would be fabricating provenance.

use std::sync::Arc;
use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite, SearchResult, SoftFallbackBranch};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

/// Deterministic embedder: every text maps to the same unit vector, so the
/// vector branch always surfaces the candidate with a finite rerank distance.
#[derive(Clone, Debug)]
struct FixedEmbedder;

impl Embedder for FixedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("deterministic", "rev-a", 8)
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; 8];
        v[0] = 1.0;
        Ok(v)
    }
}

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn node(body: &str, source_id: &str, logical_id: Option<&str>) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: fathomdb_engine::SourceId::new(source_id).expect("test source id"),
        logical_id: logical_id.map(str::to_string),
        state: fathomdb_engine::InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn edge(from: &str, to: &str, logical_id: &str, source_id: &str, body: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: fathomdb_engine::SourceId::new(source_id).expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        body: Some(body.to_string()),
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

/// Poll `search` until the projection has caught up past `min_cursor` and the
/// query actually returns something (the embed/projection workers are async).
fn search_after_projection(engine: &Engine, query: &str, min_cursor: u64) -> SearchResult {
    let started = Instant::now();
    loop {
        let result = engine.search(query).expect("search");
        if result.projection_cursor >= min_cursor && !result.results.is_empty() {
            return result;
        }
        if started.elapsed() > Duration::from_secs(10) {
            return result;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn count(conn: &Connection, sql: &str) -> u64 {
    conn.query_row(sql, [], |row| row.get(0)).expect("count query")
}

/// RAW witness helper: after `erase_source(sid)`, no canonical row may remain
/// for that provenance and the secret body must be gone from every
/// content-storing table (`search_index_v2` stores bodies verbatim; so does
/// `search_index` / `search_index_edges`).
fn assert_erased_raw(path: &std::path::Path, source_id: &str, secret: &str) {
    let conn = Connection::open(path).expect("open sqlite raw");

    for table in ["canonical_nodes", "canonical_edges"] {
        let residue: u64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE source_id = ?1"),
                [source_id],
                |r| r.get(0),
            )
            .expect("residue count");
        assert_eq!(residue, 0, "{table} still holds rows for erased source_id={source_id}");
    }

    for table in ["search_index_v2", "search_index", "search_index_edges"] {
        // The table may not exist on a schema that never created it; a missing
        // table is trivially residue-free, but an existing one must be clean.
        let exists: u64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name = ?1", [table], |r| r.get(0))
            .expect("sqlite_master");
        if exists == 0 {
            continue;
        }
        let residue =
            count(&conn, &format!("SELECT COUNT(*) FROM {table} WHERE body LIKE '%{secret}%'"));
        assert_eq!(
            residue, 0,
            "{table} still stores the erased body verbatim (secret={secret:?}) — \
             data-at-rest leak, invisible to search()"
        );
    }
}

// ---------------------------------------------------------------------------
// TC-31 — text / BM25F arm
// ---------------------------------------------------------------------------

#[test]
fn tc31_text_hit_carries_source_id_and_erases() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc31_text");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let receipt = opened
        .engine
        .write(&[node("tc31textsecret confidential dossier", "SRC-TEXT-1", None)])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "tc31textsecret", receipt.cursor);
    let hit = result
        .results
        .iter()
        .find(|h| h.body.contains("tc31textsecret"))
        .expect("the text arm must surface the document");
    assert_eq!(hit.branch, SoftFallbackBranch::Text, "must be the text/BM25F arm");
    let sid = hit
        .source_id
        .clone()
        .expect("TC-31: a text hit MUST carry the node's own source_id (was hard-coded None)");
    assert_eq!(sid, "SRC-TEXT-1", "the hit must carry the provenance that was written");

    // The whole point: the value read off the hit is the erasure key.
    opened.engine.erase_source(&sid).expect("erase_source with the hit's own source_id");
    opened.engine.close().unwrap();

    assert_erased_raw(&path, "SRC-TEXT-1", "tc31textsecret");
}

// ---------------------------------------------------------------------------
// TC-31 — vector arm (node hit)
// ---------------------------------------------------------------------------

#[test]
fn tc31_vector_hit_carries_source_id_and_erases() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc31_vector");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    // The query term is NOT in the body, so the ONLY arm that can surface this
    // row is the vector arm.
    let receipt = opened
        .engine
        .write(&[node("tc31vecsecret semantic only payload", "SRC-VEC-1", None)])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "vectorize", receipt.cursor);
    let hit = result
        .results
        .iter()
        .find(|h| h.body.contains("tc31vecsecret"))
        .expect("the vector arm must surface the document");
    assert_eq!(hit.branch, SoftFallbackBranch::Vector, "must be the vector arm");
    let sid = hit
        .source_id
        .clone()
        .expect("TC-31: a vector hit MUST carry the node's own source_id (was hard-coded None)");
    assert_eq!(sid, "SRC-VEC-1", "the hit must carry the provenance that was written");

    opened.engine.erase_source(&sid).expect("erase_source with the hit's own source_id");
    opened.engine.close().unwrap();

    assert_erased_raw(&path, "SRC-VEC-1", "tc31vecsecret");
}

// ---------------------------------------------------------------------------
// TC-31 — edge FTS arm (`search_index_edges`)
// ---------------------------------------------------------------------------

#[test]
fn tc31_edge_fts_hit_carries_source_id_and_erases() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc31_edge");
    let opened = Engine::open(&path).expect("open");

    let receipt = opened
        .engine
        .write(&[
            node("anna the first entity", "SRC-EDGE-1", Some("anna")),
            node("bob the second entity", "SRC-EDGE-1", Some("bob")),
            edge("anna", "bob", "edge-ab", "SRC-EDGE-1", "tc31edgesecret anna trusts bob"),
        ])
        .expect("write");

    let result = search_after_projection(&opened.engine, "tc31edgesecret", receipt.cursor);
    let hit = result
        .results
        .iter()
        .find(|h| h.body.contains("tc31edgesecret"))
        .expect("the edge-FTS arm must surface the edge body");
    assert_eq!(hit.branch, SoftFallbackBranch::TextEdge, "must be the edge-FTS arm");
    let sid = hit
        .source_id
        .clone()
        .expect("TC-31: an edge-FTS hit MUST carry the edge's own source_id (was hard-coded None)");
    assert_eq!(sid, "SRC-EDGE-1", "the hit must carry the provenance that was written");

    opened.engine.erase_source(&sid).expect("erase_source with the hit's own source_id");
    opened.engine.close().unwrap();

    assert_erased_raw(&path, "SRC-EDGE-1", "tc31edgesecret");
}

// ---------------------------------------------------------------------------
// TC-31 — graph arm (semantics UNCHANGED: the traversed EDGE's source_id)
// ---------------------------------------------------------------------------

/// The graph arm already populated `source_id` before TC-31. This test is the
/// regression pin that TC-31 did not disturb it: a graph-reached node still
/// carries the TRAVERSED EDGE's provenance, and that value still drives erasure.
#[test]
fn tc31_graph_arm_hit_carries_source_id_and_erases() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc31_graph");
    let opened = Engine::open(&path).expect("open");

    opened
        .engine
        .write(&[
            node("carol tc31graphanchor entity", "SRC-GRAPH-1", Some("carol")),
            node("tc31graphsecret dave neighbor node", "SRC-GRAPH-1", Some("dave")),
            edge("carol", "dave", "edge-cd", "SRC-GRAPH-1", "carol knows dave"),
        ])
        .expect("write");
    // Let the projection worker catch up (the seed must be FTS-visible).
    let _ = search_after_projection(&opened.engine, "tc31graphanchor", 0);

    let result = opened
        .engine
        .search_reranked("tc31graphanchor", None, 0, true, 0.3, 0)
        .expect("search with graph arm");
    let hit = result
        .results
        .iter()
        .find(|h| h.body.contains("tc31graphsecret"))
        .expect("dave must be graph-reached from the carol seed");
    assert_eq!(hit.branch, SoftFallbackBranch::GraphArm, "must be the graph arm");
    let sid =
        hit.source_id.clone().expect("a graph-arm hit carries the traversed edge's source_id");
    assert_eq!(sid, "SRC-GRAPH-1");

    opened.engine.erase_source(&sid).expect("erase_source with the hit's own source_id");
    opened.engine.close().unwrap();

    assert_erased_raw(&path, "SRC-GRAPH-1", "tc31graphsecret");
}

// ---------------------------------------------------------------------------
// TC-31 — provenance is per-row, not a global constant
// ---------------------------------------------------------------------------

/// Guards against the cheapest possible fake fix (stamping one source_id on
/// every hit): two documents with DIFFERENT provenance must report their own,
/// and erasing one must leave the other completely intact at rest.
#[test]
fn tc31_hits_carry_their_own_source_id_not_a_shared_constant() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc31_per_row");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let receipt = opened
        .engine
        .write(&[
            node("tc31shared marker alpha document", "SRC-A", None),
            node("tc31shared marker beta document", "SRC-B", None),
        ])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "tc31shared", receipt.cursor);
    let alpha = result
        .results
        .iter()
        .find(|h| h.body.contains("alpha"))
        .expect("alpha hit")
        .source_id
        .clone();
    let beta = result
        .results
        .iter()
        .find(|h| h.body.contains("beta"))
        .expect("beta hit")
        .source_id
        .clone();
    assert_eq!(alpha.as_deref(), Some("SRC-A"), "each hit reports its OWN provenance");
    assert_eq!(beta.as_deref(), Some("SRC-B"), "each hit reports its OWN provenance");

    opened.engine.erase_source("SRC-A").expect("erase alpha only");
    opened.engine.close().unwrap();

    assert_erased_raw(&path, "SRC-A", "alpha document");
    // …and the untouched source survives at rest.
    let conn = Connection::open(&path).expect("open raw");
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM canonical_nodes WHERE source_id = 'SRC-B'"),
        1,
        "erasing SRC-A must not touch SRC-B"
    );
}

// ---------------------------------------------------------------------------
// TC-31 — NULL provenance stays readable as None
// ---------------------------------------------------------------------------

/// `source_id` stays `Option<String>`. Legacy rows (pre-0.8.20) and — permanently,
/// by the TC-11 pin — governed rows spared by the step-21 backfill carry NULL at
/// rest. Such a hit must surface `source_id == None`, not panic and not be dropped.
#[test]
fn tc31_null_provenance_row_still_yields_a_hit_with_none() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc31_null_prov");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let receipt = opened
        .engine
        .write(&[node("tc31nullprov legacy shaped row", "SRC-NULL", None)])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    // Simulate a pre-0.8.20 / TC-11-spared row: NULL provenance at rest.
    {
        let conn = Connection::open(&path).expect("open raw");
        let updated = conn
            .execute("UPDATE canonical_nodes SET source_id = NULL", [])
            .expect("null the node provenance");
        assert!(updated >= 1, "the fixture must actually produce a NULL-provenance row");
    }

    let result = search_after_projection(&opened.engine, "tc31nullprov", receipt.cursor);
    let hit = result
        .results
        .iter()
        .find(|h| h.body.contains("tc31nullprov"))
        .expect("a NULL-provenance row must still be retrievable");
    assert_eq!(hit.source_id, None, "NULL at rest must read back as None, not a fabricated value");

    opened.engine.close().unwrap();
}
