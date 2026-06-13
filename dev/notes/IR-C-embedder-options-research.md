# IR-C — Embedder limits & options research

Status: **research, cited** · 2026-06-11 · Branch `claude/recent-changes-state-a6wth3`
Motivation: `dev/plans/runs/IR-C-retrieval-findings.md` (chunked dense arm weak on
exploratory — median gold rank 99 over 10,506 docs). Question: are we at
bge-small's limit, or misusing it, and what are the options?

> ## ⚠️ UPDATE (2026-06-11) — the pooling hypothesis was REFUTED by experiment
> The TL;DR below hypothesized the median-99 was a **mean-pooling bug**. We then
> ran the actual A/B (full corpus, dense diagnostic, CLS pooling + query prefix vs
> mean/no-prefix). **CLS did NOT fix exploratory — it was marginally *worse*:**
> exploratory dense median rank **99 → 121**, top-50 **37% → 34%**, semantic bucket
> **142 → 112**; exact_fact nudged up (top-50 78% → 80%). CLS is correctly
> implemented (it cleared the 0.944 binary floor and helped exact_fact), so the
> conclusion is: **the median-99 is NOT a pooling bug.** And because the dense
> diagnostic embeds ~128-word chunks (~170 tokens), **512-truncation isn't in play
> either** — this is **bge-small's genuine semantic-retrieval ceiling** on
> discourse/summary queries over transcript passages. **The real lever is a stronger
> retrieval model (Phase 2), not a pooling/usage fix.** Pooling choice is ~neutral
> (±2-3 pts; tiny exact_fact gain, tiny exploratory loss) and not worth a migration
> on its own. See the "A/B RESULT" section at the end. The original (now-corrected)
> hypothesis is preserved below for the record.

## TL;DR (ORIGINAL HYPOTHESIS — superseded by the UPDATE above)

**We are almost certainly *misusing* bge-small, not hitting its ceiling.** The
headline cause is a pooling bug, and it's cheap to fix and re-measure before any
model swap.

1. **bge-small-en-v1.5 is CLS-pooled; FathomDB mean-pools it.** Verified against the
   model's own `1_Pooling/config.json` (`pooling_mode_cls_token: true`,
   `pooling_mode_mean_tokens: false`,
   https://huggingface.co/BAAI/bge-small-en-v1.5). FathomDB mean-pools
   (`src/rust/crates/fathomdb-embedder/src/candle_bge.rs:178-185`;
   `dev/design/embedder.md §0.4`). BGE's own docs warn mean-pooling causes "a
   **significant decrease** in performance"
   (https://bge-model.com/tutorial/1_Embedding/1.2.3.html). A correctly-used
   bge-small puts the relevant doc in the top handful, not median rank ~99 — so the
   IR-C result is consistent with a near-broken embedding space from wrong pooling,
   not the model's documented ceiling (~51-54 MTEB Retrieval).
2. **Compounded by 512-token truncation** of long transcripts (production doesn't
   chunk; everything past ~400 words is discarded) and a **missing query prefix**
   (minor for v1.5 — "slight degradation", https://bge-model.com/bge/bge_v1_v1.5.html).
3. **Existing gates can't catch it:** the 0.90 recall floor is *ANN fidelity*
   (binary+ANN vs the *same model's* f32 top-10), self-consistent regardless of
   whether pooling is correct. The pooling bug has therefore been latent.
4. **My IR-C diagnostics inherited the same mean-pooling embedder**, so the
   median-99 dense number *understates* bge-small's true capability.

**Do this first (free, no migration):** switch to CLS pooling, chunk long docs to
≤512 tokens, add the query prefix, then re-run the IR-C dense diagnostic.

**Caveat that gates the fix:** pooling changes the embedding-space geometry that
1-bit binary quantization is sensitive to (see §3). EU-0 selected mean-pooling and
it cleared the binary recall floor; CLS pooling must be **A/B'd on *both* axes —
IR relevance *and* binary-quant retention / the 0.90 ANN floor** — not assumed free.

> **GATE RESULT (2026-06-11) — CLS clears the 1-bit binary floor.**
> `tests/ir_c_pooling_floor_gate.rs`, full corpus (10,506 docs), 200 gold queries,
> faithful vector stage (mean-center → sign-bit → Hamming K=192 → f32 rerank vs
> exact-f32 top-10): **mean recall@10 = 0.946, CLS recall@10 = 0.944** — both PASS
> the 0.90 floor, statistically tied. So the geometry-change risk did **not**
> materialize; CLS is binary-quant-safe and adoptable. (Harness mean-pool 0.946 is
> consistent with the canonical eu7 baseline 0.896–0.937, cross-checking fidelity.)
> Artifact: `dev/plans/runs/IR-C-pooling-floor-gate.json`. **Still pending: the
> *relevance* axis** — re-run the IR-C dense diagnostic under `Pooling::Cls` to
> confirm CLS fixes the median-99 exploratory result (the payoff measurement).

## Constraint gate (any replacement must pass, in priority order)

From `dev/adr/ADR-0.6.0-default-embedder.md`, `dev/design/embedder.md`, and the
EU-0 selection (`dev/notes/0.7.1-default-embedder-research.md`):

1. **candle-runnable** (Rust, in-process, CPU) — architecture must have a
   candle-transformers module, else custom Rust port.
2. **survives 1-bit sign-bit binary quantization** (Hamming candidate + f32 rerank,
   K=192) — this is what *killed* e5-small in EU-0.
3. **permissive license** (MIT / Apache-2.0 / BSD).
4. **lightweight**, **ideally 384-dim** (drop-in; else schema migration + K-ladder
   re-derivation + full re-embed) — `ADR-0.6.0-vector-identity-embedder-owned.md`.
5. **>512-token context** is the lever most directly targeting the long-transcript
   truncation finding — *if* candle-compatible.

## §1 — Is bge-small at its limit? No.

- bge-small-en-v1.5: 33M params, 384-d, **512-token** max, CLS-pooled, MIT; MTEB
  Retrieval ~51.7 (v1, https://huggingface.co/BAAI/bge-small-en-v1.5). Sept 2023.
- Short-context (512-tok) dense retrievers lose the most on **long documents**;
  truncation degradation scales with the fraction of relevant content past the
  cutoff (LongEmbed, https://arxiv.org/pdf/2404.12096; Late Chunking,
  https://arxiv.org/html/2409.04701). BGE's own guidance: for short-query→long-doc,
  add the query instruction; for long inputs use **BGE-M3** (8192 tok), not
  bge-small (https://huggingface.co/BAAI/bge-base-en-v1.5).
- **Verdict:** median-rank-99 on long-transcript summary queries is far below
  bge-small's documented ceiling and points to **mean-pool mismatch + truncation**,
  not the model. (No bge-small-specific "summary-query" ablation was found; the
  long-doc degradation evidence is for the short-context class generally.)

## §2 — Candle compatibility (the binding filter) — verified from candle source

`candle-transformers/src/models/mod.rs` (main, June 2026) has dedicated modules for:
`bert`, `modernbert`, `xlm_roberta`, `jina_bert`, `nomic_bert`, `distilbert`,
`stella_en_v5`, `nvembed_v2`
(https://github.com/huggingface/candle/blob/main/candle-transformers/src/models/mod.rs).
Runnable embedding **examples** exist for bert, modernbert, nomic-bert, jina-bert,
stella, gte-qwen, nvembed.

| Architecture | candle-native? | Affected models |
|---|---|---|
| BERT | ✅ (canonical) | bge-*, gte-small, arctic-embed-s/m/m-v1.5, e5-*, mxbai |
| ModernBERT | ✅ module+example | modernbert-embed-base, **gte-modernbert-base** |
| nomic-bert (RoPE/SwiGLU) | ✅ module+example (targets nomic-embed-v1.5) | nomic-embed-text-v1.5 |
| JinaBERT (ALiBi) | ✅ module+example | jina-embeddings-v2-small/base |
| XLM-RoBERTa | ✅ module (no example) | multilingual-e5, arctic-embed-l-v2.0 (long-ctx XLM-R — verify) |
| **GTE "transformer++" encoder** (RoPE+GLU) | ❌ **no module** | **gte-base-en-v1.5, arctic-embed-m-v2.0** → custom code needed |

So **gte-base-en-v1.5** and **arctic-embed-m-v2.0** are effectively out (no candle
GTE encoder); **gte-modernbert-base** is in (it's ModernBERT, not the gte encoder).

## §3 — Binary-quantization robustness (corroborates the e5 rejection)

Per HF "Binary and Scalar Embedding Quantization" (Mar 2024,
https://huggingface.co/blog/embedding-quantization), % of fp32 retrieval retained
under **1-bit binary**:

| model | binary retention |
|---|---|
| mxbai-embed-large-v1 | 96.45% |
| Cohere embed-v3 | 94.6% |
| all-MiniLM-L6-v2 | 93.79% |
| nomic-embed-text-v1.5 | 87.7% |
| **e5-base-v2** | **74.77%** (collapses) |

- **Why e5 collapses:** *dimension collapse* — e5 uses only a subspace of the latent
  space, which collapses further under quantization (HF blog; arXiv:2110.09348).
  This **corroborates EU-0's finding** that e5-small fell to ~0.45 while bge held
  (the exact 0.45/0.79 figures are FathomDB-internal, unverified externally, but
  directionally consistent).
- **MRL front-loads information into early dims** → binary-robust, and MRL is
  *orthogonal* to quantization (truncate then binarize, ~2% loss). MRL/quant-friendly
  families: nomic-embed-v1.5, Snowflake arctic-embed-m-v1.5/v2.0, mxbai
  (https://huggingface.co/blog/matryoshka;
  https://www.snowflake.com/en/blog/engineering/arctic-embed-m-v1-5-enterprise-retrieval/).
- **Implication:** any replacement should ideally be **MRL-trained** to be safe under
  FathomDB's 1-bit quantization. bge-small is *not* MRL but empirically survived; a
  CLS-pooling change could alter that — re-validate.

## §4 — Candidate models (filtered to candle-runnable + permissive)

Sizes are fp32 (params×4B); safetensors/fp16 ≈ half. MTEB = English Retrieval where
available. All rows are candle-runnable and MIT/Apache unless noted.

| model | params | dim (MRL) | ctx | arch / candle | license | MTEB Retr | binary-safe? |
|---|---|---|---|---|---|---|---|
| **bge-small-en-v1.5** (current) | 33M | 384 | 512 | BERT ✅ | MIT | ~51.7 | empirically yes |
| arctic-embed-s | 33M | **384** | 512 | BERT ✅ | Apache | **51.98** | unverified (e5-derived) |
| gte-small | 33M | **384** | 512 | BERT ✅ | MIT | 49.46 | unverified |
| **nomic-embed-text-v1.5** | 137M | 768 → **256/512** MRL | **8192** | nomic-bert ✅ | Apache | 62.28@768 / 61.04@256 | **87.7%** |
| modernbert-embed-base | 149M | 768 → **256** MRL | **8192** | ModernBERT ✅ | Apache | ~53 (unconf.) | MRL ⇒ likely |
| gte-modernbert-base | 149M | 768 | **8192** | ModernBERT ✅ | Apache | **55.33** | no MRL ⇒ unverified |
| jina-embeddings-v2-small-en | 33M | 512 | **8192** (ALiBi) | jina_bert ✅ | Apache | unconf. | unverified |
| mxbai-embed-large-v1 | 335M | 1024 → MRL | 512 | BERT-large ✅ | Apache | 54.39 | **96.45%** (best) |
| bge-base-en-v1.5 | 109M | 768 | 512 | BERT ✅ | MIT | 53.25 | likely (bge family) |

Excluded: **gte-base-en-v1.5**, **arctic-embed-m-v2.0** (no candle GTE encoder);
**jina-embeddings-v3** (CC-BY-NC, non-commercial — license-blocked); static
model2vec/potion (MIT, ~30 MB, but retrieval only ~82% of all-MiniLM and you'd hand-
write the EmbeddingBag forward — a fast-first-stage option, not a quality upgrade).

Note: drop-in 384-d swaps (arctic-embed-s, gte-small) are **marginal** over a
*correctly-pooled* bge-small and are not MRL (binary risk). The real upgrades are the
**MRL + long-context** models, which require a dimension/schema migration.

## §5 — On long context vs chunking

Long-context models remove the 512-truncation penalty, **but a single mean-pooled
vector over 8192 tokens dilutes the "needle"** and underperforms chunking (LongEmbed,
https://arxiv.org/pdf/2404.12096; Late Chunking, https://arxiv.org/html/2409.04701).
The win is using a long-context model to produce **context-aware chunk** embeddings
(late chunking), not one giant vector. FathomDB's experiment already chunks; the
upgrade path is *long-context model + chunking*, not *long-context model alone*.

## §6 — Recommendation (phased)

**Phase 0 — fix usage, then re-measure (free; do before anything else).**
- CLS pooling (`output[0][:,0]`) instead of mean-pool in `candle_bge.rs`; add the
  query instruction prefix for queries; chunk long docs to ≤512 tokens in production
  (the experiment already does). Re-run `ir_c_gold_diagnostics` (dense) — the harness
  takes any `Embedder`, so this is a one-file change + a re-run.
- **Gate:** confirm the gain on IR relevance **and** re-validate the 0.90 ANN /
  binary-retention floor (CLS pooling may change quantization behavior). If CLS hurts
  binary retention, keep mean-pool or adopt an MRL model (Phase 2).
- *Expectation:* this likely recovers most of the exploratory gap at zero migration
  cost; the median-99 is far more consistent with a pooling bug than bge's ceiling.

**Phase 1 — if a same-dimension model swap is still wanted (low cost: 384-d, no
schema change, K-ladder re-derive + re-embed):** `arctic-embed-s` (Apache, 384,
candle BERT, retrieval ≥ bge-small). Verify its pooling and **binary retention**
first (it's e5-derived; e5 collapsed under binary). Marginal benefit — only worth it
if Phase 0 confirms the model (not usage) is the limit.

**Phase 2 — the real upgrade for long-doc/exploratory (requires schema migration:
new dimension, mean-vec recompute, K-ladder re-derivation, full re-embed):**
**nomic-embed-text-v1.5** is the standout — Apache-2.0, **candle-native** (dedicated
nomic-bert example), **8192 context**, **MRL** (binary-safe; truncate to 256-d, which
is *smaller* than today's 384-d while staying quantization-robust), MTEB 62.28.
Alternative: **modernbert-embed-base** / **gte-modernbert-base** (ModernBERT, 8192,
candle-native; gte-modernbert has the higher retrieval 55.33 but no MRL).
**mxbai-embed-large-v1** has the best binary retention (96%) but is 335M / 512-ctx —
heavier and no long-context benefit.

**Do NOT** pursue gte-base-en-v1.5 / arctic-embed-m-v2.0 (not candle-native) or
jina-v3 (non-commercial license).

## Confidence & flags
- **High confidence:** the pooling mismatch (verified against config.json + source);
  candle module support (read from candle source); e5 binary fragility (HF blog).
- **Medium / to-verify:** exact MTEB sub-scores for modernbert-embed-base,
  jina-v2-small (cards truncated on fetch); arctic-embed-s binary retention (not
  published — must measure); whether CLS pooling helps FathomDB's *quantized*
  pipeline (must A/B). candle `mod.rs` reflects `main` at fetch — pin & re-verify.
- The single most valuable next action is **empirical**: A/B CLS vs mean pooling on
  the IR-C dense diagnostic (harness ready), measuring relevance *and* binary floor.

## A/B RESULT (2026-06-11) — CLS pooling does not fix exploratory

Ran the full-corpus dense diagnostic under `IRC_DIAG_POOLING=cls IRC_DIAG_PREFIX=1`
(CLS pooling + BGE query instruction) vs the mean/no-prefix baseline. Same corpus,
same 128/96 geometry, bge-small.

| class | metric | mean (baseline) | CLS + prefix | Δ |
|---|---|---|---|---|
| exploratory | dense median rank | 99 | **121** | worse |
| exploratory | top-10 / top-50 | 16% / 37% | 15% / **34%** | worse |
| exploratory | buckets L/S/H | 846 / 142 / 596 | 846 / **112 / 626** | more hard |
| exact_fact | dense median rank | 2 | 2 | = |
| exact_fact | top-10 / top-50 | 69% / 78% | **70% / 80%** | slightly better |

**Conclusions:**
1. **Pooling hypothesis refuted.** CLS (the model-native mode, + prefix) does not
   recover exploratory; it is marginally worse there and marginally better on
   exact_fact — a wash. The BGE docs' "significant decrease from mean-pooling" did
   **not** manifest as a significant *retrieval* difference on this corpus/task.
2. **Not truncation either.** The dense diagnostic embeds ~128-word chunks, well
   under 512 tokens, so the 512-truncation penalty is not the cause here. (It would
   still hurt the *production* whole-doc path, which is a separate reason to chunk.)
3. **It's the model.** median-rank-99 is bge-small's real semantic-retrieval ceiling
   on discourse/summary queries over transcript passages. The lever is **Phase 2 —
   a stronger retrieval model** (candle-native, binary-safe, ideally long-context +
   MRL): `nomic-embed-text-v1.5` is the standout to test next on this exact harness
   (swap the `Embedder`, re-run). exact_fact is a solved lexical task; leave it.
4. **CLS adoption:** optional and minor. It clears the binary floor (0.944) and is
   the model-native mode with a small exact_fact gain, but it's not the fix; default
   stays Mean. Artifacts: `all.gold.diagnostics.{mean,}.json` (gitignored).

**Methodological note:** this is the value of the empirical gate — a well-motivated,
documented hypothesis (mean-pooling a CLS model) was *plausible* (config + BGE docs
confirmed the mismatch) but turned out not to explain the symptom. Cheap to test,
and it redirected effort from a usage fix to the actual lever (the model).

## NOMIC A/B RESULT (2026-06-11) — a stronger model does NOT fix exploratory

Ran the full-corpus dense diagnostic with `nomic-embed-text-v1.5` (768-d, candle
`nomic_bert`, `search_document:`/`search_query:` prefixes) vs the bge-mean baseline,
same corpus + same 128/96 geometry.

| class | metric | bge mean | nomic |
|---|---|---|---|
| exact_fact | median / top-50 | 2 / 78% | **1 / 84%** (+6) |
| exploratory | median rank | 99 | **135** (worse) |
| exploratory | top-50 | 37% | **32%** (worse) |
| exploratory | semantic / hard | 142 / 596 | **76 / 662** (worse) |

Speed (measured, same chunks): nomic ~310 ms/passage @ ~245% CPU vs bge ~225 ms @
~150% — **~1.4× wall-clock, ~2.2× CPU**, ~4× weights on disk (522 MB vs 133 MB).

**Conclusions:**
1. **Stronger ≠ better here.** Nomic (MTEB retrieval 62.28 vs bge 51.68) is clearly
   better on **exact_fact** (median rank 1, +6 pts top-50) but **worse on
   exploratory** (median 99→135). So the exploratory weakness is **not** a
   model-capacity problem a bigger model fixes.
2. **The exact_fact win is moot for the hybrid:** exact_fact is already lexically
   solved (BM25 median rank 1); a better vector arm there adds nothing the fusion
   needs.
3. **Most likely it's a GRANULARITY / task-structure problem, not a model one.** The
   diagnostic embeds ~128-word chunks, which (a) **neutralizes nomic's headline
   advantage** — its 8192-token long context is wasted on 128-word windows — and
   (b) is hostile to discourse/summary queries, whose answer spans the *whole
   discussion*, not a single chunk. Max-pool over short chunks can't represent
   "what did they decide about X" over a meeting transcript. Both models hit this
   wall; the lexical (BM25) arm remains the best exploratory component (median rank
   26 vs dense 99–135).
4. **Decision:** nomic is **not** worth ~2× compute / ~4× model size — it doesn't
   move the class we care about (exploratory) and only helps an already-solved one.
   Stay on bge-small (Mean pooling); for exploratory, lean on the lexical arm.

**The one untested lever** (if dense exploratory is pursued): **WHOLE-document
long-context embedding** — the config where nomic's 8192-ctx actually applies and
bge's 512-truncation actually hurts. The chunked diagnostic deliberately never
tested it (skip-whole). It's the only remaining dense angle with a real mechanism
to help discourse retrieval, but it's expensive (embedding full transcripts) and
unproven; possibly via *late chunking* (long-context encode → pool to chunks).

**Three negatives, one shape:** Option A (chunking), pooling, and a stronger model
all failed to lift exploratory. They converge on the same conclusion — chunk-based
single-vector dense retrieval is structurally weak for discourse/summary queries
over long transcripts, and BM25 is the better tool there. The remaining dense idea
is long-context whole-doc (untested), not "a better model."
