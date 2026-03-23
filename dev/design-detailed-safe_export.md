# Design: `safe_export` Hardening

## Purpose

Harden the `export` command so that every exported database file is provably
complete. The current Go `export` command is a naive `io.Copy` that may silently
omit committed transactions sitting in the WAL. The current Rust `safe_export`
does checkpoint and hash correctly but produces an incomplete manifest. This
document closes both gaps.

This is the last open item in the `fathom-integrity` Definition of Done.

---

## Current State

### Rust (`AdminService::safe_export`)

Located in `crates/fathomdb-engine/src/admin.rs`.

What it does correctly:
- Runs `PRAGMA wal_checkpoint(FULL)` before copying
- Copies the database file to the destination path
- Computes SHA-256 of the copied file
- Writes `<dest>.export-manifest.json` alongside the copy

What is wrong:
- `PRAGMA wal_checkpoint(FULL)` is called via `conn.execute(...)`, which discards
  the result set. SQLite returns three columns: `busy`, `log`, `checkpointed`. If
  `busy > 0`, active readers blocked the checkpoint — the WAL was not fully
  applied. The current code cannot detect this and proceeds to copy anyway.
- `SafeExportManifest` is missing three fields required by the recovery spec:
  `schema_version`, `protocol_version`, `page_count`.
- `safe_export()` takes only `destination: impl AsRef<Path>`. There is no
  `SafeExportOptions` to carry intent, which makes the API opaque.

Current `SafeExportManifest`:
```rust
pub struct SafeExportManifest {
    pub exported_at: u64,   // Unix seconds
    pub sha256: String,     // 64 hex characters
}
```

One Rust test exists (`safe_export_writes_manifest_with_sha256`) but it only
checks that the manifest file exists and the SHA-256 is 64 hex characters. It
does not assert the new fields, the WAL checkpoint result, or the page count.

### Go (`commands/export.go`)

```go
func RunExport(databasePath, destinationPath string, out io.Writer) error {
    os.MkdirAll(filepath.Dir(destinationPath), 0o755)
    copyFile(destinationPath, databasePath)
    fmt.Fprintf(out, "exported %s -> %s\n", databasePath, destinationPath)
}
```

No WAL checkpoint. No bridge call. No manifest. An operator running `fathom-integrity
export` today may receive a database copy that is silently missing the last
committed write batch.

### Admin bridge

The bridge binary (`crates/fathomdb-engine/src/bin/fathomdb-admin-bridge.rs`)
already handles the `safe_export` command. The Go bridge client already has
`DestinationPath string` in the `Request` struct. No bridge protocol changes
are needed.

---

## Scope

Changes in this design:

**Rust:**
1. Fix WAL checkpoint result verification in `safe_export`
2. Add `SafeExportOptions` with `force_checkpoint: bool`
3. Extend `SafeExportManifest` with `schema_version`, `protocol_version`,
   `page_count`
4. Extend the existing Rust test; add a test for checkpoint-blocked failure

**Go:**
1. Replace the naive `RunExport` with a bridge-backed implementation
2. Add `ExportManifest` struct to the bridge package (or `commands` package)
3. Add a unit test for `RunExport` (mock bridge)
4. Add an E2E test for the full export → open → verify round-trip

**Not in scope:**
- Restore semantics
- Incremental export / WAL-segment export
- Remote or cloud export destinations
- Cross-version manifest compatibility policy beyond protocol_version field

---

## Manifest Contract

The canonical manifest written by Rust alongside every export:

```
<destination>.export-manifest.json
```

JSON structure (final form after this design):

```json
{
  "exported_at": 1742741234,
  "sha256": "a3f1c2...(64 hex chars)",
  "schema_version": 1,
  "protocol_version": 1,
  "page_count": 512
}
```

| Field | Type | Source | Meaning |
|---|---|---|---|
| `exported_at` | u64 (Unix seconds) | `SystemTime::now()` | When the export was taken |
| `sha256` | String (64 hex) | SHA-256 of exported file | File integrity check |
| `schema_version` | u32 | `MAX(version) FROM fathom_schema_migrations` | Engine schema at export time |
| `protocol_version` | u32 | Compile-time constant `PROTOCOL_VERSION` | Bridge protocol at export time |
| `page_count` | u64 | `PRAGMA page_count` after checkpoint | Database size in pages |

The manifest file is written by Rust (inside the bridge call) atomically with
the database copy. The Go command reads it back from the bridge response payload
and displays a summary to the operator. Go never writes the manifest independently.

---

## Rust Changes

### 1. `SafeExportOptions`

New struct in `admin.rs`:

```rust
#[derive(Clone, Debug)]
pub struct SafeExportOptions {
    /// When true (always the case for trustworthy production exports),
    /// the writer runs PRAGMA wal_checkpoint(FULL) before copying and
    /// fails the export if any WAL frames could not be applied.
    pub force_checkpoint: bool,
}

impl Default for SafeExportOptions {
    fn default() -> Self {
        Self { force_checkpoint: true }
    }
}
```

`force_checkpoint: false` is provided for tests that seed a database without
WAL mode and want to skip the checkpoint PRAGMA. Production callers must always
use `force_checkpoint: true` (or `SafeExportOptions::default()`).

### 2. Extended `SafeExportManifest`

```rust
#[derive(Clone, Debug, serde::Serialize)]
pub struct SafeExportManifest {
    pub exported_at: u64,
    pub sha256: String,
    pub schema_version: u32,
    pub protocol_version: u32,
    pub page_count: u64,
}
```

### 3. `safe_export` signature change

```rust
pub fn safe_export(
    &self,
    destination: impl AsRef<Path>,
    options: SafeExportOptions,
) -> Result<SafeExportManifest, EngineError>
```

Callers that previously called `service.safe_export(dest)` must now call
`service.safe_export(dest, SafeExportOptions::default())`.

The bridge handler in `fathomdb-admin-bridge.rs` is the only non-test call site.

### 4. WAL checkpoint verification

Replace the current `conn.execute("PRAGMA wal_checkpoint(FULL)", [])` with a
`query_row` that reads the result set:

```rust
if options.force_checkpoint {
    let (busy, log, checkpointed): (i64, i64, i64) = conn.query_row(
        "PRAGMA wal_checkpoint(FULL)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    if busy != 0 {
        return Err(EngineError::Bridge(format!(
            "WAL checkpoint blocked: {} active reader(s) prevented a full checkpoint; \
             log frames={log}, checkpointed={checkpointed}; \
             retry export when no readers are active",
            busy
        )));
    }
    // log == checkpointed is the success invariant for FULL mode.
    // If they diverge without busy=1 (e.g. log=0, checkpointed=0 on a
    // non-WAL database), that is still safe — log=0 means nothing to flush.
}
```

The `busy` column is the authoritative signal. When `busy=0` the checkpoint
ran to completion. When `busy!=0` the copy must not proceed.

### 5. Schema version and page count queries

After a successful checkpoint:

```rust
let schema_version: u32 = conn
    .query_row(
        "SELECT COALESCE(MAX(version), 0) FROM fathom_schema_migrations",
        [],
        |row| row.get(0),
    )
    .unwrap_or(0);

let page_count: u64 = conn
    .query_row("PRAGMA page_count", [], |row| row.get(0))
    .unwrap_or(0);
```

`schema_version` falls back to 0 if the migrations table does not exist (corrupt
or pre-schema database). `page_count` falls back to 0 on error. Both fallbacks
are safe: they result in a manifest that is still written and still
cryptographically bound to the file via SHA-256.

### 6. `PROTOCOL_VERSION` constant

The bridge binary defines `const PROTOCOL_VERSION: u32 = 1`. This constant must
be accessible from `admin.rs`. Move it (or re-export it) to a shared location —
or simply duplicate it as `const EXPORT_PROTOCOL_VERSION: u32 = 1` in `admin.rs`
with a comment pointing to the bridge constant. The two must be kept in sync when
the protocol version increments.

Design choice: keep a local `const EXPORT_PROTOCOL_VERSION: u32 = 1` in
`admin.rs` and add a comment: `// must match PROTOCOL_VERSION in
fathomdb-admin-bridge.rs`. A compile-time assertion is not possible across
binaries, so a comment is the best available enforcement.

### 7. Admin bridge handler update

`BridgeCommand::SafeExport` in `fathomdb-admin-bridge.rs` currently calls
`service.safe_export(destination)`. It must be updated to:

```rust
service.safe_export(destination, SafeExportOptions::default())
```

The bridge response payload shape is unchanged (it serialises `SafeExportManifest`
as JSON). The three new fields are additive — existing consumers that ignore
unknown fields will not break.

### 8. Rust TDD plan

**Write these tests first (they will fail until the implementation is complete):**

| Test | What to assert |
|---|---|
| `safe_export_manifest_includes_schema_version_and_page_count` | `manifest.schema_version == 1`, `manifest.protocol_version == 1`, `manifest.page_count > 0` |
| `safe_export_force_checkpoint_false_skips_checkpoint` | Export succeeds with `force_checkpoint: false`; no WAL PRAGMA error even if WAL is absent |
| `safe_export_fails_when_checkpoint_blocked` | Cannot easily simulate `busy != 0` in tests, but the error path must be exercised via a WAL-mode database with an open reader transaction blocking the checkpoint — mark as `#[ignore]` with a note if environment simulation is not feasible in unit tests; cover the logic path with a mock or integration approach |

The first two are straightforward unit tests. The third is best covered by the
Go E2E test which can run the full binary.

Extend the existing `safe_export_writes_manifest_with_sha256` test to also
assert the three new fields rather than writing a separate test for the same
setup.

---

## Go Changes

### 1. `ExportManifest` struct

Add to `internal/bridge/client.go` (alongside existing response types), or to
a new `internal/commands/exporttypes.go`:

```go
// ExportManifest is the structured payload returned by the bridge safe_export
// command and written as <destination>.export-manifest.json by the Rust engine.
type ExportManifest struct {
    ExportedAt      int64  `json:"exported_at"`       // Unix seconds
    SHA256          string `json:"sha256"`             // 64 hex characters
    SchemaVersion   uint32 `json:"schema_version"`
    ProtocolVersion uint32 `json:"protocol_version"`
    PageCount       uint64 `json:"page_count"`
}
```

Place it in `internal/bridge/client.go` so it is co-located with the bridge
protocol types and can be used by both commands and tests.

### 2. `RunExport` replacement

Replace the naive `io.Copy` implementation in `internal/commands/export.go`:

```go
// RunExport exports a fathomdb database to destinationPath using the Rust
// admin bridge to ensure the WAL is fully checkpointed before the copy.
// bridgePath must point to the fathomdb-admin-bridge binary.
func RunExport(
    databasePath, destinationPath, bridgePath string,
    out io.Writer,
) error {
    if bridgePath == "" {
        return fmt.Errorf(
            "safe export requires the admin bridge binary (--bridge); " +
            "without it the WAL cannot be checkpointed and the copy may be incomplete",
        )
    }

    if err := os.MkdirAll(filepath.Dir(destinationPath), 0o755); err != nil {
        return fmt.Errorf("creating destination directory: %w", err)
    }

    client := bridge.NewClient(bridgePath)
    resp, err := client.SafeExport(databasePath, destinationPath)
    if err != nil {
        return fmt.Errorf("safe_export bridge call failed: %w", err)
    }
    if !resp.OK {
        return fmt.Errorf("safe_export failed: %s", resp.Message)
    }

    var manifest bridge.ExportManifest
    if err := json.Unmarshal(resp.Payload, &manifest); err != nil {
        return fmt.Errorf("parsing export manifest: %w", err)
    }

    exportedAt := time.Unix(manifest.ExportedAt, 0).UTC().Format(time.RFC3339)
    fmt.Fprintf(out, "exported  %s\n", destinationPath)
    fmt.Fprintf(out, "manifest  %s.export-manifest.json\n", destinationPath)
    fmt.Fprintf(out, "sha256    %s\n", manifest.SHA256)
    fmt.Fprintf(out, "pages     %d\n", manifest.PageCount)
    fmt.Fprintf(out, "schema    v%d\n", manifest.SchemaVersion)
    fmt.Fprintf(out, "at        %s\n", exportedAt)
    return nil
}
```

### 3. Bridge client: `SafeExport` method

Add to `internal/bridge/client.go`:

```go
func (c *Client) SafeExport(databasePath, destinationPath string) (*Response, error) {
    return c.send(Request{
        ProtocolVersion: ProtocolVersion,
        DatabasePath:    databasePath,
        Command:         CommandSafeExport,
        DestinationPath: destinationPath,
    })
}
```

`CommandSafeExport` is already in the `Command` enum as `"safe_export"`.
`DestinationPath` is already a field on `Request`. No protocol changes needed.

### 4. CLI wiring

The `export` subcommand in `internal/cli/cli.go` must pass the bridge path
through to `RunExport`. The `--bridge` flag already exists in the CLI for other
commands. Wire it to the export command the same way it is wired to `rebuild`
and `excise`.

If `--bridge` is not provided, `RunExport` returns a clear error rather than
falling back to naive copy. Silent degradation to an unsafe copy path must not
happen.

### 5. Go TDD plan

**Unit test** (`internal/commands/export_test.go`):

```go
// TestRunExport_BridgeBackedExport:
//   - Set up a temp DB and temp destination
//   - Provide a fake bridge binary (or stub the client) that returns a valid
//     manifest JSON payload
//   - Call RunExport
//   - Assert output contains sha256, pages, schema version lines
//   - Assert no error returned
//
// TestRunExport_FailsWithoutBridgePath:
//   - Call RunExport with bridgePath=""
//   - Assert error message mentions --bridge
//
// TestRunExport_FailsWhenBridgeReturnsError:
//   - Stub bridge to return ok=false with a message
//   - Assert RunExport returns a non-nil error containing the message
```

**E2E test** (`test/e2e/export_test.go` or extending existing e2e suite):

```go
// TestExportCommand_RoundTrip:
//   1. Build the bridge binary (or locate it)
//   2. Seed a database via the bridge (write a node, verify it)
//   3. Run: fathom-integrity export --db <path> --out <dest> --bridge <bridge>
//   4. Assert exit code 0
//   5. Assert <dest> exists and is a valid SQLite file (open and query)
//   6. Assert <dest>.export-manifest.json exists and is valid JSON
//   7. Parse manifest; assert sha256 matches file, schema_version >= 1,
//      page_count > 0, exported_at > 0
//   8. Optionally: open <dest> and run a check to verify it is clean
```

The E2E test is the most valuable test because it exercises the full pipeline:
Go CLI → bridge subprocess → Rust checkpoint → file copy → manifest write →
Go manifest parse → operator output.

---

## Error Handling

| Failure | Error surface | Recovery |
|---|---|---|
| `busy != 0` from WAL checkpoint | `EngineError::Bridge(...)` in Rust; surfaced as `resp.OK = false` to Go | Wait for active readers to finish; retry export |
| Destination directory not writable | `EngineError::Io(...)` in Rust; `fmt.Errorf(...)` in Go | Fix permissions or choose a different destination |
| Bridge binary not found | `bridge.NewClient` returns error on first `send` | Provide `--bridge` path |
| Source database does not exist | `rusqlite::Error` on open | Fix database path |
| SHA-256 computation fails (I/O error) | `EngineError::Io(...)` | Disk error; check storage |
| Manifest JSON serialization fails | Logged; `json!({})` fallback in bridge | Never happens in practice; `serde` on a simple struct always succeeds |

The Go `RunExport` must not silently fall back to a naive copy on any of these
failures. Any bridge error is a hard failure that returns a non-zero exit code.

---

## Transaction Discipline

`safe_export` must not be called while a write is in flight. The current
implementation acquires no explicit lock. The WAL checkpoint naturally handles
this: `PRAGMA wal_checkpoint(FULL)` waits for active writers to complete before
checkpointing. In practice, the single writer thread serializes all writes, so
the checkpoint will always find the writer idle.

The exported file is a point-in-time snapshot of the database as of the moment
the checkpoint completed. Any writes submitted after the checkpoint starts are
not included in the export, which is correct and expected.

---

## Implementation Checklist

### Rust

- [ ] Write failing test: `safe_export_manifest_includes_schema_version_and_page_count`
- [ ] Write failing test: `safe_export_force_checkpoint_false_skips_wal_pragma`
- [ ] Add `SafeExportOptions` struct with `Default`
- [ ] Extend `SafeExportManifest` with `schema_version`, `protocol_version`, `page_count`
- [ ] Fix WAL checkpoint to use `query_row` and check `busy == 0`
- [ ] Add `schema_version` and `page_count` queries
- [ ] Add `EXPORT_PROTOCOL_VERSION` constant
- [ ] Change `safe_export` signature to accept `SafeExportOptions`
- [ ] Update bridge handler call site: `safe_export(dest, SafeExportOptions::default())`
- [ ] All Rust tests pass (65 existing + 2 new = 67)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean

### Go

- [ ] Write failing unit test: `TestRunExport_FailsWithoutBridgePath`
- [ ] Write failing unit test: `TestRunExport_BridgeBackedExport` (mock/stub bridge)
- [ ] Write failing unit test: `TestRunExport_FailsWhenBridgeReturnsError`
- [ ] Write failing E2E test: `TestExportCommand_RoundTrip`
- [ ] Add `ExportManifest` struct to `internal/bridge/client.go`
- [ ] Add `SafeExport` method to bridge `Client`
- [ ] Replace `RunExport` with bridge-backed implementation
- [ ] Wire `--bridge` flag through to export subcommand in `cli.go`
- [ ] All Go unit tests pass
- [ ] E2E test passes
- [ ] `go vet ./...` clean

### Definition of Done

- [ ] `fathom-integrity export --db <path> --out <dest> --bridge <binary>` exports
      a WAL-checkpointed, SHA-256-verified database with a manifest
- [ ] Attempting export without `--bridge` fails with a clear error
- [ ] Export on a WAL-blocked database fails with a diagnostic message
- [ ] The manifest includes `schema_version`, `protocol_version`, `page_count`,
      `exported_at`, `sha256`
- [ ] Round-trip E2E test passes: export → open exported DB → verify clean + manifest
- [ ] All existing tests continue to pass
- [ ] The last unchecked box in `fathom-integrity-recovery.md` Definition of Done
      is checked
