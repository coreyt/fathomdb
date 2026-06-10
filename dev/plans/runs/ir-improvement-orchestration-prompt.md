# Orchestration Prompt — Engineer FathomDB IR to Rival Same-Dataset SOTA

**Purpose:** Drive a coordinated review + experiment effort to raise FathomDB's
retrieval scores from *dense-baseline* territory to rival the **BM25-strong
same-dataset results** (EnronQA BM25 R@5 = 0.875; QAConv BM25 R@5/10 = 0.800/0.848)
documented in `dev/plans/runs/performance-output-and-compare.md`.

**Baseline to beat (current FathomDB, commit `fee78ea`):**
exact_fact (EnronQA+QAConv) **R@5 0.489 / R@10 0.557**; exploratory (QMSum)
**R@10 0.236**; overall **R@10 0.443**. Metric = single-evidence Recall@K
(strict==graded). Runner: `tests/ir_c_recall_run.rs` (gated `--features
default-embedder` + `IRC_RUN=1`).

> Use this as the prompt for an orchestrator that fans out to specialist agents,
> one per workstream below. Each workstream returns: (a) current-state finding
> with file:line evidence, (b) a concrete design, (c) a measurable experiment
> against the exact_fact / exploratory slices, (d) expected vs. measured delta.
> All claims must be grounded in code and re-measured with the IR-C runner — no
> speculative numbers.

---

## Shared context (ground truth, already mapped — verify, don't re-discover)

| Area | Current state | Key location |
|---|---|---|
| Chunking | **ABSENT** — whole body → one vector | `fathomdb-engine/src/lib.rs:4434` (`embed_with_watchdog(embedder, &job.body, …)`) |
| Dense index | sqlite-vec `vec0`, bge-small 384-d, bit-KNN 192 → f32 rerank 10 | `lib.rs:5561-5568`, `:3411`, `:3424` |
| BM25 / lexical | FTS5 `bm25()`; query = whitespace split, quoted, AND-joined (no stemming/expansion) | `schema/src/lib.rs:272`; `lib.rs:3963-3970`; `query/src/lib.rs:12-22` |
| RRF fusion | **K=60, equal arm weights, hardcoded** | `lib.rs:3600`, `fuse_rrf` `:3620-3659`, call `:4009-4012` |
| Re-ranking | **STUB** — `rerank_fused` identity passthrough | `lib.rs:3689-3696` |
| NER / augmentation | **ABSENT** | n/a |

---

## Workstream 1 — RRF weighting (FASTEST, do first)

**Hypothesis:** the equal-weight RRF dilutes BM25 (which these datasets reward)
with the weaker dense arm; that is why hybrid (0.557) barely beats vector-only
(0.533) on exact_fact while BM25-alone literature hits 0.80+.

**Tasks:**
1. Review `fuse_rrf` (`lib.rs:3620-3659`) and `RRF_K` (`:3600`). Confirm both
   arms contribute `1/(K+rank)` with no weight.
2. Add a **tunable per-arm weight** `w_bm25`, `w_vec` (fused score =
   `w_bm25·1/(K+rank_text) + w_vec·1/(K+rank_vec)`), plumbed via a test seam so
   the IR-C runner can sweep it without touching production defaults.
3. **Experiment** on the exact_fact slice (sample is fine; no re-embed needed if
   the index is reused — note the IR-C temp index is ephemeral, so plan a single
   re-seed and sweep in-process): BM25-only, BM25-heavy (e.g. 3:1, 5:1),
   equal, vector-heavy. Also sweep `RRF_K`.
4. Report exact_fact + exploratory R@5/10/20/50 per setting.

**Kill/confirm criterion:** if BM25-only or BM25-heavy lifts exact_fact R@10
toward 0.7–0.8, the fusion weighting was the bug → ship a tuned default.
If it doesn't move, the lexical arm or gold-doc mapping is the real limiter →
escalate to Workstream 4.

---

## Workstream 2 — Chunking strategy (BIGGEST lift on exploratory/long docs)

**Hypothesis:** embedding whole emails/meeting transcripts as one vector buries
the relevant span; chunking is what QAConv's own baseline does (≤512 tok).

**Review + design questions to answer with evidence:**
- **Length:** fixed token window (e.g. 256/512) vs sentence/paragraph vs
  semantic? Pick per `source_type` (short emails vs long QMSum meetings)?
- **Overlap:** none / fixed stride (e.g. 64–128 tok) / sentence-boundary?
- **Dynamic:** structure-aware splitting (email headers/quotes, meeting turns,
  paper sections) vs naive windows?
- **Index/identity impact:** chunks need parent-doc mapping so Evidence Recall@K
  (which scores at the **doc** level) still resolves a retrieved chunk → its
  `doc_id`. Define the chunk→doc rollup (max-score? any-hit?) and how it
  interacts with `build_body_to_doc_id_map` in the runner.
- **Cost:** chunking multiplies embed count — quantify vs the ~2 docs/sec seed.

**Deliverable:** a chunking design + a runner variant that seeds chunked docs and
rolls chunk hits up to doc_ids; measure exploratory (QMSum) R@10 vs the 0.236
baseline. Target: close toward the "workable" 0.4–0.5 band.

---

## Workstream 3 — Re-ranking engine

**Hypothesis:** recall keeps climbing to K=50 (exact_fact 0.620, overall 0.545),
so the relevant doc is usually in the candidate pool — it's a **ranking**, not a
findability, problem. The `rerank_fused` seam (`lib.rs:3689`) is the slot.

**Tasks:**
- Survey reranker options against the cost/latency budget: MMR (cheap,
  diversity), **cross-encoder reranker** (e.g. bge-reranker-base/-v2-m3 — same
  family as the embedder), late-interaction (ColBERT-style). Note ColBERTv2 is
  the strong dense baseline in EnronQA.
- Design the rerank stage to consume the fused top-N (deeper fanout, e.g. 50–100)
  and re-order to top-10.
- **Experiment:** with fanout 50, how much of the K=50 recall (0.620 exact_fact)
  can a reranker pull into K=10 (vs current 0.557)? That headroom is the prize.

**Constraint:** keep it an additive seam (the comment already says "lands
additively in a later slice") — do not regress the no-rerank path.

---

## Workstream 4 — Lexical quality + NER / augmentation

**Hypothesis:** EnronQA rewards proper-noun overlap; FathomDB's query compiler is
naive (whitespace split, AND-join, no stemming/expansion — `query/src/lib.rs:12-22`),
and there is no entity-aware indexing.

**Tasks:**
- Audit the FTS5 tokenizer/config (`schema/src/lib.rs:272`) and query compilation:
  is AND-joining every token too strict (one OOV token → zero BM25 hits)? Test
  OR / phrase / BM25-default tokenizer variants.
- Evaluate **NER / entity augmentation**: extract entities (people, projects —
  note the corpus already carries `people_mentions`, `project_mentions` fields!)
  and index/boost them. The metadata is *already in the raw docs* but currently
  used only as filter predicates, not retrieval signal.
- **Experiment:** BM25 query-compiler variants on exact_fact; entity-boost on/off.

---

## Cross-workstream rules

1. **One source of truth for numbers:** every result via
   `tests/ir_c_recall_run.rs` against the resolved gold; report R@5/10/20/50 by
   class (exact_fact, exploratory) + overall + negative FPR.
2. **No re-embedding waste:** the full corpus embed is ~2h. Prefer sampled
   slices (`IRC_SAMPLE`, `IRC_MAX_DOCS`) for iteration; reserve a full run for
   the final verified delta. Reuse a persisted index where possible.
3. **Isolate changes:** each lever behind a test seam / config so its delta is
   measured independently before stacking.
4. **Target:** rival same-dataset SOTA — exact_fact R@10 → **0.75+** (toward BM25's
   0.80–0.87), exploratory R@10 → **0.40+**. State expected vs measured for each.
5. **Honesty gate:** report regressions and null results plainly; a lever that
   doesn't move the number is a finding, not a failure.

**Sequencing:** WS1 (hours) → WS4 lexical audit (cheap, may stack with WS1) →
WS3 reranker (medium) → WS2 chunking (largest, most invasive). Re-baseline a full
IR-C run only after the cheap wins land.
