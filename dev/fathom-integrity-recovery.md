# fathom-integrity: Recovery Design

## Directive

`fathom-integrity` must be **world-class** at SQLite and SQLite+fathomdb data
recovery. It must detect, diagnose, and repair problems at every layer — from
raw storage bits through engine invariants through application-level semantics.
The Go tool must *exceed* what the Rust engine exposes on its own.

The Rust engine (`fathomdb`) is built intentionally for recoverability:
every write carries provenance, excision is a first-class primitive, projection
rebuild is deterministic. The Go tool orchestrates the full recovery pipeline
on top of that foundation.

These are hard constraints, not aspirational goals.

---

## Layer Model

Recovery work is organized into three layers. Each requires different detection
techniques and different repair paths. A world-class `check` command runs all
three and classifies findings by layer and severity.

```
Layer 3: Application semantics      ← fathomdb invariants, provenance, chains
Layer 2: Engine invariants          ← projection sync, active-row uniqueness
Layer 1: SQLite storage             ← file structure, B-tree, WAL, pragmas
```

Problems at Layer 1 can mask problems at Layers 2 and 3. A check run always
starts at Layer 1 and works upward. If Layer 1 is critically corrupted the
tool reports that and does not pretend higher-layer checks are meaningful.

---

## Layer 1: SQLite Storage

### What can go wrong

**File-level:**
- Header magic bytes corrupted (bytes 0–15 ≠ `"SQLite format 3\0"`) — immediate open failure
- Page size field invalid (bytes 16–17 not a power of 2 in 512–65536) — unreadable
- File truncated (size not a multiple of declared page size) — partial last page lost
- File too small (< 100 bytes — minimum SQLite header) — cannot be a valid database

**B-tree structure:**
- Page type byte invalid (not 0x02 / 0x05 / 0x0A / 0x0D) — corrupt on access
- Cell count set too high — reads past valid cells, detected by `integrity_check`
- Cell count set too low — silently hides rows, NOT detected by `integrity_check`
- Cell offset array pointing outside valid cell content area
- Interior page right-child pointer corrupt — misdirects B-tree traversal
- Overflow page chain corrupt — loop or invalid pointer
- Freelist trunk/leaf page corrupt — double-allocation or leaked pages
- Index content diverges from table content — silent wrong query results
- Duplicate rowids in a table B-tree

**WAL (Write-Ahead Log):**
- WAL header magic wrong (`0x377F0682` / `0x377F0683`) — WAL ignored entirely
- WAL salt mismatch — all frames appear invalid, silent rollback to pre-WAL state
- Single bit flip in a WAL frame — **silent truncation of all subsequent frames**,
  including committed transactions. This is the most dangerous known SQLite failure
  mode: committed data is silently discarded with no error reported.
- WAL-reset bug (CVE-equivalent, affected 3.7.0–3.51.2, fixed 3.51.3 / 2026-03-03)
- Checkpoint torn write — page partially overwritten in main file during checkpoint

**Journal (rollback mode):**
- Hot journal deleted after crash — rollback never applied, partially-written state persists
- Journal from wrong database applied — wrong undo log, silent corruption

### Detection methods

| Check | How | Detects |
|---|---|---|
| Header magic + page size | Read bytes 0–17 directly | Open failures |
| File size alignment | `stat` size mod page size | Truncation |
| WAL file presence | `stat <db>-wal` | Active WAL |
| WAL header magic | Read bytes 0–3 of WAL file | WAL unreadable |
| WAL frame count | Parse WAL header frame count | Unflushed frames |
| `PRAGMA integrity_check` | Via `database/sql` + go-sqlite3 | Structural corruption |
| `PRAGMA quick_check` | Same (faster, skips index cross-check) | Structural corruption |
| `PRAGMA foreign_key_check` | Same | FK violations |
| `PRAGMA page_count` + file size | Cross-check declared vs actual | Header/file mismatch |

### What `integrity_check` misses

- Cell count set *too low* (rows silently hidden)
- Silent value-level bit flips that preserve B-tree structure
- Application-level semantic invariants (see Layer 3)
- WAL content (operates only on checkpointed data)
- Pointer map correctness for auto-vacuum databases

### Severity classification

| Finding | Severity |
|---|---|
| Header magic corrupt | Critical — database unreadable |
| File truncated | Critical |
| `integrity_check` returns anything other than `ok` | Critical |
| WAL present but header magic wrong | Critical |
| WAL frames present with no checkpoint | Warning |
| FK violations | Error |
| Any `integrity_check` row other than `ok` | Error |
| File size small but header valid | Warning |

---

## Layer 2: Engine Invariants

These are fathomdb-specific invariants enforced by the engine schema. The
Rust bridge exposes these via `check_integrity`. The Go tool must call the
bridge and surface the results as part of its check output.

| Invariant | Detection | Current state |
|---|---|---|
| Missing FTS rows | `check_integrity` payload `missing_fts_rows` | ✅ implemented |
| Duplicate active logical_ids | `check_integrity` payload `duplicate_active_logical_ids` | ✅ implemented |
| FK violations (bridge-side) | `check_integrity` payload `foreign_keys_ok` | ✅ implemented |
| Physical integrity | `check_integrity` payload `physical_ok` | ✅ implemented |

### Severity classification

| Finding | Severity |
|---|---|
| `physical_ok: false` | Critical |
| `foreign_keys_ok: false` | Error |
| `duplicate_active_logical_ids > 0` | Error |
| `missing_fts_rows > 0` | Warning (repairable) |

---

## Layer 3: Application Semantics

These invariants are invisible to SQLite tools and require fathomdb-aware
queries. The Go tool must check these directly against the database.

### Invariants to check

**Supersession chain integrity:**
- Active rows whose `logical_id` has no row with `superseded_at IS NULL` (orphaned
  supersession: the prior version was superseded but no new version was written)
- Superseded rows whose `superseded_at` timestamp predates their own `created_at`
  (clock regression or corruption of timestamps)
- `source_ref` NULL on nodes/edges/actions that should be traceable — provenance
  gap means excise cannot target these rows

**Projection consistency:**
- FTS rows whose `chunk_id` references a chunk that no longer exists (stale FTS row)
- FTS rows whose `node_logical_id` references a node that is superseded (FTS not
  cleaned up after supersession — text search returns dead content)
- Chunks whose `node_logical_id` has no active node (orphaned chunks — no node to
  attach them to)

**Runtime table chain integrity:**
- Steps whose `run_id` references a run that does not exist
- Actions whose `step_id` references a step that does not exist
- After excision: runs/steps/actions whose source was excised but whose FK targets
  were not also excised (partially-excised provenance chain)

**Provenance coverage:**
- Nodes with `source_ref IS NULL` — these cannot be excised by source
- Actions with `source_ref IS NULL` — same
- Fraction of rows without provenance is a health metric

### Severity classification

| Finding | Severity |
|---|---|
| Stale FTS rows (pointing to non-existent chunks) | Error |
| FTS rows pointing to superseded nodes | Warning |
| Orphaned chunks (no active node) | Warning |
| Broken runtime table FK chain (step without run) | Error |
| NULL `source_ref` on nodes/actions | Warning (provenance gap) |
| Superseded rows with no active successor | Info (may be intentional) |

---

## Corruption Injection Test Harness

A dedicated Go test package (`test/corrupt/`) that can reproducibly inject
specific corruption types into a test database. Required for TDD of the deep
`check` command — every detection claim must have a failing test that proves
the corruption is detectable and a passing test after the repair.

### Storage-level injections

```go
// Corrupt the SQLite header magic bytes
InjectHeaderCorruption(path string) error

// Truncate the file to lose the last page
InjectTruncation(path string) error

// Flip a single bit in WAL frame N (silent data loss scenario)
InjectWALBitFlip(walPath string, frameIndex int, byteOffset int, bitPosition uint8) error

// Delete the WAL file while it has unflushed committed frames
InjectWALDelete(walPath string) error

// Overwrite a data page with zeros (page N, 1-indexed)
InjectZeroPage(path string, pageNumber int) error

// Corrupt the page type byte of a B-tree page
InjectPageTypeCorruption(path string, pageNumber int) error
```

### Engine-level injections

```go
// Delete all FTS rows (missing projection)
InjectFTSDeletion(db *sql.DB) error

// Insert a stale FTS row pointing to a non-existent chunk
InjectStaleFTSRow(db *sql.DB) error

// Insert FTS rows for a superseded node
InjectFTSForSupersededNode(db *sql.DB) error
```

### Application-level injections

```go
// Insert two active rows with the same logical_id (bypassing UNIQUE index
// by temporarily disabling it — requires writable_schema trick)
InjectDuplicateActiveLogicalID(db *sql.DB, logicalID string) error

// Create a node with NULL source_ref (provenance gap)
InjectNullSourceRef(db *sql.DB) error

// Create a step with a run_id that does not exist (broken FK chain)
InjectOrphanedStep(db *sql.DB) error

// Partially excise a provenance chain (excise run but not its actions)
InjectPartialExcision(db *sql.DB, sourceRef string) error

// Create a chunk with no active node (orphaned chunk)
InjectOrphanedChunk(db *sql.DB) error
```

### Test structure

Each injection function has a corresponding pair of tests:

```
TestCheck_Detects_<CorruptionType>  — inject, run check, assert finding present
TestRepair_Fixes_<CorruptionType>   — inject, run repair, run check, assert clean
```

---

## Recovery Paths

### Storage-level recovery

**When `integrity_check` reports structural corruption:**
1. Run `sqlite3 .recover` (via `sqlite3_recover` API or CLI) to extract salvageable rows
2. Write extracted rows into a new database using the fathomdb schema bootstrap path
3. Run engine-level and application-level checks on the new database
4. Report what was recovered vs. what was lost

**When WAL is in a suspect state:**
1. Force a WAL checkpoint before any other operation: `PRAGMA wal_checkpoint(TRUNCATE)`
2. Verify checkpoint succeeded (no dirty pages remain in WAL)
3. If checkpoint fails (writer is active or WAL is corrupt), surface as blocking error

**File header corruption:** surface immediately; do not attempt pragma-level checks.
Direct the operator to `sqlite3 .recover`.

### Engine-level recovery

| Problem | Repair command |
|---|---|
| Missing FTS rows | `fathom-integrity rebuild --target fts` |
| Stale FTS rows (stale chunks) | `fathom-integrity rebuild --target fts` (full rebuild clears stale) |
| FTS rows for superseded nodes | `fathom-integrity rebuild --target fts` |
| Missing optional projections | `fathom-integrity rebuild-missing` |

### Application-level recovery

| Problem | Repair command |
|---|---|
| Bad provenance (known bad source_ref) | `fathom-integrity excise --source-ref <ref>` |
| Partially-excised chain | Re-run `excise` on the source_ref (idempotent) |
| Orphaned chunks | No automated repair yet — requires operator decision |
| Broken runtime FK chain | No automated repair yet — surface as error |

### `safe_export` hardening ✅ Implemented

Current `safe_export` behavior:
1. Forces `PRAGMA wal_checkpoint(FULL)` before copy and fails if the checkpoint is blocked
2. Copies the checkpointed database via the Rust admin bridge rather than a naive Go-side file copy
3. Writes a companion `<db>.export-manifest.json` containing:
   - Schema version
   - Protocol version
   - Page count
   - Export timestamp (UTC)
   - SHA-256 of the exported file
4. Exposes the workflow through `fathom-integrity export --out <path> --bridge <binary>`

---

## Deep `check` Command Design

Replace the current file-header stub with a layered diagnostic.

### Output structure

```json
{
  "database_path": "/path/to/fathom.db",
  "checked_at": "2026-03-21T22:00:00Z",
  "wal_present": true,
  "wal_frames_unflushed": 3,
  "layer1": {
    "header_valid": true,
    "page_size_valid": true,
    "file_size_aligned": true,
    "integrity_check": "ok",
    "foreign_key_violations": 0,
    "findings": []
  },
  "layer2": {
    "physical_ok": true,
    "foreign_keys_ok": true,
    "missing_fts_rows": 0,
    "duplicate_active_logical_ids": 0,
    "findings": []
  },
  "layer3": {
    "stale_fts_rows": 0,
    "fts_rows_for_superseded_nodes": 0,
    "orphaned_chunks": 0,
    "orphaned_steps": 0,
    "orphaned_actions": 0,
    "null_source_ref_nodes": 0,
    "null_source_ref_actions": 0,
    "findings": []
  },
  "overall": "clean",
  "suggestions": []
}
```

`overall` is one of: `clean` / `warnings` / `errors` / `critical`.

`repair_suggestions` maps each finding to the command that repairs it.

### Implementation notes

- Layer 1 uses `mattn/go-sqlite3` directly (no bridge needed for pragmas)
- WAL state is detected by `stat`-ing `<db>-wal` and parsing the first 32 bytes
  of the WAL header (magic, page size, checkpoint sequence, salt values, frame count)
- Layer 2 calls the Rust bridge (`check_integrity` command)
- Layer 3 runs direct SQL queries against the database via `database/sql`
- If Layer 1 reports Critical, Layer 2 and 3 are skipped with a note

---

## Implementation Plan

### Phase 1: Deep `check` command + corruption injection harness ✅ Complete

**Goal:** Replace the file-header stub. Prove every detection claim with TDD.

Rust changes:
- None required — Rust `check_integrity` already covers Layer 2

Go changes:
- `internal/sqlitecheck/` rewritten as a layered checker
  - Layer 1: file header, page size, file alignment, WAL state, `integrity_check`, `quick_check`, FK check
  - Layer 2: calls bridge `check_integrity` and `check_semantics`, decodes structured payloads
  - Layer 3: direct SQL for stale FTS, orphaned chunks, NULL source_refs
- `internal/sqlitecheck/check.go`: `DiagnosticReport` with per-layer findings, severity classification,
  `suggestions` field mapping each finding to the command that fixes it
- `test/testutil/corrupt.go`: injection helpers — header corruption, truncation, FTS deletion,
  null source_ref, orphaned chunk, broken step FK, large truncation, broken supersession, WAL bit flip
- `check` command uses layered checker; outputs structured JSON via `--json` flag
- Unit and E2E tests for each injected corruption type

### Phase 2: `safe_export` hardening ✅ Complete

**Goal:** Make export trustworthy — WAL checkpointed, manifest written.

Current state: `RunExport()` is bridge-backed and decodes the manifest returned
by the Rust `safe_export` command. The Rust admin service checkpoints WAL,
writes the export manifest, and returns schema/protocol/page-count metadata.

Rust changes implemented:
- `AdminService::safe_export()` takes a `SafeExportOptions` with `force_checkpoint: bool`
- Checkpoint via `PRAGMA wal_checkpoint(FULL)` before copy
- Returns structured manifest data with page count, schema version, protocol version, timestamp, and SHA-256

Go changes implemented:
- `export` command calls bridge `safe_export` with checkpoint option
- Prints the returned manifest details to stdout for operators
- E2E test exports a temp DB, verifies the manifest file, and checks schema/protocol/page-count fields

### Phase 3: Layer 3 application-semantic checks in Rust bridge ✅ Complete

**Goal:** Move the most complex application-semantic queries into the Rust bridge
so they have access to engine internals and can be kept in sync with schema changes.

Bridge command `check_semantics` implemented and integrated:
- `stale_fts_rows`: FTS rows whose chunk_id does not exist in chunks
- `fts_rows_for_superseded_nodes`: FTS rows whose node_logical_id has no active node
- `orphaned_chunks`: chunks whose node_logical_id has no active node
- `null_source_ref_nodes`: count of nodes with `source_ref IS NULL`
- `null_source_ref_actions`: count of actions with `source_ref IS NULL`
- `broken_step_fk_chains`: steps with non-existent run_id
- `broken_action_fk_chains`: actions with non-existent step_id

Go `check` Layer 2 calls both `check_integrity` and `check_semantics` via the bridge.
Broken FK chain detection is surfaced from Layer 2 (bridge) rather than standalone Layer 3 SQL.

### Phase 4: `sqlite3 .recover` integration ✅ Complete

**Goal:** When structural corruption is detected, offer a recovery path.

Go changes implemented:
- `fathom-integrity recover --db <path> --dest <new-db-path> [--bridge <binary>]`
- Runs `sqlite3 .recover` against the corrupt database via exec of sqlite3 CLI
- Replays recovered SQL into the destination database
- Bootstraps the fathomdb schema via bridge if provided
- Returns structured `RecoverReport` with row counts and a full diagnostic result
- E2E tests: `TestRecoverCommand_CleanDBRoundTrip`, `TestRecoverCommand_LargeTruncationRecoversSomething`

### Phase 5: WAL corruption detection and advisory ✅ Complete

**Goal:** Surface the most dangerous SQLite failure mode explicitly.

Go changes implemented:
- `internal/walcheck/wal.go`: parse WAL file header and frame headers
  - Verifies magic bytes (0x377f0682 / 0x377f0683)
  - Counts valid frames with rolling checksum validation
  - Detects frame checksum continuity — reports truncation offset and frame count
  - Reports whether a checkpoint is safe to run
- `check` integrates `walcheck` into Layer 1 findings; advisory emitted when
  unflushed committed frames are detected
- E2E test: `TestCheckCommand_DetectsWALBitFlip` using `InjectWALBitFlip`

---

## Definition of Done

`fathom-integrity` reaches world-class when:

- [x] `check` runs all three layers and produces a structured report with severity classification
- [x] Every corruption type in the injection harness has a paired detect+repair test
- [x] `check` output includes `suggestions` mapping each finding to the command that fixes it
      _(field is named `suggestions` in the actual output, not `repair_suggestions`)_
- [x] `safe_export` checkpoints WAL and writes a metadata manifest
- [x] `recover` wraps `sqlite3 .recover` and reimports into a clean fathomdb database
- [x] WAL state is inspected and reported in every `check` run
- [x] Layer 3 application-semantic checks cover: stale FTS, orphaned chunks, broken FK chains, NULL source_ref
      _(FK chains surfaced via Layer 2 bridge `check_semantics`, not standalone Layer 3 SQL)_
- [x] All detection and repair paths have E2E tests using the corruption injection harness
- [x] The tool can be run on a database it has never seen before and produce a useful report

---

## Companion Documents

- [design-repair-provenance-primitives.md](./design-repair-provenance-primitives.md)
- [setup-round-trip-fixtures.md](./setup-round-trip-fixtures.md)
- [dbim-playbook.md](./dbim-playbook.md)
- [db-integrity-management.md](./archive/db-integrity-management.md) historical source note
- [0.1_IMPLEMENTATION_PLAN.md](./archive/0.1_IMPLEMENTATION_PLAN.md) historical implementation plan
