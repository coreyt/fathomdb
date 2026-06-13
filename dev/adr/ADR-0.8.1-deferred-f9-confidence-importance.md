---
title: ADR-0.8.1-deferred-f9-confidence-importance
date: 2026-06-13
target_release: 0.8.2+
desc: Framing ADR for F9 — confidence-vs-importance weighting on graph-retrieved nodes and edges. Deferred pending consumer signal + eval results from the BYO-LLM ingest track (Slices 0/15) and the R2 parity eval (Slice 25). Captures what Slice 15 now provides, what signal is needed before deciding, and what the implementation would look like.
status: DEFERRED — 0.8.2+
origin: dev/plans/0.8.1-implementation.md Slice 35 ("deferred-feature framing ADRs"); dev/design/0.8.0-v05-feature-triage.md F9 (DEFER 0.8.x); dev/adr/ADR-0.8.0-filter-grammar.md §7 follow-on
---

# ADR-0.8.1 — Deferred F9: Confidence-vs-Importance Weighting

**Status:** `DEFERRED — 0.8.2+`. **No code change follows from this ADR.** Framing-only.
HITL sign-off is NOT required in Slice 35; this ADR is authored to capture the decision
model for future work.

---

## 1. Context — what is F9?

F9 is the feature gap covering **confidence-vs-importance weighting** on graph-retrieved
facts. The v0.5.6 feature triage (`dev/design/0.8.0-v05-feature-triage.md` §F9) classified
F9 as `DEFER 0.8.x` — the table-stakes retrieval quality delivered by G1/G9/G10 (dense +
lexical fusion with RRF) is usable without confidence weighting; F9 adds a *quality
differentiation* once the underlying confidence signal exists and is validated.

**Relationship to G12 (importance).** G12 is the existing `recency`-weighted re-ranking
flag (Slice 10). F9 is a distinct axis: not recency but fact-level **confidence** (how
strongly does the LLM-extracted fact claim to be true?) vs node/edge-level **importance**
(how central is this entity/fact to the memory graph?).

---

## 2. What Slice 15 (CLOSED on main) now provides

Slice 15 shipped the G11 BYO-LLM ingest API with the following schema additions:

- `canonical_edges.confidence REAL` — the extractor-supplied confidence score
  (`0.0..=1.0`) for each extracted fact-edge. Nullable for pre-G11 edges.
- `canonical_edges.body TEXT` — fact-edge payload body (JSON), enabling
  edge-body retrieval and `json_extract`-style filtering on edge properties.
- `canonical_edges.t_valid`/`t_invalid` — temporal validity window for the fact.

What Slice 15 does NOT provide: a `confidence` column on `canonical_nodes` (only on edges),
and no importance score computation (that would require graph centrality, e.g. PageRank or
degree weighting, not currently computed by the engine).

---

## 3. What consumer signal is needed before deciding

The F9 implementation decision has two prerequisite inputs:

**3.1 Eval signal (R2 eval harness, Slice 25)**
The R2 end-to-end parity eval (Mem0-OSS baseline, per-class scoring: factoid / temporal /
multi-hop / knowledge-update / multi-session) is the only instrument that can measure whether
confidence-weighted retrieval improves result quality on the categories where it matters
(temporal facts, competing-update resolution). The `confidence` column was added in Slice 15,
but no eval harness yet measures whether high-confidence facts improve downstream task
accuracy. **R2's temporal and knowledge-update class scores are the prerequisite signal.**

**3.2 Consumer-side importance signal**
The Memex / Hermes / OpenClaw consumer agents currently do not feed back an "importance" or
"relevance" signal to FathomDB beyond query-time retrieval. A confidence-vs-importance
weighting that factors in consumer access patterns (e.g., a more-accessed entity should rank
higher) requires a feedback loop that does not yet exist. **The consumer feedback protocol
is a future BYO-LLM harness extension (not in Slice 15's `fathomdb.extract.v1`).**

**3.3 Volume of `confidence`-bearing edges**
At the time of Slice 35, the `confidence` column is populated only by BYO-LLM ingest
(Slice 15). Its population density in real-world memory graphs has not been measured.
A weighting feature that mostly fires on NULL values is not useful. **The first meaningful
eval requires ≥1 real BYO-LLM ingest run on the R2 eval corpus to confirm coverage.**

---

## 4. What the implementation would look like (rough sketch, not a contract)

Once the prerequisite signals exist:

**4.1 Confidence-weighted RRF scoring (graph arm)**
The Slice 30 graph-retrieval arm (R3, the third RRF arm) fuses fact-edge nodes into the
retrieval result. Confidence weighting would modify the graph arm's contribution to the
RRF rank:

```
graph_rrf_score(edge) = confidence(edge) × 1/(K + bfs_rank(edge))
```

where `bfs_rank` is the BFS depth × edge count, and `confidence` defaults to `1.0` for
NULL (pre-G11 edges). This keeps the formula additive and backward-compatible.

**4.2 Importance weighting (node-level)**
Node importance could be approximated as in-degree on `canonical_edges(to_id)` (how many
edges point to this node). A materialized `importance` score would require a periodic
background computation or a trigger-based update — both have write-side cost. The simpler
path is a real-time `COUNT(*)` sub-query at retrieval time (feasible at small graph scales,
expensive at large scale). Decision deferred until consumer graph scale is measured.

**4.3 API surface (speculative)**
A future `read_list` or `search_with_expand` option:
```rust
pub struct GraphRetrievalOptions {
    pub confidence_weight: Option<f64>,  // None = equal weight
    pub importance_weight: Option<f64>,  // None = equal weight
}
```
Or folded into the existing `Predicate` filter (filter by `confidence > 0.8`, for example).

---

## 5. Decision model (when to open the implementation slice)

Open the F9 implementation slice when ALL THREE gates are green:

1. **R2 eval harness exists** (Slice 25 CLOSED) AND temporal/knowledge-update class scores
   show a measurable gap between RRF-fused retrieval and confidence-weighted retrieval.
2. **≥100 `confidence`-bearing edges** exist in the test corpus (BYO-LLM ingest at scale).
3. **HITL sign-off** on the chosen weighting formula (additive confidence-weight vs
   multiplicative vs recency-vs-confidence trade-off axis).

**Do NOT open an implementation slice speculatively.** The `confidence REAL` column is
present and will populate as BYO-LLM ingest runs, but a feature that fires on small or
NULL data is not useful product surface.

---

## 6. Reserved follow-on

- `dev/roadmap/0.8.2.md` tracks F9 as a 0.8.2 candidate.
- The R2 eval harness (Slice 25) is the gating input; its temporal/knowledge-update results
  are the decision trigger.
- Slice 40's GA verify will include a status check on F9 (framing ADR present, no
  implementation signal yet → status stays DEFERRED — 0.8.2+).
