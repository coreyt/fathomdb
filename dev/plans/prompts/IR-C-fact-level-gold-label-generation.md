# IR-C — Fact-level gold-label GENERATION prompt (reusable)

**What this is.** A precise, self-contained prompt for a future LLM run that produces **fact-level
gold labels** for the FathomDB IR recall eval, on the **frozen** corpus. The output is consumed
directly by the IR-B harness (`tests/support/ir_eval.rs::load_gold_set` →
`tests/crates/fathomdb-engine/tests/ir_recall_eval.rs`) and must pass `validate_gold_set`.

**Research basis:** `dev/notes/IR-C-fact-level-gold-labels-research.md`.
**Measure:** `dev/design/ir-recall-measure.md`.

**Methodology stance (load-bearing — do NOT deviate):**

- You **GENERATE labels from a given document; you do NOT judge retrieval output.** You never see,
  rank, or score any retriever's results. This avoids embedder-circularity / self-preference and
  keyword-stuffing bias documented for LLM-as-judge (research note §B.2).
- The recall **denominator is authored from the document, independent of retrieval** (seed-then-pool;
  research §B.1). Any later pooling step only *adds* positives — it can never remove a seeded one.
- **Anti-hallucination is mechanical and absolute:** every evidence pointer is a **real corpus
  `doc_id`** plus a **verbatim substring** of that doc's `body`. Re-verify programmatically; drop
  any row that fails — never "repair" it.

---

## 0. Pin (fill these in at run time from the frozen snapshot)

```
corpus_hash    = fe973fcd49fbbda083158f69fe720f17858ab8528e171fa2188eec84131c7d4e   # tests/corpus/snapshot.json
corpus_version = 0.8.x-B
qrels_version  = ir-c-<source-or-batch>-v1        # bump on any change
```

Every output file records `corpus_hash` + `qrels_version`. If `corpus_hash` is the
`TODO(COR-2-freeze)` placeholder, STOP — the set is not pinned and the validator will reject it.

---

## 1. Input — the corpus doc schema + which sources

You are given corpus documents (from `data/corpus-data/raw/<source>.jsonl`), each with this shape
(corpus-card §"Document schema"):

```jsonc
{ "doc_id": "<16 hex>", "source_type": "email|meeting|paper|article|note|todo",
  "title": "...|null", "body": "<the text that is embedded>", "created_at": "...",
  "author_or_sender": "...|null", "recipients": [...], "people_mentions": [...],
  "project_mentions": [...], "tags": [...], "thread_id": "...|null", "license": "...",
  "provenance": "..." }
```

**Scope — generate ONLY for the source-buckets that lack usable QA today** (research §A.5):
`cnn_dailymail` (2500 article), `enron` (2000 email), `landes_todos` (500 todo),
`bahmutov_dailylogs` (300 note), `synthetic_notes` (1200 note), and `qasper` (1585 paper) **only if
its acquire-script parser was not repaired** (the preferred path recovers ~5k human-annotated paper
QA — see research §A.2 / Tier 2; do not LLM-generate qasper labels if the dataset labels were
recovered).

**Do NOT regenerate** sources that already have resolved dataset labels — `enronqa`, `qaconv`,
`qmsum`. Those are transformed mechanically (research §A.4/Tier-1), not generated here. **Do NOT
ingest** these QA rows as corpus documents (eval-only).

**Sampling, not exhaustion.** For large buckets (`cnn_dailymail`, `enron`), **sample** docs
deterministically (sort by `doc_id`, stride-sample) to a per-source cap rather than labeling every
doc; the measure is class-stratified, so balanced coverage beats raw volume (research §"quotas").

---

## 2. Output — the exact JSON schema (one `GoldSet` file)

Emit a single JSON object. **This is the schema `parse_gold_set` reads — match keys exactly.**

```jsonc
{
  "corpus_hash": "fe973fcd49fbbda083158f69fe720f17858ab8528e171fa2188eec84131c7d4e",
  "qrels_version": "ir-c-<source>-v1",
  "note": "IR-C fact-level gold, <source>, LLM-generated, doc-grounded.",
  "queries": [
    {
      "query": "<natural-language question — the SAME `query` key eu8 reads>",
      "query_id": "ir-c-<source>-<zero-padded-seq>",         // globally unique across the set
      "query_class": "commitment|action|exact_fact|preference|exploratory|negative",
      "required_evidence": [
        {
          "evidence_id": "<unique within THIS query, e.g. ev-<query_seq>-a>",
          "doc_id": "<a REAL corpus doc_id from the frozen snapshot>",
          "necessity": "required|supporting",               // only `required` is in the recall denominator
          "locator": { "kind": "span" }                     // "span" when you cite text; "whole_body" otherwise
        }
      ],
      "expected_top_k_doc_ids": ["<doc_id>", "..."],          // PRESERVED eu8 view: the distinct `required` doc_ids
      "relation_type": "action_from|contradicts|follows_up_on|mentions|summarizes|cites",
      "chain_shape": "single",                                 // "single" for one-doc facts; chain shape if multi-doc

      // ── AUDIT-ONLY fields (NOT parsed by the harness; carried for verification + future chunking) ──
      "answer": "<the gold answer string>",
      "answer_type": "span|free_form|yes_no_maybe|summary|abstain",
      "relevance_grade": "required|supporting",               // coarse 2-level grade (research §B.5)
      "evidence_spans": [
        { "doc_id": "<doc_id>", "start": 0, "end": 0, "text": "<VERBATIM substring of that doc body>" }
      ],
      "provenance": "llm-generated:<model-id>@<run-date>",
      "license": "<inherit the evidence doc's license SPDX/LicenseRef>"
    }
  ]
}
```

**Field rules (the harness-binding subset):**

- `query` — non-empty. `query_id` — globally unique in the file. `query_class` — one of the six.
- `required_evidence[].evidence_id` — unique **within the query**. `.doc_id` — non-empty, **must
  exist in the frozen snapshot**. `.necessity` — `required` puts the doc in the recall denominator;
  `supporting` is a separate corroboration diagnostic (NOT in any recall number).
- `expected_top_k_doc_ids` — set it to the distinct `required` `doc_id`s (keeps the eu8 doc-view
  consistent; the harness uses `required_evidence` when present and falls back to this only if
  `required_evidence` is absent).
- **`locator`/`evidence_spans` are recorded for audit but are NOT load-bearing for the score** —
  FathomDB does not chunk; presence is at doc-body granularity. Author spans anyway: they are the
  anti-hallucination proof and become load-bearing if FathomDB ever chunks.

**Class/denominator coherence (validator will reject violations):**

- A **non-`negative`** query MUST have a **non-empty** `required` denominator (≥1 unit with
  `necessity=required`).
- A **`negative`** query MUST have an **EMPTY** `required_evidence` and empty `expected_top_k_doc_ids`
  (its correct retrieval behavior is to return nothing / abstain). Set `answer_type: "abstain"`,
  `answer: ""`.

---

## 3. Grounding / anti-hallucination rules (absolute)

For **every** non-negative query:

1. Pick **one specific document** as the evidence source. Its `doc_id` goes in
   `required_evidence[].doc_id` and **must be a real `doc_id` in the frozen snapshot**.
2. The answer must be **fully supported by that document's `body`.** Do not use outside knowledge.
3. Provide a **verbatim span**: `evidence_spans[].text` must be an **exact, character-for-character
   substring** of the evidence doc's `body` (copy it; do not paraphrase, normalize whitespace, or
   re-case). `start`/`end` are the substring's char offsets into `body`.
4. **Multi-doc facts:** if the answer genuinely requires two+ docs, add one `required` unit per doc;
   set `chain_shape` accordingly and `relation_type` to the cross-doc relation. Use `supporting`
   for docs that corroborate but are not strictly necessary.
5. **Never invent a `doc_id`, never cite a span that is not verbatim, never answer from world
   knowledge.** A row that cannot be grounded this way is **dropped**, not guessed.

For **negative** queries: write a plausible question whose answer is **genuinely absent** from the
entire corpus (e.g. a fact about an entity the corpus never discusses). Leave evidence empty.

---

## 4. How many queries per doc / source

- **Default: 1–3 queries per selected document.** Prefer 1 high-quality grounded query over several
  weak ones. Stop at 1 for short docs (todos, fleeting notes); up to 3 for long docs
  (cnn_dailymail articles, enron threads, qasper papers).
- **Per-source caps (tune at run time; see research §"quotas"):** sample large buckets rather than
  labeling exhaustively — e.g. cnn_dailymail/enron ~1–2 queries/doc over a deterministic sample;
  landes_todos/bahmutov ~1/doc (small enough to cover fully); synthetic_notes ~1/doc.
- **Negative quota:** include a deliberate **negative subset** (~5–10% of the set) so the
  abstention-correctness bucket is populated (measure §(d)). Negatives are authored, not
  doc-derived.

---

## 5. Difficulty + diversity guidance

- **Class spread (cover all six):**
  - `exact_fact` — a specific number/name/date/entity retrievable from one doc.
  - `action` — the next action a note/email/todo implies (`relation_type: action_from`).
  - `commitment` — a promise/obligation + its parties + due date (emails/meetings).
  - `preference` — a stated standing preference/instruction (synthetic_notes, personal-crm/decision-log).
  - `exploratory` — open-ended "what do I know about X" (articles, papers, meeting notes).
  - `negative` — answer absent (authored).
- **Map class to natural source fit:** enron/qaconv → commitment/action/exact_fact; cnn_dailymail/
  qasper → exact_fact/exploratory; landes_todos → action/commitment; synthetic_notes →
  preference/action/exact_fact.
- **Anti-leakage (research §B.3):** do **not** echo the doc's wording in the query. Paraphrase,
  use synonyms, ask indirectly — answer-shaped queries that reuse the doc's vocabulary make
  retrieval look artificially easy and hide embedder weaknesses. Vary phrasing difficulty:
  ~⅓ easy/literal, ~⅓ paraphrased, ~⅓ requiring inference over the doc.
- **Diversity:** spread across many distinct `doc_id`s and `thread_id`s; do not cluster many
  queries on a few docs.
- **Grades:** keep recall binary (`required` vs not). Use `supporting` sparingly for genuine
  corroboration only — it feeds the supporting-coverage diagnostic and a coarse 2-level nDCG, not
  the headline recall (research §B.5). Do not invent a fine 0–3 scale.

---

## 6. Self-check / validation step (run before emitting; emit a report)

After generating, **programmatically verify** and **drop any failing row** (report counts):

1. **Schema parse:** the file loads via `parse_gold_set` (well-formed `GoldSet` + `queries`).
2. **Pinning:** `corpus_hash` == the frozen snapshot hash (not `TODO(COR-2-freeze)`);
   `qrels_version` present.
3. **doc_id existence:** every `required_evidence[].doc_id` AND every `evidence_spans[].doc_id`
   exists in the frozen snapshot's doc-id set. (Build the set from `data/corpus-data/raw/*.jsonl`.)
4. **Verbatim span:** every `evidence_spans[].text` is an exact substring of that doc's `body`, and
   `body[start:end] == text`. Mismatch → drop the row.
5. **Uniqueness:** `query_id` unique across the file; `evidence_id` unique within each query.
6. **Class/denominator coherence:** non-`negative` ⇒ non-empty `required` denominator;
   `negative` ⇒ empty denominator. (Mirror `validate_gold_set`.)
7. **`expected_top_k_doc_ids`** equals the distinct `required` `doc_id`s for that query.
8. **No-leakage sanity (soft):** flag queries that are near-verbatim copies of their evidence span
   for rephrasing.
9. **License propagation:** each row's `license` matches its evidence doc's license; enronqa/qmsum-
   derived rows stay cache-only (N/A here since those aren't generated, but enforce for any source).

**Emit a validation report** alongside the gold file: total queries, per-source/per-class counts,
negatives count, rows dropped (with reason tallies: bad doc_id / non-verbatim span / coherence),
and the final `corpus_hash` + `qrels_version`. Then run the IR-B validator
(`validate_gold_set`) and confirm **zero issues** before the set is used. For any LLM-generated
batch, **human-sample-validate a subset and report Cohen's κ** vs the human labels before the
numbers are trusted (research §B.6); treat κ < ~0.4 as a stop-and-review signal.

---

## 7. Worked example (one grounded `exact_fact` query)

Given an `enron` email doc `doc_id="6c08397c1d6a3e6e"` whose `body` contains the verbatim sentence
`"We received a termination notice from Ameren today."`:

```jsonc
{
  "query": "Which counterparty sent a termination notice in the Panus thread?",
  "query_id": "ir-c-enron-000042",
  "query_class": "exact_fact",
  "required_evidence": [
    { "evidence_id": "ev-000042-a", "doc_id": "6c08397c1d6a3e6e",
      "necessity": "required", "locator": { "kind": "span" } }
  ],
  "expected_top_k_doc_ids": ["6c08397c1d6a3e6e"],
  "relation_type": "mentions",
  "chain_shape": "single",
  "answer": "Ameren",
  "answer_type": "span",
  "relevance_grade": "required",
  "evidence_spans": [
    { "doc_id": "6c08397c1d6a3e6e", "start": 1234, "end": 1287,
      "text": "We received a termination notice from Ameren today." }
  ],
  "provenance": "llm-generated:<model-id>@2026-06-09",
  "license": "LicenseRef-Enron-Research-Use"
}
```

Note: the query paraphrases ("which counterparty"/"termination notice") rather than copying the
sentence verbatim (anti-leakage), the answer is a single fact in the body, and the span is an exact
substring whose offsets satisfy `body[start:end] == text`.

---

## 8. Done criteria

- One `GoldSet` JSON file per source-batch, pinned to the frozen `corpus_hash`, that **parses and
  passes `validate_gold_set` with zero issues**.
- All evidence `doc_id`s resolve to the snapshot; all spans verbatim-verified; failing rows dropped
  and tallied in the validation report.
- Class coverage spans all six classes with a populated negative subset; phrasing diversity per §5.
- A validation report + (for LLM-generated batches) a human-sample κ accompany the gold file.
- No corpus docs modified, no QA rows ingested as documents, no retrieval run/scored during
  generation.
</content>
