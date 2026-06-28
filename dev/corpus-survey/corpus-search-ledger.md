# FathomDB corpus-search ledger

**Purpose.** A durable, append-only record of each corpus-search cycle: what
needs/functions were targeted, what was searched, what was found, what is already
on disk, confirmed gaps, and open questions. Each cycle appends a new dated
section — never rewrite prior cycles. Pairs with the MAP
([`corpus-map.md`](./corpus-map.md)) and the re-run prompt
([`corpus-search-cycle-PROMPT.md`](./corpus-search-cycle-PROMPT.md)).

---

## Cycle 1 — 2026-06-28 (v1 foundation)

**Operator:** corpus-survey agent · **Base:** `origin/main` @ `db34820c`.

### Scope targeted this cycle

Every FathomDB user-need and feature/function from the capability frame
(`dev/plans/runs/0.8.x-capability-status-report.md`), the parity-portfolio
strategy (`dev/design/0.8.x-parity-portfolio-strategy.md`), and the experiments
ledger (`dev/experiments-ledger.md`): fact retrieval, discovery-in-k / exploratory
recall@k, single-hop, multi-hop, single-shot Q&A, multi-query Q&A, BM25, FTS5,
dense/vector recall, CE-rerank, alpha, agentic memory, sensemaking, temporal,
multi-session, knowledge-update, plus the standalone functions (graph arm,
coverage index, map-reduce QFS, supersession, consolidation provider, router,
embed verifier, ANN fidelity).

### What was already on disk (grounded BEFORE searching)

Inventoried via the new `enumerate-corpora.sh` against
`/home/coreyt/projects/fathomdb/data/corpus-data/` (gitignored; physically in the
primary checkout). Confirmed present:

- **Agentic memory:** LOCOMO (`raw/locomo10.json`, 10 conv / 1,986 QA, CC-BY-NC,
  EVAL-ONLY); LongMemEval (HF-streamed `xiaowu0162/longmemeval-cleaned`, cached);
  LOCOMO memory gold (`eval/0.8.3-locomo-memory-gold.json`); memex-ELPS golden
  (`external/memex-elps/`).
- **Multi-hop:** MuSiQue (`raw/musique_dev.jsonl`, 4,834 rows, CC-BY-4.0) + graph
  cache (`graph-cache/0.8.2-m1-v1/`).
- **Sensemaking:** AP-News BenchmarkQED (`raw/apnews_benchmarkqed/`, 1,397 articles
  - AutoQ questions, MS-Research license, NON-REDISTRIBUTABLE EVAL-ONLY).
- **IR relevance / fidelity (FathomDB self-corpora):** IR gold / eu8
  (`eval/ir_gold/all.gold.json`, 301 labeled queries); eu7 fidelity corpus (derived
  from the 0.7.0 test corpus); the ~10k 0.7.0 test corpus (Enron, EnronQA, QMSum,
  QAConv, QASPER, CNN/DailyMail, Landes todos, bahmutov logs, synthetic notes,
  chain connectives + 200 committed chains under `tests/corpus/chains/`).
- **Comparators (systems, not corpora):** Microsoft GraphRAG workspace cache
  (`0.8.4-graphrag-artifacts/`); Mem0-OSS adapter (`eval/mem0_local.py`).
- **Acquisition machinery:** 11 reproducible `tests/corpus/scripts/acquire_*.py`
  (incl. LOCOMO, MuSiQue, AP-News) + `manifest.json` (note: LOCOMO and AP-News are
  NOT in `manifest.json` — they have standalone acquire scripts with inline license
  headers; the manifest covers the 0.7.0 IR-freeze sources only).

### What was searched (queries / sources)

Web (WebSearch), targeted at license / HF-id / size verification for canonical
datasets — NOT re-deriving what the repo already documents:

1. BEIR — composition, license posture, nDCG@10/BM25 role → arXiv:2104.08663; per-dataset licenses vary (BEIR only repackages).
2. LongMemEval — size, 5 ability classes, HF id → confirmed `xiaowu0162/longmemeval-cleaned`; **no clear SPDX on the card** (open question).
3. 2WikiMultihopQA — license/size → **Apache-2.0**, 192k Q up to 5-hop; HF `framolfese/2WikiMultihopQA`.
4. Natural Questions / NQ-Open — license/size → **CC-BY-SA-3.0**; NQ-Open 79k/8.7k/3.6k.
5. HotpotQA — license/size → **CC-BY-SA-4.0**, 113k 2-hop, distractor/fullwiki; HF `hotpotqa/hotpot_qa`.
6. MS MARCO + TREC-DL — rerank corpus → 8.8M passages / 1M queries; non-commercial research terms (verify).
7. TriviaQA / SQuAD — license/size → TriviaQA Apache-2.0 annotations (UW disclaims underlying copyright); SQuAD **CC-BY-SA-4.0**, 1.1 107k / 2.0 +53k unanswerable.
8. LOCOMO — license/categories → confirmed **CC-BY-NC-4.0**, 1,986 QA, 5 categories (matches on-disk).
9. GraphRAG QFS — sensemaking corpora → original podcast (~1M tok) + news (~1.7M tok); BenchmarkQED AP-News is the on-disk realization.
10. MuSiQue — license/composition → confirmed **CC-BY-4.0**, ~25k 2–4-hop answerable.

### What was found (new candidates added to the map)

- **BEIR** (zero-shot IR / BM25 / dense / rerank floor) — the missing canonical home for IR-relevance.
- **MS MARCO passage + TREC-DL** (the canonical CE-rerank proving ground).
- **Natural Questions / NQ-Open, TriviaQA, SQuAD 1.1/2.0** (single-hop QA + abstention).
- **HotpotQA, 2WikiMultihopQA, MultiHop-RAG** (multi-hop breadth beyond MuSiQue).
- **MSC (Multi-Session Chat)** (a dedicated multi-session-linking corpus).
- **MQuAKE** (a sharper knowledge-update/supersession probe than LongMemEval's 78 q).
- **GraphRAG podcast/news corpora** (alternative QFS corpora to AP-News).

### Confirmed gaps (needs with NO good corpus on disk)

Ranked by how load-bearing the need is to the parity strategy:

1. **Discovery-in-k / exploratory recall@k** — FathomDB's structurally hardest IR
   class (dense median gold rank ~99) is tested only on its OWN synthetic chains +
   eu8. There is **no acquired, well-respected public corpus** purpose-built for
   exploratory/discourse retrieval. Closest public proxies (BEIR ArguAna / Touché /
   TREC-COVID) are candidates but unverified-as-fit. **This is the #1 gap.**
2. **First-stage IR relevance vs a standard external benchmark** — FathomDB's
   "ties BM25" claim rests on LongMemEval + its own eu8; **BEIR / MS MARCO are not on
   disk**, so there is no cross-domain external IR number. Acquiring a few commit-clean
   BEIR subsets (SciFact, NFCorpus) would let FathomDB report a standard nDCG@10.
3. **CE-rerank on its native benchmark** — the cross-encoder is null on FathomDB's
   own corpora but has never been run on **MS MARCO / TREC-DL**, where it is meant to
   shine. Without it we cannot tell "CE is weak" from "CE has nothing to fix on this
   corpus." MS MARCO is not on disk.

Secondary gaps: a dedicated **multi-session-linking** corpus (MSC, candidate);
a sharp **knowledge-update/supersession** corpus (MQuAKE, candidate); an
**abstention / confident-wrong** probe (SQuAD 2.0, candidate) — FathomDB's M1 graph
is answerable-only so the confident-wrong guard is UNEVALUATED.

### Open questions (carry to next cycle)

- **LongMemEval license** — the HF card (`xiaowu0162/longmemeval-cleaned`) does not
  state a clear SPDX license. Resolve the redistribution posture before treating it
  as anything other than research-use cache-only.
- **MultiHop-RAG license** — some mirrors report CC-BY-NC; confirm the canonical
  repo's terms before acquiring.
- **MS MARCO redistribution** — confirm the exact non-commercial research terms and
  whether TREC-DL qrels can be cached locally.
- **Are BEIR ArguAna / Touché actually a good "exploratory" proxy?** — needs a small
  empirical check (do FathomDB's exploratory failure modes reproduce there?), not
  just a license check.
- **eu7/eu8 provenance** — eu7's `research/eu-0/` sweep tree is untracked/git-ignored
  (experiments-ledger R-2 HOLD); confirm the eu7/eu8 corpora are fully rebuildable
  from committed scripts before relying on them as durable.
- **MSC source terms** — ParlAI code is MIT but the underlying dialogue data terms
  vary; verify before acquiring.

### Cycle-1 deliverables

- `corpus-map.md` (v1 map), this ledger, `corpus-search-cycle-PROMPT.md`,
  `enumerate-corpora.sh` — all under `dev/corpus-survey/`.

---

<!-- APPEND THE NEXT CYCLE BELOW THIS LINE. Do not edit cycles above. -->

## Cycle N — YYYY-MM-DD (template)

**Operator:** … · **Base:** `origin/main` @ `<sha>`.

- **Scope targeted:** …
- **On-disk delta since last cycle** (run `enumerate-corpora.sh`): …
- **Searched:** …
- **Found / added to map:** …
- **Gaps closed:** … · **Gaps still open:** …
- **Open questions resolved / new:** …
