//! AC-040a engine half: `Engine::dump_row_counts` reports per-canonical
//! table row counts; counts increment after a canonical write.

use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::{CANONICAL_TABLES, SQLITE_SUFFIX};
use tempfile::TempDir;

fn counts_for(report: &fathomdb_engine::DumpRowCountsReport, table: &str) -> u64 {
    report
        .counts
        .iter()
        .find(|c| c.name == table)
        .map(|c| c.rows)
        .unwrap_or_else(|| panic!("table {table} missing from counts"))
}

#[test]
fn ac_040a_dump_row_counts_empty_database_returns_all_zero() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("counts{SQLITE_SUFFIX}"));
    let opened = Engine::open(path).expect("open");
    let report = opened.engine.dump_row_counts().expect("dump_row_counts");

    let names: Vec<&str> = report.counts.iter().map(|c| c.name.as_str()).collect();
    for canonical in CANONICAL_TABLES {
        assert!(names.contains(canonical), "missing canonical {canonical}");
    }
    assert_eq!(counts_for(&report, "canonical_nodes"), 0);
    assert_eq!(counts_for(&report, "canonical_edges"), 0);
}

#[test]
fn ac_040a_dump_row_counts_reflects_canonical_writes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("counts_after{SQLITE_SUFFIX}"));
    let opened = Engine::open(path).expect("open");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "alpha".to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write");
    let report = opened.engine.dump_row_counts().expect("dump_row_counts");
    assert_eq!(counts_for(&report, "canonical_nodes"), 1);
    assert_eq!(counts_for(&report, "canonical_edges"), 0);
}
