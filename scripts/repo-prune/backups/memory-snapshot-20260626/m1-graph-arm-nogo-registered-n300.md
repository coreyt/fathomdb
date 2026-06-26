---
name: m1-graph-arm-nogo-registered-n300
description: "0.8.2 M1 DECISIVE NO-GO — registered n=300 MuSiQue multi-hop study: lexically-seeded PPR graph-fusion does NOT beat fused-RRF; CI upper bound below materiality. Redirect 0.8.3 to index-key enrichment."
metadata: 
  node_type: memory
  type: project
  originSessionId: 1ca602a1-a2f1-4e36-bfc8-17ee640761da
---

The 0.8.2 **M1 milestone** delivered a clean, registered, decisive **NO-GO** on the graph arm
(2026-06-17). This is the rigorous successor to the 0.8.1 n=40 work in [[graph-arm-doesnt-beat-bm25-pivot]]
— and it tested the *fix that memory hypothesized* (lexical seeding) and the graph arm **still loses**.

**Run:** gpt-5.4 reader, n=300, MuSiQue multi-hop, completeness 1.0, 0 errors, $2.50 (clean after
the infra battles — see [[priced-runs-need-resilience-before-spend]]).

**5-arm pooled ≥3-hop F1:** passage_dense **0.487 (best)** · fused-RRF (comparator) 0.450 ·
fused_rerank 0.415 · ppr_fusion (graph arm) 0.410 · bm25 0.370.

**Primary endpoint — ΔF1 (ppr_fusion − fused_RRF), pooled ≥3-hop:** **−0.0405**, 95% CI
**[−0.116, +0.031]** (paired bootstrap, n=144). ΔEM −0.035. Per-hop ΔF1 all negative (2-hop −0.016,
3-hop −0.044, 4-hop −0.033; none individually significant); trend slope −0.013, not sig. `decide() = NO_GO`.

**Why robust, not just underpowered:** the CI upper bound **(+0.031) sits BELOW the +0.04 materiality
threshold** — even the optimistic edge of the data rules out a material graph win, and the point estimate
is negative. More N can't rescue a best-case that's already sub-material ⇒ no stage 2. The earlier
P(GO)=0.45 underpower worry is moot: well-powered to reject, and it rejects.

**Cross-reader robust:** direction holds across gemini 81%-partial (ppr 0.227 < fused 0.240), cheap
gpt-5.4 (fused 0.395 > ppr 0.379), and full gpt-5.4. ppr_fusion ≠ bm25 on **66/300** ⇒ it genuinely
ran, not a vacuous copy.

**Honest caveats:** (1) confident-wrong guard **UNEVALUATED** — this graph has only answerable
questions, no unanswerable contrast set, so that gate is a placeholder. (2) formal `decide()=NO_GO`
routes through the power gate (n=144 ≪ 1165), but the **load-bearing** read is the scientific one
(negative effect + sub-material CI). (3) passage_dense being strongest is an **observation, not a
registered comparison**.

**Decision / How to apply:** RECORD = **NO-GO** on graph work. Redirect **0.8.3 → index-key
enrichment**, not more graph. Note passage_dense (0.487) beat fused-RRF here — dense passage retrieval
is the strongest unregistered arm, a candidate direction. Data: `dev/plans/runs/0.8.2-m1-verdict-n300.json`,
report `dev/plans/runs/0.8.2-m1-report-gpt54.md`, findings `dev/plans/runs/0.8.2-m1-FINDINGS.md`.
Corroborates [[fathomdb-recall-fidelity-vs-relevance]].
