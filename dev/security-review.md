# Security Review — FathomDB

**Date:** 2026-03-24
**Scope:** Full codebase (Rust crates, Go integrity tool, CI/CD, scripts)
**Status:** All findings remediated — see inline code comments tagged with fix IDs (e.g. `Security fix H-1`)

---

## Executive Summary

This review identified **6 High-severity** and **10 Medium-severity** findings across the Rust engine, Go integrity tool, CI pipeline, and developer scripts. The most important issues are SQL injection via JSON path interpolation, unvalidated binary execution paths, and missing CI permissions scoping.

No critical remote-code-execution vulnerabilities were found in the production Rust engine. The highest-risk findings are in the Go integrity CLI and developer setup scripts.

---

## Findings

### HIGH Severity

#### H-1: SQL Injection via JSON Path Interpolation
**File:** `crates/fathomdb-query/src/compile.rs:214-220`

The `JsonPathEq` predicate interpolates user-supplied JSON path strings directly into SQL after only single-quote escaping:

```rust
let escaped_path = path.replace('\'', "''");
write!(&mut sql, "\n  AND json_extract(n.properties, '{escaped_path}') = ?{}", ...);
```

Single-quote escaping is insufficient here. A path like `$') OR 1=1 --` would break out of the string literal context. The *value* is correctly parameterized, but the *path* is not.

**Recommendation:** Validate `path` against a strict JSON-path allowlist (e.g., `^\\$([.][a-zA-Z_][a-zA-Z0-9_]*)+$`), or pass it as a bind parameter if SQLite's `json_extract` supports it.

---

#### H-2: SQL Injection via Table Name Concatenation (Go)
**File:** `go/fathom-integrity/internal/sqlitecheck/check.go:159`

```go
out, err := runSQLiteQuery(sqliteBin, dbPath, "SELECT count(*) FROM "+table+";")
```

`CountTable` is an exported function that concatenates the `table` argument directly into SQL. Current callers pass hardcoded names, but the API is public and has no validation.

**Recommendation:** Validate `table` against a hardcoded allowlist of known table names before use.

---

#### H-3: Unvalidated Bridge Binary Path
**File:** `go/fathom-integrity/internal/bridge/client.go:60`

```go
cmd := exec.CommandContext(ctx, c.BinaryPath)
```

`BinaryPath` originates from the `FATHOM_ADMIN_BRIDGE` environment variable or `--bridge` CLI flag with no validation. An attacker who controls either can execute an arbitrary binary.

**Recommendation:** Validate that the path is absolute, exists, and is not world-writable. Consider restricting to a known install location.

---

#### H-4: Missing CI Workflow Permissions Block
**File:** `.github/workflows/ci.yml`

The workflow has no `permissions:` key. GitHub Actions defaults grant broader access than needed. A compromised action (e.g., from unpinned third-party actions) would inherit those permissions.

**Recommendation:** Add top-level `permissions: { contents: read }` and grant elevated permissions per-job only where needed.

---

#### H-5: Unpinned Third-Party GitHub Actions
**File:** `.github/workflows/ci.yml:20, 23, 32, 51, 65, 78`

Actions are referenced by mutable tags (`@v4`, `@v5`, `@v2`, `@stable`) rather than commit SHAs:

- `actions/checkout@v4`
- `dtolnay/rust-toolchain@stable`
- `Swatinem/rust-cache@v2`
- `taiki-e/install-action@nextest`
- `actions/setup-go@v5`

A compromised or force-pushed tag could inject malicious code into CI.

**Recommendation:** Pin all third-party actions to full commit SHAs.

---

#### H-6: Arbitrary Code Execution via Sourced .env File
**File:** `scripts/developer-setup.sh:6,11-13`

```bash
SQLITE_POLICY_FILE="${SQLITE_POLICY_FILE:-$REPO_ROOT/tooling/sqlite.env}"
if [[ -f "$SQLITE_POLICY_FILE" ]]; then
  source "$SQLITE_POLICY_FILE"
fi
```

The `SQLITE_POLICY_FILE` environment variable allows overriding which file is `source`d. A malicious value can execute arbitrary bash code.

**Recommendation:** Hardcode the path or validate it is within the repository root.

---

### MEDIUM Severity

#### M-1: Exported Database Files Created with Permissive Mode
**File:** `go/fathom-integrity/internal/commands/export.go:11,28`

`os.MkdirAll` uses `0o755` and `os.Create` defaults to `0o666` (minus umask). On shared systems, exported database files may be world-readable.

**Recommendation:** Use `os.OpenFile` with `0o600` for files and `0o700` for directories.

---

#### M-2: TOCTOU Race in Recover Destination Check
**File:** `go/fathom-integrity/internal/commands/recover.go:51-57`

The code checks `os.Stat(destPath)` then later creates the file. Another process could create a file at that path between the check and write.

**Recommendation:** Use `os.OpenFile` with `os.O_CREATE|os.O_EXCL` for atomic creation.

---

#### M-3: Silent Error Suppression in CountTable
**File:** `go/fathom-integrity/internal/sqlitecheck/check.go:160-161`

```go
if err != nil {
    return 0, nil  // silently returns success on error
}
```

Query failures are swallowed, returning 0 rows as if the query succeeded. This masks corruption or missing tables.

**Recommendation:** Return the error, or at minimum log it. The function comment acknowledges this design choice but it undermines diagnostic accuracy.

---

#### M-4: Information Leakage in Error Messages
**Files:**
- `crates/fathomdb-engine/src/bin/fathomdb-admin-bridge.rs:169`
- `go/fathom-integrity/internal/bridge/client.go:69`

Both the Rust bridge and Go client include raw error strings (including stderr output) in responses. These may contain internal file paths, schema details, or system information.

**Recommendation:** Log detailed errors server-side; return sanitized messages to callers.

---

#### M-5: Unquoted Paths in Repair Suggestions
**File:** `go/fathom-integrity/internal/sqlitecheck/check.go:343-382`

Suggested shell commands embed database paths without quoting:

```go
add("WAL file has invalid header; remove if no writers are active: rm " + r.DatabasePath + "-wal")
```

Paths with spaces or shell metacharacters will produce broken or dangerous commands.

**Recommendation:** Shell-quote all paths in suggestions (e.g., wrap in single quotes with proper escaping).

---

#### M-6: Missing .gitignore Entries for Sensitive Files
**File:** `.gitignore`

Missing patterns for common sensitive files: `.env`, `.env.*`, `*.pem`, `*.key`, `*.p12`, `credentials.*`. The repo currently contains `tooling/sqlite.env` (tracked intentionally), but developer-created `.env` files could be accidentally committed.

**Recommendation:** Add ignore rules for `.env*` (with explicit `!tooling/sqlite.env` exception), private keys, and credential files.

---

#### M-7: Unverified Archive Downloads in Setup Script
**File:** `scripts/developer-setup.sh:250-253, 297-299`

Both Go and SQLite archives are downloaded via `curl` and extracted without SHA-256 checksum verification.

**Recommendation:** Download and verify official checksums before extracting.

---

#### M-8: Mutex Poisoning Causes Cascading Panics
**File:** `crates/fathomdb-engine/src/coordinator.rs:66,75,106`

All mutex acquisitions use `.expect()`, causing panics if any thread holding a mutex panics. A single thread panic will crash all subsequent operations.

**Recommendation:** Use `.lock().unwrap_or_else(|e| e.into_inner())` to recover from poisoning, or document the panic-on-poison behavior as intentional fail-fast.

---

#### M-9: Missing Upper Bound on WAL Page Size
**File:** `go/fathom-integrity/internal/walcheck/wal.go:68-72`

```go
if pageSize < 512 {
    return report, nil
}
```

Only a lower bound is checked. A malicious WAL with `pageSize = 2^31` could cause excessive memory allocation.

**Recommendation:** Add upper bound check: `pageSize > 65536`.

---

#### M-10: Empty source_ref Defaults Silently in Admin Commands
**File:** `crates/fathomdb-engine/src/bin/fathomdb-admin-bridge.rs:111,122`

`TraceSource` and `ExciseSource` commands default to `""` when `source_ref` is not provided, which could cause unintended broad operations.

**Recommendation:** Return an error when `source_ref` is missing for these commands.

---

## Positive Findings

- **Parameterized queries** are used correctly throughout most of the Rust SQL layer
- **No `unsafe` blocks** in Rust code
- **Minimal Go dependencies** (only `testify` for tests) — excellent attack surface reduction
- **Foreign key constraints enabled** by default in production
- **SHA-256 integrity** used for safe_export
- **Immediate transactions** used for writes (prevents concurrent corruption)
- **`set -euo pipefail`** used in shell scripts

---

## Priority Remediation Order

1. **H-1** — JSON path SQL injection (highest exploitability in the engine)
2. **H-4 + H-5** — CI permissions and action pinning (supply chain risk)
3. **H-2 + H-3** — Go input validation (CLI attack surface)
4. **H-6** — Script sourcing vulnerability
5. **M-1 through M-10** — Address in order of operational risk
