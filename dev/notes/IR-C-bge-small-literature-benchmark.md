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
to chase there. The two productive levers are (1) a **stronger long-context small
embedder** and (2) the **hybrid lexical+dense+graph architecture** that peer systems
already rely on. FathomDB is *already* on lever 2; the open move is lever 1.

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
- **Realistic upgrade: a long-context *small* embedder ≈ doubles hard-task recall in
  the same footprint.** granite-embedding-small-r2 scores ~61.9 vs ~32.1 for the
  short-context small baseline on the hard long-doc set (arXiv 2508.21085). Same
  param/footprint class, long-context architecture = the lever. [HIGH]
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
— and it constrains the upgrade path: the right lever is a **long-context small
embedder that survives 1-bit quantization**, not a large API model or late-interaction
reranker that would break the footprint.

## 5. Recommendation — do both, in this order

1. **Lever 1 (model): run the Phase-2 long-context small-embedder A/B** already
   scaffolded (`NomicEmbedder` + model-agnostic `ir_c_gold_diagnostics`). Measure
   nomic-embed-text-v1.5 (and ideally granite-small-r2 / gte-modernbert) on our exact
   dense diagnostic, on **both** axes: exploratory median-rank/recall **and** 1-bit
   binary-floor retention. Expected: a material exploratory lift in-footprint; gate on
   it actually clearing the binary floor (`ir_c_pooling_floor_gate`-style).
2. **Lever 2 (architecture): add cross-encoder reranking + graph-aware retrieval.**
   This is what peer memory systems use to turn a weak dense arm into good end-to-end
   recall, and it composes with our existing hybrid fusion + graph substrate. A small
   CPU cross-encoder reranker over the fused top-K is the highest-leverage
   architecture add that respects the local-first constraint.
3. **Leave factoid alone.** ~0.90 is the lexical ceiling; the content-OR/BM25 path
   already wins it and the vector arm adds ~nothing.
4. **Do not** chase whole-doc chunk geometry for exploratory (IR-C full-corpus result:
   deferred) or a non-binary-safe model (ColBERT / large API) — both break either the
   evidence or the footprint.

## Confidence & provenance
- **HIGH:** the expected-ceiling verdict (empirical anchor reproduces leaderboard +
  NFCorpus ≈ our exploratory; 3 papers agree on the discourse-hard pattern); pooling
  penalty ≈ 0; factoid at lexical ceiling; peers use hybrid+graph+reranking.
- **MEDIUM / directional:** the exact "~2×" upgrade magnitude (vendor self-reports;
  several specific deltas refuted in verification — trust direction, not the number);
  any single peer system's precise embedder choice (Zep-BGE-M3 refuted).
- **Method:** BEIR anchor = `mteb 2.15`, `BAAI/bge-small-en-v1.5`, SciFact/NFCorpus/
  ArguAna, CPU, both pooling configs (`IR-C-bge-small-beir-anchor.json`). Literature =
  21 sources, claims killed on ≥2/3 refute votes; refuted claims listed above are
  *excluded* from the verdict. Key sources: LongEmbed (2404.12096), small-model
  ceiling (2510.14880, 2505.19274), granite (2508.21085), nomic technical report,
  agent-memory (2501.13956, LongMemEval, Letta/memmachine LoCoMo), hybrid/graph
  (2504.19413).
