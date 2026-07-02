//! 0.8.12 Slice 15 (OPP-2, ADR-0.8.12) — Consolidation / Recency provider tests.
//!
//! The consolidation provider is the SECOND consumer of the one OPP-8 provider
//! transport (ADR-0.8.6): it reuses `provider_session` verbatim and adds only a
//! task variant (`fathomdb.consolidate.v1`), a payload, and the
//! `EngineError::Consolidator` leaf.
//!
//! FOOTPRINT / NO-EGRESS (R-CON-3): every harness here is a LOCAL, DETERMINISTIC
//! Python script — CALLER-SIDE BYO-LLM / OFFLINE-BUILD. No network, no LLM, no
//! randomness. The library write/index/query path stays CPU-only/deterministic;
//! consolidation records supersession/recency METADATA only and never rewrites a
//! body or deletes a row (ADR-0.8.12 §2.1).

use fathomdb_engine::{ConsolidateAxis, Engine, EngineError, PreparedWrite};
use rusqlite::Connection;
use tempfile::TempDir;

use fathomdb_schema::SQLITE_SUFFIX;

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/slice15_consolidate")
}

/// The DETERMINISTIC, LOCAL consolidate stub harness (recency rule).
/// CALLER-SIDE BYO-LLM / OFFLINE-BUILD: a local file run by python3 from PATH.
fn consolidate_harness_cmd() -> Vec<String> {
    let script = fixture_dir().join("stub_consolidate_harness.py");
    assert!(script.exists(), "consolidate stub harness must exist at {}", script.display());
    vec!["python3".to_string(), script.to_string_lossy().to_string()]
}

/// The EXISTING extract-only stub harness (speaks only `fathomdb.extract.v1`,
/// does NOT advertise `consolidate`). Used for the negotiation-refusal test.
fn extract_only_harness_cmd() -> Vec<String> {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/slice15_byo_llm/stub_harness.py");
    assert!(script.exists(), "extract stub harness must exist at {}", script.display());
    vec!["python3".to_string(), script.to_string_lossy().to_string()]
}

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

/// Write one active fact-edge directly (bypassing the extract harness) so the
/// setup is deterministic and focused on consolidation.
#[allow(clippy::too_many_arguments)]
fn fact_edge(
    kind: &str,
    from: &str,
    to: &str,
    logical_id: &str,
    body: &str,
    t_valid: &str,
    confidence: f64,
) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: kind.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: Some(format!("doc-{to}")),
        logical_id: Some(logical_id.to_string()),
        body: Some(body.to_string()),
        t_valid: Some(t_valid.to_string()),
        t_invalid: None,
        confidence: Some(confidence),
        extractor_model_id: Some("stub-extractor-v1".to_string()),
        temporal_fallback: None,
    }
}

/// Seed two COMPETING active fact-edges on one (subject=`bob`, relation=
/// `works_for`) axis: an older `bob→acme` (t_valid 2019) and a newer
/// `bob→globex` (t_valid 2022). Different `to_id` ⇒ both stay active (the G11
/// invalidate-not-accumulate triple key does not collapse them).
fn seed_competing_edges(engine: &Engine) {
    let older = fact_edge(
        "works_for",
        "bob",
        "acme",
        "edge-acme",
        "Bob works for Acme",
        "2019-01-01T00:00:00Z",
        0.90,
    );
    let newer = fact_edge(
        "works_for",
        "bob",
        "globex",
        "edge-globex",
        "Bob works for Globex",
        "2022-01-01T00:00:00Z",
        0.80,
    );
    engine.write(&[older, newer]).expect("seed two competing edges");
}

/// fix-1 [P1]: count edge-FTS shadow rows matching `term` in `search_index_edges`
/// (the same query pattern used by `slice15_byo_llm_ingest::edge_fts_searchable`).
fn edge_fts_count(conn: &Connection, term: &str) -> u64 {
    conn.query_row(
        "SELECT COUNT(*) FROM search_index_edges WHERE search_index_edges MATCH ?1",
        [term],
        |r| r.get(0),
    )
    .expect("search_index_edges must exist and be queryable")
}

fn edge_row(conn: &Connection, logical_id: &str) -> (Option<String>, Option<String>, Option<i64>) {
    // (body, t_invalid, superseded_at) for the row with this logical_id.
    conn.query_row(
        "SELECT body, t_invalid, superseded_at FROM canonical_edges WHERE logical_id = ?1",
        [logical_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .expect("edge row must exist")
}

// ---------------------------------------------------------------------------
// R-CON-1 (primary witness) — recency consolidation invalidates the older edge
// ---------------------------------------------------------------------------

/// R-CON-1: ingest two conflicting/updated facts on one (subject, relation)
/// axis, run `consolidate_with_provider` against the deterministic stub, and
/// assert the OLDER fact gets `t_invalid` set (with correct temporal bounds) and
/// the NEWER fact stays live. Bodies + rows survive (metadata-only, §2.1).
#[test]
fn recency_consolidation_invalidates_older_edge() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "recency");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    let cmd_strings = consolidate_harness_cmd();
    let cmd_refs: Vec<&str> = cmd_strings.iter().map(|s| s.as_str()).collect();

    let axes = vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }];
    let receipt = opened
        .engine
        .consolidate_with_provider(&cmd_refs, &axes)
        .expect("consolidate_with_provider must succeed with stub harness");

    // Receipt: one cluster, two edges examined, one kept + one invalidated.
    assert_eq!(receipt.clusters_processed, 1, "one (subject, relation) cluster");
    assert_eq!(receipt.edges_examined, 2, "two competing edges examined");
    assert_eq!(receipt.edges_kept, 1, "newer edge kept");
    assert_eq!(receipt.edges_invalidated, 1, "older edge invalidated");
    assert_eq!(receipt.edges_superseded, 0, "no supersede verdicts in recency path");

    let conn = Connection::open(&path).unwrap();

    // Older edge (bob→acme): t_invalid set to the winner's t_valid; body + row
    // preserved (NOT a content rewrite; NOT deleted; NOT tombstoned).
    let (acme_body, acme_t_invalid, acme_superseded) = edge_row(&conn, "edge-acme");
    assert_eq!(
        acme_t_invalid.as_deref(),
        Some("2022-01-01T00:00:00Z"),
        "older edge must be invalidated at the newer edge's t_valid (correct temporal bound)"
    );
    assert_eq!(
        acme_body.as_deref(),
        Some("Bob works for Acme"),
        "older edge body must be preserved verbatim (no content rewrite/merge)"
    );
    assert!(acme_superseded.is_none(), "invalidate is metadata-only: row is NOT tombstoned");

    // Newer edge (bob→globex): stays LIVE (t_invalid NULL, superseded_at NULL).
    let (globex_body, globex_t_invalid, globex_superseded) = edge_row(&conn, "edge-globex");
    assert!(globex_t_invalid.is_none(), "newer edge must stay live (t_invalid NULL)");
    assert!(globex_superseded.is_none(), "newer edge must stay live (not superseded)");
    assert_eq!(globex_body.as_deref(), Some("Bob works for Globex"), "newer edge body preserved");

    // Both rows survive (invalidate-not-delete).
    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM canonical_edges WHERE kind = 'works_for'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(total, 2, "both edge rows survive consolidation");

    // Temporal liveness: exactly the newer edge is temporally live now.
    let live_now: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_edges \
             WHERE kind = 'works_for' AND superseded_at IS NULL \
               AND (t_invalid IS NULL OR datetime(t_invalid) > datetime('now'))",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(live_now, 1, "exactly the newer edge is temporally live after consolidation");
}

// ---------------------------------------------------------------------------
// supersede verdict path (marks-superseded via the G0 tombstone; row survives)
// ---------------------------------------------------------------------------

/// A harness that rules `supersede` marks the loser superseded via the existing
/// G0 `superseded_at` column; the row + body survive (invalidate-not-delete).
#[test]
fn supersede_verdict_marks_superseded_row_survives() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "supersede");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    // Inline harness: supersede the acme edge (by globex), keep globex.
    // CALLER-SIDE BYO-LLM / OFFLINE-BUILD — local python, no network.
    let harness = r#"
import json, sys
P = "fathomdb.consolidate.v1"
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("type") == "hello":
        print(json.dumps({"protocol": P, "type": "ready", "schema_version": 1,
                          "model": "stub-consolidate-v1", "supported_tasks": ["consolidate"],
                          "max_docs_per_request": 8}), flush=True)
    elif msg.get("type") == "consolidate":
        edges = msg.get("cluster", {}).get("edges", [])
        verdicts = []
        for e in edges:
            ref = e.get("edge_ref")
            if ref == "edge-acme":
                verdicts.append({"edge_ref": ref, "verdict": "supersede", "by": "edge-globex"})
            else:
                verdicts.append({"edge_ref": ref, "verdict": "keep"})
        print(json.dumps({"protocol": P, "type": "result",
                          "request_id": msg.get("request_id"), "verdicts": verdicts}), flush=True)
"#;
    let cmd = ["python3".to_string(), "-c".to_string(), harness.to_string()];
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let axes = vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }];

    let receipt = opened
        .engine
        .consolidate_with_provider(&cmd_refs, &axes)
        .expect("consolidate must succeed");
    assert_eq!(receipt.edges_superseded, 1, "one supersede verdict applied");
    assert_eq!(receipt.edges_kept, 1, "one keep verdict");

    let conn = Connection::open(&path).unwrap();
    let (acme_body, _acme_t_invalid, acme_superseded) = edge_row(&conn, "edge-acme");
    assert!(acme_superseded.is_some(), "superseded edge must have a non-null superseded_at");
    assert_eq!(
        acme_body.as_deref(),
        Some("Bob works for Acme"),
        "superseded edge body must be preserved (invalidate-not-delete, no rewrite)"
    );

    let (_g_body, _g_ti, globex_superseded) = edge_row(&conn, "edge-globex");
    assert!(globex_superseded.is_none(), "kept edge must remain active");

    // Both rows survive.
    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM canonical_edges WHERE kind = 'works_for'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(total, 2, "both rows survive supersession (no delete)");
}

// ---------------------------------------------------------------------------
// negotiation — dispatching `consolidate` to an extract-only harness is refused
// ---------------------------------------------------------------------------

/// The SAME `supported_tasks` negotiation `provider_session` runs for extract:
/// the extract-only stub speaks `fathomdb.extract.v1` and never advertises
/// `consolidate`, so opening a consolidate session against it fails the
/// handshake → `Err(EngineError::Consolidator)`.
#[test]
fn consolidate_refused_by_extract_only_harness() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "refused");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    let cmd_strings = extract_only_harness_cmd();
    let cmd_refs: Vec<&str> = cmd_strings.iter().map(|s| s.as_str()).collect();
    let axes = vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }];

    let result = opened.engine.consolidate_with_provider(&cmd_refs, &axes);
    assert!(
        matches!(result, Err(EngineError::Consolidator)),
        "an extract-only harness must refuse the consolidate task, got {result:?}"
    );
}

/// A harness that ADVERTISES only `extract` in `supported_tasks` (but over the
/// consolidate protocol) is likewise refused by the negotiation.
#[test]
fn consolidate_refused_when_task_not_advertised() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "not_advertised");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    let harness = r#"
import json, sys
P = "fathomdb.consolidate.v1"
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("type") == "hello":
        print(json.dumps({"protocol": P, "type": "ready", "schema_version": 1,
                          "model": "bad", "supported_tasks": ["extract"],
                          "max_docs_per_request": 8}), flush=True)
"#;
    let cmd = ["python3".to_string(), "-c".to_string(), harness.to_string()];
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let axes = vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }];

    let result = opened.engine.consolidate_with_provider(&cmd_refs, &axes);
    assert!(
        matches!(result, Err(EngineError::Consolidator)),
        "a harness that does not advertise 'consolidate' must be refused, got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// protocol fault — a verdict for an edge NOT in the presented cluster is rejected
// ---------------------------------------------------------------------------

/// The harness may only rule on edges FathomDB presented. An out-of-cluster
/// `edge_ref` is a protocol fault → `Err(EngineError::Consolidator)`.
#[test]
fn consolidate_rejects_out_of_cluster_verdict() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "out_of_cluster");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    let harness = r#"
import json, sys
P = "fathomdb.consolidate.v1"
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("type") == "hello":
        print(json.dumps({"protocol": P, "type": "ready", "schema_version": 1,
                          "model": "stub", "supported_tasks": ["consolidate"],
                          "max_docs_per_request": 8}), flush=True)
    elif msg.get("type") == "consolidate":
        print(json.dumps({"protocol": P, "type": "result",
                          "request_id": msg.get("request_id"),
                          "verdicts": [{"edge_ref": "edge-not-in-cluster", "verdict": "keep"}]}),
              flush=True)
"#;
    let cmd = ["python3".to_string(), "-c".to_string(), harness.to_string()];
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let axes = vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }];

    let result = opened.engine.consolidate_with_provider(&cmd_refs, &axes);
    assert!(
        matches!(result, Err(EngineError::Consolidator)),
        "an out-of-cluster verdict must return Err(Consolidator), got {result:?}"
    );

    // Fault is caught BEFORE any metadata write: both edges stay live.
    let conn = Connection::open(&path).unwrap();
    let (_b, acme_ti, acme_sup) = edge_row(&conn, "edge-acme");
    assert!(
        acme_ti.is_none() && acme_sup.is_none(),
        "no metadata change on a rejected verdict batch"
    );
}

// ---------------------------------------------------------------------------
// R-CON-3 — footprint / no network egress
// ---------------------------------------------------------------------------

/// R-CON-3: the consolidate harness is a LOCAL script (absolute path, exists on
/// disk) — CALLER-SIDE BYO-LLM / OFFLINE-BUILD. The engine spawns a local
/// subprocess and opens no TCP socket of its own; no LLM/network symbol is
/// reachable from the library consolidate path.
#[test]
fn footprint_no_network_egress() {
    let cmd = consolidate_harness_cmd();
    assert!(cmd.len() >= 2, "command must have at least 2 parts");
    let script_path = std::path::Path::new(&cmd[1]);
    assert!(script_path.is_absolute(), "consolidate harness script must be an absolute path");
    assert!(script_path.exists(), "consolidate harness script must exist locally");

    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "no_network");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let axes = vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }];
    let receipt =
        opened.engine.consolidate_with_provider(&cmd_refs, &axes).expect("consolidate w/o network");
    assert_eq!(receipt.clusters_processed, 1);
}

// ---------------------------------------------------------------------------
// empty axis — no competing edges → no-op cluster (deterministic)
// ---------------------------------------------------------------------------

/// An axis with no matching active edges yields an empty cluster: it is skipped
/// (not dispatched), and the receipt records zero processed clusters.
#[test]
fn empty_cluster_is_skipped() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "empty");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    let cmd_strings = consolidate_harness_cmd();
    let cmd_refs: Vec<&str> = cmd_strings.iter().map(|s| s.as_str()).collect();
    // Unknown subject → empty cluster.
    let axes = vec![ConsolidateAxis {
        subject_logical_id: "nobody".to_string(),
        relation: "works_for".to_string(),
    }];
    let receipt = opened
        .engine
        .consolidate_with_provider(&cmd_refs, &axes)
        .expect("consolidate must succeed");
    assert_eq!(receipt.clusters_processed, 0, "empty cluster is skipped");
    assert_eq!(receipt.edges_examined, 0);
}

// ---------------------------------------------------------------------------
// fix-1 [P1] — a consolidated-away edge is hidden from active FTS retrieval
// ---------------------------------------------------------------------------

/// After `invalidate` (ended as of now), the stale edge's FTS shadow row is
/// pruned so `search_index_edges` no longer surfaces it — while the winning
/// edge stays searchable. The canonical row + body survive (§2.1).
#[test]
fn invalidate_hides_stale_edge_from_edge_fts() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "invalidate_fts");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    // Before: both edge bodies are indexed in search_index_edges.
    {
        let conn = Connection::open(&path).unwrap();
        assert!(edge_fts_count(&conn, "acme") > 0, "acme edge must be FTS-indexed pre-consolidate");
        assert!(edge_fts_count(&conn, "globex") > 0, "globex edge must be FTS-indexed pre");
    }

    let cmd_strings = consolidate_harness_cmd();
    let cmd_refs: Vec<&str> = cmd_strings.iter().map(|s| s.as_str()).collect();
    let axes = vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }];
    let receipt = opened
        .engine
        .consolidate_with_provider(&cmd_refs, &axes)
        .expect("consolidate must succeed");
    assert_eq!(receipt.edges_invalidated, 1, "older edge invalidated");

    let conn = Connection::open(&path).unwrap();
    // The invalidated (older) edge is gone from edge FTS; the winner remains.
    assert_eq!(
        edge_fts_count(&conn, "acme"),
        0,
        "invalidated edge must NO LONGER appear in edge FTS (shadow pruned)"
    );
    assert!(edge_fts_count(&conn, "globex") > 0, "winning edge must still be FTS-searchable");

    // NON-DESTRUCTIVE: the canonical_edges row + body survive.
    let (acme_body, acme_ti, acme_sup) = edge_row(&conn, "edge-acme");
    assert_eq!(acme_body.as_deref(), Some("Bob works for Acme"), "body preserved (no rewrite)");
    assert!(acme_ti.is_some(), "t_invalid recorded");
    assert!(acme_sup.is_none(), "invalidate is not a tombstone");
}

/// A `supersede` verdict unconditionally prunes the loser's FTS shadow (it is
/// out of the active set); the winner remains searchable, and the loser row +
/// body survive.
#[test]
fn supersede_hides_loser_from_edge_fts() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "supersede_fts");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    // Inline harness: supersede acme (by globex), keep globex.
    let harness = r#"
import json, sys
P = "fathomdb.consolidate.v1"
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("type") == "hello":
        print(json.dumps({"protocol": P, "type": "ready", "schema_version": 1,
                          "model": "stub-consolidate-v1", "supported_tasks": ["consolidate"],
                          "max_docs_per_request": 8}), flush=True)
    elif msg.get("type") == "consolidate":
        edges = msg.get("cluster", {}).get("edges", [])
        verdicts = []
        for e in edges:
            ref = e.get("edge_ref")
            if ref == "edge-acme":
                verdicts.append({"edge_ref": ref, "verdict": "supersede", "by": "edge-globex"})
            else:
                verdicts.append({"edge_ref": ref, "verdict": "keep"})
        print(json.dumps({"protocol": P, "type": "result",
                          "request_id": msg.get("request_id"), "verdicts": verdicts}), flush=True)
"#;
    let cmd = ["python3".to_string(), "-c".to_string(), harness.to_string()];
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let axes = vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }];
    let receipt = opened
        .engine
        .consolidate_with_provider(&cmd_refs, &axes)
        .expect("consolidate must succeed");
    assert_eq!(receipt.edges_superseded, 1, "one supersede verdict applied");

    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        edge_fts_count(&conn, "acme"),
        0,
        "superseded edge must NO LONGER appear in edge FTS (shadow pruned)"
    );
    assert!(edge_fts_count(&conn, "globex") > 0, "kept edge must still be FTS-searchable");

    // Loser row + body survive (invalidate-not-delete).
    let (acme_body, _ti, acme_sup) = edge_row(&conn, "edge-acme");
    assert_eq!(acme_body.as_deref(), Some("Bob works for Acme"), "loser body preserved");
    assert!(acme_sup.is_some(), "loser is superseded");
}

// ---------------------------------------------------------------------------
// fix-1 [P2] — the verdict set must be a bijection with the presented cluster
// ---------------------------------------------------------------------------

/// A cluster edge left WITHOUT a verdict is a protocol fault (incomplete
/// coverage) → `Err(EngineError::Consolidator)`, with no metadata written.
#[test]
fn consolidate_rejects_missing_verdict() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "missing_verdict");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    // Harness rules only on edge-acme, omitting edge-globex.
    let harness = r#"
import json, sys
P = "fathomdb.consolidate.v1"
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("type") == "hello":
        print(json.dumps({"protocol": P, "type": "ready", "schema_version": 1,
                          "model": "stub", "supported_tasks": ["consolidate"],
                          "max_docs_per_request": 8}), flush=True)
    elif msg.get("type") == "consolidate":
        print(json.dumps({"protocol": P, "type": "result",
                          "request_id": msg.get("request_id"),
                          "verdicts": [{"edge_ref": "edge-acme", "verdict": "keep"}]}),
              flush=True)
"#;
    let cmd = ["python3".to_string(), "-c".to_string(), harness.to_string()];
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let axes = vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }];

    let result = opened.engine.consolidate_with_provider(&cmd_refs, &axes);
    assert!(
        matches!(result, Err(EngineError::Consolidator)),
        "an incomplete verdict set must return Err(Consolidator), got {result:?}"
    );

    // Fault caught before commit: both edges stay live.
    let conn = Connection::open(&path).unwrap();
    let (_b, acme_ti, acme_sup) = edge_row(&conn, "edge-acme");
    assert!(acme_ti.is_none() && acme_sup.is_none(), "no metadata change on a rejected batch");
    let (_b2, gx_ti, gx_sup) = edge_row(&conn, "edge-globex");
    assert!(gx_ti.is_none() && gx_sup.is_none(), "no metadata change on a rejected batch");
}

/// A REPEATED `edge_ref` is a protocol fault (not a bijection) →
/// `Err(EngineError::Consolidator)`, with no metadata written.
#[test]
fn consolidate_rejects_duplicate_verdict() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "duplicate_verdict");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    // Harness rules on edge-acme TWICE (and never on edge-globex).
    let harness = r#"
import json, sys
P = "fathomdb.consolidate.v1"
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("type") == "hello":
        print(json.dumps({"protocol": P, "type": "ready", "schema_version": 1,
                          "model": "stub", "supported_tasks": ["consolidate"],
                          "max_docs_per_request": 8}), flush=True)
    elif msg.get("type") == "consolidate":
        print(json.dumps({"protocol": P, "type": "result",
                          "request_id": msg.get("request_id"),
                          "verdicts": [{"edge_ref": "edge-acme", "verdict": "keep"},
                                       {"edge_ref": "edge-acme", "verdict": "keep"}]}),
              flush=True)
"#;
    let cmd = ["python3".to_string(), "-c".to_string(), harness.to_string()];
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let axes = vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }];

    let result = opened.engine.consolidate_with_provider(&cmd_refs, &axes);
    assert!(
        matches!(result, Err(EngineError::Consolidator)),
        "a duplicate edge_ref must return Err(Consolidator), got {result:?}"
    );

    // Fault caught before commit: both edges stay live.
    let conn = Connection::open(&path).unwrap();
    let (_b, acme_ti, acme_sup) = edge_row(&conn, "edge-acme");
    assert!(acme_ti.is_none() && acme_sup.is_none(), "no metadata change on a rejected batch");
}
