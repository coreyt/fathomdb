//! Pack B: `AdminService::configure_embedding` database-wide embedding
//! identity admin API.
//!
//! These tests cover the four behavioural variants of the configure_embedding
//! contract:
//!
//!   1. Fresh engine → new active profile row persisted.
//!   2. Same identity twice → no-op, `Unchanged` outcome.
//!   3. Different identity, ack given, existing enabled kinds → stale.
//!   4. Different identity, ack missing, existing enabled kinds → error + no
//!      mutation.
//!   5. Different identity, ack missing, no enabled kinds → proceeds.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::sync::Arc;

use fathomdb_engine::{
    AdminService, ConfigureEmbeddingOutcome, EmbedderError, EngineError, QueryEmbedder,
    QueryEmbedderIdentity,
};
use fathomdb_schema::SchemaManager;

/// Minimal test-only embedder returning a caller-controlled identity.
#[derive(Debug)]
struct FakeEmbedder {
    identity: QueryEmbedderIdentity,
}

impl FakeEmbedder {
    fn new(
        model_identity: &str,
        model_version: &str,
        dimension: usize,
        normalization_policy: &str,
    ) -> Self {
        Self {
            identity: QueryEmbedderIdentity {
                model_identity: model_identity.to_owned(),
                model_version: model_version.to_owned(),
                dimension,
                normalization_policy: normalization_policy.to_owned(),
            },
        }
    }
}

impl QueryEmbedder for FakeEmbedder {
    fn embed_query(&self, _text: &str) -> Result<Vec<f32>, EmbedderError> {
        Ok(vec![0.0_f32; self.identity.dimension])
    }
    fn identity(&self) -> QueryEmbedderIdentity {
        self.identity.clone()
    }
    fn max_tokens(&self) -> usize {
        512
    }
}

struct Harness {
    _dir: tempfile::TempDir,
    db_path: std::path::PathBuf,
    service: AdminService,
}

fn new_admin() -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let schema = Arc::new(SchemaManager::new());
    let service = AdminService::new(&db_path, schema);
    // Touch a connection to force schema bootstrap (v24 creates the tables).
    let _ = service
        .check_integrity()
        .expect("bootstrap via check_integrity");
    Harness {
        _dir: dir,
        db_path,
        service,
    }
}

fn connect(h: &Harness) -> rusqlite::Connection {
    let schema = SchemaManager::new();
    let conn = rusqlite::Connection::open(&h.db_path).expect("open");
    schema.bootstrap(&conn).expect("bootstrap");
    conn
}

fn count_active_profiles(conn: &rusqlite::Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM vector_embedding_profiles WHERE active = 1",
        [],
        |r| r.get::<_, i64>(0),
    )
    .expect("count")
}

fn get_active_identity(conn: &rusqlite::Connection) -> (String, String, i64) {
    conn.query_row(
        "SELECT model_identity, COALESCE(model_version,''), dimensions \
         FROM vector_embedding_profiles WHERE active = 1",
        [],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        },
    )
    .expect("active row")
}

fn seed_enabled_kind(conn: &rusqlite::Connection, kind: &str, state: &str) {
    conn.execute(
        "INSERT INTO vector_index_schemas \
            (kind, enabled, source_mode, state, created_at, updated_at) \
         VALUES (?1, 1, 'chunks', ?2, unixepoch(), unixepoch())",
        rusqlite::params![kind, state],
    )
    .expect("seed vector_index_schemas");
}

fn get_kind_state(conn: &rusqlite::Connection, kind: &str) -> String {
    conn.query_row(
        "SELECT state FROM vector_index_schemas WHERE kind = ?1",
        rusqlite::params![kind],
        |r| r.get::<_, String>(0),
    )
    .expect("kind row")
}

#[test]
fn test_configure_embedding_fresh_engine_persists_identity() {
    let h = new_admin();
    let svc = &h.service;
    let embedder = FakeEmbedder::new("bge-small-en-v1.5", "1.5", 384, "l2");
    let outcome = svc
        .configure_embedding(&embedder, false)
        .expect("configure fresh");
    match outcome {
        ConfigureEmbeddingOutcome::Activated { profile_id } => {
            assert!(profile_id > 0, "profile_id should be positive");
        }
        other => panic!("expected Activated, got {other:?}"),
    }
    let conn = connect(&h);
    assert_eq!(count_active_profiles(&conn), 1);
    let (id, ver, dim) = get_active_identity(&conn);
    assert_eq!(id, "bge-small-en-v1.5");
    assert_eq!(ver, "1.5");
    assert_eq!(dim, 384);
}

#[test]
fn test_configure_embedding_identical_config_is_noop() {
    let h = new_admin();
    let svc = &h.service;
    let embedder = FakeEmbedder::new("bge-small-en-v1.5", "1.5", 384, "l2");
    let first = svc.configure_embedding(&embedder, false).expect("first");
    let first_id = match first {
        ConfigureEmbeddingOutcome::Activated { profile_id } => profile_id,
        other => panic!("expected Activated, got {other:?}"),
    };
    let second = svc.configure_embedding(&embedder, false).expect("second");
    match second {
        ConfigureEmbeddingOutcome::Unchanged { profile_id } => {
            assert_eq!(profile_id, first_id);
        }
        other => panic!("expected Unchanged, got {other:?}"),
    }
    let conn = connect(&h);
    assert_eq!(count_active_profiles(&conn), 1);
}

#[test]
fn test_configure_embedding_identity_change_marks_kinds_stale() {
    let h = new_admin();
    let svc = &h.service;
    let first = FakeEmbedder::new("bge-small-en-v1.5", "1.5", 384, "l2");
    svc.configure_embedding(&first, false).expect("first");

    let conn = connect(&h);
    seed_enabled_kind(&conn, "Article", "ready");
    drop(conn);

    let second = FakeEmbedder::new("other-model", "1.0", 768, "l2");
    let outcome = svc
        .configure_embedding(&second, true)
        .expect("configure with ack");
    match outcome {
        ConfigureEmbeddingOutcome::Replaced { stale_kinds, .. } => {
            assert_eq!(stale_kinds, 1, "expected one kind marked stale");
        }
        other => panic!("expected Replaced, got {other:?}"),
    }
    let conn = connect(&h);
    assert_eq!(count_active_profiles(&conn), 1);
    let (id, _, dim) = get_active_identity(&conn);
    assert_eq!(id, "other-model");
    assert_eq!(dim, 768);
    assert_eq!(get_kind_state(&conn, "Article"), "stale");
}

#[test]
fn test_configure_embedding_identity_change_without_ack_rejects() {
    let h = new_admin();
    let svc = &h.service;
    let first = FakeEmbedder::new("bge-small-en-v1.5", "1.5", 384, "l2");
    svc.configure_embedding(&first, false).expect("first");

    let conn = connect(&h);
    seed_enabled_kind(&conn, "Article", "ready");
    drop(conn);

    let second = FakeEmbedder::new("other-model", "1.0", 768, "l2");
    let err = svc
        .configure_embedding(&second, false)
        .expect_err("should reject without ack");
    match err {
        EngineError::EmbeddingChangeRequiresAck { affected_kinds } => {
            assert_eq!(affected_kinds, 1);
        }
        other => panic!("expected EmbeddingChangeRequiresAck, got {other:?}"),
    }
    let conn = connect(&h);
    // Active profile still the original identity; no mutation.
    let (id, _, dim) = get_active_identity(&conn);
    assert_eq!(id, "bge-small-en-v1.5");
    assert_eq!(dim, 384);
    assert_eq!(get_kind_state(&conn, "Article"), "ready");
}

#[test]
fn test_configure_embedding_identity_change_no_enabled_kinds_no_ack_needed() {
    let h = new_admin();
    let svc = &h.service;
    let first = FakeEmbedder::new("bge-small-en-v1.5", "1.5", 384, "l2");
    svc.configure_embedding(&first, false).expect("first");
    // No enabled kinds seeded.
    let second = FakeEmbedder::new("other-model", "1.0", 768, "l2");
    let outcome = svc
        .configure_embedding(&second, false)
        .expect("should proceed without ack when no enabled kinds");
    match outcome {
        ConfigureEmbeddingOutcome::Replaced { stale_kinds, .. } => {
            assert_eq!(stale_kinds, 0);
        }
        other => panic!("expected Replaced, got {other:?}"),
    }
}
