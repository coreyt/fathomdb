---
status: PROPOSED
---

# EARP Slice 10 — engine adoption: `unavailable` readiness + `projection_status` witness

Design of record for the post-Slice-19/21/22 adoption slice. Depends on S7
(witnesses), S9 (witness-source precedent), and main @ `e95afd29` merged at
`93722e22` with the worktree binding rebuilt.

## Requirements

R1. EARP's witness vocabulary admits the engine's third readiness state —
    without crashing, without mislabeling, and recorded under its own name.
R2. The resolver's vector-without-embedder refusal STAYS, but its message
    must cite the engine's honest state; the current text claims the
    readiness witness "reads vacuously `ready`", which Slice 21 made false.
R3. `read.projection_status` (Slice 22) is adopted as a **supplementary**
    fourth witness — it never replaces the three true sources (S7's
    discipline stands; the status verb is derived from the same internals).
R4. A binding-present drift alarm pins the engine's readiness vocabulary,
    so the NEXT vocabulary change announces itself the way this one did not
    (EARP's suite stayed green because nothing pinned the binding set).

## Facts this design stands on (all verified by execution, 2026-08-08)

1. A no-embedder session with a `vector: true` declaration now reads
   `'unavailable'` from `read.projections` (probed live in the rebuilt
   binding). The binding contract admits three spellings.
2. `read.projection_status(engine)` returns `ProjectionRuntimeStatus`:
   `runtime_embedder_available: bool`, `runtime_unavailability_reason:
   none|no_runtime|vector_equivalence_disabled`, `projections:
   (ProjectionRuntimeStatusEntry(name, dense_readiness),…)` sorted by
   name, `vector_unsupported_kinds: tuple`. Pure read; observed
   `('no_runtime', 'unavailable')` on the probe store.
3. EARP's `ProjectionWitnesses.readiness_state` (`models.py:249-263`)
   raises `AssertionError` for any vector-spec readiness outside
   `{ready, embedding}` — under the new engine this is a latent crash,
   not a safety net. Today it is unreachable through EARP configs (the
   resolver refuses vector-without-embedder, and a degraded open blocks
   at the open-report witness before the poll) — but Slice 21 broadened
   `unavailable` to "embedder present but not safety-approved", so the
   assertion is one engine-behavior edge from firing mid-run.
4. The result schema's readiness map enum is
   `{ready, embedding, not_declared}` (`projection_witnesses` block);
   `witness.source` enum most recently gained `"answer_arm"` (S9) — the
   additive-value precedent this slice follows.
5. The resolver message at `config.py:583-586` pins the pre-Slice-21
   story; at least one test pins that wording.

## Changes, by file

### `schema/models.py`

- `readiness_state`: `(vector=True, 'unavailable') → 'unavailable'`. The
  assertion narrows to its real contract — `None` or an unknown spelling
  with `vector=True` — and its message names the three binding values.
  Docstring updated: `not_declared` remains the derived value for
  `vector=False`; `unavailable` is the ENGINE's value, passed through,
  never derived by EARP.
- Frozen `ProjectionStatusWitness` value type mirroring fact 2's shape
  (name-keyed readiness mapping, reason, unsupported kinds) — EARP's
  sidecar records the status object's content, not a repr.

### `schema/earp.result.v1.schema.json` — additive only

- `projection_witnesses` readiness enum gains `"unavailable"`.
- `witness.source` enum gains `"projection_status"`.
- Optional `projection_status` object alongside the existing witness
  blocks in the scenario `projection_witnesses` group: `{
  runtime_embedder_available, runtime_unavailability_reason
  (enum none|no_runtime|vector_equivalence_disabled),
  readiness (name→state map), vector_unsupported_kinds }`. Absent for
  pre-S10 sidecars and projection-less runs (S7 absence semantics).

### `runner.py`

After the readiness poll settles (ready, or timeout blocker recorded),
capture `read.projection_status(engine)` once and record it as (a) the
`projection_status` object in the sidecar and (b) a witness with source
`projection_status`. Projection-less runs: not captured (S7's rule —
absent ≠ empty). The three existing witnesses are untouched; on any
disagreement between the status object and a true-source witness, both
are recorded as-is — EARP records, it does not reconcile.

### `config.py`

Blocker message for `vector: true` + `use_default_embedder: false`
rewritten: the engine now reports `unavailable` for that state; the
config is refused because it declares a dense arm the run cannot use —
measuring dense retrieval requires an embedder, and an honest config
declares only what it exercises. (Refusal semantics unchanged; message
only.)

### Tests

- `readiness_state` truth table gains the `unavailable` row; the
  assertion test narrows to `None`/unknown.
- Binding-present drift alarm (R4): raw-engine test (no resolver) —
  fresh tmp store, vector declaration, no embedder → `read.projections`
  readiness == `'unavailable'` AND `projection_status` returns reason
  `no_runtime` with matching per-projection state. Pins the vocabulary
  from the consumer side.
- Runner end-to-end: a projection-declaring diagnostic run's sidecar
  carries the `projection_status` witness + object and validates; a
  projection-less run's sidecar has neither.
- The config-message pin updated to the new wording (named, deliberate).

## Acceptance criteria

1. `readiness_state(vector=True, vector_dense_readiness='unavailable')`
   returns `'unavailable'`; `None`/unknown still raise with the
   three-value message.
2. A sidecar carrying `unavailable` in the readiness map and a
   `projection_status` witness/object validates against the result
   schema; pre-S10 sidecars remain valid (additive-only edits).
3. The R4 drift alarm passes against the rebuilt binding and fails
   against any future readiness-vocabulary change (asserts the exact
   spelling set EARP handles).
4. The resolver still refuses vector-without-embedder; its message names
   `unavailable` and no longer claims a vacuous `ready`.
5. Projection-declaring runs record the fourth witness; projection-less
   runs record neither the witness nor the object.
6. Full suite green; ruff clean; pyright 0 new / 0 in touched files.

## Out of scope

- Any use of `projection_status` for control flow (blockers still key on
  the three true sources; the status is evidence, not a gate).
- Comparison/sweep integration beyond what arms inherit from the runner
  path (arms campaigns run through `execute_arm`, which configures
  projections but does not poll — S8 deviation 3 stands; extending the
  status witness to arms is future work if an arms campaign ever
  declares projections).
- Vocabulary changes to `built`/`deferred` (OPP-12 ≥0.9.x, per Slice 21's
  own docs scope).

## Review

Pending independent code-grounded review.
