//! 0.8.6 Slice 5 — Provider-protocol typed-task seam (ADR-0.8.6, Option A).
//!
//! These tests pin the ADR-0.8.6 §3 back-compat + negotiation acceptance for the
//! generalized transport seam, WITHOUT touching the byte-identical extract
//! behavior pinned by `slice15_byo_llm_ingest.rs` and `elps_conformance_golden.rs`.
//!
//! INV-2:
//!  - an extract-only harness whose `ready` has NO `supported_tasks` field still
//!    works (default-to-requested-task back-compat) — GREEN before and after the
//!    refactor.
//!  - a `ready` WITH `supported_tasks` that OMITS "extract" is REJECTED — RED
//!    before the negotiation lands, GREEN after (so the negotiation actually bites).
//!  - a `ready` WITH `supported_tasks` that INCLUDES "extract" still works.

use fathomdb_engine::{Engine, EngineError, ExtractDocument};
use tempfile::TempDir;

use fathomdb_schema::SQLITE_SUFFIX;

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/slice15_byo_llm")
}

fn stub_harness_cmd() -> Vec<String> {
    let script = fixture_dir().join("stub_harness.py");
    assert!(script.exists(), "stub harness must exist at {}", script.display());
    vec!["python3".to_string(), script.to_string_lossy().to_string()]
}

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

// ---------------------------------------------------------------------------
// INV-2a — back-compat: a `ready` WITHOUT `supported_tasks` still works.
// ---------------------------------------------------------------------------

/// ADR-0.8.6 §3 back-compat: the shipped extract-only stub harness (its `ready`
/// carries NO `supported_tasks` field) must keep ingesting unchanged. The task
/// being requested (extract) is the default served task when the field is absent.
#[test]
fn back_compat_ready_without_supported_tasks_ingests() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "no_supported_tasks");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let cmd_strings = stub_harness_cmd();
    let cmd_refs: Vec<&str> = cmd_strings.iter().map(|s| s.as_str()).collect();
    let docs = vec![ExtractDocument {
        source_doc_id: "doc-simple".to_string(),
        body: "Alice owns the project".to_string(),
    }];

    let receipt = opened
        .engine
        .ingest_with_extractor(&cmd_refs, &docs)
        .expect("extract-only harness without supported_tasks must still ingest");
    assert!(receipt.edges_written > 0, "back-compat ingest must still write edges");
}

// ---------------------------------------------------------------------------
// INV-2b — negotiation bites: `supported_tasks` omitting "extract" is REJECTED.
// ---------------------------------------------------------------------------

/// ADR-0.8.6 §2.2: if a harness advertises `supported_tasks` in `ready`,
/// FathomDB refuses to dispatch a task the harness did not advertise. A harness
/// that advertises ["consolidate","summarize"] (NOT "extract") must be rejected
/// with `EngineError::Extractor` before any extract request is dispatched.
#[test]
fn ready_supported_tasks_omitting_extract_is_rejected() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "omits_extract");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    // Harness advertises supported_tasks WITHOUT "extract". It never needs to
    // answer an extract request because negotiation must reject at handshake.
    let harness = r#"
import json, sys
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("type") == "hello":
        print(json.dumps({"protocol": "fathomdb.extract.v1", "type": "ready",
                          "schema_version": 1, "model": "stub-v1",
                          "max_docs_per_request": 10,
                          "supported_tasks": ["consolidate", "summarize"]}),
              flush=True)
    elif msg.get("type") == "extract":
        # Should never be reached; if it is, return valid data so the test
        # fails LOUDLY (ingest would succeed) rather than for the wrong reason.
        print(json.dumps({"protocol": "fathomdb.extract.v1", "type": "result",
                          "request_id": msg.get("request_id"),
                          "entities": [{"name": "A", "type": "person", "aliases": []},
                                       {"name": "B", "type": "project", "aliases": []}],
                          "edges": [{"from_entity": "A", "to_entity": "B",
                                     "relation": "owns", "body": "A owns B",
                                     "confidence": 0.9}]}), flush=True)
"#;
    let cmd = ["python3".to_string(), "-c".to_string(), harness.to_string()];
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let docs =
        vec![ExtractDocument { source_doc_id: "d1".to_string(), body: "A owns B".to_string() }];

    let result = opened.engine.ingest_with_extractor(&cmd_refs, &docs);
    assert!(
        matches!(result, Err(EngineError::Extractor)),
        "a ready advertising supported_tasks without 'extract' must return \
         Err(EngineError::Extractor), got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// INV-2c — negotiation allows: `supported_tasks` INCLUDING "extract" works.
// ---------------------------------------------------------------------------

/// ADR-0.8.6 §2.2: a harness that DOES advertise "extract" in `supported_tasks`
/// dispatches normally and writes output (the negotiation must not over-reject).
#[test]
fn ready_supported_tasks_including_extract_works() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "includes_extract");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let harness = r#"
import json, sys
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("type") == "hello":
        print(json.dumps({"protocol": "fathomdb.extract.v1", "type": "ready",
                          "schema_version": 1, "model": "stub-v1",
                          "max_docs_per_request": 10,
                          "supported_tasks": ["extract", "consolidate"]}),
              flush=True)
    elif msg.get("type") == "extract":
        print(json.dumps({"protocol": "fathomdb.extract.v1", "type": "result",
                          "request_id": msg.get("request_id"),
                          "entities": [{"name": "A", "type": "person", "aliases": []},
                                       {"name": "B", "type": "project", "aliases": []}],
                          "edges": [{"from_entity": "A", "to_entity": "B",
                                     "relation": "owns", "body": "A owns B",
                                     "confidence": 0.9}]}), flush=True)
"#;
    let cmd = ["python3".to_string(), "-c".to_string(), harness.to_string()];
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let docs =
        vec![ExtractDocument { source_doc_id: "d1".to_string(), body: "A owns B".to_string() }];

    let receipt = opened
        .engine
        .ingest_with_extractor(&cmd_refs, &docs)
        .expect("harness advertising 'extract' must ingest normally");
    assert!(receipt.edges_written > 0, "must write the owns edge");
}
