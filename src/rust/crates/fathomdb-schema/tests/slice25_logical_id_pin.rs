//! 0.8.20 Slice 25 (R-20-SUR) — the TC-11 pin's ENFORCING invariant, in both
//! its static and dynamic form.
//!
//! **The requirement.** Surrogate minting serves ONLY registry-admitted governed
//! entities, and admission is decided at WRITE TIME. Its acceptance signal is a
//! migration-guard: rows transitioning `logical_id` NULL → NOT NULL must be
//! **exactly 0**.
//!
//! **The ruling being enforced** (TC-11 pin A, HITL-ratified 2026-07-12; plan
//! `dev/plans/plan-0.8.20.md` §2.1) — do not re-open:
//!
//! - Anonymous / doc-seeded nodes stay `h:<content-hash>` PERMANENTLY. No
//!   backfill, no forward-mint, no split. The anonymous-surrogate leg is
//!   CANCELLED, not deferred.
//! - Enforcement is "**no new column**". The record IS `canonical_nodes.logical_id`'s
//!   null-ness, so the invariant is a PROHIBITION: *no migration, backfill, or
//!   verb shall ever populate `logical_id` on an existing canonical row.*
//! - A stored row's id-space is NEVER re-derived. Supplying a `logical_id` at
//!   write time is what makes a record governed.
//!
//! This file carries two of the slice's three deliverables:
//!
//! - **D1 — static migration guard.** [`check_migration_logical_id_pin`] is the
//!   sibling of [`check_migration_accretion`]: it statically rejects any
//!   migration step whose SQL could populate `logical_id` on an existing
//!   canonical row. The synthetic-offender tests are the RED witness; the
//!   loop over every shipped step (1..=`SCHEMA_VERSION`) is both the regression
//!   proof that the shipped ladder is clean AND the wiring that makes a FUTURE
//!   violating migration impossible to land (a new step must enter `MIGRATIONS`
//!   to have any effect, and this test walks `MIGRATIONS` whole).
//! - **D2 — dynamic migration guard.** Real SQLite, no mocking (`AGENTS.md`):
//!   build a DB at a pre-head schema holding BOTH anonymous (`logical_id IS
//!   NULL`) and governed (`logical_id NOT NULL`) canonical rows, migrate to
//!   head, and assert 0 NULL → NOT NULL transitions, 0 NOT NULL → NULL
//!   transitions, no value change at all, and byte-identical derived
//!   `IdSpace::to_prefixed()` for every pre-existing row.
//!
//! D3 ("registering a kind does not alter any pre-existing row's `IdSpace`")
//! is engine-level and lives in
//! `fathomdb-engine/tests/slice25_registration_identity_inert.rs`.

use fathomdb_schema::{
    check_migration_accretion, check_migration_logical_id_pin, migrate_with_steps,
    MigrationLogicalIdPinError, MIGRATIONS, SCHEMA_VERSION,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::sync::Once;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Step 9 creates a vec0 virtual table; register sqlite-vec once per binary.
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

/// One canonical row's identity-relevant state, as it is at rest.
///
/// `(write_cursor, logical_id, body)` is the COMPLETE input to the id-space
/// split: `derive_stable_id` reads `logical_id` (null-ness + value) and falls
/// back to `sha256(body)`. Snapshotting the triple therefore pins both the
/// pin's invariant and the derived hit id.
type IdentityRow = (i64, Option<String>, String);

/// The engine's `derive_stable_id` / `IdSpace::to_prefixed()` rule, restated
/// here because `IdSpace` lives in `fathomdb-engine` and the schema crate is a
/// LEAF (engine → schema; the reverse would be a cycle). Kept byte-identical to
/// `fathomdb-engine::derive_stable_id`: `Some(non-empty)` → `l:<logical_id>`,
/// otherwise → `h:<sha256(body)>`. The engine-side D3 test asserts the same
/// property against the real `IdSpace` type, so the two are cross-checked.
fn prefixed_id(logical_id: Option<&str>, body: &str) -> String {
    match logical_id {
        Some(lid) if !lid.is_empty() => format!("l:{lid}"),
        _ => {
            let mut hasher = Sha256::new();
            hasher.update(body.as_bytes());
            let hex: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
            format!("h:{hex}")
        }
    }
}

fn snapshot_nodes(conn: &Connection) -> Vec<IdentityRow> {
    conn.prepare("SELECT write_cursor, logical_id, body FROM canonical_nodes ORDER BY write_cursor")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn snapshot_edges(conn: &Connection) -> Vec<IdentityRow> {
    conn.prepare(
        "SELECT write_cursor, logical_id, COALESCE(body, '') FROM canonical_edges \
         ORDER BY write_cursor",
    )
    .unwrap()
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

/// The acceptance signal itself, as a function: how many rows present in BOTH
/// snapshots went `logical_id` NULL → NOT NULL. Keyed on `write_cursor`, which
/// migrations never rewrite.
fn null_to_not_null(before: &[IdentityRow], after: &[IdentityRow]) -> usize {
    before
        .iter()
        .filter(|(cursor, lid, _)| {
            lid.is_none()
                && after
                    .iter()
                    .any(|(c, l, _)| c == cursor && l.as_ref().is_some_and(|v| !v.is_empty()))
        })
        .count()
}

/// The mirror direction — a governed row silently DE-governed is just as much a
/// re-derivation of a stored row's id-space.
fn not_null_to_null(before: &[IdentityRow], after: &[IdentityRow]) -> usize {
    before
        .iter()
        .filter(|(cursor, lid, _)| {
            lid.is_some() && after.iter().any(|(c, l, _)| c == cursor && l.is_none())
        })
        .count()
}

// ===========================================================================
// D1 — static migration guard
// ===========================================================================

/// **RED witness (synthetic offender, `UPDATE … SET logical_id`).** The
/// canonical shape of a forward-mint backfill: a migration that stamps a
/// surrogate onto every anonymous row. This is exactly what the TC-11 pin
/// forbids, and the guard must reject it statically — before it can ever run.
#[test]
fn pin_rejects_update_that_mints_logical_id_on_anonymous_nodes() {
    let offender = "UPDATE canonical_nodes \
                       SET logical_id = 'l:' || lower(hex(randomblob(16))) \
                     WHERE logical_id IS NULL;";

    let err = check_migration_logical_id_pin("099_surrogate_backfill.sql", offender)
        .expect_err("a migration that populates logical_id must be rejected");
    assert_eq!(err.offender, "099_surrogate_backfill.sql");
    assert!(
        err.statement.contains("LOGICAL_ID"),
        "the rejection must name the offending statement, got {:?}",
        err.statement
    );
}

/// The same offence on the edge table. Edges carry `logical_id` too (a
/// supersession identity), and the pin covers BOTH canonical tables.
#[test]
fn pin_rejects_update_that_mints_logical_id_on_edges() {
    let offender = "UPDATE canonical_edges SET logical_id = from_id || ':' || to_id \
                    WHERE logical_id IS NULL;";
    check_migration_logical_id_pin("099_edge_backfill.sql", offender)
        .expect_err("a migration that populates canonical_edges.logical_id must be rejected");
}

/// **RED witness (synthetic offender, `INSERT … SELECT` backfill).** The other
/// route to the same outcome: recreate-and-copy, computing a surrogate on the
/// way through. Naming `logical_id` in the insert column list of a canonical
/// table is refused outright.
#[test]
fn pin_rejects_insert_select_backfill_that_writes_logical_id() {
    let offender = "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) \
                        SELECT write_cursor, kind, body, \
                               COALESCE(logical_id, 'minted:' || write_cursor) \
                          FROM canonical_nodes_old;";
    check_migration_logical_id_pin("099_recreate_copy.sql", offender)
        .expect_err("an INSERT…SELECT backfill that writes logical_id must be rejected");
}

/// A column-list-less `INSERT INTO canonical_nodes SELECT …` writes EVERY
/// column, `logical_id` among them, without ever naming it. The guard refuses
/// the shape rather than trying to count columns.
#[test]
fn pin_rejects_insert_into_canonical_table_without_a_column_list() {
    let offender = "INSERT INTO canonical_nodes SELECT * FROM canonical_nodes_old;";
    check_migration_logical_id_pin("099_blind_copy.sql", offender)
        .expect_err("an INSERT with no column list writes logical_id implicitly");
}

/// Quoting is not an escape. `"logical_id"` / `[logical_id]` / `` `logical_id` ``
/// are the same identifier to SQLite, so they are the same identifier to the
/// guard.
#[test]
fn pin_rejects_quoted_identifier_evasion() {
    for offender in [
        r#"UPDATE canonical_nodes SET "logical_id" = 'x' WHERE 1;"#,
        "UPDATE canonical_nodes SET [logical_id] = 'x' WHERE 1;",
        "UPDATE canonical_nodes SET `logical_id` = 'x' WHERE 1;",
    ] {
        check_migration_logical_id_pin("099_quoted.sql", offender)
            .unwrap_err_or_panic("quoted-identifier evasion must still be rejected", offender);
    }
}

/// **Schema-qualifying the table is not an escape either.** SQLite resolves
/// `main.canonical_nodes` to the SAME pinned table as the bare name, so an
/// author who writes the qualified spelling has performed the exact backfill the
/// pin exists to reject. Both canonical tables, both write shapes.
///
/// (Reviewer finding, codex §9 [P2] on Slice 25: the guard compared the extracted
/// token LITERALLY against `CANONICAL_NODES` / `CANONICAL_EDGES`, so the token
/// `MAIN.CANONICAL_NODES` silently ACCEPTED the forbidden shape.)
#[test]
fn pin_rejects_schema_qualified_backfills() {
    for offender in [
        "UPDATE main.canonical_nodes SET logical_id = 'minted' WHERE logical_id IS NULL;",
        "UPDATE main.canonical_edges SET logical_id = from_id || ':' || to_id WHERE 1;",
        "INSERT INTO main.canonical_nodes(write_cursor, logical_id) \
             SELECT write_cursor, COALESCE(logical_id, 'minted') FROM canonical_nodes_old;",
        "INSERT INTO main.canonical_edges(write_cursor, logical_id) \
             SELECT write_cursor, 'minted' FROM canonical_edges_old;",
        // No column list: writes every column, identity included.
        "INSERT INTO main.canonical_nodes SELECT * FROM canonical_nodes_old;",
    ] {
        check_migration_logical_id_pin("099_qualified.sql", offender)
            .unwrap_err_or_panic("a schema-qualified backfill must still be rejected", offender);
    }
}

/// The qualifier is normalised the same way the TABLE name already is: EITHER
/// half may be quoted (`"main".canonical_nodes`, `[main].[canonical_nodes]`,
/// `` `main`.`canonical_nodes` ``), and `temp.` qualifies just as `main.` does.
/// SQLite treats all of these as the one pinned table; so must the guard.
#[test]
fn pin_rejects_quoted_and_temp_schema_qualified_evasion() {
    for offender in [
        r#"UPDATE "main".canonical_nodes SET logical_id = 'x' WHERE 1;"#,
        r#"UPDATE main."canonical_nodes" SET logical_id = 'x' WHERE 1;"#,
        r#"UPDATE "main"."canonical_nodes" SET logical_id = 'x' WHERE 1;"#,
        "UPDATE [main].[canonical_nodes] SET logical_id = 'x' WHERE 1;",
        "UPDATE `main`.`canonical_nodes` SET logical_id = 'x' WHERE 1;",
        "UPDATE temp.canonical_nodes SET logical_id = 'x' WHERE 1;",
        "INSERT INTO [temp].[canonical_edges](write_cursor, logical_id) VALUES(1, 'x');",
    ] {
        check_migration_logical_id_pin("099_qualified_quoted.sql", offender).unwrap_err_or_panic(
            "a quoted / temp-qualified backfill must still be rejected",
            offender,
        );
    }
}

/// SQLite's tokenizer accepts whitespace around the qualifier dot —
/// `UPDATE main . canonical_nodes SET …` parses and targets the same table
/// (verified against real SQLite 3.45 before this assertion was written). The
/// guard's token extractors cut at whitespace, so the spaced spelling must be
/// tightened during normalisation or it slips the pin as the bare token `MAIN`.
#[test]
fn pin_rejects_whitespace_around_the_qualifier_dot() {
    for offender in [
        "UPDATE main . canonical_nodes SET logical_id = 'x' WHERE 1;",
        "UPDATE main. canonical_edges SET logical_id = 'x' WHERE 1;",
        "INSERT INTO main .canonical_nodes(write_cursor, logical_id) VALUES(1, 'x');",
    ] {
        check_migration_logical_id_pin("099_spaced_dot.sql", offender).unwrap_err_or_panic(
            "whitespace around the qualifier dot must not defeat the pin",
            offender,
        );
    }
}

/// The DDL arms take the table name through the same extractor, so they carry
/// the same hole: a qualified `ALTER TABLE` / `CREATE TABLE` must be judged on
/// the table it actually targets.
#[test]
fn pin_rejects_schema_qualified_ddl() {
    for offender in [
        "ALTER TABLE main.canonical_nodes ADD COLUMN logical_id TEXT NOT NULL DEFAULT 'minted';",
        "ALTER TABLE main.canonical_nodes RENAME COLUMN kind TO logical_id;",
        "CREATE TABLE main.canonical_edges(write_cursor INTEGER, logical_id TEXT DEFAULT 'x');",
    ] {
        check_migration_logical_id_pin("099_qualified_ddl.sql", offender).unwrap_err_or_panic(
            "a schema-qualified DDL offender must still be rejected",
            offender,
        );
    }
}

/// **Control — stripping the qualifier must not degrade into a substring match.**
/// `main.canonical_nodes_old` is a DIFFERENT table and is none of the pin's
/// business; if the fix widened the comparison to "contains", these would start
/// failing and the guard would reject legitimate recreate scaffolding.
#[test]
fn pin_still_ignores_qualified_non_canonical_tables() {
    for accepted in [
        "UPDATE main.canonical_nodes_old SET logical_id = 'x' WHERE 1;",
        "UPDATE main.node_projections SET logical_id = 'x' WHERE 1;",
        "INSERT INTO temp.canonical_nodes_backup(write_cursor, logical_id) VALUES(1, 'x');",
    ] {
        check_migration_logical_id_pin("098_unrelated.sql", accepted).unwrap_or_else(|err| {
            panic!("an unrelated table is none of the pin's business: {err}")
        });
    }
}

/// **SQLite's `UPDATE OR <conflict-action>` prefix is not an escape.** All five
/// actions (`ROLLBACK`, `ABORT`, `FAIL`, `IGNORE`, `REPLACE`) sit BETWEEN the
/// `UPDATE` keyword and the table name, so a naive "first token after `UPDATE`"
/// extractor reads the table as `OR` and lets the backfill through. Verified
/// against real SQLite 3.45.1 before this assertion was written: every spelling
/// below parses and targets `canonical_nodes` / `canonical_edges`.
///
/// (Reviewer finding, codex §9 [P2] on Slice 25 fix-1.)
#[test]
fn pin_rejects_update_with_an_or_conflict_clause() {
    for action in ["ROLLBACK", "ABORT", "FAIL", "IGNORE", "REPLACE"] {
        for offender in [
            format!("UPDATE OR {action} canonical_nodes SET logical_id = 'minted' WHERE 1;"),
            format!("UPDATE OR {action} canonical_edges SET logical_id = 'minted' WHERE 1;"),
            // …and composed with the fix-1 schema qualifier, in every spelling.
            format!("UPDATE OR {action} main.canonical_nodes SET logical_id = 'minted';"),
            format!("UPDATE OR {action} temp . canonical_edges SET logical_id = 'minted';"),
            format!(r#"UPDATE OR {action} "main"."canonical_nodes" SET logical_id = 'x';"#),
        ] {
            check_migration_logical_id_pin("099_or_conflict.sql", &offender).unwrap_err_or_panic(
                "an UPDATE OR <conflict-action> backfill must still be rejected",
                &offender,
            );
        }
    }
    // Lowercase is the same statement to SQLite, so it is to the guard.
    let lowered = "update or replace main.canonical_nodes set logical_id = 'x';";
    check_migration_logical_id_pin("099_or_lower.sql", lowered)
        .unwrap_err_or_panic("case is not an escape", lowered);
}

/// The analogous `INSERT OR <conflict-action> INTO …` shape — the same finding
/// family. The insert arm anchors on ` INTO `, which the conflict clause
/// precedes, so this is a REGRESSION pin rather than a hole: it must stay
/// rejected whatever the extractor does next.
#[test]
fn pin_rejects_insert_with_an_or_conflict_clause() {
    for action in ["ROLLBACK", "ABORT", "FAIL", "IGNORE", "REPLACE"] {
        for offender in [
            format!(
                "INSERT OR {action} INTO canonical_nodes(write_cursor, logical_id) VALUES(1,'x');"
            ),
            format!("INSERT OR {action} INTO main.canonical_edges(logical_id) VALUES('x');"),
            // No column list: writes every column, identity included.
            format!("INSERT OR {action} INTO canonical_nodes SELECT * FROM canonical_nodes_old;"),
        ] {
            check_migration_logical_id_pin("099_insert_or.sql", &offender).unwrap_err_or_panic(
                "an INSERT OR <conflict-action> backfill must still be rejected",
                &offender,
            );
        }
    }
}

/// **Control — the conflict-clause skip must not widen into a substring match.**
/// A lookalike table is none of the pin's business, conflict clause or not.
#[test]
fn pin_still_ignores_or_conflict_updates_on_unrelated_tables() {
    for accepted in [
        "UPDATE OR REPLACE canonical_nodes_backup SET logical_id = 'x' WHERE 1;",
        "UPDATE OR IGNORE main.canonical_edges_old SET logical_id = 'x';",
        // `or_ledger` is a table whose name merely STARTS with the OR keyword's
        // letters; skipping the clause must be token-exact.
        "UPDATE or_ledger SET logical_id = 'x';",
    ] {
        check_migration_logical_id_pin("098_unrelated_or.sql", accepted).unwrap_or_else(|err| {
            panic!("an unrelated table is none of the pin's business: {accepted} → {err}")
        });
    }
}

/// **A leading `WITH …` CTE is not an escape.** SQLite's grammar puts an
/// optional `with-clause` in FRONT of `UPDATE`, `INSERT` and `DELETE`, so
/// `WITH x AS (SELECT 1) UPDATE canonical_nodes SET logical_id = 'y'` is a
/// running backfill whose statement does not START with `UPDATE` — and every
/// arm of the guard anchored on the leading keyword. Verified against real
/// SQLite 3.45.1 before these assertions were written: every shape below parses
/// AND executes against the canonical tables.
///
/// (Self-reported by Slice 25 fix-2; same finding family as the two codex §9
/// [P2]s it follows.)
#[test]
fn pin_rejects_a_cte_prefixed_backfill() {
    for offender in [
        // The reported shape, both canonical tables.
        "WITH x AS (SELECT 1) UPDATE canonical_nodes SET logical_id = 'y';",
        "WITH x AS (SELECT 1) UPDATE canonical_edges SET logical_id = 'y';",
        // …composed with the fix-1 schema qualifier, spaced dot included.
        "WITH x AS (SELECT 1) UPDATE main.canonical_nodes SET logical_id = 'y';",
        "WITH x AS (SELECT 1) UPDATE temp . canonical_edges SET logical_id = 'y';",
        r#"WITH x AS (SELECT 1) UPDATE "main"."canonical_nodes" SET [logical_id] = 'y';"#,
        // …and with the fix-2 conflict clause, so all three fixes compose.
        "WITH x AS (SELECT 1) UPDATE OR REPLACE canonical_nodes SET logical_id = 'y';",
        "WITH x AS (SELECT 1) UPDATE OR IGNORE main.canonical_edges SET logical_id = 'y';",
        "WITH x AS (SELECT 1) UPDATE OR ABORT temp.canonical_edges SET logical_id = 'y';",
        // A recursive CTE is the same clause with one more keyword.
        "WITH RECURSIVE x(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM x WHERE n < 3) \
             UPDATE canonical_nodes SET logical_id = 'y' \
             WHERE write_cursor IN (SELECT n FROM x);",
        // The INSERT … SELECT backfill route, CTE-prefixed.
        "WITH src AS (SELECT write_cursor, body FROM canonical_nodes_old) \
             INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) \
             SELECT write_cursor, 'doc', body, 'minted' FROM src;",
        // …and the column-list-less form, which writes identity implicitly.
        "WITH src AS (SELECT * FROM canonical_nodes_old) \
             INSERT INTO canonical_nodes SELECT * FROM src;",
        "WITH x AS (SELECT 1) INSERT INTO main.canonical_edges\
             (write_cursor, kind, from_id, to_id, logical_id) VALUES(9,'k','a','b','y');",
        // Leading whitespace / newlines are not an escape…
        "\n   WITH x AS (SELECT 1)\n   UPDATE canonical_nodes SET logical_id = 'y';",
        // …nor is case.
        "with x as (select 1) update canonical_nodes set logical_id = 'y';",
    ] {
        check_migration_logical_id_pin("099_cte_backfill.sql", offender)
            .unwrap_err_or_panic("a CTE-prefixed backfill must be rejected", offender);
    }
}

/// **Control — the CTE catch-all must not conscript unrelated statements.**
/// A lookalike target table is still none of the pin's business, and READING
/// `logical_id` off a canonical table inside a CTE while writing something else
/// entirely is not a backfill. Both shapes verified valid against SQLite 3.45.1.
#[test]
fn pin_still_accepts_cte_statements_that_write_no_canonical_identity() {
    for accepted in [
        "WITH x AS (SELECT 1) UPDATE canonical_nodes_backup SET logical_id = 'y';",
        "WITH x AS (SELECT logical_id FROM canonical_nodes) UPDATE other_t SET note = 'n';",
    ] {
        check_migration_logical_id_pin("098_cte_unrelated.sql", accepted).unwrap_or_else(|err| {
            panic!("a CTE that writes no canonical logical_id must be accepted: {accepted} → {err}")
        });
    }
}

/// **The trigger arm must identify its target by TOKEN, not by substring.**
/// `canonical_nodes_backup` is a different table; a trigger that stamps
/// `logical_id` on the BACKUP is legitimate recreate/backup scaffolding and the
/// pin has no business rejecting it. (Reviewer finding, codex §9 [P3] on Slice 25
/// fix-1: `statement.contains(table)` conscripted every lookalike name.)
#[test]
fn pin_accepts_a_trigger_on_a_lookalike_table() {
    for accepted in [
        "CREATE TRIGGER backup_stamp AFTER INSERT ON canonical_nodes_backup BEGIN \
             UPDATE canonical_nodes_backup SET logical_id = NEW.logical_id \
             WHERE write_cursor = NEW.write_cursor; END;",
        "CREATE TRIGGER edge_backup_stamp AFTER INSERT ON main.canonical_edges_old BEGIN \
             UPDATE canonical_edges_old SET logical_id = 'x' WHERE 1; END;",
    ] {
        check_migration_logical_id_pin("098_backup_trigger.sql", accepted).unwrap_or_else(|err| {
            panic!("a trigger on a NON-canonical lookalike table must be accepted: {err}")
        });
    }
}

/// **Control — precision on the trigger TARGET must not cost the trigger BODY.**
/// A trigger declared on a lookalike table whose body writes `logical_id` on a
/// REAL canonical table is the same deferred backfill and must stay rejected,
/// whether the offending statement is the first in the body (glued to the
/// `CREATE TRIGGER` head by the `;` split) or a later one (its own fragment).
#[test]
fn pin_rejects_a_lookalike_trigger_whose_body_writes_a_canonical_logical_id() {
    for offender in [
        // First body statement — same fragment as the head.
        "CREATE TRIGGER sneaky AFTER INSERT ON canonical_nodes_backup BEGIN \
             UPDATE canonical_nodes SET logical_id = 'minted' \
             WHERE write_cursor = NEW.write_cursor; END;",
        // Later body statement — its own fragment.
        "CREATE TRIGGER sneakier AFTER INSERT ON canonical_nodes_backup BEGIN \
             INSERT INTO audit_log(note) VALUES('backup'); \
             UPDATE canonical_nodes SET logical_id = 'minted'; END;",
        // …and through the fix-1 qualifier.
        "CREATE TRIGGER sneakiest AFTER INSERT ON canonical_nodes_backup BEGIN \
             UPDATE main.canonical_edges SET logical_id = 'minted'; END;",
    ] {
        check_migration_logical_id_pin("099_sneaky_trigger.sql", offender).unwrap_err_or_panic(
            "a trigger BODY that writes a canonical logical_id must still be rejected",
            offender,
        );
    }
}

/// A trigger is a deferred `UPDATE`. One that writes `logical_id` on a canonical
/// table is the same offence, executed later.
#[test]
fn pin_rejects_a_trigger_that_writes_logical_id() {
    let offender = "CREATE TRIGGER mint_ids AFTER INSERT ON canonical_nodes BEGIN \
                        UPDATE canonical_nodes SET logical_id = 'minted' \
                        WHERE write_cursor = NEW.write_cursor; END;";
    check_migration_logical_id_pin("099_trigger.sql", offender)
        .expect_err("a trigger that writes logical_id must be rejected");
}

/// `ADD COLUMN … DEFAULT` populates every EXISTING row in one statement — the
/// quietest possible forward-mint.
#[test]
fn pin_rejects_add_column_logical_id_with_a_default() {
    let offender =
        "ALTER TABLE canonical_nodes ADD COLUMN logical_id TEXT NOT NULL DEFAULT 'minted';";
    check_migration_logical_id_pin("099_defaulted.sql", offender)
        .expect_err("a defaulted logical_id column populates every existing row");
}

/// A RENAME can turn an already-populated column INTO `logical_id` without a
/// single write.
#[test]
fn pin_rejects_renaming_a_populated_column_to_logical_id() {
    let offender = "ALTER TABLE canonical_nodes RENAME COLUMN kind TO logical_id;";
    check_migration_logical_id_pin("099_rename.sql", offender)
        .expect_err("renaming a populated column to logical_id is a backfill in disguise");
}

/// **The pin is TERMINAL-FOREVER, so it has NO exemption escape hatch.**
/// `check_migration_accretion` takes a `-- MIGRATION-ACCRETION-EXEMPTION: `
/// marker because accretion is a budget an author may deliberately spend. The
/// TC-11 pin is not a budget: an escape hatch would defeat it, so a marked
/// offender is rejected identically to an unmarked one.
#[test]
fn pin_has_no_exemption_escape_hatch() {
    let marked = "-- MIGRATION-ACCRETION-EXEMPTION: totally legitimate, honest\n\
                  UPDATE canonical_nodes SET logical_id = 'minted' WHERE logical_id IS NULL;";
    check_migration_logical_id_pin("099_marked_offender.sql", marked)
        .expect_err("no marker may exempt a migration from the TC-11 pin");

    // …and the accretion guard's own marker text is not special-cased anywhere
    // in the pin: a comment can never make an offending statement legal.
    let commented = "-- we promise this UPDATE canonical_nodes SET logical_id is fine\n\
                     SELECT 1;";
    check_migration_logical_id_pin("099_only_a_comment.sql", commented)
        .expect("a mention inside a COMMENT executes nothing and must be accepted");
}

/// **Regression proof + the wiring.** Every shipped step 1..=`SCHEMA_VERSION`
/// passes the pin. This is the future-migration blocker: `MIGRATIONS` is the
/// only functional migration surface (the `migrations/*.sql` files are
/// documentation duplicates and are not `include_str!`'d), so a new step MUST
/// appear here to have any effect — and this test walks the registry whole.
#[test]
fn pin_accepts_every_shipped_migration_step() {
    assert_eq!(
        MIGRATIONS.len(),
        SCHEMA_VERSION as usize,
        "the registry must cover every step up to head"
    );
    for step in MIGRATIONS {
        check_migration_logical_id_pin(&format!("step-{}", step.step_id), step.sql).unwrap_or_else(
            |err| panic!("shipped migration step {} violates the TC-11 pin: {err}", step.step_id),
        );
    }
}

/// **The second enforcement surface.** `check_migration_accretion` is enforced
/// in TWO places: these Rust tests, and `scripts/agent-lint-migrations.py`
/// (wired into `scripts/agent-lint.sh`) over the `migrations/*.sql` authoring
/// copies. The pin covers both surfaces WITHOUT a second implementation: this
/// test runs the SAME Rust guard over the on-disk `.sql` files. Mirroring the
/// guard's ~150-line lexical scanner into Python would create a second source of
/// truth that can drift; a shared guard cannot.
///
/// (Those `.sql` files are documentation duplicates — they are NOT `include_str!`'d,
/// so `MIGRATIONS` above is the only surface that can actually run. They are
/// checked anyway so an author who edits the copy first still meets the pin.)
#[test]
fn pin_accepts_every_on_disk_migration_sql_file() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut checked = 0usize;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("migrations dir must be readable at {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    entries.sort();
    for path in entries {
        let sql = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        check_migration_logical_id_pin(&name, &sql)
            .unwrap_or_else(|err| panic!("{name} violates the TC-11 pin: {err}"));
        checked += 1;
    }
    assert!(checked > 0, "the on-disk migration corpus must not be silently empty");
}

/// The three LEGITIMATE `logical_id` uses that already ship, pinned one by one
/// so a future tightening of the guard cannot break the ladder silently:
///
/// - step 12 DECLARES the column (`ADD COLUMN logical_id TEXT`, no default);
/// - step 12/23 INDEX it (`… ON canonical_nodes(logical_id) WHERE …`);
/// - step 21 READS it as a PREDICATE (`WHERE … AND logical_id IS NULL`) while
///   writing only `source_id`;
/// - step 23 re-DECLARES it in a `CREATE TABLE` recreate (declaration ≠
///   population — the recreate deliberately copies no rows).
#[test]
fn pin_accepts_declaration_index_and_predicate_uses() {
    let step = |id: u32| MIGRATIONS.iter().find(|m| m.step_id == id).expect("step must exist");

    // Declaration + index (step 12, the G0 identity substrate).
    let s12 = step(12);
    assert!(s12.sql.contains("ADD COLUMN logical_id TEXT"), "step 12 declares the column");
    check_migration_logical_id_pin("012_g0_substrate.sql", s12.sql)
        .expect("declaring + indexing logical_id is not populating it");

    // Predicate-only read (step 21, the legacy-provenance backfill).
    let s21 = step(21);
    assert!(s21.sql.contains("logical_id IS NULL"), "step 21 reads logical_id as a predicate");
    assert!(s21.sql.contains("SET source_id"), "step 21 writes only source_id");
    check_migration_logical_id_pin("021_legacy_provenance_backfill.sql", s21.sql)
        .expect("reading logical_id in a WHERE clause is not populating it");

    // CREATE TABLE re-declaration (step 23, the TC-33 edge recreate).
    let s23 = step(23);
    assert!(s23.sql.contains("CREATE TABLE canonical_edges"), "step 23 recreates the edge table");
    check_migration_logical_id_pin("023_tc33_edge_recreate.sql", s23.sql)
        .expect("re-declaring the column in a recreate is not populating it");
}

/// The two guards are INDEPENDENT and compose: a step can pass one and fail the
/// other. Pinned so a future refactor cannot quietly fold the pin into the
/// accretion budget (which has an exemption marker — see
/// `pin_has_no_exemption_escape_hatch`).
#[test]
fn pin_and_accretion_guard_are_independent() {
    // Passes accretion (pure data statement, no CREATE TABLE / ADD COLUMN),
    // fails the pin.
    let mints = "UPDATE canonical_nodes SET logical_id = 'x' WHERE logical_id IS NULL;";
    check_migration_accretion("099.sql", mints).expect("a pure UPDATE passes the accretion guard");
    check_migration_logical_id_pin("099.sql", mints).expect_err("…but not the TC-11 pin");

    // Fails accretion (additive, unmarked), passes the pin (touches no
    // canonical identity column).
    let accretes = "ALTER TABLE canonical_nodes ADD COLUMN nickname TEXT;";
    check_migration_accretion("098.sql", accretes).expect_err("unmarked accretion is rejected");
    check_migration_logical_id_pin("098.sql", accretes)
        .expect("an unrelated column is none of the pin's business");
}

// ===========================================================================
// D2 — dynamic migration guard (real SQLite, no mocking)
// ===========================================================================

/// **The acceptance signal, end to end.** Build a DB at step 12 — the exact
/// moment `logical_id` first exists — holding BOTH anonymous (`logical_id IS
/// NULL`, the doc-seeded class) and governed (`logical_id NOT NULL`) node rows.
/// Migrate the whole remaining ladder to head. Assert:
///
/// 1. rows transitioning `logical_id` NULL → NOT NULL == **0** (the pin);
/// 2. rows transitioning NOT NULL → NULL == 0 (the mirror);
/// 3. no `logical_id` changed AT ALL, and no `body` changed;
/// 4. every pre-existing row's derived `IdSpace::to_prefixed()` is
///    BYTE-IDENTICAL before and after — the anonymous rows are still `h:`, the
///    governed rows still `l:`.
#[test]
fn migrating_to_head_never_populates_logical_id_on_an_existing_node() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, &steps_through(12)).expect("migrate to step 12");
    assert_eq!(user_version(&conn), 12);

    // Anonymous / doc-seeded rows: logical_id IS NULL, permanently `h:`.
    for (cursor, body) in [(1i64, "the quick brown fox"), (2, "a doc-seeded chunk"), (3, "")] {
        conn.execute(
            "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) \
             VALUES(?1, 'doc', ?2, NULL)",
            rusqlite::params![cursor, body],
        )
        .unwrap();
    }
    // Governed rows: a caller supplied a logical_id at WRITE time, permanently `l:`.
    for (cursor, lid, body) in [(4i64, "ent-alice", "Alice"), (5, "ent-bob", "Bob")] {
        conn.execute(
            "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) \
             VALUES(?1, 'person', ?3, ?2)",
            rusqlite::params![cursor, lid, body],
        )
        .unwrap();
    }

    let before = snapshot_nodes(&conn);
    assert_eq!(before.len(), 5);
    let ids_before: Vec<String> =
        before.iter().map(|(_, lid, body)| prefixed_id(lid.as_deref(), body)).collect();
    assert_eq!(
        ids_before.iter().filter(|id| id.starts_with("h:")).count(),
        3,
        "three anonymous rows start in the content id-space"
    );
    assert_eq!(
        ids_before.iter().filter(|id| id.starts_with("l:")).count(),
        2,
        "two governed rows start in the logical id-space"
    );

    migrate_with_steps(&conn, MIGRATIONS).expect("migrate from 12 to head");
    assert_eq!(user_version(&conn), SCHEMA_VERSION);

    let after = snapshot_nodes(&conn);
    assert_eq!(after.len(), before.len(), "no canonical node row may be added or dropped");

    // (1) THE acceptance signal.
    assert_eq!(
        null_to_not_null(&before, &after),
        0,
        "TC-11 pin: rows transitioning logical_id NULL → NOT NULL must be exactly 0"
    );
    // (2) the mirror.
    assert_eq!(
        not_null_to_null(&before, &after),
        0,
        "TC-11 pin: a governed row may not be silently de-governed either"
    );
    // (3) nothing moved at all.
    assert_eq!(
        before, after,
        "no migration may populate, clear or re-derive logical_id on an existing canonical row"
    );
    // (4) the derived id-space is byte-identical.
    let ids_after: Vec<String> =
        after.iter().map(|(_, lid, body)| prefixed_id(lid.as_deref(), body)).collect();
    assert_eq!(ids_before, ids_after, "every pre-existing row's IdSpace must be byte-identical");
}

/// **Negative control — the detector is not vacuous.** Same fixture, but a
/// rogue `UPDATE` (the very statement `check_migration_logical_id_pin` refuses
/// to let into the ladder) is applied by hand. Both the transition counter and
/// the byte-identity comparison MUST fire. Without this, an assertion of "0
/// transitions" proves nothing about the assertion itself.
#[test]
fn the_transition_detector_catches_a_rogue_backfill() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, &steps_through(12)).expect("migrate to step 12");

    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) \
         VALUES(1, 'doc', 'anonymous body', NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) \
         VALUES(2, 'person', 'Alice', 'ent-alice')",
        [],
    )
    .unwrap();

    let before = snapshot_nodes(&conn);

    // The forbidden statement, run directly against the DB.
    let rogue = "UPDATE canonical_nodes SET logical_id = 'minted-' || write_cursor \
                 WHERE logical_id IS NULL";
    check_migration_logical_id_pin("rogue", rogue)
        .expect_err("the static guard must refuse this statement");
    conn.execute(rogue, []).unwrap();

    let after = snapshot_nodes(&conn);
    assert_eq!(
        null_to_not_null(&before, &after),
        1,
        "the detector must COUNT a real NULL → NOT NULL transition (else the 0 above is vacuous)"
    );
    assert_ne!(before, after, "the byte-identity comparison must FAIL on a real mutation");
    let id_before = prefixed_id(before[0].1.as_deref(), &before[0].2);
    let id_after = prefixed_id(after[0].1.as_deref(), &after[0].2);
    assert!(id_before.starts_with("h:") && id_after.starts_with("l:"));
    assert_ne!(id_before, id_after, "a backfill DOES move the id-space — that is why it is banned");
}

/// The mirror control: the NOT NULL → NULL detector is not vacuous either.
#[test]
fn the_degovern_detector_catches_a_rogue_clear() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, &steps_through(12)).expect("migrate to step 12");
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, logical_id) \
         VALUES(1, 'person', 'Alice', 'ent-alice')",
        [],
    )
    .unwrap();

    let before = snapshot_nodes(&conn);
    conn.execute("UPDATE canonical_nodes SET logical_id = NULL", []).unwrap();
    let after = snapshot_nodes(&conn);

    assert_eq!(not_null_to_null(&before, &after), 1, "the de-govern detector must count it");
    assert_eq!(null_to_not_null(&before, &after), 0, "…and must not confuse the two directions");
}

/// Edges carry `logical_id` too. Step 23 (TC-33) RECREATES `canonical_edges`
/// with NO data migration (HITL 2026-07-21) — pre-23 edge rows do not survive
/// at all — so the honest edge fixture starts AFTER that recreate. From step 23
/// to head, a mixed anonymous/governed edge population must survive with its
/// `logical_id` untouched.
#[test]
fn migrating_to_head_never_populates_logical_id_on_an_existing_edge() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, &steps_through(23)).expect("migrate to step 23");
    assert_eq!(user_version(&conn), 23);

    conn.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, logical_id, body) \
         VALUES(1, 'mentions', 'a', 'b', NULL, 'anonymous edge')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, logical_id, body) \
         VALUES(2, 'owns', 'a', 'c', 'edge-ac', 'governed edge')",
        [],
    )
    .unwrap();

    let before = snapshot_edges(&conn);
    assert_eq!(before.len(), 2);

    migrate_with_steps(&conn, MIGRATIONS).expect("migrate from 23 to head");
    assert_eq!(user_version(&conn), SCHEMA_VERSION);

    let after = snapshot_edges(&conn);
    assert_eq!(null_to_not_null(&before, &after), 0, "no edge may gain a logical_id");
    assert_eq!(not_null_to_null(&before, &after), 0, "no edge may lose one");
    assert_eq!(before, after, "edge identity state must be byte-identical across the migration");
}

/// Step 23's edge recreate is a DROP, not a mint: pre-23 edge rows disappear
/// entirely rather than being copied through a surrogate-computing `SELECT`.
/// Pinned because "no surviving row transitioned" would otherwise be a vacuous
/// reading of the acceptance signal for edges.
#[test]
fn step_23_edge_recreate_drops_rows_rather_than_minting_ids() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, &steps_through(22)).expect("migrate to step 22");
    conn.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, logical_id) \
         VALUES(1, 'mentions', 'a', 'b', NULL)",
        [],
    )
    .unwrap();

    migrate_with_steps(&conn, MIGRATIONS).expect("migrate from 22 to head");

    let surviving: i64 =
        conn.query_row("SELECT COUNT(*) FROM canonical_edges", [], |r| r.get(0)).unwrap();
    assert_eq!(surviving, 0, "step 23 drops pre-TC-33 edge rows (HITL: no data migration)");
    let minted: i64 = conn
        .query_row("SELECT COUNT(*) FROM canonical_edges WHERE logical_id IS NOT NULL", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(minted, 0, "and mints nothing on the way through");
}

// ---------------------------------------------------------------------------
// Small assertion helper
// ---------------------------------------------------------------------------

trait UnwrapErrOrPanic {
    fn unwrap_err_or_panic(self, message: &str, context: &str);
}

impl UnwrapErrOrPanic for Result<(), MigrationLogicalIdPinError> {
    fn unwrap_err_or_panic(self, message: &str, context: &str) {
        assert!(self.is_err(), "{message}: {context}");
    }
}
