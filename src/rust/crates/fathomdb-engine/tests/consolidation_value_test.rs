//! 0.8.12 Slice 20 (OPP-2, ADR-0.8.12 §2.2) — Consolidation value-test ($0).
//!
//! Pre-registration: `dev/design/0.8.12-coverage-probe-and-value-test.md` §B. This is the `$0`,
//! LLM-free, deterministic mechanism measurement that feeds the SHIP-ON / STAY-OFF decision recorded
//! in `dev/plans/runs/consolidation-value-test-results.md`.
//!
//! Independent variable: consolidation OFF (accumulate all facts) vs ON (apply the recency verdict via
//! the deterministic stub harness). Dependent variables, measured on edge-FTS retrieval:
//!   * PRECISION — for a query term that matches BOTH the stale and the updated fact of an axis, the
//!     fraction of returned edges that are the CURRENT (updated) fact. Stale contradictions returned =
//!     precision loss.
//!   * LOSSINESS — the false-supersede rate: still-valid (updated) facts wrongly hidden. Must be 0.
//!
//! FOOTPRINT / NO-EGRESS (R-CON-3): the harness is the local deterministic recency stub; no network,
//! no LLM, no randomness. The library query path stays CPU-only.

use fathomdb_engine::{ConsolidateAxis, Engine, PreparedWrite};
use rusqlite::Connection;
use tempfile::TempDir;

use fathomdb_schema::SQLITE_SUFFIX;

fn harness_cmd() -> Vec<String> {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/slice15_consolidate/stub_consolidate_harness.py");
    assert!(script.exists(), "consolidate stub harness must exist at {}", script.display());
    vec!["python3".to_string(), script.to_string_lossy().to_string()]
}

#[allow(clippy::too_many_arguments)]
fn fact_edge(from: &str, to: &str, logical_id: &str, body: &str, t_valid: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "works_for".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: Some(format!("doc-{logical_id}")),
        logical_id: Some(logical_id.to_string()),
        body: Some(body.to_string()),
        t_valid: Some(t_valid.to_string()),
        t_invalid: None,
        confidence: Some(0.9),
        extractor_model_id: Some("stub-extractor-v1".to_string()),
        temporal_fallback: None,
    }
}

/// N axes, each: a STALE fact (older t_valid) + an UPDATED fact (newer t_valid). Both bodies share the
/// query term "works" so an unconsolidated edge-FTS query returns BOTH (the stale contradiction leaks).
const SUBJECTS: &[(&str, &str, &str)] = &[
    // (subject, stale_org, updated_org)
    ("bob", "acme", "globex"),
    ("cara", "initech", "hooli"),
    ("dan", "umbrella", "wayne"),
    ("eve", "stark", "wonka"),
    ("finn", "cyberdyne", "tyrell"),
    ("gwen", "soylent", "aperture"),
];

fn seed(engine: &Engine) {
    let mut writes = Vec::new();
    for (subj, stale, updated) in SUBJECTS {
        writes.push(fact_edge(
            subj,
            stale,
            &format!("edge-{subj}-stale"),
            &format!("{subj} works for {stale}"),
            "2019-01-01T00:00:00Z",
        ));
        writes.push(fact_edge(
            subj,
            updated,
            &format!("edge-{subj}-updated"),
            &format!("{subj} works for {updated}"),
            "2022-01-01T00:00:00Z",
        ));
    }
    engine.write(&writes).expect("seed competing edges");
}

/// Count edge-FTS hits for a term, split into (updated_hits, stale_hits) by matching the logical_id
/// suffix via a join back to canonical_edges.
fn fts_hits(conn: &Connection, term: &str) -> (u64, u64) {
    // search_index_edges.write_cursor == canonical_edges.write_cursor. FTS5 MATCH requires the table
    // name (not an alias), so match in a subquery and join back by write_cursor.
    let count = |suffix: &str| -> u64 {
        conn.query_row(
            "SELECT COUNT(*) FROM canonical_edges e \
             WHERE e.logical_id LIKE ?2 \
               AND e.write_cursor IN \
                   (SELECT write_cursor FROM search_index_edges WHERE search_index_edges MATCH ?1)",
            rusqlite::params![term, suffix],
            |r| r.get(0),
        )
        .unwrap()
    };
    (count("%-updated"), count("%-stale"))
}

#[test]
fn consolidation_removes_stale_contradictions_without_lossiness() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("value{SQLITE_SUFFIX}"));
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed(&opened.engine);

    let conn = Connection::open(&path).unwrap();

    // --- Consolidation OFF (baseline): both stale and updated facts are retrievable. ---
    let (off_updated, off_stale) = fts_hits(&conn, "works");
    let n = SUBJECTS.len() as u64;
    assert_eq!(off_updated, n, "every updated fact retrievable OFF");
    assert_eq!(
        off_stale, n,
        "every stale contradiction ALSO retrievable OFF (the precision problem)"
    );
    let off_precision = off_updated as f64 / (off_updated + off_stale) as f64;

    // --- Consolidation ON: apply the recency verdict for every axis. ---
    let cmd = harness_cmd();
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let axes: Vec<ConsolidateAxis> = SUBJECTS
        .iter()
        .map(|(subj, _, _)| ConsolidateAxis {
            subject_logical_id: subj.to_string(),
            relation: "works_for".to_string(),
        })
        .collect();
    let receipt = opened.engine.consolidate_with_provider(&cmd_refs, &axes).expect("consolidate");
    assert_eq!(receipt.edges_invalidated, n, "one stale fact invalidated per axis");

    let (on_updated, on_stale) = fts_hits(&conn, "works");
    let on_precision = on_updated as f64 / (on_updated + on_stale).max(1) as f64;

    // Mechanism: stale contradictions are hidden from retrieval (precision → 1.0) ...
    assert_eq!(on_stale, 0, "ON: every stale contradiction is hidden from edge-FTS retrieval");
    // ... with ZERO lossiness: every still-valid updated fact is retained.
    assert_eq!(
        on_updated, n,
        "ON: no false-supersede — all updated facts retained (lossiness = 0)"
    );

    // Print the measured value for the results doc (visible with `cargo test -- --nocapture`).
    println!(
        "VALUE-TEST n_axes={n} precision_off={off_precision:.3} precision_on={on_precision:.3} \
         precision_lift={:.3} stale_off={off_stale} stale_on={on_stale} lossiness={} ",
        on_precision - off_precision,
        n - on_updated
    );
    assert!(on_precision > off_precision, "consolidation must lift retrieval precision");
}
