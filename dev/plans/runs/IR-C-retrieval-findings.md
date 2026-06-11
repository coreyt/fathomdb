# IR-C — Retrieval diagnostics: measured findings & conclusions

Status: **findings, evidence-complete** · 2026-06-11 · Branch `claude/recent-changes-state-a6wth3`
Instrument: `tests/ir_c_fusion_experiment.rs` (fusion) + `tests/ir_c_gold_diagnostics.rs`
(gold diagnostics). Plan: `dev/plans/IR-C-test-query-quality-instrumentation-plan.md`.
Decision context: `dev/plans/runs/IR-C-api-surface-knobs-to-review.md`.

## TL;DR

On the **full frozen corpus** (10,506 docs; bge-small-en-v1.5, 384-d; production
k=30):
- **exact_fact is a lexical task** — BM25 finds the gold doc at median rank 1 (74%
  at rank 1). A vector arm adds ~nothing (1% of exact_fact is dense-only).
- **exploratory is genuinely hard, and chunked dense does NOT fix it.** BM25 buries
  the gold doc at **median rank 26**; the chunked dense arm is **worse (median rank
  99)**. Only **9%** of exploratory queries are rescued by dense (gold past BM25's
  top-50 but inside dense's top-50); **38%** are *hard* — neither arm finds them by
  rank 50.
- **Conclusion:** Option A (chunking) / dense-arm re-weighting does **not** unlock
  exploratory at bge-small. The median-99 dense rank is a **model-quality** signal —
  **the next lever is a stronger embedding model**, not a chunk-geometry or fusion
  change. exact_fact stays lexical; positional locators (#8) stand on citation merit.

## How we got here (the slice was misleading)

1. **1,200-doc slice (fusion experiment).** Hybrid looked "lexical-bound" — the
   content-OR/BM25 arm was ~the exploratory ceiling and the chunked dense arm at the
   shipped 3:1 added nothing; at 1:1 it *hurt*. The small-corpus "1:1 deep-K win"
   from an earlier directional run **did not reproduce**.
2. **Complementarity diagnostic (same slice, oracle-union).** Dense was *not*
   redundant but only thinly complementary: union beat text by +0.044/+0.044/+0.048
   at R@10/20/50, rescuing ~4–5% of exploratory queries. Flagged that the slice has
   ~9× too few distractors, so the lexical arm looked artificially strong.
3. **Full-corpus lexical + dense diagnostics (below)** confirmed the slice bias and
   answered the question at scale.

## Full-corpus LEXICAL measures (model-free, 4,472 positive queries)

`bm25_gold_rank` = rank of the gold doc under content-OR + FTS5 `bm25()` over the
whole corpus; `idf_overlap` = IDF-weighted query∩gold-doc content-term coverage.

| class | n | bm25_rank1_frac | median bm25 rank | found@1000 | mean idf_overlap |
|---|---|---|---|---|---|
| exact_fact | 2888 | 0.738 | 1 | 0.985 | 0.743 |
| exploratory | 1584 | 0.102 | 26 | 0.859 | 0.704 |

Read: exact_fact is trivially lexical. exploratory is **not** lexical-bound at full
scale (median rank 26) — and `idf_overlap ≈ 0.70` says the query terms *are* in the
gold doc, so it's a **discrimination** problem (the right transcript is buried among
many others that mention the same terms), not a vocabulary gap. The slice's strong
text recall (0.64 R@10) was a low-distractor artifact.

## Full-corpus DENSE measures (bge-small, 128/96 max-pool, bucket_cap=50)

Buckets: `lexical` = BM25 reaches gold by rank 50; else `semantic` = chunked dense
reaches it by 50 (the stratum that would justify a vector arm); else `hard` =
neither.

| class | n | lexical | **semantic** | hard | dense median rank | dense top-10 / top-50 |
|---|---|---|---|---|---|---|
| exact_fact | 2888 | 2732 (95%) | 36 (1%) | 120 (4%) | 2 | 69% / 78% |
| exploratory | 1584 | 846 (53%) | **142 (9%)** | **596 (38%)** | **99** | 16% / 37% |

Read (exploratory):
- The chunked dense arm is a **weak** retriever here — median gold rank **99**,
  top-50 only 37%, *worse* than BM25's median 26 / top-50 53%. It does **not** seize
  the rank-26 rerank opportunity the lexical tier exposed.
- Complementarity is real but **small (9%)**: oracle union lifts exploratory top-50
  from **53% → 62%** (+9 pts). Capturing that needs a near-perfect fusion; the dense
  arm's median-99 weakness is exactly why the real fusion went *negative* on the
  slice (a weak arm displaces good lexical hits).
- The dominant problem is the **38% `hard`** exploratory queries — neither arm finds
  them by rank 50. Chunking does not touch this.

## Conclusions / decision lean

1. **Defer B (chunking) and dense-arm re-weighting for exploratory.** At bge-small
   the chunked dense arm is too weak to cash even the 9-pt oracle headroom, and the
   bottleneck is the 38%-hard stratum that a chunk-geometry change cannot move.
2. **The next lever is a stronger embedding model** (the median-99 dense rank is a
   model-quality signal). Quantify on the same instrument by swapping the `Embedder`
   and re-running the dense diagnostic — see the embedder research note (in progress)
   for candidates that respect FathomDB's lightweight + dimension/quantization +
   license constraints.
3. **exact_fact stays lexical.** Ship the content-OR/BM25 path; vector adds ~nothing.
4. **#8 (positional/citation locators)** still stands on its citation argument.

## Caveats / provenance
- Scope = full corpus (10,506 docs). Dense run skipped the whole-doc geometry to fit
  the window (buckets need only 128/96); the persistent-box run can fill
  `dense_gold_rank_whole`.
- `passage_evidence_iou` had no data — the source QA (enronqa/qaconv/qmsum) carries
  no char spans, so `span_locator_queries=0`. The metric is wired and waits on
  span-bearing labels (item 4 / future).
- Full per-query records are in the gitignored
  `data/corpus-data/eval/ir_gold/all.gold.diagnostics.json` (reproducible via the
  runbook in the instrumentation plan); the summaries above are the durable record.
- gold `qrels_version` = `ir-c-reused-v2`; corpus_hash `fe973fcd…` (frozen snapshot).
