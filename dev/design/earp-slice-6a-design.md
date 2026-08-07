---
status: PROPOSED
---

# EARP Slice 6a — public result-limit adoption (D-5 successor)

Design of record for the interstitial slice commissioned by the D-5 retirement
(`dev/notes/earp-hitl-decisions.md`, 2026-08-07). Sits between S6 and S7;
depends on S6 and on 0.8.22 Slice 18 being present in the working tree (merged
at `44f727a6`, binding rebuilt).

## Why this slice exists

0.8.22 Slice 18 ("bound ranked retrieval results", `c7779a76`) gave every
search verb a public `limit`. EARP's depth doctrine — refuse @K>10 for
vector/hybrid, treat FTS as unbounded — describes the engine that no longer
exists. Left alone, EARP would refuse depths the engine now delivers
(@20/@50 with `limit=50`) and, worse, would keep recording `fanout_used=10`
while the runner's calls default to an engine parameter EARP does not model.
The catalog guard already caught this:
`test_catalog_covers_the_search_signatures` fails on `search:limit`.

## Engine facts this design stands on (verified in the merged tree)

1. `Engine.search`, `Engine.search_text_only`, `Engine.search_projected_text`
   all take keyword `limit: int = 10` (`engine.py:353,530,578`).
2. `DEFAULT_SEARCH_RESULT_LIMIT = 10`, `MAX_SEARCH_RESULT_LIMIT = 100`;
   `validate_search_result_limit` **refuses** values outside `1..=100` with a
   typed engine error — refusal, not clamp (`lib.rs:10027-10040`).
3. Text-only mode puts a real `LIMIT {final_limit}` in the node-FTS SQL
   (`fts_only_limit = query_vector.is_none().then_some(final_limit)`); the
   candidate set is bounded at the source, not post-truncated.
4. The vector phase-1 fanout scales as
   `max(candidate_limit, final_limit, TOP_K_BIT_CANDIDATES)`; visible results
   are truncated to the caller's limit after ranking. Depths up to the limit
   are therefore real for vector and hybrid, not copies of a shallower page.
5. Hybrid's text branch still ranks **unbounded** before the post-fusion
   cutoff. The limit bounds *visible cardinality* for hybrid, not internal
   text-branch cost. (Cost re-characterization is S6's re-run, out of scope
   here.)
6. `search_projected_text`'s reader break now honours the same public limit;
   the 2026-08-06 D-5 correction is historical.
7. `set_search_limit_for_test` remains unexported (D-5.3 stands); the public
   `limit` replaces every use EARP had for it.

## Contract

One rule replaces the mode table: **@K is measurable exactly when
`K ≤ limit`, for every retrieval mode, with `limit` validated to the engine's
own `1..=100` window and recorded with every number.** Mode is still derived
from `(call, use_default_embedder)` — mode determines *cost and semantics*,
no longer *depth*.

## Changes, by file

### `schema/earp.config.v1.schema.json` — additive optional key

`scenario.query` gains optional `limit`: integer, `minimum: 1`,
`maximum: 100`. **Absent means 10**, the engine default — identical to every
pre-slice run, so no existing config changes meaning and no `config_sha256`
moves. This is an additive amendment to the S0 lock, not a new schema
version: the lock exists to prevent silent semantic drift, and an optional
key whose absence reproduces prior behaviour byte-for-byte introduces none.

### `schema/models.py`

- `MAX_MEASURABLE_K` (mode → cap) is **deleted**; nothing else consumes it.
- `PRODUCTION_RERANK_LIMIT` is renamed in place to
  `ENGINE_DEFAULT_RESULT_LIMIT = 10` and joined by
  `ENGINE_MAX_RESULT_LIMIT = 100`, mirroring the engine constants they pin.
  A comment records that these mirror `DEFAULT_SEARCH_RESULT_LIMIT` /
  `MAX_SEARCH_RESULT_LIMIT` and must move with them; the S2 drift detector
  already guards the reference file that defines the semantics.

### `depth.py`

`check_depth(mode, k)` becomes `check_depth(mode, k, limit)`:

- returns `None` when `k <= limit` (any mode);
- returns a `METRIC_NOT_MEASURABLE` blocker when `k > limit`, whose message
  names 0.8.22 Slice 18's public limit as the lever ("raise `limit` up to
  100") instead of the retired D-5.2 commissioning and the hidden seam.

Limit *range* validation is not `check_depth`'s job (it is a config-shape
fact, not a depth fact); the resolver owns it, below.

### `config.py`

- `CALL_MODE` drops its per-call max-K column — it maps to `RetrievalMode`
  only. The `search_projected_text` cap comment goes; the mode-derivation
  comment (embedder, not call alone) stays.
- `CALL_PARAMS`: all three search calls gain `"limit"`.
- Resolver: `limit` resolves from the config (default 10), is validated
  against `1..=ENGINE_MAX_RESULT_LIMIT` as a **collected** config error
  (mirroring the engine's refusal window so an invalid value is caught at
  validation, not mid-run as an engine exception), and every declared
  `evidence_recall_k` is checked via `check_depth(mode, k, limit)`.
- `ResolvedScenario.max_measurable_k` becomes the resolved `limit` (an `int`,
  never `None` — "unbounded" no longer exists).
- Catalog (`knobs.py`): one new entry —
  `Knob(name="limit", classification=SEMANTIC, call_path="Engine.search(limit=)")`
  with witness = result cardinality. The coverage test resolves `limit` for
  all three signatures from the entry's `name`, as it does for `alpha`.

### `runner.py` / `characterize.py`

- The runner passes the resolved `limit` to whichever search verb the config
  names. No call site may rely on the engine default once the config carries
  the value.
- `characterize.py`'s `DEFAULT_FANOUT` indirection is replaced by the
  resolved limit.

### `schema/earp.result.v1.schema.json`

`fanout_used`'s description is corrected: it records **the public result
limit in effect for the run** (the engine's own fanout for hybrid/vector is
`max(limit, TOP_K_BIT_CANDIDATES)` internally and is not caller-visible; what
EARP can honestly record is the limit it requested, which now bounds visible
cardinality in every mode). Field name and type are unchanged — no result
schema version bump.

## Acceptance criteria

1. `test_catalog_covers_the_search_signatures` passes: `limit` is covered for
   all three search verbs.
2. The resolver accepts `limit` in `1..=100`; refuses 0, 101, and non-integer
   values as collected typed config errors naming the engine's window.
3. `@K` with `K ≤ limit` is admitted for **every** mode (pinned:
   `limit=50, k=50, hybrid` resolves); `K > limit` is refused as
   `METRIC_NOT_MEASURABLE` whose message names the Slice 18 `limit` lever and
   does not name D-5.2 or `set_search_limit_for_test`.
4. A config with no `limit` key resolves to 10 and its `config_sha256` is
   byte-identical to its pre-slice value (pinned against a stored hash).
5. The resolved limit is recorded in the sidecar with every number
   (`fanout_used == limit`), and the runner demonstrably passes it to the
   engine call (witnessed by cardinality: a diagnostic run with `limit=3`
   returns ≤ 3 hits).
6. `ruff` clean, `pyright` 0 errors, full EARP suite green.

## Test-first sequence (RED before GREEN)

1. The already-failing catalog coverage test (RED exists in the tree today).
2. Resolver: limit-range refusal triplet (0 / 101 / non-integer), collected.
3. Depth: `(hybrid, k=50, limit=50) → None`;
   `(fts_only, k=20, limit=10) → METRIC_NOT_MEASURABLE`; message-content
   assertions (names the lever, not the seam).
4. Identity: stored-hash pin for a limit-less config.
5. Runner: `limit=3` diagnostic run returns ≤ 3 hits and sidecar carries 3.

## Out of scope

- Hybrid pre-fusion cost measurement (S6 re-characterization re-run).
- Any exposure of `candidate_limit` / the test seam (D-5.3 stands).
- Comparison/sweep semantics of differing limits across arms (S8 owns
  `changed_knobs`; `limit` becomes just another knob there).

## Review

Pending independent code-grounded review.
