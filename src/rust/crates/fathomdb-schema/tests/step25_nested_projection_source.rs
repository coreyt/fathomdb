//! 0.8.21 Slice 45 — the nested-source declaration migration is additive-only.

use fathomdb_schema::{
    check_migration_accretion, migrate_with_steps, Migration, MIGRATIONS, SCHEMA_VERSION,
};
use rusqlite::Connection;
use std::sync::Once;

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

fn steps_through(limit: u32) -> Vec<Migration> {
    MIGRATIONS.iter().filter(|step| step.step_id <= limit).cloned().collect()
}

#[test]
fn step25_adds_nullable_source_without_rewriting_registry_rows() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    migrate_with_steps(&conn, &steps_through(24)).unwrap();
    conn.execute(
        "INSERT INTO _fathomdb_projection_registry(name, roles, fts_tokenizer, vector_embedder, vector_declared)
         VALUES('status', 'filterable', NULL, NULL, 0)",
        [],
    )
    .unwrap();

    migrate_with_steps(&conn, MIGRATIONS).unwrap();
    assert_eq!(SCHEMA_VERSION, 25);
    let source: Option<String> = conn
        .query_row(
            "SELECT source FROM _fathomdb_projection_registry WHERE name = 'status'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source, None, "a v24 direct projection remains direct after the additive migration");
}

#[test]
fn step25_is_additive_and_guarded() {
    let step = MIGRATIONS.iter().find(|step| step.step_id == 25).unwrap();
    check_migration_accretion("step-25", step.sql).unwrap();
    assert!(step.sql.contains("ADD COLUMN source TEXT"));
    assert!(!step.sql.contains("INSERT INTO"));
}
