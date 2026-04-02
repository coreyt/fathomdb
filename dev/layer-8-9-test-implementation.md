# Layer 8 & 9 Test Implementation Guide

Implementation plan for the crash recovery, durability, stress, and scale tests
defined in `dev/test-plan.md` Layers 8-9.

---

## New Test Files

| File | Language | Tests | CI Schedule |
|---|---|---|---|
| `crates/fathomdb/tests/crash_recovery.rs` | Rust | 8.1.1, 8.1.2, 8.1.3, 8.3.2 | Every PR |
| `crates/fathomdb/tests/scale.rs` | Rust | 9.1.1–9.1.4, 9.2.2 | Weekly (`#[ignore]`) |
| `python/tests/test_stress.py` | Python | 9.2.1 | Weekly |
| `go/fathom-integrity/test/e2e/disk_full_test.go` | Go | 8.3.1 | Weekly |

---

## Layer 8: Crash Recovery and Durability

### Key Technique: Raw rusqlite for WAL Simulation

The 8.1.x tests simulate crash state by writing via raw `rusqlite::Connection`
in WAL mode instead of through the Engine. This avoids the `DatabaseLock` flock
contention (raw rusqlite doesn't acquire fathomdb's lock file) and leaves
un-checkpointed WAL frames — the exact state after an unclean shutdown.

The Engine's `EngineRuntime::drop()` calls `sqlite3_close()` which triggers an
automatic WAL checkpoint. By using raw rusqlite (which also checkpoints on
close by default), we must ensure we leave WAL frames un-checkpointed. To do
this: open the raw connection, set `PRAGMA journal_mode=wal`, write data,
commit, then close normally. SQLite's automatic checkpoint on close only
checkpoints if the WAL exceeds `PRAGMA wal_autocheckpoint` pages (default
1000). For small writes (a few rows), the WAL frames remain un-checkpointed
after close, which is the desired state.

If the auto-checkpoint interferes, disable it with
`PRAGMA wal_autocheckpoint=0` before writing.

### 8.1.1: `reopen_after_unclean_shutdown_recovers_committed_data`

**File:** `crates/fathomdb/tests/crash_recovery.rs`
**CI:** Every PR
**Effort:** Small

```
Setup:
  1. NamedTempFile for DB path
  2. Open Engine, write node "meeting-1" + chunk, drop engine (clean shutdown)
  3. Open raw rusqlite::Connection on same path
  4. PRAGMA journal_mode=wal; PRAGMA wal_autocheckpoint=0
  5. INSERT node "meeting-2" + chunk + fts_nodes row via raw SQL
  6. COMMIT, close connection (WAL frames remain)
  7. Verify WAL file exists: {db_path}-wal
  8. Open Engine on same path (triggers WAL replay)

Assertions:
  - count_rows(db.path(), "nodes") == 2
  - active_count(db.path(), "nodes", "meeting-1") == 1
  - active_count(db.path(), "nodes", "meeting-2") == 1
  - check_integrity() returns physical_ok = true
```

**Reuse:** `helpers::count_rows()`, `helpers::active_count()`,
`helpers::exec_sql()` pattern for raw SQL.

### 8.1.2: `wal_replay_does_not_duplicate_fts_rows`

**File:** `crates/fathomdb/tests/crash_recovery.rs`
**CI:** Every PR
**Effort:** Small

```
Setup:
  1. Open Engine, write node "meeting-1" + chunk "chunk-1" (creates FTS row)
  2. Drop engine
  3. Open raw rusqlite::Connection, PRAGMA wal_autocheckpoint=0
  4. INSERT chunk "chunk-2" for "meeting-1" + FTS row via raw SQL
  5. Close connection (WAL frames remain)
  6. Open Engine (WAL replay)

Assertions:
  - fts_row_count(db.path(), "meeting-1") == 2 (not 3+)
  - Direct query: SELECT count(*) FROM fts_nodes WHERE chunk_id = 'chunk-1' == 1
  - Direct query: SELECT count(*) FROM fts_nodes WHERE chunk_id = 'chunk-2' == 1
```

### 8.1.3: `reopen_after_crash_mid_write_discards_uncommitted`

**File:** `crates/fathomdb/tests/crash_recovery.rs`
**CI:** Every PR
**Effort:** Small

```
Setup:
  1. Open Engine, write node "meeting-1" + chunk, drop engine
  2. Open raw rusqlite::Connection
  3. BEGIN IMMEDIATE
  4. INSERT node "meeting-2" (uncommitted transaction)
  5. Drop connection WITHOUT COMMIT (SQLite rolls back on close)
  6. Open Engine

Assertions:
  - active_count(db.path(), "nodes", "meeting-1") == 1
  - active_count(db.path(), "nodes", "meeting-2") == 0
  - count_rows(db.path(), "nodes") == 1
  - check_integrity() returns physical_ok = true
```

### 8.3.1: `write_on_full_disk_returns_error_not_corruption`

**File:** `go/fathom-integrity/test/e2e/disk_full_test.go`
**CI:** Weekly (benchmark-and-robustness)
**Effort:** Medium

```
Setup:
  1. t.TempDir() for base
  2. Create subdirectory as mount point
  3. exec.Command("mount", "-t", "tmpfs", "-o", "size=512K", "tmpfs", mountPoint)
     - t.Skip("requires root or tmpfs support") if mount fails
  4. t.Cleanup: exec.Command("umount", mountPoint)
  5. Bootstrap DB with makeBridgeScript() + bootstrapBridgeDB()
  6. Seed a small amount of data via bridge
  7. Create padding file to fill remaining space
  8. Attempt another write via bridge script

Assertions:
  - Write attempt returns non-zero exit code (error, not silent corruption)
  - Remove padding file
  - queryDB(t, dbPath, "SELECT count(*) FROM nodes") returns original count
  - queryDB(t, dbPath, "PRAGMA integrity_check") returns "ok"
```

**Reuse:** `makeBridgeScript()`, `bootstrapBridgeDB()`, `queryDB()` from
`recover_test.go`.

### 8.3.2: `checkpoint_failure_leaves_wal_intact`

**File:** `crates/fathomdb/tests/crash_recovery.rs`
**CI:** Every PR
**Effort:** Medium

```
Setup:
  1. NamedTempFile for DB path
  2. Open Engine, write node "meeting-1" + chunk, drop engine
  3. Open raw rusqlite::Connection, PRAGMA wal_autocheckpoint=0
  4. INSERT node "meeting-2" + chunk + fts row, COMMIT, close
  5. Verify WAL file exists and has content
  6. Set DB file to read-only: std::fs::set_permissions(db.path(), readonly)
  7. Open raw rusqlite::Connection (read-only)
  8. PRAGMA wal_checkpoint(TRUNCATE) — expect error or no-op
  9. Close connection
  10. Verify WAL file still exists (checkpoint didn't consume it)
  11. Restore write permissions
  12. Open Engine (WAL replay succeeds)

Assertions:
  - WAL file persists through failed checkpoint
  - After restoring permissions and reopening: both nodes present
  - check_integrity() returns physical_ok = true
```

**Note:** On some SQLite builds, opening a read-only DB file may fail or the
checkpoint may silently no-op rather than error. The test verifies the WAL is
preserved regardless of the specific failure mode.

---

## Layer 9: Stress and Scale

### Bulk Seeding Helper

All 9.1.x tests need a bulk seeding function. Define inline in `scale.rs`:

```rust
/// Seed `count` nodes with chunks, submitted in batches of `batch_size`.
/// Returns the list of logical_ids created.
fn seed_n_nodes_batched(
    engine: &Engine,
    count: usize,
    batch_size: usize,
) -> Vec<String> {
    let mut logical_ids = Vec::with_capacity(count);
    for batch_start in (0..count).step_by(batch_size) {
        let batch_end = (batch_start + batch_size).min(count);
        let mut nodes = Vec::new();
        let mut chunks = Vec::new();
        for i in batch_start..batch_end {
            let lid = format!("node-{i}");
            nodes.push(NodeInsert {
                row_id: format!("row-{i}"),
                logical_id: lid.clone(),
                kind: "Document".to_owned(),
                properties: format!(r#"{{"index":{i}}}"#),
                source_ref: Some(format!("seed-src-{i}")),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
            });
            chunks.push(ChunkInsert {
                id: format!("chunk-{i}"),
                node_logical_id: lid.clone(),
                text_content: format!("content for document {i}"),
                byte_start: None,
                byte_end: None,
            });
            logical_ids.push(lid);
        }
        engine.writer().submit(WriteRequest {
            label: format!("seed-batch-{batch_start}"),
            nodes,
            chunks,
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        }).expect("seed batch");
    }
    logical_ids
}
```

Modeled on `production_paths.rs:22` `single_node_chunk_request()`.

A variant for FTS accuracy testing:

```rust
/// Like seed_n_nodes_batched, but every `marker_interval`th node
/// contains `marker_word` in its chunk text.
fn seed_n_nodes_with_marker(
    engine: &Engine,
    count: usize,
    batch_size: usize,
    marker_word: &str,
    marker_interval: usize,
) -> Vec<String> { /* same structure, conditional text content */ }
```

### 9.1.1: `write_and_query_10k_nodes`

**File:** `crates/fathomdb/tests/scale.rs`
**CI:** Weekly (`#[ignore]`)
**Effort:** Medium

```
Setup:
  1. Open Engine
  2. seed_n_nodes_batched(&engine, 10_000, 50) — 200 batch requests

Assertions:
  - count_rows(db.path(), "nodes") == 10_000
  - count_rows(db.path(), "chunks") == 10_000
  - count_rows(db.path(), "fts_nodes") == 10_000
  - FTS text_search for "document 42" returns results
  - check_integrity(): physical_ok = true, missing_fts_rows = 0
```

### 9.1.2: `supersession_chain_depth_100`

**File:** `crates/fathomdb/tests/scale.rs`
**CI:** Weekly (`#[ignore]`)
**Effort:** Small

```
Setup:
  1. Open Engine
  2. Submit initial NodeInsert: logical_id = "chain-node", upsert = false,
     chunk_policy = Preserve, properties = {"version": 0}
  3. Loop 1..100: submit same logical_id with upsert = true,
     ChunkPolicy::Replace, new chunk text, properties = {"version": N}

Assertions:
  - active_count(db.path(), "nodes", "chain-node") == 1
  - historical_count(db.path(), "nodes", "chain-node") == 99
  - active_properties(db.path(), "chain-node") contains "version":99
  - fts_row_count(db.path(), "chain-node") == 1 (only active)
  - check_integrity() passes
```

**Reuse:** `helpers::active_count()`, `helpers::historical_count()`,
`helpers::active_properties()`, `helpers::fts_row_count()`.

### 9.1.3: `fts_search_accuracy_at_scale`

**File:** `crates/fathomdb/tests/scale.rs`
**CI:** Weekly (`#[ignore]`)
**Effort:** Medium

```
Setup:
  1. Open Engine
  2. seed_n_nodes_with_marker(&engine, 10_000, 50, "thermodynamics", 100)
     — nodes 0, 100, 200, ..., 9900 contain "thermodynamics" (100 total)

Assertions:
  - FTS text_search("thermodynamics") returns exactly 100 results
  - All 100 returned logical_ids match expected set {node-0, node-100, ...}
  - Precision = 1.0 (no false positives)
  - Recall = 1.0 (no false negatives)
  - check_integrity() passes
```

### 9.1.4: `rebuild_projections_at_scale`

**File:** `crates/fathomdb/tests/scale.rs`
**CI:** Weekly (`#[ignore]`)
**Effort:** Medium

```
Setup:
  1. Open Engine, seed 10,000 nodes with chunks
  2. Drop engine
  3. injection::delete_all_fts_rows(db.path())
  4. Reopen engine
  5. check_integrity() → missing_fts_rows == 10_000
  6. admin.rebuild_projections(ProjectionTarget::Fts)

Assertions:
  - After rebuild: check_integrity() → missing_fts_rows == 0
  - FTS search for known term returns correct results
  - Rebuild completes without timeout
```

**Reuse:** `injection::delete_all_fts_rows()`.

### 9.2.1: `sustained_concurrent_reads_under_write_load`

**File:** `python/tests/test_stress.py` (Python) + `crates/fathomdb/tests/scale.rs` (Rust)
**CI:** Weekly
**Effort:** Large

#### Python Implementation

```python
import threading
import time
from fathomdb import Engine, WriteRequest, NodeInsert, ChunkInsert, new_row_id

def test_sustained_concurrent_reads_under_write_load(tmp_path):
    engine = Engine.open(str(tmp_path / "stress.db"))
    # Seed initial data so readers have something to query
    engine.write(_make_write("seed-0"))

    errors = []
    stop = threading.Event()
    write_count = [0]
    read_count = [0]

    def writer(thread_id):
        i = 0
        while not stop.is_set():
            try:
                engine.write(_make_write(f"w{thread_id}-{i}"))
                write_count[0] += 1
                i += 1
            except Exception as e:
                errors.append(("write", thread_id, e))

    def reader(thread_id):
        while not stop.is_set():
            try:
                engine.nodes("Document").limit(10).execute()
                read_count[0] += 1
            except Exception as e:
                errors.append(("read", thread_id, e))

    writers = [threading.Thread(target=writer, args=(i,)) for i in range(5)]
    readers = [threading.Thread(target=reader, args=(i,)) for i in range(20)]
    for t in writers + readers:
        t.start()

    time.sleep(60)
    stop.set()

    for t in writers + readers:
        t.join(timeout=15)
        assert not t.is_alive(), f"Thread {t.name} hung"

    assert errors == [], f"Errors during stress test: {errors}"
    assert write_count[0] > 0, "No writes completed"
    assert read_count[0] > 0, "No reads completed"

    report = engine.admin.check_integrity()
    assert report["physical_ok"]
    engine.close()
```

**Reuse:** `_make_write()` helper from `test_concurrency_deadlocks.py` (copy
into `test_stress.py` since test files are self-contained).

#### Rust Implementation

```
Setup:
  1. Open Engine, wrap in Arc<Engine>
  2. Seed 100 nodes
  3. Spawn 5 writer threads + 20 reader threads
  4. Run for 60 seconds (Instant::now() + Duration)
  5. Signal stop via AtomicBool
  6. Join all threads with timeout

Assertions:
  - Zero errors from any thread
  - All threads joined (no hangs)
  - check_integrity() passes
  - write_count > 0, read_count > 0
```

### 9.2.2: `check_integrity_during_active_writes`

**File:** `crates/fathomdb/tests/scale.rs`
**CI:** Weekly (`#[ignore]`)
**Effort:** Medium

```
Setup:
  1. Open Engine, seed 100 nodes
  2. Spawn writer thread: continuously submit writes for 10 seconds
  3. Main thread: call check_integrity() in a loop during the write window
  4. Join writer thread

Assertions:
  - Every check_integrity() call succeeds (no EngineError)
  - physical_ok = true on every call
  - Writer thread encountered no errors
  - At least 5 integrity checks completed (not blocked by writer)
```

---

## CI Integration

### Changes to `.github/workflows/benchmark-and-robustness.yml`

Add after `go-fuzz-smoke` job:

```yaml
  rust-scale-tests:
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2
      - uses: dtolnay/rust-toolchain@3c5f7ea28cd621ae0bf5283f0e981fb97b8a7af9 # stable
        with:
          toolchain: stable
      - uses: taiki-e/install-action@e9e8e031bcd90cdbe8ac6bb1d376f8596e587fbf # v2.70.2
        with:
          tool: cargo-nextest
      - uses: Swatinem/rust-cache@c19371144df3bb44fab255c43d04cbc2ab54d1c4 # v2.9.1
      - run: cargo nextest run --workspace --run-ignored=only

  python-stress-tests:
    runs-on: ubuntu-latest
    timeout-minutes: 5
    steps:
      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2
      - uses: actions/setup-python@a309ff8b426b58ec0e2a45f0f869d46889d02405 # v6.2.0
        with:
          python-version: "3.11"
      - uses: dtolnay/rust-toolchain@3c5f7ea28cd621ae0bf5283f0e981fb97b8a7af9 # stable
        with:
          toolchain: stable
      - uses: mozilla-actions/sccache-action@d651010b8da762cde178750d8eda7b5febfe147a # v0.0.9
      - run: python -m pip install --upgrade pip maturin pytest pytest-timeout
      - run: python -m pip install -e python --no-build-isolation
      - run: PYTHONPATH=python pytest python/tests/test_stress.py -v --timeout=120
```

### Test Schedule Summary

| Test | CI Schedule | Timeout |
|---|---|---|
| 8.1.1 reopen after unclean shutdown | Every PR (`cargo nextest run`) | default |
| 8.1.2 WAL replay FTS dedup | Every PR | default |
| 8.1.3 discard uncommitted | Every PR | default |
| 8.3.2 checkpoint failure | Every PR | default |
| 8.3.1 write on full disk | Weekly (Go, needs tmpfs/root) | 5 min |
| 9.1.1 write and query 10k nodes | Weekly (`#[ignore]`) | 15 min |
| 9.1.2 supersession chain depth 100 | Weekly (`#[ignore]`) | 15 min |
| 9.1.3 FTS accuracy at scale | Weekly (`#[ignore]`) | 15 min |
| 9.1.4 rebuild projections at scale | Weekly (`#[ignore]`) | 15 min |
| 9.2.1 sustained concurrent load | Weekly (Python + Rust) | 2 min (Python) |
| 9.2.2 check_integrity during writes | Weekly (`#[ignore]`) | 15 min |

---

## Existing Code to Reuse

| What | File | Used By |
|---|---|---|
| Engine open/reopen pattern | `tests/last_access_touch.rs:309` | 8.1.x |
| `single_node_chunk_request()` | `benches/production_paths.rs:22` | 9.1.x seeding |
| `count_rows()`, `fts_row_count()`, `active_count()` | `tests/helpers.rs:12-75` | All Rust tests |
| `active_properties()`, `historical_count()` | `tests/helpers.rs:32-53` | 9.1.2 |
| `delete_all_fts_rows()` | `tests/injection.rs` | 9.1.4 |
| `exec_sql()` | `tests/helpers.rs:79` | 8.1.x raw SQL |
| `_make_write()` + threading | `test_concurrency_deadlocks.py` | 9.2.1 Python |
| `makeBridgeScript()`, `bootstrapBridgeDB()` | `test/e2e/recover_test.go` | 8.3.1 |
| `queryDB()` | `test/e2e/recover_test.go` | 8.3.1 |

---

## Phased Implementation Order

### Phase 1: Crash Recovery (fast tests, every PR)

1. Create `crates/fathomdb/tests/crash_recovery.rs`
2. Implement 8.1.1 `reopen_after_unclean_shutdown_recovers_committed_data`
3. Implement 8.1.2 `wal_replay_does_not_duplicate_fts_rows`
4. Implement 8.1.3 `reopen_after_crash_mid_write_discards_uncommitted`
5. Implement 8.3.2 `checkpoint_failure_leaves_wal_intact`
6. Run `cargo nextest run -p fathomdb` — verify all 4 pass

### Phase 2: Scale Infrastructure and Tests

7. Create `crates/fathomdb/tests/scale.rs`
8. Implement `seed_n_nodes_batched()` and `seed_n_nodes_with_marker()` helpers
9. Implement 9.1.2 `supersession_chain_depth_100` (fastest to verify)
10. Implement 9.1.1 `write_and_query_10k_nodes`
11. Implement 9.1.3 `fts_search_accuracy_at_scale`
12. Implement 9.1.4 `rebuild_projections_at_scale`
13. Run `cargo nextest run -p fathomdb --run-ignored=only` — verify all pass

### Phase 3: Concurrent Load and Disk-Full

14. Implement 9.2.2 `check_integrity_during_active_writes` in `scale.rs`
15. Create `python/tests/test_stress.py`
16. Implement 9.2.1 Python `sustained_concurrent_reads_under_write_load`
17. Implement 9.2.1 Rust version in `scale.rs`
18. Create `go/fathom-integrity/test/e2e/disk_full_test.go`
19. Implement 8.3.1 `write_on_full_disk_returns_error_not_corruption`

### Phase 4: CI Wiring

20. Add `rust-scale-tests` job to `benchmark-and-robustness.yml`
21. Add `python-stress-tests` job to `benchmark-and-robustness.yml`
22. Update `dev/test-plan.md` status markers for all implemented tests
