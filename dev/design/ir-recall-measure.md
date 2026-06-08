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
- **Graded (fraction-of) evidence recall@K** — per query, `|required ∩ retrieved@K| /
  |required|`; averaged. This is the **diagnostic** companion (how close did we get when we
  missed?), and it degrades gracefully so it is useful below the all-of threshold.

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
  "query_id": "stable-unique-id",
  "query_text": "...",
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
FathomDB chunks — not planned for 0.8.0/0.8.1). `necessity=required` units gate the strict
all-of recall; `supporting` units feed the graded recall only. **The label *values* (which
facts are required for which query) are produced in Phase 3 by human/HITL labeling — this
document defines only the slot they go in.**

---

## (c) The K-ladder + the headline K

Report at **K ∈ {5, 10, 20, 50}** (extensible to 200 as a pure diagnostic). Rationale,
anchored to FathomDB's real surface:

| K | Role | Anchor |
|---|------|--------|
| **@5** | UX-proximal (what the agent actually reads first) | — |
| **@10** | **HEADLINE** — matches the production `search()` `LIMIT`=10 | `eu7_real_corpus_ac.rs:444-445`; eu8 `K=10` (`:70`) |
| **@20** | near-UX / reranker-target band (when a real reranker lands) | charter §3 item 1 |
| **@50** | **retriever-health** (is the evidence anywhere in the candidate pool?) | charter §5 #3 |
| (@200) | deep diagnostic only — not a UX surface | charter §3 (chunk/fact note) |

**Headline = Evidence Recall@10**, because 10 is FathomDB's production retrieval LIMIT — the
number the agent actually sees. @50 is the **retriever-health** companion (separating "the
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
| **FTS-only** (BM25 over FTS5) | **EXISTS** (branch runs inside `read_search_in_tx`) | `lib.rs:912-917`; charter §1 |
| **vector-only** (bit-KNN K=192 → f32 rerank) | **EXISTS** | `ADR-0.7.0-vector-binary-quant.md:97-106` |
| **RRF-hybrid** (the production fused ranking) | **EXISTS** — the unconditional ranking | `lib.rs:3564,3584-3623` |
| **+reranker** (rerank top-N → top-K) | **STUB** — `rerank_fused()` is identity | `lib.rs:3653-3660` |
| **+graph expansion** (`expand=N` / neighbors) | **0.8.1** — designed, deferred | `ADR-0.8.0-graph-traversal-scope.md`; `dev/roadmap/0.8.1.md` |

So **4 of 5 modes are runnable now** (FTS / vector / RRF-hybrid as the fused production path,
plus the two branches poolable separately); **+reranker is a no-op identity stub** and
**+graph is 0.8.1**, so any reranker-/graph-dependent comparison is **aspirational and
report-only until those land** — it must not be used to set or fail a gate. The headline
mode is the **production RRF-hybrid** path (the unconditional ranking the agent actually gets).

---

## (f) Eval-set composition + qrels/pooling methodology + the pinning principle

**Composition.** The eval set is a set of `(query, query_class, required_evidence[])` records
(the (b) schema) over a **fixed document corpus**. It is built as an *additive superset* of the
existing chain `ground_truth_queries` (so eu8 is the doc-granularity baseline and no labeling is
thrown away). Class coverage (d) is a composition constraint: every class is represented,
including a deliberate **negative/"not-found"** subset whose correct answer is empty.

**qrels + pooling methodology (TREC-style).** To avoid scoring only what the production ranker
happens to surface (which would make the gate self-confirming), relevance judgments are formed
by **pooling**: run **each retrieval mode (e) separately** (FTS-only, vector-only, RRF-hybrid;
+reranker/+graph when real), take the **union of their top-N candidates**, and label that pool.
The primitives exist — `fuse_rrf` already consumes the two branch lists as separate inputs
(`lib.rs:3584-3607`), so pooling is **harness orchestration, not an engine change**. Judgments
are **binary** (required / supporting / not-relevant) for now; **graded** judgments (for full
nDCG) are a labeled superset, flagged TBD. The CI is a **bootstrap** percentile interval over
per-query recall, reusing the established method (`eu8_ir_validation.rs:283-304`).

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

> Convergence record per the prompt §2.2. Consult logs:
> `dev/plans/runs/IR1-phase1-codex-consult-<ts>.md`.

_(to be completed after the codex consensus loop)_

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
</content>
</invoke>
