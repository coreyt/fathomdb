---
status: PROPOSED
---

# EARP Slice 8 — comparison and sweep statistics

Design of record for S8 of `dev/plans/earp-foundation.md` ("A comparative
claim is earned, not assumed"). Depends on S6 (characterization executor),
S6a (`limit` in `query_params`), S7 (projection matrix — arms may differ at
projection knobs). Plan requirement row: arms differ only at
`changed_knobs`; pairing on immutable query ids; CI method, seed, and power
rule fixed before running; `UNDERPOWERED` emitted against the declared rule.

## What S0 already locked (verified in the schemas)

- **The comparison stats tuple is locked, but the result side is NOT
  done** (review-executed against the S4 validator):
  `comparison: null | {n, effect, ci_low, ci_high, ci_method, seed,
  changed_knobs, underpowered}` validates clean, but every block is
  `additionalProperties: false`, so the design's own required content —
  `comparison.metric`, exclusions by reason, per-arm metrics/blockers,
  two-arm scenario identity — has nowhere to live without the additive
  edits listed under § writer.py below.
- **Config side is half-done.** `earp.config.v1.schema.json` has the
  `comparison` block (`changed_knobs` required; `strata`, `ci_method`,
  `seed`, `resamples`, `min_n` optional) and the D-4 `decision_rule` block.
- **Per-query side** has `query_id`, `stratum`, and typed `outcome` — the
  pairing substrate exists.
- **What is missing is the input:** `earp.v1` has one `scenario` and no
  arms array — `comparison` and `sweep` are in `INEXPRESSIBLE` for exactly
  that reason (`config.py:73-76`), and every `comparison.*` consumer is
  registered `_never`.

## Contract

A comparison is **two named arms over the same frozen corpus and gold,
whose resolved configurations differ at exactly the declared knob paths,
paired per immutable gold query id, with the CI method, seed, resample
count, effect metric, and power rule all fixed before the first retrieval.**
A sweep is N such arms with outcomes recorded and **no** comparative claim.
Everything else follows from four rules:

1. **One-knob honesty is symmetric.** Every path at which the resolved arms
   differ must be in `changed_knobs`, AND every `changed_knobs` path must
   actually differ. A declared knob that does not differ is as much a lie
   as an undeclared one that does.
2. **The effect is defined before it is seen.** `comparison.metric` (new
   config key) names the paired metric (e.g.
   `strict_evidence_recall@10`); `ci_method`, `seed`, `resamples`, and
   `min_n` become **required for comparison campaigns** at resolution
   (schema keeps them optional — sweep and future kinds share the block).
   `decision_rule` stays optional per D-4: without it the paired delta and
   CI are still computed and recorded (they are the comparison's *output*,
   per `earp.md:58`), but no better-than claim token is emitted.
3. **Pairing is on gold `query_id`, and only both-scored pairs count.** A
   query enters the paired set only when both arms produced a scored
   outcome for it; every exclusion is counted by reason (blocked, failed,
   abstain-mismatch is NOT an exclusion — abstention correctness is a
   scored outcome). `n` in the sidecar is the paired count.
   `underpowered = n < min_n`, meaningful only against the declared rule.
4. **One RNG, one method, pinned.** Percentile bootstrap over paired
   per-query deltas (`ci_method: paired_bootstrap`), driven by a Python
   port of the Rust harness's SplitMix64 (`eu8_ir_validation.rs:103`,
   `BOOTSTRAP_SEED` precedent at `:71`) — not `random`, not numpy. The
   port is pinned two ways: published SplitMix64 test vectors, and an
   executed expectations file generated once from the Rust `bootstrap_ci`
   (S2's methodology: run the reference locally, commit its actual output;
   D-1 forbids committing engine-crate tests, not running them once).
   `percentile_bootstrap` (unpaired) stays schema-legal but is refused by
   the resolver for comparison campaigns in v1 — one method, deliberately.

## Changes, by file

### `schema/earp.config.v1.schema.json` — additive

- Top-level optional `arms`: array (`minItems: 2`) of
  `{name: string, scenario: <$ref: "#/properties/scenario">}` — full
  scenario objects via `$ref` (review-executed: the walker resolves it and
  reports arm-internal defects with indexed paths; an inline copy would be
  a drift hazard). Not sparse overrides: inheritance is a resolver concern
  EARP v1 deliberately does not have; explicit arms are hashable and
  diff-able as written.
- `scenario` is **dropped from the top-level `required` list** (an
  arms-only doc must be able to validate; review-executed — without this
  edit every arms campaign fails `missing: scenario`). Per-kind presence
  is the resolver's, below.
- `comparison` block gains optional `metric` (string) — required at
  resolution for comparison campaigns (rule 2).
- `scenario` and `arms` are **mutually exclusive** (resolver-enforced;
  the schema cannot express it): `diagnostic`/`characterization`/`replay`
  require `scenario`; `comparison`/`sweep` require `arms`. `comparison`
  requires exactly 2 arms; `sweep` ≥ 2. Arm names must be unique and
  non-empty (resolver — walker has no `minLength`).

### `config.py`

- `INEXPRESSIBLE` loses `comparison` and `sweep`; the campaign-kind
  validation described above replaces it.
- `CONSUMER_REGISTRY`: gains **only `arms`** — review-executed:
  `declared_paths` never descends into arrays, so no `arms.*` path is
  derivable. Arm-internal governance happens **solely in the per-arm
  resolution pass**: each arm is resolved as a synthesized
  single-scenario document, which re-runs every consumer/applicability
  check (`scenario.fixture` inside an arm is caught exactly as it would
  be at top level). `comparison.*` paths flip from `_never` to
  `applies = campaign == "comparison"` **only** — a `sweep` config may
  NOT carry the `comparison` block in v1 (sweep makes no claim and
  declares no knob axis; outcomes only). The two existing tests pinning
  `_never` refusals are amended, not deleted (their
  characterization-carrying-comparison case still refuses).
  `comparison.metric` registered with the same predicate.
- Resolution for arms campaigns: resolve each arm's scenario with the
  existing single-scenario machinery (S6a limit injection and S7
  projection coherence apply per arm unchanged), then:
  - symmetric `changed_knobs` check over the per-arm **resolved
    representations projected back to config paths** — resolver defaults
    materialized (`limit` 10, `use_default_embedder` false,
    `readiness_timeout_s` 30) — NOT the raw docs. Review-executed
    canonical case: one arm declaring `limit: 10`, the other omitting it
    → raw docs differ, resolved effect identical → the declared knob does
    not differ → refused. Derived fields (`retrieval_mode`,
    `max_measurable_k`) are not diffable paths; a derived divergence
    attributes to its causal config path. A differing path not in
    `changed_knobs`, or a declared path that does not differ, are each
    collected errors naming the offending paths;
  - `comparison.metric` must name a metric the mode/limit can measure —
    reuses `check_depth` and metric eligibility (a `@K` metric with
    `K > limit` of either arm is refused at resolution);
  - comparison-required fields (rule 2) enforced, collected.
- `ResolvedConfig` grows `arms: tuple[ResolvedArm, ...]`
  (`ResolvedArm = (name, ResolvedScenario)`) and `comparison:
  ResolvedComparison | None` (metric, strata, ci_method, seed, resamples,
  min_n).

### `stats.py` (new module)

- `splitmix64(seed) -> Iterator[int]` — the RNG, pinned to the published
  Vigna vectors (seed 0 → `0xE220A8397B1DCDAF`, …) AND to the Rust
  harness's exact usage: index mapping is `next_u64() % n`
  (`eu8_ir_validation.rs:118`), not rejection sampling.
- `paired_bootstrap_ci(deltas, seed, resamples, alpha=0.05) ->
  (low, high)` — percentile bootstrap over the paired deltas, byte-faithful
  to the Rust `bootstrap_ci` (`eu8_ir_validation.rs:283`): percentiles are
  **truncated-index order statistics**, `lo = means[int(resamples *
  alpha/2)]`, `hi = means[min(int(resamples * (1 - alpha/2)),
  resamples - 1)]` — NOT interpolated. These numerics **intentionally
  diverge** from the numpy analogues (`paired_bootstrap_delta` uses
  `np.percentile` interpolation + PCG64); EARP adopts the Rust reference,
  not the forks — that is the point of rule 4.
- Expectations provenance: the Rust fn hardcodes
  `BOOTSTRAP_SEED = 0x0E88B007574A9001`, so the parity expectations are
  generated at that seed, via a verbatim standalone copy of the two Rust
  functions compiled with bare `rustc` (the in-crate `cargo test` route
  pays a full candle compile — zero cached artifacts in this worktree).
  The review already demonstrated bit-identical CI bounds
  (`lo_bits=0xbf8291d55da3d586, hi_bits=0x3f7f86a2f2417147`) between that
  copy and a pure-Python port. The generated expectations file is
  committed with a provenance note naming the source lines and rustc
  invocation.
- Pure, no engine, no numpy (D-1 footprint). Measured cost: 4,597 deltas ×
  1,000 resamples ≈ 1.7 s pure Python; the Rust precedent uses 1,000
  resamples — that is the default; 10,000 (~17 s) remains configurable.

### `comparison.py` (new module) + the executor refactor it requires

`run_characterization` cannot be "parameterized by an arm" as it stands
(review-verified): it synthesizes its own hardcoded config doc
(`search_text_only`, no embedder, no projections), passes
`limit=max(ladder)` rather than a resolved limit, ignores `query_params`,
and **writes its own run records on both its complete and blocked paths**.
Naive reuse would write standalone characterization records per arm and
let a blocked arm write its own blocked run. So:

- **Refactor (named):** extract a pure arm-executor core from
  `characterize.py` — ingest + `verify_gold` + retrieve loop + score →
  `(cache, errors, per-query rows)`, **no writes** — taking a
  `ResolvedScenario`, threading `query_call`/`query_params` through
  `_resolve_call` + `PARAM_RENAMES` (today runner-only), honoring the
  arm's resolved limit, embedder flag, and projections, with a per-arm
  `retrieve_override`. `run_characterization` becomes a thin
  single-arm wrapper around it (behaviour pinned by the existing S6
  tests); `comparison.py` owns **all** writing for arms campaigns.
- `run_comparison(...)`: per arm — fresh DB, the arm executor. Then: pair
  per-query rows by `query_id`, compute the paired deltas on
  `comparison.metric`, run `paired_bootstrap_ci`, evaluate
  `underpowered = n < min_n`, apply `decision_rule` if declared.
- **Arm order and effect sign are pinned:** `arms[0]` is control,
  `arms[1]` is treatment; `effect` = mean of per-query
  (treatment − control) deltas, accumulated in gold query order with
  plain sequential f64 summation (the same semantics the CI's means use).
- **The paired set is pinned:** queries with a **value on
  `comparison.metric` in both arms**. Negative-class rows are scored
  with `strict/graded = None` (verified), so they enter only an
  abstention-metric comparison; a scored-but-valueless query is counted
  under `exclusions.metric_inapplicable`, so
  `n + Σ exclusions == gold query_count` reconciles exactly.
- **`decision_rule.result` token mapping (result-schema enum
  `pass|fail|withheld|underpowered`):** rule absent → `decision_rule:
  null`; `n < min_n` → `underpowered` (takes precedence); `n == 0` with
  the run otherwise complete → `withheld`; otherwise `pass`/`fail`
  strictly by the predeclared direction and threshold.
- Sweep: same arm executor per arm, per-arm metrics and blockers recorded,
  `comparison: null` in the sidecar, no deltas, no CI, no claim.
- A blocked **treatment or control arm** blocks the comparison run (S4
  blocked-run recording; the surviving arm's partial results are recorded
  under its arm name — a one-armed comparison is not a comparison).
- Every per-query row gains its arm: per-query schema gets optional `arm`
  (additive; absent for single-scenario campaigns).

### `writer.py` / result schema — additive edits (S0 did NOT finish this)

- `comparison` block gains `metric` (string) and `exclusions` (object,
  reason → integer count).
- New optional top-level `arms` object keyed by arm name, each entry
  carrying per-arm scenario identity (`query_call`, `retrieval_mode`,
  `fanout_used`, the arm's synthesized-scenario sha — labeled as such),
  per-arm metrics (reusing `$defs/k_aggregate`), and per-arm blockers.
  This is where sweep outcomes and a blocked-comparison's surviving-arm
  partials live.
- Run identity is unchanged and already correct: `make_run_id` keys on
  the **whole-document** `config_sha256` (verified `_lib.py:182-205`),
  which covers both arms; per-arm synthesized hashes are supplementary
  labels, never the run identity.
- Per-query schema: optional `arm` property (additive). All schema edits
  follow the S6a/S7 additive no-bump rule.
- The single-scenario `scenario` block stays as-is; for arms campaigns it
  is written from the control arm with the `arms` object as the
  authoritative per-arm record (the sidecar reader's rule: `arms` present
  ⇒ read arms).

### `cli.py`

`earp validate` prints arm count, changed knobs, and the fixed statistics
tuple (metric, method, seed, resamples, min_n) for arms campaigns.

## Acceptance criteria

1. A two-arm config differing at exactly its declared `changed_knobs`
   resolves; the same config with an undeclared difference, or a declared
   knob that does not differ, is a collected error naming the paths.
2. `comparison` campaigns refuse resolution when `metric`, `ci_method`,
   `seed`, `resamples`, or `min_n` is absent; sweep campaigns do not
   require them.
3. Pairing: only queries scored in both arms enter `n`; exclusions are
   counted by reason in the sidecar; per-query rows carry `arm` and pair
   on `query_id`.
4. `stats.splitmix64` matches published test vectors;
   `paired_bootstrap_ci` byte-matches the executed Rust `bootstrap_ci`
   expectations file (same seed, same resamples, same deltas).
5. Determinism: two runs of the same comparison config produce identical
   `effect`, `ci_low`, `ci_high` (seeded RNG, no wall-clock dependence).
6. `underpowered` is `n < min_n` exactly; a comparison with
   `decision_rule` absent records effect and CI but emits no
   better-than claim; with a rule, the claim follows the predeclared
   direction/threshold only.
7. A blocked arm produces a blocked comparison run (S4 semantics), never a
   one-armed number.
8. Campaign-kind coverage table satisfied: `comparison` and `sweep` each
   have an owning, executable path; `INEXPRESSIBLE` no longer names them.
9. `ruff` clean; pyright 0 new / 0 in touched files; full suite green.

## Test-first sequence (RED before GREEN)

1. Resolver: arms/scenario mutual exclusion by kind; exact-2 vs ≥2;
   unique names; symmetric changed-knobs (both failure directions);
   comparison-required stats fields; `comparison.metric` eligibility
   against per-arm mode and limit.
2. Stats: SplitMix64 published vectors; bootstrap determinism; the
   executed Rust-parity expectations.
3. Pairing: both-scored intersection; exclusion counting; arm field on
   per-query rows.
4. End-to-end comparison on the fixture with `retrieve_override` per arm
   (zero engine cost, deterministic synthetic rankings): effect/CI/
   underpowered/decision-rule outcomes, including the no-rule
   report-but-no-claim case and the blocked-arm case.
5. Sweep end-to-end: N arms, per-arm outcomes, `comparison: null`.

## Strata (scoped commitment, not vacuous)

Nothing populates per-query `stratum` today (review-verified). S8 commits
to the minimal honest version: v1 `comparison.strata` entries are
restricted to the vocabulary `{"query_class"}`; declaring it sets each
per-query row's `stratum` to its gold `query_class`. Per-stratum CIs
remain out of scope — declared strata are recorded so the option is real,
not asserted.

## Out of scope

- Per-stratum CIs (see above — recorded, not inferred over).
- Sparse arm overrides / scenario inheritance.
- Unpaired `percentile_bootstrap` execution (schema-legal, resolver-refused
  for v1 comparisons).
- Any priced arm (S9 owns the D-3 gate).

## Review

Independent code-grounded executing review, 2026-08-07. Verdict: **PROCEED
WITH REVISIONS**. The statistical core was verified by execution to
byte-parity (a pure-Python SplitMix64+bootstrap port produced bit-identical
CI bounds to a verbatim copy of the Rust `bootstrap_ci`); four P1s and the
full edit list are incorporated above:

| # | Sev | Finding | Resolution |
| ---: | --- | --- | --- |
| 1 | P1 | "Result side is done" false — every locked block is closed; metric/exclusions/per-arm content had nowhere to live | Additive result-schema edits specified (§ writer.py) |
| 2 | P1 | `scenario` in top-level `required` — no arms doc could ever validate | Dropped from `required`; per-kind presence is the resolver's |
| 3 | P1 | changed_knobs raw-vs-resolved self-contradiction; raw reading forces declaring no-effect knobs | Resolved-representation diff pinned; limit-vs-default is the canonical refused case |
| 4 | P1 | Executor reuse understated — `run_characterization` hardcodes its config and writes its own records on both paths | Pure arm-executor extraction named; all arms-campaign writes in comparison.py |
| 5 | P2 | Paired-set ambiguity (negatives score with null metric values) | Paired set = valued-in-both-arms; `exclusions.metric_inapplicable`; n reconciles with query_count |
| 6 | P2 | Control/treatment order and effect sign undeclared | arms[0]=control; effect = mean(treatment−control), sequential f64 |
| 7 | P2 | Parity plan under-specified (hardcoded seed; order-stat percentiles; candle compile cost) | Seed/percentile/index-map pinned; standalone-rustc generation with provenance |
| 8 | P2 | `declared_paths` never yields `arms.*`; array interiors evade consumers | Registry gains only `arms`; per-arm resolution re-runs all governance |
| 9 | P2 | Consumer flip would silently accept comparison blocks on characterizations | `applies = campaign == "comparison"`; sweep forbids the block; tests amended |
| 10 | P2 | `decision_rule.result` token mapping unpinned | Mapped: null / underpowered (precedence) / withheld (n==0) / pass / fail |
| 11 | P3 | "Strata recorded per query" was vacuous | Scoped commitment: vocabulary {"query_class"}, populated when declared |
| 12 | P3 | Scenario reuse mechanism unstated | `$ref: "#/properties/scenario"` pinned (walker-verified) |
| 13 | P3 | Per-arm hash identity ambiguity | Whole-doc sha stays the identity; arm hashes are labeled supplements |
