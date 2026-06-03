---
title: ADR-0.8.0-canonical-identity-substrate
date: 2026-06-02
target_release: 0.8.0
desc: Settle the canonical-identity substrate (G0 keystone) for 0.8.0. Inherits Option 2A (design bi-temporal-aware, ship the minimal transaction-time subset) from ADR-0.8.0-agent-memory-retrieval-and-identity. Settles four substrate questions as decisions, authorizes the exact Slice-15 schema delta verbatim (logical_id + superseded_at on canonical_nodes AND canonical_edges; partial-unique-active index; folded G4/G5 indexes; accretion exemption; SCHEMA_VERSION 11→12), states the forward-migration policy, flags the write_cursor-as-row-id deviation for HITL, and names the shadow vec0/FTS5 reconciliation as a reserved Slice-16 follow-on.
blast_radius: src/rust/crates/fathomdb-schema/src/lib.rs (MIGRATIONS step 12; SCHEMA_VERSION 11→12; check_migration_accretion exemption marker); src/rust/crates/fathomdb-engine/src/lib.rs (PreparedWrite::Node/Edge logical_id; validate_write; commit_batch tombstone-then-insert; WriteReceipt.row_cursors); dev/roadmap/0.8.0.md:84-108 (substrate scope + forward-migration policy); dev/plans/0.8.0-implementation.md Slice 15 (keystone) + Slice 16 (reserved gap); dev/design/0.8.0-agent-memory-fit.md §4/§7 (G0 keystone, G11 deferred); fathomdb-py / fathomdb-napi (row_cursors parity)
status: draft, HITL-required
origin: ADR-0.8.0-agent-memory-retrieval-and-identity Q2 (Option 2A recommendation, lines ~178-195); dev/roadmap/0.8.0.md:84-108 (originally-deferred substrate scope, moved to 0.8.0 by HITL 2026-05-24); dev/design/0.8.0-agent-memory-fit.md §8b Pillar 3 (bi-temporal world-model literature)
---

# ADR-0.8.0 — Canonical-identity substrate (G0 keystone)

**Status:** ✅ **SIGNED / accepted (HITL 2026-06-03).** Slice 15 is gate-clear to prompt.

> **HITL sign-off 2026-06-03 (substrate gate package — completes the partial sign-off of 2026-06-02).**
> The 2026-06-02 session signed Q2 (=Option 2A) and Q4 (=edges carry temporal columns). The three
> remaining substrate items are now **signed** at the §3 keystone gate:
> - **Decision 4 — op-store cascade under supersession: RATIFIED as-is.** Supersession + cascade
>   in one transaction (atomic tombstone-then-insert with the write batch); `latest_state` updates
>   to the new active row; `append_only_log` accretes; vec0/FTS5 projection shadows NOT cascaded by
>   G0 (deferred to reserved Slice 16).
> - **Forward-migration policy: SIGNED = in-place additive `ALTER` (no re-open, no data migration).**
>   Accepted consequence: legacy (pre-0.8.0) rows carry `logical_id = NULL` until rewritten; the
>   engine owns a documented NULL-on-legacy-rows rule (G2 `read.get(logical_id)` resolves a pre-0.8.0
>   row by `logical_id` only once it is rewritten — it stays reachable by its existing means meanwhile).
> - **FLAGGED `write_cursor`-as-row-id deviation: ACCEPTED for 0.8.0 G0; dedicated `row_id` +
>   `restore_provenance` DEFERRED** to a later additive slice. Rationale on record: `write_cursor`
>   is a sufficient per-version identity for G2 + supersession history; the deferral is bounded and
>   additively-reversible (no cursor-renumber slice and no committed `restore_provenance` consumer is
>   on the 0.8.0 roadmap; `recover/restore` are SDK-absent under the recovery-denylist). Revisit only
>   if a future slice renumbers/compacts cursors or recovery requires structured provenance.
>
> Slice 15 still FLAGS the `write_cursor`-as-row-id deviation in its `output.json` per its contract
> (sign-off ≠ self-resolution licence); it now lands the accepted shape rather than escalating it.

This ADR settles the **storage substrate** for canonical identity and
supersession that 0.8.0's knowledge-store (Memex et al.) consumes. It is the
**G0 keystone**: Slice 15 implements exactly this delta; Slices 20 (G8), 30
(G2/G3) hang off it. The five questions below are settled **as decisions** so
Slice 15 executes a fixed contract with no open menu, and so HITL signs off a
single substrate shape rather than a set of options.

## Inherited constraint — Option 2A (binding)

`ADR-0.8.0-agent-memory-retrieval-and-identity` Q2 **recommends Option 2A** and
directs this ADR to **design for the bi-temporal end-state and implement the
minimal subset**:

> The schema and writer contract are chosen so adding **valid-time** later is
> **additive, not a reshape**.

This ADR **inherits Option 2A as a binding constraint.** 0.8.0 ships
**transaction-time only** (`superseded_at` ≈ system-expiry / supersession time).
**Valid-time (`t_valid`/`t_invalid`, the G11 full bi-temporal half), graph
traversal (G5), and edge invalidation are NOT implemented in 0.8.0** — but the
column shape, the invalidate-not-delete semantics, and the writer contract are
chosen so they can be added **purely additively** with no reshape of the columns
landed here. The world-model reference design (Zep/Graphiti; arXiv 2501.13956,
confirmed in `0.8.0-agent-memory-fit.md` §8b Pillar 3) is **four timestamps**
(transaction created/expired + real-world valid/invalid) with contradiction
handled by **invalidating, not deleting**. We land the transaction-time half of
that model now and reserve the valid-time half.

---

## The four substrate questions — settled as DECISIONS

### Decision 1 — Column shape (transaction-time now, additive valid-time later, no reshape)

**DECISION.** Both `canonical_nodes` and `canonical_edges` gain **two additive
nullable columns**: `logical_id TEXT` (the stable cross-re-ingestion identity)
and `superseded_at INTEGER` (the transaction-time supersession tombstone; NULL =
currently-active row). **No existing column is reshaped, retyped, or dropped.**

This is the **transaction-time** half of the bi-temporal model. `superseded_at`
plays the role of the bi-temporal **transaction-expired** timestamp. The
**transaction-created** timestamp is already carried implicitly by the existing
monotonic `write_cursor` column (every row records the cursor at which it was
written), so no separate `created` column is required now.

**Reshape-safety (Option 2A honored).** A later release adds **valid-time** as
**two further additive nullable columns** — `t_valid INTEGER` and
`t_invalid INTEGER` — on the same two tables, via the identical additive-`ALTER`
mechanism. Because:

- the active-row predicate landed now is `superseded_at IS NULL` (a
  **transaction-time** predicate) — it does not need to change when valid-time
  arrives; valid-time filtering is a **further `AND`** on the read path, not a
  redefinition of the active row;
- `logical_id` is the identity key under both the transaction-time-only and the
  full bi-temporal model — its uniqueness scope (active rows per `(logical_id,
  kind)`) is unchanged by adding valid-time;
- nullable additive columns never force a back-fill of, or a type change to, the
  columns landed here,

adding valid-time later is **additive, not a reshape**. This is the precise
property Option 2A requires and the "implement supersession twice" outcome
(`0.8.0.md:77-79`) it exists to prevent.

**Why not land valid-time now (reject 2C):** there is no validated 0.8.0
consumer for point-in-time queries (`agent-memory-retrieval-and-identity` Q2,
Option 2C *Against*), and it would over-commit a release already carrying
identity + knowledge-store + retrieval. Reserving the shape (2A) gets the
one-design-pass benefit without the implementation cost.

### Decision 2 — Invalidate-not-delete (explicit, bi-temporal-compatible)

**DECISION.** Supersession is **invalidate-not-delete**. When a write supersedes
a prior version of a `logical_id`, the prior canonical row is **retained** and
its `superseded_at` is **set to the superseding write's cursor** (a timestamp
tombstone); the new version is **inserted as a fresh active row**
(`superseded_at IS NULL`). **No `DELETE` of canonical state occurs on
supersession.** History is queryable by selecting rows with non-NULL
`superseded_at` for a `logical_id`.

This is exactly the world-model "invalidate, do not delete" semantic
(`agent-memory-retrieval-and-identity` Q2, point 2) and is **bi-temporal
forward-compatible**: when valid-time arrives, the same tombstone-then-insert
ordering applies, additionally setting the superseded edge's `t_invalid` to the
new edge's `t_valid` (the Graphiti contradiction-resolution rule), with no change
to the transaction-time tombstone written here.

The mechanism in `commit_batch` is **tombstone-then-insert in a single
transaction** (see "Op-store cascade contract" below for atomicity):

```sql
-- when logical_id is Some:
UPDATE <canonical_table>
   SET superseded_at = :cursor
 WHERE logical_id = :logical_id AND kind = :kind AND superseded_at IS NULL;
-- THEN
INSERT INTO <canonical_table> (..., logical_id, superseded_at) VALUES (..., :logical_id, NULL);
```

Hard erasure (GDPR `recover --purge-logical-id`) and resurrection
(`recover --restore-logical-id`) remain **distinct, recovery-surface verbs**
(`0.8.0.md:93-103`) — they are NOT part of the supersession write path and are
out of scope for this substrate slice.

### Decision 3 — Edges carry temporal columns too (not nodes only) — answers Q4

**DECISION.** **Edges carry the identity + temporal columns, not just nodes.**
Both `logical_id TEXT` and `superseded_at INTEGER` are added to **`canonical_edges`
as well as `canonical_nodes`**, with the **same** partial-unique-active index and
the **same** tombstone-then-insert supersession semantics.

**Rationale.** The world-model graph (G5/G11) puts temporal validity on **fact
edges**, not only nodes (`agent-memory-retrieval-and-identity` Q2 point 3 / Q4;
`0.8.0-agent-memory-fit.md` §8b Pillar 3 — edge invalidation is the unit of
contradiction resolution). Adding the columns to edges now is what makes the
later valid-time edge-invalidation work (G11 full) additive rather than a second
migration over `canonical_edges`. Putting identity on nodes only would force
exactly the second schema migration Option 2A exists to avoid. This ADR therefore
answers the parent ADR's **Q4 = edges carry `logical_id` + `superseded_at`**.

0.8.0 **implements** transaction-time supersession on edges (tombstone-then-insert
keyed by `(logical_id, kind)`); it does **not** implement edge *valid-time*
invalidation (G11 full) — that is the reserved column pair from Decision 1.

### Decision 4 — Op-store cascade contract under supersession (extended to invalidate-not-delete)

**DECISION.** Supersession of a canonical row and any cascading op-store effect
occur in the **same single transaction** as the canonical tombstone-then-insert,
preserving the existing op-store cascade contract (`0.8.0.md:89-90`) under the
invalidate-not-delete semantics:

- **Atomicity.** The `UPDATE …superseded_at` (tombstone) and the `INSERT` (new
  active row) for a given `logical_id` commit atomically with the rest of the
  write batch in `commit_batch` (single `BEGIN IMMEDIATE … COMMIT`). A reader
  never observes two active rows for one `(logical_id, kind)`, nor zero rows
  mid-supersession. (This is what the partial-unique-active index in Decision 1
  enforces structurally.)
- **`latest_state` op collections** track the **current** canonical version: a
  supersession updates the latest-state projection to the new active row's
  payload in the same txn. The superseded canonical row is retained (Decision 2)
  but is no longer the "latest".
- **`append_only_log` op collections** are **not** rewritten by supersession —
  by definition they accrete; the supersession is a new appended fact, and prior
  appended entries are retained. This matches invalidate-not-delete: supersession
  adds, it does not rewrite history.
- **Projection shadows are NOT cascaded by G0.** Excising stale vec0/FTS5 shadow
  rows for superseded canonical versions is explicitly **deferred** — see "Shadow
  reconciliation" below (reserved Slice 16). G0 leaves the active-vs-superseded
  distinction in canonical state; the read path is responsible for not surfacing
  superseded rows until the shadow tables are reconciled.

---

## AUTHORIZED Slice-15 schema delta (verbatim — Slice 15 executes this exactly)

> **✅ RENUMBER APPLIED 2026-06-03 (at Slice 15 close).** Slice 5 (G1 FTS5 tokenizer upgrade)
> consumed `step_id 11` / `SCHEMA_VERSION 11` on `main`, so this delta landed as **`step_id: 12`**,
> bumping **`SCHEMA_VERSION 11 → 12`**. The literal code blocks below are now updated to match what
> shipped (`fathomdb-schema/src/lib.rs:294-310`, `SCHEMA_VERSION = 12` at `:6`). Everything else in
> the delta landed unchanged.

Slice 15 appends a single migration step. The SQL below is the **authorized
delta**; it MUST carry the accretion-exemption marker because
`check_migration_accretion` (`fathomdb-schema/src/lib.rs:362-373`) rejects
`CREATE TABLE`/`ADD COLUMN` SQL that names no `DROP` and carries no
`-- MIGRATION-ACCRETION-EXEMPTION: ` marker.

```rust
// Append AFTER the step-11 Migration (fathomdb-schema/src/lib.rs:269-280).
Migration {
    step_id: 12,
    sql: "-- MIGRATION-ACCRETION-EXEMPTION: G0 transaction-time identity substrate
          ALTER TABLE canonical_nodes ADD COLUMN logical_id TEXT;
          ALTER TABLE canonical_nodes ADD COLUMN superseded_at INTEGER;
          ALTER TABLE canonical_edges ADD COLUMN logical_id TEXT;
          ALTER TABLE canonical_edges ADD COLUMN superseded_at INTEGER;
          CREATE UNIQUE INDEX IF NOT EXISTS canonical_nodes_logical_active_idx
              ON canonical_nodes(logical_id, kind) WHERE superseded_at IS NULL;
          CREATE UNIQUE INDEX IF NOT EXISTS canonical_edges_logical_active_idx
              ON canonical_edges(logical_id, kind) WHERE superseded_at IS NULL;
          CREATE INDEX IF NOT EXISTS canonical_nodes_kind_idx
              ON canonical_nodes(kind);
          CREATE INDEX IF NOT EXISTS canonical_edges_from_id_idx
              ON canonical_edges(from_id);
          CREATE INDEX IF NOT EXISTS canonical_edges_to_id_idx
              ON canonical_edges(to_id);",
},
```

And the version bump:

```rust
pub const SCHEMA_VERSION: u32 = 12; // was 11 (fathomdb-schema/src/lib.rs:6)
```

**Authorized elements (each REQUIRED, none optional):**

1. **Columns.** `ALTER TABLE canonical_nodes` **AND** `ALTER TABLE
   canonical_edges`, each `ADD COLUMN logical_id TEXT` + `ADD COLUMN
   superseded_at INTEGER`. Nullable; legacy rows back-fill to NULL (an active
   row with NULL `logical_id`).
2. **Partial UNIQUE INDEX (NULL-safe), one per table.** `(logical_id, kind)
   WHERE superseded_at IS NULL`. **NULL-safety is load-bearing:** SQLite treats
   each NULL `logical_id` as distinct, so legacy back-filled rows (NULL
   `logical_id`) **never collide** with one another; only rows that opt into a
   non-NULL `logical_id` are constrained to one active version per
   `(logical_id, kind)`.
3. **Folded indexes (one offset budget):** `canonical_nodes(kind)` (G4 list),
   `canonical_edges(from_id)`, `canonical_edges(to_id)` (G5 traversal). Folded
   into this migration because the accretion regime spends one offset budget per
   schema-touching step; landing them here avoids a second touch.
4. **Accretion exemption marker.** `-- MIGRATION-ACCRETION-EXEMPTION: G0
   transaction-time identity substrate` — REQUIRED. Without it the guard rejects
   the `ADD COLUMN`s (it sees schema-adding SQL with no `DROP`). Slice 15's
   accretion-guard test (mirroring `ac_049`) must assert the marker is the **only**
   thing letting the guard pass.
5. **`SCHEMA_VERSION 11 → 12`** at `fathomdb-schema/src/lib.rs:6` (witnessed
   current = 11). The migration registry must stay contiguous (the migrate loop
   asserts `step_id == current + 1`).

**Idempotence / Pack-1-upgrade requirement.** `IF NOT EXISTS` on every index;
the `ALTER … ADD COLUMN`s run once (guarded by the `user_version` gate — step 12
only runs when `user_version < 12`). Applying step 12 to a from-scratch DB and to
a legacy 0.7.x DB must both land identically (columns present, legacy rows
back-fill NULL); re-open is a no-op.

---

## Forward-migration policy (DECISION)

**DECISION: in-place additive `ALTER` migration. No re-open, no data migration.**

The roadmap (`0.8.0.md:104-108`) names two candidate shapes — **in-place
column-add migration** vs **re-open required** — and leaves the choice to this
ADR. We choose **in-place additive `ALTER`**:

- The columns are **purely additive and nullable** (`feedback_no_data_migration`
  regime, `0.8.0.md:104`). SQLite `ALTER TABLE … ADD COLUMN` of a nullable column
  is an O(1) catalog-only operation — it does **not** rewrite existing rows. There
  is **no data migration**: legacy rows simply read NULL for the new columns.
- A 0.7.x → 0.8.0 upgrade therefore needs **no re-open / re-ingest**. The standard
  open-time `migrate()` path runs step 12 transactionally (`BEGIN IMMEDIATE …
  COMMIT`, advancing `user_version` to 12 atomically per `apply_one`), and the DB
  is immediately usable.
- Legacy rows are **correct by construction** for the new verbs: a NULL
  `logical_id` means "not a supersession participant," which is the right default
  for pre-0.8.0 data. The verbs (`get`, supersession) operate only on rows whose
  `logical_id` was set by a 0.8.0+ writer. **No back-fill of `logical_id` is
  required or performed** — assigning synthetic logical ids to legacy rows would
  be a behavioral change, not a migration, and is explicitly out of scope.

**Why not re-open:** re-open would only be justified if the new columns required
recomputed values from canonical state (a true data migration). They do not —
NULL is the correct legacy value — so re-open would impose cost and an upgrade
gate with no correctness benefit. Rejected.

---

## FLAGGED open deviation (do NOT self-resolve — for HITL)

> **DEVIATION — `write_cursor`-as-row-id vs roadmap `row_id` + `restore_provenance`.**
>
> The roadmap substrate scope (`0.8.0.md:84-87`) lists **`row_id`** as a named
> column and an **optional `restore_provenance` payload** column alongside
> `logical_id` / `superseded_at`. The authorized Slice-15 delta above does **NOT**
> add a dedicated `row_id` column or a `restore_provenance` column. Instead Slice
> 15 reuses the existing monotonic **`write_cursor`** as the per-row identity and
> surfaces it to callers as **`WriteReceipt.row_cursors`** (`lib.rs:794`,
> populated in `write_inner` `:1774`; mirrored as `row_cursors` in
> `fathomdb-py` / `fathomdb-napi`).
>
> This is a **recorded, deliberate deviation**, surfaced here for HITL — it is
> **not silently reconciled** and is **not dropped**. Trade-off summary for the
> sign-off decision:
> - **For `write_cursor`-as-row-id (what Slice 15 ships):** the cursor already
>   exists on every canonical row, is already monotonic and unique-per-write, and
>   needs no new column or offset budget; it satisfies the by-id read (G2) and the
>   supersession-history need without schema growth.
> - **Against / what is deferred:** a dedicated `row_id` decoupled from the write
>   cursor, and the `restore_provenance` payload that `recover --restore-logical-id`
>   (`0.8.0.md:100-103`) may want to carry, are **not** landed. If HITL requires a
>   row identity independent of `write_cursor`, or a structured restore-provenance
>   payload, that is an **additional additive column** in a later slice — it does
>   not block G0, but it must be a conscious sign-off, not an accident.
>
> **Action requested of HITL at substrate sign-off:** accept
> `write_cursor`-as-row-id for 0.8.0 G0 (deferring dedicated `row_id` +
> `restore_provenance` to a later additive slice), OR direct that `row_id` /
> `restore_provenance` be folded into the Slice-15 delta. Slice 15 must FLAG this
> same deviation in its `output.json.blockers_encountered` and **must not
> self-resolve** it (`0.8.0-implementation.md` Slice 15 HITL note, `:613-614`).

---

## NAMED follow-on (reserved Slice 16 candidate)

> **RESERVED GAP — shadow vec0/FTS5 reconciliation against `superseded_at`
> (Slice 16 candidate).**
>
> G0 lands the **canonical** active-vs-superseded distinction but does **NOT**
> excise stale **projection shadow rows** for superseded canonical versions:
> `_fathomdb_vector_rows` (vec0), `search_index` (FTS5), and
> `_fathomdb_projection_terminal` retain entries for superseded `write_cursor`s.
> Those stale shadow rows compete for phase-1 prefilter slots and can surface a
> superseded body in search until reconciled.
>
> Reconciling the shadow tables against `superseded_at` (excising or filtering
> shadow rows whose canonical version is now superseded; modeled on
> `excise_source_inner`) is **required follow-on work**, **explicitly named here**
> as a **reserved-gap Slice 16 candidate** (`0.8.0-implementation.md` reserved
> follow-on 16, `:616-619`). **It is later work — G0 does not do it.** The 0.8.0
> read path must, in the interim, not depend on shadow-table freshness for
> supersession correctness (it filters on canonical `superseded_at IS NULL`).

---

## Consequences if signed off

- Slice 15 executes the **verbatim** delta above (schema crate: step 12 +
  `SCHEMA_VERSION 12`; engine crate: `PreparedWrite::Node/Edge.logical_id`,
  `validate_write`, `commit_batch` tombstone-then-insert, `WriteReceipt.row_cursors`;
  bindings: `row_cursors` Py+TS parity). **No design choices remain for Slice 15**
  — it consumes this ADR.
- 0.8.0 ships **transaction-time** supersession on **both** nodes and edges,
  invalidate-not-delete, in-place additive migration.
- **Reserved (not done in 0.8.0):** valid-time columns (`t_valid`/`t_invalid`),
  edge valid-time invalidation (G11 full), graph traversal (G5), shadow
  reconciliation (Slice 16), dedicated `row_id` + `restore_provenance` (pending
  HITL on the flagged deviation).

## Open questions for HITL (sign-off gate)

1. Accept **Decision 1–4** (column shape; invalidate-not-delete; edges carry
   temporal columns = **Q4 yes**; op-store cascade under supersession) as the
   binding G0 substrate?
2. Accept the **forward-migration policy** = in-place additive `ALTER`, no
   re-open, no `logical_id` back-fill?
3. Resolve the **flagged `write_cursor`-as-row-id deviation**: accept for 0.8.0
   (defer `row_id` + `restore_provenance`), or fold them into the Slice-15 delta?
4. Confirm the **shadow vec0/FTS5 reconciliation** is correctly deferred to the
   reserved Slice 16 (G0 ships without it).
