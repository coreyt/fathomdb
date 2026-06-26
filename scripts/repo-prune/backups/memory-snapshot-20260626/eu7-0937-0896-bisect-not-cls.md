---
name: eu7-0937-0896-bisect-not-cls
description: "eu7 0.937→0.896 root-cause bisected — case A (vector-path/SUT), NOT CLS/embedding; 0.937 is a non-comparable pre-seam anchor; Slice-20 recovery is quant-path not pooling"
metadata: 
  node_type: memory
  type: project
  originSessionId: bef05bc1-1b62-4fa6-b640-f1ec2360e4c1
---

0.8.3 offline bisect (2026-06-22, $0 / no-LLM / no-GPU / no-build) of the eu7
ANN-fidelity recall@10 0.937→0.896 "regression" — branch `eu7-bisect-20260622`,
report `dev/plans/runs/0.8.3-eu7-bisect-report.md`. **Verdict: case A
(vector-path / measurement-SUT), NOT case B (CLS/embedding); corpus also ruled out.**

**Why CLS is NOT the cause (definitive):** `git diff v0.7.2 v0.8.0 -- fathomdb-embedder/src/`
is **empty** → f32 vectors byte-identical. CLS pooling is commit `c7afbfde`
(2026-06-11, AFTER the 0.896 run), `default Mean — production unchanged`;
`CandleBgeEmbedder::new()` (used by eu7) still returns `Pooling::Mean`. It's an
unused gated option. **The vector-stage ranking is unchanged v0.7.2→v0.8.0** too:
the Pack1→Pack2 migration copies the quantized bits **byte-identical** ("no
re-quantize, centering survives"); the unfiltered bit-KNN SQL is **ranking-invariant**
(d28d2046 cosmetically refactored it — `vec_distance_l2(...) AS l2` + order-by-alias;
same distance/order, NOT literally byte-identical — codex §9 P2); f32 rerank
unchanged. So there is
**no fidelity-loss commit**. Leading cause = the **B-1 `vector_stage_only` SUT
change**: 0.937 was the 0.7.1 `search()`-SUT anchor; 0.896 (CI 0.864–0.925, the live
`GA-signoff-eu7-remeasure-20260608` value) is the new seam — **not directly
comparable**. Corpus ruled out by GA-1 (byte-identical, mtime 2026-05-27).

**How to apply (Slice 20):** (1) 0.937 is **not a recoverable target** — the true
vector-stage fidelity is **0.896**, holding the 0.90 floor only via the one-sided CI
gate (`recall_ci_hi 0.925 ≥ 0.90`). (2) Treat the CLS-fix as an **eu8/relevance**
lever (the Mem0 gap), NOT the eu7 fidelity-recovery. (3) The CLS-fix still rewrites
stored vectors → re-measure eu7 **fresh** vs 0.90 on the seam after the re-embed; if
it breaches, the fork is the **quant path** (rotation/whitening → ANN fan-out K>192 →
2-bit), **not pooling**. Pairs with [[0.8.3-slice15a-embedder-probe-no-swap]]
(Slice 20 keeps CLS-corrected bge-small, no swap).

**Residual (honest):** the definitive engine 0.7.x-vs-0.8.0 vector-stage A/B was NOT
run — no pre-embedded eu7 IR-corpus DB exists ([[embed-completeness-and-gpu-readiness]]),
re-embed forbidden/out of $0 scope. eu7 is **LLM-free → $0 in API**; the A/B needs an
embed only (minutes on GPU when util<5%, ~1h CPU). Corrects the stale "0.937 predates
corpus expansion" claim in [[0.8.0-ga-blocked-recall-corpus]].
