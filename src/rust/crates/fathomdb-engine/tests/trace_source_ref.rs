use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn write_node(engine: &fathomdb_engine::Engine, body: &str, source_id: &str) -> u64 {
    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: body.to_string(),
            source_id: Some(source_id.to_string()),
        }])
        .expect("write")
        .cursor
}

#[test]
fn ac_042_trace_source_ref_returns_exact_cursor_set_for_named_source() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "trace_exact");
    let opened = Engine::open(&path).expect("open");

    let mut s1_cursors = Vec::new();
    let mut s2_cursors = Vec::new();

    // Interleave S1 and S2 writes so the assertion proves filtering, not
    // ordering happenstance.
    for i in 0..15 {
        if i < 10 {
            s1_cursors.push(write_node(&opened.engine, &format!("s1 doc {i}"), "S1"));
        }
        s2_cursors.push(write_node(&opened.engine, &format!("s2 doc {i}"), "S2"));
    }

    let report = opened.engine.trace_source_ref("S1").expect("trace");
    assert_eq!(report.source_ref, "S1");
    let cursors: Vec<u64> = report.events.iter().map(|e| e.write_cursor).collect();
    assert_eq!(cursors, s1_cursors, "trace_source_ref must return S1's cursors exactly");
    assert!(report.events.iter().all(|e| e.table == "canonical_nodes"));
    assert!(report.events.iter().all(|e| e.kind == "doc"));
}

#[test]
fn ac_042_trace_source_ref_includes_edges_alongside_nodes() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "trace_edges");
    let opened = Engine::open(&path).expect("open");

    write_node(&opened.engine, "node from S1", "S1");
    opened
        .engine
        .write(&[PreparedWrite::Edge {
            kind: "rel".to_string(),
            from: "a".to_string(),
            to: "b".to_string(),
            source_id: Some("S1".to_string()),
        }])
        .expect("edge write");

    let report = opened.engine.trace_source_ref("S1").expect("trace");
    assert_eq!(report.events.len(), 2);
    let tables: Vec<&str> = report.events.iter().map(|e| e.table).collect();
    assert!(tables.contains(&"canonical_nodes"));
    assert!(tables.contains(&"canonical_edges"));
}

#[test]
fn ac_042_trace_source_ref_excludes_null_source_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "trace_null");
    let opened = Engine::open(&path).expect("open");

    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "nullable".to_string(),
            source_id: None,
        }])
        .expect("write");

    opened
        .engine
        .trace_source_ref("")
        .expect_err("empty string must be rejected as invalid source_id");

    let report_unknown = opened.engine.trace_source_ref("NOPE").expect("trace unknown");
    assert!(report_unknown.events.is_empty());
}
