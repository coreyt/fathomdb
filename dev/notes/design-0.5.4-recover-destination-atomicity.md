# Design: Recovery Destination Atomicity (0.5.4 candidate)

**Release:** 0.5.4 candidate
**Status:** Draft 2026-04-22
**Finding covered:** Review finding 2
**Breaking:** No public API break intended

---

## Problem

`fathom-integrity recover` attempts to ensure the recovery destination does not
already exist by opening it with `O_CREATE|O_EXCL`, then closes and removes the
guard file so `sqlite3` can create the recovered database at that path. This
re-opens a time-of-check/time-of-use window. In an attacker-writable destination
directory, another process can create a symlink or different file at the
destination path after the guard is removed and before `sqlite3` opens it.

The destination path should be published exactly once, with no overwrite and no
symlink following at the final path.

---

## Current State Anchors

| Area | Current behavior |
|---|---|
| Guard creation | `go/fathom-integrity/internal/commands/recover.go:113` creates `destPath` with `O_EXCL`. |
| TOCTOU window | `recover.go:121` removes the guard before replaying recovered SQL. |
| SQLite replay | `recover.go:135` invokes `sqlite3 destPath`, which creates or opens whatever path exists at that moment. |
| Empty target creation | `recover.go:147` invokes `sqlite3 destPath SELECT 1` if no recovered SQL produced a file. |

---

## Goals

- Never ask external `sqlite3` to create the final destination path directly.
- Publish the final database with no-overwrite semantics.
- Avoid following symlinks at the final destination.
- Keep recovered output in the same filesystem as the final destination so
  publish can be atomic or no-overwrite by link/rename primitive.
- Preserve current CLI behavior: destination must not already exist.
- Clean up temporary files on failure.

## Non-Goals

- Making recovery safe when the attacker controls the source database contents.
  SQL sanitization remains a separate concern.
- Supporting recovery into an already-existing destination.
- Replacing the `sqlite3` CLI dependency.

---

## Design

### 1. Recover into a private temp directory

Resolve:

```go
parent := filepath.Dir(destPath)
base := filepath.Base(destPath)
```

Create parent if needed with `0700` as today. Then create a private temp
directory inside parent:

```go
tmpDir, err := os.MkdirTemp(parent, ".fathom-recover-*")
os.Chmod(tmpDir, 0o700)
tmpDB := filepath.Join(tmpDir, base+".tmp")
```

All calls to `sqlite3` write to `tmpDB`, never `destPath`.

Properties:

- The destination path remains absent while recovery runs.
- The temp database is not attacker-replaceable unless the attacker already has
  access to the current process credentials and private temp dir.
- Temp and destination are on the same filesystem, enabling atomic publish
  primitives.

### 2. Reject pre-existing final destination by `Lstat`

Before starting work and immediately before publish:

```go
if _, err := os.Lstat(destPath); err == nil {
    return fmt.Errorf("destination already exists: %s", destPath)
} else if !os.IsNotExist(err) {
    return fmt.Errorf("check destination: %w", err)
}
```

Use `Lstat`, not `Stat`, so a symlink at the destination counts as existing and
is rejected.

This check alone is not the security boundary; final publish also needs
no-overwrite behavior.

### 3. Publish with no-overwrite semantics

Add helper:

```go
func publishNoReplace(tmpPath, destPath string) error
```

Preferred implementation order:

1. Linux: `renameat2(RENAME_NOREPLACE)` via `golang.org/x/sys/unix` when
   available. This is atomic and does not overwrite.
2. POSIX fallback: `os.Link(tmpPath, destPath)` followed by `os.Remove(tmpPath)`.
   `link(2)` fails if `destPath` already exists and does not follow a symlink at
   `destPath` because the destination name must be new.
3. Platform fallback: create a new destination with `O_CREATE|O_EXCL`, copy the
   temp database bytes into that already-open file, `fsync`, and close. This is
   no-overwrite but not rename-atomic; it is acceptable only on platforms where
   the stronger primitives are unavailable. The command must document this
   weaker fallback in verbose/debug output.

The first two paths should cover Linux/macOS operator environments. If Windows
support is in scope, add a Windows-specific `MoveFileEx`/`ReplaceFile` strategy
that refuses replacement.

### 4. Ensure temp database is a single-file SQLite DB before publish

Because recovery uses external SQLite commands, ensure the final DB is not
published while WAL or journal sidecars contain required state.

Before publish, run against `tmpDB`:

```sql
PRAGMA journal_mode=DELETE;
PRAGMA wal_checkpoint(TRUNCATE);
VACUUM;
PRAGMA integrity_check;
```

Then close all commands. The existing post-recovery diagnostics should run on
`tmpDB` before publish. If diagnostics fail, remove `tmpDir` and leave
`destPath` untouched.

### 5. Rewrite `runRecover` flow

New flow:

1. Resolve `sqlite3` binary.
2. Verify source exists.
3. Create parent and private temp dir.
4. Verify `destPath` does not exist with `Lstat`.
5. Run `.recover` from source into memory as today.
6. Replay sanitized SQL into `tmpDB`.
7. If no recovered SQL, create empty `tmpDB` with `sqlite3 tmpDB SELECT 1`.
8. Bootstrap schema, restore projections, count rows, and diagnose using `tmpDB`.
9. Normalize `tmpDB` to single-file SQLite.
10. Verify `destPath` still does not exist with `Lstat`.
11. Publish `tmpDB` to `destPath` with `publishNoReplace`.
12. Emit report referencing `destPath`.
13. Remove temp dir.

If any step before publish fails, `destPath` must remain absent. If publish
fails because the destination appeared concurrently, return a clear
`destination already exists` error and leave temp cleanup best-effort.

### 6. Error and cleanup behavior

Use a cleanup guard:

```go
published := false
defer func() {
    if !published {
        _ = os.Remove(destPath) // only if this process created fallback partial
    }
    _ = os.RemoveAll(tmpDir)
}()
```

The normal strong publish paths should never require removing `destPath` on
failure, because they either succeed completely or leave it absent. The fallback
copy path must track whether it created a partial file.

---

## Compatibility

CLI flags and output stay the same. The command may take slightly longer because
full diagnostics run before final publish and because `VACUUM` may rewrite the
temp database.

Operator-visible changes:

- A destination symlink is rejected as already existing.
- If another process creates the destination during recovery, recover fails at
  publish instead of overwriting or writing through it.
- Failed recovery no longer leaves a partial final destination except on the
  documented weakest platform fallback.

---

## Test Plan

Add unit tests for `publishNoReplace`:

- Publishes when destination is absent.
- Fails when destination file exists.
- Fails when destination symlink exists.
- Leaves source temp file intact or cleans predictably on failure.

Add recover command tests:

- Existing destination file still fails.
- Existing destination symlink fails.
- Destination created after recovery starts but before publish causes recover to
  fail without overwriting it. Use an injectable hook before publish.
- Successful recovery writes only after diagnostics pass. Use an injectable
  diagnostics failure to assert final destination remains absent.
- Temp directory is removed on success and failure.

Add platform tests:

- Linux path exercises `renameat2` when available, otherwise link fallback.
- Fallback copy path is covered behind an injectable strategy, not by depending
  on host platform limitations.

---

## HITL Gates

No immediate question blocks this design. Ask for human input during
implementation only if one of these choices affects release scope:

- Whether to add `golang.org/x/sys/unix` as a new Go dependency for
  `renameat2(RENAME_NOREPLACE)` or use `os.Link` as the primary POSIX path.
- Whether weaker non-atomic copy fallback is acceptable on unsupported platforms
  or should make recovery unsupported there.
- Whether pre-publish `VACUUM` is acceptable for very large recovered databases.
