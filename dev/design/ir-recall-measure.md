# IR / agentic evidence-recall MEASURE (definition + methodology) · `[design / consensus]`

> **Status:** Phase-1 DEFINITION, Claude↔codex consensus-signed (see § Consensus).
> **Scope:** This document *defines the measure and its methodology only.* It mints no AC,
> builds no gold set, runs no experiment, picks no threshold number, and **does not touch the
> eu7 / AC-075 fidelity gate.** Every number here is **TBD** and is settled downstream by the
> Phase-4 experiments + the IR-2 / HITL gate. It does **not** commit to any corpus snapshot —
> the corpus basis is ruled separately by B-1 + the corpus freeze.
>
> **This is the input to IR-1 Phases 2–4 (AC-077 mint, gold set, experiments) + IR-2.**
> **Initiative:** IR-eval. **Prompt:** `dev/plans/prompts/0.8.x-IR-1-phase1-measure-consensus.md`.
> **Charter:** `dev/notes/recall-eval-framework-assessment-20260607T174821Z.md`.

## 0. Why this measure exists (the one-paragraph frame)

FathomDB has exactly **one** gated recall number today — eu7 / AC-075 — and it measures
**ANN/quantization FIDELITY**: does the cheap 1-bit sign-quant index (K=192 Hamming + f32
rerank) reproduce the *exact-f32 top-10 of the same embedder*
(`dev/adr/ADR-0.7.0-vector-binary-quant.md:151-158`; `eu7_real_corpus_ac.rs`). That is a
**system-health** property — necessary but **not sufficient** for "is the agent's memory
useful." A store can sit at 0.99 fidelity and still bury the one fact the agent needed.
The **product-value** axis — *when the agent needs a memory to act, is the required evidence
in the retrieved context?* — is measured today only by eu8 (`eu8_ir_validation.rs`), which is
**report-only** and capped at the embedder's IR ceiling (≈0.571,
`ADR-0.7.0-vector-binary-quant.md:169-179`). This document defines the **product-value
recall measure** so it can later become a tracked signal and (post-experiment, HITL-ruled)
a gate — **complementary to, never a replacement for, eu7.**

---

## (a) What is measured — the primary measure + its relationships

**Primary measure: Evidence Recall@K.**

> **Evidence Recall@K** = the fraction of evaluation queries for which the retrieved
> top-K context **contains *all* the evidence units required to correctly act on / answer
> the query.** It is computed against a **gold set** of queries each labelled with its
> *required-evidence set* (the unit of relevance is defined in (b)).

Two reporting forms, both required (they answer different questions):

- **Strict (all-of) Evidence Recall@K** — per query, `1` iff **every** required-evidence unit
  is present in top-K, else `0`; then averaged. This is the headline product-value number
  ("could the agent have acted?"). It is *all-or-nothing per query* because a commitment with
  the date but not the obligor is not actionable.
- **Graded (fraction-of) evidence recall@K** — per query, **over the `required` evidence set
  only**: `|required ∩ retrieved@K| / |required|`; averaged. This is the **diagnostic** companion
  (how close did we get when we missed?), and it degrades gracefully so it is useful below the
  all-of threshold. *Both* strict and graded use the **same `required`-only denominator** — they
  differ only in all-or-nothing vs. fractional — so they are directly comparable. `supporting`
  evidence is **not** in either recall number (see (b)); it is reported as a separate
  supporting-coverage diagnostic.

**Relationship to plain retrieval Recall@K (eu8 today).** Plain Recall@K is *doc-id* recall —
"did a labelled-relevant *document* appear?" (`eu8_ir_validation.rs:227-253`, binary relevance
over `expected_doc_ids`). Evidence Recall is *evidence-unit* recall — "did the labelled-required
*fact* appear?" Plain Recall@K is the **layer-1 retriever-health** signal and is **subsumed**:
when the unit of relevance is the whole document (the degenerate label, see (b)), Evidence
Recall reduces exactly to eu8's doc-id Recall@K. So eu8 is **not discarded** — it is the
document-granularity special case and the migration path.

**Relationship to MRR / nDCG.** MRR and nDCG are **ranking-quality** signals (where in the
list did relevant items land), already computed by eu8 (`eu8_ir_validation.rs:244-278`). They
are reported alongside Evidence Recall as **secondary** signals, never as the headline: a high
MRR with incomplete evidence is still an un-actionable result. nDCG's graded value needs graded
qrels, which the current chain labels do not carry — flagged, report-only until graded labels
exist.

**Relationship to the eu7 fidelity gate (explicit, load-bearing).** Evidence Recall and eu7
are on **different axes** and measure **different ground truths**:

| | eu7 / AC-075 (the GA gate) | Evidence Recall@K (this measure) |
|---|---|---|
| Ground truth | exact-f32 top-10 of the **same** embedder | **externally-labelled required-evidence** sets |
| Question | does quant reproduce exact-f32? | is the evidence needed to act present? |
| Axis | **ANN / quantization fidelity** | **IR / agentic relevance (product value)** |
| Role | system-health (necessary) | product-value (sufficiency of context) |
| Ceiling | ~1.0 achievable (index faithfulness) | embedder/graph-bound (~0.571-class for vector-only) |

**They are complementary and orthogonal. This measure does NOT replace, relax, alter, or
re-anchor eu7 / AC-075. The 0.90 fidelity floor stays exactly as it is.** (§ (g) restates why.)

---

## (b) Unit of relevance + the gold-set encoding protocol

**Unit of relevance = the atomic evidence unit (`evidence_id`).** Because FathomDB stores
**whole node bodies** and **does not chunk** (`fathomdb-engine/src/lib.rs:1010-1019,922-928`;
charter §1), the "chunk recall" axis collapses — *document recall and chunk recall are the
same thing here.* The genuinely missing axis is **fact-level / evidence-level** recall: a long
`enron` / `qmsum` / `cnn_dailymail` body can rank highly as a *document* while the *specific
fact* needed to act is buried. The unit of relevance is therefore the **atomic evidence unit**,
not the document.

**The gold-set encoding protocol (schema, NOT the labels — labeling is Phase 3).** The schema
is **additive over the existing chain `ground_truth_queries` shape** (`IRQuery`:
`support/corpus_subset.rs:214-220`; chains carry `expected_top_k_doc_ids` + `relation_type` +
`chain_shape` today), so eu8 stays the document-granularity special case and the gold set is a
strict superset:

```jsonc
// per gold query (additive superset of today's ground_truth_queries entry)
{
  "query": "...",                        // SAME key the eu8 parser reads (corpus_subset.rs:239 q.get("query")) — additive superset, stays parseable
  "query_id": "stable-unique-id",        // additive (new; optional for existing eu8 entries)
  "query_class": "commitment | action | exact_fact | preference | exploratory | negative",
  "required_evidence": [                 // the "all evidence required to act" set
    {
      "evidence_id": "stable-unique-id",
      "doc_id": "<the body that carries this fact>",   // which node body must be retrieved
      "necessity": "required | supporting",            // required = must be in top-K for strict recall
      "locator": { "kind": "span|whole_body", "...": "OPTIONAL provenance, not used for scoring in a non-chunking store" }
    }
  ],
  // back-compat / migration: a query whose required_evidence is exactly its
  // distinct doc_ids with necessity=required degenerates to today's eu8 doc-id qrels.
  "expected_top_k_doc_ids": ["..."],     // PRESERVED: the eu8 doc-id view
  "relation_type": "action_from | contradicts | follows_up_on | mentions | summarizes | ...",
  "chain_shape": "..."
}
```

Scoring contract: an `evidence_id` counts as **retrieved@K** iff its `doc_id` appears in the
engine's top-K result bodies (body→doc_id mapping is in-harness today,
`eu8_ir_validation.rs:189-216`). Because FathomDB returns whole bodies, **presence is at
doc-body granularity**; the `locator` is recorded for label provenance/audit but is **not**
load-bearing for the score in a non-chunking store (it becomes load-bearing only if/when
FathomDB chunks — not planned for 0.8.0/0.8.1). **Only `necessity=required` units are in the
recall denominator** — they gate the strict all-of recall *and* form the `required`-only
denominator of the graded recall (a). **`supporting` units are NOT in either recall number**;
they are reported as a separate **supporting-coverage** diagnostic (how much corroborating
context was also retrieved). This keeps strict and graded recall on one consistent denominator
and removes any ambiguity about whether supporting evidence inflates the headline. **The label
*values* (which facts are required for which query) are produced in Phase 3 by human/HITL
labeling — this document defines only the slot they go in.**

---

## (c) The K-ladder + the headline K

Report at **K ∈ {5, 10, 20, 50}** (extensible to 200 as a pure diagnostic). Rationale,
anchored to FathomDB's real surface:

| K | Role | Anchor |
|---|------|--------|
| **@5** | UX-proximal (what the agent reads first) | — |
| **@10** | **HEADLINE** — the eval/reporting convention; aligns with the vector-branch phase-2 rerank depth `SEARCH_RERANK_LIMIT`=10 and with eu7/eu8's K=10 | `lib.rs:3388` (`SEARCH_RERANK_LIMIT`); eu7 `:444-445`; eu8 `K=10` (`:70`) |
| **@20** | near-UX / reranker-target band (when a real reranker lands) | charter §3 item 1 |
| **@50** | **retriever-health** (is the evidence anywhere in the candidate pool?) | charter §5 #3 |
| (@200) | deep diagnostic only — not a UX surface | charter §3 (chunk/fact note) |

**Headline = Evidence Recall@10**, as the **eval/reporting convention** — *not* an
API-enforced result-set cap. **Accuracy note (codex round-7 [P2]):** `search()` does **not**
truncate its final result set to 10. Only the **vector branch's phase-2 rerank** is limited to
`SEARCH_RERANK_LIMIT`=10 (`lib.rs:3388`); the **FTS/text branch is unbounded by default** and the
fused `SearchResult.results` is returned **without a final top-K truncation** (`lib.rs:2206`;
`read_search_in_tx` returns the full fused list). So @10 is chosen as the headline because it is
the natural UX-proximal depth *and* it matches the vector-rerank depth + the eu7/eu8 K=10 the rest
of FathomDB's recall measurement already uses — the eval applies the @K cut itself; it must not
assume the API enforces a top-10. @50 is the **retriever-health** companion (separating "the
ranker buried it" from "retrieval never surfaced it"); @5 is the UX-proximal stress; @200 is a
diagnostic ceiling, never a UX claim. **No threshold number is assigned to any K here** — the
pass/fail lines are Phase-4 experiment output + IR-2/HITL.

---

## (d) Per-class structure (structure, not numbers)

Evidence Recall is **stratified by query class**, because the stakes differ by class and a
single mean hides must-not-miss failures. The classes (the *structure*; the per-class
thresholds are **TBD**, Phase 4 / IR-2):

| Class | What it asks of memory | Maps onto existing chain `relation_type` |
|-------|------------------------|------------------------------------------|
| **commitment** | a promise/obligation + its parties + due date | `action_from`, `follows_up_on` |
| **action** | the next action a note/email implies | `action_from` |
| **exact-fact** | a specific retrievable fact (number, name, date) | `mentions`, `summarizes` |
| **preference** | a stated user preference / standing instruction | (new — not in current chains) |
| **exploratory** | open-ended "what do I know about X" recall | `summarizes`, `mentions` |
| **negative ("not-found")** | the answer is *absent*; correct behaviour is to return nothing / abstain | `contradicts` (partial), (new) |

Two structural notes:

1. **Must-not-miss classes** (commitment / exact-fact) are reported **separately** and are the
   natural future stricter-gate candidates — but their thresholds are **TBD** and **must be
   re-anchored to FathomDB's embedder class** before any gate (the charter is explicit: against
   a measured 0.571 IR ceiling, importing generic-RAG "≥98–99%" lines would produce a
   permanently-red, uninformative gate — §3 item 2, §5 #7).
2. **The negative class is first-class.** "Correctly returns nothing when nothing is relevant"
   is a real product property (avoiding confident retrieval of irrelevant memory). It is scored
   as **abstention-correctness**, not recall, and reported separately. The existing
   per-relation / per-chain-shape buckets (`eu8_ir_validation.rs:321-344`) are the mechanism
   this stratification grows into.

---

## (e) Retrieval modes to compare (with today-vs-future flags)

The measure is reported **per retrieval mode**, so the eval separates embedder/ranker effects
from retrieval-architecture effects:

| Mode | Status today | Anchor |
|------|--------------|--------|
| **FTS branch** (FTS5 `MATCH` filter) | **EXISTS, but NOT bm25-ranked in production** — the text branch carries `bm25()` only as a *score*; it is `ORDER BY write_cursor` (insertion order), and RRF fuses on that insertion-order rank | `lib.rs:3905-3928` (text SQL: `ORDER BY write_cursor`, `bm25()` selected as score) |
| **BM25-ranked FTS-only baseline** | **HARNESS-CONSTRUCTIBLE, not the production path** — a true BM25 baseline needs the eval to order by the already-carried `bm25()` score; **no engine change** (score is present), but it is NOT what production search ranks by today | `lib.rs:3918-3920` (score carried, order is write_cursor) |
| **vector-only** (bit-KNN K=192 → f32 rerank) | **EXISTS** | `ADR-0.7.0-vector-binary-quant.md:97-106` |
| **RRF-hybrid** (the production fused ranking) | **EXISTS** — the unconditional ranking; fuses the vector branch and the **write-cursor-ordered** text branch on rank | `lib.rs:3955-3956,3584-3623` |
| **+reranker** (rerank top-N → top-K) | **STUB** — `rerank_fused()` is identity | `lib.rs:3653-3660` |
| **+graph expansion** (`expand=N` / neighbors) | **0.8.1** — designed, deferred | `ADR-0.8.0-graph-traversal-scope.md`; `dev/roadmap/0.8.1.md` |

So the runnable-now modes are **vector-only**, the **production RRF-hybrid** path, the **FTS
`MATCH` branch as it actually ranks** (write-cursor order), and a **BM25-ranked FTS-only
baseline** the eval can construct by ordering on the already-carried `bm25()` score (a
harness-level `ORDER BY`, no engine change). The eval **must not** silently equate the
production FTS branch with a BM25-ranked baseline — they rank differently; report both and
label which is which. **+reranker is a no-op identity stub** and **+graph is 0.8.1**, so any
reranker-/graph-dependent comparison is **aspirational and report-only until those land** — it
must not be used to set or fail a gate. The headline mode is the **production RRF-hybrid** path
(the unconditional ranking the agent actually gets).

> **Note (surfaced by the codex consult, [P2]):** that the production FTS branch is
> write-cursor-ordered rather than bm25-ranked is a *measurement-relevant property*, not a
> defect this doc fixes. The eval names both the production branch and a bm25-ordered baseline
> explicitly so the mode comparison is honest about what FathomDB ranks by today.

---

## (f) Eval-set composition + qrels/pooling methodology + the pinning principle

**Composition.** The eval set is a set of `(query, query_class, required_evidence[])` records
(the (b) schema) over a **fixed document corpus**. It is built as an *additive superset* of the
existing chain `ground_truth_queries` (so eu8 is the doc-granularity baseline and no labeling is
thrown away). Class coverage (d) is a composition constraint: every class is represented,
including a deliberate **negative/"not-found"** subset whose correct answer is empty.

**qrels methodology — seed-then-pool (TREC-style pooling that does NOT define the denominator).**
The Recall@K **denominator is a single, authored required-evidence set per query** — authored
**independently of any retrieval result** and *always* in the qrels, **whether or not any mode
surfaces it.** This is the load-bearing rule: **pure pooling must not be the source of the
required-evidence set.** If the denominator were only "what some mode returned in top-N," a
required fact that *no* mode surfaces would silently drop out — making the metric
**self-confirming on exactly the hard queries the eval exists to catch** and overstating recall
(the codex round-3 [P2] finding). So:

1. **Seed** the qrels with the authoritative known positives, independent of retrieval, **as one
   consistent unit of relevance per query** (codex round-5 [P2] — do not mix units):
   - When the gold record carries `required_evidence` (b), **that is the denominator**, full stop.
   - For a **legacy/unlabeled** record that has only `expected_top_k_doc_ids` (today's eu8
     chains), those doc-ids are mapped **exactly once** to the **degenerate whole-document
     evidence units** (necessity=`required`) — this is the eu8 reduction of (a). They are a
     **fallback** used *only when* `required_evidence` is absent; they are **never added on top
     of** an evidence-labelled set (that would double-count and require docs that may not be
     necessary evidence). One query → one unit-of-relevance system.
2. **Pool to *augment*** — run **each retrieval mode (e) separately** (the production FTS branch,
   the bm25-ordered FTS baseline, vector-only, RRF-hybrid; +reranker/+graph when real), take the
   **union of their top-N candidates**, and label that pool to *discover additional* relevant
   judgments the authors missed. Pooling **adds** positives; it can never **remove** a seeded
   required positive from the denominator.

The primitives exist — `fuse_rrf` already consumes the two branch lists as separate inputs
(`lib.rs:3584-3607`), so the pooling step is **harness orchestration, not an engine change**.
Judgments are **binary** (required / supporting / not-relevant) for now; **graded** judgments
(for full nDCG) are a labeled superset, flagged TBD. The CI is a **bootstrap** percentile
interval over per-query recall, reusing the established method (`eu8_ir_validation.rs:283-304`).

**The pinning PRINCIPLE (the lesson behind the GA recall halt).** The eval set MUST run against
a **single, pinned, versioned corpus snapshot + a versioned qrels set**, and **both the corpus
hash and the qrels version are recorded with every reported number.** This is non-negotiable
*methodology*: fidelity recall is known to **drift with N** (eu7's own anchor note,
`eu7_real_corpus_ac.rs:834-837`), and the GA halt (0.937 anchor measured pre-expansion vs
0.8710 on the silently-expanded corpus, `AC-075`/MEMORY) is exactly the failure of an
unpinned basis. A relevance number compared across *different* corpora is meaningless.

> **This document does NOT pick the corpus version/snapshot.** Which snapshot the eval (and the
> floor) is pinned *to* is settled **downstream** by the **B-1 corpus-basis ruling** + the
> **corpus freeze** (`dev/plans/0.8.0-GA-and-IR-eval-roadmap.md`). Here we fix only the
> *invariant*: pinned + versioned + hash-recorded, so the eval set cannot silently drift.

---

## (g) Fidelity-vs-relevance separation (why this is the product-value axis)

The two axes answer different questions and **must both exist, separately**:

- **eu7 / AC-075 — FIDELITY (system health).** "Does the cheap quantized index faithfully
  reproduce the exact-f32 neighbours of the *same* embedder?" Ground truth is the model's *own*
  exact top-10. **Necessary, not sufficient.** Gated at ≥0.90 recall@10. **Untouched by this
  work.**
- **Evidence Recall@K — RELEVANCE / PRODUCT VALUE.** "When the agent needs a memory to act, is
  the required evidence in the retrieved context?" Ground truth is **external** labelled
  required-evidence. This is the axis a user actually feels.

FathomDB already discovered the gap empirically: fidelity 0.937 vs IR relevance ≈0.571 — ~37 pp
apart **on purpose** (`ADR-0.7.0-vector-binary-quant.md:172-179`). The load-bearing consequence:
**once fidelity exceeds the relevance ceiling, pushing fidelity higher buys ≈0 product value** —
the lever for end-to-end quality is a better embedder or the graph (0.8.1), not K/ANN tuning.
This is precisely why a *separate* product-value measure is needed: the fidelity gate cannot see
product quality, and a relevance gate cannot replace the system-health guarantee. They are
orthogonal and both required. **Nothing in this measure weakens, re-anchors, or substitutes for
eu7 / AC-075.**

---

## § Consensus (Claude↔codex)

> Convergence record per the prompt §2.2. Consult log:
> `dev/plans/runs/IR1-phase1-codex-consult-20260608T011707Z.md`.

**Converged decision (Claude + codex).** The measure is **Evidence Recall@K** (strict all-of
headline + graded diagnostic) on the **IR/agentic-relevance (product-value) axis**, with the
atomic-evidence unit-of-relevance and gold-set schema of (b), the @5/@10(headline)/@20/@50
K-ladder of (c), the six-class stratification of (d), the retrieval-mode matrix of (e), and the
pooled-qrels + pinned-versioned-corpus methodology of (f). It is **complementary to and does
not alter eu7 / AC-075**; every threshold is **TBD** (Phase 4 / IR-2). Claude and codex
**converged** on this definition.

**codex consult (round 1) — three findings, all accepted and resolved (no definitional
disagreement):**

1. **[P2] FTS-only mode mislabeled as "BM25 over FTS5."** Codex correctly observed that the
   production text branch is `ORDER BY write_cursor` (insertion order) and carries `bm25()` only
   as a score (`lib.rs:3905-3928`), so a harness following the original wording would wrongly
   treat the production FTS branch as a BM25-ranked baseline. **Resolved:** §(e) now distinguishes
   the **production FTS `MATCH` branch (write-cursor-ordered)** from a **BM25-ranked FTS-only
   baseline** the eval constructs by ordering on the already-carried `bm25()` score (harness-level
   `ORDER BY`, no engine change), and requires the eval to label which is which. This is a
   genuine measurement-accuracy improvement to the definition.
2. **[P3] Consensus record incomplete while status claimed "signed."** **Resolved:** this section
   now records the actual convergence (was a pre-loop placeholder).
3. **[P3] Stray `</content>`/`</invoke>` tool-wrapper markup at EOF.** **Resolved:** removed.

**codex consult (round 3) — one substantive methodology finding, accepted:**

4. **[P2] qrels must seed known positives independent of pooling (§(f)).** Codex observed that a
   *pooling-only* qrels drops required evidence that no mode surfaces, so the Recall@K denominator
   would omit the exact misses the eval exists to catch — self-confirming on hard queries,
   overstating recall. **Resolved:** §(f) rewritten to **seed-then-pool** — the denominator is the
   **authored** required-evidence set (independent of retrieval, always present; one unit of
   relevance per query — see finding 7 for the legacy-doc-id refinement); pooling **only augments**
   discovery and can never remove a seeded required positive.

**codex consult (round 4) — two schema/scoring consistency findings, accepted:**

5. **[P2] schema must use the eu8 `query` key (b).** The draft's `query_text` would break the
   "additive superset" claim — the eu8 parser reads `q.get("query")` (`corpus_subset.rs:239`).
   **Resolved:** schema now uses `query` (+ additive `query_id`).
6. **[P2] graded recall denominator was self-contradictory (a)/(b).** (a) defined graded over
   `required`; (b) said `supporting` feeds graded. **Resolved:** graded recall is over the
   `required` set **only** (same denominator as strict); `supporting` is removed from both recall
   numbers and reported as a separate supporting-coverage diagnostic. (a) and (b) now agree.

**codex consult (round 5) — one denominator-purity finding, accepted:**

7. **[P2] §(f) must seed ONE unit of relevance, not mix evidence units with legacy doc-ids.** The
   round-3 wording "`required_evidence` + `expected_top_k_doc_ids`" would double-count / require
   non-necessary legacy doc-ids. **Resolved:** §(f) now seeds **one** unit per query —
   `required_evidence` is the denominator when present; `expected_top_k_doc_ids` map **exactly
   once** to degenerate whole-document required units **only** as a fallback when evidence labels
   are absent (the eu8 reduction), never added on top of an evidence-labelled set.

**codex consult (round 7) — one production-surface accuracy finding, accepted:**

8. **[P2] §(c) anchored @10 to a nonexistent API `LIMIT`.** `search()` does not truncate to 10;
   only the vector phase-2 rerank is capped at `SEARCH_RERANK_LIMIT`=10 and the fused result is
   returned untruncated. **Resolved:** §(c) reframes @10 as the **eval/reporting convention**
   (aligned with the vector-rerank depth + eu7/eu8 K=10), with an explicit note that the API does
   not enforce a top-10 and the eval applies the @K cut itself.

**Convergence:** the methodology in (a)–(g) is coherent and consensus-signed. Trajectory:
round 1 = §(e) FTS accuracy + cleanups; round 2 = doc coherent; round 3 = §(f) seed-then-pool;
round 4 = schema/scoring consistency; round 5 = single-unit-of-relevance denominator; round 6 =
ledger alignment; round 7 = §(c) @10 reframed as a reporting convention. Every finding accepted
and resolved; no definitional reversal.

**Residual disagreements escalated to HITL:** **none.** (The substantive product decisions —
actual threshold numbers, the exact corpus snapshot, whether/when this becomes a *gate* — are
deliberately out of this phase's scope and belong to Phase 4 experiments + the IR-2 / HITL gate,
not to this consensus.)

---

## References

- Charter: `dev/notes/recall-eval-framework-assessment-20260607T174821Z.md`
- Fidelity gate: `src/rust/crates/fathomdb-engine/tests/eu7_real_corpus_ac.rs`;
  `dev/adr/ADR-0.7.0-vector-binary-quant.md` §2 (recall floor + the fidelity-vs-IR note,
  lines 123-179)
- IR harness (the seed): `src/rust/crates/fathomdb-engine/tests/eu8_ir_validation.rs`;
  qrels shape `support/corpus_subset.rs:214-260`
- Capabilities/goals: `dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md`;
  `dev/adr/ADR-0.8.0-graph-traversal-scope.md`; `dev/roadmap/0.8.1.md`;
  `dev/architecture.md` §9 (the two-axis open question this resolves)
- Sequencing: `dev/plans/0.8.0-GA-and-IR-eval-roadmap.md`;
  prompt `dev/plans/prompts/0.8.x-IR-1-phase1-measure-consensus.md`
