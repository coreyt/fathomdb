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
        state: fathomdb_engine::InitialState::Active,
        reason: None,
    }
}

fn edge(kind: &str, from: &str, to: &str, logical_id: Option<&str>) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: kind.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: None,
        logical_id: logical_id.map(str::to_string),
        body: None,
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
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
/// active, keyed by `logical_id` alone (Decision 5, HITL-SIGNED 2026-06-05).
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

/// (c) — two active rows with the same non-NULL `logical_id` collide: the
/// partial-unique-active index fires structurally. (The engine's
/// tombstone-then-insert never produces this; we exercise the index directly.)
/// Decision 5 (HITL-SIGNED 2026-06-05): active uniqueness is scoped to
/// `logical_id` ALONE — a *different* `kind` for the same active `logical_id` is
/// NO LONGER a distinct active row; it collides too.
#[test]
fn s15_partial_unique_active_index_rejects_two_active_versions() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "uniqueidx");
    let conn = Connection::open(&path).expect("open sqlite");
    migrate(&conn).expect("migrate to head");

    // First active row for logical_id DUP: accepted.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) VALUES(1, 'doc', 'a', 'DUP')",
        [],
    )
    .expect("first active row");

    // Second active row for the SAME logical_id: rejected by the index.
    let err = conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) VALUES(2, 'doc', 'b', 'DUP')",
        [],
    );
    assert!(err.is_err(), "two active rows for one logical_id must be rejected");

    // But a tombstoned (superseded_at NOT NULL) row for the same logical_id is allowed.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id, superseded_at)
         VALUES(3, 'doc', 'c', 'DUP', 2)",
        [],
    )
    .expect("a superseded row for the same logical_id must be allowed alongside the active one");

    // Decision 5 (2026-06-05): a DIFFERENT kind for the same active logical_id is
    // now ALSO rejected — `kind` is no longer an identity-scope component, so the
    // `logical_id`-alone partial-unique-active index fires. (Inverted from the
    // pre-Slice-31 behavior, which treated it as a distinct active key.)
    let err_diff_kind = conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) VALUES(4, 'note', 'd', 'DUP')",
        [],
    );
    assert!(
        err_diff_kind.is_err(),
        "a different kind for the same active logical_id must now be rejected (logical_id-alone)"
    );
}

/// (s31-node) — a `kind`-change re-ingest of the same `logical_id` is a true
/// SUPERSESSION, not a fork (Decision 5, HITL-SIGNED 2026-06-05). Writing
/// `logical_id="L1"` first as `kind="fact"` then as `kind="note"` must leave
/// EXACTLY ONE active row (the second, `note`), with the first (`fact`) retained
/// but tombstoned. Under the pre-Slice-31 compound `(logical_id, kind)` key this
/// FAILS (two active rows) — that failure is the fork bug this slice fixes.
#[test]
fn s31_node_kind_change_reingest_supersedes() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "node_kind_change");
    {
        let opened = Engine::open(&path).expect("open");
        opened.engine.write(&[node("fact", "v1", Some("L1"))]).expect("write kind=fact");
        opened.engine.write(&[node("note", "v2", Some("L1"))]).expect("re-ingest kind=note");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");

    // Invalidate-not-delete: both versions retained.
    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM canonical_nodes WHERE logical_id = 'L1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 2, "both kind versions retained (invalidate-not-delete)");

    // EXACTLY ONE active row for L1 — the kind-change re-ingest SUPERSEDED (no fork).
    let active: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_nodes WHERE logical_id = 'L1' AND superseded_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active, 1, "a kind-change re-ingest must SUPERSEDE (one active row), not fork");

    // The single active row is the second write (kind=note, body=v2).
    let (active_kind, active_body): (String, String) = conn
        .query_row(
            "SELECT kind, body FROM canonical_nodes WHERE logical_id = 'L1' AND superseded_at IS NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(active_kind, "note", "the active row is the most-recent (kind-changed) version");
    assert_eq!(active_body, "v2");

    // The first version (kind=fact) is retained but tombstoned (superseded_at set).
    let first_tombstoned: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_nodes
             WHERE logical_id = 'L1' AND kind = 'fact' AND superseded_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(first_tombstoned, 1, "the prior (kind=fact) version must be tombstoned, not active");
}

/// (s31-edge) — identical to (s31-node) but on `canonical_edges`: a `kind`-change
/// re-ingest of the same edge `logical_id` SUPERSEDES rather than forks
/// (Decision 5). One active edge after the re-ingest; the prior is tombstoned.
#[test]
fn s31_edge_kind_change_reingest_supersedes() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "edge_kind_change");
    {
        let opened = Engine::open(&path).expect("open");
        opened.engine.write(&[edge("rel", "a", "b", Some("E1"))]).expect("write kind=rel");
        opened
            .engine
            .write(&[edge("mentions", "a", "b", Some("E1"))])
            .expect("re-ingest kind=mentions");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");

    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM canonical_edges WHERE logical_id = 'E1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 2, "both edge kind versions retained (invalidate-not-delete)");

    let active: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_edges WHERE logical_id = 'E1' AND superseded_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        active, 1,
        "an edge kind-change re-ingest must SUPERSEDE (one active row), not fork"
    );

    let active_kind: String = conn
        .query_row(
            "SELECT kind FROM canonical_edges WHERE logical_id = 'E1' AND superseded_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        active_kind, "mentions",
        "the active edge is the most-recent (kind-changed) version"
    );

    let first_tombstoned: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_edges
             WHERE logical_id = 'E1' AND kind = 'rel' AND superseded_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(first_tombstoned, 1, "the prior (kind=rel) edge must be tombstoned, not active");
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

    // Upgrade: the step-12 G0 substrate lands in place (plus any later additive
    // steps, e.g. step-13's op-store index), no re-open, no data migration.
    let report = migrate(&conn).expect("upgrade to head");
    assert_eq!(report.schema_version_after, SCHEMA_VERSION);
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0)).unwrap(),
        SCHEMA_VERSION
    );

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
