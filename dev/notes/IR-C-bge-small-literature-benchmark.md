# IR-C — bge-small IR performance vs the literature (positioning benchmark)

Status: **benchmark + literature synthesis, evidence-graded** · 2026-06-11 · Branch
`claude/recent-changes-state-a6wth3`
Motivation: validate whether FathomDB's low *exploratory* retrieval result is an
expected small-model/hard-task ceiling vs underperformance, and position the
bge-small + BM25-hybrid + 1-bit-binary local-first stack against peer agent-memory
systems. Companions: `dev/plans/runs/IR-C-retrieval-findings.md` (our measured
numbers) · `dev/notes/IR-C-embedder-options-research.md` (model options/constraints).
Inputs: (a) our frozen-corpus IR-C measures; (b) a fresh **empirical BEIR anchor**
(`dev/plans/runs/IR-C-bge-small-beir-anchor.json`); (c) a verified literature sweep
(21 sources, 3-vote adversarial verification).

## Verdict (HIGH confidence)

**FathomDB's exploratory recall (~0.33 fused R@10, ~0.16 dense R@10, dense median
rank 99) is an *expected ceiling* for a 33M-param / 512-token model on
discourse/long-transcript retrieval — NOT underperformance.** The empirical anchor
proves bge-small runs at its documented level in our pipeline; the literature shows
this entire small-model class is weak on discourse/long-doc tasks. Separately, the
**~0.90 factoid recall already sits at the lexical ceiling** — there is no headroom
to chase there.

**The productive levers are ARCHITECTURAL, not embedder quality.** The "stronger
dense model" lever was directly **tested and refuted** (the Nomic A/B, §6: a model at
MTEB 62.3 vs bge's 51.7 made exploratory *worse*, median rank 99→135) — together with
the earlier chunk-geometry and pooling A/Bs, that is three independent failures, so
chunk-based single-vector dense quality is a **closed dead end** for exploratory. What
*can* move it: (1) **cross-encoder reranking** to cash the existing first-stage recall
into better top-K precision; (2) a **graph / relational retrieval path** for
multi-hop/temporal queries — the orthogonal signal peer memory systems actually win
on; (3) the one *untested* dense angle — **whole-doc long-context embedding** (where a
long-context model's context is actually exercised, unlike the chunked diagnostic).
FathomDB already runs hybrid lexical+dense fusion over a graph substrate; the open
moves are **reranking + graph-aware retrieval** (and, as a research probe, whole-doc
long-context dense). These gains are bounded but **less bounded than first stated** —
see the corrected headroom math in §6: reranking is capped by candidate recall *at the
depth you rerank*, and that ceiling is a free (latency-bounded) knob — it rises from
~0.53 at depth 50 toward **~0.86 at depth 1000** (lexical found@1000), so deep-candidate
reranking can pass the depth-50 ~0.62 union. The truly reorder-proof core is **~14%**
(lexically unreachable at depth 1000), not the ~38% measured at depth 50. So exploratory
is **not blocked from improving** — only blocked from improving via a bigger *chunked*
dense embedder; graph's distinct payoff is on the ~14% lexically-unreachable stratum and
on multi-hop/temporal classes the current Recall@K instrument barely contains.

## 1. Empirical anchor — bge-small reproduces its leaderboard in our hands

Ran `BAAI/bge-small-en-v1.5` on three public BEIR tasks via `mteb 2.15`, in both the
published config (CLS pooling + query prompt) and **FathomDB's exact config**
(mean-pooling, no prefix). nDCG@10 / Recall@10:

| BEIR task (type) | published (CLS+prefix) | FathomDB (mean, no-prefix) | MTEB leaderboard |
|---|---|---|---|
| SciFact (factoid claim→evidence) | 0.713 / 0.836 | **0.720 / 0.843** | ~0.71 ✓ |
| ArguAna (argument/counter-argument) | 0.603 / 0.859 | **0.597 / 0.853** | ~0.59 ✓ |
| NFCorpus (hard medical, short-q→long-doc) | 0.343 / **0.162** | **0.350 / 0.170** | ~0.34 ✓ |

**Two conclusions, both load-bearing:**
1. **The mean-pool / no-prefix "usage penalty" is ~zero** (≤0.7 nDCG pts, sign
   varies). This independently confirms the internal CLS-vs-mean A/B: pooling is a
   wash, the low exploratory number is *not* a usage bug.
2. **A "hard" BEIR task reproduces our exploratory numbers.** bge-small's *own*
   documented Recall@10 on NFCorpus is **0.17** — essentially identical to our
   exploratory *dense* arm (~0.16) and below our *fused* exploratory (~0.33). Our
   corpus isn't pathological; it's behaving exactly like a published hard task.

## 2. Similarly-sized models — the factoid-easy / discourse-hard split is universal

Verified literature (strong evidence; 3 independent papers agree):

- **bge-small is bottom-of-class on discourse/long-document retrieval.** LongEmbed
  ~0.31, QMSum ~0.208, MTRAG ~38.2 — vs its ~0.52 BEIR average. The factoid-easy /
  discourse-hard gap is a **known general phenomenon** for short-context dense
  retrievers, not a FathomDB artifact. (arXiv 2510.14880, 2404.12096/LongEmbed,
  2505.19274.) [HIGH]
- The deficit is **representation-quality-bound, not (only) truncation-bound for us.**
  The "truncation is the root cause" framing was **refuted (0-3)** in verification;
  and because FathomDB's dense diagnostic embeds ~128-word chunks (~170 tokens, well
  under 512), truncation isn't even in play on our chunked arm. The limit is model
  capacity on discourse semantics. [HIGH]
- **Literature claim (since SUPERSEDED for our setup): a long-context *small* embedder
  ≈ doubles hard-task recall in the same footprint.** granite-embedding-small-r2 ~61.9
  vs ~32.1 for a short-context small baseline on the hard long-doc set (arXiv 2508.21085).
  [HIGH in the literature] **BUT this is a *long-document* result that assumes the long
  context is actually used. FathomDB's Nomic A/B (§6) tested a stronger long-context
  model in our *chunked* (128-word) setup and got *no* exploratory gain — so the claim
  only plausibly applies to the untested whole-doc long-context angle (§5.3), not to a
  drop-in embedder swap on the current chunked arm.**
- **Caveat on the magnitude:** vendor/technical-report figures self-report favourable
  setups; the "~2×" is directional. Several specific upgrade claims were **refuted**
  in verification (e.g. fine-tuned-bge-small ≈ bge-base 0-3; an MLDR mGTE>BGE-M3
  margin 0-3; nomic retrieval/summary subscores 1-2) — so treat individual
  leaderboard deltas as indicative, not exact. The *direction* (long-context small
  model materially beats short-context small model on hard tasks) is robust.

Candidate long-context small models that **stay binary-quantizable** (our 1-bit
constraint): granite-embedding-small-r2, gte-modernbert-base, nomic-embed-text-v1.5
(MRL). ColBERT-style late-interaction scores higher but **resists 1-bit
quantization** and breaks the footprint model — out for FathomDB. (Consistent with
`embedder-options-research.md` §3–4.)

## 3. Peer agent-memory systems — the architecture *is* the answer, and we have it

Verified + graded (mix of primary papers and vendor blogs — see confidence flags):

- **Peers solve "dense is weak on discourse" with architecture, not a bigger
  embedder.** The consistent pattern across Mem0, Zep/Graphiti, Letta, and
  RAG-memory stacks is **hybrid retrieval (BM25/lexical + dense + reranking) plus a
  structured/graph layer** over the raw vector index. (arXiv 2504.19413; agent-memory
  comparisons; LongMemEval.) [HIGH on the pattern; MEDIUM on any single vendor's exact
  stack.]
- **Embedder size among peers is modest.** Production memory systems lean on *small*
  or *API-small* embedders (MiniLM / bge-family / OpenAI `text-embedding-3-small`),
  not giant models — the differentiation is the memory graph + fusion + reranking,
  not embedder scale. *(Note: a specific "Zep uses BGE-M3" claim was **refuted
  (1-2)** — do not assert Zep's exact embedder; the general small-embedder pattern
  still holds.)*
- **FathomDB is already architecturally aligned.** We run BM25/FTS5 + dense fused via
  weighted RRF, on top of the G0 canonical-identity + edge graph substrate. The piece
  peers add that we don't yet exploit for retrieval is **cross-encoder reranking**
  (verified 3-0 as a standard component) and graph-aware retrieval over the memory
  edges.

## 4. Footprint positioning (the local-first tradeoff, made explicit)

| Axis | FathomDB | Typical peer (Mem0/Zep/Letta) |
|---|---|---|
| Embedder | bge-small 33M, **on-device CPU**, no API | OpenAI `text-embedding-3-small` / small open, often **API/cloud** |
| Vector storage | **1-bit binary** (Hamming + f32 rerank) | full fp32 / fp16 in a vector DB |
| Lexical arm | BM25/FTS5 (built-in) | BM25 + dense (hybrid) |
| Structure | canonical-identity + edge graph | memory graph (Graphiti/Zep) |
| Cost / privacy | **zero-API, local, private** | API calls, network, $/token |

The stack deliberately trades *peak* retrieval quality for **on-device, CPU, no-API,
binary-compact** operation. That is a *different point on the curve*, not a deficiency
— and it constrains the upgrade path. The footprint-respecting levers are a **small
CPU cross-encoder reranker** and **graph-aware retrieval** over the existing edge
substrate (both local, both binary-compatible); a larger dense embedder is both
footprint-costly *and* — per §6 — empirically useless on exploratory. A large API
embedder or a late-interaction reranker (ColBERT) would break the footprint and is
out.

## 5. Recommendation — architecture first (the embedder lever is closed)

Ordered by leverage, after the Nomic A/B (§6) closed the "stronger dense model" path:

1. **Cross-encoder reranking over the fused top-K (highest leverage, in-footprint).**
   A small CPU cross-encoder reorders the fused candidates to cash first-stage recall
   into top-K precision. **Bounded:** it cannot exceed candidate recall (exploratory
   R@50 ≈ 0.53, oracle-union ≈ 0.62), so expect exploratory R@10 ≈ 0.33 → ~0.45–0.50,
   nothing for the ~38% hard stratum, and ~nothing for factoid (already 0.90). This is
   what peers (Zep/Mem0/Letta) use to turn a weak dense arm into good end-to-end recall.
2. **Graph-aware retrieval over the edge substrate (raises the ceiling on the right
   slice).** Adds an *orthogonal* candidate path beyond lexical+dense — the only way
   past the ~0.62 union ceiling. Biggest, most defensible win on **multi-hop /
   relational / temporal** ("deep-exploratory") queries; limited on single-transcript
   summary *discrimination*. This is the lever Zep's graph wins on over Mem0.
3. **Research probe — whole-doc long-context dense (the one untested angle).** The
   chunk-based dense A/Bs (geometry, pooling, Nomic) are exhausted, but they never
   exercised a long-context model's actual context window. A single long-context
   vector per doc (or late-chunking) is the remaining dense idea for the *discrimination*
   subset; gate on the 1-bit binary floor. Treat as exploratory R&D, not a committed lever.
4. **Leave factoid alone.** ~0.90 is the lexical ceiling; content-OR/BM25 wins it and
   the vector arm adds ~nothing.
5. **Do NOT** swap to a stronger chunked dense embedder (§6: refuted), chase whole-doc
   *chunk geometry* (IR-C full-corpus: deferred), or adopt a non-binary-safe model
   (ColBERT / large API) — closed on the evidence or the footprint.

## 6. The "stronger model" lever — TESTED and REFUTED (Nomic A/B)

After this benchmark, FathomDB ran the Phase-2 swap on the same dense diagnostic
(`dev/plans/runs/IR-C-retrieval-findings.md`, commit `d4aace9`):

| model (dense, 128/96 chunks) | exploratory median rank | exploratory top-50 | exact_fact |
|---|---|---|---|
| bge-small-en-v1.5 (33M, MTEB 51.7) | 99 | 37% | baseline |
| **nomic-embed-text-v1.5 (137M, MTEB 62.3)** | **135 (worse)** | **32% (worse)** | +6 pts (already solved) |

A model **+10.6 MTEB points and ~4× the size made exploratory *worse***, at ~2×
compute. With the earlier chunk-geometry and CLS-pooling A/Bs, that is **three
independent dense-quality experiments, all failing** → exploratory is a **structural**
limit of chunk-based single-vector dense retrieval for discourse/summary queries, not
a model-capacity problem. BM25 (median rank 26) is the stronger exploratory component;
the dense investigation is **closed under current (chunked) knobs**. The single
remaining dense idea is whole-doc long-context (§5.3), which none of the three tests
exercised.

**Why reranking/graph are bounded (the headroom math — CORRECTED, see note).**
A reranker can only *reorder* the candidate pool, so its ceiling is the candidate
recall **at the depth you rerank** — and that depth is a free, latency-bounded knob.
At depth 50, fused R@50 ≈ 0.53 and the dense+lexical oracle union ≈ 0.62; but the
**lexical arm's found@1000 ≈ 0.86** (`IR-C-retrieval-findings.md`), so reranking a
deeper pool raises the reachable ceiling well past 0.62 (log-interpolating: ≈0.61 @100,
≈0.68 @200, ≈0.78 @500 — INFERRED, must be measured). So exploratory R@10 0.307 → a
plausible ~0.40–0.47 with a depth-100–200 cross-encoder, not capped at ~0.53.

The **truly reorder-proof core is ~14%** (lexically unreachable even at depth 1000),
**not the ~38%** — that 38% is the *depth-50* "gold in neither arm's top-50" artifact,
and part of it is plausibly single-gold **label noise** (the exploratory qrels are
single-doc/sparse, deflating measured R@K for every system). **Graph is therefore NOT
"the only lever past ~0.62"** (deep reranking passes it lexically); graph is the lever
for (a) the ~14% lexically-unreachable stratum and (b) multi-hop/temporal/knowledge-
update classes the current Recall@K instrument barely contains — and that payoff is
**mostly invisible to Recall@K**, so it can only be seen on an end-to-end memory eval.

> **Provenance of this correction:** the original paragraph capped the ceiling at the
> depth-50 numbers and called ~38% irreducible. The Fable-5 roadmap review (§1.2 C1–C3
> of `dev/plans/runs/IR-C-roadmap.md`) showed the ceiling is depth-conditional, the
> reorder-proof core is ~14%, and graph's value is largely off-instrument. The deeper-
> depth recall figures are INFERRED and gated on measurement (roadmap R0).

## 7. Peer benchmark numbers (context — NOT comparable to our Recall@K)

Published agent-memory benchmark scores, for scale only. **These measure end-to-end
answer accuracy** (an LLM reads retrieved memory and answers; an LLM judge scores the
answer) on conversational benchmarks (LoCoMo, LongMemEval) — **a different metric and
corpus than our first-stage Recall@K**, so they do *not* translate to "N points higher
than FathomDB." They are also vendor-contested.

| system | LoCoMo | LongMemEval | note |
|---|---|---|---|
| Mem0 (own claims) | 92.5 | 94.4 | self-reported |
| Zep vs Mem0 (independent, GPT-4o) | — | **63.8% vs 49.0%** | Zep +14.8 pts |
| LoCoMo dispute | Zep 84% → Mem0 recompute **58.44%** → Zep **75.14%** | — | methodology contested |

**The load-bearing point:** peers do **not** win with bigger embedders — they use the
same small-model class (`text-embedding-3-small` / small open) and win on
**architecture** (hybrid + memory **graph** + reranking). Zep's edge over Mem0 is
explicitly its graph. This *independently reinforces* §5: the lever is rerank + graph,
not embedder scale. Sources: mem0.ai/blog/state-of-ai-agent-memory-2026 ·
blog.getzep.com (Mem0-SOTA critique) · github.com/getzep/zep-papers/issues/5 (84%→58.44%)
· arXiv 2504.19413 (Mem0) · atlan.com/know/zep-vs-mem0.

## Confidence & provenance
- **HIGH:** the expected-ceiling verdict (empirical anchor reproduces leaderboard +
  NFCorpus ≈ our exploratory; 3 papers agree on the discourse-hard pattern); pooling
  penalty ≈ 0; factoid at lexical ceiling; peers use hybrid+graph+reranking;
  **the "stronger dense model doesn't fix exploratory" finding (§6) is now a measured
  FathomDB result (Nomic A/B), not an inference** — it overrides the literature's
  in-the-abstract "long-context ~2× hard-recall" claim *for the chunked setup we use*.
- **MEDIUM / directional / superseded:** the literature's "~2×" long-context upgrade
  magnitude was vendor-self-reported on *long-document* benchmarks and is **superseded
  for our case** by the Nomic A/B (no exploratory gain when chunked); it may still hold
  for the untested whole-doc long-context angle (§5.3). Any single peer system's precise
  embedder choice is MEDIUM (Zep-BGE-M3 refuted). Peer LoCoMo/LongMemEval figures (§7)
  are vendor-contested and not metric-comparable to our Recall@K.
- **Method:** BEIR anchor = `mteb 2.15`, `BAAI/bge-small-en-v1.5`, SciFact/NFCorpus/
  ArguAna, CPU, both pooling configs (`IR-C-bge-small-beir-anchor.json`). Literature =
  21 sources, claims killed on ≥2/3 refute votes; refuted claims listed above are
  *excluded* from the verdict. Key sources: LongEmbed (2404.12096), small-model
  ceiling (2510.14880, 2505.19274), granite (2508.21085), nomic technical report,
  agent-memory (2501.13956, LongMemEval, Letta/memmachine LoCoMo), hybrid/graph
  (2504.19413).
