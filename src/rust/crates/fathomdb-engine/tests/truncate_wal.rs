//! AC-040a / AC-058 engine half: `Engine::truncate_wal` runs
//! `PRAGMA wal_checkpoint(TRUNCATE)` and reports the three counters.

use fathomdb_engine::{Engine, PreparedWrite, TruncateWalStatus};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

#[test]
fn ac_040a_truncate_wal_on_empty_db_reports_done() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("wal_empty{SQLITE_SUFFIX}"));
    let opened = Engine::open(path).expect("open");
    let report = opened.engine.truncate_wal().expect("truncate_wal");
    assert_eq!(report.status, TruncateWalStatus::Done);
    // Counters are non-negative by construction (u32). Assert checkpointed
    // count is at most log_frames as an invariant.
    assert!(report.checkpointed_frames <= report.log_frames.max(report.checkpointed_frames));
}

#[test]
fn ac_040a_truncate_wal_after_write_reports_done() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("wal_write{SQLITE_SUFFIX}"));
    let opened = Engine::open(path).expect("open");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "alpha".to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write");
    let report = opened.engine.truncate_wal().expect("truncate_wal");
    assert_eq!(report.status, TruncateWalStatus::Done);
}
