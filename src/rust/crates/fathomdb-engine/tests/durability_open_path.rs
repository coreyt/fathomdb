//! Open-path corruption matrix (AC-035a / AC-035b / AC-035c).
//!
//! Four fixtures, one per `CorruptionKind` variant. Each fixture seeds a
//! clean database, closes it, applies a deterministic corruption via
//! `tests/support/corruption.rs`, and asserts that `Engine::open`:
//!
//!   - returns `Err(EngineOpenError::Corruption(_))` (AC-035a)
//!   - carries the documented `(kind, stage, recovery_hint.code)` tuple
//!     from `dev/design/errors.md` § OpenStage matrix (AC-035b)
//!   - releases the writer lock and retains no SQLite fd / writer thread
//!     in the calling process (AC-035c)
//!
//! Substrate notes:
//!   - `corrupt_database_header` / `corrupt_interior_page_byte` already
//!     existed for AC-006 page-corruption coverage; reused here.
//!   - `corrupt_wal_invalid_page_size` / `corrupt_embedder_profile_row`
//!     are new helpers added for the WAL-replay and embedder-identity
//!     drift fixtures.
//!   - Process introspection (AC-035c) is linux-only. The test gates on
//!     `cfg(target_os = "linux")` rather than skipping silently.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use fathomdb_engine::{
    CorruptionDetail, CorruptionKind, CorruptionLocator, Engine, EngineOpenError, OpenStage,
};
use tempfile::TempDir;

#[path = "support/corruption.rs"]
mod corruption;

/// Serialize every test in this binary. AC-035c inspects process-wide
/// thread names and open fds; parallel sibling tests that hold an
/// `Engine` alive would otherwise show up as `fathomdb-reader-*` threads
/// and false-fail the assertion. Even tests that do not check thread
/// names take the guard so an AC-035c test never races a sibling that
/// has just spawned reader threads.
fn serial_guard() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|p| p.into_inner())
}

#[derive(Clone, Copy, Debug)]
struct Expected {
    kind: CorruptionKind,
    stage: OpenStage,
    code: &'static str,
    doc_anchor: &'static str,
}

const EXPECTED_WAL: Expected = Expected {
    kind: CorruptionKind::WalReplayFailure,
    stage: OpenStage::WalReplay,
    code: "E_CORRUPT_WAL_REPLAY",
    doc_anchor: "design/recovery.md#wal-replay-failures",
};
const EXPECTED_HEADER: Expected = Expected {
    kind: CorruptionKind::HeaderMalformed,
    stage: OpenStage::HeaderProbe,
    code: "E_CORRUPT_HEADER",
    doc_anchor: "design/recovery.md#header-malformed",
};
const EXPECTED_SCHEMA: Expected = Expected {
    kind: CorruptionKind::SchemaInconsistent,
    stage: OpenStage::SchemaProbe,
    code: "E_CORRUPT_SCHEMA",
    doc_anchor: "design/recovery.md#schema-inconsistent",
};
const EXPECTED_EMBEDDER: Expected = Expected {
    kind: CorruptionKind::EmbedderIdentityDrift,
    stage: OpenStage::EmbedderIdentity,
    code: "E_CORRUPT_EMBEDDER_IDENTITY",
    doc_anchor: "design/recovery.md#embedder-identity-drift",
};

fn fixture_for(kind: CorruptionKind) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("corrupt.sqlite");
    // Seed a clean database via the engine so schema + embedder profile
    // are populated before corruption is applied.
    let opened = Engine::open(&path).expect("seed open");
    opened.engine.close().expect("seed close");
    drop(opened);
    apply_corruption(kind, &path);
    (dir, path)
}

fn apply_corruption(kind: CorruptionKind, path: &Path) {
    match kind {
        CorruptionKind::WalReplayFailure => {
            corruption::corrupt_wal_invalid_page_size(path);
        }
        CorruptionKind::HeaderMalformed => {
            corruption::corrupt_database_header(path);
        }
        CorruptionKind::SchemaInconsistent => {
            // Magic intact; XOR a byte in the b-tree header so the
            // schema probe trips during page decode.
            corruption::corrupt_interior_page_byte(path, 0, 100, 0xFF);
        }
        CorruptionKind::EmbedderIdentityDrift => {
            corruption::corrupt_embedder_profile_row(path);
        }
    }
}

fn open_and_expect_corruption(path: &Path) -> CorruptionDetail {
    let err = Engine::open(path).expect_err("Engine::open must fail on corruption fixture");
    match err {
        EngineOpenError::Corruption(detail) => detail,
        other => panic!("expected Corruption, got {other:?}"),
    }
}

// ── AC-035a: refusal + no handle + no side effect ───────────────────────────

#[test]
fn ac_035a_wal_replay_corruption_refuses_open() {
    assert_refuses_with_no_side_effect(CorruptionKind::WalReplayFailure);
}

#[test]
fn ac_035a_header_corruption_refuses_open() {
    assert_refuses_with_no_side_effect(CorruptionKind::HeaderMalformed);
}

#[test]
fn ac_035a_schema_corruption_refuses_open() {
    assert_refuses_with_no_side_effect(CorruptionKind::SchemaInconsistent);
}

#[test]
fn ac_035a_embedder_identity_corruption_refuses_open() {
    assert_refuses_with_no_side_effect(CorruptionKind::EmbedderIdentityDrift);
}

fn assert_refuses_with_no_side_effect(kind: CorruptionKind) {
    let _guard = serial_guard();
    let (_dir, path) = fixture_for(kind);
    let mtime_before = fs::metadata(&path).expect("stat before").modified().expect("mtime before");
    let len_before = fs::metadata(&path).expect("stat before len").len();

    let result = Engine::open(&path);
    assert!(
        matches!(result, Err(EngineOpenError::Corruption(_))),
        "expected Err(Corruption) for {kind:?}, got {result:?}"
    );

    let mtime_after = fs::metadata(&path).expect("stat after").modified().expect("mtime after");
    let len_after = fs::metadata(&path).expect("stat after len").len();
    assert_eq!(
        mtime_before, mtime_after,
        "{kind:?}: failed open must not modify the DB file (mtime drift)"
    );
    assert_eq!(
        len_before, len_after,
        "{kind:?}: failed open must not modify the DB file (len drift)"
    );
}

// ── AC-035b: CorruptionDetail shape per kind ────────────────────────────────

#[test]
fn ac_035b_wal_replay_shape() {
    let _guard = serial_guard();
    let (_dir, path) = fixture_for(CorruptionKind::WalReplayFailure);
    let detail = open_and_expect_corruption(&path);
    assert_shape(&detail, &EXPECTED_WAL);
}

#[test]
fn ac_035b_header_shape() {
    let _guard = serial_guard();
    let (_dir, path) = fixture_for(CorruptionKind::HeaderMalformed);
    let detail = open_and_expect_corruption(&path);
    assert_shape(&detail, &EXPECTED_HEADER);
}

#[test]
fn ac_035b_schema_shape() {
    let _guard = serial_guard();
    let (_dir, path) = fixture_for(CorruptionKind::SchemaInconsistent);
    let detail = open_and_expect_corruption(&path);
    assert_shape(&detail, &EXPECTED_SCHEMA);
}

#[test]
fn ac_035b_embedder_identity_shape() {
    let _guard = serial_guard();
    let (_dir, path) = fixture_for(CorruptionKind::EmbedderIdentityDrift);
    let detail = open_and_expect_corruption(&path);
    assert_shape(&detail, &EXPECTED_EMBEDDER);
}

fn assert_shape(detail: &CorruptionDetail, expected: &Expected) {
    assert_eq!(detail.kind, expected.kind, "kind mismatch");
    assert_eq!(detail.stage, expected.stage, "stage mismatch");
    // AC-035b: `OpenStage` must remain in `{WalReplay, HeaderProbe,
    // SchemaProbe, EmbedderIdentity}`. Compile-time enforced by
    // exhaustive match below; any future LockAcquisition-style variant
    // would cause this match to non-exhaustively fail to compile.
    match detail.stage {
        OpenStage::WalReplay
        | OpenStage::HeaderProbe
        | OpenStage::SchemaProbe
        | OpenStage::EmbedderIdentity => {}
    }
    // AC-035b: opaque SQLite paths must surface as OpaqueSqliteError with
    // a typed extended_code; CorruptionLocator must never expose a
    // free-form Unspecified variant.
    match detail.locator {
        CorruptionLocator::OpaqueSqliteError { sqlite_extended_code: _ }
        | CorruptionLocator::FileOffset { .. }
        | CorruptionLocator::PageId { .. }
        | CorruptionLocator::TableRow { .. }
        | CorruptionLocator::Vec0ShadowRow { .. }
        | CorruptionLocator::MigrationStep { .. } => {}
    }
    assert_eq!(
        detail.recovery_hint.code, expected.code,
        "recovery_hint.code mismatch for {:?}",
        expected.kind
    );
    assert_eq!(
        detail.recovery_hint.doc_anchor, expected.doc_anchor,
        "recovery_hint.doc_anchor mismatch for {:?}",
        expected.kind
    );
}

// AC-035b: `code` is a `&'static str`; re-running the fixture twice must
// yield bit-equal pointers (and obviously bit-equal contents).
#[test]
fn ac_035b_code_is_bit_stable_across_runs() {
    let _guard = serial_guard();
    for expected in [&EXPECTED_WAL, &EXPECTED_HEADER, &EXPECTED_SCHEMA, &EXPECTED_EMBEDDER] {
        let (_dir1, path1) = fixture_for(expected.kind);
        let (_dir2, path2) = fixture_for(expected.kind);
        let detail1 = open_and_expect_corruption(&path1);
        let detail2 = open_and_expect_corruption(&path2);
        assert_eq!(detail1.recovery_hint.code, detail2.recovery_hint.code);
        assert_eq!(detail1.recovery_hint.code, expected.code);
        // The `&'static str` from two separate calls within the same
        // process must point at the same backing storage — string-
        // literal interning gives bit-equal pointer comparison and
        // proves the engine isn't allocating a fresh `String` per
        // failure.
        assert!(
            std::ptr::eq(detail1.recovery_hint.code, detail2.recovery_hint.code),
            "recovery_hint.code must be a static literal, not an allocated string"
        );
    }
}

// ── AC-035c: lock released, fd not retained, no writer/scheduler thread ─────

#[cfg(target_os = "linux")]
#[test]
fn ac_035c_lock_released_after_corruption_header() {
    assert_lock_released_and_no_engine_residue(CorruptionKind::HeaderMalformed);
}

#[cfg(target_os = "linux")]
#[test]
fn ac_035c_lock_released_after_corruption_schema() {
    assert_lock_released_and_no_engine_residue(CorruptionKind::SchemaInconsistent);
}

#[cfg(target_os = "linux")]
fn assert_lock_released_and_no_engine_residue(kind: CorruptionKind) {
    let _guard = serial_guard();
    let (_dir, path) = fixture_for(kind);
    // Trigger the failed open; the lock must be released by the time
    // the call returns.
    let err = Engine::open(&path).expect_err("must fail");
    assert!(matches!(err, EngineOpenError::Corruption(_)));

    let lock_path = lock_path_for(&path);
    assert!(lock_path.exists(), "engine should have created the lockfile before releasing it");

    // Re-acquire the advisory lock that the engine took on `.lock`. If
    // the failed open leaked the file lock, this `flock(LOCK_EX|NB)`
    // would return EWOULDBLOCK.
    let probe = File::options().read(true).write(true).open(&lock_path).expect("open lock file");
    let probe_fd: i32 = std::os::unix::io::AsRawFd::as_raw_fd(&probe);
    let rc = unsafe { libc::flock(probe_fd, libc::LOCK_EX | libc::LOCK_NB) };
    assert_eq!(
        rc,
        0,
        "AC-035c: lock must be releasable after failed open ({}): errno = {}",
        lock_path.display(),
        std::io::Error::last_os_error()
    );
    let _ = unsafe { libc::flock(probe_fd, libc::LOCK_UN) };

    // /proc/self/fd: assert no fd points at the database file. The
    // canonical absolute path of `path` must not appear as the
    // destination of any open fd in this process.
    let canonical = path.canonicalize().expect("canonicalize db path");
    let fd_dir = PathBuf::from("/proc/self/fd");
    for entry in fs::read_dir(&fd_dir).expect("read /proc/self/fd") {
        let entry = entry.expect("dirent");
        let link = match fs::read_link(entry.path()) {
            Ok(target) => target,
            // The fd may have been closed between readdir and readlink;
            // race is fine for this assertion.
            Err(_) => continue,
        };
        assert_ne!(
            link,
            canonical,
            "AC-035c: /proc/self/fd/{} still points at the database \
             file after a failed open",
            entry.file_name().to_string_lossy()
        );
    }

    // /proc/self/task: no thread named per fathomdb conventions
    // (`fathomdb-reader-N`, etc.) must be live in this process.
    let task_dir = PathBuf::from("/proc/self/task");
    for entry in fs::read_dir(&task_dir).expect("read /proc/self/task") {
        let entry = entry.expect("task dirent");
        let comm_path = entry.path().join("comm");
        let Ok(comm_bytes) = fs::read(&comm_path) else { continue };
        let comm = String::from_utf8_lossy(&comm_bytes).trim().to_string();
        assert!(
            !comm.starts_with("fathomdb-"),
            "AC-035c: thread named '{comm}' is still live after a failed open"
        );
    }
}

#[cfg(target_os = "linux")]
fn lock_path_for(db_path: &Path) -> PathBuf {
    let mut lock = db_path.as_os_str().to_owned();
    lock.push(".lock");
    PathBuf::from(lock)
}

// ── Finding 1: bounded WAL sidecar probe ────────────────────────────────────
//
// `probe_wal_sidecar` must only read the 32-byte WAL header, never the
// full sidecar. Without this guard, an unclean shutdown that leaves a
// large -wal file behind would force `Engine::open` to slurp the whole
// file into memory before SQLite itself touches recovery — measurable
// latency + RSS regression on the open path. Test makes the WAL sidecar
// 256 MiB; bounded read returns in ~ms, a full read pays ~hundreds of
// ms (alloc + copy) and would flunk the wall-clock bound.
#[test]
fn probe_wal_sidecar_bounded_read() {
    let _guard = serial_guard();
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("bounded.sqlite");
    // Seed a real DB so the open path reaches `probe_wal_sidecar`.
    let opened = Engine::open(&path).expect("seed open");
    opened.engine.close().expect("seed close");
    drop(opened);

    let wal_path = corruption::wal_sidecar_path(&path);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&wal_path)
        .expect("create large wal");
    // Valid WAL magic + version, invalid page size so the probe still
    // rejects; the rest of the sidecar is sparse zeros. A full-file
    // read pays for the entire 256 MiB.
    let mut header = [0u8; 32];
    header[0..4].copy_from_slice(&0x377f_0683_u32.to_be_bytes());
    header[4..8].copy_from_slice(&3_007_000_u32.to_be_bytes());
    header[8..12].copy_from_slice(&0x0080_0000_u32.to_be_bytes());
    use std::io::Write;
    file.write_all(&header).expect("wal header");
    const WAL_LEN: u64 = 256 * 1024 * 1024;
    file.set_len(WAL_LEN).expect("set_len 256 MiB sparse");
    file.sync_all().expect("fsync large wal");
    drop(file);

    let started = std::time::Instant::now();
    let result = Engine::open(&path);
    let elapsed = started.elapsed();

    assert!(
        matches!(result, Err(EngineOpenError::Corruption(_))),
        "expected Corruption on invalid page size, got {result:?}"
    );
    // Bounded read is sub-ms; an unbounded `std::fs::read` of 256 MiB
    // measurably loses this margin. 200 ms is generous for slow CI
    // hosts while still flagging a regression to full-file read.
    assert!(
        elapsed < std::time::Duration::from_millis(200),
        "probe_wal_sidecar took {elapsed:?} on a 256 MiB sidecar; \
         must read only the 32-byte header"
    );
}
