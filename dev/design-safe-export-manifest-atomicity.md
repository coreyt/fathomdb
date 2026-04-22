# Design: Atomic Manifest Write and Export Page Count

## Purpose

Address two verified findings in `safe_export`: non-atomic manifest write
(H-4) and the checkpoint-to-backup page count race (M-6).

---

## H-4. Atomic Manifest Write

### Current State

`crates/fathomdb-engine/src/admin/mod.rs`

The manifest JSON is written via `fs::write(&manifest_path, manifest_json)?`.
If the process crashes mid-write, a partial manifest file exists on disk
without complete integrity metadata. Consumers cannot verify the export.

### Design

Write to a temporary file, then rename atomically:

```rust
let manifest_tmp = manifest_path.with_extension("json.tmp");
fs::write(&manifest_tmp, &manifest_json)?;
fs::rename(&manifest_tmp, &manifest_path)?;
```

`rename()` is atomic on POSIX filesystems when source and destination are
on the same filesystem. Since both are in the same directory (the export
destination), this is guaranteed.

On the error path, clean up the temporary file:

```rust
let manifest_tmp = manifest_path.with_extension("json.tmp");
if let Err(e) = fs::write(&manifest_tmp, &manifest_json)
    .and_then(|_| fs::rename(&manifest_tmp, &manifest_path))
{
    let _ = fs::remove_file(&manifest_tmp);
    return Err(e.into());
}
```

---

## M-6. Page Count Timing

### Current State

`crates/fathomdb-engine/src/admin/mod.rs`

`page_count` is captured via `PRAGMA page_count` before `conn.backup()`
runs. New writes can land in the WAL between the PRAGMA and the backup
call, so the backup may contain more pages than the manifest reports.

### Design

Two options, in order of preference:

**Option A: capture page count from the backup file.**

After `conn.backup()` completes, query the page count from the *exported*
file rather than the source:

```rust
conn.backup(DatabaseName::Main, &destination_path, None)?;

let export_conn = Connection::open_with_flags(
    &destination_path,
    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
)?;
let page_count: u64 = export_conn.query_row(
    "PRAGMA page_count", [], |row| row.get(0)
)?;
```

This is authoritative — the page count reflects exactly what was backed
up. The cost is one additional connection open on the exported file, which
is negligible.

**Option B: document `page_count` as advisory.**

Mark the field as `page_count_approximate` in the manifest and document
that it reflects a lower bound captured before backup. Consumers should
use the SHA-256 hash for integrity verification, not page count.

**Recommendation:** Option A. The manifest exists for verification. An
advisory page count weakens the verification contract.

---

## Test Plan

- Write a manifest via the atomic path. Kill the process (or simulate
  failure) between write and rename. Verify no corrupt manifest exists.
- Verify that `page_count` in the manifest matches the actual page count
  of the exported database file.
