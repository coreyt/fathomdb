//! 0.8.22 Slice 19 — canonical FTS-hydration join indexes are additive only.

use fathomdb_schema::{migrate, migrate_with_steps, Migration, MIGRATIONS, SCHEMA_VERSION};
use rusqlite::Connection;
use std::sync::Once;

const NODE_CURSOR_INDEX: &str = "canonical_nodes_write_cursor_idx";
const EDGE_CURSOR_INDEX: &str = "canonical_edges_write_cursor_idx";

fn register_sqlite_vec_once() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| unsafe {
        let entrypoint: unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *mut std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int = std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
        rusqlite::ffi::sqlite3_auto_extension(Some(entrypoint));
    });
}

fn steps_through(limit: u32) -> Vec<Migration> {
    MIGRATIONS.iter().filter(|step| step.step_id <= limit).cloned().collect()
}

fn index_sql(connection: &Connection, name: &str) -> String {
    connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [name],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| panic!("required index {name} must exist"))
}

fn assert_join_indexes(connection: &Connection) {
    assert_eq!(
        index_sql(connection, NODE_CURSOR_INDEX),
        "CREATE INDEX canonical_nodes_write_cursor_idx ON canonical_nodes(write_cursor)"
    );
    assert_eq!(
        index_sql(connection, EDGE_CURSOR_INDEX),
        "CREATE INDEX canonical_edges_write_cursor_idx ON canonical_edges(write_cursor)"
    );
}

#[test]
fn fresh_database_installs_both_unconditional_cursor_indexes() {
    register_sqlite_vec_once();
    let connection = Connection::open_in_memory().unwrap();

    migrate(&connection).unwrap();

    assert_eq!(SCHEMA_VERSION, 26);
    assert_eq!(
        connection.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0)).unwrap(),
        26
    );
    assert_join_indexes(&connection);
}

#[test]
fn schema_25_upgrade_installs_both_unconditional_cursor_indexes() {
    register_sqlite_vec_once();
    let connection = Connection::open_in_memory().unwrap();
    migrate_with_steps(&connection, &steps_through(25)).unwrap();
    assert_eq!(
        connection.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0)).unwrap(),
        25
    );

    let report = migrate(&connection).unwrap();

    assert_eq!(report.schema_version_before, 25);
    assert_eq!(report.schema_version_after, 26);
    assert_eq!(
        report.migration_steps.iter().map(|step| step.step_id).collect::<Vec<_>>(),
        vec![26]
    );
    assert_join_indexes(&connection);
}
