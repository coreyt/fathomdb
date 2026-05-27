// Pack 1 tests for the binary-quant schema migration and ingest
// double-write per `dev/design/0.7.0-vector-quant-pack1.md` D1-D8.

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, EngineOpenError};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::{params, Connection};
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct FixedEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
}

impl FixedEmbedder {
    fn new(dim: u32, vector: Vector) -> Self {
        Self { identity: EmbedderIdentity::new("pack1-fixed", "rev-a", dim), vector }
    }
}

impl Embedder for FixedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        Ok(self.vector.clone())
    }
}

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn half_half_768() -> Vector {
    let mut v = vec![0.0_f32; 768];
    for slot in v.iter_mut().take(384) {
        *slot = 1.0;
    }
    for slot in v.iter_mut().skip(384) {
        *slot = -1.0;
    }
    v
}

fn popcount(bytes: &[u8]) -> u32 {
    bytes.iter().map(|b| b.count_ones()).sum()
}

#[test]
fn binary_quant_roundtrip_popcount_384() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "popcount");
    let embedder = Arc::new(FixedEmbedder::new(768, half_half_768()));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure kind");
    let receipt = opened.engine.write_vector_for_test("doc", "anything").expect("write");
    drop(opened);

    let conn = Connection::open(&path).expect("reopen raw");
    let bin: Vec<u8> = conn
        .query_row(
            "SELECT embedding_bin FROM vector_default WHERE rowid = ?1",
            [receipt.cursor as i64],
            |row| row.get(0),
        )
        .expect("read embedding_bin");
    assert_eq!(bin.len(), 96, "768 bits packed into 96 bytes");
    assert_eq!(popcount(&bin), 384, "half-half vector must yield popcount 384");
}

#[test]
fn migration_preflight_rejects_unknown_kind() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "preflight");

    // 1. Open + close to land schemas 1..=current. Post-Pack 1 this
    //    lands schema 9; pre-Pack 1 it lands schema 8.
    {
        let opened = Engine::open(&path).expect("initial open");
        drop(opened);
    }

    // 2. Seed an unknown kind into _fathomdb_vector_rows and rewind
    //    user_version to 8 so the next open re-runs step 9.
    {
        let conn = Connection::open(&path).expect("raw open");
        conn.execute(
            "INSERT INTO _fathomdb_vector_rows(rowid, kind, write_cursor) VALUES(?1, 'banana', ?1)",
            [1_i64],
        )
        .expect("seed banana");
        conn.pragma_update(None, "user_version", 8_u32).expect("rewind user_version");
    }

    // 3. Re-open. Migration step 9's preflight CHECK must fire and
    //    surface as EngineOpenError::MigrationError { step_id: 9, .. }.
    let err = Engine::open(&path).expect_err("preflight must reject 'banana'");
    match err {
        EngineOpenError::MigrationError { step_id, .. } => {
            assert_eq!(step_id, 9, "preflight failure must surface as step 9");
        }
        other => panic!("expected MigrationError step 9, got {other:?}"),
    }
}

#[test]
fn writer_double_write_populates_bin_column() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "bincol");
    let embedder = Arc::new(FixedEmbedder::new(8, vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure kind");
    opened.engine.write_vector_for_test("doc", "row-1").expect("write 1");
    opened.engine.write_vector_for_test("doc", "row-2").expect("write 2");
    drop(opened);

    let conn = Connection::open(&path).expect("reopen raw");
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM vector_default", [], |row| row.get(0))
        .expect("count rows");
    let bin_set: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM vector_default WHERE embedding_bin IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("count bin");
    assert_eq!(total, 2);
    assert_eq!(bin_set, 2, "every row must have embedding_bin populated");
}

#[test]
fn writer_populates_source_type_partition_key() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "stype");
    let embedder = Arc::new(FixedEmbedder::new(8, vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure kind");
    let receipt = opened.engine.write_vector_for_test("doc", "row-1").expect("write");
    drop(opened);

    let conn = Connection::open(&path).expect("reopen raw");
    let source_type: String = conn
        .query_row(
            "SELECT source_type FROM vector_default WHERE rowid = ?1",
            params![receipt.cursor as i64],
            |row| row.get(0),
        )
        .expect("read source_type");
    assert_eq!(source_type, "article", "kind=doc must coerce to source_type=article (D3)");
}
