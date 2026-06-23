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
| **0** | Design + pre-register (+ codex §9 + HITL gate) | **IN REVIEW** | design `decision-ready` + `decision_rule_084.py` + 52-test pin committed `45aa2f4f`; codex §9 running (`0.8.4-slice-0-review-codex.log`); then HITL honest-prior + budget gate |
| 5 | Corpus + baselines + AutoQ + pilot | BLOCKED on Slice-0 gate | AP-News EVAL-ONLY; vector-RAG + long-context; AutoQ batched; pilot → `strong_baseline_clears` kill-early |
| 10 | S1 build: Leiden + community summaries | BLOCKED on gate | OFFLINE-BUILD, local Qwen3.6-27B ($0); determinism + coverage ACs |
| 15 | Map-reduce QFS reader (KEYSTONE) + running GraphRAG + HippoRAG-2 | BLOCKED on gate | competitor LLMs competitor-side (EVAL-ONLY) |
| 20 | AutoE adjudication + RESOLUTION + surpass-option | BLOCKED on gate | batched; ≥5 runs, order-swap, cross-family judge, length corroboration |

## HITL gate (Slice 0) — decision package pending

1. **Honest-prior FUND/NO-FUND** (design §2): the cross-graph prior is strongly
   negative (M1 NO-GO, M2 dropped). S1 = community-summary *synthesis* (different
   mechanism + axis than refuted PPR/BFS traversal) — fund only if the §2 checklist
   (`honest_prior_cleared`) clears codex + HITL. A **third graph null is publishable**.
2. **Budget top-up** (above) — approve a number or accept a reduced/under-powered scope.
3. **Sign the frozen pre-registration** (band ε_wr=0.05, ≥5 runs, cross-family judge,
   surpass-option protocol) → unblocks Slice 5.

_No build slice (10/15) or judged run (5 pilot / 20 AutoE) runs until signed._
