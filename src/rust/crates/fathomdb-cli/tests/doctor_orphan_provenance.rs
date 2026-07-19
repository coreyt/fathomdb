//! 0.8.20 Slice 5d (R-20-E8, design §4 item 11) — `fathomdb doctor
//! orphan-provenance`: a READ-ONLY diagnostic listing per-`source_id` row
//! counts, and flagging rows that no erasure verb can reach.
//!
//! **What "orphan provenance" means here.** After Slice 5c every canonical row
//! is addressable by exactly one of two handles:
//!
//! * `logical_id` — governed rows, reachable by `purge`;
//! * `source_id` — anonymous rows, reachable by `erase_source`.
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
