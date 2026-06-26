---
name: fathomdb-recall-fidelity-vs-relevance
description: "FathomDB's gated recall floor measures ANN-quantization FIDELITY, not IR/agentic RELEVANCE — two different axes; the product-value gate (IR-1/IR-2 IR-eval initiative) doesn't exist yet"
metadata: 
  node_type: memory
  type: project
  originSessionId: 857bb76f-928c-49c7-a858-4ecfbb197057
---

FathomDB recall is **two axes**, and only one is gated. A 2026-06-07 retrieval-eval
assessment (`dev/notes/recall-eval-framework-assessment-20260607T174821Z.md`) established
the distinction — carry it into ANY recall discussion:

- **Fidelity axis (GATED): eu7 / AC-075, floor ≥0.90.** Measures whether bit-KNN + f32
  rerank reproduces the **exact-f32 top-10 of the SAME model** — i.e. quantization
  faithfulness. A **system-health** property. This is the 0.8.0 GA gate that read 0.8710 on
  the expanded corpus ([[0.8.0-ga-blocked-recall-corpus]]).
- **Relevance axis (NOT gated): eu8_ir_validation.rs.** qrels-based Recall/precision/MRR/
  NDCG — "did the genuinely-relevant doc/fact get retrieved." Observed **ceiling ≈0.571**,
  embedder/graph-bound (NOT quantization-bound; `ADR-0.7.0-vector-binary-quant.md:172-179`),
  deliberately **report-only**. This is the **product-value** axis.

Key consequence: **fidelity (0.937/0.871) already ≫ the 0.571 relevance ceiling, so chasing
fidelity higher via engine work buys ≈0 product value.** Don't conflate the two; don't treat
a fidelity dip as a product-quality cliff; don't gate on relevance numbers above the
embedder ceiling (permanently-red gate). The real product question — "when the agent needs a
memory to act, is the evidence retrieved" — has **no gate yet**.

FathomDB has no chunking (stores whole bodies), no real reranker (`rerank_fused` is an
identity stub, `lib.rs:3658`), and graph traversal is 0.8.1. So fact-level recall (not
chunk recall) is the missing axis; a fact-level gold set is the prerequisite.

**IR-eval `IR-1`/`IR-2` initiative** (commissioned 2026-06-07, post-0.8.0-GA / 0.8.1) stands up
the product-value gate: `IR-1` (`dev/plans/prompts/0.8.x-IR-1-recall-measure.md`) defines the
measure via a **Claude↔codex consensus step before experiments**, mints AC(s) (REQ-067;
thresholds TBD), builds eval infra (promote eu8 + K-ladder, fact-level gold set, pooling,
reranker seam); `IR-2` (`0.8.x-IR-2-recall-gate.md`) analyzes IR-1 outputs → HITL gate
recommendation. **`IR-1`/`IR-2` are this initiative — DISTINCT from the shipped G8 (dangling-edge)
/ G9 (RRF) gaps.** Placeholders seeded in architecture.md §9, requirements.md REQ-067,
acceptance.md. Extends [[perf-recall-gates-masked-and-ac013b-conflation]].
