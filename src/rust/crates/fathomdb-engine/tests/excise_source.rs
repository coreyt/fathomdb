use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
}

impl DeterministicEmbedder {
    fn new(dim: u32) -> Self {
        let mut vector = vec![0.0_f32; dim as usize];
        vector[0] = 1.0;
        Self { identity: EmbedderIdentity::new("excise-test", "rev-a", dim), vector }
    }
}

impl Embedder for DeterministicEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        Ok(self.vector.clone())
    }
}

fn wait_until<F: FnMut() -> bool>(mut predicate: F, timeout: Duration) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if predicate() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    predicate()
}

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

#[test]
fn excise_source_audit_cursor_exceeds_max_canonical_cursor() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "excise_audit_cursor");
    let opened = Engine::open(&path).expect("open");
    let c1 = write_node(&opened.engine, "alpha", "S1");
    let c2 = write_node(&opened.engine, "beta", "S1");
    let max_canonical = c1.max(c2);

    opened.engine.excise_source("S1").expect("excise");
    opened.engine.close().unwrap();

    let conn = Connection::open(&path).expect("open sqlite");
    let audit_cursor: i64 = conn
        .query_row(
            "SELECT write_cursor FROM operational_mutations
             WHERE collection_name = 'excise_source_audit' AND record_key = ?1",
            ["S1"],
            |row| row.get(0),
        )
        .expect("audit row");
    assert!(
        (audit_cursor as u64) > max_canonical,
        "audit write_cursor {audit_cursor} must exceed max canonical cursor {max_canonical}"
    );
}

#[test]
fn excise_source_audit_records_wallclock_excised_at() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "excise_audit_excised_at");
    let opened = Engine::open(&path).expect("open");
    write_node(&opened.engine, "alpha", "S1");

    let before = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    opened.engine.excise_source("S1").expect("excise");
    let after = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    opened.engine.close().unwrap();

    let conn = Connection::open(&path).expect("open sqlite");
    let payload: String = conn
        .query_row(
            "SELECT payload_json FROM operational_mutations
             WHERE collection_name = 'excise_source_audit' AND record_key = ?1",
            ["S1"],
            |row| row.get(0),
        )
        .expect("audit row");
    let value: serde_json::Value = serde_json::from_str(&payload).expect("payload json");
    let excised_at = value.get("excised_at").and_then(|v| v.as_u64()).expect("excised_at u64");
    assert!(
        excised_at >= before && excised_at <= after,
        "excised_at {excised_at} must be in [{before}, {after}]"
    );
}

#[test]
fn excise_source_clears_vec0_rows_for_excised_source_only() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "excise_vec0");

    let embedder = Arc::new(DeterministicEmbedder::new(8));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let s1_a = write_node(&opened.engine, "s1 alpha vec", "S1");
    let s1_b = write_node(&opened.engine, "s1 beta vec", "S1");
    let s2_a = write_node(&opened.engine, "s2 gamma vec", "S2");

    opened.engine.drain(10_000).expect("drain projections");

    // Sanity: vec0 actually got rows for both sources before excise.
    let pre_total: i64 = {
        let conn = Connection::open(&path).expect("open sqlite ro");
        conn.query_row("SELECT COUNT(*) FROM vector_default", [], |row| row.get(0))
            .expect("count vec0 pre")
    };
    assert!(pre_total >= 3, "expected at least 3 vec0 rows from S1+S2 writes, got {pre_total}");

    opened.engine.excise_source("S1").expect("excise");

    // Drain again so any post-excise projection settling completes.
    assert!(wait_until(
        || {
            let conn = match Connection::open(&path) {
                Ok(c) => c,
                Err(_) => return false,
            };
            let s2_present: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM vector_default WHERE rowid = ?1",
                    [s2_a as i64],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            s2_present == 1
        },
        Duration::from_secs(5),
    ));

    opened.engine.close().unwrap();

    let conn = Connection::open(&path).expect("open sqlite");
    for cursor in [s1_a, s1_b] {
        let in_vec0: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vector_default WHERE rowid = ?1",
                [cursor as i64],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(in_vec0, 0, "vec0 residue for excised cursor {cursor}");
    }
    let s2_in_vec0: i64 = conn
        .query_row("SELECT COUNT(*) FROM vector_default WHERE rowid = ?1", [s2_a as i64], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(s2_in_vec0, 1, "S2's vec0 row must survive S1 excise");
}
