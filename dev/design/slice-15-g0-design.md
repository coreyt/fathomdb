# Slice 15 — G0 Canonical Identity Substrate (design memo)

> KEYSTONE. Schema `step_id 12` / `SCHEMA_VERSION 11→12`. Consumes
> `dev/adr/ADR-0.8.0-canonical-identity-substrate.md` (✅ SIGNED 2026-06-03).
> Transaction-time only (Option 2A): valid-time / G11 / G5 are reserved, additive-later.
>
> **Amended by Slice 31 (2026-06-05):** active-row uniqueness is scoped to
> `logical_id` ALONE (substrate ADR **Decision 5**, HITL-SIGNED), not the compound
> `(logical_id, kind)` this memo originally described. `kind` is payload/
> classification (nodes) / relationship-type (edges), never identity. Step-12 was
> amended in place (no `SCHEMA_VERSION` bump). The `(logical_id, kind)` references
> below read as `logical_id` alone.

## 1. What lands

Two additive nullable columns on **both** `canonical_nodes` and `canonical_edges`:

- `logical_id TEXT` — the stable cross-re-ingestion identity. NULL = legacy /
  own-identity row (not a supersession participant).
- `superseded_at INTEGER` — the transaction-time supersession tombstone. NULL =
  currently-active row; non-NULL = the cursor at which a newer version superseded it.

Plus, per table, a partial UNIQUE INDEX `(logical_id) WHERE superseded_at IS NULL`
(scoped to `logical_id` alone — Decision 5; amended in place by Slice 31)
and the folded G4/G5 read indexes (`canonical_nodes(kind)`, `canonical_edges(from_id)`,
`canonical_edges(to_id)`). All in one migration step (one accretion offset budget).

## 2. Exemption-marker rationale (ALTER…ADD with no DROP)

`check_migration_accretion` (`fathomdb-schema/src/lib.rs`) rejects SQL that
`adds_schema` (`CREATE TABLE` / `ADD COLUMN`) but `!names_removal` (no `DROP`) and
`!has_exemption`. Step 12 is pure additive `ALTER … ADD COLUMN` (no DROP), so it would
be rejected without the `-- MIGRATION-ACCRETION-EXEMPTION: G0 transaction-time identity
substrate` marker. The marker is therefore **load-bearing**, not decorative — the
accretion-guard test asserts the step passes the guard **only because** of the marker
(mirroring `ac_049`). Precedent: the step-8 `source_id` additive ALTER carries the same
shape (it relied on the offset budget; step 12 uses the explicit marker).

## 3. Tombstone-then-insert ordering + same-txn atomicity

Supersession happens inside `commit_batch`, in the single `BEGIN IMMEDIATE … COMMIT`
that carries the whole write batch. When a Node/Edge write carries `logical_id = Some(lid)`:

```sql
-- 1. tombstone the prior active version (no-op if none) — keyed by logical_id alone (Decision 5):
UPDATE <canonical_table>
   SET superseded_at = :cursor
 WHERE logical_id = :lid AND superseded_at IS NULL;
-- 2. insert the new active row (logical_id set, superseded_at implicitly NULL):
INSERT INTO <canonical_table>(write_cursor, kind, …, logical_id) VALUES(:cursor, :kind, …, :lid);
```

Because both statements run in the same transaction, a reader **never** observes two
active rows for one `logical_id` (the partial-unique index would reject that
structurally) nor zero rows mid-supersession. The ordering is tombstone-**then**-insert:
if it were insert-then-tombstone, the insert would momentarily create a second active row
and trip the partial-unique index. Idempotent: re-writing the same `logical_id` supersedes
the current active row and leaves exactly one active version; running it twice within a
batch also converges (the second write tombstones the first write's row).

When `logical_id = None` the UPDATE is skipped entirely and the INSERT is byte-for-byte
the prior behavior with `logical_id` NULL — **no behavior change for the unfiltered/legacy
write path** (guardrail: unfiltered byte-identity + recall floor un-regressed).

## 4. The NULL-on-legacy-rows rule (load-bearing)

This is the companion mechanism the forward-migration sign-off requires.

- The migration is **in-place additive ALTER, no re-open, no data migration**. Legacy
  (pre-0.8.0) rows read `logical_id = NULL` — they are not back-filled.
- An active row with NULL `logical_id` is a **legacy / own-identity** row. SQLite treats
  each NULL as **distinct** in a UNIQUE index, so many NULL-`logical_id` active rows
  coexist without colliding. This NULL-safety is **load-bearing**: if NULL legacy rows
  ever collided the migration would be wrong. Pinned by test (b).
- By-id resolution (future G2 `read.get(logical_id)`) finds a legacy row by `logical_id`
  **only once it is rewritten** with a non-NULL `logical_id`. We do **not** synthesize
  `logical_id` for legacy rows (that would be a data migration, out of scope). Until
  rewritten, a legacy row stays reachable by its existing means (`write_cursor`).

## 5. `row_cursors` semantics (write_cursor-as-row-id, accepted deviation)

`WriteReceipt` gains `row_cursors: Vec<u64>` — the per-row `write_cursor` of each row in
the batch, 1:1 with input order. `write_inner` already allocates one cursor per row
(`base_cursor + i + 1`); `row_cursors` simply surfaces that allocation. The existing
scalar `cursor` (the batch's final/high-water cursor) is unchanged.

This reuses the existing monotonic `write_cursor` as the per-row identity rather than
adding a dedicated `row_id` column or a `restore_provenance` payload. Per the ADR's
flagged deviation, HITL **ACCEPTED `write_cursor`-as-row-id for 0.8.0** and **DEFERRED**
the dedicated `row_id` + `restore_provenance` to a later additive slice. Slice 15 still
FLAGS this in `output.json.blockers_encountered` (sign-off ≠ silent reconciliation), but
lands the accepted shape. Bindings: `row_cursors` Py (`Vec<u64>`) ≡ TS (`rowCursors:
number[]`, per-element `u64→i64` at the napi boundary), cross-binding-equivalent.

## 6. Op-store cascade under supersession (Decision 4)

The existing op-store cascade contract is **preserved as-is**: `commit_batch` already
commits the whole batch (canonical rows, `latest_state` `operational_state` upserts,
`append_only_log` `operational_mutations` appends, projection terminals) in one
transaction, so supersession is atomic with any op-store effect by construction.
`latest_state` collections update to the new active payload in the same txn (existing
ON CONFLICT upsert); `append_only_log` collections accrete (supersession is a new
appended fact — prior entries retained). No cascade reshape was needed; flagged as
"preserved, not changed."

## 7. Reserved gap — shadow vec0/FTS5 reconciliation (Slice 16)

G0 lands the canonical active-vs-superseded distinction but does **NOT** excise stale
projection shadow rows (`search_index` FTS5, `_fathomdb_vector_rows`/`vector_default`
vec0, `_fathomdb_projection_terminal`) for superseded `write_cursor`s. Reconciling shadows
against `superseded_at` is **reserved Slice 16**. The read path must not depend on shadow
freshness for supersession correctness in the interim (it filters on canonical
`superseded_at IS NULL`).

## 8. Test plan

Schema (`fathomdb-schema/tests/migrations.rs`):

- extend `ac_046*` step-id pins (`…, 11` → `…, 11, 12`; counts +1);
- mirror `ac_049` accretion-guard: step-12 SQL passes **only** with the exemption marker;
- `s12_g0_adds_logical_id_superseded_at_columns_and_partial_unique_index`: apply step 12,
  assert both columns on both tables, both partial-unique-active indexes, the folded
  G4/G5 indexes, and `user_version == 12`.

Engine (`fathomdb-engine/tests/pr_g0_identity.rs`, NEW):

- (a) idempotent supersession upsert — re-writing the same `logical_id` leaves exactly one
  active row;
- (b) **partial-unique NULL-safety** — many NULL-`logical_id` rows coexist active;
- (c) two active rows with the same non-NULL `logical_id` collide (constraint fires);
  Slice 31 inverts the tail: a different `kind` for the same active `logical_id` is now
  REJECTED too (Decision 5 — `logical_id`-alone), and adds `s31_*_kind_change_reingest_supersedes`;
- (d) a superseded row + a new active row for the same `logical_id` coexist;
- (e) `row_cursors` is 1:1 with the batch (Rust; X1 mirrors Py/TS);
- (f) Pack-1/legacy upgrade — open a pre-step-12 DB, confirm columns + indexes land and old
  rows back-fill NULL and stay queryable.

Migration idempotence / crash-safety: applying step 12 twice (or to a from-scratch DB) is a
no-op the second time (`IF NOT EXISTS` indexes; `user_version` gate skips replayed ALTERs);
a crash after the `user_version=12` commit re-opens cleanly (open path is re-entrant — the
ALTERs do not re-run).
