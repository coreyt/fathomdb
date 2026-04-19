# Design: `check_semantics()` drift detection on weighted FTS tables (0.5.2 Item 1)

**Release:** 0.5.2
**Scope item:** Item 1 from `dev/notes/0.5.2-scope.md`
**Breaking:** No (diagnostic-only bug fix; no runtime API change)

---

## Problem

`Admin::check_semantics()` fails with
`SqliteError: no such column: fp.text_content` whenever the target
database has a kind whose FTS property schema was registered with
per-path specs (the weighted / per-column shape introduced alongside
`register_fts_property_schema_with_entries`).

Root cause is local to one helper:

- `crates/fathomdb-engine/src/admin/mod.rs:1060` —
  `count_drifted_property_fts_rows` hardcodes a `fp.text_content`
  column read for every per-kind FTS table it finds:

  ```sql
  SELECT fp.node_logical_id, fp.text_content, n.properties
  FROM {table} fp
  JOIN nodes n ON n.logical_id = fp.node_logical_id
             AND n.superseded_at IS NULL
  WHERE n.kind = ?1
  ```

But per-kind FTS tables come in two shapes, both valid after 0.5.1:

1. **Baseline (non-weighted)**: `node_logical_id UNINDEXED,
   text_content`. Built by bootstrap
   (`crates/fathomdb-schema/src/bootstrap.rs:999`), by write-time
   defensive DDL (`crates/fathomdb-engine/src/writer/mod.rs:1825`),
   and by the rebuild actor swap path
   (`crates/fathomdb-engine/src/rebuild_actor.rs:329`).
2. **Weighted (per-column)**: `node_logical_id UNINDEXED, <col0>,
   <col1>, ...` where each column name comes from
   `fts_column_name(path, is_recursive)`
   (`crates/fathomdb-schema/src/bootstrap.rs:1328`). Built by
   `create_or_replace_fts_kind_table` when `specs` is non-empty
   (`crates/fathomdb-engine/src/admin/fts.rs:602-614`). **No
   `text_content` column.**

The coordinator already accommodates both shapes — see
`crates/fathomdb-engine/src/coordinator.rs:1941-1948`, where
`is_weighted` switches snippet extraction between
`substr(fp.text_content, 1, 200) AS snippet` and `'' AS snippet`.
`count_drifted_property_fts_rows` was not updated alongside it.

## Reproduction

Memex v040-item2 worktree, 2026-04-18:

```
test_check_semantics_clean
  → FathomStore.open(db)
  → engine.admin.configure_fts(kind="KnowledgeItem", ...)
    registers weighted FTS property schemas for KnowledgeItem +
    ConversationTurn.
  → admin.check_semantics()
  → SqliteError: no such column: fp.text_content in SELECT
    fp.node_logical_id, fp.text_content, n.properties FROM
    fts_props_wmaction fp JOIN nodes n ON ...
```

Triggers from any caller: Rust `Admin::check_semantics()`, Python
`admin.check_semantics()`, TypeScript `admin.checkSemantics()`, Go
`fathom-integrity check`. All of them ultimately hit
`count_drifted_property_fts_rows`.

## Impact surface

Narrow, diagnostic-only:

- Write path: unaffected.
- Search path (including weighted BM25 ranking): unaffected. Verified
  downstream by Memex (`engine.nodes('WMGoal').search(...)` returns
  results correctly after m007).
- Rebuild/swap actor: unaffected; it already distinguishes weighted
  from non-weighted rebuilds
  (`rebuild_actor.rs:340-409`).
- Fathom integrity / doctor commands: any path that calls
  `check_semantics()` panics on weighted DBs.

## Fix

Make `count_drifted_property_fts_rows` shape-aware.

### Shape detection

Before querying a per-kind FTS table, probe its column list via
`PRAGMA table_info({table})` (same idiom used in
`crates/fathomdb-schema/src/bootstrap.rs:1025-1029`). If `text_content`
is among the column names, the table is non-weighted; otherwise it is
weighted.

### Non-weighted path (existing behavior)

Unchanged. Existing query reads `fp.text_content`, reconstructs the
expected text via `extract_property_fts(props, schema)`
(`crates/fathomdb-engine/src/writer/fts_extract.rs:282`), and
increments `drifted` on mismatch.

### Weighted path (new)

Use `extract_property_fts_columns(props, schema)` instead
(`crates/fathomdb-engine/src/writer/fts_extract.rs:345`), which already
produces the exact per-column text that the rebuild actor writes at
`rebuild_actor.rs:354-394`. For each row:

1. Load `node_logical_id`, every per-column value, and
   `n.properties` via a dynamically-built
   `SELECT fp.node_logical_id, fp.<col0>, fp.<col1>, ..., n.properties
   FROM {table} fp JOIN nodes n ...` where the column list is taken
   from the PRAGMA probe (excluding `node_logical_id`).
2. Reconstruct expected columns via `extract_property_fts_columns`.
3. For each schema path, compare stored-column text with expected
   text. Any mismatch → `drifted += 1` (count per row, not per
   column — consistent with the non-weighted arm).

### Why not "just skip weighted tables and return 0"

Considered as a fallback in the scope doc, but rejected for the
landing fix. Returning `0` would be correct (never produces false
positives) but would silently demote `check_semantics()` to an
incomplete diagnostic on any DB that has ever registered a weighted
schema. Since `extract_property_fts_columns` already exists and
matches the rebuild actor's write path byte-for-byte, the per-column
comparison is cheap to wire up and preserves the diagnostic's
contract.

Keep the skip-and-TODO as an emergency escape hatch if the per-column
implementation hits a snag during review.

## Implementation sketch

```rust
fn count_drifted_property_fts_rows(conn: &rusqlite::Connection) -> Result<i64, EngineError> {
    let schemas = crate::writer::load_fts_property_schemas(conn)?;
    if schemas.is_empty() {
        return Ok(0);
    }

    let mut drifted = 0i64;
    for (kind, schema) in &schemas {
        let table = fathomdb_schema::fts_kind_table_name(kind);
        let table_exists: bool = /* unchanged */;
        if !table_exists { continue; }

        // NEW: probe table shape.
        let columns: Vec<String> = {
            let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
            stmt.query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let has_text_content = columns.iter().any(|c| c == "text_content");

        if has_text_content {
            // Existing path.
            drifted += count_drift_non_weighted(conn, kind, &table, schema)?;
        } else {
            // New path.
            drifted += count_drift_weighted(conn, kind, &table, schema, &columns)?;
        }
    }
    Ok(drifted)
}
```

`count_drift_weighted`:

- Filter `columns` to exclude `node_logical_id` (UNINDEXED) — these
  are the per-path columns to read.
- Build the SELECT dynamically: column names come from
  `fathomdb_schema::fts_column_name(&spec.path, is_recursive)` — the
  same function the DDL used. Cross-check that each schema path's
  derived column name appears in the probed column list; if a column
  is missing (schema drift we can't assess), skip that row safely or
  count as drifted. Prefer "count as drifted" — it surfaces the issue
  rather than hiding it.
- Row-wise: parse `n.properties` as JSON, call
  `extract_property_fts_columns(&props, schema)`, compare each
  `(column_name, expected_text)` pair with the stored value. Any
  per-path mismatch → `drifted += 1` and move to the next row.

### Dynamic SQL safety

- Table name comes from `fts_kind_table_name(kind)`, which the rest
  of admin already trusts for interpolation and which validates kind
  characters upstream. No change.
- Column names come from `fts_column_name(path, is_recursive)`. That
  helper sanitizes paths into `[a-zA-Z0-9_]` (see tests at
  `bootstrap.rs:1956-1983`). Safe to interpolate.
- Fall back to quoted identifiers (`"col"`) in the SELECT if review
  prefers defense-in-depth; the coordinator already does this
  pattern.

## Tests

### Rust (primary regression coverage)

Add to `crates/fathomdb-engine/src/admin/mod.rs` next to the existing
drift tests (line 4197):

1. `check_semantics_clean_on_weighted_fts_schema_does_not_panic` —
   register a weighted FTS property schema via
   `register_fts_property_schema_with_entries` (two specs: one
   scalar, one recursive). Insert at least one node. Call
   `check_semantics()`. Assert returns `Ok`, `drifted_property_fts_rows
   == 0`.
2. `check_semantics_detects_drifted_property_fts_text_weighted` —
   same setup as (1). Then manually `UPDATE fts_props_<kind>` to
   mutate one per-column value to a string that the schema would not
   produce. Call `check_semantics()`. Assert
   `drifted_property_fts_rows == 1`.
3. `check_semantics_mixed_weighted_and_non_weighted_schemas` —
   register one weighted kind and one non-weighted kind, insert
   nodes into both, call `check_semantics()`. Assert clean
   (0 drift across both).

### Python (cross-language smoke)

Add to `python/tests/test_admin.py` (or create it if missing):

- `test_check_semantics_survives_weighted_fts_registration` —
  mirrors the Memex reproduction. Register weighted FTS property
  schema(s), open a new connection, call `admin.check_semantics()`,
  assert no raise and `dangling_edges == 0`, `orphaned_chunks == 0`.

### TypeScript / Go

Not required for 0.5.2 — the bug lives entirely in Rust and is
exercised the same way from any SDK. Cross-SDK coverage can be added
in 0.5.3 if we want parity with the Python smoke test.

## What this design deliberately does not do

- Does not change the semantics report shape, field names, or any
  public API.
- Does not alter the weighted FTS table schema or the
  `register_fts_property_schema*` surface.
- Does not touch the search/rank path, the rebuild actor, or the
  coordinator.
- Does not introduce a new per-column drift metric — drift is still
  counted per row to stay consistent with the non-weighted arm and
  with the existing `drifted_property_fts_rows` field contract.

## Risk

- **Shape-detection correctness** — if a future per-kind FTS table
  ever has both a `text_content` column AND per-path columns (e.g.,
  a migration in flight), the current "if text_content present, use
  legacy path" branches to the simpler query and would miss drift on
  the per-path columns. No such shape exists in 0.5.1; 0.6.x
  migrations that introduce one would need to revisit this helper
  alongside their migration.
- **Dynamic SQL** — column-name interpolation is safe under
  `fts_column_name`'s sanitization, but the review should confirm
  by reading
  `crates/fathomdb-schema/src/bootstrap.rs::fts_column_name` and its
  tests.
- **Performance** — the weighted arm issues a per-kind SELECT with N
  columns and iterates rows in Rust. Same cost envelope as the
  non-weighted arm; both are already
  `O(rows_per_kind)` and the diagnostic is not on a hot path.

## CHANGELOG entry

```
### Fixed

- `Admin::check_semantics()` (and every SDK wrapper: `check_semantics`
  in Python, `checkSemantics` in TypeScript, `fathom-integrity check`
  in Go) no longer raises `SqliteError: no such column:
  fp.text_content` when any registered FTS property schema uses the
  per-column (weighted) shape. Drift detection now probes the per-kind
  FTS table and compares values column-by-column against the same
  extractor the rebuild actor uses.
```
