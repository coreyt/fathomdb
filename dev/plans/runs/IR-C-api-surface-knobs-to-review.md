# IR-C — API-surface knobs to review (post full-corpus chunking experiment)

Drafted: 2026-06-11
Status: **PARKED — review after the full-corpus chunking experiment lands**
Gating artifact: `dev/plans/runs/IR-C-ws1-fusion-experiment-full.json`
(produced by `IRC_FX_FULL=1 … --test ir_c_fusion_experiment`; the Option-A
deep-K exploratory run, ~3.5 h, in flight at draft time).

This is the consolidated list of retrieval API-surface / config knobs surfaced
during the IR-C discussion. **Do not act on B/C until the gating run lands** —
it measures how much daylight there is between the shipped `whole-doc + 3:1 +
k=30` default and the `chunked + 1:1` bundle at deep K (R@20/R@50) on the full
10,506-doc corpus. If the gap is small, most of B/C is not worth the surface.

Today's production stack (the defaults everything below is measured against):
content-OR text compile (`fathomdb-query::compile_text_query`), `bm25()` text
ordering, weighted RRF `RRF_K=30`, `RRF_WEIGHT_TEXT:RRF_WEIGHT_VECTOR = 3:1`
(`fathomdb-engine/src/lib.rs`), whole-doc embedding — one vector per node
(`lib.rs:4434`).

---

## A. Per-request (query-side) knobs — "tell us your intent"

| # | Knob | Controls | Default | Lean | Notes |
|---|------|----------|---------|------|-------|
| 1 | **Arm weights** `w_text:w_vector` | lexical vs. dense dominance (3:1 ↔ 1:1) | 3:1 | **Candidate** | Primary exact_fact↔exploratory lever. Caller usually knows intent ("find a fact" vs "sweep a topic"). |
| 2 | **RRF `k`** | rank-curve steepness (low = top-heavy, high = flat) | 30 | Candidate (lower priority) | Smaller effect than weights; expose only if weights are insufficient. NB: distinct from the `K` in R@K. |
| 3 | **Search depth / result `K`** | how many ranked results returned ("how deep the caller reads") | current limit | **Candidate** | This *is* shallow-vs-deep-K as an API param. Exploratory callers read deep; factoid callers read shallow. |
| 4 | **(meta) Caller-supplied intent** | whether 1–3 are caller-set vs. inferred | — | **For** (vs. a classifier) | Conclusion: in an agent-memory store the caller knows its intent; exposing 1–3 beats a query classifier — no added latency, no nondeterminism. |

## B. Ingest / data-side (config) knobs — the Option A family

| # | Knob | Controls | Default | Lean | Notes |
|---|------|----------|---------|------|-------|
| 5 | **Chunking on/off + geometry** (window size / stride) | passage fan-out (Option A): whole-doc vs 128/96 etc. | whole-doc | **Pending numbers** | The deep-K-exploratory lever. Cheap form is a **length-gated heuristic** ("chunk only if long"), NOT a classifier. |
| 6 | **Pooling strategy** (max / mean / top2) | how passage scores roll up to the node | max | Bake max, don't expose | Only relevant if #5 on; sweep already says max ≫ top2 ≫ mean. |
| 7 | **Data-type classifier at ingest** | ML routing of docs by type | — | **Against** (overkill) | A length/format heuristic (part of #5) gets ~all the benefit without a model dependency. |

## C. Response-surface additions — positional metadata

| # | Surface | Adds | Default | Lean | Notes |
|---|---------|------|---------|------|-------|
| 8 | **Passage locator on each hit** | matched passage's `(node_id, seq/offset)` — *which part* of the node matched | node-level only | **For**, if citations matter | Today only the **shared node_id** is kept (passages roll up to the node); **seq/offset is dropped**. Adding it enables snippets / highlights / **citation-grade answers** ("from minute 34 of the meeting"). |
| 8a | — store vs. derive | persist `seq` with each passage vector vs. recompute span at query time | — | persist if #5 lands | If Option A is built, storing `(node_id, seq, offset, vector)` is a small delta over the minimum `(node_id, vector)`. |

**Invariant (Option A):** the node stays the **identity/return unit**; positional
metadata is *additive* — a locator *inside* the returned node, not a new return
granularity. Its strongest justification is **independent of the recall
numbers**: grounding/citations are valuable in an agent-memory system even if
deep-K recall barely moves.

## D. Deliberately rejected (on record as considered-and-declined)

| # | Knob | Why rejected |
|---|------|--------------|
| 9 | **`fusion_mode` switch** (legacy-union vs RRF) | HITL Q3 — RRF is unconditional, no legacy path. Pinned by the determinism tests (`pr_g9_rrf_fusion.rs`). |
| 10 | **BGE query-instruction prefix** | Swept and definitively closed (tiny exact gain, hurts exploratory, every geometry — `performance-output-and-compare.md` 2026-06-10e). |
| 11 | **Query intent classifier** | Superseded by #4 (caller-supplied params) — avoids latency + nondeterminism. |

---

## Cross-cutting cost of any of A–C

Adding API surface is not free in this codebase:
- **Compatibility commitment** — each param is a long-lived contract.
- **Governed-surface enforcement** — must respect the facade-scope discipline
  (0.8.0 slice-27 / ADR-0.6.0-cli-scope); new query verbs/params are governed.
- **Determinism contract** — the RRF tests pin byte-identical ordering. Params
  are fine (deterministic given inputs); a *classifier* is not.

## Recommended starting position (revisit against the gating numbers)

- **Expose #1 and #3** (and maybe #2) as optional per-request params, with
  today's values as defaults.
- Treat **#5 + #8** as a single "build Option A with locators" decision,
  **gated on the deep-K numbers** from the full-corpus run.
- **Bake #6** (max-pool); **do not build #7 / #11**.

## Decision gate — what the full-corpus run must show to justify B/C

1. `v_128/96_max` vs `v_whole_max` — does passage fan-out beat the whole-doc
   dense arm at full scale (expected yes on the dense arm)?
2. `h_128/96_1:3` vs `h_whole_1:3` and `…_1:1` — does chunking lift **exploratory
   R@20 / R@50** materially over whole-doc at production k=30, and how much of
   that requires moving off 3:1 toward 1:1?
3. `exact_fact` must stay ~flat (lexical-bound) — confirm chunking costs nothing
   on the factoid class.

If (2) shows only a couple of points at deep K, the single `whole-doc + 3:1 +
k=30` default is "good enough" and B/C can be deferred; #8 (locators) may still
be worth it on the citation argument alone.

## Reduced-slice result (2026-06-11) — directional, NOT the full-corpus answer

`IR-C-ws1-fusion-experiment-slice1200.json` — 1,200 docs / 250 sampled
exploratory + 120 exact + 125 negative, production k=30. Ran in an ephemeral
container that could not finish the full job; the full-corpus run was handed off
to a non-volatile box. **Caveat:** this slice has ~9× fewer distractors than the
full corpus, so the lexical arm looks stronger here than it will at full scale —
read the hybrid conclusions as a *lower bound* on how much the dense arm matters.

Exploratory R@10 / R@20 / R@50 (the deep-K class):

| config | R@10 | R@20 | R@50 |
|---|---|---|---|
| text_only_ORc | 0.636 | 0.748 | 0.848 |
| v_whole_max (dense) | 0.228 | 0.336 | 0.532 |
| v_128/96_max (dense) | **0.328** | **0.420** | **0.652** |
| h_whole_1:3 (shipped) | 0.624 | 0.724 | 0.856 |
| h_128/96_1:3 | 0.636 | 0.732 | 0.852 |
| h_whole_1:1 | 0.504 | 0.672 | 0.832 |
| h_128/96_1:1 | 0.528 | 0.704 | **0.860** |

exact_fact stays ~0.92–0.95 R@10 across every hybrid (flat — gate #3 holds).

**Read against the gates:**
1. **Dense arm — chunking clearly wins** (gate #1 ✅): v_128/96 vs v_whole, explor
   R@10 0.228→0.328, R@50 0.532→0.652; exact R@10 0.758→0.842.
2. **Hybrid deep-K lift — did NOT reproduce at this scale** (gate #2 ✗ here):
   the content-OR/BM25 **text arm alone (0.636/0.748/0.848) is essentially the
   exploratory ceiling**. At the shipped 3:1 the chunked hybrid is geometry-
   invariant (h_128/96_1:3 ≈ h_whole_1:3 ≈ text_only). At 1:1 the dense arm
   *hurts* shallow K (explor R@10 0.636→0.528) and buys only ~+0.03 at R@20/R@50.
   The small-corpus "1:1 deep-K win" (R@20 0.725→0.850) **did not hold** — that
   looks like a low-distractor artifact.

**Provisional lean (pending full corpus):** at modest corpus size the hybrid is
**lexical-bound** — Option A is a real dense-arm win that the fusion does not cash
in, so **defer B (chunking) and prefer the cheap `whole-doc + 3:1 + k=30`
default**. #8 (positional/citation locators) survives on its own merit. **But**
the full-corpus run is the real arbiter precisely because more distractors are
where the dense arm was expected to earn its keep (the production full-corpus
exploratory R@10 was only 0.236 — far below this slice's 0.636). Do not finalize
B/C until that lands.

## Complementarity diagnostic (2026-06-11) — is the dense arm redundant?

`-slice1200` run, exploratory n=250. **Oracle-union recall** (gold found if EITHER
arm has it in top-K) upper-bounds any fusion; `rescue` = queries with a gold doc
the 128/96 arm surfaces that text misses.

| R@K | text | dense_whole | dense_128/96 | union_whole | union_128/96 | rescue_128/96 |
|---|---|---|---|---|---|---|
| 10 | 0.644 | 0.228 | 0.328 | 0.668 | **0.688** | 11 (4%) |
| 20 | 0.752 | 0.336 | 0.420 | 0.784 | **0.796** | 11 (4%) |
| 50 | 0.844 | 0.532 | 0.652 | 0.876 | **0.892** | 12 (5%) |

**Read:**
1. **Not redundant — but barely complementary.** `union_128/96 − text` = **+0.044 /
   +0.044 / +0.048** (R@10/20/50): the chunked dense arm rescues gold docs text
   misses on only **~4–5% of queries** (11–12 of 250). So "redundant" was too
   strong; the honest verdict is *mostly redundant, a thin complementary stratum*.
2. **Chunking's incremental complementary value is ~1–2 pts** (`union_128/96 −
   union_whole` = +0.020 / +0.012 / +0.016) — most of chunking's win is to the
   dense arm's *solo* recall (≈ doubles R@10), not to the fused ceiling.
3. **The fusion captures none of even this small headroom.** The best shipped-style
   hybrid (`h_128/96_1:3`, R@20 ≈ 0.732) sits *below* text-only (0.748) and far
   below the oracle union (0.796): the dense arm's noise currently *displaces* more
   good text hits than its 4–5% rescues add. So a **re-weighting** (cheaper than
   building chunking) is the prerequisite, and even a perfect fusion caps at ~+4–5
   pts here.

**Updated lean:** the realizable hybrid gain from Option A is **small (≤~5 pts,
oracle) and currently negative**, so the cost-justified order is **(a) fix the
fusion weighting first**, not build chunking; **(b)** re-test complementarity **at
full corpus**, where the thin stratum may widen (text degrades from 0.644→~0.236
R@10 with 9× the distractors). The WI-1 diagnostics sidecar will size the
lexical/semantic bucket split that this oracle-union summarizes per-query.

## Full-corpus LEXICAL diagnostics (2026-06-11) — the slice bias confirmed

`all.gold.diagnostics.json` lexical tier, whole frozen corpus (10,506 docs),
4,472 positive queries. `bm25_gold_rank` = rank of the gold doc under content-OR
+ `bm25()` over the *entire* corpus.

| class | n | bm25_rank1_frac | median bm25 rank | found@1000 | mean idf_overlap |
|---|---|---|---|---|---|
| exact_fact | 2888 | **0.738** | **1** | 0.985 | 0.743 |
| exploratory | 1584 | **0.102** | **26** | 0.859 | 0.704 |

**This is the key correction to the slice story:**
- **exact_fact is genuinely lexical** — 74% of gold docs at BM25 rank 1, median
  rank 1. A vector arm cannot add much here by construction; "exact_fact is
  lexical-bound" is real, not an artifact.
- **exploratory is NOT lexical-bound at full corpus.** Only **10%** at rank 1,
  **median rank 26**, 14% not found in the top 1,000. The slice's strong text
  exploratory recall (0.64 R@10) was a **low-distractor artifact** — at full scale
  BM25 buries the relevant transcript at rank ~26.
- **And it's a discrimination problem, not a vocabulary problem:** mean
  `idf_overlap ≈ 0.70` means the query's content terms *are* in the gold doc —
  BM25 just can't separate it from the many other long transcripts that mention
  the same terms. **That median-26 band is precisely the semantic-rerank
  opportunity** a dense arm exists to exploit.

**So the provisional "defer Option A" lean is now in question for exploratory.**
The open question is whether the dense arm actually *pulls* those rank-26
exploratory golds into the top-K (the `semantic` bucket) or not — that is the
dense-tier (WI-1D) measure, which needs the full-corpus embed. The fusion's
failure to help was measured on the *easy* slice; it must be re-judged here.
