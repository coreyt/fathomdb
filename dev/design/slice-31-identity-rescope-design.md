# Slice 31 — G0 identity re-scope: active-uniqueness = `logical_id` ALONE (both tables)

> **Status:** design memo for the Slice 31 implementation. The decision it
> executes is **HITL-SIGNED 2026-06-05** (substrate gate). This memo records the
> carried decision, the model, the in-place amend mechanism, the edit list, and
> the test plan. It does **not** re-litigate the decision.

## 1. The decision (carried, signed)

FathomDB's canonical active-row uniqueness is scoped to **`logical_id` ALONE**,
uniformly on **BOTH `canonical_nodes` AND `canonical_edges`** (uniform, not
asymmetric). This **reverses** the compound `(logical_id, kind)` key that landed
in Slice 15 (schema step 12), which was an un-argued silent regression from
v0.5.x.

`kind` remains a **payload / classification** attribute on nodes and a
**relationship-type** attribute on edges — it is **never** an identity-scope
component again.

### Why (the now-argued rationale — see substrate ADR Decision 5)
- **Silent identity-fork bug (high severity).** Node + edge supersession was
  `UPDATE … WHERE logical_id = ? AND kind = ? …`; re-ingesting one `logical_id`
  with a *changed* `kind` matched no prior active row, and the compound unique
  index permitted the insert → a **second active row** for the same `logical_id`
  (a fork, not a supersession).
- **Slice 30 `read.get` lossy / nondeterministic (codex §9 [P2]).** The read
  collapse map is keyed by `logical_id`; with two active rows per `logical_id`,
  `read.get`/`get_many` returned an arbitrary one. `logical_id`-alone makes the
  `WHERE logical_id = ? AND superseded_at IS NULL` query yield ≤1 row → the read
  is deterministic **with zero read-API change**.
- **G8 consistency.** Slice 20/G8 already probes endpoints by `logical_id` alone
  (`WHERE logical_id = ? AND superseded_at IS NULL`); the compound write key
  contradicted that read.
- **Edge `kind` buys no real edge capability.** Edge `logical_id` is
  caller-provided and opaque (not derived from `(from,to)` or `kind`); an edge
  needing multiple active relationship facts between the same endpoints uses
  **distinct edge `logical_id`s**. Reusing one edge `logical_id` with a different
  `kind` should **supersede, not fork**.
- **v0.5.x precedent.** v0.5.6 keyed active uniqueness on `logical_id` alone for
  both tables; the compound scope was the unargued drift.
- **No consumer need.** The only "different kind allowed" case was a
  self-referential G0 test, not a use case.

## 2. The model: `logical_id`-alone, both tables

- Per table, the partial UNIQUE INDEX becomes
  `ON canonical_<table>(logical_id) WHERE superseded_at IS NULL` — one active row
  per non-NULL `logical_id`. NULL-safety is unchanged (SQLite treats each NULL as
  distinct → legacy NULL-`logical_id` rows still coexist active).
- Supersession (`commit_batch`) UPDATEs drop the `AND kind = ?` clause:
  `UPDATE … SET superseded_at = ?1 WHERE logical_id = ?2 AND superseded_at IS NULL`.
- The Slice 20/G8 in-batch supersession precompute re-keys from
  `HashMap<(&str, &str), usize>` (`(logical_id, kind)`) to `HashMap<&str, usize>`
  (`logical_id` alone): an edge at index `i` is in-batch-superseded iff
  `last_index[lid] > i`.

## 3. Mechanism: amend step-12 IN PLACE (no version bump)

**Decision (HITL): amend the existing step-12 DDL directly — NO `SCHEMA_VERSION`
bump (stays 12), NO new migration step 13.**

Consequence (recorded explicitly): an already-migrated local DB at
`user_version = 12` will **not** re-run the edited step-12 SQL, so it keeps the
old compound index until rebuilt. **HITL accepts that local v12 DBs are
disposable.** No production data exists; this is a pre-release substrate
correction. (A new step-13 drop/recreate would additionally FAIL the UNIQUE
index creation on any DB that already has identity-forked duplicate active rows —
explicitly out of scope per the HITL decision.)

## 4. Edit list

- **Schema** `fathomdb-schema/src/lib.rs` (~`:300-303`): drop `, kind` from both
  `CREATE UNIQUE INDEX` statements; update the index-scope comment (~`:285`).
  **No `SCHEMA_VERSION` bump.**
- **Engine** `fathomdb-engine/src/lib.rs`: remove `AND kind = ?3` from the node
  (~`:5998`) and edge (~`:6031`) supersession `UPDATE`s (re-number bound params
  `params![cursor, logical_id]`); re-key the G8 `last_index` precompute (~`:6111`)
  to `logical_id` alone; update the stale `(logical_id, kind)` comments.
- **Tests** (RED first): add `s31_node_kind_change_reingest_supersedes` +
  `s31_edge_kind_change_reingest_supersedes` to `pr_g0_identity.rs`; invert the
  `:177-182` tail of `s15_partial_unique_active_index_rejects_two_active_versions`
  (different kind + same active `logical_id` now REJECTED); update the comments
  that say "keyed by (logical_id, kind)"; tighten `migrations.rs` to assert the
  index SQL is `logical_id`-only (no `kind`). Add a `pr_g2` test that `read.get`
  after a kind-change re-ingest returns the single active record deterministically.
- **ADR** `ADR-0.8.0-canonical-identity-substrate.md`: add **Decision 5 —
  active-identity scope = `logical_id`** (HITL-SIGNED 2026-06-05); amend the
  authorized DDL block + authorized-elements to the `logical_id`-only form; note
  the in-place step-12 amendment + the local-v12-disposable consequence.
- **Propagated docs**: parent ADR Q2 + Q4 (Q4 wording corrected: edges DO
  supersede), roadmap, slice-15 design memo, `0.8.0-implementation.md` (Slice 15
  section), `architecture.md` identity model — all `(logical_id, kind)`
  active-identity language → `logical_id`. `dev/DOC-INDEX.md` refreshed.

## 5. Test plan (the load-bearing RED)

The RED that proves the fix is `s31_node_kind_change_reingest_supersedes` and
`s31_edge_kind_change_reingest_supersedes`: write `logical_id="L1", kind="fact"`,
then `logical_id="L1", kind="note"`; assert **exactly one active row** for `L1`
(the second supersedes the first) and the first is tombstoned (retained). Under
the *current* compound key these FAIL (two active rows) — proving both the fork
bug and the fix. The inverted `s15` case-c asserts the same-`logical_id`
+different-`kind` insert is now rejected by the unique index. `migrations.rs`
asserts the active-index SQL no longer contains `kind`.

Regression pins that MUST stay green under `logical_id`-alone: Slice 10 Search
byte-identity, Slice 20 G8 (`pr_g8_dangling_edges.rs` — distinct logical_ids keep
case (g) green; the EXPLAIN plan still names `canonical_nodes_logical_active_idx`,
whose leading column is still `logical_id`), Slice 30 `pr_g2`/`pr_g3`, and the
byte-frozen recovery suites.
