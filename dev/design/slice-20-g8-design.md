---
title: slice-20-g8-design
date: 2026-06-03
target_release: 0.8.0
desc: Design memo for Slice 20 (G8) — additive, default-non-rejecting dangling-edge flag-and-count at write time, surfaced as WriteReceipt.dangling_edge_endpoints (Py+TS parity). Settles the post-row-insert-in-open-tx placement (NOT validate_write), the logical_id-alone EXISTS probe against the step-12 partial index canonical_nodes_logical_active_idx, the intended legacy-NULL-logical_id endpoint consequence, and the flag-and-count default with strict-mode deferred to reserved-gap band 22.
blast_radius: src/rust/crates/fathomdb-engine/src/lib.rs (WriteReceipt, write_inner, commit_batch); src/rust/crates/fathomdb-py/src/lib.rs (PyWriteReceipt); src/rust/crates/fathomdb-napi/src/lib.rs (WriteReceipt); src/python/fathomdb/types.py + engine.py + admin.py; src/ts/src/index.ts; docs/reference/python-api.md + typescript-api.md
status: implementation
depends_on: dev/adr/ADR-0.8.0-canonical-identity-substrate.md (SIGNED 2026-06-03); Slice 15 (G0 keystone, CLOSED on main)
---

# Slice 20 — G8 Dangling-Edge Flag-and-Count (design memo)

## Mandate

Add an **additive, default-non-rejecting** referential check at write time so callers
learn how many edge endpoints point at non-existent **or superseded** canonical nodes,
surfaced as a new `WriteReceipt.dangling_edge_endpoints: u64` with **Python + TypeScript
parity**. This is F10/G8 for graph consumers. It hard-depends on G0 (Slice 15): edge
endpoints are node `logical_id`s, and "exists" means an **active** node
(`superseded_at IS NULL`) carries that `logical_id`.

It changes no existing behavior — the count is **informational** (flag-and-count default,
non-rejecting) and **no current consumer sets `logical_id`**, so it only adds a signal.

## Why a post-row-insert pass inside the open tx — NOT `validate_write`

The check is **cross-row**: bulk loaders legitimately insert an edge *before* its target
node **in the same batch** (e.g. `[edge(N1→N2), node(N1), node(N2)]`). A single-row,
pre-insert hook (`validate_write`) sees only one `PreparedWrite` at a time and cannot see
same-batch siblings, so it would wrongly flag `N1`/`N2` as missing.

Therefore the check lives as a **post-row-insert pass inside `commit_batch`'s open
transaction** — after the batch loop has inserted *every* row (so every same-batch node is
already on disk in `tx`), and **before** `enforce_provenance_retention` /
`advance_projection_cursor` / `tx.commit()`. At that point each edge's endpoints are probed
against the now-fully-populated `canonical_nodes`, so same-batch insertion **order is
irrelevant**: a same-batch later-inserted node is **NOT** flagged.

Placement (current `lib.rs`): the batch loop ends at `:5839`; the pass goes between `:5839`
and `enforce_provenance_retention` at `:5841`, in the same `tx`. `commit_batch` changes
its return type from `rusqlite::Result<()>` to `rusqlite::Result<u64>` (the dangling count),
captured by `write_inner` at the call site (`:1948`) and threaded into the receipt (`:1968`).

We iterate the **in-memory `batch`** for the edges (rather than re-reading
`canonical_edges`) and probe each edge's `from`/`to` against the fully-populated node rows —
this keeps same-batch ordering irrelevant by construction. An edge is counted only when it
is **active**; an edge inserted in this batch is active unless a **later** same-batch edge
with the same `(Some(logical_id), kind)` tombstoned it (the loop's supersession `UPDATE`), so
that single in-batch-supersession case is skipped to honor `edge.superseded_at IS NULL`.

## Settled probe predicate (do not re-litigate — settled by construction)

Count an edge endpoint as dangling when the edge itself is active **AND** no active node
carries that `logical_id`:

```sql
-- per endpoint (from_id and to_id probed INDEPENDENTLY — an edge contributes 0, 1, or 2):
edge.superseded_at IS NULL
AND NOT EXISTS (
  SELECT 1 FROM canonical_nodes
   WHERE logical_id = <endpoint> AND superseded_at IS NULL
)
```

**The probe is `logical_id`-alone (NOT `(logical_id, kind)`).** `canonical_edges` stores only
the *edge's own* `kind`, **not the endpoint node's kind** (see the schema:
`canonical_edges(write_cursor, kind, from_id, to_id, source_id, logical_id, superseded_at)`).
So the "edges-carry-the-referenced-node-kind" variant the parent contract floated is **ruled
out by the landed schema** — there is no node-kind to match on. We probe `logical_id` alone.

**Both endpoints are probed independently:** an edge with both endpoints missing contributes
**2** to the count; one missing contributes 1; none missing contributes 0.

**Missing OR superseded both count.** A superseded node has its active version tombstoned
(`superseded_at` set), so `NOT EXISTS (... superseded_at IS NULL)` is true for it — it counts
as dangling exactly like a never-written endpoint. This is intended: the graph integrity
signal is about whether a *live* node backs the endpoint.

## Index-hit argument (EXPLAIN QUERY PLAN — no SCAN)

Step 12 (cited: `ADR-0.8.0-canonical-identity-substrate.md` AUTHORIZED delta; landed at
`fathomdb-schema/src/lib.rs:294-309`) created:

```sql
CREATE UNIQUE INDEX canonical_nodes_logical_active_idx
    ON canonical_nodes(logical_id, kind) WHERE superseded_at IS NULL;
```

The per-endpoint probe `WHERE logical_id = ?1 AND superseded_at IS NULL` matches this
**partial** index two ways: its partial predicate (`WHERE superseded_at IS NULL`) is implied
by the query, and `logical_id` is the index's **leading column**. SQLite therefore resolves
the probe with `SEARCH canonical_nodes USING INDEX canonical_nodes_logical_active_idx` — **no
`SCAN canonical_nodes`**. Test (f) asserts this via `EXPLAIN QUERY PLAN`. The prepared
statement is **hoisted once** outside the per-edge loop so every endpoint reuses the same
compiled plan / index.

## Legacy NULL-`logical_id` endpoint consequence (intended, informational)

Only nodes written with a **non-NULL `logical_id`** are valid (non-dangling) endpoints. A
legacy / own-identity node (`logical_id = NULL`, the byte-identical 0.7.x path) is **not
matchable by `logical_id`** (SQLite treats each NULL as distinct and the probe binds a
concrete value), so an edge pointing at one counts as **dangling**.

This is **correct and intended** ("G8 rides on G0"): G8 validates the **`logical_id`-keyed
graph**. Because the count is informational (flag-and-count default, non-rejecting) and **no
current consumer sets `logical_id`**, this changes **no existing behavior** — it only adds a
signal for consumers who opt into the logical_id-keyed graph. Documented here per the
contract.

## Default flag-and-count vs optional strict-mode rollback

- **Default = FLAG-AND-COUNT (commit anyway).** The pass only *counts*; the batch commits
  regardless, and the count rides out on `WriteReceipt.dangling_edge_endpoints`. No hard
  reject by default; `from_id`/`to_id` meaning for existing rows is unchanged.
- **Strict mode (rollback-before-commit) is DEFERRED.** There is no existing write-options /
  flag path on `write_inner(&self, batch: &[PreparedWrite])` to thread, and inventing a new
  public write-options surface is **out of scope** for this slice (scope discipline §6 /
  AC-057a-clean). Strict-mode rollback is therefore recorded as a **flagged reserved-gap
  (band 22)**, not built here. No test (g).

## Test plan (`tests/pr_g8_dangling_edges.rs`)

- **(a)** edge → one missing `logical_id` (other endpoint valid) increments the count by 1.
- **(b)** edge → a node inserted **later in the same batch** is **NOT** flagged (cross-row;
  the case `validate_write` would get wrong) → count 0.
- **(c)** edge → a **superseded** node (active version raw-tombstoned, no active version)
  **counts** as dangling.
- **(d)** **default** flag-and-count **commits** the batch (edge row present after write;
  receipt carries the count).
- **(e)** the count is the **sum over both endpoints** — an edge with both endpoints missing
  contributes 2.
- **(f) [latency / plan gate]** `EXPLAIN QUERY PLAN` for the per-endpoint probe shows
  `USING INDEX canonical_nodes_logical_active_idx` and **no `SCAN canonical_nodes`**.
- legacy baseline: a `logical_id = None` batch of nodes+edges writes unchanged (no panic;
  count well-defined — NULL endpoints simply count as dangling per the intended consequence).

No test (g): strict mode deferred (band 22).

## Acceptance ids

`dev/acceptance.md` is the **locked 0.6.0** criteria file (max `AC-073`); it has **no G8 /
dangling / F10 entries**. The tests therefore bind to the **F10/G8 capability label** from
`dev/design/0.8.0-agent-memory-fit.md` §4 (row G8) / §7. Recorded in `output.json`. Adding
0.8.0 AC entries is out of scope for this implementation slice.

## Out of scope (flag, do not build)

- Shadow vec0/FTS5 reconciliation (reserved Slice 16) — if the probe surfaces a superseded
  node with live projection shadows, flag it; do not fix.
- Read verbs / `read.*` surface (Slice 30, gated by Slice 25); a filter DSL (Slice 35).
- `restore_validated_edges` / any recovery verb on the SDK — stays CLI-only; recovery
  unreachability suites stay byte-unchanged and green.
- Strict-mode options surface (reserved-gap band 22).
