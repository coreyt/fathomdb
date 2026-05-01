# Design: Restore Re-Establishes Retired Projection State

## Purpose

Define the work required to satisfy the updated lifecycle requirement that
restore must re-establish full pre-retire content by preserving/restoring the
retired content and projection state needed to bring a logical object fully
back.

This is a requirements-level design note, not an implementation patch plan.

## Requirement

Restore is not allowed to mean only:

- set the logical row active again

Restore must instead mean:

- re-establish the last pre-retire active content state for the logical object,
  including the object row, directly related retired edges, chunks, and the
  projection state needed for search/vector behavior to match the pre-retire
  object again

That implies the current destructive retire behavior is insufficient as the
long-term lifecycle contract.

## 1. Reversible Retire/Restore Lifecycle Contract

### Feature

Retire becomes a reversible lifecycle state, not a destructive cleanup step
that destroys restoration prerequisites.

### Remaining Items For Acceptance

- define what counts as the restorable unit of state for one retired logical
  object
- define which directly related edges are restored with the object
- define whether restore targets only the last retire event or an explicit
  retire scope/snapshot

Acceptance criteria:

- restore returns the logical object to its last pre-retire active state
- restore does not leave the restored object orphaned from directly related
  edges that were retired as part of the same retire scope
- restore remains explicit and auditable

### Design Outline

- The lifecycle contract must distinguish reversible retire from irreversible
  purge.
- Retire should preserve the state needed for later full-fidelity restore.
- Restore should operate against a well-defined retire scope, not a vague
  “make active again” rule.
- The restore unit should include the logical object and the directly attached
  edge/content state that was retired with it or because of it.

## 2. Preserved Restorable State Model

### Feature

The system must retain enough retired state to make restore full-fidelity.

### Remaining Items For Acceptance

- classify which retired state is canonical content versus rebuildable
  projection state
- define which state must be preserved directly at retire time
- define which state may be restored by deterministic rebuild from preserved
  retired state

Acceptance criteria:

- restore never depends on external re-ingest or application resubmission
- restore does not require best-effort operator repair steps
- the restored object has the same user-visible content/search/vector behavior
  expected from its pre-retire state

### Design Outline

- Treat the pre-retire logical row, its chunks, and directly related retired
  edges as restorable state that must survive retire.
- Treat FTS and vec behavior as part of restore completeness, not optional
  post-restore cleanup.
- The design may either:
  - preserve retired projection rows directly, or
  - preserve enough retired canonical state that projections can be rebuilt
    deterministically as part of restore
- The critical requirement is not which mechanism is used, but that restore
  re-establishes full pre-retire content without outside help.

## 3. Purge Finality Under A Reversible Retire Model

### Feature

Purge becomes the operation that irreversibly destroys the state that retire
must preserve.

### Remaining Items For Acceptance

- define exactly what purge removes once retire is no longer destructive
- define audit/tombstone behavior after irreversible deletion
- define how purge behaves when a previously retired object has been restored

Acceptance criteria:

- purge is the only operation that permanently removes restorable retired state
- purge leaves no orphaned edges, chunks, FTS rows, vec rows, or retained
  restore snapshots
- a restored object is not later purged by a stale scheduled purge action

### Design Outline

- Retire preserves reversibility; purge provides irreversibility.
- Purge must explicitly remove:
  - active/superseded canonical rows within scope
  - preserved retired content/projection state
  - directly attached edges in purge scope
  - derived rows and restore-only retained state
- Purge should leave bounded provenance/tombstone proof that the destructive
  action occurred.

## 4. Restore Scope, Reporting, And Audit Contract

### Feature

Restore must expose enough scope/reporting information to be operationally
trustworthy.

### Remaining Items For Acceptance

- define whether restore returns a scope report of what was restored
- define how partial or impossible restores are surfaced
- define provenance metadata for retire, restore, and purge linkage

Acceptance criteria:

- operators can tell which rows/state were restored and from which retire scope
- failed restore is diagnosable without inspecting raw tables manually
- lifecycle provenance lets repair/recovery tools understand the chain:
  retire -> restore or retire -> purge

### Design Outline

- Restore should return a report that describes the restored scope at least at
  the level of object rows, edges, chunks, and projection state.
- Provenance should tie restore back to the retire scope it reverses.
- If some retired state is no longer restorable because purge already ran,
  restore must fail clearly rather than silently restoring a degraded object.

## 5. Integrity, Recovery, And Proof Obligations

### Feature

The revised lifecycle model must be reflected in integrity semantics and
recovery expectations.

### Remaining Items For Acceptance

- define lifecycle invariants for preserved retired state
- define semantic/integrity checks for broken restore state
- define required proof scenarios for vector and excision interactions

Acceptance criteria:

- integrity/semantic checks can detect broken preserved-retire state
- restore/purge/excise interactions are deterministic and testable
- vector behavior is proven for:
  - purge plus vec cleanup
  - excision plus vec cleanup
  - restore/purge interaction with regenerated vectors

### Design Outline

- Recovery tooling must treat preserved retired state as intentional lifecycle
  material, not corruption.
- Semantic checks should distinguish:
  - valid retained retire state
  - missing restorable content for a retired object
  - broken projection-restoration prerequisites
- The verification bar must prove that restore does not reintroduce drift or
  leave search/vector state inconsistent with the restored content.

## Bottom Line

Meeting the new requirement means changing the lifecycle model, not merely
adding a restore admin command.

The design work required is:

- redefine retire as reversible
- preserve enough retired state for full-fidelity restore
- move irreversible destruction responsibility to purge
- make restore auditable and scope-aware
- extend integrity/recovery semantics to understand preserved retired state
