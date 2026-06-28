# Agent Prompt B — Fable-5 Roadmap Review (consumes Prompt A's dossier)

**Agent type:** general-purpose (needs Read, WebSearch/WebFetch to verify/extend citations).
**Model:** `fable` (Fable 5), **high reasoning effort** — this is the deliberate
high-quality review/decision step; instruct deep, adversarial reasoning.
**Input:** `dev/plans/runs/IR-C-roadmap-analysis-dossier.md` (the Prompt-A dossier).
**Output:** write `dev/plans/runs/IR-C-roadmap.md` and return a ≤300-word executive summary.

## Objective

Critically **review** the analysis dossier, then produce a **prioritized retrieval
roadmap** for FathomDB whose explicit goal is **retrieval/answer quality as-good-or-better
than Mem0 and Zep within the local-first / CPU / no-API / 1-bit-binary footprint**. The
roadmap MUST include, per initiative: **justification, a probability of success, and
citations.** This is the decision artifact — be rigorous, quantitative, and honest about
uncertainty.

## Step 1 — Review the dossier (adversarial)

Before recommending anything, audit Prompt A's dossier:

- **Correctness:** spot-check the load-bearing numbers against the cited sources (the
  dossier's §4 verification log + the underlying files). Flag any number that is unsupported,
  internally inconsistent, or where MEASURED/CLAIMED/INFERRED is mislabeled.
- **Completeness:** what's missing for a roadmap decision? (e.g. no end-to-end QA metric vs
  peers; untested whole-doc long-context; reranker candidate-recall ceiling not quantified;
  graph candidate-generation design unspecified.) List the gaps.
- **Framing:** is the candidate-recall ceiling (reranker bounded ~0.53–0.62; ~38% hard core
  irreducible; dense-embedder lever closed by the Nomic A/B) correctly carried? Correct it
  if not.
Write a short **Review** section (findings + corrections) at the top of the output.

## Step 2 — Produce the roadmap

A prioritized sequence of initiatives. For EACH initiative provide:

1. **What & why (justification):** the change, the mechanism by which it should help
   (tie to the measured bottleneck: factoid=lexical-solved; exploratory=discrimination +
   multi-hop; the ~0.62 union ceiling; the irreducible hard core), and which query class it
   targets.
2. **Probability of success — with reasoning and citations.** Give an explicit probability
   (e.g. "≈70%") that the initiative materially improves the targeted metric, justified by
   citing (a) FathomDB's own measured data in the dossier and (b) external evidence
   (papers/benchmarks). Distinguish *probability it helps at all* from *expected magnitude*
   (give a rough effect-size range — **derive the bound from the dossier's headroom math, do
   NOT anchor on a fixed figure**; e.g. reranking lifts exploratory R@10 toward but cannot pass
   the ~0.53 candidate-recall ceiling, and the ~38% hard core is unreachable by reordering).
   State the key risk that would make it fail.
3. **Footprint fit:** CPU/local/no-API/1-bit-binary compatibility — reject or flag anything
   that violates it (API embedders, GPU-only, ColBERT-style late interaction vs 1-bit).
4. **Cost & sequencing:** rough implementation cost, dependencies, and where it sits in the
   order. Prefer cheap, high-probability, in-footprint wins first.
5. **How to measure it:** the experiment/gate that would confirm success on the existing
   harness (e.g. dense diagnostic, fusion harness, a new end-to-end QA eval vs Mem0/Zep),
   including the binary-floor gate where relevant.

Cover at minimum the levers the dossier surfaces: **cross-encoder reranking**, **graph-aware
retrieval (node/edge/both — pick and justify)**, **whole-doc long-context dense (research
probe)**, and an **end-to-end QA eval to make the Mem0/Zep comparison fair**. Add any the
review surfaces. Explicitly say which levers are **closed** (stronger chunked dense embedder)
and why.

## Step 3 — Goal assessment

Close with a candid assessment: **can FathomDB plausibly reach Mem0/Zep-parity within its
footprint, and with what probability?** Aggregate the per-initiative probabilities into an
honest overall outlook (best-case / likely / floor), state the biggest single risk, and name
the one measurement that would most reduce uncertainty. Carry the metric-comparability caveat
(peers' LoCoMo/LongMemEval answer-accuracy vs our Recall@K) — note that a true parity claim
needs an end-to-end eval, and treat any single peer benchmark number as vendor-contested.

## Quality bar

- Every probability and effect-size claim carries a citation (dossier data and/or external
  URL). No bare assertions.
- Quantitative and bounded — respect the candidate-recall ceiling; don't promise gains the
  data can't support. Honest about the irreducible hard core.
- Footprint is a hard constraint, not a preference. Flag every violation.
- Output = the `Review` section + the roadmap + the goal assessment, written to
  `dev/plans/runs/IR-C-roadmap.md`; return the executive summary.
