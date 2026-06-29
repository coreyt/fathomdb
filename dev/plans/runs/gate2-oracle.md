# Gate-2 — oracle-routing upper bound (0.8.11 Slice 5) · $0 EVAL-ONLY

> Bounds the maximum value dynamic routing could ever buy (PSD §III.B). Machine-readable:
> `gate2-oracle-output.json` (regenerate at $0:
> `PYTHONPATH=src/python .venv/bin/python -m eval.gate2_oracle_run`).

## Method

No LLM calls. A *fresh* oracle-context decomposition would need the priced gpt-5.4 reader
(`gap_decomposition_run.py`), which is **out of the $0 scope → deferred**. So Gate-2:

1. **Reuses** the already-paid, byte-verified `0.8.3-gap-decomposition-n606.json` for the
   **oracle-CONTEXT** ceiling (perfect-retrieval answer-accuracy lift), and
2. **Computes at $0** the **oracle-ARM-selection** ceiling from existing per-arm recall
   runs (LME `0.8.1-p0a-fused-recall-n160.json`; MuSiQue `0.8.2-m1-verdict-gpt54.json`).

## 1. Oracle-CONTEXT bound — value of perfect retrieval (REUSED, n606)

`acc_oracle_raw − acc_fathomdb` (gold docs in context vs FathomDB's actual retrieval):

| Class (→intent) | Point | 95% CI | n |
| --- | ---: | --- | ---: |
| factoid (→needle) | **+0.372** | [0.295, 0.449] | 156 |
| knowledge_update (→needle) | **+0.530** | [0.435, 0.626] | 115 |
| multi_session | **+0.412** | [0.294, 0.529] | 68 |
| temporal | **+0.247** | [0.165, 0.340] | 97 |
| **pooled** | **+0.392** | **[0.346, 0.436]** | 436 |

## 2. Oracle-ARM-selection bound — value of static-arm switching ($0, this run)

`max_arm − fused_RRF`, class-level (a **lower bound** on the true per-query oracle; per-query
per-arm cells were not persisted in the source runs).

LME recall@10 (n160, 40/class):

| Class | bm25 | fts_only | fused (RRF) | best arm | arm headroom |
| --- | ---: | ---: | ---: | --- | ---: |
| factoid | 0.70 | 0.65 | 0.65 | bm25 | **+0.05** |
| knowledge_update | 0.875 | 0.80 | 0.825 | bm25 | **+0.05** |
| multi_session | 0.275 | 0.20 | 0.325 | **fused** | **0.00** |
| temporal | 0.65 | 0.60 | 0.625 | bm25 | **+0.025** |

MuSiQue multi-hop answer-F1 (≥3-hop pooled, n=144): bm25 0.370, passage_dense **0.487**,
fused 0.450, fused_rerank 0.415, ppr_fusion 0.410 → best = passage_dense, arm headroom over
fused **+0.036 F1** (m1 primary endpoint ppr-vs-fused was a tie, ΔF1 −0.0405 [−0.116, +0.031];
dense, not graph, held the multi-hop signal).

## 3. Per-arm cost tiers (from prior measurements)

| Arm | Tier | Evidence |
| --- | --- | --- |
| `fts_bm25` | low | CPU; p50<1ms / p99 4ms @10k (0.8.0 tokenizer exp) |
| `vector_ann` | low-medium | 1-bit quant + f32 rerank; p50 25 / p99 40 ms (eu7) |
| `rrf` | low | CPU rank-merge; negligible |
| `ce_rerank` | medium | TinyBERT-L-2 1.54ms/pair → 308ms @K=200 (IR-C R0); MiniLM-L12 = high (3364ms) |
| `map_reduce_qfs` | high | LLM tier, reads everything; 0.8.4 C-arm ≥ $21; F4(global)-only, router-isolated |
| `graph_bfs` | low-compute / ~0-value | measured-REFUTED ×2 (ΔF1 −0.0405); default-OFF |

## Reconciliation with +0.392

Pooled RETRIEVAL = **+0.3922 [0.3463, 0.4358]** — **reconciles exactly** with the prior
0.8.3 ledger figure (+0.392 [0.346, 0.436]). It is exact **by construction**: the same n606
artifact is reused (a fresh gpt-5.4 recompute is deferred under $0), so this is consistency,
not an independent re-measurement.

## KILL check — is the ceiling within fused-RRF noise for every class?

- **Oracle-CONTEXT axis: NO KILL.** Per-class ceiling +0.25..+0.53 (pooled +0.392, CI lower
  0.346 ≫ 0) — far outside fused-RRF noise. Large routing-relevant headroom **exists**.
- **Oracle-ARM-selection axis: within noise for every class.** Recall headroom +0.00..+0.05
  (multi_session = 0.00 — fused is already best; multi-hop +0.036 F1), all below the per-class
  recall MDE (~0.11–0.17 at n=40). Static-arm switching alone buys ≈0.

**Conclusion.** The program is **not killed**. The realizable headroom is in recall/precision
**generation** (EXP-A wider candidate generation; EXP-B′ per-intent α/pool_n/candidate_k),
captured by a **config-carrying** router — **not** by switching which static arm runs. Gate-2's
value locus for the L2 router is per-intent config-carrying tuning, not arm routing. This is
consistent with the refuted graph arm and the CE-rerank-is-the-lever findings.

## Deferred

A fresh oracle-context decomposition (independent re-measurement, larger/other corpora,
`global`/`multi_hop` oracle-context cells) needs the priced gpt-5.4 reader → **deferred** (out
of $0 scope). Per-query per-arm recall cells were not persisted in the source runs, so the
arm-selection oracle is class-level (a lower bound); a true per-query oracle would require
re-running the recall harness with per-query arm logging (EXP-A, $0, Slice 10).
