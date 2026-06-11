# IR-C — Test-query-quality instrumentation plan (items 1–3)

Status: **in progress** · Drafted 2026-06-11 · Branch `claude/recent-changes-state-a6wth3`

**Landed:** WI-2 (query tracers) + WI-3a (evidence-span locators) — gold schema +
build script + tests (`6f39b3a`). WI-1L (lexical diagnostics harness + shared
`ir_retrieval.rs` seams) — model-free `bm25_gold_rank`/`idf_overlap` sidecar with
unit tests (`72643f8`). **Pending:** gold regeneration (`ir-c-reused-v2`) + the
real-corpus lexical run — both need the gitignored corpus, so they run where the
data lives. **Next:** WI-1D + WI-3b (dense ranks, buckets, chunk offsets,
passage↔span overlap).

## Why

The fusion slice showed the hybrid is "lexical-bound" — the content-OR/BM25 arm
is ~the exploratory ceiling and the chunked dense arm adds little. Before we act
on that, we need to know how much of it is a **property of the test set** vs the
retriever (see the discussion captured in `IR-C-api-surface-knobs-to-review.md`
and the sparse-judgment / lexical-overlap-bias risks already named in
`dev/notes/IR-C-fact-level-gold-labels-research.md:220,263`). This plan builds the
*objective instrumentation* to measure that, in three landable pieces:

1. **Per-query diagnostics sidecar** — quantify lexical easiness, semantic
   necessity, and the lexical/semantic/hard query mix.
2. **Query tracer schema hygiene** — promote `source`/`answer_type` into the gold
   struct and add `query_origin` (lock leakage provenance before synthetic queries
   exist).
3. **Passage locators + span-overlap metric** — carry evidence-span offsets through
   the gold, record chunk char-spans in the harness, and measure whether the dense
   arm matches the *right span*, not just the right doc.

**Item 4** (unlabeled-positive / pooling-augmentation label audit) is deferred and
gets its own plan once 1–3 land; this plan leaves a hook for it.

## Goals / non-goals

**Goals**
- Turn "is the benchmark lexically biased?" into committed numbers, recomputable
  and pinned to the frozen corpus.
- Zero change to the *production* retrieval/index path. All new metadata is
  eval-only.
- Preserve the gold's "zero-generation, deterministic, `corpus_hash`-pinned"
  guarantee; keep model-dependent diagnostics out of the gold.

**Non-goals**
- No production chunking / index-schema change. #3 is the *measurement* half of the
  parked locator item (#8); the *product* half stays deferred with the chunking
  decision (it must not be prejudged by this work).
- No new labels / LLM judging (that is item 4).

## Design (before any code)

### Architecture & the gold/sidecar split

Two artifacts, two different contracts:

| Artifact | Contract | Mutability |
|---|---|---|
| `data/corpus-data/eval/ir_gold/all.gold.json` | zero-generation, deterministic, pinned to `corpus_hash`. Human/dataset-authored only. | changes only when gold schema/labels change → bump `qrels_version` |
| `data/corpus-data/eval/ir_gold/all.gold.diagnostics.json` (NEW) | *derived*. Pinned to `corpus_hash` **and** (for dense fields) `embedder_identity` + `scope`. | recomputed by a tool anytime; never hand-edited |

**Why the split:** `bm25_gold_rank` is deterministic given the corpus, but
`dense_gold_rank` depends on which embedder we run — putting it in the gold would
break the zero-generation guarantee and make the gold non-reproducible across model
swaps. The sidecar is the right home for everything *measured*.

**Leakage guardrail (validity-critical):** the sidecar is eval-only and is *never*
visible to any retriever path, exactly like the gold. We do **not** denormalise
gold/eval pairing (e.g. `is_evidence_for`) into the production FTS/vec0 tuples —
the join key (`doc_id`) already exists on both sides, and coupling the index to the
benchmark would let the retriever "see" which docs are gold. The join stays in the
harness.

### Two diagnostic tiers (this is what makes it tractable)

- **Lexical tier — model-free, full-corpus, cheap.** Seeding the corpus + querying
  FTS5 needs **no embedding** (full-mode already freezes the vector projection and
  FTS is synchronous), so the full 10,506-doc lexical sidecar is a minutes-scale
  job and is the *authoritative* lexical truth. Runs in CI.
- **Dense tier — model-dependent, expensive, scope-labeled.** `dense_gold_rank` +
  buckets + span-overlap need full-corpus passage embeddings (the costly job we keep
  fighting). It piggybacks on experiment runs we already do (reusing in-memory
  embeddings — no extra cost) and is written with `scope` = `full` or
  `slice@<docs_seeded>` so a reduced-corpus dense section is never mistaken for full.

Net: the lexical sidecar lands cheap and complete; the dense section fills in
opportunistically and is always honestly scoped.

### Computation home: one Rust harness, shared seams

Both tiers must use the **same** retrieval logic as the fusion experiment, or the
ranks won't be comparable. Refactor the experiment's proven helpers
(`compile_content_or`, `fts_bodies`, `chunk_words`, `knn_docs_pool`, the
stopword/IDF tokenizers) out of `tests/ir_c_fusion_experiment.rs` into
`tests/support/ir_retrieval.rs`, and have both the experiment and a new
`tests/ir_c_gold_diagnostics.rs` consume them. The diagnostics harness:
- **default (lexical)**: seed corpus → for each gold query, content-OR FTS rank of
  each required gold doc → write/refresh the lexical section. No embedder.
- **`IRC_DIAG_DENSE=1`**: also embed (whole + 128/96), compute dense gold ranks,
  buckets, and (for span-locator queries) passage↔evidence-span overlap → write the
  dense section tagged with embedder identity + scope.

### Data model

**Gold additions (`ir_eval.rs` structs + `build_ir_gold.py` emit):**
```jsonc
{
  "query_id": "...", "query": "...", "query_class": "exploratory",
  "source": "qmsum",            // #2: promoted from the dropped "_source"
  "answer_type": "summary",     // #2: promoted from the dropped "_answer_type"
  "query_origin": "human_dataset", // #2: {human_dataset|llm_generated|templated}
  "required_evidence": [
    { "evidence_id": "...", "doc_id": "...", "necessity": "required",
      "locator": { "kind": "span",
                   "spans": [ {"doc_id":"...","start":1234,"end":1290} ] } } // #3: was dropped
  ]
}
```

**Sidecar (`all.gold.diagnostics.json`):**
```jsonc
{
  "corpus_hash": "fe973fcd…",            // must match snapshot.json or refuse
  "qrels_version": "ir-c-reused-v2",
  "lexical": {                            // model-free, full-corpus
    "scope": "full", "n_docs": 10506,
    "per_query": { "qmsum:123": {
        "bm25_gold_rank": 1,             // min rank over required gold docs; null = unranked
        "idf_overlap": 0.92,             // IDF-weighted query∩gold-doc / query content terms
        "gold_doc_tokens": 5123,
        "gold_locator_kind": "whole_body"
    }},
    "summary": { "bm25_rank1_frac": 0.41, "median_bm25_gold_rank": 3, "...": "..." }
  },
  "dense": {                              // model-dependent, scope-labeled
    "embedder_identity": "bge-small-en-v1.5/…", "scope": "slice@1200",
    "per_query": { "qmsum:123": {
        "dense_gold_rank_whole": 14, "dense_gold_rank_128_96": 6,
        "bucket": "lexical",             // see rule below
        "passage_evidence_iou": null     // span queries only
    }},
    "summary": { "bucket_counts": {"lexical": 0, "semantic": 0, "hard": 0},
                 "mean_passage_evidence_iou": null }
  }
}
```

### Metric definitions (objective, pinned)

Let `Qc` = query content tokens (`content_tokens()` — ≥3 chars, stopwords removed),
`D` = gold-doc token set, `df(t)` = document frequency over the frozen corpus,
`N` = corpus size, BM25 idf `idf(t) = ln((N−df+0.5)/(df+0.5) + 1)`.

- **`bm25_gold_rank`** = rank (1-based) of the best required gold doc in the
  content-OR + FTS5 `bm25()` ranking over the full corpus; `null` if outside the
  scan cap. *Lexical easiness — rank 1 = trivially lexical.*
- **`idf_overlap`** = `Σ_{t∈Qc∩D} idf(t) / Σ_{t∈Qc} idf(t)` ∈ [0,1]. Raw coverage
  saturates on long docs; IDF-weighting measures whether the *discriminative* query
  terms are present.
- **`dense_gold_rank_{geom}`** = rank of the best gold doc under the pooled passage
  KNN for that geometry. *Lower than BM25's rank ⇒ dense-favoring.*
- **`bucket`** (needs both ranks, cap `C`): `lexical` if `bm25_gold_rank ≤ C`;
  else `semantic` if `dense_gold_rank_128_96 ≤ C`; else `hard`. The **`semantic`
  count is the headline** — a benchmark that can justify a vector arm must have a
  non-trivial semantic bucket. (`C` defaults to the deepest K-ladder rung, 50.)
- **`passage_evidence_iou`** (span-locator queries only) = char-span IoU between the
  evidence span and the dense arm's best-scoring passage on that doc. *Right span,
  not just right doc.* Mean reported over exact_fact (where spans are dense).

### Determinism & validity invariants
- Lexical sidecar is bit-reproducible given `corpus_hash`; the tool refuses to run
  if the on-disk corpus hash ≠ `snapshot.json` (mirrors `build_ir_gold.py`).
- Dense sidecar is reproducible given corpus + serialized embedder; always carries
  `embedder_identity` + `scope`.
- Gold schema additions do **not** change `corpus_hash` (that hashes corpus bytes,
  not gold) but **do** bump `qrels_version` → `ir-c-reused-v2`.
- New gold fields are **optional on read** (back-compat): old gold without
  `query_origin`/`spans` still parses.

### Touched files
- `src/rust/crates/fathomdb-engine/tests/support/ir_eval.rs` — `GoldQuery`
  {`source`,`answer_type`,`query_origin`}, `Locator.spans`, parse + validate.
- `src/rust/crates/fathomdb-engine/tests/support/ir_retrieval.rs` — NEW, extracted
  shared seams.
- `src/rust/crates/fathomdb-engine/tests/ir_c_fusion_experiment.rs` — consume the
  extracted seams (no behaviour change).
- `src/rust/crates/fathomdb-engine/tests/ir_c_gold_diagnostics.rs` — NEW harness.
- `tests/corpus/scripts/build_ir_gold.py` — emit `source`/`answer_type`/
  `query_origin`, carry `spans`, bump `qrels_version`.
- `data/corpus-data/eval/ir_gold/*.gold.json` — regenerated (gitignored).

## Work items (TDD — failing test first, then implement)

Landing order is dependency order. Each item is independently shippable.

### WI-2 — Query tracer schema hygiene  *(do first; cheap, foundational)*

**Design notes.** Promote the already-emitted `_source`/`_answer_type` (currently
dropped by `parse_gold_set`) to first-class optional fields, and add `query_origin`
defaulting to `human_dataset` for the reuse tier. `validate_gold_set` warns (not
fails) on missing `query_origin` to stay back-compat.

**Tests first** (`ir_eval.rs` unit tests):
- `parse_promotes_source_and_answer_type` — a gold row with `source`/`answer_type`
  populates the struct; a legacy row with `_source`/`_answer_type` still parses via
  fallback.
- `parse_query_origin_default` — missing `query_origin` ⇒ `human_dataset`; explicit
  `templated`/`llm_generated` round-trips; unknown value ⇒ parse error.
- `validate_flags_unknown_origin` — invalid origin is reported by
  `validate_gold_set`.
- `build_ir_gold` Python: add a tiny fixture test asserting emitted rows contain
  `source`,`answer_type`,`query_origin` and `qrels_version == ir-c-reused-v2`.

**Implement.** Add enum `QueryOrigin` + fields to `GoldQuery`; extend
`parse_gold_set`/`parse_evidence`; update `build_ir_gold.py` emit + version bump;
regenerate gold; re-run the existing IR-B/IR-C harness smoke to confirm nothing
regressed.

**Done when:** new fields parse + validate, gold regenerates deterministically,
`qrels_version` bumped, existing recall numbers unchanged.

### WI-3a — Carry evidence-span offsets into the gold  *(schema; bundle regen with WI-2)*

**Design notes.** `_corpus_lib.py` already validates `evidence_spans`
{`doc_id`,`start`,`end`}; `build_ir_gold.py` discards them. Carry them into
`locator.spans`. `ir_eval.rs` `Locator` gains `Option<Vec<Span>>`.

**Tests first:**
- `parse_locator_spans` — a `span` locator round-trips its offsets; a `whole_body`
  locator has `spans: None`.
- `validate_span_bounds` — `end ≥ start ≥ 0` and `span.doc_id == evidence.doc_id`,
  else a validation issue.
- Python fixture: a `span` QA row emits `locator.spans`; an abstain/summary row does
  not.

**Implement.** Extend the Python transform + the Rust `Locator`/parse/validate;
fold the regen into WI-2's single `qrels_version` bump.

**Done when:** span offsets survive into `all.gold.json` and validate.

### WI-1L — Lexical diagnostics harness + sidecar  *(cheap, full-corpus, CI-able)*

**Design notes.** Extract shared seams to `ir_retrieval.rs` first (pure refactor,
guarded by the existing experiment test still passing). New
`ir_c_gold_diagnostics.rs` seeds the corpus, computes `bm25_gold_rank`,
`idf_overlap`, `gold_doc_tokens`, `gold_locator_kind` per query, plus the corpus
summary, and writes the `lexical` section pinned to `corpus_hash`.

**Tests first:**
- `ir_retrieval` refactor: assert extracted `compile_content_or`/`fts_bodies`/
  `chunk_words` behave identically (golden-string + the experiment test staying
  green is the regression guard).
- `bm25_gold_rank_on_synthetic` — a tiny in-test corpus where a doc is the obvious
  lexical match ⇒ rank 1; a doc with no query-term overlap ⇒ `null`.
- `idf_overlap_weights_rare_terms` — a query sharing only a stopword-like
  (low-IDF) term scores ≪ one sharing a rare term, on a controlled corpus.
- `sidecar_refuses_corpus_hash_mismatch` — wrong `corpus_hash` ⇒ skip/error, no
  write.
- `lexical_sidecar_shape` — emitted JSON has the documented keys + summary.

**Implement.** Write the harness (gated like the experiment: `IRC_RUN`/skip);
compute over the full corpus; assert hash; write sidecar. Add the summary
aggregates (`bm25_rank1_frac`, median rank, idf-overlap histogram).

**Done when:** `all.gold.diagnostics.json` `lexical` section is produced over the
full corpus, deterministic, hash-guarded; the `bm25_rank1_frac` number directly
answers "how many queries are lexically trivial."

### WI-1D + WI-3b — Dense diagnostics, chunk offsets, span-overlap

**Design notes.** Teach `chunk_words` to return `(text, char_start, char_end)` so
each passage tuple carries its span (the harness-side "passage locator"). In the
diagnostics harness under `IRC_DIAG_DENSE=1`, compute `dense_gold_rank_{whole,
128_96}`, the `bucket`, and — for span-locator queries — `passage_evidence_iou`
against the WI-3a spans. Write the `dense` section tagged with `embedder_identity`
+ `scope`. Reuse the same embeddings the fusion experiment already builds.

**Tests first:**
- `chunk_words_reports_offsets` — offsets are monotonic, cover the body, and slice
  back to the chunk text (`body[start..end] == chunk`), including the short-body
  single-chunk and `usize::MAX` whole-doc cases.
- `dense_gold_rank_and_bucket` — controlled corpus where a doc is the semantic (not
  lexical) match ⇒ `bucket == "semantic"`; lexical match ⇒ `lexical`; neither ⇒
  `hard`.
- `passage_evidence_iou` — a chunk overlapping the evidence span yields the expected
  IoU; a disjoint chunk yields 0; whole_body (no span) yields `null`.
- `dense_section_scope_label` — a reduced run writes `scope: "slice@N"`, never
  `full`; carries `embedder_identity`.

**Implement.** Modify `chunk_words` (+ update the experiment call sites to ignore
offsets — no behaviour change there). Add the dense pass + sidecar merge (merge, do
not clobber the lexical section). Run once on a slice to populate a labeled dense
section; the full-corpus dense section follows from the handed-off full run.

**Done when:** the `dense` section reports buckets + span-IoU, honestly
scope-labeled, and the `semantic` bucket count gives the empirical answer to "does
this benchmark contain queries that *need* vector?"

## Sequencing, rollout, CI
1. WI-2 + WI-3a together → one gold regen, one `qrels_version` bump.
2. WI-1L (refactor seams first, then lexical harness) → cheap full-corpus lexical
   sidecar; wire into CI (fast, no embedder).
3. WI-1D + WI-3b → dense section; slice-scoped now, full-scoped after the
   handed-off full-corpus run.
4. Update `IR-C-api-surface-knobs-to-review.md` decision gate with the
   `bm25_rank1_frac` + bucket counts once WI-1 lands — those numbers tell us how
   much of "lexical-bound" is the test set.

## Risks / open questions
- **Refactor risk (ir_retrieval extraction):** the experiment test is the
  regression guard — extract with it green, no logic edits in the same step.
- **Dense-rank cost:** full-corpus dense ranks remain an embed-bound job; mitigated
  by scope-labeling and reusing experiment embeddings rather than a dedicated run.
- **Bucket cap `C` choice:** 50 (deepest K rung) by default; expose via env so the
  sidecar can be recomputed at other cutoffs without re-embedding (ranks are stored,
  bucketing is cheap re-derivation).
- **IDF source:** computed over the frozen corpus in the lexical pass; pin the
  tokenizer (reuse `content_tokens`) so `idf_overlap` is reproducible.

## Deferred — item 4 hook
The sidecar's per-query records (especially `bm25_gold_rank` + the dense ranks) are
exactly the pool the unlabeled-positive / pooling-augmentation audit (item 4) will
judge. Item 4 will add a `label_quality` section to the same sidecar
(judged-fraction, estimated false-negative rate) with the LLM-judge lexical-bias
debias caveat from `IR-C-fact-level-gold-labels-research.md:220`. Out of scope here.
