# Design: Bridge and FFI Input Safety

## Purpose

Address two verified production-readiness findings related to untrusted
input handling: bridge path traversal (H-1) and unbounded JSON parsing in
the Python FFI and bridge (H-6).

---

## H-1. Bridge Path Validation

### Current State

`crates/fathomdb-engine/src/bin/fathomdb-admin-bridge.rs:29,40,138`

`database_path` and `destination_path` are deserialized from JSON without
any validation. A caller can supply path-traversal payloads
(`../../etc/passwd`) to probe the filesystem or overwrite arbitrary files
via `safe_export`.

### Threat Model

The bridge binary reads a single JSON request from stdin and executes an
admin command against the specified database. The caller is the Go
`fathom-integrity` tool, which constructs the request programmatically.

In the current architecture, the bridge is invoked as a subprocess by a
trusted operator tool. The path validation risk is lower than in a
network-facing service, but defense-in-depth is still warranted:

- An operator might invoke the bridge directly with user-influenced input.
- A bug in the Go tool might pass unsanitized paths.
- The bridge binary is a standalone executable that could be misused if
  deployed without the Go wrapper.

### Design

Add a `validate_path` function called before `AdminService::new()`:

```rust
fn validate_path(path: &Path, label: &str) -> Result<(), BridgeError> {
    // Must be absolute
    if !path.is_absolute() {
        return Err(BridgeError::InvalidPath(
            format!("{label} must be an absolute path: {}", path.display())
        ));
    }

    // Must not contain .. components
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(BridgeError::InvalidPath(
                format!("{label} must not contain '..' components: {}", path.display())
            ));
        }
    }

    Ok(())
}
```

Call for both paths before constructing `AdminService`:

```rust
validate_path(&request.database_path, "database_path")?;
if let Some(ref dest) = request.destination_path {
    validate_path(dest, "destination_path")?;
}
```

### Why not canonicalize?

`fs::canonicalize()` resolves symlinks and requires the path to exist. For
`destination_path` in `safe_export`, the target file does not exist yet.
Component-level validation is sufficient and does not require filesystem
access.

### Optional: allowlist directory

For stricter deployments, support an optional `--allowed-dir` CLI flag
that restricts both paths to a subtree. This is not required for v1 but
is a natural extension.

---

## H-6. Input Size Limits

### Current State

**Python FFI** (`crates/fathomdb/src/python.rs:405-415`):
`parse_ast()` and `parse_write_request()` call `serde_json::from_str()`
on caller-provided strings with no size check.

**Bridge** (`crates/fathomdb-engine/src/bin/fathomdb-admin-bridge.rs:97`):
`io::stdin().read_to_string(&mut stdin)` reads all of stdin into memory
with no limit.

### Design

**Bridge: bounded stdin read.**

Replace `read_to_string` with a size-limited read:

```rust
const MAX_BRIDGE_INPUT_BYTES: u64 = 64 * 1024 * 1024; // 64 MB

let mut stdin = String::new();
io::stdin()
    .take(MAX_BRIDGE_INPUT_BYTES)
    .read_to_string(&mut stdin)?;

if stdin.len() as u64 >= MAX_BRIDGE_INPUT_BYTES {
    return Err(BridgeError::InputTooLarge(MAX_BRIDGE_INPUT_BYTES));
}
```

64 MB is generous for any admin command payload. The largest realistic
input is a `safe_export` request with a long path — well under 1 KB.

**Python FFI: length check before parse.**

```rust
const MAX_AST_JSON_BYTES: usize = 16 * 1024 * 1024;  // 16 MB
const MAX_WRITE_JSON_BYTES: usize = 64 * 1024 * 1024; // 64 MB

fn parse_ast(ast_json: &str) -> PyResult<...> {
    if ast_json.len() > MAX_AST_JSON_BYTES {
        return Err(PyValueError::new_err(
            format!("AST JSON exceeds maximum size of {} bytes", MAX_AST_JSON_BYTES)
        ));
    }
    // ... existing parse ...
}
```

AST JSON is a compiled query — 16 MB is far beyond any realistic query.
Write request JSON can be larger due to embedded payloads, so 64 MB.

### Constants location

Define the limits as constants in each module where they are used. No
need for a shared limits module — the bridge and Python FFI have different
acceptable sizes and different error types.

---

## Test Plan

- **H-1:** Test that relative paths are rejected. Test that paths with
  `..` components are rejected. Test that valid absolute paths pass.
- **H-6 bridge:** Test that input exceeding 64 MB is rejected before
  JSON parsing.
- **H-6 Python:** Test that `parse_ast` with a string exceeding 16 MB
  returns `PyValueError`.
