# IR-C — Fact-level gold labels for FathomDB recall eval · research + plan

**Date:** 2026-06-09. **Status:** research / planning (no labels generated, no code changed).
**Author context:** RESEARCH + DELIVERABLE only. This note inventories the gold data that
already ships, surveys 2024–2026 methodology for building fact-level IR gold labels, and
recommends a concrete reuse/adapt/generate plan. The companion generation prompt is
`dev/plans/prompts/IR-C-fact-level-gold-label-generation.md`.

**What IR-C is (from the roadmap).** IR-1 Phase 3: build the *real* fact-level gold set on the
**frozen** corpus, then run the Evidence Recall@K measure (mode×K×class) and feed IR-2/AC-077.
IR-A (`dev/design/ir-recall-measure.md`) defines the *measure*; IR-B
(`tests/support/ir_eval.rs`) implements the corpus-independent math + schema + validator; IR-C
supplies the **labels**. See `dev/plans/prompts/scaffolds/7-IR-B-C-D-recall-measure.md` and
`dev/plans/prompts/0.8.x-IR-1-recall-measure.md`.

---

## Part A — Inventory of what already exists

### A.0 The frozen corpus (the pinned basis)

`tests/corpus/snapshot.json`: **10,506 docs, 10 sources**, `corpus_version = 0.8.x-B`,
`corpus_hash = fe973fcd49fbbda083158f69fe720f17858ab8528e171fa2188eec84131c7d4e`,
`reproduced_bit_identical = true`. Per-source doc counts:

| source | source_type | docs | license | distribution |
|---|---|---:|---|---|
| bahmutov_dailylogs | note | 300 | MIT | commit |
| chain_connectives | (synthetic) | 371 | — | (synthetic; chains) |
| cnn_dailymail | article | 2500 | Apache-2.0 | commit |
| enron | email | 2000 | LicenseRef-Enron-Research-Use | commit |
| enronqa | email | 200 | LicenseRef-EnronQA-Undeclared | **cache** |
| landes_todos | todo | 500 | MIT | commit |
| qaconv | note/email/meeting | 1250 | BSD-3-Clause | commit |
| qasper | paper | 1585 | CC-BY-4.0 | commit |
| qmsum | meeting | 600 | LicenseRef-QMSum-MIT-with-upstream-chain | **cache** |
| synthetic_notes | note | 1200 | Apache-2.0 | commit |

`doc_id` = first 16 hex chars of `SHA-256(provenance|source_native_id)`
(`tests/corpus/scripts/_corpus_lib.py:56`). Doc body schema (corpus-card §"Document schema"):
`doc_id, source_type, title, body, created_at, modified_at, author_or_sender, recipients,
people_mentions, project_mentions, tags, url_or_external_id, thread_id, parent_doc_id, license,
provenance`. `body` is the text that is embedded; **FathomDB does not chunk** — whole node bodies
are stored and returned, so document-recall == chunk-recall and the only genuinely missing axis is
**fact/evidence-level** recall (per `dev/design/ir-recall-measure.md` §(b)).

### A.1 Existing eval QA — what each source carries (verified counts)

The eval QA lives in `data/corpus-data/eval/*.jsonl` (gitignored; produced by the acquire scripts).
Row schema (per `dev/plans/prompts/0.8.x-corpus-qa-expansion-handoff.md` §1): `qa_id, source,
source_type, question, answers[], answer_type, evidence_doc_ids[], evidence_spans[],
negative_doc_ids[], relation_type, metadata{split,user_id,thread_id,upstream_id}, license,
provenance`.

| file | rows | answer_type breakdown | rows w/ evidence_doc_ids | evidence_spans | unique evidence doc_ids | doc coverage |
|---|---:|---|---:|---:|---:|---|
| `enronqa_qa.jsonl` | 710 | span 710 | **710 (100%)** | 0 | 200 | **200 / 200 enronqa docs** |
| `qaconv_qa.jsonl` | 2303 | span 1493, free_form 685, abstain 125 | **2303 (100%)** | 0 | 1250 | **1250 / 1250 qaconv docs** |
| `qasper_qa.jsonl` | 7993 | **abstain 7993 (ALL)** | **0** | 0 | 0 | **0 / 1585 qasper docs** |
| `qmsum_qa.jsonl` | 1584 | summary 1584 | **1584 (100%)** | 0 | 200 | **200 / 600 qmsum docs** |

**Grounding spot-check (Part A Q2 — verified, not assumed).** I built the set of all 10,506
corpus `doc_id`s from `data/corpus-data/raw/*.jsonl` and cross-checked every `evidence_doc_ids`
entry in each eval file. Result: **enronqa, qaconv, qmsum — 0 unresolved evidence ids** (every
evidence doc id is present in the frozen snapshot). qasper has no evidence ids to resolve. The
per-source raw doc counts match `snapshot.json` exactly. So the existing evidence pointers are
**clean and corpus-resolvable** — they can be lifted directly into the IR-B `GoldQuery` schema.

### A.2 Source-by-source characterization

- **enronqa (710 QA, email).** Best-shaped source: every row is `answer_type=span`, carries
  exactly one `evidence_doc_ids` entry that resolves, plus rich metadata (`rephrased_question`,
  `incorrect_answers`, `include_email`, `user_id`). `evidence_spans` is **empty** (the doc id is
  there, but no character offsets). **License: cache-only / undeclared** — usable for internal
  eval, not redistributable; the gold set inherits that posture.
- **qaconv (2303 QA, note/email/meeting).** Conversational QA over Slack/email/court threads.
  100% evidence-resolved, single evidence doc per row, but ~30% are `free_form` (paraphrased
  answers, weaker for span localization) and 125 are `abstain` (these are *negative-class
  candidates*). `evidence_spans` empty. **License: BSD-3-Clause — clean.**
- **qasper (7993 QA, paper).** **Currently unusable as fact-level gold.** All 7993 rows are
  `answer_type=abstain` with empty `answers` and empty `evidence_doc_ids`, even though the
  acquisition intent (`0.8.x-corpus-qa-expansion-handoff.md` §3) was to "export all QA/evidence
  rows" (~5049 questions, QASPER ships extractive spans + `highlighted_evidence`). The acquire
  script logic is correct in principle (`acquire_qasper.py:185-196,229` sets evidence when
  `extractive_spans`/`free_form_answer`/`highlighted_evidence` are present), so **the all-abstain
  output is an upstream-parse defect** — `_answer_records()`/`_qa_items()` is almost certainly not
  descending into the real QASPER answer objects, so every answer falls through to the
  `return [], "abstain"` default. **Net: qasper contributes 0 gold today; fixing the parser would
  recover ~5k paper-domain extractive labels with verbatim evidence spans — the single highest-value
  recovery.** License CC-BY-4.0 — clean.
- **qmsum (1584 QA, meeting).** Query→summary pairs over meeting transcripts. 100%
  evidence-resolved, but evidence points only at the **200 transcript docs** (the raw source is 600
  `meeting` docs = 200 transcript + 200 general-summary + 200 specific-summary; the QA evidence is
  the transcript). Every row is `answer_type=summary` — **these are summarization/exploratory
  labels, not exact-fact labels**, and the "answer" is a long abstractive summary, so they map to
  the `exploratory`/`summarizes` class, not `exact_fact`. License cache-only.

### A.3 Chain fixtures (the eu8 doc-granularity baseline)

`tests/corpus/chains/*.json` (~200 files; snapshot shows `chain_connectives = 371` docs). Each
carries `ground_truth_queries[]` of `{query, expected_top_k_doc_ids[], relation_type}` (e.g.
`chain-article_note_email-0001.json`). These are **synthetic cross-doc retrieval chains** — 2–3
queries/chain, 2–10 expected doc ids — and are the *document-granularity* qrels that eu8
(`eu8_ir_validation.rs`) already consumes. In the IR-B schema they are the legacy/fallback case:
a query with only `expected_top_k_doc_ids` degenerates to whole-document `required` evidence units
(`ir_eval.rs::required_doc_ids`, §(f) eu8 reduction). They carry **no fact-level evidence** and
**no graded relevance** — useful as the migration baseline and for `commitment`/`action` chain
shapes, not as fact-level gold.

### A.4 What the eval harness actually consumes (Part A Q3 — the binding contract)

The IR-B harness is **already built** and defines the exact input format IR-C must produce:

- **Loader/schema:** `src/rust/crates/fathomdb-engine/tests/support/ir_eval.rs`
  (`load_gold_set` / `parse_gold_set`). A gold set file is:
  ```jsonc
  {
    "corpus_hash": "<frozen snapshot hash>",   // pinning principle §(f)
    "qrels_version": "ir-c-...-vN",
    "note": "optional",
    "queries": [ GoldQuery, ... ]
  }
  ```
  and each `GoldQuery` is:
  ```jsonc
  {
    "query": "...",                    // SAME key eu8 reads (additive-superset invariant)
    "query_id": "stable-unique",
    "query_class": "commitment|action|exact_fact|preference|exploratory|negative",
    "required_evidence": [
      { "evidence_id": "stable-unique-within-query",
        "doc_id": "<corpus doc_id>",
        "necessity": "required|supporting",
        "locator": { "kind": "span|whole_body" } }
    ],
    "expected_top_k_doc_ids": ["..."],  // PRESERVED eu8 doc-id view (legacy/fallback denominator)
    "relation_type": "action_from|contradicts|follows_up_on|mentions|summarizes|...",
    "chain_shape": "..."
  }
  ```
- **Driver/runner:** `src/rust/crates/fathomdb-engine/tests/ir_recall_eval.rs` (the wired
  experiment test) calls `run_experiment` over `RUNNABLE_NOW_MODES`.
- **Fixture (schema illustration only, NOT real labels):**
  `tests/fixtures/ir_gold/synthetic_gold.json` — invented doc_ids,
  `corpus_hash = "TODO(COR-2-freeze)"` so the validator flags it fixture-only.
- **Validator invariants (`validate_gold_set`) — IR-C MUST satisfy all:**
  1. `corpus_hash` present and **not** the `TODO(COR-2-freeze)` placeholder; `qrels_version` present.
  2. Non-empty `query`; **unique** `query_id` across the set; **unique** `evidence_id` within a query;
     non-empty `doc_id` on every unit.
  3. Class/denominator coherence: a **non-`negative`** query MUST have a **non-empty** `required`
     denominator; a **`negative`** query MUST have an **empty** one (abstention class).
- **Scoring contract (`evidence_recall_at_k`):** an evidence unit counts as retrieved@K iff its
  `doc_id` appears in the engine's top-K result bodies. Presence is at **doc-body granularity** —
  `locator` is recorded for audit but is **NOT load-bearing for the score** (FathomDB does not
  chunk). Only `necessity=required` units form the recall denominator (strict all-of headline +
  graded fraction); `supporting` units are a separate supporting-coverage diagnostic. `negative`
  queries are scored as abstention-correctness, never recall. K-ladder = {5,10,20,50}, headline @10.

**Critical alignment fact for IR-C:** the existing eval QA schema (`evidence_doc_ids`,
`answers`, `answer_type`, `evidence_spans`) is the *acquisition* schema; the harness consumes the
*GoldQuery* schema. They are **not identical** — IR-C's job includes a **transform** from the
acquisition rows into `GoldQuery` (map `evidence_doc_ids` → `required_evidence[].doc_id` with
`necessity=required`, assign a `query_class`, mint `query_id`/`evidence_id`, attach the frozen
`corpus_hash`). The transform is mechanical for the resolved sources; the labeling work is
(a) **class assignment**, (b) **span localization** (currently absent everywhere), (c) **negative
queries**, and (d) **the qasper gap**.

### A.5 Part A key-question answers (concise)

1. **Which sources have answers/evidence, which need generation?**
   - **Reusable as-is (evidence resolves):** enronqa (710), qaconv (2303 minus 125 abstain),
     qmsum (1584). Plus chain fixtures (~300 doc-granularity queries) as the eu8 baseline.
   - **Has corpus docs but NO usable labels (needs generation/repair):** **qasper (1585 paper
     docs, 0 gold — parser defect)**; **cnn_dailymail (2500 article docs, 0 gold)**; **enron
     (2000 email docs, 0 gold — distinct from the 200 enronqa docs)**; **bahmutov_dailylogs (300
     note docs, 0 gold)**; **landes_todos (500 todo docs, 0 gold)**; **synthetic_notes (1200 note
     docs, 0 gold)**. These six source-buckets (≈8,085 docs) have **no fact-level QA** and are the
     fresh-generation target.
2. **Do existing evidence_doc_ids resolve to the frozen snapshot?** **Yes — verified, 0
   unresolved** for enronqa/qaconv/qmsum; qasper has none.
3. **What format does the harness consume?** The `GoldSet`/`GoldQuery` JSON in `ir_eval.rs`
   (above), validated by `validate_gold_set`, pinned to `corpus_hash`.

---

## Part B — Methodology (2024–2026 best practices, with citations)

### B.1 Pooling vs. exhaustive judging, and the recall-bias trap

Exhaustive judging of a 10.5k-doc corpus per query is infeasible; the field standard is
**TREC-style pooling** — judge the union of the top-N of multiple systems. But pooling
**systematically under-finds relevant docs and overstates recall**: many relevant documents are
missed, especially for topics with many relevant docs, and recall is generally overestimated by
pooled collections; high fractions of unjudged docs hinder recall-oriented evaluation and
reusability for systems that did not contribute to the pool
([Buckley & Voorhees / pooling-bias survey](https://link.springer.com/article/10.1007/s10791-007-9032-x);
[Rau & Kamps, TREC-30 recall aspects](https://trec.nist.gov/pubs/trec30/papers/UAmsterdam-DL.pdf)).
This is exactly the failure mode `ir-recall-measure.md` §(f) (codex round-3) guards against: a
**pooling-only** denominator silently drops required evidence that *no* mode surfaces, making the
metric self-confirming on the hard queries the eval exists to catch.

**Mitigation = seed-then-pool (already the FathomDB methodology).** The Recall@K denominator MUST
be an **authored required-evidence set per query, independent of retrieval**, *always* present
whether or not any mode surfaces it. Pooling only **augments** (discovers extra positives the
authors missed); it can never remove a seeded required positive. For FathomDB this is doubly
natural because the reusable QA sources **already carry the authoritative positive** (the
upstream-annotated `evidence_doc_ids`), so the denominator is seeded from human/dataset annotation,
not from pooling.

### B.2 LLM-as-judge for relevance labeling — known biases and mitigations

LLM relevance assessors (Bing's UMBRELA / Thomas et al.) can reach high **system-ranking**
correlation with humans (Kendall's τ / Spearman's ρ ≈ 0.8–0.9), and Thomas et al. report LLM
labels "as accurate as human labellers" for *finding the best systems and hardest queries*
([Thomas et al., SIGIR'24](https://arxiv.org/pdf/2309.10621);
[UMBRELA / Judging the Judges](https://arxiv.org/pdf/2502.13908)). **But** at the
*judgment level* agreement is only fair (Cohen's κ ≈ 0.3–0.5), and there is a consistent,
documented **over-rating / inflation** of relevance:

- **Keyword-stuffing / lexical-overlap bias:** LLM judges systematically over-rate non-relevant
  passages that merely contain query terms
  ([When LLM Judges Inflate Scores](https://arxiv.org/pdf/2602.17170);
  [Benchmarking LLM-based Relevance Judgment Methods](https://arxiv.org/pdf/2504.12558)).
- **Self-preference / circularity:** LLM-generated labels can be biased toward rankers that
  themselves use LLMs (or the same embedder family); optimizing toward LLM labels risks
  over-fitting the judge's idiosyncrasies rather than true relevance
  ([Thomas et al.](https://arxiv.org/pdf/2309.10621);
  [MLFrontiers — the LLM-judge controversy](https://mlfrontiers.substack.com/p/the-llm-judge-controversy)).
  For FathomDB this is **acute and specific**: the headline mode is RRF-hybrid over a *vector*
  branch; an LLM judge asked "is this doc relevant?" will tend to agree with whatever the embedder
  surfaced, inflating recall on exactly the embedder's strengths and hiding its blind spots.
- **There is an explicit skeptic camp:** ["Don't Use LLMs to Make Relevance
  Judgments"](https://arxiv.org/html/2409.15133) argues against fully-automated qrels for
  reusable, high-stakes collections; ["Topic-Specific Classifiers are Better Relevance Judges than
  Prompted LLMs"](https://arxiv.org/pdf/2510.04633) shows prompted LLMs are not always the best
  tool. The pragmatic middle ground is **LLM-assisted, human-validated** labeling
  ([LLM-Assisted Relevance Assessments — when to ask the LLM](https://arxiv.org/pdf/2411.06877);
  [Principles & Guidelines for LLM Judges, ICTIR'25](https://dl.acm.org/doi/10.1145/3731120.3744588)).

**Mitigations FathomDB should adopt:**
1. **Generation ≠ judging.** Use the LLM to *generate* a query + cite verbatim evidence **from a
   given doc** (a constrained generation task with a verifiable answer), **not** to score
   "is X relevant?" over retrieval output. This sidesteps the embedder-circularity and
   keyword-stuffing biases entirely, because the gold is authored from the document, blind to any
   retriever.
2. **Anti-hallucination grounding:** every evidence pointer must be a **real corpus `doc_id`** and
   a **verbatim substring** of that doc's `body` (programmatically re-verified). This is the single
   most important defense and is mechanically checkable.
3. **Decouple the judge model from the embedder** when any pooling-augmentation judging is done
   (use a different model family than the retrieval embedder, and report it).
4. **Human/HITL spot-validation** on a sampled subset; report **inter-annotator agreement** (κ)
   between LLM labels and the human sample before trusting any number — the roadmap already calls
   IR-C labeling "human/HITL".

### B.3 Reusing existing QA datasets (EnronQA/QASPER/QAConv/QMSum) as retrieval gold

This is exactly how BEIR/MTEB are built — aggregate public QA/retrieval datasets and treat the
question as the query and the annotated answer-bearing passage/doc as the relevant judgment
([BEIR](https://arxiv.org/pdf/2104.08663);
[Resources for Brewing BEIR](https://arxiv.org/pdf/2306.07471)). **Two well-known pitfalls apply
directly to FathomDB's sources:**

- **Sparse judgments → false negatives.** Most BEIR datasets have *sparse* judgments (e.g. SciFact
  ~339 judgments); a query is often labeled with **one** relevant doc even when others answer it,
  so a retriever that surfaces a *different* correct doc is wrongly penalized. FathomDB's reusable
  sources are all **single-evidence-doc** (enronqa/qaconv/qmsum each carry exactly one
  `evidence_doc_ids` per row) — **classic sparse-judgment risk.** Pooling-augmentation (B.1) is the
  standard remedy: discover the missed equally-correct docs and add them (as `supporting` or
  additional `required`), but never remove the seeded one.
- **Answer-leakage / over-easy retrieval.** BEIR's own analysis warns that "answer retrieval"
  datasets (relevant doc = exact answer to the question) **overestimate** retrieval performance,
  because answers written for a question share vocabulary with it
  ([Benchmarking IR on complex tasks](https://arxiv.org/html/2509.07253v1)). enronqa/qaconv
  questions are often paraphrases of the source — recall on them will read optimistically; this
  should be flagged in the report, and the generated fresh queries should deliberately include
  **lexically-divergent** phrasings (B.5) to stress the embedder, not flatter it.

### B.4 Span/passage- vs document-level relevance

The 2024–2026 RAG-eval trend is **span-attached / span-level** evidence and attribution — each
retrieved span carries its own verdict, claims are linked to source spans for groundedness
([RAG-eval 2026 guides](https://futureagi.com/blog/what-is-rag-evaluation-2026);
[Braintrust RAG metrics](https://www.braintrust.dev/articles/rag-evaluation-metrics)).
**However, FathomDB does not chunk** — it stores and returns whole node bodies, so the *scoring*
unit is unavoidably the document (`ir-recall-measure.md` §(b); `ir_eval.rs` `locator` is
"recorded for audit but NOT load-bearing"). Recommendation: **author spans anyway** (verbatim
substring + char offsets) as label provenance and anti-hallucination proof, store them in
`locator`/`evidence_spans`, but **score at doc granularity**. The spans cost little, harden the
labels, and become load-bearing for free if FathomDB ever chunks (0.8.1+). This matches the
acquisition schema's existing `evidence_spans: [{doc_id,start,end,text}]` slot — currently empty
everywhere, so adding spans is pure upside.

### B.5 Graded vs. binary relevance

nDCG (already reported by eu8) needs **graded** qrels to be meaningful; binary qrels make nDCG
degenerate ([graded vs binary / DCG@k–NDCG@k](https://towardsdatascience.com/how-to-evaluate-retrieval-quality-in-rag-pipelines-part-3-dcgk-and-ndcgk/)).
The measure doc flags this: current chain labels are binary, so nDCG is "report-only until graded
labels exist." IR-B already encodes a **two-grade** scheme via `necessity` (`required` vs
`supporting`) — that is the right minimal graded structure for a non-chunking store. Recommendation:
keep **binary required/not** for the *recall* numbers (strict + graded-fraction over the required
set), and use `required`/`supporting` as a coarse 2-level grade for nDCG. Do **not** invent a fine
0–3 graded scale yet — it raises annotation cost and inter-annotator disagreement
([UMBRELA accuracy drops at higher grades](https://arxiv.org/pdf/2502.13908)) without a chunking
store to exploit it.

### B.6 Inter-annotator agreement & validation

Report agreement explicitly. For the *reused* dataset labels, agreement is inherited from the
upstream dataset's own annotation (QASPER/QMSum are human-annotated; EnronQA is
LLM-generated-then-curated — note that provenance). For any *LLM-generated* fresh labels, validate
a **sampled human subset** and report **Cohen's κ** vs the human sample before any number is
trusted; treat κ < ~0.4 as a red flag ([UMBRELA κ ≈ 0.3–0.5 even for Bing's tuned
system](https://arxiv.org/pdf/2502.13908);
[Principles & Guidelines, ICTIR'25](https://dl.acm.org/doi/10.1145/3731120.3744588)). Always pin
the **corpus hash + qrels version** with every reported number (the GA-halt lesson,
`ir-recall-measure.md` §(f)).

---

## Recommended plan (reuse / adapt / generate)

### Top-line recommendation

**Hybrid, seed-first, LLM-assisted-but-human-validated**, in three tiers, all emitting the IR-B
`GoldQuery` schema pinned to `corpus_hash = fe973fcd…`:

**Tier 1 — REUSE the resolved dataset annotations as the seed denominator (no LLM, highest trust).**
Transform the existing eval rows into `GoldQuery` via the mechanical mapping in A.4:
- **enronqa** 710 → `exact_fact` (span answers over single email); evidence → one `required` unit.
- **qaconv** ~2178 non-abstain → `exact_fact` (span) / `exploratory` (free_form); 125 `abstain`
  rows → **negative-class candidates** (empty denominator).
- **qmsum** 1584 → `exploratory`/`summarizes` (these are summaries, not exact facts); evidence →
  the transcript doc as one `required` unit.
- **chains** ~300 `ground_truth_queries` → keep as the **eu8 doc-granularity baseline**
  (`commitment`/`action` shapes via `relation_type`), legacy `expected_top_k_doc_ids` path.
This alone yields **~4,500 fact/doc-level gold queries** with corpus-resolved evidence and **zero
hallucination risk**, covering enronqa(200)/qaconv(1250)/qmsum(200-transcript) docs + chains.
Add **verbatim spans** for enronqa/qaconv (substring-locate the span `answers` in the evidence
`body`) to fill the empty `evidence_spans`/`locator` slot.

**Tier 2 — REPAIR qasper (highest-value recovery, no fresh labeling).** The 1585 paper docs have
~5k upstream extractive QA with `highlighted_evidence` that the acquire parser dropped (A.2). Fixing
`acquire_qasper.py`'s answer extraction recovers paper-domain `exact_fact`/`span` gold **with
verbatim evidence spans, human-annotated upstream** — strictly better than generating them. *This is
a source-script fix, out of IR-C's "no source changes" scope to execute, but it is the #1
recommendation to flag to the orchestrator.* If the repair is out of scope, qasper falls to Tier 3.

**Tier 3 — GENERATE fresh labels only for the uncovered source-buckets.** The ~8,085 docs with no
QA — **cnn_dailymail (2500 article), enron (2000 email), bahmutov_dailylogs (300 note),
landes_todos (500 todo), synthetic_notes (1200 note)**, and **qasper if not repaired** — need fresh
fact-level labels. Use the companion generation prompt
(`dev/plans/prompts/IR-C-fact-level-gold-label-generation.md`):
- **LLM generates, never judges** (B.2): given a corpus doc, produce 1–3 grounded queries + a
  verbatim evidence span; the model never sees or scores retrieval output.
- **Anti-hallucination is mechanical:** evidence `doc_id` must exist in the snapshot; span must be a
  verbatim substring of that `body`; re-verified programmatically; rows failing verification are
  dropped, not "fixed."
- **Class + difficulty diversity:** spread across `exact_fact`/`action`/`commitment`/`preference`/
  `exploratory`, plus a deliberate **negative** subset (queries whose answer is absent), and
  deliberately **lexically-divergent** phrasings to avoid the answer-leakage over-easiness (B.3).
- **Validate** a human-sampled subset and report κ before trusting numbers (B.6).

### Coverage / quotas (suggested, tune at IR-C run time)

| tier | source | docs | labels | class focus | provenance |
|---|---|---:|---:|---|---|
| 1 | enronqa | 200 | 710 | exact_fact | dataset (cache) |
| 1 | qaconv | 1250 | ~2178 + 125 neg | exact_fact / exploratory / negative | dataset (BSD) |
| 1 | qmsum | 200 | 1584 | exploratory / summarizes | dataset (cache) |
| 1 | chains | 371 | ~300 | commitment / action (eu8 baseline) | synthetic |
| 2 | qasper | 1585 | ~5000 (if repaired) | exact_fact / span | dataset (CC-BY) |
| 3 | cnn_dailymail | 2500 | ~1–2 / doc sampled | exact_fact / exploratory | LLM-generated |
| 3 | enron | 2000 | ~1–2 / doc sampled | exact_fact / action / commitment | LLM-generated |
| 3 | landes_todos | 500 | ~1 / doc | action / commitment | LLM-generated |
| 3 | bahmutov_dailylogs | 300 | ~1 / doc | exact_fact / exploratory | LLM-generated |
| 3 | synthetic_notes | 1200 | ~1 / doc | preference / action / exact_fact | LLM-generated |

Generation should **sample**, not exhaustively label, the large Tier-3 sources (cnn_dailymail,
enron) to keep cost bounded and per-class balance even — the measure is stratified by class
(`ir-recall-measure.md` §(d)), and a few hundred well-distributed queries per class is worth more
than thousands of skewed ones.

### Non-negotiables (carried into the prompt)

1. Output is the **IR-B `GoldQuery` schema**, in a `GoldSet` file pinned to
   `corpus_hash = fe973fcd49fbbda083158f69fe720f17858ab8528e171fa2188eec84131c7d4e`,
   `qrels_version = ir-c-...-vN`; must pass `validate_gold_set` (A.4).
2. **Seed denominator is authored, not pooled** (B.1). Pooling only augments.
3. **Every evidence pointer = real corpus `doc_id` + verbatim span** (B.2/B.4).
4. **negative** queries have an **empty** required denominator; non-negative have a non-empty one
   (validator invariant).
5. **License posture propagates:** enronqa/qmsum gold stays cache-only; the gold set records per-row
   `provenance` + `license`.
6. Pin **corpus hash + qrels version**; report **κ** on a validated sample before any number is
   used (B.6).

---

## Sources

- Thomas et al., *Large Language Models can Accurately Predict Searcher Preferences*, SIGIR'24 — https://arxiv.org/pdf/2309.10621
- *Judging the Judges* (UMBRELA / LLM-generated qrels) — https://arxiv.org/pdf/2502.13908
- *When LLM Judges Inflate Scores* — https://arxiv.org/pdf/2602.17170
- *Benchmarking LLM-based Relevance Judgment Methods* — https://arxiv.org/pdf/2504.12558
- *Don't Use LLMs to Make Relevance Judgments* — https://arxiv.org/html/2409.15133 (PMC: https://pmc.ncbi.nlm.nih.gov/articles/PMC11984504/)
- *Topic-Specific Classifiers are Better Relevance Judges than Prompted LLMs* — https://arxiv.org/pdf/2510.04633
- *LLM-Assisted Relevance Assessments: When Should We Ask LLMs for Help?* — https://arxiv.org/pdf/2411.06877
- *Principles and Guidelines for the Use of LLM Judges*, ICTIR'25 — https://dl.acm.org/doi/10.1145/3731120.3744588
- Pooling bias for large collections — https://link.springer.com/article/10.1007/s10791-007-9032-x
- Rau & Kamps, *Recall Aspects of Transformers for Text Ranking*, TREC-30 — https://trec.nist.gov/pubs/trec30/papers/UAmsterdam-DL.pdf
- BEIR — https://arxiv.org/pdf/2104.08663 ; Resources for Brewing BEIR — https://arxiv.org/pdf/2306.07471
- *Benchmarking IR Models on Complex Retrieval Tasks* (answer-leakage over-easiness) — https://arxiv.org/html/2509.07253v1
- RAG evaluation 2026 / span-level — https://futureagi.com/blog/what-is-rag-evaluation-2026 ; Braintrust RAG metrics — https://www.braintrust.dev/articles/rag-evaluation-metrics
- Graded vs binary / DCG@k–NDCG@k — https://towardsdatascience.com/how-to-evaluate-retrieval-quality-in-rag-pipelines-part-3-dcgk-and-ndcgk/
- The LLM-judge controversy — https://mlfrontiers.substack.com/p/the-llm-judge-controversy

## Internal references

- Measure: `dev/design/ir-recall-measure.md`
- Harness/schema/validator: `src/rust/crates/fathomdb-engine/tests/support/ir_eval.rs`
- Driver: `src/rust/crates/fathomdb-engine/tests/ir_recall_eval.rs`; fixture: `tests/fixtures/ir_gold/synthetic_gold.json`
- eu8 baseline: `src/rust/crates/fathomdb-engine/tests/eu8_ir_validation.rs`; `dev/notes/0.7.2-EU-8-ir-recall-design.md`
- Roadmap: `dev/plans/0.8.0-GA-and-IR-eval-roadmap.md`; scaffold `dev/plans/prompts/scaffolds/7-IR-B-C-D-recall-measure.md`; `dev/plans/prompts/0.8.x-IR-1-recall-measure.md`
- Corpus: `tests/corpus/snapshot.json`; `tests/corpus/scripts/manifest.json`; `tests/corpus/corpus-card.md`; QA exports `dev/plans/prompts/0.8.x-corpus-qa-expansion-handoff.md`
- Acquisition scripts: `tests/corpus/scripts/acquire_{enronqa,qaconv,qasper,qmsum}.py`; `_corpus_lib.py`
</content>
