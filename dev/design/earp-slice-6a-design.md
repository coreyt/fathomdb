---
status: COMPLETE
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
   typed engine error — refusal, not clamp.
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
`maximum: 100`. **Absent means 10**, the engine default. `config_sha256` is
computed over the raw document (`config.py:444` → `_lib.canonical_json`), so
no existing config's hash moves.

Two resolution-behaviour changes are owned openly rather than hidden behind
"nothing changes":

- an fts_only config declaring `evidence_recall_k` beyond 10 with no `limit`
  key was **accepted** under the old unbounded-FTS doctrine and is now
  **refused** — the honest correction, because the rebuilt engine really does
  return at most 10 by default;
- K > 100 becomes permanently unmeasurable in every mode
  (`ENGINE_MAX_RESULT_LIMIT`).

`evidence_recall_k`'s own description at `earp.config.v1.schema.json:164`
still recites the retired D-5 doctrine ("fts_only is unbounded") and is
amended in the same edit.

This remains an additive amendment to the S0 lock, not a new schema version:
nothing hashes the schema files, absence-means-10 reproduces prior behaviour
at the hash and runner level, and the doctrine change above is this design's
reviewed content, not silent drift.

### `schema/models.py`

- `MAX_MEASURABLE_K` (mode → cap) is **deleted**; nothing else consumes it.
- `PRODUCTION_RERANK_LIMIT` is renamed in place to
  `ENGINE_DEFAULT_RESULT_LIMIT = 10` and joined by
  `ENGINE_MAX_RESULT_LIMIT = 100`, mirroring the engine constants they pin.
  The S2 drift detector guards `ir_eval.rs`, **not** `lib.rs`, so these
  mirrors get their own binding-present guard test: assert
  `inspect.signature(Engine.search).parameters["limit"].default ==
  ENGINE_DEFAULT_RESULT_LIMIT`, and pin the window empirically —
  `limit=100` accepted, `limit=101` refused.

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
- `CONSUMER_REGISTRY` gains `"scenario.query.limit": Consumer("S5")` — the
  same owner as the other query knobs — so the every-schema-path-has-a-
  consumer guard stays green.
- Range validation: the **schema owns the window** (`minimum: 1`,
  `maximum: 100`), following the alpha precedent — the stdlib walker already
  refuses 0 / 101 / non-integer / bool as collected `CONFIG_INVALID_VALUE`
  errors, and a duplicate resolver check would double-report. The schema
  key's `description` names the engine window
  (`validate_search_result_limit`, 1..=100) so the refusal is
  self-explaining; no bespoke resolver check is added.
- Resolver: `limit` resolves from the config (default 10) and is **injected
  into `query_params`** — the single source the runner already passes
  through (`config.py:449` → `runner.py:259-264`), so no duplicate-kwarg
  path exists and the runner needs no knowledge of the knob. Every declared
  `evidence_recall_k` is checked via `check_depth(mode, k, limit)`.
- `ResolvedScenario.max_measurable_k` becomes the resolved `limit` (an `int`,
  never `None` — "unbounded" no longer exists).
- Catalog (`knobs.py`): one new entry —
  `Knob(name="limit", classification=SEMANTIC, call_path="Engine.search(limit=)")`
  with witness = result cardinality. The coverage test resolves `limit` for
  all three signatures from the entry's `name`, as it does for `alpha`.

### `runner.py` / `characterize.py` / `cli.py`

- The runner stays a pass-through: the resolved limit arrives via
  `query_params` (above). Its diagnostic sidecar gains `fanout_used` in the
  scenario block (`runner.py:_write`), which it does not record today.
  `query_override` test callables gain the new kwarg.
- `characterize()` does not resolve a config, so it cannot take "the resolved
  limit": it passes `limit=max(ladder)` **explicitly** at its
  `search_text_only` call (`characterize.py:304`), refuses a ladder whose
  max exceeds `ENGINE_MAX_RESULT_LIMIT`, and records that value at the two
  `fanout_used` sites. Today it truncates to
  `deepest = max(ladder)` while calling with the engine default — a ladder
  of (5, 10, 50) would silently score @50 over 10 hits. `DEFAULT_FANOUT` is
  deleted, including from `__all__`.
- `cli.py:51` prints `max_measurable_k or 'unbounded'`; the `'unbounded'`
  branch is now dead. Replaced with an honest `result limit` line.

### `schema/earp.result.v1.schema.json`

`fanout_used`'s description is corrected: it records **the public result
limit in effect for the run** (the engine's own fanout for hybrid/vector is
`max(limit, TOP_K_BIT_CANDIDATES)` internally and is not caller-visible; what
EARP can honestly record is the limit it requested, which now bounds visible
cardinality in every mode). Field name and type are unchanged — no result
schema version bump. The description **names the cutover**: before 6a the
value was the engine default in effect (always 10); from 6a it is the
requested public limit. The rename to `result_limit` was considered and
declined to avoid a version bump; the reuse is honest only because every
existing record used ladder (5, 10) with the default fanout of 10, so the
two readings coincide on all sidecars that exist.

## Acceptance criteria

1. `test_catalog_covers_the_search_signatures` passes: `limit` is covered for
   all three search verbs.
2. `limit` in `1..=100` is accepted; 0, 101, non-integer, and bool are
   refused **by schema validation** as collected `CONFIG_INVALID_VALUE`
   errors; the schema description names the engine's window so the refusal
   is self-explaining.
3. `@K` with `K ≤ limit` is admitted for **every** mode (pinned:
   `limit=50, k=50, hybrid` resolves); `K > limit` is refused as
   `METRIC_NOT_MEASURABLE` whose message names the Slice 18 `limit` lever and
   does not name D-5.2 or `set_search_limit_for_test`.
4. A config with no `limit` key resolves to 10 and its `config_sha256` is
   byte-identical to its pre-slice value (pinned against a stored hash).
5. The resolved limit is recorded in the sidecar with every number
   (`fanout_used == limit`), and the runner demonstrably passes it to the
   engine call — witnessed by exact cardinality: a fixture with more than 3
   matching documents searched with `limit=3` returns **exactly** 3 hits.
6. The engine-mirror constants are guarded: `Engine.search`'s `limit` default
   equals `ENGINE_DEFAULT_RESULT_LIMIT`, `limit=100` is accepted, and
   `limit=101` is refused (binding-present test).
7. `ruff` clean, `pyright` 0 errors, full EARP suite green.

## Existing tests that change (deliberate, not improvised)

| Test | Change |
| --- | --- |
| `test_metrics_parity.py::test_measurable_depth_is_allowed_for_every_mode` | 2-arg `check_depth` calls gain `limit` |
| `test_metrics_parity.py::test_deep_k_is_refused_for_vector_and_hybrid` | drops the `"SEARCH_RERANK_LIMIT" in message` pin; pins the new lever wording |
| `test_metrics_parity.py::test_deep_k_is_allowed_for_fts_only` | doctrine inverted: k=200 is now refused for every mode; rewritten as a `k ≤ limit` admission plus a `k > 100` permanent refusal |
| `test_config_resolver.py::test_mode_derives_from_call_and_embedder` | `CALL_MODE` tuples lose the max-K column |
| `test_config_resolver.py::test_deep_k_allowed_for_text_only` | must declare `limit: 50` to keep its deep ladder |

Stale docstrings at `test_config_resolver.py:215,241` and `models.py:250`
are corrected in the same commit.

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

Independent code-grounded executing review, 2026-08-07. Verdict: **PROCEED
WITH REVISIONS** — the contract and the two central mechanical claims
(config-hash identity, single-knob catalog coverage) verified by execution;
two factual claims corrected and five missed touch points added. All eight
required edits are incorporated above:

| # | Severity | Finding | Resolution |
| ---: | --- | --- | --- |
| 1 | MAJOR | The S2 drift detector guards `ir_eval.rs`, not `lib.rs`; the mirrored constants would drift silently | Claim deleted; dedicated binding-present guard test added (AC-6) |
| 2 | MAJOR | "No config changes meaning" overstated — fts_only deep-K flips from accepted to refused; two green tests pin the old doctrine | Doctrine change owned in prose; breaking tests enumerated |
| 3 | MAJOR | `characterize()` never resolves a config and today scores @K>10 over 10 hits silently | `limit=max(ladder)` passed explicitly; deep ladders refused; `DEFAULT_FANOUT` deleted |
| 4 | MAJOR | Breaking-test inventory absent | "Existing tests that change" section added |
| 5 | MINOR | `scenario.query.limit` missing from `CONSUMER_REGISTRY` | Added, owner S5 |
| 6 | MINOR | `cli.py` `'unbounded'` branch goes dead | cli.py added to changes |
| 7 | MINOR | `evidence_recall_k` schema description still recites D-5 | Amended in the same schema edit |
| 8 | MINOR | Walker/resolver double-reporting risk | Schema owns the window (alpha precedent); AC-2 restated |
| 9 | MINOR | Runner duplicate-kwarg path; sidecar lacks `fanout_used` | Limit injected via `query_params`; sidecar field added |
| 10 | MINOR | `fanout_used` cutover unrecorded across old/new sidecars | Description names the cutover and the declined rename |
