---
status: PROPOSED
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
   `vector_unsupported_kinds`. FTS sub-targets build same-transaction
   (`built=['title']` observed); vector sub-targets defer
   (`deferred=['body']` observed). `vector_unsupported_kinds` exists **only**
   on this delta — `read.projections()` never carries it.
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
        "name":  { "type": "string", "minLength": 1 },
        "roles": { "type": "array", "minItems": 1,
                   "items": { "enum": ["filterable", "searchable", "rankable"] } },
        "fts":    { "type": "boolean" },
        "vector": { "type": "boolean" }
      }}},
    "readiness_timeout_s": { "type": "number", "exclusiveMinimum": 0, "maximum": 300 }
  },
  "required": ["declare"]
}
```

Deliberately **excluded** from v1: `fts_tokenizer` and `vector_embedder`
(fact 6 — no supported custom implementations; a stored identity is not
proof of a runtime), `drop` (scenarios own a fresh DB; there is nothing to
drop), and `source` segments (nested source projections are a wider surface
than the witnesses this slice owes; deferring keeps the knob catalog
honest). Each exclusion is a catalog `UNSUPPORTED` entry with the reason, so
the refusal is self-documenting.

Absent `projections` block = today's behaviour, byte-for-byte; hashes of
existing configs do not move (raw-doc hashing, as established in S6a).
`readiness_timeout_s` defaults to 30.

### `config.py`

- `CONSUMER_REGISTRY`: `scenario.projections.declare` and
  `scenario.projections.readiness_timeout_s` → `Consumer("S7")`.
- Resolution validates matrix coherence, collected:
  - `Engine.search_projected_text` with a `projection_name` not among the
    declared FTS-bearing specs (`fts: true` and `"searchable"` in roles) →
    `CONFIG_INVALID_VALUE` naming the declared set;
  - a declared spec with `vector: true` while
    `scenario.engine.use_default_embedder` is false → collected error (a
    dense sub-target with no embedder can never become ready; declaring it
    is a config that cannot be honestly executed);
  - `fts: true` requires `"searchable"` in roles (mirror of the engine's
    sub-target rule), same for `vector: true`.
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
- New `BlockerCode` members: `PROJECTION_UNSUPPORTED_KINDS`,
  `PROJECTION_READINESS_TIMEOUT`, `DENSE_DISABLED`.

### `runner.py`

Execution order per scenario (fresh DB, as today): open → capture
open-report witness → `configure_projections(declared)` **before ingest** →
capture delta witness → ingest → poll readiness → query.

- **Delta witness** (source: `configure_projections` return): non-empty
  `vector_unsupported_kinds` → typed blocker `PROJECTION_UNSUPPORTED_KINDS`
  (permanent, per `earp.md`). The delta is recorded verbatim either way.
- **Readiness witness** (source: `read.projections()` poll): after ingest,
  poll every 0.5 s until every `vector: true` spec reads `ready`, or
  `readiness_timeout_s` elapses. A timeout with any spec still `embedding` →
  typed blocker `PROJECTION_READINESS_TIMEOUT` recording the stuck specs.
  Specs with `vector: false` are recorded `not_declared`, never polled-for.
- **Open-report witness** (source: `open_report()` at open): recorded
  always; `dense_disabled: true` while the scenario declares any
  `vector: true` spec → typed blocker `DENSE_DISABLED` carrying the reason
  and `vector_equivalence_refusal_count()`.
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

### `knobs.py`

- `projections.declare` → SEMANTIC, call path
  `Engine.configure_projections(specs=)`, witness = the delta.
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
   collected config error, not a run that times out.
5. A readiness timeout is the typed blocker `PROJECTION_READINESS_TIMEOUT`
   (blocked-run recording per S4 — indexed with a blocked verdict, no
   metrics); simulated via a `poll_override` seam, not a real 30 s wait.
6. Non-empty `vector_unsupported_kinds` is the typed blocker
   `PROJECTION_UNSUPPORTED_KINDS`; `dense_disabled` with a dense-requiring
   scenario is the typed blocker `DENSE_DISABLED`. Neither is representable
   as an empty result.
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
   recorded; readiness map `ready` for a vector spec with embedder on
   (skip-marked if the default embedder is unavailable in CI, per D-2's
   opt-in rule); timeout path via `poll_override`; sidecar shape per AC-7.
5. Existing-suite regressions: none expected — additive block; the S5
   diagnostic fixture runs unchanged.

## Out of scope

- Nested `source` projections, custom tokenizers/embedders, `drop` — all
  catalog-refused with reasons (above).
- Any retrieval-quality claim about projected search (S8 owns comparisons).
- The corpus-scale characterization path (`characterize.py` does not declare
  projections; unchanged).

## Review

Pending independent code-grounded review.
