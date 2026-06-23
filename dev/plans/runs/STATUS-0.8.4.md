# 0.8.4 — running $ ledger + status (GraphRAG-parity resolution)

Resolution: **near-parity-or-better vs Microsoft GraphRAG** on global sensemaking,
S1 (community-summary build) **paired with** G-HH-2 (measured S1-vs-running-GraphRAG
head-to-head); HippoRAG-2 a secondary MuSiQue cross-check. Gate frozen as
`src/python/eval/decision_rule_084.py` (design `dev/design/0.8.4-graphrag-sensemaking.md`).
Budget discipline ([[0.8.1-budget-discipline-cheap-validate-and-ledger]],
[[priced-runs-need-resilience-before-spend]], [[airlock-batch-and-provider-protection]]).

## Budget reality — UPDATED with a $0 corpus-measured projection (2026-06-23)

0.8.3 spent **~$38.16 of the $50 program cap → ~$11.84 remaining.** The Slice-0 §7
guess ("$30–60+") was worst-case; a **$0 corpus-measured cost probe**
(`0.8.4-cost-probe-FINDINGS.md`) overturns it: with a **Haiku/Sonnet cross-family judge**
the realistic powered run is **single-digit-to-low-double-digit dollars** (e.g. one
`vector_rag`-vs-`long_context` pair at 100 q ≈ **$4.51 Haiku / $7.40 Sonnet**; a full
multi-pair ~100-q run ≈ **~$8–12 Haiku / ~$15–20 Sonnet**). **The judge tier is the
dominant cost lever** — an Opus judge (~$60–80 full) is what forces a top-up; Haiku/Sonnet
fit at/near the remaining $11.84. **Required pre-run fix:** pin the chosen Claude judge's
price in `eval/gap_decomposition_run.py::PRICE_PER_1M` (currently unpinned → fails closed).

## $ ledger

| date | slice | item | reader | calls | USD | running total |
|---|---|---|---|---|---|---|
| 2026-06-23 | 0 | design + pre-registration (decision_rule_084, $0 — no LLM) | — | 0 | 0.00 | 0.00 |

## Slice board

| slice | title | state | notes |
|---|---|---|---|
| **0** | Design + pre-register (+ codex §9 + HITL gate) | **SIGNED ✓ (2026-06-23)** | design `decision-ready` + `decision_rule_084.py` + 52-test pin (`45aa2f4f`); **codex §9 PASS** after 2×[P1] pyright fixes (`67079e40`); typecheck exit 0, 52/52. **HITL signed: honest-prior CLEARED (pilot-first); budget top-up to powered run APPROVED (amount set post-pilot).** |
| **5a** | $0 infra: corpus + AutoQ + baselines | **LANDED ✓ (`1eebcc35`)** | AP-News loader (1397 arts, sha256+count guard) + bundled-AutoQ loader (350 q, every bucket, 150 v2 assertions — **no priced synth needed**) + VectorRag/LongContext adapters on the r2 seam. codex §9 0 findings; real-corpus validated; 68/68 tests. Verdict `0.8.4-slice-5-review-VERDICT.md`. **[P2] vector_rag is hashing-BoW placeholder → must become a real semantic embedder before any judged run.** |
| **5b-infra** | AutoE pairwise-judge harness ($0) | **LANDED ✓ (`d909364c`)** | `eval/autoe_judge.py`: pairwise prompt (3 metrics + separate directness), order-swap, ABSENT-safe resume, **question-clustered bootstrap** win-rate → `decide_084` (round-trip tested), bias-control/length assembly, batch-build point (no live submit), `project_autoe_cost`. codex §9 1×[P2] (ABSENT-resume) fixed; 27/27, 95/95 on main. Verdict `0.8.4-slice-5b-review-VERDICT.md`. |
| **5b-runner** | Resilient AutoE pilot runner + LLMJudge ($0) | **LANDED ✓ (`f4e22468`→main)** | `eval/autoe_pilot_run.py`: cross-family LLMJudge, run_pilot orchestration, per-key atomic checkpoint + idempotent resume, `--max-usd` ledger guard, total-spend cost projection, `--cheap-validate`. §9 **fallback** (codex rate-limited) PASS after 2×[P2] (5× under-projection; answerer leg now metered → TOTAL spend) + 1×[P3]. 16/16; 111/111 on main. Verdict `0.8.4-slice-5b-runner-review-VERDICT.md`. |
| **5b-pilot** | Priced cheap-validate cost probe → pilot | **BLOCKED on airlock creds** | $0 runway COMPLETE; cost probe is one command. Needs reader env in-shell: `R2_RUN=1` + `R2_ANSWERER_*` (gpt-5.4) + `R2_JUDGE_*` (cross-family Claude), both providers funded. cheap-validate (tiny N, cents) → `project_autoe_cost` TOTAL → **HITL top-up approval** → bounded pilot (`vector_rag` vs `long_context`). [P2] real vector_rag embedder bites at the pilot *verdict* (cost probe is embedder-agnostic); `strong_baseline_clears(s1_vs_long_context)` runs at Slice-10 start. |
| 10 | S1 build: Leiden + community summaries | BLOCKED on gate | OFFLINE-BUILD, local Qwen3.6-27B ($0); determinism + coverage ACs |
| 15 | Map-reduce QFS reader (KEYSTONE) + running GraphRAG + HippoRAG-2 | BLOCKED on gate | competitor LLMs competitor-side (EVAL-ONLY) |
| 20 | AutoE adjudication + RESOLUTION + surpass-option | BLOCKED on gate | batched; ≥5 runs, order-swap, cross-family judge, length corroboration |

## HITL gate (Slice 0) — SIGNED 2026-06-23 (design §0)

1. **Honest-prior FUND/NO-FUND — CLEARED, PILOT-FIRST.** S1 funded as a staged bet:
   Slice 5 runs the bounded long-context pilot first. If `strong_baseline_clears` is
   **False** (long-context ≈ S1), **settle the publishable third null before funding
   Slice 10+** — don't spend the big-build budget. A third graph null is a valid result.
2. **Budget — TOP-UP to a powered run APPROVED**, exact $ set after the Slice-5 pilot
   measures per-call cost + judge variance (powered = win-rate MDE ≤ 0.05). Pilot spend
   stays small (cheap-validate + bounded pilot).
3. **Pre-registration frozen** in `decision_rule_084.py` (band ε_wr=0.05, ≥5 runs,
   cross-family judge, surpass-option). **Slice 5 is UNBLOCKED.**

_Slices 10/15/20 stay gated behind the Slice-5 pilot return (fund-the-build verdict +
exact powered-run cost)._
