# EXP-B' — 3-stage joint tuning (0.8.11 Slice 15, KEYSTONE)

- cost: **$0.00** — retrieval-metric arm over existing node-level gold; LLM judge NOT spent (global is provisional-pinned by design; gold sufficient for the measured intents).
- grid: candidate_k [200, 300, 500] x pool_n [10, 20, 50, 100, 200] x alpha [0.0, 0.3, 0.5, 0.7, 1.0] x final_K=10
- bootstrap: 2000x seed 0xb5
- measured intents: ['multi_session', 'needle', 'temporal'] · provisional: ['global', 'multi_hop']
- elapsed: 9.4s

## The §II.C crux — alpha=1.0, candidate_k=200: pool_n=50 vs pool_n=10 r@10

| intent | r@10 pool_n=10 | r@10 pool_n=50 | Δ(50−10) | drops? | MRR p10 | MRR p50 |
|---|---|---|---|---|---|---|
| multi_session | 0.38 | 0.46 | 0.08 | False | 0.654666 | 0.680602 |
| needle | 0.6405 | 0.5098 | -0.1307 | True | 0.574373 | 0.553708 |
| temporal | 0.4933 | 0.5133 | 0.02 | False | 0.575159 | 0.569445 |
| **pooled** | 0.5396 | 0.4983 | -0.0413 | **True** | 0.594442 | 0.589013 |

## Per-intent optimum (r@10 maximizer; CI = 95% bootstrap)

| intent | n | candidate_k | pool_n | alpha | r@10 [lo,hi] | MRR |
|---|---|---|---|---|---|---|
| multi_session | 150 | 300 | 100 | 1.0 | 0.4667 [0.3867,0.5467] | 0.671465 |
| needle | 306 | 200 | 50 | 0.7 | 0.6438 [0.5882,0.6961] | 0.456043 |
| temporal | 150 | 500 | 20 | 1.0 | 0.5133 [0.4333,0.5933] | 0.575686 |

## Recall envelope (gold-in-pool @ candidate_k; base order, alpha-invariant)

| intent | @10 | @50 | @100 | @200 | @300 | @500 |
|---|---|---|---|---|---|---|
| multi_session | 0.380 | 0.633 | 0.720 | 0.807 | 0.840 | 0.853 |
| needle | 0.637 | 0.840 | 0.882 | 0.899 | 0.905 | 0.915 |
| temporal | 0.480 | 0.720 | 0.747 | 0.767 | 0.773 | 0.800 |

## KILL check — do per-intent optima collapse to one global config?

- distinct optima: 3 of 3 measured
- signatures (candidate_k,pool_n,alpha): `{'needle': [200, 50, 0.7], 'multi_session': [300, 100, 1.0], 'temporal': [500, 20, 1.0]}`
- **GO — per-intent optima DIVERGE; the config-carrying router has measured value (EXP-Fr routing-value case supported).**

## EXP-B'.5 — forbidden-composition / joint-regression guard

map_reduce_qfs + community_summary are valid ONLY for `global` (sensemaking) and FORBIDDEN on needle/multi_session/temporal/multi_hop — the §II.B blind-distiller -0.362 cross-wire. This is the forbidden-composition the 0.8.15 plan validator consumes.

### Empirical cross-application (each intent's optimum applied to the others)

| source optimum | applied to | r@10 applied | r@10 dst optimum | Δ | regresses? |
|---|---|---|---|---|---|
| needle (200, 50, 0.7) | multi_session | 0.4 | 0.4667 | -0.0667 | False |
| needle (200, 50, 0.7) | temporal | 0.5133 | 0.5133 | 0.0 | False |
| multi_session (300, 100, 1.0) | needle | 0.4967 | 0.6438 | -0.1471 | True |
| multi_session (300, 100, 1.0) | temporal | 0.5 | 0.5133 | -0.0133 | False |
| temporal (500, 20, 1.0) | needle | 0.5686 | 0.6438 | -0.0752 | True |
| temporal (500, 20, 1.0) | multi_session | 0.46 | 0.4667 | -0.0067 | False |

**Any optimum regresses another intent beyond noise:** True

## Per-intent config tuples (§3 format — the keystone artifact)

- **needle** (EXP-B'): candidate_k=200 pool_n=50 alpha=0.7 final_K=10 · forbidden_ops=['map_reduce_qfs', 'community_summary'] · r@10=0.6438
- **multi_session** (EXP-B'): candidate_k=300 pool_n=100 alpha=1.0 final_K=10 · forbidden_ops=['map_reduce_qfs', 'community_summary'] · r@10=0.4667
- **temporal** (EXP-B'): candidate_k=500 pool_n=20 alpha=1.0 final_K=10 · forbidden_ops=['map_reduce_qfs', 'community_summary'] · r@10=0.5133
- **global** (provisional, EXP-0-global): candidate_k=200 pool_n=10 alpha=0.3 final_K=10 · forbidden_ops=[] · r@10=None
- **multi_hop** (provisional, EXP-0-global): candidate_k=200 pool_n=10 alpha=0.3 final_K=10 · forbidden_ops=['map_reduce_qfs', 'community_summary'] · r@10=None

