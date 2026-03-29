# Design: Schema Migration Safety

## Purpose

Address two verified production-readiness findings in the schema bootstrap
path: transactional migrations (C-3) and downgrade protection (C-4).

---

## C-3. Transactional Schema Migrations

### Current State

`crates/fathomdb-schema/src/bootstrap.rs:417`

The generic migration arm runs `conn.execute_batch(migration.sql)?`
without a surrounding transaction. If a multi-statement migration fails
partway through, some DDL is committed and some is not. The migration
version is recorded only after successful execution (lines 419-422), so a
retry re-runs the entire migration but may fail on already-applied
statements (e.g. duplicate `CREATE TABLE`).

Special-case migrations (versions 4-13) use individual functions with
`IF NOT EXISTS` guards — these are individually idempotent. The generic
`_` arm has no such guarantee.

### Design

Wrap each migration (including version recording) in an explicit
transaction:

```rust
for migration in &MIGRATIONS {
    if applied_versions.contains(&migration.version) {
        continue;
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    match migration.version {
        4 => ensure_provenance_metadata(&tx)?,
        // ... other special cases ...
        _ => tx.execute_batch(migration.sql)?,
    }

    tx.execute(
        "INSERT INTO fathom_schema_migrations (version, applied_at) VALUES (?1, unixepoch())",
        params![migration.version],
    )?;

    tx.commit()?;
}
```

SQLite supports DDL inside transactions. `CREATE TABLE`, `ALTER TABLE`,
and `CREATE INDEX` are all transactional in SQLite. If any statement
fails, the entire migration rolls back, and the version is not recorded.
A retry will re-attempt the exact same migration from a clean state.

### Special-case migration functions

The existing helper functions (`ensure_provenance_metadata`, etc.) use
`conn.execute()` directly. They must be updated to accept `&Transaction`
instead of `&Connection`. Since `Transaction` derefs to `Connection`, the
SQL calls remain identical — only the function signatures change.

### Migration SQL authoring rule

With transactional wrapping, new migrations no longer need `IF NOT EXISTS`
guards for idempotency. The transaction guarantees atomicity. However,
existing special-case migrations (versions 4-13) should keep their guards
as defense-in-depth — removing them would be a gratuitous behavioral
change.

---

## C-4. Schema Version Downgrade Protection

### Current State

`crates/fathomdb-schema/src/bootstrap.rs:393-405`

The migration loop skips already-applied versions but never checks whether
the database has been opened by a newer engine version. Opening a
v13-schema database with a v12 engine silently proceeds with an
incompatible schema.

### Design

After loading applied versions, check the maximum applied version against
the engine's maximum known version:

```rust
let max_applied = applied_versions.iter().max().copied().unwrap_or(0);
let max_known = MIGRATIONS.last().map(|m| m.version).unwrap_or(0);

if max_applied > max_known {
    return Err(EngineError::SchemaVersionMismatch {
        database_version: max_applied,
        engine_version: max_known,
    });
}
```

This check runs before the migration loop, so no partial work is done on
an incompatible database.

### New error variant

Add `SchemaVersionMismatch` to `EngineError`:

```rust
#[error("database schema version {database_version} is newer than engine version {engine_version}; upgrade the engine before opening this database")]
SchemaVersionMismatch {
    database_version: u32,
    engine_version: u32,
},
```

This is a hard, non-recoverable error. The only resolution is to upgrade
the engine binary. The error message should be clear enough that an
operator knows exactly what to do.

### Bridge and Python surface

Both the bridge binary and the Python FFI open databases through the same
`bootstrap()` path, so the downgrade check applies automatically. No
separate surface changes are needed.

---

## Test Plan

- **C-3:** Integration test that runs a migration containing two
  statements where the second fails (e.g. invalid SQL). Verify the first
  statement's DDL is rolled back and the migration version is not
  recorded. Verify a corrected migration can be retried successfully.
- **C-4:** Unit test that creates a database with a future schema version
  (e.g. version 999) in `fathom_schema_migrations`. Open with the current
  engine and verify `SchemaVersionMismatch` is returned. Verify the
  database is not modified.
