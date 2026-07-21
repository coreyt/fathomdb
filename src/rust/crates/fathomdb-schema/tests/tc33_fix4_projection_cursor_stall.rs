//! 0.8.20 Slice 15c (TC-33) fix-4 — the step-23 `canonical_edges` recreate must
//! not STRAND the shared projection cursor on a dropped pending-projection edge.
//!
//! **The finding (codex §9 P1).** `write_cursor` is a SINGLE global sequence
//! shared across nodes AND edges. `advance_projection_cursor` walks the
//! readiness watermark forward ONE value at a time, and only while a
//! `_fathomdb_projection_terminal` row exists for the next cursor. Step 23 drops
//! every `canonical_edges` row (NO DATA MIGRATION, HITL 2026-07-21). If an
//! upgraded v22 DB had a body-bearing edge whose vector projection had NOT
//! completed — so it has NO terminal row — step 23 removes that row but leaves
//! its cursor value with no terminal and no owning row. The projection cursor
//! then stalls PERMANENTLY at that gap, and because the sequence is shared this
//! freezes advancement past SURVIVING node projections too (every upgraded DB's
//! `wait_for_idle` / search-freshness wedges).
//!
//! This is projection-cursor STATE bookkeeping, not data preservation: the fix
//! records a terminal for every dropped edge cursor BEFORE the DROP, so the
//! cursor can walk past it. The edge data still does not survive.
//!
//! **Why the terminal state is `'up_to_date'`, NOT `'superseded'`.** The
//! `_fathomdb_projection_terminal` table (step 7) carries a structural
//! `CHECK(state IN ('failed', 'up_to_date'))`, and every writer uses
//! `INSERT OR IGNORE`. Under SQLite, `OR IGNORE` SKIPS a row that violates a
//! CHECK and returns NO error — so a backfill written as `'superseded'` would be
//! SILENTLY DROPPED and the cursor would still stall (a vacuous green). The only
//! CHECK-valid, non-alarming terminal is `'up_to_date'`: it means "nothing left
//! to project here", which is exactly true of a row the recreate deleted, and it
//! keeps `projection_status` out of the `'failed'` arm. `fix4_superseded_token_
//! is_rejected_by_check` pins that OR-IGNORE-swallows-CHECK behaviour so a future
//! reader cannot "simplify" the token back to `'superseded'`.

use fathomdb_schema::{migrate_with_steps, MIGRATIONS};
use rusqlite::Connection;
use std::sync::Once;

// Step 9 creates a vec0 virtual table; register sqlite-vec once per binary.
fn register_sqlite_vec_once() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| unsafe {
        let entrypoint: unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *const std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int = std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
        rusqlite::ffi::sqlite3_auto_extension(Some(entrypoint));
    });
}

fn user_version(conn: &Connection) -> u32 {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0)).unwrap()
}

fn set_user_version(conn: &Connection, version: u32) {
    conn.pragma_update(None, "user_version", version).unwrap();
}

fn steps_through(limit: u32) -> Vec<fathomdb_schema::Migration> {
    MIGRATIONS.iter().filter(|m| m.step_id <= limit).cloned().collect()
}

fn has_terminal(conn: &Connection, cursor: u64) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM _fathomdb_projection_terminal WHERE write_cursor = ?1)",
        [cursor],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        != 0
}

/// A faithful replica of the engine's `advance_projection_cursor` walk: start at
/// the stored `projection_cursor` and advance while the NEXT value has a
/// `_fathomdb_projection_terminal` row. What is under test is not this loop (it
/// is trivial) but whether step 23 leaves a terminal for the dropped edge cursor
/// so the walk can reach the projected node beyond it.
fn advance_projection_cursor(conn: &Connection) -> u64 {
    let mut cursor: u64 = conn
        .query_row(
            "SELECT value FROM _fathomdb_open_state WHERE key = 'projection_cursor'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map(|value| value.parse::<u64>().unwrap_or(0))
        .unwrap_or(0);
    loop {
        let next = cursor + 1;
        if has_terminal(conn, next) {
            cursor = next;
        } else {
            break;
        }
    }
    cursor
}

/// Build a v22 DB with a body-bearing edge at cursor 5 that has NO terminal
/// (vector projection never completed), a node at cursor 6 that DID complete
/// (terminal `'up_to_date'`), and the projection watermark parked at 4.
///
/// Edge cursor 5 and node cursor 6 are DISJOINT values of the single shared
/// `write_cursor` sequence — the shape the finding is about.
fn seed_v22_with_pending_edge() -> Connection {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, &steps_through(22)).expect("migrate to v22");
    assert_eq!(user_version(&conn), 22, "precondition: DB at v22 (pre-step-23)");

    // cursor 5 — body-bearing edge, pending projection (NO terminal row).
    conn.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, body)
             VALUES(5, 'relates_to', 'a', 'b', 'edge fact pending projection')",
        [],
    )
    .expect("seed pending edge at cursor 5");

    // cursor 6 — node whose projection DID complete.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
             VALUES(6, 'doc', 'node body', 'doc-1', 'lid-1')",
        [],
    )
    .expect("seed node at cursor 6");
    conn.execute(
        "INSERT INTO _fathomdb_projection_terminal(write_cursor, state) VALUES(6, 'up_to_date')",
        [],
    )
    .expect("mark node cursor 6 projected");

    // projection watermark parked at 4 (cursors 1..=4 already advanced).
    conn.execute(
        "INSERT INTO _fathomdb_open_state(key, value) VALUES('projection_cursor', '4')
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [],
    )
    .expect("park projection cursor at 4");

    conn
}

/// **RED keystone.** After step 23 drops the pending edge at cursor 5, the shared
/// projection cursor must be able to advance PAST 5 to the projected node at 6.
/// Against pre-fix-4 code there is no terminal for 5, so the walk stalls at 4 and
/// never reaches the surviving, fully-projected node — `wait_for_idle` wedges on
/// every upgraded DB.
#[test]
fn fix4_dropped_pending_edge_does_not_strand_the_projection_cursor() {
    let conn = seed_v22_with_pending_edge();

    // Apply step 23 (v22 -> head).
    migrate_with_steps(&conn, MIGRATIONS).expect("migrate v22 -> head (step 23)");
    assert_eq!(user_version(&conn), 23, "step 23 must have applied");

    // The dropped edge's cursor must now carry a terminal so the walk can pass it.
    assert!(
        has_terminal(&conn, 5),
        "step 23 must record a projection terminal for the dropped pending edge at \
         cursor 5 BEFORE dropping canonical_edges; without it the shared cursor stalls"
    );

    // Behavioural consequence: the cursor advances past the dropped edge to the
    // projected node at 6 (RED pre-fix: stalls at 4).
    let reached = advance_projection_cursor(&conn);
    assert_eq!(
        reached, 6,
        "the shared projection cursor must advance past the dropped edge (5) to the \
         projected node (6); a stall at {reached} freezes search-freshness on every \
         upgraded DB"
    );
}

/// The edge/node cursors are DISJOINT values of the single shared sequence, so
/// backfilling terminals for edge cursors never fabricates one for a node cursor
/// that has not actually projected. Here only the edge cursor 5 is backfilled;
/// node cursor 6's terminal is the one it already had.
#[test]
fn fix4_backfill_touches_only_edge_cursors_not_node_cursors() {
    let conn = seed_v22_with_pending_edge();

    // A SECOND node at cursor 7 that has NOT projected (no terminal) — the
    // backfill must NOT invent a terminal for it.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
             VALUES(7, 'doc', 'unprojected node', 'doc-2', 'lid-2')",
        [],
    )
    .unwrap();

    migrate_with_steps(&conn, MIGRATIONS).expect("migrate v22 -> head (step 23)");

    assert!(has_terminal(&conn, 5), "edge cursor 5 must be backfilled");
    assert!(
        !has_terminal(&conn, 7),
        "node cursor 7 never projected and is NOT an edge cursor — the edge backfill \
         must not fabricate a terminal for it (that would mark it prematurely ready)"
    );
}

/// Evidence for the token choice: `'superseded'` (the token the engine uses on
/// the supersession path) is NOT in the step-7 CHECK set, and every writer uses
/// `INSERT OR IGNORE`, which under SQLite SKIPS a CHECK-violating row and returns
/// no error. So a `'superseded'` backfill would be silently dropped, leaving the
/// cursor stranded. `'up_to_date'` is the CHECK-valid, non-`'failed'` token the
/// fix must use.
#[test]
fn fix4_superseded_token_is_rejected_by_check_so_or_ignore_drops_it() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, MIGRATIONS).expect("migrate to head");

    // OR IGNORE + CHECK violation => row skipped, NO error, NO row.
    conn.execute(
        "INSERT OR IGNORE INTO _fathomdb_projection_terminal(write_cursor, state)
             VALUES(99, 'superseded')",
        [],
    )
    .expect("OR IGNORE must not error even though the CHECK rejects 'superseded'");
    assert!(
        !has_terminal(&conn, 99),
        "'superseded' violates CHECK(state IN ('failed','up_to_date')); OR IGNORE drops \
         it silently, so it can NOT be used to advance the cursor"
    );

    // 'up_to_date' is CHECK-valid and actually lands.
    conn.execute(
        "INSERT OR IGNORE INTO _fathomdb_projection_terminal(write_cursor, state)
             VALUES(100, 'up_to_date')",
        [],
    )
    .unwrap();
    assert!(has_terminal(&conn, 100), "'up_to_date' is CHECK-valid and must persist");
}
