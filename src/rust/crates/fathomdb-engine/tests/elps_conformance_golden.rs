//! Slice 30 (conformance gate, reserved-gap 16-19) — ELPS golden fixture ingestion.
//!
//! Vendors the real cross-repo ELPS golden bytes from
//! `~/projects/memex/src/memex/elps/fixtures/golden.jsonl` into
//! `tests/fixtures/elps_conformance/` and drives them through
//! `engine.ingest_with_extractor` via a stub harness.
//!
//! For each of the 8 golden cases, asserts:
//! 1. `orphaned == 0`: every active edge endpoint resolves to an active node.
//! 2. Round-trip fidelity: `receipt.nodes_written > 0 || receipt.edges_written > 0`
//!    for every non-`no_facts` case (the `no_facts` case d4 yields 0 for both,
//!    which is the correct and expected outcome).
//!
//! This test FAILS before the fixture files are vendored (the fixture directory
//! does not exist until §3.2.1 copies the files). That is the intended RED-1
//! state.
//!
//! The stub harness at `fixtures/elps_conformance/stub_harness.py` reads
//! `golden.jsonl`, parses each `{request, expected}` pair, and returns the
//! `expected` JSON for the matching `source_doc_id`. Pattern follows
//! `qd_envelope_deserialize.rs` write_stub() and `slice15_byo_llm/stub_harness.py`.

use fathomdb_engine::{Engine, ExtractDocument};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/elps_conformance")
}

fn stub_harness_cmd() -> Vec<String> {
    let script = fixture_dir().join("stub_harness.py");
    assert!(
        script.exists(),
        "ELPS conformance stub harness must exist at {}; \
         run §3.2.1 to vendor the fixture files",
        script.display()
    );
    vec!["python3".to_string(), script.to_string_lossy().to_string()]
}

/// Helper: open an engine, ingest via the golden stub, assert round-trip and no orphans.
fn run_case(dir: &TempDir, doc_id: &str, body: &str, expect_facts: bool) {
    let db = dir.path().join(format!("golden_{doc_id}{SQLITE_SUFFIX}"));
    let opened = Engine::open_without_embedder_for_test(&db)
        .unwrap_or_else(|e| panic!("golden case {doc_id}: open failed: {e:?}"));

    let cmd = stub_harness_cmd();
    let cmd_refs: Vec<&str> = cmd.iter().map(String::as_str).collect();
    let docs = vec![ExtractDocument { source_doc_id: doc_id.to_string(), body: body.to_string() }];

    let receipt = opened
        .engine
        .ingest_with_extractor(&cmd_refs, &docs)
        .unwrap_or_else(|e| panic!("golden case {doc_id}: ingest failed: {e:?}"));

    assert_eq!(receipt.docs_processed, 1, "case {doc_id}: docs_processed");

    // Round-trip fidelity: non-no_facts cases must write at least one row.
    if expect_facts {
        assert!(
            receipt.nodes_written > 0 || receipt.edges_written > 0,
            "case {doc_id}: expected facts (nodes_written={} edges_written={})",
            receipt.nodes_written,
            receipt.edges_written
        );
    }

    // Orphaned-edge check (identical to qd_envelope_deserialize.rs:274-286).
    let conn = Connection::open(&db)
        .unwrap_or_else(|e| panic!("case {doc_id}: open DB for orphan check: {e}"));
    let orphaned: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_edges e
             WHERE e.superseded_at IS NULL
               AND ( NOT EXISTS (SELECT 1 FROM canonical_nodes n
                                 WHERE n.logical_id = e.from_id AND n.superseded_at IS NULL)
                  OR NOT EXISTS (SELECT 1 FROM canonical_nodes n
                                 WHERE n.logical_id = e.to_id AND n.superseded_at IS NULL) )",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|e| panic!("case {doc_id}: orphan query: {e}"));

    assert_eq!(
        orphaned, 0,
        "case {doc_id}: orphaned == 0 (every edge endpoint must link to an active node)"
    );

    println!(
        "golden case {doc_id}: nodes={} edges={} orphaned=0 OK",
        receipt.nodes_written, receipt.edges_written
    );
}

/// RED-1: All 8 golden cases ingest with zero orphaned edges.
///
/// Cases match the 8 QD ELPS golden inputs from
/// `~/projects/memex/src/memex/elps/fixtures/golden.jsonl`.
/// The stub harness returns the `expected` field for each case.
///
/// Fails at the RED phase because `tests/fixtures/elps_conformance/stub_harness.py`
/// does not exist yet (§3.2.1 vendors the files).
#[test]
fn elps_golden_all_8_cases_ingest_no_orphaned_edges() {
    // Verify the fixture directory and stub harness exist (RED fail point).
    let stub = fixture_dir().join("stub_harness.py");
    assert!(
        stub.exists(),
        "ELPS conformance fixtures not yet vendored; run §3.2.1 to copy from Memex. \
         Expected: {}",
        stub.display()
    );

    let dir = TempDir::new().unwrap();

    // Case d1: Alice joined Acme Corp — has facts.
    run_case(&dir, "d1", "Alice joined Acme Corp in 2021.", true);

    // Case d2: Bob leads Platform team — has facts.
    run_case(&dir, "d2", "Bob now leads the Platform team.", true);

    // Case d3: Carol introduced Dave to Eve — has facts (2 edges).
    run_case(&dir, "d3", "Carol introduced Dave to Eve at the Berlin summit.", true);

    // Case d4: Weather / coffee — no facts (no_facts warning; 0 nodes/edges is correct).
    run_case(&dir, "d4", "The weather was pleasant and the coffee was warm.", false);

    // Case d5: Café UTF-8 — has facts.
    run_case(&dir, "d5", "Café 🚀 launch: Renée shipped Zürich pilot.", true);

    // Case d6: Frank reports to Grace (synthesized=true) — has facts.
    run_case(&dir, "d6", "Frank reports to Grace.", true);

    // Case d7: Heidi / temporal_fallback warning — has facts.
    run_case(&dir, "d7", "Heidi acquired the Northwind contract.", true);

    // Case d8: Ivan / capped warning (4 deals, 2 edges kept) — has facts.
    run_case(&dir, "d8", "Ivan signed four deals across the quarter.", true);

    println!("elps_golden_all_8_cases_ingest_no_orphaned_edges: ALL 8 PASS");
}
