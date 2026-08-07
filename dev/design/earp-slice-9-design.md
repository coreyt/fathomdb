---
status: PROPOSED
---

# EARP Slice 9 — opt-in priced arms behind the D-3 cumulative budget gate

Design of record for S9 of `dev/plans/earp-foundation.md` ("Spend is bounded
and visible"). Depends on S8 (arms, comparison machinery). Plan requirement
row: every priced arm opt-in and visibly skipped by default; projected
cumulative spend over the D-3 authorization refused as `budget_exceeded`;
cost recorded per run. Governing rulings: D-3 ($5.00 cumulative,
**enforced**, cheap-validation precedes priced execution), D-2 (opt-in +
visible SKIP; a skipped arm is never a pass or a zero).

## What S0 already locked (verified)

- Config: `budget.estimated_usd` (required-when-present, minimum 0).
- Models: `CostLedger(authorized_usd, cumulative_spent_usd, estimated_usd,
  actual_usd)`; `BlockerCode.BUDGET_EXCEEDED`.
- Result schema: `cost` block mirroring `CostLedger` exactly.
- The durable spend ledger already exists outside EARP:
  `experiments/index.jsonl` rows carry `cost_usd` (`_lib.py:103,481`);
  the writer already appends the index line last (S4).

## Scope decision (owned openly)

The plan names four priced arms (R2, Mem0, extractor, GPU). S9 delivers the
**enforcement machinery plus exactly one real adapter** — the R2
identical-answerer, the only one with an in-repo protocol
(`r2_parity_eval.py`) and a standing env convention (`R2_JUDGE_*` via
gitignored `dev/.env.eval`). Mem0 / extractor / GPU become catalog entries
classified `UNSUPPORTED` with reason "adapter deferred — D-3 machinery
ready; commissioning is an HITL scope decision", so their absence is a
recorded refusal, not silence. Building three more network adapters inside
this slice would dilute the thing S9 must get right: the money gate.

## Contract

A priced arm runs only when **all four** of these hold, in order:

1. **Declared:** the config carries the arm (`scenario.answer_arm`, below)
   and a `budget.estimated_usd` — a priced arm without a declared worst
   case is a collected config error (`CONFIG_MISSING_KEY`).
2. **Opted in at run time:** `FDB_EARP_PRICED=1` in the environment AND the
   adapter's own credentials present (`R2_JUDGE_*`). Absent either → the
   arm records a **visible skip** (typed outcome `skipped`, reason naming
   the missing gate) and the run completes unpriced. Never a pass, never a
   zero (D-2).
3. **Under budget:** the preflight sums `cost_usd` across
   `experiments/index.jsonl` at the run's `experiments_root`, adds
   `estimated_usd`, and refuses with typed blocker `BUDGET_EXCEEDED`
   (blocked-run recording per S4 — durable, indexed) when the projection
   exceeds `authorized_usd` = **$5.00** (a module constant named
   `D3_AUTHORIZED_USD`; changing it is an HITL act, and the preflight
   reads the ledger fresh on every run — cumulative across all priced
   EARP runs, per D-3).
4. **Cheap-validated:** the adapter's cheap-validation step (stub or
   recorded fixture pass over the same query set, $0) must have run and
   its witness recorded **in the same run** before the first priced call.
   No cheap-validate witness → the priced call is refused (typed blocker,
   stage `priced.preflight`).

Cost is recorded three places, consistently: the sidecar `cost` block
(actual filled on completion), `Record.cost_usd`, and the index row —
which is what makes rule 3's ledger self-feeding.

## Changes, by file

### `schema/earp.config.v1.schema.json` — additive

`scenario` gains optional `answer_arm`:

```json
"answer_arm": {
  "type": "object", "additionalProperties": false,
  "required": ["kind"],
  "properties": {
    "kind": { "enum": ["r2_identical_answerer"] },
    "judge_model": { "type": "string" },
    "max_queries": { "type": "integer", "minimum": 1 }
  }
}
```

`max_queries` bounds the priced call count (the estimate multiplies by it);
`judge_model` defaults to the `R2_JUDGE_MODEL` env value at run time and is
recorded resolved in the sidecar.

### `config.py`

- Registry: `scenario.answer_arm` (+ walker-yielded object node children
  per the S7/S8 lesson — enumerate with `declared_paths` at implementation
  time) → `Consumer("S9")`.
- Collected rules: `answer_arm` present ⇒ `budget` present;
  `answer_arm` present ⇒ the campaign's metrics may include
  `answer_accuracy` (a new `METRIC_NAMES` entry, `k_free`, emitting only
  from the priced arm); `answer_accuracy` requested without `answer_arm` ⇒
  ineligible (existing metric-eligibility path).
- `ResolvedScenario` gains `answer_arm: ResolvedAnswerArm | None`.

### `pricing.py` (new module)

- `read_cumulative_spend(experiments_root) -> float` — sums `cost_usd`
  over `index.jsonl`, tolerating rows without the field (0.0), refusing a
  malformed ledger as a typed blocker (`stage: priced.ledger`) rather than
  treating it as $0.
- `preflight(ledger: CostLedger) -> Blocker | None` — pure; the
  `BUDGET_EXCEEDED` blocker carries the full projection arithmetic in
  `detail` so the refusal is auditable from the sidecar alone.
- `D3_AUTHORIZED_USD = 5.00` with the D-3 citation.

### `answer_arm.py` (new module)

- Adapter protocol: `estimate(n_queries) -> float`,
  `cheap_validate(queries) -> Witness`, `run(queries) -> per-query answer
  outcomes + actual_usd`.
- `R2IdenticalAnswerer`: wraps the `r2_parity_eval.py` answerer protocol
  (same prompt build, same models); credentials from `R2_JUDGE_*`;
  per-call cost from the model's published token pricing, worst-case at
  estimate time, metered actual at run time.
- `StubAnswerer` (test-only, from `r2_parity_eval`'s deterministic stub):
  drives every gate test at $0 — including a fake nonzero "cost" path so
  budget arithmetic is testable without any network call.
- Answer accuracy scoring reuses the R2 protocol's judgment shape;
  per-query outcomes land in the per-query jsonl (additive optional
  `answer` fields if needed — enumerate against the schema at
  implementation time, same no-bump rule).

### `runner.py` / `comparison.py`

The arm executes **after** retrieval scoring (it consumes the retrieved
contexts), in whichever campaign declared it. Skip/blocked outcomes flow
through the existing witness + blocker machinery; the `cost` block is
written on every run that declared a budget (estimate always; actual on
completion; `cumulative_spent_usd` as read at preflight).

### `cli.py`

`earp validate` prints the declared estimate, the current cumulative spend,
and the projected total for any config carrying `answer_arm`.

## Acceptance criteria

1. A priced-arm config without `budget` is a collected config error; with
   budget, `earp validate` shows estimate/cumulative/projection.
2. With `FDB_EARP_PRICED` unset or credentials absent, the run completes
   with the arm visibly `skipped` (reason names the missing gate) — never
   a zero, never a pass, and no network call is attempted (asserted via a
   stub that fails the test if invoked).
3. Preflight refusal: with a synthetic `index.jsonl` whose `cost_usd` sum
   plus the estimate exceeds $5.00, the run is BLOCKED with
   `BUDGET_EXCEEDED` and the projection arithmetic in `detail`; the
   blocked run is durably indexed (S4 semantics).
4. A malformed ledger is a typed blocker, not $0.
5. Cheap-validation precedence: a priced execution without a recorded
   cheap-validate witness in the same run is refused; with it, the priced
   path proceeds (stub-driven).
6. Cost lands in all three places (sidecar `cost`, `Record.cost_usd`,
   index row) with `actual_usd` on completion; a re-run's preflight sees
   the prior run's actual in `cumulative_spent_usd`.
7. `answer_accuracy` without `answer_arm` is ineligible; with the arm it
   emits only from the arm's outcomes.
8. Deferred adapters (Mem0, extractor, GPU) are catalog-refused with
   reasons.
9. `ruff` clean; pyright 0 new / 0 in touched files; full suite green.
10. **No test in the default or integration suite makes a priced network
    call.** The real R2 path is exercised only by explicit HITL-run
    campaigns; its adapter is covered by the stub contract tests.

## Test-first sequence (RED before GREEN)

1. Preflight arithmetic: under / at / over the ceiling; malformed ledger;
   missing-field tolerance.
2. Gate ordering: declared→opted-in→budget→cheap-validate, each failing
   stage producing its distinct typed outcome, later stages unreached
   (stub instrumented to detect any call).
3. Resolver: answer_arm/budget coupling; `answer_accuracy` eligibility
   both directions.
4. End-to-end stub run: skip case, blocked case, complete case with fake
   cost — all three sidecar/index dispositions.
5. Ledger self-feeding: two sequential stub runs, second preflight reads
   the first's actual.

## Out of scope

- Mem0, extractor, and GPU adapters (catalog-refused; commissioning is an
  HITL decision).
- Any change to the $5.00 figure (HITL-owned constant).
- Answer-accuracy gold beyond what the R2 protocol already defines.

## Review

Pending independent code-grounded review.
