---
status: COMPLETE
---

# EARP Slice 10 — engine adoption: `unavailable` readiness + `projection_status` witness

Design of record for the post-Slice-19/21/22 adoption slice. Depends on S7
(witnesses), S9 (witness-source precedent), and main @ `e95afd29` merged at
`93722e22` with the worktree binding rebuilt.

## Requirements

R1. EARP's witness vocabulary admits the engine's third readiness state —
    without crashing, without mislabeling, and recorded under its own name.
R2. The resolver's vector-without-embedder refusal STAYS, and its message
    cites the engine's honest state. The pre-Slice-21 wording claimed the
    readiness witness "reads vacuously `ready`"; Slice 21 made that false.
R3. `read.projection_status` (Slice 22) is adopted as a **supplementary**
    fourth witness — it never replaces the three true sources (S7's
    discipline stands; the status verb is derived from the same internals).
R4. A binding-present drift alarm pins the engine's readiness vocabulary,
    so the NEXT vocabulary change announces itself the way this one did not
    (EARP's suite stayed green because nothing pinned the binding set).

## Pre-S10 facts (all verified by execution, 2026-08-08)

1. A no-embedder session with a `vector: true` declaration now reads
   `'unavailable'` from `read.projections` (probed live in the rebuilt
   binding). The binding contract admits three spellings.
2. `read.projection_status(engine)` returns `ProjectionRuntimeStatus`:
   `runtime_embedder_available: bool`, `runtime_unavailability_reason:
   none|no_runtime|vector_equivalence_disabled`, `projections:
   (ProjectionRuntimeStatusEntry(name, dense_readiness),…)` sorted by
   name, `vector_unsupported_kinds: tuple`. Pure read; observed
   `('no_runtime', 'unavailable')` on the probe store.
3. Before S10, EARP's `ProjectionWitnesses.readiness_state` method
   raised `AssertionError` for any vector-spec readiness outside
   `{ready, embedding}` — and the crash was **reachable**
   (review-executed): `classify_open` only *accumulates* a
   `DENSE_DISABLED` blocker; the run proceeds to configure → ingest →
   poll (the runner has no early return before that poll). On a degraded open
   (embedder present, equivalence-refused ⇒ Slice-21 `unavailable`) the
   run records DENSE_DISABLED, then **crashes to `verdict=FAILED` at the
   poll** with empty blockers and no readiness in the sidecar — a
   BLOCKED run mislabeled as FAILED. The resolver's
   vector-without-embedder refusal is the only real gate; it does not
   cover this state. S10 is therefore a defect fix, not only adoption.
   Post-fix outcome, pinned: degraded-open + vector declaration ⇒
   readiness `'unavailable'` recorded + verdict BLOCKED via
   DENSE_DISABLED.
4. The result schema's readiness map enum is
   `{ready, embedding, not_declared}` (`projection_witnesses` block);
   `witness.source` enum most recently gained `"answer_arm"` (S9) — the
   additive-value precedent this slice follows.
5. Before S10, the resolver message in `config.py` told the pre-Slice-21
   story — and **no test pinned the wording** (review-verified: the
   refusal test asserts only the blocker code). The stale prose also
   lives in a `config.py` comment and `test_projection_matrix.py` docstrings.
   S10 ADDS a
   message-content pin (new, not an update) and corrects every prose
   site.
6. The status object's nested readiness values are the engine's
   FOUR-value set `{not_declared, unavailable, embedding, ready}`
   (review-executed: a vector-less projection reads `not_declared` from
   the engine). That `not_declared` is ENGINE-produced — same spelling
   as the poll map's EARP-derived value, different producer; the schema
   description records the distinction so S7's "derived by EARP" doc
   stays true.

## Changes, by file

### `schema/models.py`

- `readiness_state`: `(vector=True, 'unavailable') → 'unavailable'`. The
  assertion narrows to its real contract — `None` or an unknown spelling
  with `vector=True` — and its message names the three binding values.
  Docstring updated: `not_declared` remains the derived value for
  `vector=False`; `unavailable` is the ENGINE's value, passed through,
  never derived by EARP.
- Frozen `ProjectionStatusWitness` value type mirroring fact 2's shape
  (name-keyed readiness mapping, reason, unsupported kinds) — with an
  explicit `as_value()` dict conversion (S7 precedent): the raw
  dataclass is not JSON-serializable (review-executed `TypeError`), and
  `Witness.value` flows raw into `json.dumps`.
- `WitnessSource` gains the `PROJECTION_STATUS` member (the JSON-schema
  enum edit alone is unimplementable; no models↔schema parity guard
  exists to catch the omission).

### `schema/earp.result.v1.schema.json` — additive only

- `projection_witnesses` readiness enum gains `"unavailable"`.
- `witness.source` enum gains `"projection_status"`.
- Optional `projection_status` object alongside the existing witness
  blocks in the scenario `projection_witnesses` group (review-verified:
  the readiness `additionalProperties` map is confined to its own
  sibling property, so a new named property collides with nothing):
  `additionalProperties: false`, `required` on all four fields —
  `{ runtime_embedder_available, runtime_unavailability_reason
  (enum none|no_runtime|vector_equivalence_disabled — the string
  "none", a Literal spelling, never null), readiness (name→state map,
  enum {not_declared, unavailable, embedding, ready} — engine-produced,
  see fact 6), vector_unsupported_kinds }`. Absent for pre-S10 sidecars
  and projection-less runs (S7 absence semantics).

### `runner.py`

- **`'unavailable'` is a settled poll state.** Before S10, it exited the poll
  loop only by accident of the `== "embedding"` comparison. The now-stated
  and tested behavior exits immediately, never spins to timeout, and
  never emits `DENSE_READINESS_TIMEOUT` for it. Blocked-verdict coverage
  for the degraded-open case comes from `DENSE_DISABLED` (fact 3's
  pinned outcome).
- After the readiness poll settles (ready, `unavailable`, or timeout
  blocker recorded), capture `read.projection_status(engine)` once —
  **inside the `try`, after the poll block and before the query call**,
  so a query-time FAILED run still carries the status witness and a
  poll-raise skips capture. Record it as (a) the `projection_status`
  object in the sidecar and (b) a witness with source
  `projection_status`. Capture happens on the blocked path too (S7
  precedent: the delta witness persists on DENSE_READINESS_TIMEOUT
  blocked runs).
- Projection-less runs: not captured (S7's rule — absent ≠ empty; the
  always-captured open-report witness already carries runtime
  availability). The three existing witnesses are untouched; on any
  disagreement between the status object and a true-source witness,
  both are recorded as-is — capture is not atomic with the poll, so
  transient disagreement is legitimate; EARP records, it does not
  reconcile.

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
- **Drift alarm (R4, rewritten per review):** the raw-engine no-runtime
  probe would duplicate the landed upstream tests
  (the upstream dense-readiness and projection-status tests) and would stay green
  through the next vocabulary change. The alarm instead pins EARP's
  HANDLING at the binding seam: (a)
  `set(typing.get_args(fathomdb.DenseReadiness)) ==
  {"unavailable", "embedding", "ready"}` — fires on any Literal change;
  (b) `readiness_state(vector=True, v)` accepts **every member of
  `get_args(DenseReadiness)` by iteration**, not literals — so a fourth
  value breaks (a) and, until handled, (b). A slim live probe may stay
  as a smoke check; it is not the alarm.
- Poll semantics: `poll_override` test — first poll returns
  `'unavailable'` → loop exits immediately, no timeout blocker, witness
  records `unavailable`.
- Degraded-open outcome (poll_override synthetic, per S7's own test
  pattern): DENSE_DISABLED + `unavailable` readiness ⇒ verdict BLOCKED,
  readiness present in the sidecar — the fact-3 mislabeling fixed and
  pinned.
- NEW message pin for the resolver refusal (no pin existed before S10);
  stale docstrings/comment sites corrected in the same commit.
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

## Acceptance-criteria amendments (review round)

AC-3 is restated to match the rewritten R4: the alarm asserts the
binding's `DenseReadiness` Literal set equality AND `readiness_state`
acceptance over `get_args(...)` by iteration. New AC-7: a degraded-open
scenario (synthetic via `poll_override`) yields verdict BLOCKED with
`unavailable` recorded — never FAILED with a bare AssertionError.

## Review

Independent code-grounded executing review, 2026-08-08. Verdict:
**PROCEED WITH REVISIONS** — mechanics all executed and confirmed
(schema edits exactly three-plus-enum, no map collision, blocked-path
capture precedent); two facts corrected, four contracts pinned:

| # | Sev | Finding | Resolution |
| ---: | --- | --- | --- |
| 1 | MAJOR | **Before S10,** "Degraded open blocks before the poll" was false — blockers accumulated, the run proceeded, and crashed to FAILED at the poll | Fact 3 rewritten; S10 reframed as defect fix; BLOCKED outcome pinned (AC-7) |
| 2 | MAJOR | `'unavailable'` exits the poll only by accident of `== "embedding"` | Settled-state semantics stated + tested |
| 3 | MAJOR | Before S10, no test pinned the resolver message (fact 5 false) | Fact corrected; NEW pin added; stale prose sites enumerated |
| 4 | MAJOR | R4 raw-engine probe duplicates upstream tests and cannot satisfy AC-3 | Alarm rewritten to binding-seam Literal-set + iteration acceptance |
| 5 | MOD | `WitnessSource.PROJECTION_STATUS` member missing from change list | Added |
| 6 | MOD | Nested status readiness map is the engine's 4-value set incl. engine-produced `not_declared` | Fact 6 added; schema enum + producer distinction specified |
| 7 | MOD | Raw dataclass not JSON-serializable through the writer | `as_value()` conversion required |
| 8-10 | MIN/INFO | Schema collision checked clean; capture placement pinned; record-don't-reconcile rationale recorded | Folded in |

Unverified upstream (inferred from Rust tests, marked): the
`vector_equivalence_disabled` reason and `'none'` spelling at the Python
layer require the Rust-only divergent-embedder seam to construct.
