# FathomDB data-corpus map

**Purpose.** Connect every FathomDB user-need and feature/function to the
well-respected dataset(s) that can TEST it — grounded in what is already on disk,
and pointing at how to acquire what is not. This is the durable MAP; the work that
produced it and the open gaps are in
[`corpus-search-ledger.md`](./corpus-search-ledger.md), and the next search cycle
is driven by [`corpus-search-cycle-PROMPT.md`](./corpus-search-cycle-PROMPT.md).
Re-inventory on-disk state any time with
[`enumerate-corpora.sh`](./enumerate-corpora.sh).

**Status:** v1 · **Date:** 2026-06-28 · base `origin/main` @ `db34820c`.

## How to read this map

- **Status tags** in the *Corpus Name* column:
  - **[ON-DISK]** — payload already acquired locally (see *Local Storage Point*).
  - **[HF-STREAM]** — pulled from HuggingFace at run-time and cached; not a static
    committed file.
  - **[CANDIDATE]** — well-respected dataset that tests this need but is **not yet
    acquired** (acquisition instructions given).
  - **[COMPARATOR]** — a competitor *system* (not a corpus); runs over one of the
    corpora to produce a head-to-head number.
- **Local Storage Point** paths are repo-relative under `data/corpus-data/`, which
  is **`.gitignored`** — payloads are physically present in the primary checkout
  (`/home/coreyt/projects/fathomdb/data/corpus-data/`) but are **never committed**.
  Acquisition scripts in `tests/corpus/scripts/acquire_*.py` are the reproducible
  source of truth.
- **Licensing rule (hard):** corpora marked **EVAL-ONLY / no-redistribute** (LOCOMO,
  AP-News) MUST stay under the gitignored data dir; never commit their payloads,
  never ship them in the library. "commit-eligible" notes only mean the *license*
  would permit it — the project default is still "scripts in git, data out of git".

---

## A. Agentic long-term memory (end-to-end: store long multi-session history → recall → answer)

Competitor: **Mem0 / Zep**. The defining FathomDB capability (capability-status-report §1).

| User Need | Feature/Function | Corpus Name | Corpus Description | Local Storage Point | Acquisition Instructions (+ license / redistributability) |
|---|---|---|---|---|---|
| Agentic long-term memory (overall) | end-to-end pipeline; identical-answerer protocol | **LongMemEval** [HF-STREAM] | 500 expert-written questions over long chat histories (~115k tok / ~40 sessions in `_S`; ~500 sessions in `_M`); 5 abilities: info-extraction, multi-session, temporal, knowledge-update, abstention. ICLR 2025 (arXiv:2410.10813). | streamed + cached (HF `xiaowu0162/longmemeval-cleaned`, `oracle` split); loaded via `src/python/eval/gold_repin.py::load_lme` / `src/python/eval/p0a_base_retrieval.py::load_lme_smoke` | `datasets.load_dataset("xiaowu0162/longmemeval-cleaned")`. License: dataset card does not state a clear SPDX license — treat as **research-use, verify before any redistribution**; keep cached under the gitignored data dir. |
| Agentic long-term memory (overall) | end-to-end; second real source for multi_session/temporal power | **LOCOMO** [ON-DISK] | 10 very-long multi-session conversations, 1,986 QA; categories {1 multi-hop, 2 temporal, 3 open-domain, 4 single-hop, 5 adversarial}. Maharana et al. 2024, ACL (arXiv:2402.17753). FathomDB maps cat 4→factoid, 2→temporal, 1→multi_session; 3/5 excluded. | `data/corpus-data/raw/locomo10.json` (+ `locomo10.LICENSE.txt`); memory gold `data/corpus-data/eval/0.8.3-locomo-memory-gold.json`; loader `src/python/eval/locomo_loader.py` | `tests/corpus/scripts/acquire_locomo.py` (pulls `snap-research/locomo` `data/locomo10.json`). License **CC-BY-NC-4.0 — NON-COMMERCIAL, EVAL-ONLY**; gitignored, NEVER commit/ship. |
| Agentic long-term memory (overall) | competitor stand-up (the head-to-head number) | **Mem0-OSS** [COMPARATOR] | Open-source agentic-memory system; the primary parity target. Runs over LongMemEval/LOCOMO to produce identical-answerer accuracy deltas. (Arm was BLOCKED in Slice 25 — backend unavailable; D0b later produced the gap number.) | not a corpus — `eval/mem0_local.py` adapter; runs on the two corpora above | `pip install mem0ai` (backend). Verify a working local backend before any priced run. |
| Agentic long-term memory (overall) | personal-corpus extraction (ELPS) golden | **memex-ELPS golden** [ON-DISK] | Cross-repo "Memex" personal-corpus extraction golden (entity/edge labels) used to validate the ELPS two-pass ingest. | `data/corpus-data/external/memex-elps/*.jsonl` | External artifact (Memex repo); EVAL-ONLY, gitignored. Treat as project-internal; do not redistribute. |
| Multi-session reasoning | dense/FTS recall, coverage-node expand (D2-as-router), wider candidate-gen | **LongMemEval `multi_session`** [HF-STREAM] / **LOCOMO multi-hop** [ON-DISK] | The cross-session synthesis class — FathomDB's weakest (R@10 ~0.20–0.30); the recall-bound surpass-blocker. | as above | as above. |
| Multi-session reasoning | candidate for a dedicated multi-session corpus | **Multi-Session Chat (MSC)** [CANDIDATE] | Facebook/Meta multi-session open-domain dialogue (persona continuity across up to 5 sessions). Tests session-linking recall distinct from QA gold. | — | HF `nayohan/multi_session_chat` (re-host) or ParlAI `msc`. Verify license (ParlAI is MIT; underlying data terms vary) before acquiring. |
| Temporal reasoning | leaf + valid-time filter; recency judgment | **LongMemEval `temporal`** [HF-STREAM] / **LOCOMO temporal (cat 2)** [ON-DISK] | Time-aware questions ("what did I do *before* X"); requires ordering/valid-time over sessions. | as above | as above. |
| Knowledge-update (ku) | bi-temporal edges + supersession (G0 logical_id); consolidation/recency provider (OPP-2) | **LongMemEval `knowledge-update`** [HF-STREAM] | Recognize that a user fact CHANGED and answer with the *latest* value (78 q). The canonical agent-memory ku signal; FathomDB trails Mem0 −0.27 here. | as above | as above. |
| Knowledge-update (ku) | candidate for explicit fact-edit / supersession | **MQuAKE** [CANDIDATE] | Multi-hop knowledge-EDITING benchmark (counterfactual fact updates + ripple-effect multi-hop). Tests supersession/contradiction handling more sharply than LongMemEval's 78 q. | — | HF `henryzhongsc/MQuAKE` / GH `princeton-nlp/MQuAKE`. License MIT (verify). Note: built for *model editing*; use only the fact-update structure for the retrieval-memory angle. |

---

## B. Information-retrieval relevance (first-stage retrieval & ranking, LLM-free)

Competitor/floor: **strong BM25**. Capability-status-report §2.

| User Need | Feature/Function | Corpus Name | Corpus Description | Local Storage Point | Acquisition Instructions (+ license / redistributability) |
|---|---|---|---|---|---|
| Fact retrieval (needle / factoid recall@k) | FTS5, dense (bge bi-encoder), RRF fusion | **FathomDB IR gold (eu8)** [ON-DISK] | FathomDB's own IR-relevance corpus: 301 labeled queries over the ~10k 0.7.0 test corpus + 200 chains; recall@10 ceiling ≈0.571 (embedder-bound, report-only). Per-source gold for enronqa/qaconv/qmsum. | `data/corpus-data/eval/ir_gold/all.gold.json` (+ per-source `*.gold.json`); built by `tests/corpus/scripts/build_ir_gold.py` | Rebuild from the 0.7.0 test corpus (`acquire_*.py` → `build_ir_gold.py`). Gold is project-authored; corpus payloads inherit per-source licenses (below). |
| Fact retrieval (needle / factoid recall@k) | BM25 / dense / rerank — canonical zero-shot IR | **BEIR** [CANDIDATE] | 18-dataset heterogeneous zero-shot IR benchmark (NQ, FiQA, SciFact, TREC-COVID, ArguAna, Quora, NFCorpus, …); the standard nDCG@10 leaderboard where hybrid ties/edges single methods and BM25 is a strong OOD floor — exactly FathomDB's published pattern. arXiv:2104.08663. | — | HF `BeIR/*` (e.g. `BeIR/scifact`, `BeIR/nfcorpus`, `BeIR/trec-covid`) or `pip install beir`. **Per-dataset licenses vary** — BEIR only repackages format; verify each subset's license/owner before use. Start with small commit-clean subsets (SciFact CC-BY, NFCorpus). |
| Single-hop QA (fact lookup) | base retrieval + reader | **Natural Questions / NQ-Open** [CANDIDATE] | Real Google queries + Wikipedia answers; NQ-Open = 79k/8.7k/3.6k open-domain QA. The canonical single-hop open-domain QA benchmark. | — | HF `google-research-datasets/natural_questions` (full) or `nq_open` (open-domain). License **CC-BY-SA-3.0**. Commit-eligible with attribution; large — prefer NQ-Open or a sampled slice. |
| Single-hop QA (fact lookup) | base retrieval + reader | **TriviaQA** [CANDIDATE] | 95k QA pairs + ~650k QA-evidence triples (trivia questions, Wikipedia + web evidence). Strong distant-supervision single-hop QA. arXiv:1705.03551. | — | HF `mandarjoshi/trivia_qa`. License: Apache-2.0 on the annotations; **UW disclaims copyright over the underlying questions/docs** — verify before redistribution. |
| Single-hop QA / machine reading | reader fidelity, answer-span | **SQuAD 1.1 / 2.0** [CANDIDATE] | 107k crowd QA over 536 Wikipedia articles (1.1); 2.0 adds 53k unanswerable (abstention probe). Canonical reading-comprehension / answerability. | — | HF `rajpurkar/squad`, `rajpurkar/squad_v2`. License **CC-BY-SA-4.0**. Commit-eligible with attribution. SQuAD 2.0's unanswerable set doubles as an abstention test. |
| Discovery-in-k / exploratory recall@k | dense recall over discourse queries; index-key enrichment | **FathomDB chains + eu8 `exploratory`** [ON-DISK] | Exploratory = discourse/relational queries ("what did we decide about X"); FathomDB's structurally hard class (dense median gold rank ~99). 200 synthetic cross-doc chains over real anchors w/ relation-typed gold queries. | `tests/corpus/chains/*.json` (committed) + `data/corpus-data/raw/chain_connectives.jsonl` | Chains committed in git; connectives via `tests/corpus/scripts/generate_chain_corpus.py`. Project license. |
| Discovery-in-k / exploratory recall@k | candidate broad-topic / argument retrieval | **BEIR ArguAna / Touché / TREC-COVID** [CANDIDATE] | Argument- and topic-level retrieval subsets of BEIR — closest public analogue to "exploratory / discovery" relevance (non-factoid, relational). | — | HF `BeIR/arguana`, `BeIR/webis-touche2020`, `BeIR/trec-covid`. Per-subset licenses vary (verify). |

---

## C. Deep / multi-hop retrieval & reasoning

Competitor: **HippoRAG-2** (unmeasured). Capability-status-report §4.

| User Need | Feature/Function | Corpus Name | Corpus Description | Local Storage Point | Acquisition Instructions (+ license / redistributability) |
|---|---|---|---|---|---|
| Multi-hop QA (2–4 hop) | lexically-seeded PPR graph-fusion (refuted); passage_dense; fused-RRF | **MuSiQue (-Answerable)** [ON-DISK] | ~25k connected 2–4-hop questions composed from single-hop pairs (minimal shortcut leakage); FathomDB's registered multi-hop harness (n=300, ≥3-hop cell). TACL 2022 (arXiv:2108.00573). | `data/corpus-data/raw/musique_dev.jsonl` (4,834 rows); graph cache `data/corpus-data/graph-cache/0.8.2-m1-v1/`; loaders `src/python/eval/m1_*.py` | `tests/corpus/scripts/acquire_musique.py` (HF `bdsaglam/musique`, re-hosts canonical v1.0). License **CC-BY-4.0** — commit-eligible with attribution (kept under gitignored data dir by default). |
| Multi-hop QA (2-hop, comparison) | bridge-set retrieval; graph evidence-linking | **HotpotQA** [CANDIDATE] | 113k 2-hop Wikipedia QA with sentence-level supporting facts; distractor (2 gold + 8 distractors) & fullwiki settings. The most-cited multi-hop benchmark; HippoRAG reports passage R@5 here. arXiv:1809.09600. | — | HF `hotpotqa/hotpot_qa` (configs `distractor`, `fullwiki`). License **CC-BY-SA-4.0**. Commit-eligible with attribution. |
| Multi-hop QA (up to 5-hop, structured) | deep-hop bridging; comparison/inference templates | **2WikiMultihopQA** [CANDIDATE] | 192k Wikipedia+Wikidata multi-hop questions (up to 5 hops), template-built to block shortcuts, with evidence paths. HippoRAG reports R@5 ~90% here. arXiv:2011.01060. | — | HF `framolfese/2WikiMultihopQA` (HotpotQA-style fields). License **Apache-2.0** (original). Commit-eligible. |
| Multi-query Q&A (decompose → retrieve → combine) | iterative retrieval (caller-side); `search_expand` (G6) | **MultiHop-RAG** [CANDIDATE] | RAG-oriented multi-hop benchmark over a news corpus with queries needing evidence from multiple documents; designed to stress retrieve-then-reason RAG (vs single-passage QA). EMNLP 2024. | — | HF `yixuantt/MultiHopRAG` / GH `yixuantt/MultiHop-RAG`. License: check repo (CC-BY-NC reported on some mirrors) — verify before redistribution. |

---

## D. Sensemaking (global query-focused summarization)

Competitor: **Microsoft GraphRAG**. Capability-status-report (portfolio M6); 0.8.4 track.

| User Need | Feature/Function | Corpus Name | Corpus Description | Local Storage Point | Acquisition Instructions (+ license / redistributability) |
|---|---|---|---|---|---|
| Sensemaking (global QFS) | map-reduce QFS over all chunks (C); depth-1 coverage index (D2) | **AP-News (BenchmarkQED)** [ON-DISK] | 1,397 health AP-news articles + AutoQ generated global/local questions (v1 text, v2 with assertions); the corpus the 0.8.4 GraphRAG head-to-head + AutoQ/AutoE harness names. Microsoft Research, 2025. | `data/corpus-data/raw/apnews_benchmarkqed/` (`raw_data.zip`, `generated_questions_v1/`, `v2/`, `LICENSE`) | `tests/corpus/scripts/acquire_apnews_benchmarkqed.py` (GH `microsoft/benchmark-qed`). License **Microsoft Research License — NON-COMMERCIAL, research-only, explicitly NON-REDISTRIBUTABLE, EVAL-ONLY**; gitignored, NEVER commit/ship/modify-and-share. |
| Sensemaking (global QFS) | competitor stand-up | **Microsoft GraphRAG 3.x** [COMPARATOR] | The reference graph-community-summary sensemaking system; full-strength (level-1) head-to-head comparator. | not a corpus — workspace cache at `data/corpus-data/0.8.4-graphrag-artifacts/microsoft-graphrag-workspace/` | `pip install graphrag` + index over AP-News. Build cost ~$1–3 per index (metered); see airlock operational notes. |
| Sensemaking (global QFS) | candidate alternative QFS corpora | **GraphRAG Podcast / News corpora** [CANDIDATE] | The two original "Local-to-Global" GraphRAG test corpora: podcast transcripts (~1M tok) and news articles (~1.7M tok), with synthesized global sensemaking questions. arXiv:2404.16130. | — | GH `microsoft/graphrag` (sample datasets). Underlying transcripts/news have their own source licenses — verify; the AutoQ question-gen method is the reusable part. |

---

## E. Answer synthesis (end-to-end generative QA / reader bottleneck)

Capability-status-report §5. No dedicated corpus — answer synthesis is measured **on top of** the retrieval corpora above (LongMemEval, MuSiQue) by swapping readers on fixed retrieval. Listed here so the need is visible in the map.

| User Need | Feature/Function | Corpus Name | Corpus Description | Local Storage Point | Acquisition Instructions (+ license / redistributability) |
|---|---|---|---|---|---|
| Single-shot Q&A (answer from retrieved context) | reader pass-through; abstention scoring | **LongMemEval / MuSiQue** (reused) [ON-DISK/HF] | Answer accuracy measured on the same retrieval gold by swapping the reader (gemini-3.1-pro vs nano → ~5×). Reader, not a new corpus, is the variable. | as in §A / §C | as in §A / §C. |
| Single-shot Q&A — abstention / unanswerable | confident-wrong guard | **SQuAD 2.0** [CANDIDATE] | 53k unanswerable questions — a clean abstention/confident-wrong probe FathomDB currently lacks (M1 graph is answerable-only). | — | HF `rajpurkar/squad_v2`. CC-BY-SA-4.0. |

---

## F. Retrieval fidelity & embedding integrity (system health, NOT relevance)

Capability-status-report §6. No external comparator — these are self-corpora.

| User Need | Feature/Function | Corpus Name | Corpus Description | Local Storage Point | Acquisition Instructions (+ license / redistributability) |
|---|---|---|---|---|---|
| Retrieval fidelity (ANN vs exact-f32) | 1-bit sign-quant K=192 + f32 rerank; recall-floor gate (≥0.90) | **FathomDB eu7** [ON-DISK] | FathomDB's own ANN-fidelity corpus: ~7,667 real bge-small docs, 100 queries; recall@10 vs exact-f32 GT (0.896 measured, floor 0.90 one-sided CI). Built from the 0.7.0 test corpus. | derived from `tests/corpus/` + `data/corpus-data/raw/*` via eu7 harness (`src/rust/crates/fathomdb-engine/tests/support/recall_gate.rs`; eval `research/eu-0/`*) | Rebuild the test corpus (`acquire_*.py`) then the eu7 harness. *`research/eu-0/` is untracked/git-ignored — see experiments-ledger. Corpus payloads inherit per-source licenses. |
| Embed-completeness (is the dense index actually built?) | per-kind vector attribution JOINed to live nodes | **two real partial-embed DBs** [ON-DISK, ephemeral] | `verify_embed_db.py` validation targets: a 0%-coverage DB (doc kind never registered) and a ~7%-coverage DB (all vectors `edge_fact`). System-health check, not relevance. | ephemeral test DBs (e.g. `/tmp/r2-lme-*.sqlite`); verifier `src/python/eval/verify_embed_db.py`, tests `src/python/tests/test_verify_embed_db.py` | Regenerated by the eval harness; not a stored corpus. |

---

## G. Engine performance (read/scan latency & concurrency)

Capability-status-report §7. Synthetic, no external comparator.

| User Need | Feature/Function | Corpus Name | Corpus Description | Local Storage Point | Acquisition Instructions (+ license / redistributability) |
|---|---|---|---|---|---|
| Engine performance (latency/concurrency) | PCACHE2 + LIMIT pushdown; PRAGMA sweep; AC012/013/020 | **synthetic N=1M / 100k rows** [generated] | Generated row sets at n=1M (single-read AC012), 100k (scan AC013), and concurrency harness (AC020). No semantic content — pure operational load. | generated at run-time by the perf harness (`scripts/perf-experiments/`) | Generated; no acquisition. |

---

## H. Standalone features / functions (User Need = "—")

Mechanisms that don't map cleanly to a single user-need but still need a test corpus. (Per HITL: these get rows too.)

| User Need | Feature/Function | Corpus Name | Corpus Description | Local Storage Point | Acquisition Instructions (+ license / redistributability) |
|---|---|---|---|---|---|
| — | **BM25** (lexical floor) | BEIR / MS MARCO [CANDIDATE]; eu8 IR gold [ON-DISK] | BM25 is the standing baseline FathomDB must tie/beat; BEIR is its canonical zero-shot home. | `data/corpus-data/eval/ir_gold/` | BEIR per above; MS MARCO below. |
| — | **FTS5** (SQLite tokenizer / ranking; `b` param) | eu8 IR gold + chains [ON-DISK] | FTS5 tokenizer quality + length-norm (`b`) are tested on the FathomDB IR gold; porter vs unicode61 FTS-quality 8/8 vs 5/8 (0.8.0 B2). | `data/corpus-data/eval/ir_gold/`, `tests/corpus/chains/` | Rebuild from test corpus. |
| — | **Dense / vector recall** (bge-small bi-encoder, ANN) | eu8 (relevance) [ON-DISK] + BEIR [CANDIDATE] | Dense arm; ceiling ≈0.571 (eu8). BEIR gives the cross-domain dense-vs-BM25 picture. | `data/corpus-data/eval/ir_gold/` | as above. |
| — | **RRF hybrid fusion** | eu8 + LongMemEval [ON-DISK/HF] | Fusion of vector⊕FTS5; ties BM25 (+0.044 over FTS-only) on LongMemEval n=160. | as above | as above. |
| — | **CE-rerank** (cross-encoder, TinyBERT-L-2) | **MS MARCO passage + TREC-DL** [CANDIDATE] | The canonical reranking benchmark: 8.8M passages, 1M queries, TREC-DL 2019/2020 judged qrels — exactly where cross-encoders are trained/measured. FathomDB's CE is currently null on its own corpora; MS MARCO is the standard proving ground. | — | HF `microsoft/ms_marco` (v1.1/v2.1) + TREC-DL qrels from `trec.nist.gov`. License: MS MARCO **non-commercial research** terms — verify; keep cache-only. |
| — | **alpha** (CE blend weight) / **pool_n** (rerank depth) | LongMemEval + LOCOMO (D0b harness) [ON-DISK/HF] | α=1.0/pool_n=10 is the Mem0-parity lever (MRR 0.347→0.587, r@1 ×3.9). Tested on the agentic-memory corpora via `eval/ce_rerank_probe.py`. | as in §A | as in §A. |
| — | **index-key enrichment** (BM25F-style fielded) | LongMemEval n=40 [HF-STREAM] | Append session entities/facts to FTS body; placebo-controlled (+0.075 content, −0.10 length penalty). Tested on LongMemEval. | as in §A | as in §A. |
| — | **graph arm** (BFS / lexically-seeded PPR) — substrate only | LongMemEval (recall) + MuSiQue (answer-F1) [ON-DISK/HF] | REFUTED for recall (ON−OFF=0.00) and answer-F1 (ΔF1 −0.0405); ships as substrate. Re-opens (Fork E) only for entity-rich corpora. | as in §A / §C | as above. |
| — | **coverage index (D2)** / **map-reduce QFS (C)** | AP-News (BenchmarkQED) [ON-DISK] | The two sensemaking mechanisms; C surpasses full-strength GraphRAG (provisional), D2 loses comp/div. | `data/corpus-data/raw/apnews_benchmarkqed/` | as in §D. |
| — | **bi-temporal edges / supersession** (G0 logical_id) | LongMemEval `knowledge-update` + LOCOMO temporal [ON-DISK/HF] | Latest-fact disambiguation; the substrate behind ku/temporal. MQuAKE [CANDIDATE] is a sharper supersession probe. | as in §A | as in §A; MQuAKE per §A. |
| — | **consolidation / recency provider** (OPP-2) | LongMemEval ku/temporal [HF-STREAM] | Offline memory-formation lever (LLM provider seam); value-tested on ku/temporal. The 0.8.3 blind distiller HURT (−0.362) — isolate mechanism from lossiness. | as in §A | as in §A. |
| — | **query-intent router** (portfolio dispatcher) | ALL of the above (multi-corpus) [mixed] | Classifies query intent → picks (index, retrieval, stack). Needs a *mixed* eval drawing one slice from each class to score routing accuracy — no single dataset; compose from the corpora above. | composed from §A–§D | Compose a labeled intent-routing set from existing gold (no new acquisition). |
| — | **embed-completeness verifier** | partial-embed DBs [ephemeral] | See §F. | ephemeral | — |

---

## Quick stats (v1)

- **User-needs enumerated:** 13 (fact retrieval, discovery-in-k/exploratory, single-hop QA, multi-hop QA, single-shot Q&A, multi-query Q&A, agentic memory, sensemaking, temporal, multi-session, knowledge-update, retrieval-fidelity, engine-performance).
- **Feature/functions enumerated:** ~17 (BM25, FTS5, dense/vector, RRF fusion, CE-rerank, alpha, pool_n, index-key enrichment, graph-arm/PPR, coverage-index D2, map-reduce QFS C, bi-temporal/supersession, consolidation/recency provider, query-intent router, embed-completeness verifier, ANN 1-bit quant fidelity, reader-swap).
- **Corpora mapped:** ~22 distinct datasets/comparators.
  - **On-disk (already acquired):** LongMemEval (HF-cached), LOCOMO, MuSiQue, AP-News BenchmarkQED, FathomDB IR-gold (eu8), eu7 fidelity corpus, the 0.7.0 test corpus (Enron, EnronQA, QMSum, QAConv, QASPER, CNN/DailyMail, Landes todos, bahmutov logs, synthetic notes, chains), memex-ELPS golden, GraphRAG/Mem0 comparator artifacts.
  - **Candidate-new (to acquire):** BEIR, MS MARCO + TREC-DL, Natural Questions/NQ-Open, TriviaQA, SQuAD 1.1/2.0, HotpotQA, 2WikiMultihopQA, MultiHop-RAG, MSC, MQuAKE, GraphRAG podcast/news corpora.
- **Top confirmed gaps (needs with no good corpus on disk):** see [`corpus-search-ledger.md`](./corpus-search-ledger.md) §"Confirmed gaps".
