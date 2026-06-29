# Planner-router competitor & prior-art landscape

Research date: 2026-06-28. Author: research agent (web-sourced; field knowledge flagged inline as
*[field]*). Purpose: identify the comparison class for FathomDB's adaptive/routed retrieval
("planner-router") and pick the smallest credible head-to-head we can run on a corpus already on disk.

## What FathomDB's planner-router is (the comparison frame)

Intent classifier (~5 classes: needle/single-fact, multi-session, temporal, global-sensemaking,
multi-hop) → per-intent retrieval CONFIG over typed operators (BM25 / vector / RRF / cross-encoder
rerank / map-reduce) and their knobs (candidate_k, rerank pool, blend alpha, MMR, recency). Local-first
(SQLite + sqlite-vec/FTS5), CPU-default, optional caller-side LLM. Internal finding to pressure-test:
*static arm-selection added ~nothing over a strong RRF-fused hybrid; the value was per-intent config
tuning.* This puts FathomDB squarely in the **adaptive / routed RAG** class — but routing over
*retrieval configs/knobs* rather than over *models* or *whether-to-retrieve*, which is a real
differentiator (see caveats).

## (a) Competitor table — adaptive / routed retrieval

| System | Routes ON | Routes BETWEEN | Benchmarks reported | Code? |
|---|---|---|---|---|
| **Adaptive-RAG** (Jeong et al., NAACL 2024, arXiv:2403.14403) | learned query-complexity classifier (small LM, 3 classes A/B/C, auto-labeled from model outcomes) | no-retrieval / single-step retrieval / multi-step iterative | SQuAD, Natural Questions, TriviaQA (single-hop); **MuSiQue, HotpotQA, 2WikiMultiHopQA** (multi-hop). EM/F1/Acc + efficiency (steps, rel. time). **Reports an oracle-classifier upper bound.** | Yes (StonyBrookNLP-style; official repo public) |
| **Self-RAG** (Asai et al., ICLR 2024, arXiv:2310.11511) | LM-generated "reflection tokens" at decode time | adaptive on-demand retrieve vs. not + self-critique of passages/generations | open-domain QA, reasoning, fact verification (PopQA, PubHealth, ARC, bio long-form, ASQA) | Yes (selfrag.github.io) |
| **FLARE** (Jiang et al., EMNLP 2023, arXiv:2305.06983) | low-confidence tokens in the predicted next sentence | retrieve-now vs. keep-generating (active, iterative) | long-form knowledge QA: 2WikiMultiHop, ASQA, StrategyQA, WikiAsp | Yes (jzbjyb/FLARE) |
| **DRAGIN** (Su et al., ACL 2024, arXiv:2403.10081) | real-time info-need detection (RIND, self-attention) + query-from-attention (QFS) | when-to-retrieve / what-to-retrieve, training-free | 2WikiMultiHop, HotpotQA, IIRC, StrategyQA | Yes |
| **Self-Route** (Li et al., EMNLP 2024 industry, arXiv:2407.16833; Google DeepMind/UMich) | LLM self-reflection ("can I answer from RAG chunks?") | RAG vs. full long-context | long-context QA suites (∞Bench, etc.); ~65% cost cut at ~LC quality | Partial |
| **RouterRetriever** (Lee et al., 2024, arXiv:2409.02685) | per-query routing to a domain-expert embedding model (pilot-embedding gate) | a mixture of expert embedding models (no whether/how-much) | **BEIR** (+2.1 nDCG@10 vs MSMARCO single, +3.2 vs multi-task) | Yes |
| **RouterQueryEngine** (LlamaIndex) / query routing (LangChain) | LLM selector (LLM-text or Pydantic/function-call), single- or multi-route | named query engines / retrievers / data sources (e.g. summary-index vs vector-index) | none (framework, not a benchmarked system) | Yes (OSS framework) |
| **RAGRouter-Bench** (Wang et al., arXiv:2602.00296, Jan 2026; baseline study arXiv:2604.03455) | benchmark, not a system — labels queries by **3 canonical types: factual / reasoning / summarization** + corpus indicators | standardizes **5 RAG paradigms** for routing eval | **7,727 queries / 21,460 docs across 4 domains**; jointly scores generation quality + resource cost (LLM-as-judge). Lightweight baseline: TF-IDF+SVM = 0.928 macro-F1, ~28% token savings. | Dataset/bench (recent; verify license/availability) |

Notes: Adaptive-RAG, Self-RAG, FLARE, DRAGIN and Self-Route route on **whether/how much to retrieve or
which LLM path**; RouterRetriever routes on **which embedding model**. *None routes over per-intent
retrieval CONFIG/knobs the way FathomDB does* — the closest in spirit is Adaptive-RAG (intent→
strategy), but it selects pipeline depth, not arm-knobs.

## (b) Multi-hop / agentic retrieval baselines (for the `multi_hop` class)

| System | Mechanism | Benchmarks | Code? |
|---|---|---|---|
| **IRCoT** (Trivedi et al., ACL 2023, arXiv:2212.10509) | interleave retrieval with chain-of-thought, each step's retrieval guided by prior CoT | HotpotQA, 2WikiMultiHopQA, **MuSiQue**, IIRC; +up to 21 retrieval / +15 QA points | Yes (StonyBrookNLP/ircot) |
| **HippoRAG** (Gutiérrez et al., NeurIPS 2024, arXiv:2405.14831) | KG + Personalized PageRank ("neurobiological" associative memory) | **MuSiQue, 2WikiMultiHopQA, HotpotQA** | Yes |
| **HippoRAG 2** (2025) | dense+sparse KG (passage + phrase nodes), unified recall | MuSiQue F1 44.8→51.9, R@5 69.7→74.7; 2Wiki R 76.5→90.4 | Yes |
| **ReAct-retrieval** *[field]* | interleaved reason+act tool calls incl. search | HotpotQA, FEVER (original ReAct paper, Yao et al. 2023) | Yes |

This is the head-to-head we want for `multi_hop`; FathomDB's own graph arm is already refuted internally,
so the question is the *benchmark number*, not the mechanism. Note HippoRAG/IRCoT/Adaptive-RAG **all
converge on MuSiQue + 2WikiMultiHopQA + HotpotQA** — that triple is the de-facto multi-hop standard.

## (c) Shared-corpus apples-to-apples: which on-disk corpora the literature actually uses

| On-disk corpus | Used by routing/multi-hop lit? | Who |
|---|---|---|
| **MuSiQue** | **Yes — standard** | Adaptive-RAG, IRCoT, HippoRAG/2, DRAGIN-adjacent |
| **BEIR** (FiQA, Touché-2020, NFCorpus, ArguAna) | **Yes — standard for arm/retriever routing** | RouterRetriever (BEIR is its main bench); general dense/sparse retrieval |
| **2WikiMultiHopQA / HotpotQA** | Yes — standard multi-hop | Adaptive-RAG, IRCoT, HippoRAG, FLARE, DRAGIN (*not currently flagged as on-disk — would need acquisition*) |
| **LongMemEval** | Memory-RAG lit (not classic routing). Its **5 abilities (info-extraction, multi-session reasoning, temporal, knowledge-update, abstention) map almost 1:1 to FathomDB's intent classes** | LongMemEval paper; Mem0/agent-memory work |
| **LOCOMO** | Memory lit, not routing | Mem0, memory-agent papers; multi-session/temporal/multi-hop tags |
| **AP-News / BenchmarkQED** | Global-sensemaking lit, not routing | GraphRAG/QED line |

Apples-to-apples verdict: **MuSiQue** (multi-hop) and **BEIR** (arm/retriever routing) are the two
corpora we hold that are genuinely standard in this literature. LongMemEval/LOCOMO/AP-News are standard
in the *adjacent* memory/sensemaking literature and are where FathomDB's distinctive
multi_session/temporal/global classes live — but routing papers don't use them, so a head-to-head there
would be FathomDB-vs-memory-systems, not FathomDB-vs-routers.

## (d) Metrics & protocol the routing literature uses

- **End-task quality:** EM / F1 / Accuracy (QA); nDCG@10 / Recall@k (retrieval, e.g. BEIR).
- **Routing quality:** classification accuracy / macro-F1 of the router itself (Adaptive-RAG;
  RAGRouter-Bench TF-IDF+SVM 0.928 macro-F1).
- **Efficiency / cost-vs-quality Pareto:** avg steps per query, relative latency, token savings
  (Adaptive-RAG steps & rel-time; Self-Route ~65% cost cut; RAGRouter-Bench joint quality+resource).
- **Oracle routing ceiling:** **YES, this is a recognized device.** Adaptive-RAG reports
  "Adaptive-RAG w/ Oracle" (perfect classifier) — directly analogous to FathomDB's **Gate-2 oracle
  router ceiling**. This is the single strongest framing alignment: we can quote our oracle-vs-learned
  gap in the same terms Adaptive-RAG does.

## (e) The honest gap + recommended head-to-head

**Is there a standard router benchmark?** Until ~2026, no — routing was evaluated ad-hoc, each paper on
its own dataset mix (Adaptive-RAG's 6-dataset suite became the closest informal standard).
**RAGRouter-Bench (arXiv:2602.00296, Jan 2026) is the first purpose-built one** — but it is brand-new,
uses a *3-type* taxonomy (factual/reasoning/summarization) that only partially overlaps FathomDB's
5 classes, and its availability/license should be verified before relying on it.

### Recommended single head-to-head (run now)

**Adaptive-RAG on MuSiQue, EM/F1 + steps-per-query, with the oracle-router ceiling reported alongside.**

- **Why this one:** Adaptive-RAG is the closest conceptual competitor (intent→strategy routing), its
  code and the MuSiQue corpus are both in hand, MuSiQue is the multi-hop standard shared by every
  baseline (IRCoT, HippoRAG), and it *already reports an oracle ceiling* so our Gate-2 number is
  directly comparable. This lets us state the result in the field's own vocabulary: "learned-router vs
  oracle-router gap, EM/F1 and cost, vs Adaptive-RAG on MuSiQue."
- **Metric:** EM / F1 (answer), router accuracy, avg retrieval steps / latency (the cost axis), plus
  oracle-vs-learned delta.
- **Effort:** *Medium.* MuSiQue + Adaptive-RAG public code are available; main work is harnessing
  Adaptive-RAG's iterative path against our local-first SUT and normalizing the answer-generation step
  (Adaptive-RAG assumes a strong generator LLM — use our optional caller-side LLM to keep it fair).
  Risk: Adaptive-RAG's reported numbers use a large hosted LLM; reproduce its baseline locally rather
  than quoting paper numbers, or the comparison drifts.

**Second, cheaper head-to-head (arm-routing, no LLM needed):** RouterRetriever-style arm/retriever
routing on **BEIR (FiQA/Touché-2020/NFCorpus/ArguAna)**, metric **nDCG@10 / Recall@k**. This directly
tests our internal "static arm-selection ≈ RRF" finding against the published "+2.1 nDCG@10 from
routing" claim — pure-retrieval, CPU-only, *low effort*, and the cleanest way to confirm/refute our
core thesis without an LLM in the loop.

### Caveats / things to flag

- **Routing-target mismatch:** competitors route over *retrieve-or-not / which-LLM / which-embedder*;
  FathomDB routes over *per-intent retrieval config*. Head-to-heads measure end-task quality+cost, not
  "same routing decision" — frame as capability comparison, not mechanism parity.
- **No clean standard benchmark** until RAGRouter-Bench (2026, unverified availability); Adaptive-RAG's
  6-dataset suite is the practical standard.
- **Version/generator drift:** most reported numbers depend on a specific (often large, hosted) LLM
  generation step — reproduce baselines locally for fairness given FathomDB's CPU-default, optional-LLM
  posture; do not quote paper headline numbers as if comparable.
- **Our distinctive classes (multi_session/temporal/global-sensemaking)** have NO routing-literature
  benchmark — they live in the memory (LongMemEval/LOCOMO) and sensemaking (AP-News/BenchmarkQED)
  literatures. A router head-to-head can only cover needle + multi_hop apples-to-apples.
- **Unverified:** exact RAGRouter-Bench corpora/paradigm list and code license (abstract only);
  HippoRAG-2 exact venue/date; ReAct-retrieval details are *[field]* knowledge.

## Sources

- Adaptive-RAG: https://arxiv.org/abs/2403.14403 / https://arxiv.org/html/2403.14403v2 (NAACL 2024)
- Self-RAG: https://arxiv.org/abs/2310.11511 (ICLR 2024) / https://selfrag.github.io/
- FLARE: https://arxiv.org/abs/2305.06983 (EMNLP 2023) / https://github.com/jzbjyb/FLARE
- DRAGIN: https://arxiv.org/abs/2403.10081 (ACL 2024)
- Self-Route (RAG vs Long-Context): https://arxiv.org/abs/2407.16833 / https://aclanthology.org/2024.emnlp-industry.66/
- RouterRetriever: https://arxiv.org/abs/2409.02685
- RAGRouter-Bench: https://arxiv.org/abs/2602.00296 ; baseline study https://arxiv.org/abs/2604.03455
- IRCoT: https://arxiv.org/abs/2212.10509 (ACL 2023) / https://github.com/StonyBrookNLP/ircot
- HippoRAG: https://arxiv.org/abs/2405.14831 (NeurIPS 2024); HippoRAG 2: https://www.emergentmind.com/topics/hipporag-2
- LlamaIndex routers: https://docs.llamaindex.ai/en/stable/module_guides/querying/router/
- LongMemEval / LOCOMO: https://www.emergentmind.com/topics/locomo-and-longmemeval-_s-benchmarks
