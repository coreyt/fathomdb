use std::fs::{File, TryLockError};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use crate::EngineError;

/// Exclusive file lock preventing multiple engine instances on the same database.
///
/// Acquired via [`DatabaseLock::acquire`] during [`EngineRuntime::open`].  The lock
/// is held for the lifetime of the engine and released automatically when this
/// struct is dropped (the kernel releases the `flock` when the fd closes).
///
/// The lock file also contains the PID of the holding process as a diagnostic
/// aid — this is best-effort and not part of the locking protocol.
#[derive(Debug)]
pub(crate) struct DatabaseLock {
    _file: File,
}

impl DatabaseLock {
    /// Try to acquire an exclusive lock for the database at `database_path`.
    ///
    /// Creates `{database_path}.lock` if it does not exist.  On success, writes
    /// the current PID to the lock file and returns the held lock.  On failure
    /// (another process holds the lock), returns [`EngineError::DatabaseLocked`]
    /// with the holder PID when available.
    pub(crate) fn acquire(database_path: &Path) -> Result<Self, EngineError> {
        let lock_path = lock_path_for(database_path);

        let mut file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| {
                EngineError::Io(std::io::Error::new(
                    e.kind(),
                    format!("failed to open lock file {}: {e}", lock_path.display()),
                ))
            })?;

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                let holder_pid = read_pid(&mut file);
                let pid_msg = match holder_pid {
                    Some(pid) => format!(" (held by pid {pid})"),
                    None => String::new(),
                };
                return Err(EngineError::DatabaseLocked(format!(
                    "database already in use{pid_msg}: {}",
                    database_path.display(),
                )));
            }
            Err(TryLockError::Error(e)) => {
                return Err(EngineError::Io(std::io::Error::new(
                    e.kind(),
                    format!("failed to lock {}: {e}", lock_path.display()),
                )));
            }
        }

        // Write our PID for diagnostics.
        let _ = file.set_len(0);
        let _ = file.seek(std::io::SeekFrom::Start(0));
        let _ = write!(file, "{}", std::process::id());

        Ok(Self { _file: file })
    }
}

/// Compute the lock file path for a given database path.
fn lock_path_for(database_path: &Path) -> PathBuf {
    let mut s = database_path.as_os_str().to_owned();
    s.push(".lock");
    PathBuf::from(s)
}

/// Best-effort read of a PID from the lock file.
fn read_pid(file: &mut File) -> Option<u32> {
    let _ = file.seek(std::io::SeekFrom::Start(0));
    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;
    buf.trim().parse().ok()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn lock_acquires_successfully() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");

        let lock = DatabaseLock::acquire(&db_path);
        assert!(lock.is_ok(), "acquire must succeed: {:?}", lock.err());

        let lock_file = lock_path_for(&db_path);
        assert!(lock_file.exists(), "lock file must be created");
    }

    #[test]
    fn second_lock_on_same_path_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");

        let _first = DatabaseLock::acquire(&db_path).expect("first acquire");
        let second = DatabaseLock::acquire(&db_path);

        assert!(second.is_err(), "second acquire must fail");
        let err = second.unwrap_err();
        assert!(
            matches!(err, EngineError::DatabaseLocked(_)),
            "expected DatabaseLocked, got: {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("already in use"),
            "error must mention 'already in use': {msg}"
        );
    }

    #[test]
    fn lock_released_on_drop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");

        {
            let _lock = DatabaseLock::acquire(&db_path).expect("first acquire");
        }
        // Lock should be released after drop.
        let second = DatabaseLock::acquire(&db_path);
        assert!(
            second.is_ok(),
            "re-acquire after drop must succeed: {:?}",
            second.err()
        );
    }

    #[test]
    fn lock_file_contains_pid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");

        let _lock = DatabaseLock::acquire(&db_path).expect("acquire");

        let contents = std::fs::read_to_string(lock_path_for(&db_path)).expect("read lock file");
        let pid: u32 = contents.trim().parse().expect("parse pid");
        assert_eq!(pid, std::process::id());
    }

    #[test]
    fn lock_error_includes_holder_pid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");

        let _first = DatabaseLock::acquire(&db_path).expect("first acquire");
        let err = DatabaseLock::acquire(&db_path).unwrap_err();

        let msg = err.to_string();
        let our_pid = std::process::id().to_string();
        assert!(
            msg.contains(&our_pid),
            "error must contain holder pid {our_pid}: {msg}"
        );
    }

    #[test]
    fn lock_path_appends_dot_lock() {
        let path = Path::new("/tmp/my_database.db");
        let lock = lock_path_for(path);
        assert_eq!(lock, PathBuf::from("/tmp/my_database.db.lock"));
    }
}
