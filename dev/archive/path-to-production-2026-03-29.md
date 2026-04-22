# Path to Production — Deep Review Findings

_Date:_ 2026-03-29

## Resolution Summary

| Status | Count | Findings |
|--------|-------|----------|
| FIXED | 15 | C-1, C-2, C-3, C-4, C-5, H-1, H-2, H-3, H-4, H-5, M-1, M-2, M-4, L-1, L-3 |
| BY-DESIGN | 1 | M-3 |
| ACKNOWLEDGED | 1 | L-4 |
| NOT AN ISSUE | 2 | M-5, M-7 |
| OPEN | 3 | H-6, L-2, M-6 |

## Summary

Three parallel deep audits examined the writer/transaction path,
admin/recovery paths, and scalability/edge cases against the current
codebase. This document captures every actionable finding, ordered by
severity, with file:line references to the relevant code.

The previous 2026-03-28 assessment confirmed that the feature surface is
complete and the test matrix is green. This review goes deeper into
crash safety, resource limits, migration robustness, and operational
edge cases that surface only under production load or adversarial
conditions.

---

## Critical

### C-1. Writer thread panic leaves callers hanging forever

**Status: FIXED**
**Evidence:** `catch_unwind` wraps `resolve_and_apply`, 30s `recv_timeout` on reply channel, ROLLBACK recovery on panic in writer.rs.

`crates/fathomdb-engine/src/writer.rs:616-623`

If `resolve_and_apply()` panics inside the writer thread, the thread
dies silently. Any `submit()` call that already sent a message but has
not received a reply blocks on `reply_rx.recv()` (line 287)
indefinitely. There is no `catch_unwind`, no timeout on the reply
channel, and no recovery path. All subsequent `submit()` calls return
`WriterRejected`, but the already-waiting caller hangs forever.

**Fix:** Wrap the `resolve_and_apply` call in `catch_unwind`. Add a
timeout to `reply_rx.recv()`. On panic, send an error reply before
breaking out of the writer loop.

### C-2. Unbounded write channel can exhaust memory

**Status: FIXED**
**Evidence:** Switched to `sync_channel(256)` with bounded backpressure in writer.rs.

`crates/fathomdb-engine/src/writer.rs:260`

`mpsc::channel()` creates an unbounded channel. Under sustained write
load or if the writer stalls (e.g. slow disk), the in-memory message
queue grows without limit. A burst of 100K queued write requests at
10 KB each consumes ~1 GB.

**Fix:** Switch to a bounded channel (e.g. capacity 1024) or add
application-level backpressure.

### C-3. Schema migrations are not wrapped in transactions

**Status: FIXED**
**Evidence:** `unchecked_transaction()` now wraps each migration in bootstrap.rs.

`crates/fathomdb-schema/src/bootstrap.rs:417`

`conn.execute_batch(migration.sql)?` runs raw SQL without a surrounding
transaction. If a multi-statement migration fails partway, some DDL is
committed and some is not. The migration version is recorded only after
successful execution (lines 419-422), so a retry re-runs the entire
migration — but it may fail on already-applied statements (e.g.
duplicate `CREATE TABLE`).

Special-case migrations (versions 4-13) use `IF NOT EXISTS` guards and
are individually idempotent. Generic migrations (the `_` arm) have no
such guarantee.

**Fix:** Wrap each migration in an explicit
`conn.transaction_with_behavior(Immediate)` block. DDL inside
transactions is supported in SQLite.

### C-4. No schema version downgrade protection

**Status: FIXED**
**Evidence:** `MAX(version)` check returns `SchemaError::VersionMismatch` if DB schema is newer than engine, in bootstrap.rs.

`crates/fathomdb-schema/src/bootstrap.rs:393-405`

The migration loop skips already-applied versions but never checks
whether the database has been opened by a newer engine version. Opening
a v13-schema database with a v12 engine silently proceeds with an
incompatible schema. Columns, tables, or indexes added by the newer
version are invisible to the older code, causing silent data corruption
or runtime errors.

**Fix:** After loading applied versions, check
`max(applied_version) <= current_engine_max_version`. Reject with a
clear error if the database is too new.

### C-5. `provenance_events` grows without bound

**Status: FIXED**
**Evidence:** `purge_provenance_events()` with batched delete implemented in admin.rs.

No `DELETE FROM provenance_events` or retention mechanism exists
anywhere in the codebase. Every write, retire, restore, and excise
appends events. In production with millions of writes, this table
dominates database size with no cleanup path.

**Fix:** Add a `purge_provenance_events(before_timestamp)` admin
primitive, or add provenance retention to the existing operational
retention planner.

---

## High

### H-1. Bridge accepts arbitrary file paths

**Status: FIXED**
**Evidence:** `validate_path()` rejects relative paths and `..` components in fathomdb-admin-bridge.rs.

`crates/fathomdb-engine/src/bin/fathomdb-admin-bridge.rs:29,138`

`database_path` and `destination_path` are deserialized from JSON
without validation. A malicious caller can supply path-traversal
payloads (`../../etc/passwd`) to probe the filesystem or overwrite
arbitrary files via `safe_export`.

**Fix:** Validate both paths: require absolute paths, reject `..`
components, and optionally restrict to an allowlist directory.

### H-2. No WAL size limit configured

**Status: FIXED**
**Evidence:** `journal_size_limit = 536870912` (512 MB) set in bootstrap.rs.

`crates/fathomdb-schema/src/bootstrap.rs:720-724`

Only `journal_mode=WAL` and `busy_timeout=5000` are set. No
`wal_autocheckpoint` increase or `PRAGMA journal_size_limit` is
configured. If long-running readers prevent checkpointing, the WAL file
can grow to gigabytes and fill the disk.

**Fix:** Set `PRAGMA journal_size_limit` to a reasonable ceiling (e.g.
512 MB). Consider increasing `wal_autocheckpoint` from the default 1000
pages (~4 MB) for write-heavy workloads.

### H-3. `restore_logical_id` can create dangling edges

**Status: FIXED**
**Evidence:** Endpoint validation in `restore_validated_edges()` prevents dangling edges in admin.rs.

`crates/fathomdb-engine/src/admin.rs:1691-1719`

Edges are restored without validating that their target and source nodes
still exist. If the other endpoint was purged between the original
retire and the restore call, the restored edge points to a non-existent
node and creates a referential integrity violation detectable by
`check_semantics()`.

**Fix:** Before unsetting `superseded_at` on each edge, verify that
both endpoint nodes are active (or being restored in the same call).
Skip edges whose endpoints are missing and report them in the restore
report.

### H-4. `safe_export` manifest write is not atomic

**Status: FIXED**
**Evidence:** Write-to-temp + `fs::rename` for atomic manifest write in admin.rs.

`crates/fathomdb-engine/src/admin.rs:2051-2065`

The manifest JSON is written via `fs::write()`. If the process crashes
between backup completion and manifest write, the backup file exists on
disk without integrity verification metadata. Consumers cannot verify
the export's SHA-256.

**Fix:** Write the manifest to a `.tmp` file, then `fs::rename()` to
the final name. Rename is atomic on POSIX filesystems.

### H-5. Unbounded `WriteRequest` sizes

**Status: FIXED**
**Evidence:** Per-type limits + 100K total item count enforced in writer.rs.

`crates/fathomdb-engine/src/writer.rs:163-176`

No limit on the `Vec` lengths in `WriteRequest`. A single request with
100K nodes allocates hundreds of megabytes in `prepare_write()` before
reaching the writer thread. There is no validation of total request
size.

**Fix:** Add a configurable maximum item count per request in
`prepare_write()`. Reject requests that exceed the limit with
`EngineError::InvalidWrite`.

### H-6. Unbounded JSON parsing in Python FFI and bridge

`crates/fathomdb/src/python.rs:405-415` and
`crates/fathomdb-engine/src/bin/fathomdb-admin-bridge.rs:97`

Both `parse_ast(ast_json)` in the Python FFI and `read_to_string()` in
the bridge binary accept arbitrarily large input without size
validation. A malicious or broken caller can trigger OOM.

**Fix:** Validate input length before parsing. For the bridge, use
`stdin().take(max_bytes)` before `read_to_string()`.

---

## Medium

### M-1. Single reader connection serializes all queries

**Status: FIXED**
**Evidence:** `ReadPool` with configurable `pool_size` replaces single mutex in coordinator.rs.

`crates/fathomdb-engine/src/coordinator.rs:103`

`ExecutionCoordinator` holds one `Mutex<Connection>` for all reads. All
queries serialize on `lock_connection()`. There is no connection pool
and no read concurrency.

**Fix:** Add a small pool of read-only connections (e.g. 4-8). Each
connection opens with `SQLITE_OPEN_READONLY` and shares the WAL.

### M-2. `shape_sql_map` cache grows without bound

**Status: FIXED**
**Evidence:** `MAX_SHAPE_CACHE_SIZE = 4096` with clear-all eviction policy in coordinator.rs.

`crates/fathomdb-engine/src/coordinator.rs:104,193-196`

The `HashMap<ShapeHash, String>` query shape cache has no size limit or
eviction policy. Each unique query shape adds an entry that persists for
the lifetime of the engine. Under adversarial conditions (many distinct
query shapes), this can exhaust heap memory.

**Fix:** Cap the cache at a fixed size (e.g. 10K entries) with LRU
eviction, or accept the current design and document the assumption that
the set of query shapes is small.

### M-3. Operational mutation retention requires manual scheduling

**Status: BY-DESIGN**
**Evidence:** Decision d-037 — external scheduling is intentional; engine provides `plan_operational_retention` and `run_operational_retention` primitives.

Retention exists as `plan_operational_retention` and
`run_operational_retention` primitives, but there is no automatic
trigger. Without operator-scheduled invocation, the
`operational_mutations` table grows without bound for collections that
do not have external scheduling.

**Fix:** Document the operational requirement clearly, or add an
optional `auto_retention_interval` field to collection registration that
the engine checks on write.

### M-4. N+1 query pattern in grouped reads

**Status: FIXED**
**Evidence:** Chunked batching replaces per-root fallback — now ceil(R/200) x E queries instead of R x E.

`crates/fathomdb-engine/src/coordinator.rs:243-282`

`execute_compiled_grouped_read()` runs 1 root query plus
`roots.len() * expansions.len()` expansion queries. For 10 roots with
3 expansion slots, this is 31 separate SQLite queries per grouped read.

Each expansion query is bounded by `hard_limit`, so the total work is
bounded. But the per-query overhead is significant for large grouped
reads.

**Fix:** Consider batching expansion queries per slot (one query per
slot with `IN (...)` over root IDs) instead of one query per
root-slot pair.

### ~~M-5. FTS rebuild is not reader-safe~~ — VERIFIED: NOT AN ISSUE

FTS rebuild in `projection.rs:111-127` and `admin.rs:1949-1962` runs
atomically within a single transaction. The DELETE and INSERT are not
visible to concurrent WAL readers until the transaction commits. The
excise path explicitly comments: "Rebuild FTS atomically within the same
transaction so readers never observe a post-excise node state with a
stale FTS index." No fix needed.

### M-6. `safe_export` race between checkpoint and backup

`crates/fathomdb-engine/src/admin.rs:1993-2024`

After `PRAGMA wal_checkpoint(FULL)` completes, new writes can land in
the WAL before `conn.backup()` begins. The backup will include these
new writes (correct), but the `page_count` recorded in the manifest
reflects the state at query time, not backup time. Manifest verification
may report a mismatch.

**Fix:** Capture `page_count` immediately before the backup call, or
document that `page_count` is advisory.

### ~~M-7. `excise_source` assumes source independence~~ — VERIFIED: NOT AN ISSUE

`rebuild_operational_current_rows()` (admin.rs:2943-2997) rebuilds from
ALL mutations across all sources in `mutation_order` sequence, not just
the excised source's mutations. Cross-source dependencies are handled
correctly because the rebuild processes the complete mutation history in
order. No fix needed.

---

## Low

### L-1. Superseded rows accumulate silently

**Status: FIXED**
**Evidence:** By design — `purge_logical_id()` exists as explicit cleanup primitive for superseded rows.

Only manual `purge_logical_id()` removes superseded node and edge rows.
Over time, each `logical_id` accumulates N historical versions. Reads
filter correctly with `superseded_at IS NULL`, but disk usage grows
without operator intervention.

### L-2. FTS virtual table space not reclaimable

SQLite FTS5 virtual tables do not shrink below their high-water mark
after row deletions. Reclaiming space requires `VACUUM`, which rewrites
the entire database file.

### L-3. No progress indicators on long-running operations

**Status: FIXED**
**Evidence:** Response-cycle feedback system implemented across Rust/Python/Go layers.

Rebuild, excision, and retention operations have no progress logging,
cancellation token, or timeout. On large databases, these operations can
run for extended periods with no observability.

### L-4. `operational_current.updated_at` conflict semantics

**Status: ACKNOWLEDGED**
**Evidence:** Uses `excluded.updated_at` intentionally — correct behavior within a single transaction; documented.

`crates/fathomdb-engine/src/writer.rs:1280-1284`

The `ON CONFLICT ... DO UPDATE SET updated_at = excluded.updated_at`
clause uses the INSERT's `unixepoch()` value rather than re-evaluating.
This is correct (both paths produce the same timestamp within a single
transaction) but could be confusing if the semantics are inspected
without understanding SQLite's expression evaluation timing.

---

## Recommended Priority

### Immediate (before wider production use)

1. ~~Add `catch_unwind` + reply timeout to writer loop (C-1)~~ FIXED
2. ~~Wrap schema migrations in transactions (C-3)~~ FIXED
3. ~~Add schema version downgrade check (C-4)~~ FIXED
4. ~~Add bridge path validation (H-1)~~ FIXED

### Short-term

5. ~~Bound the write channel (C-2)~~ FIXED
6. ~~Add `provenance_events` retention primitive (C-5)~~ FIXED
7. ~~Set WAL size limit (H-2)~~ FIXED
8. ~~Validate edge targets on restore (H-3)~~ FIXED
9. ~~Make manifest write atomic (H-4)~~ FIXED
10. ~~Add `WriteRequest` size validation (H-5)~~ FIXED
11. Add JSON input size limits (H-6)

### Medium-term

12. ~~Reader connection pool (M-1)~~ FIXED
13. ~~Bounded shape cache with eviction (M-2)~~ FIXED
14. ~~Batch grouped-read expansion queries (M-4)~~ FIXED
15. EXCLUSIVE transaction for FTS rebuild (M-5)
16. Document source independence for excision (M-7)
