# EXP-M4 — embedder-ceiling measurement (0.8.11 Slice 10)

- cost: **$0 (GPU local + reuse of byte-verified offline measurements; Gate-2 reuse precedent)**
- method: Embedder ceiling is a device-invariant model-weights property; consolidate the FULL s15a probe (eu7 re-clear + eu8 + hard subset + paired-bootstrap CI + cpu cost) and the eu-0 raw-recall sweep, and confirm device-invariance on the GPU.
- GPU confirmation (cuda:0 = NVIDIA GeForce RTX 3090, torch 2.10.0+cu128): bge-small GPU-vs-CPU mean row cosine **1.0**, max abs elt diff 1.2e-07 -> ceiling is device-invariant.

## s15a FULL probe (10506 docs, qrels `ir-c-reused-v1`, hard n=825)

- base (CLS-corrected bge-small): eu8=0.3994 hard@10=0.0194 11.2ms/q

| candidate | eu8 | eu8 margin CI | hard@10 | hard CI-lo | proj_eu7 | clears 0.90? | cpu_feas | in_lib | PASS |
|---|---|---|---|---|---|---|---|---|---|
| bge-base | 0.4235 | [0.015,0.0335] | 0.023 | -0.0036 | 0.7855 | False | True | True | False |
| e5-base-v2 | 0.4674 | [0.0568,0.0789] | 0.0267 | -0.0061 | 0.896 | False | True | True | False |
| gte-base | FAILED | — | — | — | — | — | — | False | n/a |
| nomic | 0.451 | [0.0416,0.0624] | 0.0218 | -0.0085 | 0.9317 | True | False | True | False |

## eu-0 raw recall@10 (1-bit Hamming->f32 ANN, n=100, 7667 docs) by fanout K

| model | dim | K=32 | K=64 | K=96 | K=128 | K=256 |
|---|---|---|---|---|---|---|
| bge-small | 384 | 0.683 | 0.793 | 0.849 | 0.882 | 0.933 |
| bge-base | 768 | 0.783 | 0.885 | 0.914 | 0.928 | 0.964 |
| e5-small-v2 | 384 | 0.362 | 0.448 | 0.495 | 0.544 | 0.664 |

## Verdict

**KEEP bge-small. No candidate clears the swap gate net of re-whiten/eu7 re-clear + cost. A productized swap is out of 0.8.11 (HITL #2).**

- Reconciliation: eu-0 raw recall@10 (fanout K=256): bge-small=0.933, bge-base=0.964, e5-small-v2=0.664. EXP-M4 CONFIRMS the eu-0 ordering (bge-base highest raw recall, e5 worst) but REVISES the naive 'bigger is better' conclusion: net of the 1-bit eu7 re-clear (bge-base projected_eu7=0.7855 < 0.90), the hard-subset margin, and 2x cost, bge-base does NOT clear the swap gate -> keep bge-small.
- Keep-unless: Keep bge-small UNLESS a candidate simultaneously (a) clears the 0.90 projected_eu7 floor after re-whiten, (b) shows a hard-subset margin CI-lo > 0 vs bge-small, and (c) is cpu_feasible (<= 3x base latency) or HITL accepts the GPU/cost tradeoff.

### Per-candidate block reason

- **bge-base**: projected_eu7 0.7855 < 0.9 (fails 1-bit eu7 re-clear); hard-subset margin CI-lo -0.0036 <= 0 (no clearance)
- **e5-base-v2**: projected_eu7 0.896 < 0.9 (fails 1-bit eu7 re-clear); hard-subset margin CI-lo -0.0061 <= 0 (no clearance)
- **gte-base**: measurement FAILED (IndexError: index 126396013669456 is out of bounds); not in-library feasible
- **nomic**: hard-subset margin CI-lo -0.0085 <= 0 (no clearance); not cpu_feasible (36.1ms/q > 3x base)
