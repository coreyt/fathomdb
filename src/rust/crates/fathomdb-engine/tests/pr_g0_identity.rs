//! Slice 15 (G0) — canonical identity substrate: transaction-time supersession
//! (tombstone-then-insert), partial-unique-active NULL-safety, and the
//! `WriteReceipt.row_cursors` per-row identity carrier.
//!
//! Consumes `dev/adr/ADR-0.8.0-canonical-identity-substrate.md` (SIGNED
//! 2026-06-03) and the design memo `dev/design/slice-15-g0-design.md`.

use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::{migrate, migrate_with_steps, SCHEMA_VERSION, SQLITE_SUFFIX};
use rusqlite::Connection;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn node(kind: &str, body: &str, logical_id: Option<&str>) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body.to_string(),
        source_id: None,
        logical_id: logical_id.map(str::to_string),
    }
}

fn edge(kind: &str, from: &str, to: &str, logical_id: Option<&str>) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: kind.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: None,
        logical_id: logical_id.map(str::to_string),
    }
}

/// (a) + (d) — re-writing the same `logical_id` supersedes the prior active row
/// (tombstone-then-insert) and leaves exactly one active version, with the
/// superseded version retained (invalidate-not-delete).
#[test]
fn s15_supersession_is_idempotent_one_active_version() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "supersede");
    {
        let opened = Engine::open(&path).expect("open");
        for body in ["v1", "v2", "v3"] {
            opened.engine.write(&[node("doc", body, Some("L1"))]).expect("write");
        }
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");
    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM canonical_nodes WHERE logical_id = 'L1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 3, "invalidate-not-delete: all three versions retained");

    let active: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_nodes WHERE logical_id = 'L1' AND superseded_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active, 1, "exactly one active version after supersession");

    // The single active row is the most-recent write.
    let active_body: String = conn
        .query_row(
            "SELECT body FROM canonical_nodes WHERE logical_id = 'L1' AND superseded_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active_body, "v3");

    // Every superseded row carries a non-NULL tombstone cursor (d).
    let superseded_null_tombstone: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_nodes
             WHERE logical_id = 'L1' AND body IN ('v1','v2') AND superseded_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(superseded_null_tombstone, 0, "superseded rows must carry a tombstone cursor");
}

/// (a-edges) — supersession works identically on `canonical_edges` (Decision 3 /
/// Q4: edges carry the temporal columns, not nodes only). Re-writing the same
/// edge `logical_id` tombstones the prior active edge and leaves exactly one
/// active, keyed by `(logical_id, kind)`.
#[test]
fn s15_edge_supersession_is_idempotent_one_active_version() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "edge_supersede");
    {
        let opened = Engine::open(&path).expect("open");
        opened.engine.write(&[edge("rel", "a", "b", Some("E1"))]).expect("write v1");
        opened.engine.write(&[edge("rel", "a", "c", Some("E1"))]).expect("supersede to v2");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");
    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM canonical_edges WHERE logical_id = 'E1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 2, "both edge versions retained (invalidate-not-delete)");

    let active_to: String = conn
        .query_row(
            "SELECT to_id FROM canonical_edges WHERE logical_id = 'E1' AND superseded_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active_to, "c", "exactly one active edge, the most-recent version");
}

/// (b) — partial-unique NULL-safety: many NULL-`logical_id` (legacy / own-identity)
/// rows coexist active without colliding (SQLite treats each NULL as distinct).
#[test]
fn s15_null_logical_id_rows_never_collide() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nullsafe");
    {
        let opened = Engine::open(&path).expect("open");
        // Same kind, NULL logical_id, distinct bodies — all must remain active.
        let batch: Vec<PreparedWrite> =
            (0..8).map(|i| node("doc", &format!("legacy-{i}"), None)).collect();
        opened.engine.write(&batch).expect("write batch of NULL-logical_id rows");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");
    let active_null: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_nodes WHERE logical_id IS NULL AND superseded_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active_null, 8, "all NULL-logical_id rows must coexist active (NULL-safety)");
}

/// (c) — two active rows with the same non-NULL `(logical_id, kind)` collide: the
/// partial-unique-active index fires structurally. (The engine's
/// tombstone-then-insert never produces this; we exercise the index directly.)
#[test]
fn s15_partial_unique_active_index_rejects_two_active_versions() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "uniqueidx");
    let conn = Connection::open(&path).expect("open sqlite");
    migrate(&conn).expect("migrate to head");

    // First active row for (DUP, doc): accepted.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) VALUES(1, 'doc', 'a', 'DUP')",
        [],
    )
    .expect("first active row");

    // Second active row for the SAME (logical_id, kind): rejected by the index.
    let err = conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) VALUES(2, 'doc', 'b', 'DUP')",
        [],
    );
    assert!(err.is_err(), "two active rows for one (logical_id, kind) must be rejected");

    // But a tombstoned (superseded_at NOT NULL) row for the same key is allowed.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id, superseded_at)
         VALUES(3, 'doc', 'c', 'DUP', 2)",
        [],
    )
    .expect("a superseded row for the same key must be allowed alongside the active one");

    // A different kind for the same logical_id is a distinct active row.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) VALUES(4, 'note', 'd', 'DUP')",
        [],
    )
    .expect("a different kind is a distinct active key");
}

/// (e) — `WriteReceipt.row_cursors` is 1:1 with the batch in input order, and the
/// scalar `cursor` remains the batch high-water cursor.
#[test]
fn s15_row_cursors_are_one_to_one_with_the_batch() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rowcursors");
    let opened = Engine::open(&path).expect("open");
    let batch =
        vec![node("doc", "r0", None), node("doc", "r1", Some("L")), node("doc", "r2", None)];
    let receipt = opened.engine.write(&batch).expect("write");

    assert_eq!(receipt.row_cursors.len(), batch.len(), "one row_cursor per input row");
    // Strictly increasing, contiguous, and the last equals the scalar cursor.
    for window in receipt.row_cursors.windows(2) {
        assert_eq!(window[1], window[0] + 1, "row cursors are contiguous in input order");
    }
    assert_eq!(
        *receipt.row_cursors.last().unwrap(),
        receipt.cursor,
        "the final row_cursor is the batch high-water cursor"
    );
    opened.engine.close().unwrap();
}

/// (f) — Pack-1 / legacy existing-DB upgrade: opening a pre-step-12 DB lands the
/// new columns + indexes, old rows back-fill NULL `logical_id` and remain
/// queryable, and re-applying the migration is a no-op (idempotence).
#[test]
fn s15_legacy_pre_step12_db_upgrades_in_place_with_null_backfill() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "legacy_upgrade");
    let conn = Connection::open(&path).expect("open sqlite");

    // Build a pre-step-12 DB (everything except the G0 substrate step) and seed a
    // legacy canonical row that predates `logical_id`.
    let pre_step12: Vec<_> =
        fathomdb_schema::MIGRATIONS.iter().filter(|m| m.step_id <= 11).copied().collect();
    migrate_with_steps(&conn, &pre_step12).expect("migrate to v11");
    assert_eq!(conn.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0)).unwrap(), 11);
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body) VALUES(7, 'doc', 'legacy-body')",
        [],
    )
    .expect("seed legacy row");

    // Upgrade: step 12 lands in place, no re-open, no data migration.
    let report = migrate(&conn).expect("upgrade to v12");
    assert_eq!(report.schema_version_after, SCHEMA_VERSION);
    assert_eq!(conn.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0)).unwrap(), 12);

    // The legacy row back-fills NULL logical_id and stays queryable.
    let logical_id: Option<String> = conn
        .query_row("SELECT logical_id FROM canonical_nodes WHERE body = 'legacy-body'", [], |r| {
            r.get(0)
        })
        .expect("legacy row still queryable");
    assert!(logical_id.is_none(), "legacy row must back-fill NULL logical_id, got {logical_id:?}");

    // Re-applying the migration is a no-op (crash-safe / idempotent open path).
    let again = migrate(&conn).expect("re-migrate is a no-op");
    assert_eq!(again.schema_version_after, SCHEMA_VERSION);
    assert!(again.migration_steps.is_empty(), "no steps re-run on an up-to-date DB");
}
