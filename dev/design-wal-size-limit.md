# Design: WAL Size Limit

## Purpose

Address the verified finding that no WAL size limit is configured (H-2).
Without `journal_size_limit`, the WAL file can grow to fill the disk if
long-running readers prevent checkpointing.

---

## Current State

`crates/fathomdb-schema/src/bootstrap.rs:720-724`

The connection initialization sets:
```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;
PRAGMA temp_store = MEMORY;
PRAGMA mmap_size = 3000000000;
```

No `journal_size_limit` or `wal_autocheckpoint` is configured.

---

## Design

### Add `journal_size_limit`

Append to the initialization PRAGMAs:

```sql
PRAGMA journal_size_limit = 536870912;  -- 512 MB
```

This tells SQLite to truncate the WAL file to at most 512 MB after each
successful checkpoint. It does not prevent the WAL from growing beyond
this during active writes — it only controls post-checkpoint truncation.

512 MB is a conservative ceiling for a local agent datastore. Most WAL
files will be well under 4 MB (the default `wal_autocheckpoint` threshold
of 1000 pages at 4 KB each).

### Keep default `wal_autocheckpoint`

The default `wal_autocheckpoint` of 1000 pages (~4 MB) is appropriate for
this workload. The engine has a single writer and typically few concurrent
readers. Increasing `wal_autocheckpoint` would delay checkpointing and
increase WAL growth under write bursts with no clear benefit.

Do not change `wal_autocheckpoint` unless production profiling reveals
checkpoint frequency as a bottleneck.

### Interaction with `safe_export`

`safe_export` calls `PRAGMA wal_checkpoint(FULL)` explicitly before
backup. The `journal_size_limit` does not interfere — it only governs
automatic post-checkpoint truncation, not explicit checkpoint calls.

---

## Test Plan

- Verify `PRAGMA journal_size_limit` returns the configured value after
  connection initialization.
- No behavioral test needed for WAL truncation — this is SQLite-internal
  behavior. The value is advisory and best-effort.
