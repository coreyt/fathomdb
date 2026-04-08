use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::EngineError;

const SHARED_SQLITE_POLICY: &str = include_str!("../../../tooling/sqlite.env");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedSqlitePolicy {
    pub minimum_supported_version: String,
    pub repo_dev_version: String,
    pub repo_local_binary_relpath: PathBuf,
}

#[cfg(feature = "tracing")]
static SQLITE_LOG_INIT: std::sync::Once = std::sync::Once::new();

/// Forward `SQLite` internal error/warning events into the tracing facade.
///
/// Registered once per process via `SQLITE_CONFIG_LOG` before any connections
/// are opened.  The primary error code determines the tracing level:
/// `NOTICE` → `INFO`, `WARNING` → `WARN`, everything else → `ERROR`.
#[cfg(feature = "tracing")]
fn sqlite_log_callback(code: std::os::raw::c_int, msg: &str) {
    let primary = code & 0xFF;
    if primary == rusqlite::ffi::SQLITE_NOTICE as std::os::raw::c_int {
        tracing::info!(target: "fathomdb_engine::sqlite", sqlite_error_code = code, "{msg}");
    } else if primary == rusqlite::ffi::SQLITE_WARNING as std::os::raw::c_int {
        tracing::warn!(target: "fathomdb_engine::sqlite", sqlite_error_code = code, "{msg}");
    } else {
        tracing::error!(target: "fathomdb_engine::sqlite", sqlite_error_code = code, "{msg}");
    }
}

/// Install `sqlite3_trace_v2` with `SQLITE_TRACE_PROFILE` on a connection.
///
/// Fires a TRACE-level event for each statement completion with the SQL text
/// and execution duration.  Only registered in debug builds — TRACE events are
/// compiled out by `release_max_level_info` in release builds, so registering
/// the callback would waste FFI overhead on every statement for no output.
#[cfg(all(feature = "tracing", debug_assertions))]
fn install_trace_v2(conn: &Connection) {
    use std::os::raw::{c_int, c_uint, c_void};

    unsafe extern "C" fn trace_v2_callback(
        event_type: c_uint,
        _ctx: *mut c_void,
        p: *mut c_void,
        x: *mut c_void,
    ) -> c_int {
        if event_type == rusqlite::ffi::SQLITE_TRACE_PROFILE as c_uint {
            let stmt = p.cast::<rusqlite::ffi::sqlite3_stmt>();
            let nanos = unsafe { *(x.cast::<i64>()) };
            let sql_ptr = unsafe { rusqlite::ffi::sqlite3_sql(stmt) };
            if !sql_ptr.is_null() {
                let sql = unsafe { std::ffi::CStr::from_ptr(sql_ptr) }.to_string_lossy();
                tracing::trace!(
                    target: "fathomdb_engine::sqlite",
                    sql = %sql,
                    duration_us = nanos / 1000,
                    "sqlite statement profile"
                );
            }
        }
        0
    }

    unsafe {
        rusqlite::ffi::sqlite3_trace_v2(
            conn.handle(),
            rusqlite::ffi::SQLITE_TRACE_PROFILE as c_uint,
            Some(trace_v2_callback),
            std::ptr::null_mut(),
        );
    }
}

pub fn open_connection(path: &Path) -> Result<Connection, EngineError> {
    #[cfg(feature = "tracing")]
    SQLITE_LOG_INIT.call_once(|| {
        // Safety: Once guard ensures no concurrent SQLite calls during config.
        // config_log must be called before any connections are opened.
        unsafe {
            let _ = rusqlite::trace::config_log(Some(sqlite_log_callback));
        }
    });

    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    conn.busy_timeout(Duration::from_millis(5_000))?;

    #[cfg(all(feature = "tracing", debug_assertions))]
    install_trace_v2(&conn);

    Ok(conn)
}

/// Open a read-only database connection.
///
/// Uses `SQLITE_OPEN_READONLY` so that any attempt to write through this
/// connection fails at the `SQLite` level.  Intended for reader-pool connections
/// where the writer has already created the database and set WAL mode.
///
/// # Errors
/// Returns [`EngineError`] if the database file cannot be opened.
pub fn open_readonly_connection(path: &Path) -> Result<Connection, EngineError> {
    #[cfg(feature = "tracing")]
    SQLITE_LOG_INIT.call_once(|| {
        // Safety: Once guard ensures no concurrent SQLite calls during config.
        // config_log must be called before any connections are opened.
        unsafe {
            let _ = rusqlite::trace::config_log(Some(sqlite_log_callback));
        }
    });

    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(5_000))?;

    #[cfg(all(feature = "tracing", debug_assertions))]
    install_trace_v2(&conn);

    Ok(conn)
}

/// Open a read-only database connection with the sqlite-vec extension loaded.
///
/// Combines [`open_readonly_connection`] with the `sqlite3_vec_init`
/// auto-extension registration.
///
/// # Errors
/// Returns [`EngineError`] if the underlying database connection cannot be
/// opened (same failure modes as [`open_readonly_connection`]).
#[cfg(feature = "sqlite-vec")]
pub fn open_readonly_connection_with_vec(path: &Path) -> Result<Connection, EngineError> {
    // Safety: sqlite3_auto_extension is idempotent for the same function pointer.
    unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    }
    open_readonly_connection(path)
}

/// Open a database connection with the sqlite-vec extension loaded.
///
/// Registers `sqlite3_vec_init` as a global auto-extension so the extension is
/// available in every connection opened after this call.  The registration is
/// idempotent — SQLite deduplicates identical function-pointer registrations.
///
/// # Errors
/// Returns [`EngineError`] if the underlying database connection cannot be
/// opened (same failure modes as [`open_connection`]).
#[cfg(feature = "sqlite-vec")]
pub fn open_connection_with_vec(path: &Path) -> Result<Connection, EngineError> {
    // Safety: sqlite3_auto_extension is idempotent for the same function pointer.
    // The transmute converts the sqlite-vec init signature
    // (db, pz_err_msg, p_api) -> c_int to the erased () -> c_int expected by
    // sqlite3_auto_extension; SQLite passes the real args at load time.
    unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    }
    open_connection(path)
}

/// # Errors
/// Returns a `String` error if the embedded `sqlite.env` policy file is malformed or missing
/// required keys (`SQLITE_MIN_VERSION`, `SQLITE_VERSION`).
pub fn shared_sqlite_policy() -> Result<SharedSqlitePolicy, String> {
    let mut minimum_supported_version = None;
    let mut repo_dev_version = None;

    for raw_line in SHARED_SQLITE_POLICY.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("invalid sqlite policy line: {line}"));
        };

        match key.trim() {
            "SQLITE_MIN_VERSION" => minimum_supported_version = Some(value.trim().to_owned()),
            "SQLITE_VERSION" => repo_dev_version = Some(value.trim().to_owned()),
            other => return Err(format!("unknown sqlite policy key: {other}")),
        }
    }

    let minimum_supported_version =
        minimum_supported_version.ok_or_else(|| "missing SQLITE_MIN_VERSION".to_owned())?;
    let repo_dev_version = repo_dev_version.ok_or_else(|| "missing SQLITE_VERSION".to_owned())?;
    let repo_local_binary_relpath =
        PathBuf::from(format!(".local/sqlite-{repo_dev_version}/bin/sqlite3"));

    Ok(SharedSqlitePolicy {
        minimum_supported_version,
        repo_dev_version,
        repo_local_binary_relpath,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::shared_sqlite_policy;

    #[test]
    fn shared_sqlite_policy_matches_repo_defaults() {
        let policy = shared_sqlite_policy().expect("shared sqlite policy");

        assert_eq!(policy.minimum_supported_version, "3.41.0");
        assert_eq!(policy.repo_dev_version, "3.46.0");
        assert!(
            policy
                .repo_local_binary_relpath
                .ends_with("sqlite-3.46.0/bin/sqlite3")
        );
    }
}
