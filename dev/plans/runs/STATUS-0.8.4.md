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
| 2026-06-23 | 5b | $0 directional smokes (premise + GraphRAG-style) | local Qwen | ~190 | 0.00 | 0.00 |
| 2026-06-23 | 5b | cheap-validate claude-haiku route (airlock unblocked via .env) | claude-haiku | 1 | ~0.0001 | ~0.0001 |
| 2026-06-23 | 5b | **CROSS-FAMILY pilot** (Qwen answers $0 + claude-haiku judge, 8q×5runs×2pairs) | claude-haiku | ~160 | ~0.25 | ~0.25 |
| 2026-06-23 | 5b | **POWERED cross-family pilot** (gpt-5.4 answerer + claude-haiku judge, 12q×5runs×2pairs) | gpt-5.4 + claude-haiku | ~370 | ~2.3 | ~2.55 |
| 2026-06-23 | 10 | **community-S1 build** (Qwen reports) + cross-family judge, 60-art/10q | gpt-5.4 + claude-haiku | ~480 | ~3.5 | ~6.0 |
| 2026-06-23 | 10 | **community-S1 STRONG reports** (gpt-5.4 reports) + judge — decisive null | gpt-5.4 + claude-haiku | ~480 | ~3.7 | ~9.7 |
| 2026-06-23 | 15 | **Microsoft GraphRAG 3.1.0** index (15 docs, gpt-5.4) + 8 global-search + cross-family judge | gpt-5.4 + claude-haiku | ~700 | ~4.5 | **~14.2** |
| 2026-06-23 | T1 | **Tier-1 FAIR re-run** (matched budget+k+MMR; nano both sides; 8q×5runs×2pairs judged) | gpt-5-nano + claude-haiku | ~340 | 0.324 | **~14.5** |

## ⭐⭐⭐⭐⭐ TIER-2 PROTOTYPE LANDED (2026-06-23) — C + D2, almost graph-free, $0 sanity-validated

`eval/tier2_coverage.py` (+ `tests/test_tier2_coverage.py` 6/6; run `runs/0.8.4-tier2-prototype.py`).
Built the Tier-2 capability **prototype** (design `0.8.4-closing-graphrag-gap.md` §3), embedder/LLM-
agnostic, engine-independent (numpy k-means, no sklearn): **C** = hierarchical map-reduce QFS reader;
**D2** = depth-1 cluster-summary coverage index (cluster chunk embeddings → LLM-summarize each cluster
→ retrieve over coverage nodes). **$0 sanity run** (15 docs → 21 chunks → 5 clusters; LOCAL Qwen3.6
summaries; BoW embed): D2 + C both produce coherent GraphRAG-report-style multi-theme global answers
(~4.7–4.9k c). **Pipeline proven.** Known prototype limits (fix before the scale MEASUREMENT): BoW
embedder isolates a degenerate cluster (cov-2 = a lone "Editorial Roundup" title → empty summary) →
**needs a real semantic embedder** (standing Slice-5 [P2]); and the decisive test is **at SCALE** (100s–
1000s docs) vs a **scaled GraphRAG index**, powered + registered. NEXT (HITL): wire real embedder →
re-index GraphRAG at scale (nano) → powered registered **C vs D2 vs GraphRAG** run.

## ⭐⭐⭐⭐ TIER-1 FAIR RE-RUN (2026-06-23) — the "GraphRAG WINS" result was LARGELY A MEASUREMENT ARTIFACT

`runs/0.8.4-tier1-fair-rerun-RESULT.md`. Re-ran the SAME head-to-head on the SAME preserved Microsoft
GraphRAG index with the three **Tier-1 fairness levers** (design `0.8.4-closing-graphrag-gap.md` §2):
matched generation budget (reader `max_tokens` 600→**1500**), raised k (8→**15**=full coverage), **MMR**
diversification. Per HITL: **gpt-5-nano on BOTH sides** (FathomDB reader + GraphRAG global-search query
LLM, equivalent gpt use), claude-haiku judge. **RESULT FLIPS:** `fathomdb_mapreduce` vs GraphRAG —
comprehensiveness **0.062→0.812** [0.562,1.0], diversity 0.319→**0.875** [0.625,1.0] (both **surpass
candidates**, ci_lo>0.5); `fathomdb_vector` comp **0.000→0.525**. `decide_084`=NOT_REACHED but binding =
**underpowered** (n=8, mde≈0.22), NOT below-parity. **Spend $0.324** (nano ≈ free; judge dominates).
**The decisive 0.00 was the 600-token cap + top-8, not a structural sensemaking deficit — at 15-doc
scale.** Caveats: underpowered; both-nano (GraphRAG also dropped from gpt-5.4 synthesis → retains only
its gpt-5.4 index); **15 docs is GraphRAG's weakest case** (its community advantage is a LARGE-corpus
phenomenon); vector-arm short-answer noise. **Fork C ("record the GraphRAG win") WITHDRAWN — would have
recorded an artifact.** Live question is now **SCALE** (does fair FathomDB hold at 100s–1000s docs?).
Next: gpt-5.4-on-both confirmation (~$2–4) + scale-up powered registered run. Ledger → **~$14.5**.

## ⭐⭐⭐ DEFINITIVE (2026-06-23) — LITERAL FathomDB vs RUNNING Microsoft GraphRAG: GraphRAG WINS [SUPERSEDED by the Tier-1 fair re-run above]

`runs/0.8.4-vs-microsoft-graphrag-RESULT.md`. Stood up an **actual Microsoft GraphRAG 3.1.0** (real
pipeline: entity/relationship/claim extraction → **Leiden hierarchical communities** → **115 LLM
community reports** → relevance-scored global map-reduce; gpt-5.4 via airlock, embeddings via a local
shim) over 15 AP-News articles, and ran its **global-search** head-to-head vs FathomDB over the SAME 15
docs, cross-family judged. **VERDICT: Microsoft GraphRAG WINS decisively — FathomDB win-rate vs GraphRAG
= 0.00–0.33** (comprehensiveness 0.06 mapreduce / **0.00** vector), well below the 0.45 near-parity band.
`decide_084`=NOT_REACHED. **FathomDB does NOT reach parity-or-better vs GraphRAG on sensemaking.**

**This OVERTURNS the provisional "community paradigm = graph null" lean below** — that was based on a
CRUDE community-S1 reimplementation (label-prop/60-doc/simple reports) that under-represented real
GraphRAG. The actual Microsoft GraphRAG beats even FathomDB's raw-text map-reduce → **the
community-summary paradigm DOES pay off when implemented well.** CORRECTED decision: the (large) gap
**JUSTIFIES funding a real Microsoft-grade S1 build** — OR record the measured GraphRAG win as the
publishable outcome (roadmap-sanctioned). The "don't fund S1" lean is **WITHDRAWN.** Caveats: 15-doc
index, n=8 (underpowered but comprehensiveness≈0 is decisive), length-bias possible, sensemaking axis
only (HippoRAG-2 multi-hop unrun). Session airlock spend ~$14–15.

## ⭐⭐ (SUPERSEDED) PROVISIONAL RESOLUTION (2026-06-23) — the GraphRAG community paradigm = the graph null

`runs/0.8.4-graphrag-RESOLUTION.md`. After driving a coherent set of **cross-family** (claude-haiku
judge, gpt-5.4 reader, ≥5-run, order-swapped) pilots, the 0.8.4 premise resolves **provisionally
NEGATIVE for the community-summary paradigm** (underpowered/subset/not-Microsoft-package, so
decision-grade not registered-REACHED). Win-rate vs vector_rag (comprehensiveness): **flat map-reduce
over RAW text 0.83 (wins)** · community-S1 + Qwen reports 0.39 (loses) · community-S1 + **gpt-5.4** reports
0.32 (**still loses**). The community-summary structure is a **lossy compression** that does NOT pay off
even with strong reports; the measured value is the **strong reader synthesizing over RAW TEXT**, not the
graph machinery (M1/M2 + Samsung-prior consistent). **RECOMMENDATION: do NOT fund the full S1
community-summary build; ship the cheaper winner (BYO-LLM strong-reader map-reduce over FathomDB's raw
retrieved text).** Two control lessons en route: same-family judge inflates (self-preference, false +0.75);
reader quality is the dominant lever (flipped the same arm win↔loss). Session airlock spend ~$10.

## ⭐ Headline measurement (2026-06-23) — TWO overturns; registered-config premise is POSITIVE (superseded by the RESOLUTION above)

**Airlock unblocked** (HITL-authorized `~/projects/airlock/.env`; exposes gpt-5.4 + claude-haiku/
sonnet/opus). Ran cross-family (claude-haiku judge), ≥5-run, order-swapped measurements through
`decide_084`. Two bias-control lessons, both characterized:

1. **Same-family judge inflated** (`0.8.4-xfamily-pilot-RESULT.md`): a GraphRAG-style map-reduce arm
   that "won" 0.750 under a SAME-family Qwen judge **lost** (0.25–0.44) under the cross-family Claude
   judge → the 0.750 was **self-preference bias.** All $0 Qwen-judged smokes are suspect.
2. **Weak answerer suppressed** (`0.8.4-xfamily-pilot-powered-RESULT.md`): with the **Qwen answerer**
   the GraphRAG-style arm lost (0.25–0.44); swapping to the design's **real reader gpt-5.4** it **WINS**
   — vs long_context **0.72/0.55/0.57**, vs vector_rag **0.83/0.68/0.63** (comp/div/emp);
   comprehensiveness-vs-vector_rag is a **SURPASS** candidate (ci_lo 0.617 > 0.5). map-reduce QFS
   depends on synthesis quality → Qwen suppressed it, gpt-5.4 realizes it (the GraphRAG thesis).

**`decide_084` = NOT_REACHED on both, but the powered run is blocked by POWER (mde 0.19–0.25 > ε),
NOT below-parity — all win-rates ≥0.5.** **Decision lean (registered config: strong reader +
cross-family judge): premise POSITIVELY supported, trending to SURPASS → power up (more q → mde≤0.05)
→ likely REACHES → JUSTIFIES funding the S1 build.** Caveats: minimal subset map-reduce arm (not
Microsoft GraphRAG), underpowered, premise-not-full-resolution. Spend to date ~$2.5.

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
