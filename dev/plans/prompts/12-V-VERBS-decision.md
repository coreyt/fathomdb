# Phase 12-V-VERBS — HITL Decision Package: Deferred Verbs Review

**Type:** HITL-decision slice (not implementer).

**Scope:** Confirm (or un-defer) the deferred logical-id verbs:
`purge_logical_id` + `restore_logical_id`. Surface any other
design-only verbs.

**Owner:** user signoff.
**Exit criterion:** per-verb written decision in
`dev/progress/0.6.0.md`; `dev/design/recovery.md` § Logical-id
purge and restore section refreshed if any verb decision changes.

## Context

Two verbs surfaced as design-only in 0.6.0 per Phase 10b-B blocker
(2026-05-16):

| Verb                 | Spec source                                             | Substrate gap                                                                                                                                                                                                                                                                                                                                                                |
| -------------------- | ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `purge_logical_id`   | `dev/design/recovery.md` § Logical-id purge and restore | `canonical_nodes` + `canonical_edges` lack `logical_id` / `row_id` / `superseded_at` / `restore_provenance` columns; `PreparedWrite::{Node,Edge}` carry no `logical_id` field; no partial unique index on `(logical_id, kind)`; no supersession writer step; `latest_state` op-store has no `record_key == logical_id` contract; `append_only_log` has no `logical_id` index |
| `restore_logical_id` | Same                                                    | Same + no `restore_provenance` writer path                                                                                                                                                                                                                                                                                                                                   |

Both verbs paired (closing one without the other is incoherent).
Both blocked on the **same** precursor: a canonical-identity
substrate slice (call it `10b-B-pre`) that lands:

1. Schema migration: add `row_id` / `logical_id` / `superseded_at` /
   `restore_provenance` columns to `canonical_nodes` +
   `canonical_edges` + partial unique index on
   `(logical_id, kind) WHERE superseded_at IS NULL`.
2. `PreparedWrite::Node` + `PreparedWrite::Edge` field additions
   (`logical_id: String`).
3. Writer supersession step per `dev/design/engine.md:154-169`.
4. `latest_state` op-store `record_key == logical_id` contract OR
   spec amendment redefining cascade target.
5. `append_only_log` `logical_id` index.

10b-B implementer surfaced the gap on 2026-05-16 and stopped per
prompt anti-invention rules. Per memory
`project_logical_id_deferred_0_7_x`, deferred to 0.7.x.

## Decision per verb pair

For the paired `(purge_logical_id, restore_logical_id)` set, choose
**(A) confirm deferred to 0.7.x** OR **(B) authorize precursor
slice 10b-B-pre for 0.6.0**.

### Option (A) confirm 0.7.x deferral

- 0.6.0 GA ships without bulk-delete + restore-from-log workflows.
- Release notes call out: "logical-id-scoped purge and restore
  require 0.7.x; clients needing bulk source-id excision before
  then use `recover --excise-source <id>` (landed in Phase 10a)."
- Pack-Phase-10b-B-pre + 10b-B-rerun queued for early 0.7.x.
- 12-V-VERBS closes immediately on signoff; no code work.
- ETA: zero days for 0.6.0 (just decision-recording).

[ ] (A) confirm 0.7.x deferral

### Option (B) authorize 10b-B-pre precursor for 0.6.0

- Schedule precursor slice (`10b-B-pre`) to land canonical-identity
  substrate: schema migration + writer supersession +
  PreparedWrite field additions + index additions + (HITL) latest_state
  contract or spec amendment.
- After 10b-B-pre lands, re-spawn 10b-B to build the actual verbs
  per `dev/design/recovery.md` § Logical-id purge and restore.
- Estimated work: > 1 week implementer time + HITL signoff on the
  latest_state contract decision; possibly weeks if the
  spec-amendment path is chosen.
- Risk: schema migration in 0.6.0 GA is a data-layer change
  affecting every existing fixture and existing 0.6.0-rewrite test;
  failure-recovery semantics ripple to recover CLI surface.
- Pushes GA timeline by 2-4 weeks minimum.
- Per memory `project_logical_id_deferred_0_7_x`: documented
  decision is 0.7.x — overriding requires HITL re-decision.

[ ] (B) authorize 10b-B-pre + 10b-B-rerun for 0.6.0 (GA timeline pushes 2-4 weeks)

### Option (C) amend spec instead

Per 10b-B blocker output JSON `minimum_unblock` § alternative: amend
`dev/design/engine.md` § Canonical identity and supersession +
`dev/design/recovery.md` § Logical-id purge and restore to redefine
"logical_id" against existing schema (e.g. as the existing
`(kind, write_cursor)` canonical row identity, or a normalised
source_id-scoped key).

- This would invalidate engine.md § Canonical identity and
  supersession + recovery.md § Logical-id purge and restore
  wholesale — design-corpus change with broader ripple.
- 10b-B implementer noted this as a path but explicitly says it
  "would invalidate" the design corpus → not a cheap edit.
- Not recommended unless HITL has strong reason to ship some
  flavor of these verbs in 0.6.0.

[ ] (C) amend spec to redefine logical_id against existing schema (broad design-corpus change)

## Surface check: any other design-only verbs?

Surveyed `dev/interfaces/*.md` + `dev/design/*.md` for verbs that
appear in spec but not in `src/`. No other gaps found as of
2026-05-17. The five-verb runtime SDK (`Engine.open`, `write`,
`search`, `close`, `admin.configure`) plus engine-attached
instrumentation (`drain`, `counters`, `set_profiling`,
`set_slow_threshold_ms`, subscriber-attach) plus CLI verbs
(`doctor` six sub-verbs, `recover` four sub-flags) are all
implemented per their locked interfaces.

If user signoff surfaces additional design-only verbs not in the
above survey, add them to this decision package as new sub-sections
before signing.

## Orchestrator recommendation

**Option (A) confirm 0.7.x deferral.** Rationale:

- Memory `project_logical_id_deferred_0_7_x` already records the
  decision (HITL 2026-05-16).
- Pushing 10b-B-pre into 0.6.0 expands GA scope by 2-4 weeks for
  one paired-verb set. Per `feedback_reliability_principles`
  net-negative-LoC, this is a heavy addition for marginal client
  benefit (clients can use `recover --excise-source` for the
  bulk-delete use case today).
- 0.7.x is the right vehicle for the canonical-identity substrate
  - the verbs together.

## Outputs after signoff

1. Append HITL decision row to `dev/progress/0.6.0.md`:

   ```text
   ## 2026-MM-DD — Phase 12-V-VERBS HITL decision
   - purge_logical_id + restore_logical_id (paired): (A) confirmed
     deferred to 0.7.x per project_logical_id_deferred_0_7_x memory
     (HITL signoff 2026-05-16 re-confirmed YYYY-MM-DD).
   - Release notes language: bulk-delete + restore-from-log
     workflows require 0.7.x; 0.6.0 clients use
     `recover --excise-source` for the bulk-delete case.
   - No other design-only verbs surfaced in 2026-05-17 survey.
   ```

2. Update `docs/release-notes/0.6.0.md` § "Logical-id verbs" to
   reflect HITL-confirmed status.
3. Mark 12-V-VERBS CLOSED in `dev/plans/runs/STATUS-phase12.md` +
   `dev/plans/0.6.0-implementation.md`.

No implementer spawn. No reviewer. HITL signature in
`dev/progress/0.6.0.md` is the closure artifact.
