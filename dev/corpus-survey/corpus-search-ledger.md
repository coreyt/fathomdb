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

## Cycle 3 — 2026-06-29

**Operator:** corpus-survey agent · **Base:** `origin/main` @ `e986acfe`.

### Scope targeted this cycle

Five overlapping need-areas (agent-memory + sensemaking families), with a hard
preference for corpora that ship Q&A / validation gold:

1. **multi_session** — memory across multiple conversation sessions.
2. **temporal** — time-sensitive reasoning / dated facts / recency.
3. **global-sensemaking** — corpus-global "what's been happening across everything"
   QFS (the GraphRAG shape).
4. **episodic memory** — event/experience recall over a personal history.
5. **daily-life** — personal-assistant / everyday-activity logs and timelines.

Decision rule: strongly prefer Q&A-gold-bearing corpora; still record strong
topical fits that ship NO gold, annotated `[NO-GOLD]` with what would make them
evaluable.

### On-disk delta since last cycle (ran `enumerate-corpora.sh`)

**Drift found and reconciled.** The four BEIR subsets are now physically ON-DISK
under `data/corpus-data/raw/beir/` (723 MB total), acquired AFTER Cycle 2's ledger
was written (commits `97e2e8f3` "add BEIR acquisition scripts" + `efa8d102`
"acquire NFCorpus + ArguAna"). Cycle 2 had recorded "on-disk delta: none." All
four ship qrels gold:

| Subset | corpus.jsonl | queries.jsonl | qrels |
|---|---|---|---|
| touche2020 | 382,545 | 49 | test.tsv |
| fiqa | 57,638 | 6,648 | train/test.tsv |
| nfcorpus | 3,633 | 3,237 | train/validation/test.tsv |
| arguana | 8,674 | 1,406 | test.tsv |

`corpus-map.md` §B rows for these four flipped `[CANDIDATE]` → `[ON-DISK]` with
storage paths + acquire-script names. `enumerate-corpora.sh` already listed them in
its §4 reconciliation (added with the acquire scripts), so no script change was
needed.

**Acquisitions performed this cycle (follow-up pass).** The four highest-value,
lowest-friction gold-bearing candidates were acquired with reproducible
`tests/corpus/scripts/acquire_*.py` scripts (inline license headers; payloads
written under the gitignored `data/corpus-data/`; manifest entries recorded).
All four flipped `[CANDIDATE]` → `[ON-DISK]`, and `enumerate-corpora.sh` §4 gained
four `check` lines:

| Corpus | Script | On-disk | Gold confirmed |
|---|---|---|---|
| SummHay | `acquire_summhay.py` | `raw/summhay/summhay.jsonl` (10 haystacks, 92 subtopics, 1,000 docs) | reference `insights` + per-doc `insights_included` citation gold + per-bullet coverage labels |
| Test of Time | `acquire_tot.py` | `raw/tot/{tot_semantic,tot_arithmetic,tot_semantic_large}.jsonl` (51,130 examples) | explicit `label` gold on every example |
| TimeQA | `acquire_timeqa.py` | `raw/timeqa/{dev,test}.{easy,hard}.json` (12,183 eval QA) | `targets` gold answer list (+ unanswerable) |
| TimelineQA | `acquire_timelineqa.py` | `raw/timelineqa/{sparse,medium,dense}/persona-NNN/` (30 personas, ~1.1M atomic QA) | `atomic_qa_pairs` NL gold per event + evidence text |

License posture: SummHay Apache-2.0, ToT CC-BY-4.0, TimeQA BSD-3-Clause (all
gitignored by project default); TimelineQA **CC-BY-NC-4.0 — NON-COMMERCIAL,
EVAL-ONLY**, gitignored, never committed/shipped. TimelineQA is generated locally
from `facebookresearch/TimelineQA`'s `generateDB.py` (pinned); its SQL multi-hop
gold (`multihopQA.py`, needs `pandasql`) is an optional step not run here — the
embedded atomic gold is sufficient for the daily-life eval.

### What was searched (queries / sources)

Three parallel verification passes (WebSearch/WebFetch), each over a named
candidate shortlist for the target need-areas; for every dataset: crisp
description, representative example, license + redistributability (authoritative
source — GitHub LICENSE / HF card / paper / datasheet), size, HF id / GH URL, and
explicit Q&A-gold status/form.

1. **multi_session + episodic memory:** Conversation Chronicles, DialSim, PerLTQA,
   EpiK-Eval, MemoryBank/SiliconFriend, MADial-Bench (discovered).
2. **temporal:** TimeQA, SituatedQA, StreamingQA, TempReason, ComplexTempQA,
   MenatQA, TimeBench, TempTabQA, plus discovered TORQUE, TRAM, Test of Time.
3. **global-sensemaking QFS + daily-life:** SummHay, ODSum, SQuALITY, QMSum
   (confirm query-focused), Multi-News, WCEP, DUC/TAC; TimelineQA, LaMP, PerLTQA,
   OpenLifelogQA, EgoSchema, DailyDialog.

### What was found / added to the map

**Added to `corpus-map.md` §A (16 new rows) and §D (4 new rows).** All ship Q&A /
eval gold EXCEPT Conversation Chronicles (`[NO-GOLD]`).

multi_session / episodic (§A):

- **Conversation Chronicles** `[CANDIDATE — NO-GOLD]` — 200k multi-session episodes
  w/ time-gaps + relationship labels; CC-BY-4.0; **corpus only, no QA**.
- **DialSim** — time-stamped long-term dialogue QA over TV scripts; gold YES
  (MC + open-ended); **license UNCLEAR + TV-script copyright → NON-REDISTRIBUTABLE,
  EVAL-ONLY**.
- **MADial-Bench** — memory-augmented dialogue; MIT; gold = retrieval labels +
  reference responses.
- **PerLTQA** — personal semantic+episodic memory QA (Chinese); CC-BY-NC-4.0; gold
  = answer + retrieval + classification.
- **EpiK-Eval** — distributed-narrative consolidation/recall; MIT; gold = reference
  answers (consolidation-style, not retrieval-over-corpus).
- **MemoryBank / SiliconFriend** — MIT; small gold (100 bilingual probes).

temporal (§A):

- **TimeQA** (BSD-3, ~40k QA, Easy/Hard + unanswerable), **SituatedQA**
  (CC-BY-SA-4.0, temporal+geo), **StreamingQA** (CC-BY-4.0, recency over a dated
  news stream, 145,719 QA), **TempReason** (CC-BY-SA-3.0, L1/L2/L3 leveled probe),
  **ComplexTempQA** (CC0, ~100M QA, compare/count/multi-hop), **Test of Time**
  (CC-BY-4.0, contamination-free synthetic). All ship scored gold.

daily-life (§A):

- **TimelineQA** — synthetic lifelog timeline QA (~600k atomic QA); CC-BY-NC-4.0;
  the strongest pure personal-timeline / daily-life fit; locally generated.
- **LaMP** — personalization benchmark (7 tasks); CC-BY-NC-SA-4.0; **LaMP-6
  LDC-gated**.

global-sensemaking / QFS (§D):

- **SummHay (Summary of a Haystack)** — Apache-2.0; the closest public analog to
  FathomDB's "across-everything" shape (~100-doc haystack → cited bulleted summary,
  Coverage+Citation gold). Best new sensemaking candidate.
- **ODSum** — MIT; open-domain retrieve-then-summarize QFS; dual gold (reference
  summaries + retrieval relevant-doc set).
- **SQuALITY** — multi-reference single-doc QFS; Gutenberg + CC-BY summaries.
- **QMSum** `[ON-DISK]` — confirmed genuinely query-focused (`general_query_list` +
  `specific_query_list`); surfaced as the on-disk QFS row (already in the 0.7.0
  corpus at `raw/qmsum.jsonl` + `eval/qmsum_qa.jsonl`).

**Logged in ledger only (secondary / not map rows):** temporal — TempTabQA
(CC-BY-4.0, table-QA), TimeBench (MIT, aggregated suite), TORQUE (Apache-2.0,
ordering RC), TRAM (MIT, 526k MCQ); MenatQA (**no LICENSE → all-rights-reserved**,
kept out of the map despite good gold). sensemaking — Multi-News (custom
non-commercial, generic MDS), WCEP (MIT, generic MDS), DUC/TAC (NIST-gated, the
canonical query-focused MDS but not openly redistributable). daily-life —
OpenLifelogQA (real wearable lifelog, gated + multimodal), EgoSchema (video-grounded
daily-activity MCQA, Ego4D DUA). no-gold dialogue — DailyDialog, MSC, DuLeMon.

### Gaps closed

- **Episodic-memory and daily-life need-areas now have mapped, gold-bearing
  candidates** (previously absent from the need column). TimelineQA (daily-life) and
  PerLTQA/EpiK-Eval/MADial-Bench (episodic) are the leads.
- **Dedicated multi-session corpus beyond MSC** — Conversation Chronicles (corpus
  scale, NO-GOLD) and DialSim (gold, license-encumbered) added; MADial-Bench adds
  retrieval-grounding gold.
- **Temporal need-area deepened** from "LongMemEval/LOCOMO slices only" to six
  external, gold-bearing, mostly-permissive benchmarks (TimeQA / Test of Time /
  TempReason easiest to acquire).
- **Global-sensemaking gained an openly-licensed external benchmark** — SummHay
  (Apache-2.0), removing the prior dependence on the non-redistributable AP-News
  BenchmarkQED for the QFS shape. QMSum confirmed query-focused.
- **BEIR on-disk drift reconciled** (see on-disk delta).

### Gaps still open

1. **Four target corpora acquired** (SummHay, Test of Time, TimeQA, TimelineQA —
   see the "Acquisitions performed this cycle" table above; all `[ON-DISK]` with
   gold confirmed). Remaining un-acquired candidates in the five need-areas are
   still `[CANDIDATE]`: multi-session/episodic (Conversation Chronicles NO-GOLD,
   DialSim license-encumbered, MADial-Bench, PerLTQA, EpiK-Eval, MemoryBank);
   temporal (SituatedQA, StreamingQA, TempReason, ComplexTempQA); daily-life
   (LaMP); sensemaking (ODSum, SQuALITY).
2. **Exploratory-proxy empirical confirmation** (carried from Cycle 2) — Touché-2020
   is now ON-DISK with qrels, so the remaining sub-task is purely to RUN FathomDB's
   retrieval stack against it and confirm the dense-fails failure mode reproduces.
3. **CE-rerank on MS MARCO / TREC-DL** — unchanged; not on disk.
4. **No-gold corpora need synthetic gold** — Conversation Chronicles (large, clean
   CC-BY-4.0) would need synthesized QA over its time-interval/relationship labels.

### Open questions (resolved / new)

Resolved this cycle (licenses/gold verified): TimeQA (BSD-3), SituatedQA
(CC-BY-SA-4.0 per datasheet — HF mirror `siyue/SituatedQA` mislabels MIT; trust
datasheet), StreamingQA (dataset CC-BY-4.0; corpus must be rebuilt from WMT, not
redistributed), TempReason (CC-BY-SA-3.0), ComplexTempQA (CC0), Test of Time
(CC-BY-4.0), SummHay (Apache-2.0), ODSum (MIT), QMSum (MIT, confirmed
query-focused), TimelineQA (CC-BY-NC-4.0), PerLTQA (CC-BY-NC-4.0, Chinese), EpiK-Eval
(MIT, EMNLP 2023 not ACL 2024), MADial-Bench (MIT), MemoryBank (MIT), LaMP
(CC-BY-NC-SA-4.0; LaMP-6 LDC-gated).

New / unresolved:

- **DialSim redistributability** — no repo LICENSE; data via Google Drive; TV-script
  copyright. Treat NON-REDISTRIBUTABLE, EVAL-ONLY until clarified.
- **MenatQA license** — repo has no LICENSE (all-rights-reserved default); not mapped.
- **SQuALITY exact summary-license tag** — CC-BY confirmed in spirit; exact version
  unverified.
- **ComplexTempQA HF viewer** — schema bug on the viewer; data present, use the
  sample or regenerate.
- **PerLTQA sub-counts / HF id** — partially unverified (GH is authoritative).

### Machinery changes

- `corpus-map.md` — v3: §B BEIR rows reconciled to `[ON-DISK]`; §A +16 rows
  (multi_session / episodic / temporal / daily-life); §D +4 rows (SummHay, ODSum,
  SQuALITY, QMSum-as-QFS); Quick-stats updated (15 user-needs, ~44 corpora,
  per-cycle license + gold posture).
- `enumerate-corpora.sh` — §4 gained four `check` lines for the corpora acquired
  this cycle (SummHay, Test of Time, TimeQA, TimelineQA); the four BEIR subsets
  were already listed.
- `tests/corpus/scripts/` — four new reproducible acquire scripts
  (`acquire_summhay.py`, `acquire_tot.py`, `acquire_timeqa.py`,
  `acquire_timelineqa.py`) + matching `manifest.json` entries.

---

## Cycle N — YYYY-MM-DD (template)

**Operator:** … · **Base:** `origin/main` @ `<sha>`.

- **Scope targeted:** …
- **On-disk delta since last cycle** (run `enumerate-corpora.sh`): …
- **Searched:** …
- **Found / added to map:** …
- **Gaps closed:** … · **Gaps still open:** …
- **Open questions resolved / new:** …
