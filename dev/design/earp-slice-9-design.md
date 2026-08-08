---
status: COMPLETE
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
identical-answerer, the only arm with an in-repo **answerer** protocol in
standing use (`r2_parity_eval.py`; its `LLMAnswerer` gates on `R2_RUN=1`
and reads `R2_ANSWERER_BASE_URL` / `R2_ANSWERER_API_KEY` /
`R2_ANSWERER_MODEL` — review-verified; `R2_JUDGE_*` is the AutoE judge's
convention in a different module and is NOT read by this protocol; the
gitignored `dev/.env.eval` needs the `R2_ANSWERER_*` entries added).
Scoring reuses the protocol's own `PerClassScorer.score_answer` —
deterministic string matching, $0, no LLM judge. A dormant local-OSS
`Mem0OSSAdapter` also exists in that file; its commissioning — like the
extractor and GPU arms — stays an HITL scope decision, and all three are
catalog entries classified `UNSUPPORTED` with that reason, so their
absence is a recorded refusal, not silence. Building three more network
adapters inside this slice would dilute the thing S9 must get right: the
money gate.

## Contract

A priced arm runs only when **all four** of these hold, in order:

1. **Declared:** the config carries the arm (`scenario.answer_arm`, below)
   with a **required `max_queries`** and a `budget.estimated_usd` — a
   priced arm without a declared worst case is a collected config error
   (`CONFIG_MISSING_KEY`), and the preflight **cross-checks the
   declaration**: `adapter.estimate(min(max_queries, n_queries)) >
   estimated_usd` is a typed refusal. The declared worst case must
   dominate the computed one; an author cannot buy past the gate with an
   invented small number.
2. **Opted in at run time:** `FDB_EARP_PRICED=1` in the environment AND
   the adapter's own credentials present (`R2_RUN=1` +
   `R2_ANSWERER_BASE_URL` + `R2_ANSWERER_MODEL`, i.e. the wrapped
   protocol's own `available` **property**). Absent either → the arm
   records a **visible skip** (typed outcome `skipped`, reason naming the
   missing gate) and the run completes unpriced. Never a pass, never a
   zero (D-2). (`FDB_EARP_PRICED` follows the house env-gate pattern S7
   established with `FDB_EARP_INTEGRATION`.)
3. **Under budget — one authoritative ledger:** D-3's ceiling is
   cumulative across ALL priced EARP runs, so the summed ledger cannot be
   whatever `experiments_root` a run happens to use (worktrees start with
   a git-tracked **empty** `index.jsonl` — a per-run root would reset the
   ceiling per checkout). A priced run requires the authoritative ledger:
   `FDB_EARP_LEDGER_ROOT` must be set and the run's `experiments_root`
   must equal it, else a typed refusal (stage `priced.ledger`). The
   preflight sums `cost_usd` across that root's `index.jsonl`, adds
   `estimated_usd`, and refuses with `BUDGET_EXCEEDED` (S4 blocked-run
   recording) when the projection exceeds `authorized_usd` = **$5.00**
   (module constant `D3_AUTHORIZED_USD`; raising it is an HITL act done
   as a reviewed code edit — no config or env input can raise it, and
   the sidecar's `authorized_usd` is always written from the constant).
   The blocker `detail` carries the full projection arithmetic AND the
   ledger path summed, so the refusal audits from the sidecar alone.
   Tmp roots remain legal for stub tests (which never pass gate 2).
4. **Cheap-validated:** the adapter's cheap-validation step must have run
   and its witness recorded **in the same run** before the first priced
   call; absent → typed refusal (stage `priced.preflight`). **Owned
   openly:** repo precedent reads "cheap-validation" as a cheap *priced*
   model pass (gemini-flash-lite); S9 deliberately reinterprets it as a
   $0 stub/recorded-fixture pass — stricter in dollars, weaker in
   endpoint coverage — and the first real priced call remains the
   endpoint validation, bounded by the per-call guard below.

**The runtime meter (closing the estimate loophole):** pricing is
**fail-closed and pinned** — a `PRICE_PER_1M`-style table with a
`price_for` lookup that refuses an unpinned model as a typed blocker,
never a default (the `gap_decomposition_run.py:94,165-176` precedent:
"a $-cap is unenforceable without pinned pricing"). Usage is captured
from the completion response's `usage` body (the current `LLMAnswerer.
_complete` discards it, so the adapter wraps the completion to capture
it); when the endpoint reports no usage, cost falls back to a declared
chars/4 token estimate and the sidecar marks the figure
`estimated-not-metered`. Every call passes a **pre-call guard**: if
`spent_so_far + next_call_worst_case > authorized_usd −
cumulative_spent_usd`, the arm halts with a typed blocker and partial
results are recorded (the `BudgetLedger.guard` precedent).

Cost is recorded three places, consistently: the sidecar `cost` block
(actual filled on completion), `Record.cost_usd`, and the index row —
which is what makes rule 3's ledger self-feeding.

## Changes, by file

### `schema/earp.config.v1.schema.json` — additive

`scenario` gains optional `answer_arm`:

```json
"answer_arm": {
  "type": "object", "additionalProperties": false,
  "required": ["kind", "max_queries"],
  "properties": {
    "kind": { "enum": ["r2_identical_answerer"] },
    "answerer_model": { "type": "string" },
    "max_queries": { "type": "integer", "minimum": 1 }
  }
}
```

`max_queries` is **required** — it bounds the priced call count and the
estimate multiplies by it (gate 1's cross-check has no meaning without
it). `answerer_model`: **explicit in config whenever a `decision_rule` or
comparison consumes `answer_accuracy`** (config identity — `config_sha256`
covers the document, so an env-defaulted model would let two different-
model runs share a sha and poison S8 pairing/replay); the
`R2_ANSWERER_MODEL` env default is permitted only for claim-free
characterizations, and the sidecar records the resolved value marked
`env-resolved`.

### `config.py`

- Registry: `scenario.answer_arm` (+ walker-yielded object node children
  per the S7/S8 lesson — enumerate with `declared_paths` at implementation
  time) → `Consumer("S9")`.
- Collected rules: `answer_arm` present ⇒ `budget` present; explicit
  `answerer_model` required when any claim consumes `answer_accuracy`.
- **`answer_accuracy` is arm-implied, never config-requested** (no
  metrics-block schema edit): it lands in the result's
  `metrics.document_metrics` map, whose free-keyed shape validates it
  today (review-executed). Eligibility mechanics: `emits()` gains a
  `has_answer_arm` input and the `k_free` branch dispatches per metric —
  `answer_accuracy` keys on the arm, NOT on `has_negatives` (the existing
  k_free coupling belongs to `abstention_rate` alone). Requesting it via
  `decision_rule.metric` without the arm stays `CONFIG_INVALID_VALUE`
  (verified today's baseline behaviour).
- `ResolvedScenario` gains `answer_arm: ResolvedAnswerArm | None`.

### `pricing.py` (new module)

- `read_cumulative_spend(experiments_root) -> float` — sums `cost_usd`
  over `index.jsonl`, tolerating rows without the field (0.0), refusing a
  malformed ledger as a typed blocker (`stage: priced.ledger`) rather than
  treating it as $0.
- `authoritative_root() -> Path | Blocker` — reads `FDB_EARP_LEDGER_ROOT`;
  unset, or unequal to the run's `experiments_root`, is the typed refusal
  from gate 3.
- `PRICE_PER_1M` + `price_for(model) -> (in_rate, out_rate)` — fail-closed
  (unpinned model → typed blocker, never a default).
- `preflight(ledger: CostLedger, computed_estimate: float) ->
  Blocker | None` — pure; refuses both the over-budget projection AND the
  under-declared estimate (gate 1 cross-check); `detail` carries the full
  arithmetic and the ledger path.
- `CallGuard` — the per-call meter: worst-case next call vs remaining
  authorization; halt is a typed blocker with partials recorded.
- `D3_AUTHORIZED_USD = 5.00` with the D-3 citation; the sidecar's
  `authorized_usd` is written from this constant only.

### `answer_arm.py` (new module)

- Adapter protocol: `estimate(n_queries) -> float`,
  `cheap_validate(queries) -> Witness`, `run(queries) -> per-query answer
  outcomes + actual_usd`.
- `R2IdenticalAnswerer`: wraps the `r2_parity_eval.py` answerer protocol
  (same prompt build, same `available` property, same models); credentials
  `R2_RUN=1` + `R2_ANSWERER_*`; the completion call is wrapped to capture
  the response `usage` body (the protocol's own `_complete` discards it);
  cost = pinned `price_for` rates × usage, chars/4 fallback marked
  `estimated-not-metered`; every call passes the `CallGuard`.
- `StubAnswerer` (test-only, from `r2_parity_eval`'s deterministic stub):
  drives every gate test at $0 — including a fake nonzero "cost" path so
  budget arithmetic is testable without any network call.
- Scoring: the protocol's `PerClassScorer.score_answer` (deterministic,
  $0).
- Per-query answer outcomes land in the per-query jsonl with **named**
  additive fields (`answer_outcome`, `answer_text_sha`, `answer_reason`)
  and the pinned convention that answer rows carry `k: null` — the
  per-query schema's `k` becomes nullable for answer rows (additive-
  nullable, same no-bump rule; AC-2's testability depends on these being
  named now, not at implementation time).

### `runner.py` / `comparison.py`

The arm executes **after** retrieval scoring (it consumes the retrieved
contexts), in whichever campaign declared it. Skip/blocked outcomes flow
through the existing witness + blocker machinery; the `cost` block is
written on every run that declared a budget (estimate always; actual on
completion; `cumulative_spent_usd` as read at preflight).

**Result-schema additive edits (enumerated now — the locked blocks are all
`additionalProperties: false`):** the `$defs/witness.source` enum gains
`"answer_arm"` (carries the visible-skip witness, the cheap-validate
witness, and the ledger-preflight witness — one source, three named
witnesses); the per-query schema gains the three named `answer*` fields
and nullable `k` (above). Nothing else; the `cost` block fits as locked
(review-executed against the validator).

### `cli.py`

`earp validate` prints the declared estimate, the current cumulative spend,
and the projected total for any config carrying `answer_arm`.

## Acceptance criteria

1. A priced-arm config without `budget` or without `max_queries` is a
   collected config error; with both, `earp validate` shows
   estimate/cumulative/projection.
2. With `FDB_EARP_PRICED` unset or credentials absent, the run completes
   with the arm visibly `skipped` (an `answer_arm`-source witness whose
   reason names the missing gate) — never a zero, never a pass, and no
   network call is attempted (asserted BOTH via an instrumented stub that
   fails if invoked AND a monkeypatched `urllib.request.urlopen` that
   raises — defense in depth, since the wrapped protocol's own network
   gate is one refactor away from silent).
3. Preflight refusal, both directions: (a) a synthetic ledger whose
   `cost_usd` sum plus the estimate exceeds $5.00 → BLOCKED with
   `BUDGET_EXCEEDED`, projection arithmetic and ledger path in `detail`,
   durably indexed (S4); (b) a declared `estimated_usd` smaller than
   `adapter.estimate(min(max_queries, n_queries))` → typed refusal (the
   estimate loophole is closed).
4. A malformed ledger is a typed blocker, not $0; a priced run whose
   `experiments_root` is not the `FDB_EARP_LEDGER_ROOT` authoritative
   root is a typed refusal; an unpinned model in `price_for` is a typed
   refusal; the per-call `CallGuard` halts with partials recorded when
   the next worst-case call would exceed remaining authorization.
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

Independent code-grounded executing review, 2026-08-07. Verdict: **PROCEED
WITH REVISIONS**. The gate skeleton and S0-locked surfaces verified exact
(validator-executed); four P1s — all in the money gate — and six further
findings corrected in this revision:

| # | Sev | Finding | Resolution |
| ---: | --- | --- | --- |
| 1 | P1 | Per-run `experiments_root` resets the D-3 ceiling per checkout (worktrees carry a git-tracked EMPTY index.jsonl) | `FDB_EARP_LEDGER_ROOT` authoritative-root gate; refusal otherwise; ledger path in detail |
| 2 | P1 | Wrong env convention: the answerer protocol reads `R2_RUN`+`R2_ANSWERER_*`, never `R2_JUDGE_*` (that's AutoE's judge); scorer is `PerClassScorer`, $0, no LLM judge | Convention corrected everywhere; `judge_model`→`answerer_model`; `.env.eval` addition named |
| 3 | P1 | "Published token pricing" was an invented mechanism; `_complete` discards `usage` | Fail-closed `price_for`/`PRICE_PER_1M` (in-repo precedent); usage-capturing wrapper; `estimated-not-metered` fallback |
| 4 | P1 | Nothing bound ACTUAL spend to the declared estimate | `max_queries` required; preflight cross-check; per-call `CallGuard` (BudgetLedger precedent) |
| 5 | P2 | Visible skip / cheap-validate witnesses had no sidecar slot; per-query k≥1 blocks answer rows | `witness.source` gains `answer_arm`; named `answer*` fields + nullable `k` enumerated now |
| 6 | P2 | `answer_accuracy` eligibility would couple to `has_negatives`; no config slot exists | Arm-implied metric; `emits()` gains `has_answer_arm`; lands in `document_metrics` (validates today) |
| 7 | P2 | Env-defaulted model breaks config identity for claims | Explicit-in-config when any claim consumes the metric; env default only for claim-free runs, marked |
| 8 | P3 | Cheap-validation reading diverges from repo precedent silently | Owned openly in gate 4 |
| 9 | P3 | "Only in-repo protocol" overstated (dormant Mem0OSSAdapter exists) | Reworded; Mem0 commissioning stays HITL |
| 10 | P3 | No-network AC guarded only the stub layer | urlopen monkeypatch added to AC-2 |
