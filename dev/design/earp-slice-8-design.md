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

- **Result side is done.** `earp.result.v1.schema.json` carries
  `comparison: null | {n, effect, ci_low, ci_high, ci_method, seed,
  changed_knobs, underpowered}` and the pinned verdict vocabulary.
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
  `{name: string, scenario: <the existing scenario object>}` — full
  scenario objects, not sparse overrides (inheritance is a resolver
  concern EARP v1 deliberately does not have; explicit arms are hashable
  and diff-able as written).
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
- `CONSUMER_REGISTRY`: `arms`, `arms.name`, `arms.scenario` (+ the walker's
  object-node yields as found by `declared_paths` — the S7 lesson) →
  `Consumer("S8")`; every `comparison.*` path flips from
  `Consumer("S8", _never)` to a real S8 consumer; `comparison.metric`
  registered.
- Resolution for arms campaigns: resolve each arm's scenario with the
  existing single-scenario machinery (S6a limit injection and S7
  projection coherence apply per arm unchanged), then:
  - symmetric `changed_knobs` check over the resolved-arm mappings
    (canonical-path walk of the raw arm docs; a differing path not in
    `changed_knobs`, or a declared path that does not differ, are each
    collected errors naming the offending paths);
  - `comparison.metric` must name a metric the mode/limit can measure —
    reuses `check_depth` and metric eligibility (a `@K` metric with
    `K > limit` of either arm is refused at resolution);
  - comparison-required fields (rule 2) enforced, collected.
- `ResolvedConfig` grows `arms: tuple[ResolvedArm, ...]`
  (`ResolvedArm = (name, ResolvedScenario)`) and `comparison:
  ResolvedComparison | None` (metric, strata, ci_method, seed, resamples,
  min_n).

### `stats.py` (new module)

- `splitmix64(seed) -> Iterator[int]` — the RNG, pinned to published
  vectors.
- `paired_bootstrap_ci(deltas, seed, resamples, alpha=0.05) ->
  (low, high)` — percentile bootstrap over the paired deltas, resampling
  indices via SplitMix64 exactly as the Rust `bootstrap_ci` does
  (executed-expectations parity per rule 4).
- Pure, no engine, no numpy (D-1 footprint: EARP already avoids adding
  runtime deps).

### `comparison.py` (new module)

- `run_comparison(...)`: per arm — fresh DB, the S6 characterization
  executor reused as the arm executor (`run_characterization`'s
  retrieve/cache/score core, parameterized by the arm's resolved scenario;
  its `retrieve_override` seam is the test seam here too). Then: pair
  per-query rows by `query_id`, compute the paired deltas on
  `comparison.metric`, run `paired_bootstrap_ci`, evaluate
  `underpowered = n < min_n`, apply `decision_rule` if declared.
- Sweep: same arm executor per arm, per-arm metrics and blockers recorded,
  `comparison: null` in the sidecar, no deltas, no CI, no claim.
- A blocked **treatment or control arm** blocks the comparison run (S4
  blocked-run recording; the surviving arm's partial results are recorded
  under its arm name — a one-armed comparison is not a comparison).
- Every per-query row gains its arm: per-query schema gets optional `arm`
  (additive; absent for single-scenario campaigns).

### `writer.py` / result schema

- No result-schema changes for comparison (S0 locked it). The `comparison`
  object is written for comparison campaigns, `null` otherwise — including
  sweeps.
- Per-query schema: optional `arm` property (additive, same no-bump rule
  as S6a/S7).

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

## Out of scope

- Per-stratum CIs (strata are declared and recorded per query; stratified
  inference is a later slice — recording now preserves the option).
- Sparse arm overrides / scenario inheritance.
- Unpaired `percentile_bootstrap` execution (schema-legal, resolver-refused
  for v1 comparisons).
- Any priced arm (S9 owns the D-3 gate).

## Review

Pending independent code-grounded review.
