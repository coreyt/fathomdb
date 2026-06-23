# 0.8.4 — running $ ledger + status (GraphRAG-parity resolution)

Resolution: **near-parity-or-better vs Microsoft GraphRAG** on global sensemaking,
S1 (community-summary build) **paired with** G-HH-2 (measured S1-vs-running-GraphRAG
head-to-head); HippoRAG-2 a secondary MuSiQue cross-check. Gate frozen as
`src/python/eval/decision_rule_084.py` (design `dev/design/0.8.4-graphrag-sensemaking.md`).
Budget discipline ([[0.8.1-budget-discipline-cheap-validate-and-ledger]],
[[priced-runs-need-resilience-before-spend]], [[airlock-batch-and-provider-protection]]).

## Carry-in budget reality (HARD constraint — TOP-UP likely needed)

0.8.3 spent **~$38.16 of the $50 program cap → ~$11.84 remaining.** 0.8.4 is the
**most expensive version** (running GraphRAG index + AutoQ synth + AutoE pairwise ×
≥5 runs × order-swap × 3 comparisons × 3 metrics). Even batched (~50% off), the
realistic spend is **plausibly $30–60+**. A **budget top-up is a Slice-0 HITL
decision** — without it, only a reduced-scope (likely under-powered) pilot is fundable.
The Slice-5 pilot produces the exact per-call cost so the HITL approves a real number.

## $ ledger

| date | slice | item | reader | calls | USD | running total |
|---|---|---|---|---|---|---|
| 2026-06-23 | 0 | design + pre-registration (decision_rule_084, $0 — no LLM) | — | 0 | 0.00 | 0.00 |

## Slice board

| slice | title | state | notes |
|---|---|---|---|
| **0** | Design + pre-register (+ codex §9 + HITL gate) | **SIGNED ✓ (2026-06-23)** | design `decision-ready` + `decision_rule_084.py` + 52-test pin (`45aa2f4f`); **codex §9 PASS** after 2×[P1] pyright fixes (`67079e40`); typecheck exit 0, 52/52. **HITL signed: honest-prior CLEARED (pilot-first); budget top-up to powered run APPROVED (amount set post-pilot).** |
| **5** | Corpus + baselines + AutoQ + pilot | **NEXT (unblocked)** | AP-News EVAL-ONLY; vector-RAG + long-context; AutoQ batched (cheap-validate first); bounded **long-context pilot** → `strong_baseline_clears` = the FUND-the-big-build hinge. Returns to HITL with the verdict + measured powered-run cost. |
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
