use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn open_fixture(name: &str) -> (TempDir, fathomdb_engine::OpenedEngine) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    let opened = Engine::open(path).unwrap();
    (dir, opened)
}

#[test]
fn ac_059a_projection_cursor_is_monotonic_non_decreasing() {
    let (_dir, opened) = open_fixture("monotonic");
    let mut previous = opened.engine.search("doc").unwrap().projection_cursor;

    for i in 0..1_000_u32 {
        if i % 10 == 0 {
            opened
                .engine
                .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: format!("doc {i}") }])
                .unwrap();
        }
        let current = opened.engine.search("doc").unwrap().projection_cursor;
        assert!(current >= previous);
        previous = current;
    }
}

#[test]
fn ac_059b_write_cursor_is_satisfied_by_projection_cursor_and_queryable() {
    let (_dir, opened) = open_fixture("satisfied");

    let write_cursor = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "findable phase seven document".to_string(),
        }])
        .unwrap()
        .cursor;

    let result = opened.engine.search("findable").unwrap();
    assert!(result.projection_cursor >= write_cursor);
    assert!(result.results.iter().any(|row| row.contains("findable phase seven document")));
}
