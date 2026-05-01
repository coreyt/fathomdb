//! Schema tests for the managed vector projection tables (pack A).
//!
//! These tables are introduced in `SchemaVersion(25)` to support
//! database-wide embedding profiles, per-kind vector indexing config,
//! and a durable async work queue. `model_identity` is recorded by the
//! embedder; it is never user-configurable. See
//! `dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md`.

#![allow(clippy::expect_used)]

use std::collections::HashMap;

use fathomdb_schema::SchemaManager;
use rusqlite::Connection;

fn bootstrapped() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory sqlite");
    let manager = SchemaManager::new();
    manager.bootstrap(&conn).expect("bootstrap");
    conn
}

fn column_map(conn: &Connection, table: &str) -> HashMap<String, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("prepare table_info");
    let rows = stmt
        .query_map([], |row| {
            let name: String = row.get(1)?;
            let col_type: String = row.get(2)?;
            Ok((name, col_type))
        })
        .expect("query_map");
    rows.collect::<Result<HashMap<_, _>, _>>()
        .expect("collect columns")
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .expect("count table");
    count == 1
}

fn index_exists(conn: &Connection, index: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [index],
            |row| row.get(0),
        )
        .expect("count index");
    count == 1
}

#[test]
fn test_vector_embedding_profiles_table_created() {
    let conn = bootstrapped();
    assert!(table_exists(&conn, "vector_embedding_profiles"));
    let cols = column_map(&conn, "vector_embedding_profiles");
    for name in [
        "profile_id",
        "profile_name",
        "model_identity",
        "model_version",
        "dimensions",
        "normalization_policy",
        "max_tokens",
        "active",
        "activated_at",
        "created_at",
    ] {
        assert!(cols.contains_key(name), "missing column {name}");
    }
    assert!(index_exists(&conn, "idx_vep_singleton_active"));
    assert!(index_exists(&conn, "idx_vep_identity"));
}

#[test]
fn test_vector_index_schemas_table_created() {
    let conn = bootstrapped();
    assert!(table_exists(&conn, "vector_index_schemas"));
    let cols = column_map(&conn, "vector_index_schemas");
    for name in [
        "kind",
        "enabled",
        "source_mode",
        "source_config_json",
        "chunking_policy",
        "preprocessing_policy",
        "state",
        "last_error",
        "last_completed_at",
        "created_at",
        "updated_at",
    ] {
        assert!(cols.contains_key(name), "missing column {name}");
    }
}

#[test]
fn test_vector_projection_work_table_created() {
    let conn = bootstrapped();
    assert!(table_exists(&conn, "vector_projection_work"));
    let cols = column_map(&conn, "vector_projection_work");
    for name in [
        "work_id",
        "kind",
        "node_logical_id",
        "chunk_id",
        "canonical_hash",
        "priority",
        "embedding_profile_id",
        "attempt_count",
        "last_error",
        "state",
        "created_at",
        "updated_at",
    ] {
        assert!(cols.contains_key(name), "missing column {name}");
    }
    assert!(index_exists(&conn, "idx_vpw_schedule"));
    assert!(index_exists(&conn, "idx_vpw_chunk"));
}

#[test]
fn test_singleton_active_profile_constraint() {
    let conn = bootstrapped();
    conn.execute(
        "INSERT INTO vector_embedding_profiles
            (profile_name, model_identity, model_version, dimensions,
             normalization_policy, max_tokens, active, activated_at, created_at)
         VALUES ('p1', 'ident-a', 'v1', 384, 'l2', 8192, 1, 1, 1)",
        [],
    )
    .expect("first active insert");

    let err = conn.execute(
        "INSERT INTO vector_embedding_profiles
            (profile_name, model_identity, model_version, dimensions,
             normalization_policy, max_tokens, active, activated_at, created_at)
         VALUES ('p2', 'ident-b', 'v1', 384, 'l2', 8192, 1, 2, 2)",
        [],
    );
    assert!(
        err.is_err(),
        "second active=1 insert must violate unique index"
    );

    // Inserting an inactive row alongside the active row succeeds.
    conn.execute(
        "INSERT INTO vector_embedding_profiles
            (profile_name, model_identity, model_version, dimensions,
             normalization_policy, max_tokens, active, activated_at, created_at)
         VALUES ('p3', 'ident-c', 'v1', 384, 'l2', 8192, 0, NULL, 3)",
        [],
    )
    .expect("inactive insert allowed");
}

#[test]
fn test_queue_schedule_index_order() {
    let conn = bootstrapped();
    conn.execute(
        "INSERT INTO vector_embedding_profiles
            (profile_name, model_identity, model_version, dimensions,
             normalization_policy, max_tokens, active, activated_at, created_at)
         VALUES ('p1', 'ident-a', 'v1', 384, 'l2', 8192, 1, 1, 1)",
        [],
    )
    .expect("profile");
    let profile_id: i64 = conn
        .query_row(
            "SELECT profile_id FROM vector_embedding_profiles WHERE profile_name = 'p1'",
            [],
            |row| row.get(0),
        )
        .expect("profile id");

    let rows = [
        ("chunk-a", 1_i64, 100_i64),
        ("chunk-b", 5_i64, 200_i64),
        ("chunk-c", 5_i64, 150_i64),
        ("chunk-d", 10_i64, 300_i64),
    ];
    for (chunk_id, priority, created_at) in rows {
        conn.execute(
            "INSERT INTO vector_projection_work
                (kind, node_logical_id, chunk_id, canonical_hash, priority,
                 embedding_profile_id, attempt_count, last_error, state,
                 created_at, updated_at)
             VALUES ('doc', 'node-1', ?1, 'hash', ?2, ?3, 0, NULL, 'pending', ?4, ?4)",
            rusqlite::params![chunk_id, priority, profile_id, created_at],
        )
        .expect("insert work");
    }

    let mut stmt = conn
        .prepare(
            "SELECT chunk_id FROM vector_projection_work
              WHERE state = 'pending'
              ORDER BY priority DESC, created_at ASC",
        )
        .expect("prepare");
    let ordered: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect");
    assert_eq!(
        ordered,
        vec![
            "chunk-d".to_string(),
            "chunk-c".to_string(),
            "chunk-b".to_string(),
            "chunk-a".to_string(),
        ]
    );

    assert!(index_exists(&conn, "idx_vpw_schedule"));
}

#[test]
fn test_migration_from_v23_preserves_legacy_tables() {
    let conn = bootstrapped();
    assert!(table_exists(&conn, "vector_profiles"));
    assert!(table_exists(&conn, "projection_profiles"));
    // Legacy tables should still accept inserts after v24 is applied.
    conn.execute(
        "INSERT INTO vector_profiles (profile, table_name, dimension, enabled)
         VALUES ('legacy', 'vec_legacy', 384, 0)",
        [],
    )
    .expect("legacy vector_profiles insert");
}
