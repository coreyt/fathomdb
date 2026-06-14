# IR-C — R2 end-to-end parity eval results (Slice 25)

> Produced by the R2 harness (`src/python/eval/r2_parity_eval.py`) on the frozen
> corpus. Binding design: `dev/adr/ADR-0.8.1-ir-measure-eval-design.md §3`. Memo:
> `dev/design/slice-25-r2-design.md`. Machine-readable run artifact:
> `dev/plans/runs/0.8.1-slice-25-r2-run.json`.

**Status: DATA-LIMITED (retrieval-only).** No answerer LLM and no local Mem0-OSS
backend were available, and the frozen FathomDB corpus carries no
temporal/multi_hop/knowledge_update/multi_session gold. The live run therefore
reports **Evidence Recall@K** for FathomDB vs naive-RAG on the two classes the
frozen gold labels (`factoid`, `exploratory`); the three R3 go/no-go classes are
**null (no data)**, not zero. See **§4 Blockers** and **§5 Consequence for Slice 30**.

## 1. Run provenance (replay pins)

| Field | Value |
|-------|-------|
| Corpus snapshot hash | `fe973fcd49fbbda083158f69fe720f17858ab8528e171fa2188eec84131c7d4e` (COR-2 `--reproduce`: **MATCH**, bit-identical) |
| Gold set | `data/corpus-data/eval/ir_gold/all.gold.json`, `qrels_version = ir-c-reused-v1`, 4 597 queries |
| K | 10 (Evidence Recall@K) |
| Systems run | `fathomdb` (lexical/FTS arm — see §3 caveat), `naive_rag` (pure-Python BM25) |
| Answerer model | **unavailable** (no LLM in env; see §4) → `answerer_accuracy` / `abstention_rate` = null |
| Mem0-OSS | **unavailable** (no local backend; see §4) |
| Identical-answerer constraint | structurally enforced (adapters expose only `retrieve()`; the harness owns the one `Answerer`); not exercised live (no answerer) |

## 2. Per-class Evidence Recall@10 (primary metric)

| Class | N | FathomDB | naive-RAG | Mem0-OSS | FathomDB − naive | FathomDB − Mem0 |
|-------|--:|---------:|----------:|---------:|-----------------:|----------------:|
| **factoid** (`exact_fact`) | 2 888 | **0.8999** | 0.8982 | — (null) | **+0.0017** | null |
| **exploratory** | 1 584 | **0.3270** | 0.3561 | — (null) | **−0.0291** | null |
| **temporal** | 0 | null | null | null | null | null |
| **multi_hop** | 0 | null | null | null | null | null |
| **knowledge_update** | 0 | null | null | null | null | null |
| **multi_session** | 0 | null | null | null | null | null |
| (report-only) negative | 125 | null† | null† | null | — | — |

† the negative class has no required evidence, so Evidence Recall@K is undefined for
it; it is scored only for abstention/false-positive (which needs an answerer — not
run here).

**Answerer accuracy (end-to-end):** not measured — no answerer LLM, and the frozen
gold carries no answer strings (`answer_type`/`answers` are absent on the reuse
tier). The harness emits these as `null`, never `0.0`.

**Sanity check.** FathomDB `factoid` R@10 = 0.8999 is consistent with the R0 anchor
(`exact_fact` R@10 ≈ 0.905 at depth-50 fused; `IR-C-recall-cdf.json`) — expected,
since both measure lexical retrieval of the same gold docs.

## 3. The load-bearing caveat — FathomDB ran lexical-only

The FathomDB adapter opened the engine with `use_default_embedder=False`, so only
the **FTS5 lexical arm** ran (no neural embedder → no dense arm, no RRF fusion with
a vector arm). This makes the FathomDB-vs-naive-RAG comparison a **lexical-vs-lexical**
one — near-parity by construction (both are BM25-family). The small deltas reflect
FathomDB's FTS5 tokenizer + write-cursor RRF ordering vs the harness's plain BM25,
not the production fused pipeline.

The full production arm (1-bit dense KNN + f32 rerank fused with FTS, the R1
cross-encoder, and the R3 graph arm) needs the BGE embedder weights, which were not
available in this environment. Re-running with `use_default_embedder=True` (BGE
weights cached) — no harness code change — exercises the fused arm.

## 4. Blockers (what is needed to complete R2)

| id | What is blocked | What is needed |
|----|-----------------|----------------|
| `answerer-llm-unavailable` | per-class **answerer accuracy** + abstention | a BYO/local answerer LLM: set `R2_ANSWERER_BASE_URL` + `R2_ANSWERER_MODEL` + `R2_RUN=1` (OpenAI-compatible shim, e.g. local llama.cpp/ollama). The `LLMAnswerer` path is implemented and gated; only the env flips. |
| `mem0-oss-unavailable` | the **Mem0-OSS baseline** arm | `pip install mem0ai` **and** a configured local LLM + embedding backend (Mem0's `add()` extraction + `search()` embedding both require a model; cloud Mem0 is forbidden by ADR §3.6). The `Mem0OSSAdapter` is implemented (lazy import) and inert until both exist. |
| (corpus) memory-class gold absent | `temporal`/`multi_hop`/`knowledge_update`/`multi_session` rows | the frozen FathomDB gold has only `exact_fact`/`exploratory`/`negative`. The memory classes require **LongMemEval** (its own conversational haystack + an answerer; not locally cloneable here) or memory-class QA pairs generated from the corpus with an offline LLM (dev-time). The harness consumes either via the same `R2Harness`. |
| (engine) embedder absent | FathomDB **dense + fused** arm | BGE weights (`use_default_embedder=True`); see §3. |

## 5. Consequence for the Slice 30 (R3) go/no-go

Slice 30's go/no-go (PREP doc / `IR-C-roadmap.md §C3`) reads the
`temporal`/`multi_hop`/`knowledge_update` deltas to decide whether the graph arm
lifts those classes without regressing `factoid`/`exploratory`. **Those three
classes are null here (no gold, no answerer).** Therefore:

- The go/no-go is **data-limited** → per prompt §11 this is a **HITL escalation**,
  not an automatic decision. Slice 30 MUST NOT flip `use_graph_arm` to `true` on the
  strength of absent data.
- What the run *does* establish: on the frozen corpus's lexical retrieval, FathomDB
  and naive-RAG are at near-parity on `factoid` (+0.0017) and FathomDB is marginally
  behind on `exploratory` (−0.0291) — consistent with the roadmap's C3 thesis that
  graph's payoff is **off the Recall@K axis** and invisible to this corpus.
- The harness is the durable deliverable: a box with LongMemEval + a local answerer
  fills the memory-class rows with **no code change**, at which point the go/no-go
  has real data.

## 6. Reproduce

```bash
# COR-2: confirm the corpus is the frozen snapshot (must print "corpus_hash MATCH")
python3 tests/corpus/scripts/freeze_corpus.py --reproduce tests/corpus/snapshot.json

# Retrieval-only run (this result). With a local answerer, also set
# R2_RUN=1 R2_ANSWERER_BASE_URL=... R2_ANSWERER_MODEL=... to add the accuracy column.
FATHOMDB_CORPUS_DIR=data/corpus-data \
  python -m eval.r2_parity_eval --corpus-hash fe973fcd --k 10 \
  --output dev/plans/runs/0.8.1-slice-25-r2-run.json
```
