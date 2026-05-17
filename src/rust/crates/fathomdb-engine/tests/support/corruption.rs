//! Documented page-corruption tool for AC-006 fixture binding.
//!
//! AC-006 requires a deterministic page-corruption harness whose flavors
//! map onto the two `code` values the engine surfaces from
//! `(EventSource::SqliteInternal, EventCategory::Corruption)`:
//!
//! - [`corrupt_database_header`] — overwrites the SQLite magic-header
//!   string at the start of the page at index 0 (the header-bearing
//!   page). On reopen this surfaces `SQLITE_NOTADB`.
//! - [`corrupt_interior_page_byte`] — XORs a single byte at a caller
//!   chosen `(page_index, byte_offset)` outside the magic header. With
//!   `byte_offset >= 100` on the page at index 0 (or any byte on a
//!   page at index >= 1) this hits the SQLite B-tree decoder during
//!   the `Engine::open` schema probe and surfaces `SQLITE_CORRUPT`.
//!
//! Both helpers operate on a CLOSED database file. They MUST NOT be
//! called against an open SQLite handle: the harness writes raw bytes via
//! `std::fs`, bypassing the page cache.
//!
//! The helpers `fsync` after every write so the corruption is durable
//! before reopen — otherwise the kernel page cache can mask the flip and
//! reproduce a clean page on the next open.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// SQLite default page size used by the engine. Page 0 is the
/// header-bearing page; pages 1..N are interior / leaf pages of the
/// canonical b-trees.
pub const DEFAULT_PAGE_SIZE: u64 = 4096;

/// Overwrite the SQLite magic-header string at the start of the page
/// at index 0 (the header-bearing page).
///
/// Effect on reopen: `Connection::open` (or the first `PRAGMA` call)
/// fails with extended code `SQLITE_NOTADB`, which the engine maps to
/// `EngineOpenError::Corruption(CorruptionDetail { kind:
/// HeaderMalformed, .. })` and emits `(SqliteInternal, Corruption,
/// code = "SQLITE_NOTADB")`.
pub fn corrupt_database_header(path: &Path) {
    let mut file =
        OpenOptions::new().read(true).write(true).open(path).expect("open db file for corruption");
    file.seek(SeekFrom::Start(0)).expect("seek to header");
    // Replace the SQLite magic ("SQLite format 3\0", 16 bytes) with a
    // pattern guaranteed not to be a valid SQLite header.
    file.write_all(b"not-a-sqlite-db!").expect("overwrite header magic");
    file.flush().expect("flush header overwrite");
    file.sync_all().expect("fsync after header corruption");
}

/// XOR a single byte at `(page_index, byte_offset)` with `xor_mask`.
///
/// Use `page_index = 0, byte_offset >= 100` to corrupt the B-tree
/// header / cell-pointer region of the page at index 0 while
/// preserving the SQLite magic string (the magic occupies the first
/// 16 bytes; the database header occupies bytes 0..100). On reopen
/// the engine's schema probe (`PRAGMA schema_version`) then forces
/// SQLite to decode the corrupt b-tree page and fails with extended
/// code `SQLITE_CORRUPT`, which the engine maps to
/// `EngineOpenError::Corruption(CorruptionDetail { kind:
/// SchemaInconsistent, .. })` and emits `(SqliteInternal, Corruption,
/// code = "SQLITE_CORRUPT")`.
///
/// `xor_mask` must be non-zero — XORing with 0 is a no-op and produces
/// a misleading "corruption that wasn't" assertion failure.
pub fn corrupt_interior_page_byte(path: &Path, page_index: u32, byte_offset: u16, xor_mask: u8) {
    assert_ne!(xor_mask, 0, "corrupt_interior_page_byte: xor_mask must be non-zero");
    let absolute = u64::from(page_index) * DEFAULT_PAGE_SIZE + u64::from(byte_offset);
    let mut file =
        OpenOptions::new().read(true).write(true).open(path).expect("open db file for corruption");
    file.seek(SeekFrom::Start(absolute)).expect("seek to interior byte");
    let mut buf = [0u8; 1];
    file.read_exact(&mut buf).expect("read byte to corrupt");
    buf[0] ^= xor_mask;
    file.seek(SeekFrom::Start(absolute)).expect("seek back");
    file.write_all(&buf).expect("write corrupted byte");
    file.flush().expect("flush corruption");
    file.sync_all().expect("fsync after interior corruption");
}

/// Path of the SQLite write-ahead-log sidecar file for `db_path`.
///
/// SQLite appends the literal suffix `-wal` to the database file name;
/// it does NOT swap a file extension. `foo.sqlite` → `foo.sqlite-wal`.
#[allow(dead_code)]
pub fn wal_sidecar_path(db_path: &Path) -> PathBuf {
    let mut wal = db_path.as_os_str().to_owned();
    wal.push("-wal");
    PathBuf::from(wal)
}

/// Replace `db_path`'s WAL sidecar with a header that advertises a page
/// size larger than `SQLITE_MAX_PAGE_SIZE` (65536). On reopen the engine's
/// WAL-replay step (`PRAGMA journal_mode = WAL`) refuses to apply the
/// log and surfaces `SQLITE_CORRUPT`, which the engine maps to
/// `EngineOpenError::Corruption(CorruptionDetail { kind:
/// WalReplayFailure, stage: WalReplay, .. })`.
///
/// MUST be called against a CLOSED database. The previous WAL sidecar
/// (if any) is truncated and rewritten.
#[allow(dead_code)]
pub fn corrupt_wal_invalid_page_size(db_path: &Path) {
    let wal_path = wal_sidecar_path(db_path);
    let mut file = File::create(&wal_path).expect("create -wal sidecar");
    // WAL header layout (`sqlite3WalOpen` / `walIndexRecover` in SQLite):
    //   off  0 .. 3  magic (big-endian 0x377f0682 or 0x377f0683)
    //   off  4 .. 7  file format version (big-endian, expected 3007000)
    //   off  8 .. 11 page size (big-endian; rejected if > SQLITE_MAX_PAGE_SIZE)
    //   off 12 .. 15 checkpoint sequence
    //   off 16 .. 19 salt-1
    //   off 20 .. 23 salt-2
    //   off 24 .. 27 checksum-1
    //   off 28 .. 31 checksum-2
    let mut header = [0u8; 32];
    header[0..4].copy_from_slice(&0x377f_0683_u32.to_be_bytes());
    header[4..8].copy_from_slice(&3_007_000_u32.to_be_bytes());
    // 0x0080_0000 (8 MiB) exceeds SQLITE_MAX_PAGE_SIZE (65 536).
    header[8..12].copy_from_slice(&0x0080_0000_u32.to_be_bytes());
    file.write_all(&header).expect("write wal header");
    file.flush().expect("flush wal header");
    file.sync_all().expect("fsync after wal header corruption");
}

/// Null out the `dimension` column of the default embedder profile row
/// so that on reopen `check_embedder_profile` cannot decode the stored
/// identity. The engine maps this to
/// `EngineOpenError::Corruption(CorruptionDetail { kind:
/// EmbedderIdentityDrift, stage: EmbedderIdentity, .. })`.
///
/// MUST be called against a CLOSED database. Uses a raw rusqlite
/// connection to bypass the engine's open path so the corruption is
/// committed before the test re-opens.
#[allow(dead_code)]
pub fn corrupt_embedder_profile_row(db_path: &Path) {
    let conn = rusqlite::Connection::open(db_path).expect("open db for profile corruption");
    // NEGATIVE dimension passes the `NOT NULL` constraint but fails
    // `row.get::<_, u32>(2)` with `IntegralValueOutOfRange`, which the
    // engine maps to `Corruption(EmbedderIdentityDrift)`. NULL would
    // hit the `NOT NULL` schema constraint and never commit.
    conn.execute(
        "UPDATE _fathomdb_embedder_profiles SET dimension = -1 WHERE profile = 'default'",
        [],
    )
    .expect("set embedder profile dimension to -1");
    // Force a checkpoint so the corruption lands in the main DB file
    // rather than sitting in the -wal sidecar (the open path examines
    // both, but we want the corruption to survive a -wal teardown).
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").ok();
    drop(conn);
}
