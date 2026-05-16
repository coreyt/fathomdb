//! AC-040a engine half: `Engine::dump_schema` enumerates the on-disk
//! schema + `PRAGMA user_version` sentinel, excluding `sqlite_*`
//! internal objects.

use fathomdb_engine::Engine;
use fathomdb_schema::{CANONICAL_TABLES, SCHEMA_VERSION, SQLITE_SUFFIX};
use tempfile::TempDir;

#[test]
fn ac_040a_dump_schema_reports_current_user_version_and_canonical_tables() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("schema{SQLITE_SUFFIX}"));
    let opened = Engine::open(path).expect("open");
    let report = opened.engine.dump_schema().expect("dump_schema");

    assert!(report.user_version > 0, "user_version must be > 0 after open");
    assert_eq!(report.user_version, SCHEMA_VERSION);

    for canonical in CANONICAL_TABLES {
        let found = report.tables.iter().any(|t| t.name == *canonical);
        assert!(found, "canonical table {canonical} missing from dump_schema");
    }

    let canonical_set: std::collections::HashSet<&str> = CANONICAL_TABLES.iter().copied().collect();
    let leading_canonical: Vec<&str> = report
        .tables
        .iter()
        .take_while(|t| canonical_set.contains(t.name.as_str()))
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(leading_canonical.len(), CANONICAL_TABLES.len(), "canonical tables must lead");
    for (i, name) in CANONICAL_TABLES.iter().enumerate() {
        assert_eq!(&leading_canonical[i], name, "canonical table ordering mismatch");
    }

    for t in &report.tables {
        assert!(!t.name.starts_with("sqlite_"), "sqlite_* row leaked: {}", t.name);
        assert!(!t.sql.is_empty(), "every table must have non-empty sql");
    }
}
