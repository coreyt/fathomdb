---
status: COMPLETE
---

# EARP Slice 7 — store/projection/query matrix + readiness witnesses

Design of record for S7 of `dev/plans/earp-foundation.md` ("Projection state
is read from its true source"). Depends on S5 (diagnostic runner), S6a
(public limit; `query_params` injection). Plan requirement row:
`vector_dense_readiness` polled, `vector_unsupported_kinds` from the delta,
`dense_disabled` from `open_report`; `None` readiness disambiguated from
"not declared".

"Split by real source" means the **signal source**, not the corpus source:
the three projection-state signals come from three different APIs, are not
interchangeable, and only the first can be polled. S7's whole job is to keep
them separate and typed.

## Engine facts this design stands on (all verified by execution, 2026-08-07)

1. `Engine.configure_projections(specs, drop)` returns a `ProjectionDelta`
   with `built / dropped / deferred / unchanged` and
   `vector_unsupported_kinds`. FTS sub-targets build same-transaction;
   vector sub-targets defer — and the two lists are **not disjoint**: a
   spec with both sub-targets appears in both (`built=['title','body']`,
   `deferred=['body']` observed for a title-fts + body-fts+vector pair).
   "In `built`" must never be read as "fully built"; the witness
   interpretation keys on `deferred` for the dense portion.
   `vector_unsupported_kinds` exists **only** on this delta —
   `read.projections()` never carries it.
2. `read.projections(engine)` returns `ProjectionSpec`s whose
   `vector_dense_readiness` is binding-enforced to `"ready"`, `"embedding"`,
   or `None` (`read.py:227`; observed `'ready'` after deferred build, `None`
   for an FTS-only spec).
3. `None` readiness is ambiguous by itself: it means both "no vector
   sub-target declared on this spec" and "caller-authored spec". The
   disambiguator is the spec's own `vector` flag, which the probe confirmed
   round-trips (`(name='title', vector=False, readiness=None)` vs
   `(name='body', vector=True, readiness='ready')`).
4. `open_report()` carries `dense_disabled`, `dense_disabled_reason`,
   `query_backend`, and the embedder fields;
   `engine.vector_equivalence_refusal_count()` exists and reads `0` on a
   healthy open.
5. Querying an undeclared projection is a **typed engine error**, not an
   empty result: `search_projected_text('x', 'title')` on a bare DB raises
   `InvalidFilterError: projected text field "title" is not declared`.
6. Custom embedder implementations are unsupported by the Python SDK
   (`earp.md:182-185`); the only embedder lever remains
   `scenario.engine.use_default_embedder`.
7. **Vacuous readiness (review-corrected):** with NO embedder configured, a
   `vector: true` spec reads `'ready'` on the very first poll — readiness
   derives from outstanding embed work, and with no embedder no work is
   ever enqueued, so it is vacuously ready with zero dense vectors behind
   it. (Control: embedder on → `embedding` → `ready`.) The poll witness can
   therefore **never detect** a dense declaration without an embedder;
   config-time refusal is the only honest gate.

## Contract

`earp.v1` learns to declare projections, and the runner learns to witness
projection state from its true sources. Three invariants:

- **Each signal from its own API, recorded under its own name.** A poll
  result never stands in for the delta; the delta never stands in for the
  open report. None of the three is ever converted into an empty retrieval
  result (the engine already refuses typed — fact 5).
- **The matrix is validated before it is run.** A config whose query calls
  `Engine.search_projected_text` naming a projection the scenario does not
  declare is a collected config error at resolution — not an
  `InvalidFilterError` twenty minutes into a run.
- **`None` is never reported bare.** The sidecar's readiness field for each
  declared projection is one of `ready | embedding | not_declared`, derived
  from `(spec.vector, vector_dense_readiness)`; `not_declared` is only
  possible for specs without a vector sub-target.

## Changes, by file

### `schema/earp.config.v1.schema.json` — additive optional block

`scenario` gains optional `projections`:

```json
"projections": {
  "type": "object", "additionalProperties": false,
  "properties": {
    "declare": { "type": "array", "minItems": 1, "items": {
      "type": "object", "additionalProperties": false,
      "required": ["name", "roles"],
      "properties": {
        "name":  { "type": "string" },
        "roles": { "type": "array", "minItems": 1,
                   "items": { "enum": ["filterable", "searchable", "rankable"] } },
        "fts":    { "type": "boolean" },
        "vector": { "type": "boolean" }
      }}},
    "readiness_timeout_s": { "type": "number", "minimum": 0.1, "maximum": 300 }
  },
  "required": ["declare"]
}
```

Walker constraint (review-verified): the stdlib walker interprets
`minimum`/`maximum` but not `exclusiveMinimum` or `minLength`, so the window
is `minimum: 0.1` and non-empty `name` is enforced by the **resolver** as a
collected error, not by the schema.

Deliberately **excluded** from v1: `fts_tokenizer` and `vector_embedder`
(fact 6 — no supported custom implementations; a stored identity is not
proof of a runtime), `drop` (scenarios own a fresh DB; there is nothing to
drop), and `source` segments (nested source projections are a wider surface
than the witnesses this slice owes; deferring keeps the knob catalog
honest). Each exclusion is a catalog `UNSUPPORTED` entry with the reason, so
the refusal is self-documenting.

Absent `projections` block = today's behaviour, byte-for-byte, **for
configs not calling `Engine.search_projected_text`**; hashes of existing
configs do not move (raw-doc hashing, as established in S6a).
`readiness_timeout_s` defaults to 30.

`Engine.search_projected_text` now **requires** a `projections` block
declaring the named projection. This is a resolution-behaviour change owned
openly: today such a config resolves and then dies at run time with
`InvalidFilterError` on every run (fresh DB per scenario — there is never a
pre-existing projection to find), so no working config is affected; the
error simply moves from twenty minutes into the run to resolution, per the
matrix invariant. No existing test asserts clean resolution of a
projected-text config.

### `config.py`

- `CONSUMER_REGISTRY`: **three** registrations (the walker yields the object
  node itself): `scenario.projections`, `scenario.projections.declare`, and
  `scenario.projections.readiness_timeout_s` → `Consumer("S7")`.
- Resolution validates matrix coherence, collected:
  - `Engine.search_projected_text` with a `projection_name` not among the
    declared FTS-bearing specs (`fts: true` and `"searchable"` in roles) →
    `CONFIG_INVALID_VALUE` naming the declared set;
  - a declared spec with `vector: true` while
    `scenario.engine.use_default_embedder` is false → collected error.
    Rationale (fact 7): readiness goes **vacuously ready** with zero dense
    vectors behind it, so the poll witness would *lie* rather than time
    out — resolution is the only place this dishonest config can be caught;
  - `fts: true` requires `"searchable"` in roles (mirror of the engine's
    sub-target rule, which refuses typed — verified), same for
    `vector: true`;
  - a declared spec whose `name` is empty → collected error (walker cannot
    express `minLength`).
- `ResolvedScenario` gains `projections: tuple[DeclaredProjection, ...]` and
  `readiness_timeout_s: float`.

### `schema/models.py`

- Frozen `DeclaredProjection` dataclass (name, roles, fts, vector).
- Frozen `ProjectionWitnesses` dataclass carrying the three signals under
  their source names:
  `configure_delta` (built/dropped/deferred/unchanged/vector_unsupported_kinds),
  `readiness` (mapping name → `ready|embedding|not_declared`),
  `open_report` (dense_disabled, dense_disabled_reason, query_backend,
  refusal_count).
- **No new `BlockerCode` members.** The three conditions were pre-declared
  for exactly this slice and already pass the result schema's closed enum
  (review-verified by executing the validator): `VECTOR_UNSUPPORTED_KINDS`
  ("from the projection delta"), `DENSE_READINESS_TIMEOUT` ("stayed
  embedding past the declared timeout"), and `DENSE_DISABLED` — which
  `classify_open` already emits today. Zero result-schema edits for codes.

### `runner.py`

Execution order per scenario (fresh DB, as today): open → capture
open-report witness → `configure_projections(declared)` **before ingest** →
capture delta witness → ingest → poll readiness → query.

- **Delta witness** (source: `configure_projections` return): non-empty
  `vector_unsupported_kinds` → typed blocker `VECTOR_UNSUPPORTED_KINDS`
  (permanent, per `earp.md`). The delta is recorded verbatim either way,
  including the non-disjoint built/deferred lists (fact 1).
- **Readiness witness** (source: `read.projections()` poll): after ingest,
  poll every 0.5 s until every `vector: true` spec reads `ready`, or
  `readiness_timeout_s` elapses. A timeout with any spec still `embedding` →
  typed blocker `DENSE_READINESS_TIMEOUT` recording the stuck specs.
  Specs with `vector: false` are recorded `not_declared` (meaning: no
  *vector sub-target* declared — the projection itself is), never
  polled-for. The `poll_override` seam replaces **both** the
  `read.projections` call and the clock — it supplies the (specs, elapsed)
  view per iteration, so the timeout test performs zero real waiting
  (mirrors S5's `query_override` precedent).
- **Open-report witness** (source: `open_report()` at open): this is an
  **amendment to `classify_open`, not an addition** — the function already
  captures the open-report fields and already emits `DENSE_DISABLED`
  unconditionally (`runner.py:98-143`), pinned by
  `test_dense_disabled_is_blocked`. It gains scenario-awareness (a
  `dense_required: bool` parameter derived from the declared specs):
  `dense_disabled` is the typed blocker only when the scenario declares a
  `vector: true` spec (matching `earp.md`'s "typed blocker when dense
  retrieval was required"), and is otherwise recorded in the witness
  without blocking. `test_dense_disabled_is_blocked` is amended
  accordingly (listed under "existing tests that change").
  `vector_equivalence_refusal_count()` is an Engine method, so it is read
  by the runner at capture time and passed into the witness alongside the
  report mapping — `classify_open` stays a pure function.
- Witnesses land in the sidecar as a `projection_witnesses` object (see
  result schema); the existing `projection_coverage` witness stays.
- When no `projections` block is declared, the runner records the
  open-report witness only (it is free and always true) and skips the other
  two — a sidecar reader can distinguish "not declared" from "not captured"
  because `configure_delta` and `readiness` are absent rather than empty.

### `schema/earp.result.v1.schema.json`

Optional `projection_witnesses` object on the scenario block mirroring
`ProjectionWitnesses` (additive; absent for pre-S7 sidecars and
projection-less runs — no version bump, same rule as S6a's `fanout_used`).
The scenario block's pre-existing, never-written `projections` **array**
slot is **retired in place**: its description is amended to point at
`projection_witnesses` as the owner of projection state, and it remains
unwritten (removing it would be the only breaking edit in the slice; a
pointer costs nothing and keeps old validators working).

### `knobs.py`

- The existing `configure_projections` entry (INDEXING, call path
  `Engine.configure_projections`, witness `projection_delta`) is
  **replaced** by `projections.declare` → SEMANTIC, call path
  `Engine.configure_projections(specs=)`, witness = the delta. One call
  path, one entry: the old entry classified the capability before any
  config could express it; now that one can, the config-facing name and the
  SEMANTIC classification (it alters stored data and results) are the
  truthful record.
- `projections.readiness_timeout_s` → RUNTIME, call path
  `fathomdb.read.projections` (poll bound), witness = readiness map.
- UNSUPPORTED entries with reasons: `fts_tokenizer`, `vector_embedder`,
  `projections.drop`, `projections.source`.

## Acceptance criteria

1. A config declaring a projection and querying it via
   `search_projected_text` resolves, runs, and scores; the same config with
   an undeclared `projection_name` is a collected config error naming the
   declared set.
2. The three witnesses are recorded from their true sources: the delta
   object appears exactly as `configure_projections` returned it; the
   readiness map comes from polling `read.projections`; the open-report
   witness carries `dense_disabled(_reason)`, `query_backend`, and the
   refusal count.
3. `None` readiness never appears in a sidecar: every declared spec reports
   `ready`, `embedding`, or `not_declared`, and `not_declared` occurs only
   for `vector: false` specs.
4. A `vector: true` declaration with `use_default_embedder: false` is a
   collected config error, not a run that records a vacuously-`ready`
   witness (fact 7: it would never time out — it would lie).
5. A readiness timeout is the typed blocker `DENSE_READINESS_TIMEOUT`
   (blocked-run recording per S4 — indexed with a blocked verdict, no
   metrics); simulated via the `poll_override` seam with zero real waiting.
6. Non-empty `vector_unsupported_kinds` is the typed blocker
   `VECTOR_UNSUPPORTED_KINDS`; `dense_disabled` with a dense-requiring
   scenario is the typed blocker `DENSE_DISABLED`, and without one it is
   witness-recorded, not blocking. Neither is representable as an empty
   result.
7. A projection-less config produces a sidecar whose `configure_delta` and
   `readiness` are absent (not empty), and whose open-report witness is
   present.
8. `ruff` clean, pyright at the worktree baseline (31; 0 in touched files),
   full suite green, catalog/consumer guards green.

## Test-first sequence (RED before GREEN)

1. Schema: `projections` block accepted; unknown keys inside it refused;
   `roles` enum enforced; timeout range enforced.
2. Resolver: undeclared-`projection_name` collected error; `vector` without
   embedder collected error; `fts` without `searchable` collected error;
   consumer-registry guard covers the new paths.
3. Witness dataclasses: `not_declared` derivation from
   `(vector=False, None)`; `embedding`/`ready` pass-through; `None` on a
   `vector: true` spec is a contract violation (assert).
4. Runner (real engine, small fixture): declared FTS projection →
   `search_projected_text` succeeds end-to-end with the delta witness
   recorded; timeout path via `poll_override` (no real waiting); sidecar
   shape per AC-7. The embedder-on readiness test (real model load) is
   **opt-in behind the established `integration` marker / env-gate pattern
   (`pyproject.toml:89-96`), visibly SKIPPED by default with a reason** —
   D-2's rule as prior slices implement it, not availability-conditional.
5. Existing tests that change (deliberate): `test_dense_disabled_is_blocked`
   gains the scenario-awareness arm (blocks only when dense is required;
   witness-recorded otherwise). No other regressions expected — additive
   block; the S5 diagnostic fixture runs unchanged.

## Out of scope

- Nested `source` projections, custom tokenizers/embedders, `drop` — all
  catalog-refused with reasons (above).
- Any retrieval-quality claim about projected search (S8 owns comparisons).
- The corpus-scale characterization path (`characterize.py` does not declare
  projections; unchanged).

## Review

Independent code-grounded executing review, 2026-08-07. Verdict: **PROCEED
WITH REVISIONS**. The three-signal contract, witness structure, and
test-first sequence verified sound; two blockers and one overturned engine
fact caught before implementation. All 11 required edits incorporated above:

| # | Severity | Finding | Resolution |
| ---: | --- | --- | --- |
| 1 | BLOCKER | Proposed "new" BlockerCodes duplicate pre-declared ones and fail the result schema's closed enum — the blocked-run path would crash in `write_run` | Reuse `VECTOR_UNSUPPORTED_KINDS` / `DENSE_READINESS_TIMEOUT` / `DENSE_DISABLED`; zero schema edits |
| 2 | BLOCKER | `exclusiveMinimum` silently uninterpreted and `minLength` loudly unsupported by the stdlib walker | `minimum: 0.1`; empty-name enforcement moved to the resolver |
| 3 | MAJOR | "Can never become ready" is false — readiness goes **vacuously ready** with no embedder (executed) | Fact 7 added; AC-4 rationale rewritten: the witness would lie, not time out |
| 4 | MAJOR | Conditional `DENSE_DISABLED` is an unlisted amendment to `classify_open` (pure, unconditional today, pinned by a test) | Spelled out: scenario-awareness parameter, pure-function preservation, test amendment listed |
| 5 | MAJOR | Absent-block × `search_projected_text` contradiction | Projected-text calls now require the block; byte-for-byte claim scoped |
| 6 | MAJOR | Embedder-availability skip violates D-2's opt-in pattern | `integration`-marker opt-in, visibly skipped by default |
| 7 | MINOR | Registry needs three paths (walker yields the object node) | Three registrations listed |
| 8 | MINOR | Fact 1's built/deferred observation wrong — lists are non-disjoint | Corrected; witness interpretation keys on `deferred` |
| 9 | MINOR | Catalog would carry two entries for one call path | Old `configure_projections` entry replaced, rationale recorded |
| 10 | MINOR | Result schema already has a dead `scenario.projections` array slot | Retired in place with a pointer description |
| 11 | MINOR | `poll_override` contract underspecified | Pinned: replaces poll + clock; zero real waiting |
