# V-1 results — keystone EXP-B′ re-run on the LIVE cross-encoder engine

> **Status: LANDED (live-CE measured, $0/local).** This discharges the `PROVISIONAL, V-1-blocking` caveat the 0.8.11 handoff put on every EXP-B′ tuple (those were derived from a feature-OFF / 0.8.3-fallback CE pass). All tuples below are measured on a `default-reranker` build with the live TinyBERT-L-2 CE active (`ce_norm_is_active=True`, max ce_norm ≈ 0.999). Per-corpus, never pooled.

- grid: candidate_k [200, 300, 500] × pool_n [10, 20, 50, 100, 200] × alpha [0.0, 0.3, 0.5, 0.7, 1.0] × final_K=10
- bootstrap: 2000× seed 0xb5; recall_ks [10, 50, 100, 200, 300, 500]
- CE-pass note: CE scored to depth 200 (= max pool_n); ranks 201-500 keep base order and are never CE-read → **exact** for the registered grid, ~60% cheaper.
- LME intents measured: ['multi_session', 'needle', 'temporal']

## 1. Per-intent config tuples (live-CE measured) — the keystone artifact

| intent | corpus | candidate_k | pool_n | alpha | r@10 [95% CI] | MRR | nDCG@10 [CI] | n | 0.8.3 prior | re-confirm? |
|---|---|---|---|---|---|---|---|---|---|---|
| needle | LME | 200 | 200 | 0.7 | 0.6405 [0.5850,0.6928] | 0.486176 | 0.5117 [0.4745,0.5488] | 306 | 200/50/0.7 | REVISED (was 200/50/0.7) |
| multi_session | LME | 300 | 200 | 1.0 | 0.4733 [0.3933,0.5533] | 0.673661 | 0.5924 [0.5342,0.6496] | 150 | 300/100/1.0 | REVISED (was 300/100/1.0) |
| temporal | LME | 500 | 50 | 1.0 | 0.5000 [0.4200,0.5800] | 0.575783 | 0.5371 [0.4802,0.5918] | 150 | 500/20/1.0 | REVISED (was 500/20/1.0) |
| multi_hop | musique | 200\* | 20 | 0.3 | 0.5350 [0.5151,0.5548] | 0.870141 | 0.7203 [0.7116,0.7291] | 2417 | EXP-0 pin (provisional) | MEASURED (new) |
| multi_hop | hotpotqa | 200\* | 10 | 0.3 | 1.0000 [1.0000,1.0000] | 0.94744 | 0.9023 [0.8994,0.9050] | 7405 | EXP-0 pin (provisional) | MEASURED (new) |

\* multi_hop corpora are per-question ~10-20 paragraph distractor pools, so `candidate_k` is not a meaningful recall axis (the whole pool is < any candidate_k); the live axes are `pool_n` × `alpha`. candidate_k is reported as the envelope-saturation pick.

**Re-confirmation pattern (live CE vs 0.8.3 fallback).** The two dominant knobs **re-confirm exactly** on live CE: `alpha` holds for all three LME intents (needle 0.7, multi_session 1.0, temporal 1.0) and `candidate_k` holds for the two it was pinned on (multi_session 300, temporal 500). The single revision is **`pool_n` deepens**: needle 50→**200**, multi_session 100→**200**, temporal 20→**50**. The live CE rewards reranking a deeper pool than the 0.8.3 fallback did. **Boundary flag:** needle and multi_session land on `pool_n=200` = the grid edge, so their true optimum may be deeper than the registered grid reaches — a cheap follow-up (extend `POOL_NS` past 200) for V-3/V-7, not a V-1 blocker. The point-estimate r@10s track 0.8.3 closely (needle 0.6405 vs 0.6438; multi_session 0.4733 vs 0.4667; temporal 0.500 vs 0.5133), so the live CE **confirms** rather than overturns the prior tuples.

**HotpotQA r@10 is vacuous — read nDCG@10 / MRR instead.** HotpotQA's distractor config gives exactly **10 paragraphs/question**, and `final_K=10`, so "all gold in top-10" is trivially satisfied (top-10 = the whole pool) ⇒ r@10 ≡ 1.0 for every config. The **discriminating** multi_hop signals on HotpotQA are **nDCG@10 (0.9023)** and **MRR (0.947)** — both rank-sensitive — which is why the optimum (`pool_n=10, alpha=0.3`) is chosen by the MRR tiebreak. MuSiQue (20-paragraph pools, r@10=0.535) is the **load-bearing** multi_hop measurement.

**global**: stays PROVISIONAL (EXP-0-global `alpha=0.3, pool_n=10, candidate_k=200`) — no node-level retrieval gold by design; real fill is the priced OPP-6/V-7 LLM-judge axis (out of $0 V-1 scope). Accepted, documented carry.

## 2. §II.C crux on LIVE CE — alpha=1.0, candidate_k=200: pool_n=50 vs pool_n=10 r@10

| intent | r@10 pool_n=10 | r@10 pool_n=50 | Δ(50−10) | drops? | MRR p10 | MRR p50 |
|---|---|---|---|---|---|---|
| multi_session | 0.3733 | 0.4533 | 0.08 | False | 0.661034 | 0.6862 |
| needle | 0.6373 | 0.5065 | -0.1308 | True | 0.587809 | 0.565449 |
| temporal | 0.48 | 0.5 | 0.02 | False | 0.581093 | 0.575738 |
| **pooled** | 0.533 | 0.4917 | -0.0413 | **True** | 0.604272 | 0.597884 |

**Needle α=1.0 anomaly (the 0.8.3 finding: p50 < p10):** on live CE, needle Δ(50−10) = -0.1308, drops = True. Reproduced.

## 3. Recall envelope (gold-in-pool @ candidate_k; base order, alpha-invariant)

| intent | @10 | @50 | @100 | @200 | @300 | @500 |
|---|---|---|---|---|---|---|
| multi_session | 0.373 | 0.627 | 0.713 | 0.800 | 0.840 | 0.853 |
| needle | 0.637 | 0.837 | 0.879 | 0.895 | 0.902 | 0.912 |
| temporal | 0.480 | 0.707 | 0.733 | 0.767 | 0.773 | 0.800 |
| multi_hop/musique | 0.534 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| multi_hop/hotpotqa | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |

## 4. P0-4 margins (live-CE decisiveness) at each intent's optimum

| intent/corpus | top_gap | gold_rival_margin | top_gap_ce | gold_rival_margin_ce | n(ce) |
|---|---|---|---|---|---|
| needle (LME) | 0.1812 | 0.0147 | 0.2456 | 0.1442 | 296 |
| multi_session (LME) | 0.1473 | 0.1641 | 0.1473 | 0.1641 | 144 |
| temporal (LME) | 0.1075 | 0.0541 | 0.1075 | 0.0541 | 135 |
| multi_hop/musique | 0.1694 | 0.1561 | 0.3183 | 0.2422 | 2417 |
| multi_hop/hotpotqa | 0.201 | 0.2738 | 0.2354 | 0.2954 | 7384 |

## 5. MMR / recency offline-transform arms (disposition bar: Δr@10 CI-lo > +0.04)

**Recency → temporal** (half_life ∈ {off,7,30,90}d, decay `score·exp(-Δt/hl)`): base r@10=0.5, best-arm r@10=0.6, lift CI-lo vs base=0.0267 → **no measured lift >= +0.04 -> off-default**.

| arm | r@10 [CI] |
|---|---|
| off | 0.5000 [0.4200,0.5800] |
| hl_7d | 0.6000 [0.5267,0.6733] |
| hl_30d | 0.5933 [0.5133,0.6667] |
| hl_90d | 0.5800 [0.5000,0.6533] |

**MMR → multi_session** (λ ∈ {off,.3,.5,.7}, bge-small embeddings, applied after CE re-blend): base r@10=0.4733, best-arm r@10=0.2267, lift CI-lo vs base=-0.3133 → **no measured lift >= +0.04 -> off-default**.

| arm | r@10 [CI] |
|---|---|
| off | 0.4733 [0.3933,0.5533] |
| lambda_0.3 | 0.0800 [0.0400,0.1267] |
| lambda_0.5 | 0.1467 [0.0933,0.2067] |
| lambda_0.7 | 0.2267 [0.1600,0.2933] |

**Mechanism notes (both knobs left off-default, honestly).**

- **Recency is suggestive but under-powered, not a lever.** `hl_7d` lifts the temporal point estimate **+0.10** (0.500→0.600), the largest single move in V-1 — but at n=150 the CI-lo lift is **+0.0267 < +0.04**, so it does not clear the bar. This is the one knob worth **re-testing with more temporal gold** (the LOCOMO corroboration set, $0) before V-7; if the lift holds at power it becomes a temporal-class default. Recorded off-default for now.
- **MMR is actively harmful for multi_session — a real finding, not a null.** Every λ *degrades* strict r@10 (0.473 → 0.227 at λ=0.7, → 0.080 at λ=0.3). The mechanism: multi_session gold sessions are mutually **topically similar** (same conversation thread), so MMR's diversity penalty demotes the very gold it should keep. MMR's diversity objective is **anti-correlated** with strict all-gold recall here. Firmly off-default for multi_session.

**MMR → global:** N/A — global has no node-level retrieval gold in V-1 (no r@10 axis); reported N/A per plan §3.

## 6. KILL / divergence re-check + EXP-B′.5 guard (live CE)

- distinct optima: 3 of 3 LME intents measured
- signatures (candidate_k,pool_n,alpha): `{'needle': [200, 200, 0.7], 'multi_session': [300, 200, 1.0], 'temporal': [500, 50, 1.0]}`
- collapses to one global config: **False** (divergence_eps=0.02)
- **GO — per-intent optima DIVERGE; the config-carrying router has measured value (EXP-Fr routing-value case supported).**

- EXP-B′.5 forbidden-composition: any optimum regresses another intent beyond noise = **True**. Router-isolation rule preserved (map_reduce_qfs/community_summary ONLY for `global`).

## 7. GATE VERDICT — does V-1 unblock V-3 (OPP-1) / V-7 (OPP-3)?

- [x] Item 1 — tuple re-validation on live CE (KILL/divergence): GO/diverge
- [x] Item 2 — §II.C crux re-confirmed on live CE: done
- [x] Item 3 — multi_hop filled (MuSiQue + HotpotQA, measured, per-corpus): done
- [x] Item 5 — recall envelope re-measured fresh per intent: done
- [x] Item 4 — MMR/recency dispositioned: done

**GATE: PASS — V-1 LANDS and UNBLOCKS V-3 / V-7.** Items 1-3 + 5 produce committed, live-CE-measured verdicts/artifacts (a result doc + repro, not an `AGREED`); item 4 has a disposition for each knob. `global` staying provisional is an acknowledged carry, not a blocker. The re-validated per-intent config registry + recall envelope above is the improved-recall substrate V-3 (decomposition) iterates *within* and V-7 (CE-default-on packaging) records the bearing of.

**Accepted carries (not gate blockers, honestly logged):**

1. **`global` provisional** — no node-level retrieval gold; real fill = the priced OPP-6 / V-7 LLM-judge axis.
2. **LOCOMO corroboration NOT run this session** — multi_session / temporal are measured on LME only. The LOCOMO set is acquired and $0 to add; it is the natural power-up for the **under-powered recency signal** (§5) and a cross-corpus check of the multi_session / temporal tuples. Recommended before V-7 finalizes the temporal default; not required to unblock V-3.
3. **`pool_n=200` grid-edge** for needle / multi_session (§1 boundary flag) — extend `POOL_NS` past 200 in a cheap follow-up.
4. **HotpotQA r@10 saturated** — use its nDCG@10 / MRR; MuSiQue is the load-bearing multi_hop tuple.

---

### Repro

```
# build: isolated venv with default-reranker (live CE) + datasets/tokenizers eval deps
# LME base grid (live CE, exact depth-200 cap):
python -m eval.expb_joint_tune_run --ce-depth 200 \
  --recall-pool-ckpt <ckpt> --rerank-ce-pass <same ckpt> --out-json V-1-lme-output.json
# multi_hop CE pass (per corpus, fused bm25+dense + live CE):
python multihop_cepass.py --corpus musique --path musique_dev.jsonl --out mh-musique.json
python multihop_cepass.py --corpus hotpotqa --path hotpotqa_dev.jsonl --out mh-hotpotqa.json
# analysis (multi_hop tuples, nDCG, margins, MMR/recency dispositions): v1_analyze.py + v1_lme_extras.py
```

