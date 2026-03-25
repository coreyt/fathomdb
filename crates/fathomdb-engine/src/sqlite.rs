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

pub fn open_connection(path: &Path) -> Result<Connection, EngineError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    conn.busy_timeout(Duration::from_millis(5_000))?;
    Ok(conn)
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
