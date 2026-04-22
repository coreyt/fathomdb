# Design: Projection Identity and Tokenizer Hardening (0.5.4 candidate)

**Release:** 0.5.4 candidate
**Status:** Draft 2026-04-22
**Findings covered:** Review findings 1 and 6
**Breaking:** Internal storage migration only; public API should remain compatible

---

## Problem

FathomDB derives per-kind projection table names from `kind` by lossy
normalization. Short kind names do not receive a hash suffix, so distinct
kinds can map to the same table:

- `Foo-Bar` -> `fts_props_foo_bar`, `vec_foo_bar`
- `Foo_Bar` -> `fts_props_foo_bar`, `vec_foo_bar`
- `Foo.Bar` -> `fts_props_foo_bar`, `vec_foo_bar`

The table name is then used as the isolation boundary for per-kind FTS and
vector projections. A collision can cause one kind's rebuild, repair, delete,
or query path to operate on another kind's projection state.

A related hardening gap exists in FTS table creation. Some paths create a
missing per-kind FTS table with `DEFAULT_FTS_TOKENIZER` instead of resolving
the kind's configured tokenizer profile. Query adaptation can then assume one
tokenizer while the actual FTS5 table uses another.

---

## Current State Anchors

| Area | Current behavior |
|---|---|
| FTS table naming | `crates/fathomdb-schema/src/bootstrap.rs:1255` returns the short lossy name unchanged when `fts_props_<slug>` is <= 63 bytes. |
| Vec table naming | `crates/fathomdb-schema/src/bootstrap.rs:1302` always returns `vec_<slug>` with no hash suffix. |
| Async FTS swap | `crates/fathomdb-engine/src/rebuild_actor.rs:327` uses `DEFAULT_FTS_TOKENIZER` when creating a missing table. |
| Writer first-write FTS creation | `crates/fathomdb-engine/src/writer/mod.rs:1824` uses `DEFAULT_FTS_TOKENIZER` when creating a missing table. |
| Eager admin creation | `create_or_replace_fts_kind_table` already accepts a resolved tokenizer and validates it. |

---

## Goals

- Make projection table names injective for all practical kind strings.
- Preserve SQLite identifier limits and avoid SQL injection risk.
- Provide deterministic names without requiring live database access.
- Migrate existing databases without losing projection data.
- Ensure all FTS table creation paths use the configured tokenizer for that kind.
- Add diagnostics so existing collision damage is visible during integrity checks.

## Non-Goals

- Changing the public node kind model.
- Supporting arbitrary user-supplied table names.
- Rebuilding vector embeddings from external embedders during schema migration.
- Altering global `fts_nodes` behavior.

---

## Design

### 1. Introduce injective canonical table names

Replace the current short-name exception with an always-hashed suffix.

Proposed helpers in `fathomdb-schema`:

```rust
pub fn fts_kind_table_name_v2(kind: &str) -> String {
    projection_table_name("fts_props", kind, 63)
}

pub fn vec_kind_table_name_v2(kind: &str) -> String {
    projection_table_name("vec", kind, 63)
}

fn projection_table_name(prefix: &str, kind: &str, max_len: usize) -> String {
    let slug = sanitize_kind_slug(kind);
    let hash = sha256_hex(kind.as_bytes());
    let suffix = &hash[..10];
    let budget = max_len - prefix.len() - 2 - suffix.len();
    let slug = truncate_ascii_boundary(&slug, budget.max(1));
    format!("{prefix}_{slug}_{suffix}")
}
```

Rationale:

- Appending a hash for every kind removes the collision class entirely.
- A 10-hex-character SHA-256 prefix is enough for table-name identity while
  keeping names readable.
- The sanitized slug remains useful for operator inspection.
- Keeping the function pure preserves the existing dynamic SQL call pattern.

Examples:

| Kind | Old FTS | New FTS |
|---|---|---|
| `Foo-Bar` | `fts_props_foo_bar` | `fts_props_foo_bar_<hashA>` |
| `Foo_Bar` | `fts_props_foo_bar` | `fts_props_foo_bar_<hashB>` |
| `Foo.Bar` | `fts_props_foo_bar` | `fts_props_foo_bar_<hashC>` |

### 2. Keep legacy-name helpers temporarily

The existing `fts_kind_table_name` and `vec_kind_table_name` have many callers.
The implementation should switch those public helpers to the v2 algorithm, but
add explicitly named legacy helpers for migration and repair:

```rust
pub fn legacy_fts_kind_table_name(kind: &str) -> String;
pub fn legacy_vec_kind_table_name(kind: &str) -> String;
```

Only migration, repair, and diagnostics code should call legacy helpers after
this change.

### 3. Add a projection table registry

Add a migration creating:

```sql
CREATE TABLE IF NOT EXISTS projection_table_registry (
    kind TEXT NOT NULL,
    facet TEXT NOT NULL CHECK (facet IN ('fts', 'vec')),
    table_name TEXT NOT NULL UNIQUE,
    naming_version INTEGER NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    migrated_from TEXT,
    PRIMARY KEY (kind, facet)
);
```

The registry is not required for name generation, but it gives integrity checks
and repair routines a durable inventory of expected projection tables. It also
makes migration resumable.

Population sources:

- FTS kinds from `fts_property_schemas.kind`.
- Vec kinds from `projection_profiles WHERE facet = 'vec'`.
- Optional: active node kinds with existing legacy projection tables discovered
  in `sqlite_master`.

### 4. Migration strategy

The migration must be idempotent and safe for partially migrated databases.

For each FTS kind:

1. Compute `legacy = legacy_fts_kind_table_name(kind)`.
2. Compute `target = fts_kind_table_name(kind)` using v2.
3. Insert or update `projection_table_registry`.
4. If `target` exists, leave it in place.
5. If `legacy` exists and no collision group shares it, rename it to `target`
   when SQLite supports the virtual-table rename safely.
6. If the legacy table is absent, create `target` from schema and tokenizer.
7. If a collision group shares `legacy`, do not copy ambiguous FTS rows by table
   ownership. Rebuild each target from canonical `nodes` plus
   `fts_property_schemas`.

For each vec kind:

1. Compute `legacy = legacy_vec_kind_table_name(kind)`.
2. Compute `target = vec_kind_table_name(kind)` using v2.
3. Insert or update `projection_table_registry`.
4. If `target` exists, leave it in place.
5. If `legacy` exists and can be partitioned by joining `chunks` to active
   `nodes.kind`, copy rows for the matching kind into `target`.
6. If rows cannot be copied because sqlite-vec is unavailable or the old table
   shape is unexpected, record a warning and leave regeneration to the existing
   vector repair flow.

Collision handling is intentionally conservative. FTS can be rebuilt from
canonical node properties. Vec rows should not be blindly duplicated because
embedding identity and dimensions matter.

### 5. Centralize FTS table creation

Add one helper in `fathomdb-engine`:

```rust
pub(crate) fn ensure_fts_kind_table(
    conn: &rusqlite::Connection,
    kind: &str,
    schema: &ParsedFtsPropertySchema,
) -> Result<(), EngineError>;
```

The helper must:

- Resolve tokenizer with `fathomdb_schema::resolve_fts_tokenizer(conn, kind)`.
- Compute the desired shape from schema and tokenizer.
- Create the table if absent.
- Recreate the table only when explicitly requested by admin/rebuild code;
  normal writer paths should not drop live rows.
- Validate and escape tokenizer exactly as `create_or_replace_fts_kind_table`
  does today.

Replace direct `CREATE VIRTUAL TABLE IF NOT EXISTS ... DEFAULT_FTS_TOKENIZER`
uses in:

- `rebuild_actor.rs` final swap.
- `writer/mod.rs` first-write insertion.
- `projection.rs` repair/rebuild paths.
- `admin/provenance.rs` restore paths.

### 6. Repair and integrity semantics

Extend `check_integrity` and repair reporting:

- Detect multiple kinds with the same legacy projection table name.
- Report `projection_table_name_collisions` with involved kinds and facets.
- Verify registry rows point to existing tables when schemas/profiles require
  them.
- Verify FTS table tokenizer matches `projection_profiles` when available.
- Offer repair: rebuild FTS projections from canonical state and regenerate or
  copy vec projections when possible.

---

## Compatibility

Public APIs remain unchanged. The internal table names change for newly opened
or migrated databases.

Operational concerns:

- Operators using raw SQL against `fts_props_<kind>` or `vec_<kind>` need a
  release note and should consult `projection_table_registry` instead.
- Existing DBs with no collision should migrate transparently.
- Existing DBs with collision may lose ambiguous projection rows during FTS
  rebuild, but canonical node data is preserved. Vec collision repair may
  require operator-triggered regeneration if rows cannot be safely copied.

---

## Test Plan

Add unit tests:

- `fts_kind_table_name` differs for `Foo-Bar`, `Foo_Bar`, `Foo.Bar`.
- `vec_kind_table_name` differs for the same set.
- Names stay <= 63 bytes for long ASCII and Unicode kind strings.
- Legacy helpers preserve old names for migration tests.

Add migration tests:

- Non-colliding legacy FTS table migrates to v2 name.
- Colliding FTS kinds rebuild into distinct v2 tables with correct rows.
- Non-colliding vec table copies rows when sqlite-vec is available.
- Collision diagnostic reports both kinds.

Add integration tests:

- Register schemas for `Foo-Bar` and `Foo_Bar`; writes/searches stay isolated.
- Configure `source-code` tokenizer, trigger async first registration with no
  pre-existing table, and verify the created table tokenizer matches profile.
- Writer first-write path uses configured tokenizer after profile + schema are
  present.

---

## HITL Gates

No design-blocking question is open. Before implementation, ask for human input
only if one of these choices must change:

- Whether to ship this in 0.5.4 as an internal migration or defer to 0.6.0.
- Whether operator-visible raw SQL table names are considered part of the
  compatibility contract.
- Whether vector collision repair should fail closed or allow best-effort copy
  with warnings when embeddings cannot be regenerated.
