# Slice 33 — `read.collection` / `read.mutations` cursor + limit hardening design memo

> Status: design-first memo for the Slice 33 implementation (op-store paginated
> read-back hardening under a genuine ~1M-row `operational_mutations` log).
> Depends-on: Slice 30 (CLOSED) — the governed `read.*` surface. Binds gap
> **G3 / F4-READ** + the new test names (no new AC id; `acceptance.md` is locked
> at 0.6.0 / max AC-073).

## 0. Problem statement

The governed `read.collection` / `read.mutations` verbs (Slice 30 / G3) read an
`append_only_log` collection's appended rows back over `operational_mutations`:

```sql
SELECT id, collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
FROM operational_mutations
WHERE collection_name = ?1 AND id > ?2
ORDER BY id
LIMIT ?3
```

`operational_mutations` (step-4 DDL, `fathomdb-schema/src/lib.rs:127`) has only
its `id INTEGER PRIMARY KEY AUTOINCREMENT`. There is **no index on
`collection_name`**. SQLite therefore satisfies the query by walking the
id-ordered PK (`id > ?`) and filtering each row on `collection_name`. For a
*small* collection embedded inside a *large* multi-collection log this is
**O(rows scanned between matching ids)**, not **O(page)** — to fill one page of
N matching rows for `small` the engine may scan thousands of `bulk` rows it then
discards. Under a genuine ~1M-row log this turns each "next page" into a long
scan even though the result page is tiny.

`op-store.md:132` names this exactly: *"cursor/limit hardening under a genuine
~1M-row log is a reserved follow-on"* — this slice.

## (a) EXPLAIN QUERY PLAN — BEFORE (current `main`, no `(collection_name, id)` index)

Measured against a migrated in-memory DB seeded with a multi-collection log
(`small` = every 100th row, `bulk` = the rest), the read_collection SELECT plans
as:

```text
SEARCH operational_mutations USING INTEGER PRIMARY KEY (rowid>?)
```

This is the pathology: the plan rides the **id PK** (`rowid>?`), scanning the
id-ordered log and filtering `collection_name = ?` row-by-row. No index covers
`collection_name`; the per-page cost is bounded by *rows-scanned*, not
*page-size*.

(Note: there is no `USE TEMP B-TREE FOR ORDER BY` here, because the PK walk is
already id-ordered. But that walk is the wrong access path for a small
collection inside a large log.)

## (b) The step-13 additive index decision + EXPLAIN AFTER

**Decision:** add a **forward-only additive step-13 migration**, bumping
`SCHEMA_VERSION 12 → 13`:

```sql
CREATE INDEX IF NOT EXISTS operational_mutations_collection_id_idx
    ON operational_mutations(collection_name, id);
```

The composite `(collection_name, id)` index is the correct shape because the
query's leading predicate is `collection_name = ?1` (equality) and the ordering

+ cursor are both on `id`. With `collection_name` fixed by the equality, the
index's second column `id` is already in ascending order, so the index serves
**both** the `id > ?2` cursor range **and** the `ORDER BY id` with no temp
B-tree.

**EXPLAIN AFTER** (same seed, with the index present):

```text
SEARCH operational_mutations USING INDEX operational_mutations_collection_id_idx (collection_name=? AND id>?)
```

Index-driven: no `SCAN`, no `SEARCH … USING INTEGER PRIMARY KEY`, no `USE TEMP
B-TREE FOR ORDER BY`. The page cost is now **O(page)** — the engine seeks
straight to `(collection_name, after_id)` in the index and reads forward exactly
`LIMIT` index entries.

**Accretion guard:** the accretion guard (REQ-045,
`check_migration_accretion` + `scripts/agent-lint-migrations.py`) fires only on
`CREATE TABLE` / `ADD COLUMN` without an offsetting `DROP` or exemption marker. A
**`CREATE INDEX`-only** step adds no table and no column, so it is **not**
flagged and needs **no** exemption marker (cf. step-8, which carried indexes
*alongside* `ADD COLUMN` and so needed the marker for the column adds; step-13
adds an index alone). No table reshape, no column add, no drop — purely
additive.

**Forward-only / idempotency:** `CREATE INDEX IF NOT EXISTS` is idempotent
across a crash between the step's `BEGIN IMMEDIATE … COMMIT` and the
`user_version` bump (mirrors `apply_one`'s single-tx-per-step contract). Existing
v12 DBs gain the index on first open at v13; v13 DBs already have it.

## (c) Clamp + cursor edge cases to harden

`read_collection_in_tx` (`fathomdb-engine/src/lib.rs:3998`) already:

+ returns an empty `Vec` (no SELECT) for `limit == 0`;
+ clamps the SQL `LIMIT` to `min(limit, READ_COLLECTION_MAX_LIMIT)` (~1M).

The cursor `after = after_id.unwrap_or(0)` is correct for the happy path (ids are
≥ 1, autoincrement), but the edge cases to **pin** (and harden where needed) are:

| case | desired behaviour | current behaviour |
| --- | --- | --- |
| `limit == 0` | empty page, **no SELECT issued** | already correct (early return) |
| `limit > READ_COLLECTION_MAX_LIMIT` | clamped to ~1M, no error, no unbounded scan | already correct (`min`) |
| `after_id = Some(0)` / `None` | start from the beginning (all ids `> 0`) | already correct |
| `after_id = Some(negative)` | treat as "from the start" — never re-read with `id > negative` selecting nothing-unexpected; `id > -5` selects all rows, which is the intended start-of-log semantics | correct (all ids match `id > negative`), but **made explicit** by clamping `after` to `>= 0` so a negative cursor is normalized to the start and can never be confused with a row id |
| `after_id` past the end (beyond max id) | empty page | already correct (range is empty) |
| unknown / empty collection | empty page | already correct (equality matches nothing) |
| order stability across pages | strictly increasing `id`, no boundary overlap | already correct (`id > ?` excludes boundary) |

**Hardening applied:** normalize the cursor with `after_id.unwrap_or(0).max(0)`
so a negative `after_id` is explicitly clamped to the start of the log (defensive

+ self-documenting; semantically identical to today for ids ≥ 1 but removes the
"is a negative cursor a sentinel or a row id?" ambiguity). The SELECT shape is
unchanged so it continues to ride the new index. No signature change.

## (d) Test plan (RED first; extend `pr_g3_read_collection.rs`)

Extend `tests/pr_g3_read_collection.rs` (preserve every existing Slice-30
assertion). New tests:

1. **EXPLAIN index gate** (`read_collection_plan_is_index_driven_no_scan_no_temp_btree`)
   — mirrors the `pr_g8` plan gate. Open a real DB, migrate to head, seed a
   multi-collection log, run `EXPLAIN QUERY PLAN` of the read SELECT, assert the
   detail **contains** `operational_mutations_collection_id_idx` and contains
   **neither** `SCAN` **nor** `USE TEMP B-TREE FOR ORDER BY` **nor** `USING
   INTEGER PRIMARY KEY`. RED until step-13 lands.
2. **Bounded large-log pagination**
   (`read_collection_paginates_small_collection_inside_large_log`) — seed one
   small collection interleaved inside a much larger `bulk` log; paginate the
   small collection via `after_id` and assert the pages are correct + ordered +
   non-overlapping across the boundary, and that the union of pages equals the
   full small collection (row-count correctness across the page boundary; the
   EXPLAIN gate is the structural O(page) proxy).
3. **Clamp / cursor edge cases**
   (`read_collection_clamp_and_cursor_edge_cases`) — `limit == 0` → empty;
   `limit > READ_COLLECTION_MAX_LIMIT` → clamped (no error); `after_id` past the
   end → empty; negative `after_id` → starts from the beginning; unknown
   collection → empty; order stable across pages.

Schema side: extend `fathomdb-schema/tests/migrations.rs` — step-count `11 → 12`,
step-id vectors gain `13`, `SCHEMA_VERSION` assertion `12 → 13`, and a new
`s13_op_store_collection_index_*` test asserting the index exists after migrate
and is shaped `(collection_name, id)`.

## (e) No SDK signature / binding change

`read_collection` / `read_mutations` / `read_collection_dispatch` keep their
`(collection: &str, after_id: Option<i64>, limit: usize)` signatures; the
`ReaderRequest::ReadCollection` arm is unchanged; the Python / TypeScript
read.\* bindings and their functional-retrieve harnesses are untouched. This is
an internal correctness/perf hardening only — the observable return values are
unchanged (the EXPLAIN/perf gate is the evidence), so the Py/TS expectations do
not move.

## Out of scope (Slice 33)

No change to `commit_batch` / the write path; no change to the search path (the
Slice 10 byte-identity pin stays green); no reshape of `operational_mutations`
(additive index only); no new AC id; no recovery-suite edits; no release / CI /
version actions.
