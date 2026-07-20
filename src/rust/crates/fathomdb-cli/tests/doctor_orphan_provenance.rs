//! 0.8.20 Slice 5d (R-20-E8, design §4 item 11) — `fathomdb doctor
//! orphan-provenance`: a READ-ONLY diagnostic listing per-`source_id` row
//! counts, and flagging rows that no erasure verb can reach.
//!
//! **What "orphan provenance" means here.** After Slice 5c every canonical row
//! is addressable by exactly one of two handles:
//!
//! * `logical_id` — governed **nodes**, reachable by `purge`;
//! * `source_id` — anonymous rows, reachable by `erase_source`.
//!
//! The node qualifier is load-bearing: `purge` resolves its target only through
//! `canonical_nodes` and erases edges by ENDPOINT, so an EDGE's `logical_id`
//! reaches no erasure verb (`null_source_governed_edge_counts_as_unerasable`).
//!
//! A row with NEITHER is reachable by no erasure verb at all: permanently
//! un-erasable. That is precisely the defect R-20-E3/E8 closed (5c made
//! provenance mandatory going forward; migration step 21 back-filled
//! `_legacy:pre-0.8.20` onto the pre-existing anonymous rows). This verb is the
//! operator-facing proof that the invariant actually HOLDS on a real database,
//! rather than only on a freshly-created one.
//!
//! So the exit-code contract follows the existing doctor convention rather than
//! inventing one: a clean database exits `OK` (0); a database still carrying an
//! un-erasable row exits `DOCTOR_FOUND_ISSUES` (65). `_legacy:` rows are
//! reported but are NOT an issue — they are erasable, merely not attributable.
//!
//! Read-only: the verb runs no DELETE/UPDATE. `orphan_provenance_is_read_only`
//! pins that by asserting the full per-source census is byte-identical before
//! and after an invocation.

use std::process::Command;

use fathomdb::{Engine, InitialState, PreparedWrite, SourceId};
use fathomdb_cli::exit_code;
use serde_json::Value;
use tempfile::TempDir;

fn fathomdb() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fathomdb"))
}

fn node(body: &str, source_id: &str, logical_id: Option<&str>) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: SourceId::new(source_id).expect("valid source id"),
        logical_id: logical_id.map(str::to_string),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// Seed a database with a known per-source census and return its path.
fn seeded_db(dir: &TempDir) -> String {
    let path = dir.path().join("orphan-provenance.sqlite");
    let path = path.to_str().expect("utf-8 path").to_string();

    let engine = Engine::open(&path).expect("open").engine;
    engine
        .write(&[
            node("alpha one", "tenant-a", None),
            node("alpha two", "tenant-a", None),
            node("alpha three", "tenant-a", None),
            node("beta one", "tenant-b", None),
            // A governed row: carries a logical_id, so it is purge-addressable.
            node("gamma governed", "tenant-b", Some("gov-1")),
        ])
        .expect("write");
    engine.close().expect("close");
    path
}

fn run_orphan_provenance(db_path: &str) -> (std::process::Output, Value) {
    let output = fathomdb()
        .args(["doctor", "orphan-provenance", "--json", db_path])
        .output()
        .expect("spawn fathomdb doctor orphan-provenance");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let value: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "doctor orphan-provenance --json must emit ONE JSON object (AC-037); \
             parse failed: {e}\nstdout={stdout}\nstderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output, value)
}

/// The design's named RED test (§4 item 11): the verb lists per-`source_id`
/// counts.
#[test]
fn doctor_lists_per_source_counts() {
    let dir = TempDir::new().expect("tempdir");
    let db_path = seeded_db(&dir);

    let (output, value) = run_orphan_provenance(&db_path);

    assert_eq!(
        output.status.code(),
        Some(exit_code::OK),
        "a database with no un-erasable row must exit OK; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(value["verb"], "orphan-provenance");

    let sources = value["sources"].as_array().expect("`sources` must be an array");

    // Per-source counts, asserted by VALUE (not merely "the key exists").
    let count_for = |sid: &str| -> i64 {
        sources
            .iter()
            .find(|s| s["source_id"] == sid)
            .unwrap_or_else(|| panic!("missing source_id {sid} in {sources:#?}"))["rows"]
            .as_i64()
            .expect("`rows` must be an integer")
    };
    assert_eq!(count_for("tenant-a"), 3, "tenant-a wrote 3 rows");
    assert_eq!(count_for("tenant-b"), 2, "tenant-b wrote 2 rows (1 anonymous + 1 governed)");

    // The un-erasable census is the load-bearing field, and it is ZERO on a
    // database created entirely after 5c.
    assert_eq!(
        value["unerasable_rows"].as_i64(),
        Some(0),
        "no row written through the 0.8.20 surface can lack BOTH handles"
    );
}

/// Read-only (design §4 item 11 says "read-only verb"): running the diagnostic
/// must not perturb the census it reports. Asserted by running it twice and
/// requiring the full `sources` payload to be byte-identical — a verb that
/// deleted or rewrote a row would drift.
#[test]
fn orphan_provenance_is_read_only() {
    let dir = TempDir::new().expect("tempdir");
    let db_path = seeded_db(&dir);

    let (_, before) = run_orphan_provenance(&db_path);
    let (_, after) = run_orphan_provenance(&db_path);

    assert_eq!(
        before["sources"], after["sources"],
        "orphan-provenance is READ-ONLY: two consecutive runs must report an identical census"
    );
    assert_eq!(before["unerasable_rows"], after["unerasable_rows"]);
}

/// Fabricate the legacy/corrupt row shape the migration comments call out: a
/// `canonical_edges` row with NULL `source_id` and a NON-NULL `logical_id`. No
/// supported write path can produce it (5c made provenance mandatory), so it is
/// injected with raw SQL — which is also what design §3 Rule 1 demands of an
/// erasure witness.
fn inject_null_source_governed_edge(db_path: &str) {
    let conn = rusqlite::Connection::open(db_path).expect("open sqlite");
    conn.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, source_id, logical_id)
         VALUES(999001, 'link', 'n-1', 'n-2', NULL, 'edge-gov-1')",
        [],
    )
    .expect("inject NULL-source governed edge");
}

/// codex §9 [P2] — an edge's `logical_id` is a SUPERSESSION identity and confers
/// NO purge-addressability: `purge_inner` resolves its target exclusively
/// through `canonical_nodes` (`SELECT state FROM canonical_nodes WHERE
/// logical_id = ?1`) and then erases edges by ENDPOINT. So a `canonical_edges`
/// row with NULL `source_id` is reachable by NO erasure verb whatever its
/// `logical_id`, and the doctor must say so.
///
/// This is the reporting-surface half of the P1 that fix-1 corrected in
/// migration step 21: with the doctor still crediting an edge `logical_id` as
/// purge-addressable, `orphan-provenance` exits CLEAN on precisely the hole the
/// migration now closes — false assurance, which is worse than no verb.
#[test]
fn null_source_governed_edge_counts_as_unerasable() {
    let dir = TempDir::new().expect("tempdir");
    let db_path = seeded_db(&dir);
    inject_null_source_governed_edge(&db_path);

    let (output, value) = run_orphan_provenance(&db_path);

    // The doctor's own report is the assertion surface (Rule 1: raw state, never
    // a search).
    assert_eq!(
        value["unerasable_rows"].as_i64(),
        Some(1),
        "a canonical_edges row with NULL source_id is erasable by NO verb — an edge \
         logical_id is not purge-addressable; report={value:#?}"
    );
    assert_eq!(
        output.status.code(),
        Some(exit_code::DOCTOR_FOUND_ISSUES),
        "a database still carrying an un-erasable row must NOT exit clean; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The node side of the asymmetry is UNCHANGED, and this pins that the fix did
/// not over-correct into it: a `canonical_nodes` row with NULL `source_id` but a
/// non-NULL `logical_id` genuinely IS reachable — `purge` resolves it by
/// `logical_id` — so it must NOT be counted un-erasable.
#[test]
fn null_source_governed_node_stays_erasable() {
    let dir = TempDir::new().expect("tempdir");
    let db_path = seeded_db(&dir);
    {
        let conn = rusqlite::Connection::open(&db_path).expect("open sqlite");
        conn.execute(
            "INSERT INTO canonical_nodes(write_cursor, kind, body, state, source_id, logical_id)
             VALUES(999002, 'doc', 'legacy governed node', 'active', NULL, 'node-gov-1')",
            [],
        )
        .expect("inject NULL-source governed node");
    }

    let (output, value) = run_orphan_provenance(&db_path);

    assert_eq!(
        value["unerasable_rows"].as_i64(),
        Some(0),
        "a governed NODE with NULL source_id is purge-addressable by logical_id and is \
         NOT un-erasable; report={value:#?}"
    );
    assert_eq!(output.status.code(), Some(exit_code::OK));
}

/// `--help` exits 0 with a `Usage:` line, matching AC-040a/AC-040b for every
/// other doctor verb. (The shared `DOCTOR_VERBS` table in `operator_cli.rs` is
/// extended in the same commit; this is the local guard.)
#[test]
fn orphan_provenance_help_exits_zero_with_usage() {
    let output =
        fathomdb().args(["doctor", "orphan-provenance", "--help"]).output().expect("spawn");
    assert!(output.status.success(), "doctor orphan-provenance --help must exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().any(|line| line.starts_with("Usage:")),
        "doctor orphan-provenance --help must include a `Usage:` line; got:\n{stdout}"
    );
}
