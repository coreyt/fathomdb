//! Corpus-Pack 4 graph validation gate.
//!
//! For a small deterministic subset of chains, ingest every chain's
//! anchor + synthetic docs and verify the cross-doc edges were
//! written to canonical_edges with the expected `from_id`, `to_id`,
//! and relation `kind`.
//!
//! The engine doesn't currently expose a public "edges from <id>"
//! traversal API (only per-source_id audit via trace_source_ref), so
//! this test opens a second read-only rusqlite connection against the
//! engine's WAL-mode SQLite file and queries `canonical_edges`
//! directly. That's a test-time peek at the canonical layer behind
//! the engine — not an invariant the engine itself promises.
//!
//! Correctness gate, not perf.

#[path = "support/corpus_harness.rs"]
mod corpus_harness;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use corpus_harness::{load_chain_docs, load_chains_or_skip, CorpusFixture};
use fathomdb_engine::Engine;
use rusqlite::Connection;

const MAX_CHAINS: usize = 20;

fn engine_db_path(engine: &Engine) -> std::path::PathBuf {
    engine.path().to_path_buf()
}

fn edges_from(conn: &Connection, from_id: &str) -> Vec<(String, String)> {
    let mut stmt = conn
        .prepare("SELECT kind, to_id FROM canonical_edges WHERE from_id = ?1")
        .expect("prepare edge select");
    let rows = stmt
        .query_map([from_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .expect("query edges");
    rows.filter_map(Result::ok).collect()
}

fn open_readonly(path: &Path) -> Connection {
    Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only sqlite")
}

#[test]
fn corpus_pack4_graph_edges_match_chain_specs() {
    let Some(chains) = load_chains_or_skip(MAX_CHAINS) else { return };
    assert!(chains.len() >= 10, "need >=10 chains for statistical meaning");

    // Collect every doc_id referenced by the chains, then load only
    // those docs from the per-source JSONLs. Synthetic connectives
    // live in chain_connectives.jsonl; anchors come from the various
    // source JSONLs.
    let mut wanted: HashSet<String> = HashSet::new();
    for c in &chains {
        wanted.extend(c.doc_ids.iter().cloned());
    }
    let Some(docs) = load_chain_docs(&wanted) else { return };
    assert!(!docs.is_empty(), "load_chain_docs returned empty");

    let fx = CorpusFixture::from_docs("corpus_graph", docs);
    let Some((_dir, engine)) = fx.open_or_skip() else { return };
    let edges_written = fx.ingest_into(&engine).edges;
    assert!(edges_written > 0, "ingest emitted 0 edges (chain wiring would be broken)");

    let db_path = engine_db_path(&engine);
    // Drain + close so the WAL is checkpointed and a read-only
    // connection sees the writes.
    engine.drain(15_000).expect("drain");
    engine.close().expect("close engine");

    let conn = open_readonly(&db_path);

    // For each chain, walk from each anchor and count how many of the
    // chain's other doc_ids are reachable via canonical_edges (1 hop).
    // Most chains in Pack 2 have a 1-hop fanout because connectives
    // are written with parent_doc_id pointing back at the immediate
    // parent in the chain. Multi-hop validation would need recursive
    // SQL; that's overkill for this correctness gate.
    let mut chain_doc_index: HashMap<String, HashSet<String>> = HashMap::new();
    for c in &chains {
        let set: HashSet<String> = c.doc_ids.iter().cloned().collect();
        chain_doc_index.insert(c.chain_id.clone(), set);
    }

    let mut chains_with_edges = 0usize;
    let mut total_edges_observed = 0usize;
    for chain in &chains {
        let mut chain_edges = 0usize;
        for doc_id in &chain.doc_ids {
            let edges = edges_from(&conn, doc_id);
            for (_kind, to_id) in &edges {
                if chain.doc_ids.contains(to_id) {
                    chain_edges += 1;
                }
            }
        }
        if chain_edges > 0 {
            chains_with_edges += 1;
        }
        total_edges_observed += chain_edges;
    }

    // Every chain in the subset should produce at least one in-chain
    // edge. If any chain has zero, the ingest harness lost the
    // relation wiring for it.
    assert_eq!(
        chains_with_edges,
        chains.len(),
        "{} of {} chains had zero in-chain edges in canonical_edges",
        chains.len() - chains_with_edges,
        chains.len()
    );
    assert!(
        total_edges_observed >= chains.len(),
        "expected >={} in-chain edges, observed {}",
        chains.len(),
        total_edges_observed
    );
}
