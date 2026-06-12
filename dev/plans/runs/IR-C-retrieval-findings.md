# IR-C — Retrieval diagnostics: measured findings & conclusions

Status: **CLOSED — dense investigation done (in the context of current knobs)** ·
2026-06-11 · Branch `claude/recent-changes-state-a6wth3`
Instrument: `tests/ir_c_fusion_experiment.rs` (fusion) + `tests/ir_c_gold_diagnostics.rs`
(gold diagnostics). Plan: `dev/plans/IR-C-test-query-quality-instrumentation-plan.md`.
Decision context: `dev/plans/runs/IR-C-api-surface-knobs-to-review.md`.

## CLOSEOUT (2026-06-11) — dense investigation done under current knobs

**Decision:** the dense-retrieval line of investigation is **closed** for now. Under
every knob we can currently turn, dense retrieval does not improve the class that
needs it (exploratory). **Ship the current default** — bge-small / **Mean** pooling,
whole-doc + **3:1** fusion + **k=30** — and **treat exploratory as the lexical (BM25)
arm's domain**; the dense arm earns its keep only on exact_fact, where BM25 already
wins, so it is a (harmless) redundancy there.

**Knobs swept (all measured on the full 10,506-doc corpus) and their verdict:**
| knob | values tried | exploratory effect |
|---|---|---|
| chunk geometry | whole-doc, 64/48, **128/96**, 256/192 (max/mean/top2 pool) | chunking lifts dense *solo* recall but the fused hybrid is flat/negative |
| fusion weight × k | 3:1, 1:1 × k=30 | 1:1 hurts shallow, ~+0.03 deep; no win |
| pooling | mean vs **CLS** (model-native) | ~neutral; CLS slightly *worse* (median 99→121) |
| query prefix | on/off | no material effect |
| embedding model | bge-small vs **nomic-embed-v1.5** | nomic *worse* on exploratory (99→135), better on the already-solved exact_fact |

**Why it's closed (the converged finding):** chunk-based single-vector dense
retrieval is **structurally weak** for discourse/summary ("exploratory") queries
over long transcripts — the answer spans the whole discussion, not any 128-word
window, so max-pool over short chunks can't represent it, *regardless of model or
pooling*. BM25 (median gold rank 26) beats dense (median 99–135) here and is the
right tool. Three independent levers (chunking, pooling, stronger model) all failed
to move it; we've hit diminishing returns under these knobs.

**Explicitly OUT of scope — PARKED, not refuted** (different mechanism, not a "knob"):
- **Whole-doc long-context embedding** (nomic 8192-ctx, no 512-truncation) and
  **late chunking** — the one dense angle with an untested mechanism for discourse
  retrieval; expensive, unproven.
- **Multi-vector / late-interaction (ColBERT-style)** and a **real reranker** stage.
- **Query-side** methods (query expansion, HyDE).
- **Test-label quality** (item 4): exact_fact is lexically aligned and exploratory
  labels are single-doc/sparse — the benchmark may understate dense's real-world
  value if production queries are more paraphrastic. This caveat bounds how far to
  generalize "dense doesn't help" beyond this corpus.

These are the doors to open *if/when* dense retrieval is revisited; none is refuted
by this investigation, which was scoped to the current knobs only.

---

## TL;DR (measured detail below)

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

   **UPDATE (2026-06-11), see `dev/notes/IR-C-embedder-options-research.md`:** a
   pooling-bug hypothesis was raised (bge-small is CLS-pooled per its
   `1_Pooling/config.json`, but FathomDB mean-pools it) and **tested** — it was
   **REFUTED**. CLS pooling + query prefix cleared the 1-bit binary floor (0.944) but
   did **not** fix exploratory: dense median rank **99 → 121**, top-50 **37% → 34%**
   (marginally worse), while exact_fact nudged up. Since the dense diagnostic embeds
   ~128-word chunks, 512-truncation isn't the cause either.

   **FURTHER UPDATE (2026-06-11):** the "stronger model" lever was then **tested** —
   `nomic-embed-text-v1.5` (MTEB 62.28 vs bge 51.68) ran on the same diagnostic. It
   **also did not fix exploratory** (median rank 99→**135**, top-50 37%→**32%**) — it
   was *better* on exact_fact (+6 pts, already lexically solved) but *worse* on
   exploratory, at ~2× compute / ~4× model size. So exploratory is **not** a model-
   capacity problem. Across three tries (chunking, pooling, stronger model), nothing
   lifts it — it's a **structural** weakness of chunk-based single-vector dense
   retrieval for discourse/summary queries; BM25 (median rank 26) is the better
   exploratory component. Stay on bge-small/Mean; lean lexical for exploratory. The
   one untested dense angle is **whole-doc long-context** embedding (where nomic's
   8192-ctx would actually apply) — see `dev/notes/IR-C-embedder-options-research.md`.
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
