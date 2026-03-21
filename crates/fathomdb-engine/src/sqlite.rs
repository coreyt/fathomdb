use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::EngineError;

pub fn open_connection(path: &Path) -> Result<Connection, EngineError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    conn.busy_timeout(Duration::from_millis(5_000))?;
    Ok(conn)
}
