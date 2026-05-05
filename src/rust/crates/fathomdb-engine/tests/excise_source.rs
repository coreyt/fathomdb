use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn write_node(engine: &Engine, body: &str, source_id: &str) -> u64 {
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
fn ac_028a_excise_source_appends_audit_row_naming_source() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "excise_audit");
    let opened = Engine::open(&path).expect("open");
    write_node(&opened.engine, "alpha", "S1");
    write_node(&opened.engine, "beta", "S1");

    let report = opened.engine.excise_source("S1").expect("excise");
    assert_eq!(report.source_ref, "S1");
    assert_eq!(report.nodes_excised, 2);
    assert_eq!(report.edges_excised, 0);

    opened.engine.close().unwrap();

    let conn = Connection::open(&path).expect("open sqlite");
    let (count, payload): (u64, String) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(MAX(payload_json), '') FROM operational_mutations
             WHERE collection_name = 'excise_source_audit' AND record_key = ?1",
            ["S1"],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?)),
        )
        .expect("audit row");
    assert!(count >= 1, "expected at least one audit row, got {count}");
    assert!(payload.contains("\"source_id\":\"S1\""), "payload should name source: {payload}");
    assert!(payload.contains("\"nodes_excised\":2"));
}

#[test]
fn ac_028b_excise_source_zero_residue_in_shadow_tables() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "excise_residue");
    let opened = Engine::open(&path).expect("open");

    let s1_a = write_node(&opened.engine, "s1 alpha s2-token", "S1");
    let s1_b = write_node(&opened.engine, "s1 beta s2-token", "S1");
    write_node(&opened.engine, "s2 gamma s2-token", "S2");

    opened.engine.excise_source("S1").expect("excise");
    opened.engine.close().unwrap();

    let conn = Connection::open(&path).expect("open sqlite");
    for cursor in [s1_a, s1_b] {
        let in_search: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM search_index WHERE write_cursor = ?1",
                [cursor],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(in_search, 0, "search_index residue for cursor {cursor}");
        let in_terminal: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _fathomdb_projection_terminal WHERE write_cursor = ?1",
                [cursor],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(in_terminal, 0, "projection_terminal residue for cursor {cursor}");
        let in_vec_rows: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
                [cursor],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(in_vec_rows, 0, "_fathomdb_vector_rows residue for cursor {cursor}");
    }
}

#[test]
fn ac_028c_excise_source_does_not_perturb_other_sources() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "excise_nonperturb");
    let opened = Engine::open(&path).expect("open");

    write_node(&opened.engine, "s1 alpha s1onlytoken", "S1");
    write_node(&opened.engine, "s1 beta s1onlytoken", "S1");
    write_node(&opened.engine, "s2 gamma s2onlytoken", "S2");
    write_node(&opened.engine, "s2 delta s2onlytoken", "S2");

    let s2_before = opened.engine.search("s2onlytoken").expect("search").results;
    let mut s2_before_sorted = s2_before.clone();
    s2_before_sorted.sort();
    assert_eq!(s2_before_sorted.len(), 2, "S2 baseline = 2");

    opened.engine.excise_source("S1").expect("excise");

    let mut s2_after = opened.engine.search("s2onlytoken").expect("search").results;
    s2_after.sort();
    assert_eq!(s2_after, s2_before_sorted, "S2 result set must be untouched by S1 excise");

    opened.engine.close().unwrap();

    let conn = Connection::open(&path).expect("open sqlite");
    let s2_canonical: u64 = conn
        .query_row("SELECT COUNT(*) FROM canonical_nodes WHERE source_id = 'S2'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(s2_canonical, 2, "S2 canonical rows untouched");
    let s1_canonical: u64 = conn
        .query_row("SELECT COUNT(*) FROM canonical_nodes WHERE source_id = 'S1'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(s1_canonical, 0, "S1 canonical rows excised");
}
