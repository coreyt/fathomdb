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
  + AutoQ questions, MS-Research license, NON-REDISTRIBUTABLE EVAL-ONLY).
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

## Cycle 2 — 2026-06-28

**Operator:** corpus-survey agent · **Base:** `origin/main` @ `25056290`.

### Scope targeted this cycle

Three focused targets from the Cycle-1 worklist:

1. **Gap #1 (exploratory proxy)** — verify whether BEIR ArguAna and/or Touché-2020
   are actually a good proxy for FathomDB's "discovery-in-k / exploratory" failure
   mode (dense median gold rank ~99).
2. **Open-question license resolution** — LongMemEval, MultiHop-RAG, MSC, MS MARCO.
3. **BEIR discriminator pair** — identify the best BEIR subsets for differentiating
   BM25 vs dense retrieval on FathomDB's actual capability dimensions.

### On-disk delta since last cycle

None. No new payloads acquired. Enumeration confirmed all Cycle-1 on-disk corpora
still present; `enumerate-corpora.sh` bug fixed (see §machinery below).

### What was searched (queries / sources)

Targeted web research (WebSearch/WebFetch):

1. ArguAna — query characteristics, BM25 vs dense nDCG@10 from BEIR Table 2.
2. Touché-2020 (Webis-Touché-2020) — retrieval numbers, 2024 SIGIR follow-up
   (arXiv:2407.07790), structural failure mechanism.
3. TREC-COVID — BM25 vs dense, pooling-bias question.
4. LongMemEval — GitHub `wu7401/LongMemEval` license field.
5. MultiHop-RAG — GitHub `yixuantt/MultiHop-RAG` README + HF card license.
6. MSC — ParlAI project page, arXiv:2107.07567, HF `nayohan/multi_session_chat` card.
7. MS MARCO — official `microsoft.github.io/msmarco/Datasets.html` terms.
8. BEIR per-dataset licenses — SciFact, NFCorpus, FiQA-2018, TREC-COVID canonical
   sources vs HF wrapper label.

### What was found / added to the map

#### Critical correction: exploratory proxy (Gap #1 partially resolved)

- **Touché-2020 confirmed as a correct proxy.** BM25 nDCG@10=0.367 vs dense
  ≤0.27 through 2024-era models (E5-large, BGE-large). Root cause is structural
  (LNC2-axiom violation, not training-distribution artifact): dense retrieves short
  non-argumentative fragments (avg &lt;350w), BM25 retrieves full arguments (avg
  &gt;600w). This matches FathomDB's dense-fails-on-discourse pattern.

- **ArguAna is the WRONG proxy.** Dense BEATS BM25 on ArguAna (TAS-B 0.429 vs BM25
  0.315) because counter-argument retrieval rewards semantic opposition over lexical
  matching. Using ArguAna would produce results inverted from FathomDB's failure mode.
  Map updated to mark ArguAna `[CANDIDATE — low priority]` with anti-example note.

- **TREC-COVID is not a reliable discriminator.** BM25 (0.656) vs ANCE (0.654) are
  tied; the BM25 result is inflated by pooling bias (judgments built from BM25-based
  systems). Not useful for the target signal.

#### BM25 vs dense discriminator pair identified

- **FiQA-2018** [CANDIDATE] — dense wins (ANCE 29.6 vs BM25 23.6; modern ≥50).
  57,638 docs, 648 queries, financial QA (StackExchange/Reddit). Best subset to
  show where dense adds value. Non-commercial research license.

- **NFCorpus** [CANDIDATE] — BM25 wins (32.5 vs DPR 17.7). 3,633 docs, 323 queries;
  highest Rel D/Q (38.2) among small subsets — statistically stable. Specialist
  biomedical vocabulary. Free for academic research (informal; non-academic commercial
  use requires data owner contact).

FiQA + NFCorpus form the recommended discriminator pair for FathomDB's external
nDCG@10 reporting.

#### License resolutions (four open questions closed)

| Dataset | Prior status | Resolved |
|---|---|---|
| **LongMemEval** | "no clear SPDX — research-use only" | **MIT** (GitHub `wu7401/LongMemEval` LICENSE + HF card; arXiv CC BY 4.0 covers the paper, not the data). Fully permissive. |
| **MultiHop-RAG** | "CC-BY-NC reported on some mirrors" | **ODC-BY 1.0** (GitHub README + HF `license: odc-by`). NC rumor is incorrect. Research/eval + redistribution permitted with attribution. |
| **MSC** | "ParlAI MIT; underlying data terms vary" | **No formal SPDX on the data.** ParlAI code is MIT; no license in paper (arXiv:2107.07567), README, or HF card. Publicly downloadable with no gating. Gitignored posture is correct; redistribution has no explicit grant. |
| **MS MARCO** | "non-commercial research, verify" | **Custom Microsoft non-commercial ToS** (authoritative page: `microsoft.github.io/msmarco/Datasets.html`). Local download + eval: explicitly permitted. Redistribution: not permitted. No SPDX identifier. |

#### Key warning: BEIR HF card licenses are not per-dataset truth

The HF BeIR wrapper displays `cc-by-sa-4.0` as a blanket metadata label on all
subsets. This is NOT authoritative. Verified per-dataset:

| Subset | HF label | Canonical license |
|---|---|---|
| SciFact | cc-by-sa-4.0 | CC BY-NC 2.0 (AllenAI) |
| NFCorpus | cc-by-sa-4.0 | "free for academic" (informal, NutritionFacts.org) |
| FiQA-2018 | cc-by-sa-4.0 | non-commercial research (FiQA challenge) |
| Touché-2020 | cc-by-sa-4.0 | CC BY 4.0 (Zenodo 6862281) |

The BEIR GitHub wiki and dataset-origin pages are the authoritative sources.

### Gaps closed

- **Gap #1 (exploratory proxy) — partially closed.** Touché-2020 is confirmed as
  the correct BEIR proxy for FathomDB's exploratory failure mode. ArguAna is an
  anti-example (wrong proxy). Remaining sub-task: acquire Touché-2020 and run
  FathomDB's retrieval stack against it to confirm the failure mode reproduces
  on an external corpus (not just the internal synthetic chains).

- **Open questions: LongMemEval / MultiHop-RAG / MS MARCO / MSC licenses** — all
  four resolved above.

### Gaps still open

1. **Exploratory proxy: empirical confirmation still needed.** Touché-2020 is
   structurally the right proxy but FathomDB has not been run against it. The gap
   is not fully closed until a retrieval number exists.

2. **BEIR nDCG@10 external number.** Touché-2020, FiQA-2018, and NFCorpus are
   confirmed candidates; none yet acquired.

3. **CE-rerank on MS MARCO / TREC-DL.** CE is null on FathomDB's own corpora;
   MS MARCO is not on disk.

4. **MSC multi-session-linking eval.** No change from Cycle 1; no acquisition.

5. **MQuAKE knowledge-update / supersession.** Candidate status unchanged.

6. **eu7/eu8 rebuildability.** `research/eu-0/` sweep tree is untracked/git-ignored;
   confirmed not yet resolved.

### Open questions (new this cycle)

- **Touché-2020 failure-mode transfer.** Does FathomDB's dense retrieval fail on
  Touché-2020 for the same structural reason (LNC2-axiom violation / length
  mismatch) or for corpus-distribution reasons? Needs a run.

- **NFCorpus academic-use scope.** "Free for academic purposes" is informal; if
  FathomDB is used in a commercial product the terms require contacting
  NutritionFacts.org / Heidelberg NLP. Flag if product context changes.

### Machinery changes (not ledger content, but record)

- `enumerate-corpora.sh` — fixed a shell operator-precedence bug (`||` and `&&`
  chaining without grouping) that caused REPO_ROOT to embed two lines, breaking the
  gitignore check (false WARN) and the `tests/corpus` section (false "not found").
  Fix: wrap the fallback in `(…)` → `|| (cd … && pwd)`.
- `corpus-map.md` — updated status to v2; fixed BEIR acquisition note to remove
  "commit-clean" framing (payloads live gitignored, any research-use license is
  acceptable).
- `corpus-search-cycle-PROMPT.md` — removed "commit-clean" qualifier from BEIR
  directions.

---

## Cycle N — YYYY-MM-DD (template)

**Operator:** … · **Base:** `origin/main` @ `<sha>`.

- **Scope targeted:** …
- **On-disk delta since last cycle** (run `enumerate-corpora.sh`): …
- **Searched:** …
- **Found / added to map:** …
- **Gaps closed:** … · **Gaps still open:** …
- **Open questions resolved / new:** …
