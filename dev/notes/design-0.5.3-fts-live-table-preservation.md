# Design: Preserve live FTS table during async re-registration (0.5.3 hotfix)

**Date:** 2026-04-20
**Status:** Locked for implementation
**Scope:** 0.5.3 pre-tag hotfix (tag not yet pushed)

## Problem

Contract (`CHANGELOG.md:168`, `docs/reference/admin.md:47`, test docstring
`async_rebuild_tests.rs:519-520`): after `register_fts_property_schema_async`
on a kind that already has a registered schema, search reads must continue
to use the existing live FTS table until the async rebuild reaches `COMPLETE`.
No scan fallback.

Implementation diverges. `register_fts_property_schema_async`
(`crates/fathomdb-engine/src/admin/fts.rs:317-329`) unconditionally calls
`create_or_replace_fts_kind_table` inside the registration transaction.
That helper (`admin/fts.rs:619-623`) executes:

```sql
DROP TABLE IF EXISTS {table};
CREATE VIRTUAL TABLE {table} USING fts5(...)
```

The live data is wiped synchronously before the async rebuild actor
runs. Readers arriving during PENDING/BUILDING see an empty recreated
FTS table and return zero rows instead of the previous live content.

The test `read_during_re_registration_uses_live_fts_table` asserts 3
rows; under single-threaded/low-contention runs the actor often
finishes step 5 before the read, so the test "passes by luck." Under
`--test-threads>=32` the read wins the race and sees 0, producing the
reported 30%-flake.

Motivation (noted in the original comment at line 317-319): "handles
weighted-to-unweighted downgrade where a stale per-spec table would
otherwise remain." Valid concern — but currently applied to **every**
re-registration, including the shape-compatible case that is the
common path.

## Scope

In: `register_fts_property_schema_async` conditional drop/recreate.
In: shape-diff helper deciding when a drop is mandatory.
In: test re-enabled and reliable under `--test-threads=32`.
Out: `register_fts_property_schema` (eager mode) — eager path
semantics are "visible immediately"; eager callers already expect a
synchronous swap; no live-table invariant.
Out: first-registration path — no prior live table to preserve.
Out: writer-path FTS population (already correct).

## Invariant (post-fix)

In `register_fts_property_schema_async`, for a re-registration
(`had_previous_schema = true`):

- If the new schema is **shape-compatible** with the existing FTS
  table: do NOT drop. Leave the live table intact. Let the async
  rebuild actor's step 5 (atomic `DELETE FROM {table}` + repopulate
  from staging) perform the data transition. Readers during
  PENDING/BUILDING observe the old data until step 5 commits.
- If the new schema is **shape-incompatible**: drop/recreate in the
  registration transaction (current behavior). Readers during
  PENDING/BUILDING observe an empty table. This is unavoidable
  because the live table's columns cannot service the new schema.
  Document the degraded window.

For a first registration: the table may not exist at all (created at
write time or by actor's step 5 defensive CREATE IF NOT EXISTS). No
live data to preserve. Behavior unchanged.

## Shape compatibility

Two FTS table shapes are compatible iff:

1. Weighted-ness matches: either both have weighted specs
   (`any_weight` = true on both) or neither does.
2. Tokenizer matches: the resolved tokenizer string for the new
   schema equals the one currently bound to the table.
3. Column set matches: the sorted list of physical column names
   derived from the registered specs (or the `text_content` singleton
   for unweighted) equals the sorted list currently present on the
   existing FTS table.

Any other difference (adding a column, removing a column, changing
tokenizer, weighted↔unweighted flip) is shape-incompatible.

Implementation:

- Extract `fts_kind_table_shape(conn, kind) -> Option<FtsTableShape>`
  where `FtsTableShape` is `(tokenizer: String, columns: Vec<String>)`
  reading from `PRAGMA table_info` / `sqlite_master` for the per-kind
  FTS table. `None` if the table does not exist.
- Compute `desired_shape(entries, tokenizer)` returning the
  `FtsTableShape` that would result from `create_or_replace_fts_kind_table`.
- `shape_compatible(existing, desired) -> bool` compares the two.

Both helpers live in `admin/fts.rs` alongside the existing
`create_or_replace_fts_kind_table`.

## Change sites

### `admin/fts.rs:317-329` (sole behavior change)

Replace the unconditional block with:

```rust
let any_weight = entries.iter().any(|e| e.weight.is_some());
let tok = fathomdb_schema::resolve_fts_tokenizer(&tx, kind)
    .map_err(|e| EngineError::Bridge(e.to_string()))?;
let desired = desired_fts_shape(kind, entries, &tok);
let existing = fts_kind_table_shape(&tx, kind)?;
let must_drop = match &existing {
    None => false, // first registration or missing — actor step 5 creates
    Some(existing) => !shape_compatible(existing, &desired),
};
if must_drop {
    // Shape-incompatible re-registration: live table cannot service
    // the new schema. Drop now; readers during PENDING/BUILDING see
    // an empty table (documented behavior for this case).
    if any_weight {
        create_or_replace_fts_kind_table(&tx, kind, entries, &tok)?;
    } else {
        create_or_replace_fts_kind_table(&tx, kind, &[], &tok)?;
    }
}
// else: shape-compatible — preserve live table. Actor step 5 atomic
// swap repopulates data.
```

Nothing else in the function changes. The `fts_property_schemas`
upsert, the `fts_property_rebuild_state` row, and the
`fts_property_schema_registered` provenance event all remain identical.

### `rebuild_actor.rs` step 5 (defensive `CREATE IF NOT EXISTS`)

Already present at `rebuild_actor.rs:328-334`. No change. This path
now also catches the compatible-re-registration case where the live
table was preserved and step 5 proceeds with the existing columns.

### Test

`crates/fathomdb-engine/tests/async_rebuild_tests.rs:522` —
`read_during_re_registration_uses_live_fts_table` already asserts
the correct invariant. Must now pass deterministically under
`--test-threads=32`.

Add a companion test:
`read_during_shape_incompatible_re_registration_sees_empty_table`
asserting the degraded-window behavior when the shape DOES change
(e.g. adding a property path). This codifies the document's
distinction between the two cases so any future change to the invariant
is explicit.

### Docs

Update `docs/reference/admin.md:47` and `CHANGELOG.md:168`-area to
clarify the two cases:

> Shape-compatible re-registration: live FTS table is preserved;
> search continues to return pre-registration results until the
> async rebuild reaches COMPLETE. Shape-incompatible re-registration
> (column set or tokenizer change): live table is dropped at
> registration time; search returns zero rows until the rebuild
> reaches COMPLETE.

## Risks

- **Staging table collision.** If a new column set causes the actor's
  step 5 to attempt `INSERT INTO {table}(col1, col2, ...)` against a
  preserved table with the old columns, the insert fails with a
  column-mismatch error and the rebuild is marked FAILED. Mitigation:
  `shape_compatible` check guarantees the column set matches before
  we decide not to drop. Only shape-compatible re-registrations take
  the preserve path.
- **Tokenizer mismatch.** Same mitigation. If the tokenizer changed,
  `shape_compatible` returns false and we drop.
- **Schema dictionary drift.** `fts_property_schemas` row updates
  atomically in the same reg transaction. A reader that sees the new
  `fts_property_schemas` row but still reads from the old live table
  is fine: the table's column names match the old schema which is
  what the live data matches. Step 5's atomic swap changes both in
  the same transaction.

## Acceptance

- `cargo nextest run -p fathomdb-engine --test async_rebuild_tests
  --test-threads=32` passes 3 consecutive runs with zero flakes.
- `cargo nextest run --workspace` passes.
- `cargo clippy --workspace --all-targets -- -D warnings -A missing-docs` clean.
- `grep -R "no scan fallback" docs/ CHANGELOG.md` finds the updated wording.
- New companion test `read_during_shape_incompatible_re_registration_sees_empty_table` passes.

## Out of scope / follow-ups

- Refactoring step 5 of the actor to stage the new FTS table under a
  shadow name and `ALTER TABLE ... RENAME` atomic swap. That would
  remove the "degraded window" even for shape-incompatible
  re-registrations. Larger change, not in 0.5.3.
- `register_fts_property_schema` (eager mode) auditing. Out of scope;
  contract for eager is different.

## References

- Bug trace: `crates/fathomdb-engine/src/admin/fts.rs:317-329` +
  `:619-623`
- Contract docstring: `crates/fathomdb-engine/tests/async_rebuild_tests.rs:519-520`
- Actor step 5: `crates/fathomdb-engine/src/rebuild_actor.rs:319-457`
- User-facing contract: `docs/reference/admin.md:47`, `CHANGELOG.md:168`
