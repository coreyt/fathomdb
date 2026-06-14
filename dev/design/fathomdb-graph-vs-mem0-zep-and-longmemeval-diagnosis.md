# FathomDB's graph implementation vs Mem0 / Zep, and the LongMemEval underperformance diagnosis

> **Purpose.** Answer four questions for the 0.8.1 graph track: (1) what *is*
> FathomDB's current underlying graph implementation; (2) how Mem0, Microsoft
> GraphRAG, and Zep differ;
> (3) does FathomDB have a "basic underlying graph ability" but a muddled
> node/edge structure that causes it to underperform on LongMemEval; (4) what
> experiments tell us the most valuable design choices.
>
> **Status:** analysis / design input. Author: 2026-06-14. Grounds: the FathomDB
> code (cited `file:line`), the existing ADR/design corpus, external primary
> sources (cited URLs), and the Slice-30 LongMemEval (LME) Option-3 run supplied
> by the implementation agent (treated as *additive, independently re-derived
> where possible*).
>
> **TL;DR.** The node/edge *model* is **not** the problem — it is clearly
> specified, HITL-signed, and architecturally close to Zep/Graphiti (fact-on-edge,
> bi-temporal). The LME underperformance has **three distinct, separable causes**,
> none of which is "muddled node-vs-edge": **(A)** a read-path identity/provenance
> contract gap that makes the graph arm *invisible to the evaluator*; **(B)**
> ~1% extraction coverage (200 / 19,195 sessions) that starves the graph arm of
> signal; and **(C)** a base-retrieval deficit — FathomDB's dense+FTS fusion trails
> plain BM25 on this conversational corpus, *upstream of and independent of* the
> graph. There is also a real **structural-completeness** gap vs Zep (missing
> episodic/community tiers, fact-surfacing, and graph-aware rerankers), but that is
> "missing pieces," not "wrong model."

---

## 1. FathomDB's current graph implementation (ground truth from the code)

FathomDB does **not** embed a separate graph engine. The graph is a **single
ontology-neutral binary property-graph substrate** living in the same SQLite
database as everything else, designed to serve both corpus (GraphRAG-shaped) and
memory (Graphiti-shaped) ontologies through opaque `logical_id` addressing
(`dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md` H1/H3, HITL-signed
2026-06-05).

### 1.1 Schema (SCHEMA_VERSION 15; `fathomdb-schema/src/lib.rs`)

**`canonical_nodes`** = entities (and corpus docs). Columns: `write_cursor`,
`kind`, `body`, `source_id` (nullable, step 8), `logical_id` (nullable, step 12),
`superseded_at` (nullable, step 12).

**`canonical_edges`** = relationships / **fact-edges**. Columns: `write_cursor`,
`kind`, `from_id`, `to_id`, `source_id`, `logical_id`, `superseded_at`, plus the
**G11 enrichment** (step 14): `body` (the fact text), `t_valid`, `t_invalid`
(event valid/invalid time, ISO-8601), `confidence`, `extractor_model_id`; plus
**R3** (step 15) `temporal_fallback` (provenance flag for ELPS-defaulted `t_valid`).

Indexes: `canonical_edges_from_id_idx (from_id)`, `canonical_edges_to_id_idx
(to_id)`, and a **partial-unique** `canonical_edges_logical_active_idx (logical_id)
WHERE superseded_at IS NULL`.

**Identity** (`lib.rs` `derive_logical_id`, ~7711): nodes =
`sha256(kind.lower():name.lower())`; edges = a `sha256` over the endpoint pair +
relation. **Active-uniqueness is `logical_id` ALONE** (Slice 31, HITL-signed) — one
active row per logical id; re-ingest tombstones (`superseded_at`) then inserts.

### 1.2 Model: facts live on EDGES (Graphiti-shaped)

A **node** is an entity (`kind` + `body` = entity name). An **edge** is a
relationship carrying the **fact text** in `body` (`lib.rs` `PreparedWrite::Edge`,
~1375). Edge `body`, when present, is projected into FTS (`search_index_edges`,
`lib.rs:8244`) and into vectors as kind `"edge_fact"` (`lib.rs:8258`), surfaced in
search under branch `text_edge`. This is the deliberate "fact-on-EDGE" choice;
fact-on-NODE (HyperGraphRAG-style n-ary) is reserved as the H6 escape hatch.

### 1.3 Temporal model: bi-temporal-aware, invalidate-not-accumulate

- **Transaction time** = `superseded_at` (the "system time" half).
- **Event/valid time** = `t_valid` / `t_invalid` (G11), the "what was true when" half.
- **Invalidate-not-accumulate**: re-ingesting a fact on the same
  `(from_id,to_id,kind)` tombstones the prior row and inserts the new one; the
  read filter is `superseded_at IS NULL AND (t_invalid IS NULL OR t_invalid > now)`
  (`ADR-0.8.1-graph-substrate-g11-migration.md`; BFS SQL at `lib.rs:5405`).

This is the same conceptual model as Zep/Graphiti's four-timestamp edge — see §2.2.

### 1.4 Retrieval: hybrid RRF + a young graph arm

The production path is RRF fusion of a **dense** arm and an **FTS** arm. Slice 30
(R3) added a **third graph arm**, gated behind `use_graph_arm` (default `false`):

1. Seed from the top-10 two-arm fused hits → resolve each `write_cursor` to a
   `logical_id` via `canonical_nodes` (`lib.rs:5379-5388`).
2. BFS over `canonical_edges` (`from_id = ? OR to_id = ?`, `superseded_at IS NULL`,
   temporal filter, `temporal_fallback` excluded), depth ≤ 3, cap 50
   (`lib.rs:5405-5412`, `5320`).
3. For each reached node, fetch `kind, body, write_cursor` from `canonical_nodes`
   (`lib.rs:5416`); score `1.0/(1.0+hop_count)`, ×0.3 penalty for synthesized
   `kind="unknown"` (`lib.rs:5456`).
4. Fuse as a third RRF arm via `fuse_three_arms(...)` (`lib.rs:4656-4706`), dedup
   on body, graph arm never overrides a vector-branch identity.

### 1.5 Construction: BYO-LLM (no LLM in the engine)

Graph construction is **caller-supplied** via the `fathomdb.extract.v1`
NDJSON-over-stdio protocol (`ingest_with_extractor`). The harness returns
entities + edges with `source_doc_id`; the engine writes them, **preserving the
source link in the `source_id` column on both tables** (`lib.rs:2623`, `2748`,
`2778`; reverse lookup via `trace_source_ref`/`excise_source`). FathomDB itself
makes no network/LLM call. (ELPS = the Memex-side extractor harness.)

### 1.6 The implementation gap that matters for evaluation

For a **graph-arm hit**, `SearchHit.id` is set to `write_cursor as u64` — the
canonical row id, **not** `source_id` (`lib.rs:5459-5465`; the SDK documents this:
`SearchHit.id` "is the canonical row's `write_cursor`", `types.py:50`). The source
provenance *exists* (`source_id` column) but is **not carried on the SearchHit**.
Any evaluator that maps hits → source via `SearchHit.id` therefore cannot match
pre-labeled session ids. **This is a read-path/contract gap, not a lost-data
problem and not a model problem.** It is the proximate reason the graph arm
"contributes nothing" in the LME run (§3).

### 1.7 Envelope / where FathomDB deliberately differs from the peers

Single-writer, single-file, in-process SQLite; brute-force vector + FTS;
≤100k–1M record envelope; BYO-LLM construction. This is a *deliberate* footprint
contract (`ADR-0.8.0-graph-model-and-edge-addressing.md:105`), not an oversight —
it rules out the Neo4j-class engines Zep and (graph-) Mem0 assume.

---

## 2. How Mem0 and Zep differ

### 2.1 Mem0 (OSS; arXiv 2504.19413)

- **Substrate**: vector-store-first (**Qdrant** default), with OpenAI
  `text-embedding-3-small` + an LLM (`mem0/vector_stores/configs.py`,
  `embeddings/openai.py`). A "memory" is an **LLM-extracted natural-language
  fact** (e.g. "Prefers vegetarian food"), *not* a triple and *not* a raw chunk.
- **Ingest = coherence-first overwrite.** The paper's `add()` runs an LLM that
  emits **ADD / UPDATE / DELETE / NOOP** against the top-similar existing
  memories — contradictions are **deleted**, not versioned
  (https://arxiv.org/html/2504.19413v1). *Drift to flag:* current OSS `main` (the
  "v3" pipeline) is actually **ADD-only + exact-hash dedup** — the LLM
  update/delete reconciliation is no longer on the default path
  (https://docs.mem0.ai/migration/oss-v2-to-v3).
- **Graph variant (Mem0g) was REMOVED from current OSS** (v3); when it existed it
  used Neo4j/Memgraph triples and was **single-axis, not bi-temporal** — the paper
  claimed soft-invalidation but the shipped code hard-`DELETE`d. Its reported lift
  over the vector base was only ~2% (arXiv 2504.19413; migration doc above).
- **Retrieval**: paper = pure dense top-k; OSS v3 = semantic + BM25 + entity-boost
  fused. **No time-travel** (passing a `reference_date` raises in OSS).
- **Net center of gravity: overwrite, one vector substrate, no temporal graph.**

### 2.2 Zep / Graphiti (arXiv 2501.13956; github.com/getzep/graphiti)

- **A bi-temporal knowledge graph** in three subgraph tiers (paper §2): an
  **episodic** tier (raw messages, non-lossy, `t_ref`), a **semantic** tier
  (resolved entity nodes + **fact edges**), and a **community** tier (entity
  clusters + summaries). **Facts live on edges** — "These temporal data points are
  stored on edges alongside other fact information" (§2.2.3).
- **Bi-temporal, four timestamps per edge**: `t'_created`/`t'_expired` (system) and
  `t_valid`/`t_invalid` (event) (`graphiti_core/edges.py`). **Invalidate, don't
  delete**: a new contradicting edge sets the old edge's `t_invalid = t_valid` of
  the new one; both rows are kept for time-travel (§2.2.3). This is Zep's headline
  differentiator.
- **Provenance is graph-native**: episodic edges + bidirectional indices let every
  fact/entity trace back to its source episode and vice-versa (§2.2).
- **Retrieval is multi-method × multi-scope × multi-reranker**: cosine + BM25 + BFS,
  over edges (→facts) / nodes (→summaries) / communities (→summaries), reranked by
  RRF / MMR / **cross-encoder** / **node-distance** / **episode-mentions**
  (blog "how-do-you-search-a-knowledge-graph"; paper §3). Summaries are built at
  *ingest*, not retrieval, for low latency.
- **Benchmarks (vendor-authored)**: LongMemEval **71.2%** (gpt-4o) vs full-context
  60.2%, ~98% fewer context tokens, ~90% lower latency (arXiv 2501.13956 Table 2).
  Per-class wins on temporal/multi-session/knowledge-update; a regression on
  single-session-assistant. (The oft-cited "94.8%" is **DMR, not LongMemEval**.)

### 2.3 GraphRAG (Microsoft; arXiv 2404.16130; github.com/microsoft/graphrag)

GraphRAG is the **different-task** peer: it is built for **query-focused
summarization / global "sensemaking" over a static corpus** ("What are the main
themes in this dataset?"), **not** agent memory, point-in-time recall, or
conversational sessions. It is **not evaluated on LongMemEval or LOCOMO** — those
measure a categorically different thing (fact recall from multi-session memory).
This matters here because FathomDB's substrate is *deliberately ontology-neutral to
serve **both*** the GraphRAG-shaped **corpus** ontology and the Graphiti-shaped
**memory** ontology on one table set (`ADR-0.8.0-graph-model-and-edge-addressing.md`
H1). So GraphRAG is the reference for FathomDB's *corpus* use case, while Zep is the
reference for the *memory* use case that LongMemEval scores.

- **Model**: a labeled attributed graph — **entity nodes** (`name` / `type` /
  LLM `description`), **relationship edges** (`description` + numeric `weight`), and
  **claims/covariates** as a *separate, optional, off-by-default* element with text
  metadata incl. `start_date`/`end_date`/`status`. Conceptually a property graph,
  but stored as **parquet tables + an in-memory NetworkX graph** (no native graph
  DB), not a queryable engine
  (https://microsoft.github.io/graphrag/index/outputs/).
- **Indexing** (heavy, index-time-LLM-bound; README warns it "can be an expensive
  operation"): chunk → LLM entity+relationship extraction with multi-round
  "gleanings" → **Leiden hierarchical community detection** (MECE partition per
  level) → **LLM community summaries at every level** of the hierarchy
  (arXiv 2404.16130 §2).
- **Retrieval**: **Global search** = map-reduce over community summaries (the
  paper's core contribution — corpus-wide synthesis); **Local search** (OSS) =
  entity-anchored, gathering an entity's relationships, claims, neighbors,
  community reports, *and source text units*; **DRIFT** = a hybrid of the two.
- **Temporal**: **none** — no bi-temporal model, no edge invalidation, no
  point-in-time/as-of query; updates merge into a single current index with no
  versioning. The only "time" is LLM-extracted *text metadata on claims*, not an
  engine validity interval (confirmed across paper + docs + MS Research blog).
- **Benchmarks**: LLM-judged pairwise win-rate on *global sensemaking* questions —
  beats naive vector RAG on comprehensiveness/diversity (~72–83%), loses on
  directness (by design); root-level summaries are the cost sweet spot. Not a recall
  metric; not comparable to LongMemEval/LOCOMO.
- **The one piece FathomDB should borrow for the corpus lens**: the **hierarchical
  community-summary tier** — which is exactly the tier Zep *also* has (its community
  subgraph) and FathomDB has **not** built. It is the shared "global sensemaking"
  mechanism across both peers.

### 2.4 Where FathomDB sits

| Axis | Mem0 (OSS) | GraphRAG (MS) | Zep / Graphiti | **FathomDB** |
|---|---|---|---|---|
| Primary task | agent memory | **corpus sensemaking** | agent memory | both (memory + corpus) |
| Substrate | vector store (Qdrant) | parquet + in-mem NetworkX | Neo4j-class temporal KG | **embedded SQLite property-graph** |
| Fact location | NL fact in vector store | relationship edges (+ entity descriptions) | fact-on-EDGE | **fact-on-EDGE (`body`)** |
| Temporal | overwrite/delete; none | **none** (static corpus) | bi-temporal, invalidate-not-delete | **bi-temporal-aware, invalidate-not-accumulate** |
| Graph tiers | none (graph removed) | entity + **community** (Leiden) | episodic + semantic + community | **semantic only** (no episodic/community tier) |
| Provenance | memory→message | text-unit↔entity (local) | graph-native bidirectional | `source_id` column (scalar, **not on SearchHit**) |
| Retrieval | dense (+BM25+entity v3) | global map-reduce + local | cosine+BM25+BFS × 5 rerankers | dense + FTS RRF (+ young graph arm) |
| Construction | in-lib LLM | in-lib LLM (heavy index-time) | in-lib LLM | **BYO-LLM** (no LLM in engine) |
| Envelope | service | batch index, re-index on change | service / Neo4j | **≤1M, single-writer, embedded** |

**For the memory task that LongMemEval scores, the architecture FathomDB is
converging on is Zep's, not Mem0's or GraphRAG's** — fact-on-edge + bi-temporal +
hybrid-with-graph. It is *behind* Zep on tiering, fact-surfacing, and rerankers, and
*deliberately different* on footprint (embedded, BYO-LLM). It is *architecturally
ahead of* current Mem0-OSS on temporal modeling (Mem0 dropped its graph and never
had bi-temporal). **GraphRAG is the wrong tool for LongMemEval** (sensemaking, not
recall) — but it is the right reference for FathomDB's *corpus* lens, and its
**community-summary tier (shared with Zep) is the most valuable structure FathomDB
is currently missing** for global questions. The kernel of overlap across all three
peers + FathomDB is the entity/fact graph; the divergence is **temporal model**
(Zep/FathomDB yes, Mem0/GraphRAG no) and **task** (memory vs corpus).

---

## 3. The core question: basic graph ability but a muddled node/edge structure?

**Short answer: No.** The node/edge structure is clear, specified, and
HITL-signed across four ADRs and several design memos (fact-on-edge; bi-temporal;
`logical_id`-alone identity; invalidate-not-accumulate). FathomDB does not have a
"node-vs-edge confusion." The LME underperformance is real but its causes are
elsewhere — and they are separable.

### The empirical anchor (Slice 30, LME `s_cleaned`, 500 Q, 19,195 sessions)

| Class | FathomDB | NaiveRAG | Δ |
|---|---:|---:|---:|
| factoid | 0.4423 | 0.5385 | **−0.0962** |
| temporal | 0.0902 | 0.1504 | −0.0602 |
| knowledge_update | 0.3590 | 0.4231 | −0.0641 |
| multi_session | 0.1278 | 0.1128 | **+0.0150** |

Graph-arm effect vs the FTS+dense baseline: **≈ 0 on every class** (knowledge_update
−0.013 = RRF noise).

### Cause A — the graph arm is *invisible to the evaluator* (read-path contract gap) [CONFIRMED in code]

`SearchHit.id = write_cursor`, not `source_id` (`lib.rs:5459`). The LME scorer
matches retrieved hits to **pre-labeled `answer_session_id`s**; entity/graph hits
carry a row id that can never equal a session id, so they score as misses
regardless of how good the traversal is. The graph arm "contributes nothing"
because **its output identity does not carry source provenance to the scorer** —
a boundary contract that was never specified or tested against independent ground
truth until LME (the agent's root-cause is correct). *This is necessary to fix but
not sufficient* (see B).

### Cause B — ~1% extraction coverage starves the arm [CONFIRMED by arithmetic]

Only 200 of 19,195 sessions were ELPS-extracted (≈1%). For a multi-session
question with 2–3 gold sessions, P(all gold sessions extracted) ≈ (200/19195)² ≈
0.01%; BFS also needs the *reached* session to be extracted, compounding the
sparsity. Even with Cause A fixed, the expected lift at 200 sessions is ~**+0–1pp**,
below the noise floor. Coverage is an experiment-design parameter, not a code bug.

### Cause C — the base retrieval trails BM25 on conversational data [CONFIRMED by the table; biggest lever]

Before any graph, FathomDB's dense+FTS fusion is **−9.6pp on factoid**, −6pp
temporal, −6.4pp knowledge_update vs plain BM25. This is the largest gap and it is
**upstream of the graph entirely**. Candidate causes (to be isolated, §4): RRF
weight calibration (the documented FIX-5 architectural concern — two-arm fused
results re-enter `fuse_three_arms` as the "vector" arm at weight 1.0, losing the
3× text weight), conversational-text chunking, embedder fit (BGE-small on chat
turns), or query/answerer construction in the runner. Note the one class FathomDB's
**fusion already beats BM25** is `multi_session` (+1.5pp) — exactly the class graph
was meant to help — yet the graph arm added 0 there (Cause A masks it).

### A real *structural-completeness* gap (the kernel of truth in the question)

The model isn't muddled, but FathomDB's graph is **less complete than Zep's**, and
some of that bites LME:

1. **The graph arm surfaces entity-node bodies (names), not fact-edge bodies.** BFS
   fetches `canonical_nodes.body` (the entity *name*) as the candidate payload
   (`lib.rs:5416`); the *fact text* lives on `canonical_edges.body` and only enters
   search via the separate `text_edge` FTS branch, which is **filtered out of BFS
   seeding** (slice-30 design). Temporal / knowledge-update answers live in the
   *fact* + its valid-time — so the arm may be returning the wrong granularity.
   [HYPOTHESIS — verify in §4 E3]
2. **Seed-type mismatch.** The arm seeds BFS from the top-10 two-arm hits, which on
   this corpus are **document/chunk nodes**, not entity nodes; edges connect
   *entity* `logical_id`s, so a doc-node seed has no incident edges and BFS returns
   empty — a second, independent reason the arm is inert. [HYPOTHESIS — verify E0]
3. **No episodic-provenance tier and no community tier.** Zep's session→entity
   episodic edges *are* the multi_session mechanism, and the scalar `source_id`
   (not a graph tier, not on the SearchHit) is the thin version of it.
4. **No graph-aware reranker** (node-distance / episode-mentions) and the R1
   cross-encoder is not yet wired over the fused+graph set.

So the honest synthesis: **clear model, incomplete realization + a boundary bug +
a coverage confound + a base-retrieval deficit.** "Lacks a clear node/edge
structure" is the wrong diagnosis; "the graph arm is wired but inert, on top of a
base retriever that is itself behind BM25 here" is the right one.

---

## 4. Experiments to find the most valuable design choices

Use the two instruments already built: the **Slice-25 identical-answerer harness**
(`src/python/eval/r2_parity_eval.py`) and the **Slice-30 LME Option-3 runner**
(pre-labeled `answer_session_id`s — the *independent* ground truth that finally made
Cause A visible; **do not** revert to self-generated gold à la `gold_gen.py`, which
masked it). Methodology guardrails for every experiment: **identical answerer**
across systems; **independent pre-labeled gold**; report **per-class**; **change
one variable at a time**; separate **oracle extraction** (strong offline LLM) from
**local extraction** (the R3a vs R3b gate).

Ordered by value-per-cost:

**E0 — Fix-and-measure the graph arm (prerequisite; cheap).** Carry `source_id` on
the graph-arm `SearchHit` (or have the LME runner resolve `write_cursor → source_id`
via `canonical_nodes`/`canonical_edges`). Also instrument *what the BFS seeds and
reaches* (entity vs doc nodes; non-empty frontier rate) to confirm/refute
hypothesis §3-2. **Output:** the graph arm's *true* contribution at current
coverage (expected +0–1pp). Gate: arm is now "correct"; decide whether to keep it
on the board. **Mandatory — no other graph experiment is interpretable without it.**

**E1 — Coverage sweep.** Re-extract at 1k / 2k / 5k / full sessions; plot graph-arm
per-class lift vs coverage. **Isolates Cause B.** **Output:** the coverage at which
graph delivers material temporal/multi_hop/knowledge_update lift (or proof it never
does on this corpus). This is the single most decision-relevant graph experiment.

**E2 — Base-retrieval deficit diagnosis (highest absolute lever).** Ablate, against
LME with the graph arm OFF: (a) RRF weights / the FIX-5 cascade (does restoring the
3× text weight close the BM25 gap?); (b) chunking for conversational turns;
(c) embedder (BGE-small vs alternatives, mean-centering); (d) the runner's query +
answerer construction. **Output:** why dense+FTS trails BM25 by ~10pp on factoid,
and the fix. Likely worth more than the entire graph arm.

**E3 — Fact-surfacing A/B.** Compare graph-arm payload = entity-node *name* (today)
vs fact-edge *body + valid-time* surfaced into fusion/context. **Targets temporal /
knowledge_update.** **Output:** whether surfacing facts (not entity names) is what
makes the graph arm answer temporal questions — tests hypothesis §3-1.

**E4 — Temporal-mechanism vs extraction-quality split.** With **oracle** extraction
(strong offline LLM building the graph for the eval slice), measure
invalidate-not-accumulate end-to-end on knowledge_update questions; then repeat with
a **local ≤4B** extractor. **Output:** the R3a (mechanism) and R3b (local
construction) gate numbers from `IR-C-roadmap.md §C3` — GO only if temporal/
multi-hop/knowledge-update lift materially with factoid flat.

**E5 — Reranker ablation.** On the fused+graph candidate set: node-distance
reranker, RRF-weight tuning, and the R1 cross-encoder (Slice 10). **Output:**
whether Zep-style graph-aware reranking recovers lift that flat RRF leaves on the
table.

**Decision rule.** Spend in order E0 → E2 → E1 → E3 → E4 → E5: prove the arm is
*measurable* (E0), fix the *largest* gap first (E2, base retrieval), then decide
whether graph earns its keep (E1/E3/E4), then optimize (E5). The Slice-30
`use_graph_arm` default stays `false` and the HITL go/no-go stays **blocked** until
E0+E1 produce a real, non-data-limited per-class delta.

---

## 5. References

**FathomDB code:** `src/rust/crates/fathomdb-schema/src/lib.rs` (schema steps
2/8/12/14/15); `src/rust/crates/fathomdb-engine/src/lib.rs` (BFS `~5320-5480`,
`fuse_three_arms` `~4656`, ingest `~2590-2787`, projection `~8131-8269`,
`derive_logical_id` `~7711`); `src/python/fathomdb/types.py:50`.
**FathomDB docs:** `dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md`,
`-agent-memory-retrieval-and-identity.md`, `-graph-traversal-scope.md`,
`-canonical-identity-substrate.md`; `dev/adr/ADR-0.8.1-graph-substrate-g11-migration.md`;
`dev/design/0.8.0-agent-memory-fit.md`, `slice-15-design.md`, `slice-30-design.md`,
`slice-31-identity-rescope-design.md`; `dev/plans/runs/IR-C-roadmap.md` (R2/R3, C3).
**External — Mem0:** arXiv 2504.19413 (https://arxiv.org/html/2504.19413v1);
OSS https://github.com/mem0ai/mem0 ; migration https://docs.mem0.ai/migration/oss-v2-to-v3 .
**External — Zep/Graphiti:** arXiv 2501.13956 (https://arxiv.org/pdf/2501.13956);
https://github.com/getzep/graphiti ;
https://blog.getzep.com/how-do-you-search-a-knowledge-graph/ ;
https://blog.getzep.com/state-of-the-art-agent-memory/ .
**External — GraphRAG (Microsoft):** arXiv 2404.16130 (https://arxiv.org/abs/2404.16130);
https://github.com/microsoft/graphrag ; https://microsoft.github.io/graphrag/
(index/default_dataflow, query/global_search, query/local_search, query/drift_search) —
query-focused summarization over a static corpus; **no temporal model**; not evaluated
on LongMemEval/LOCOMO (different task).
**Benchmark disputes (read methodology):** https://github.com/getzep/zep-papers/issues/5 ;
https://blog.getzep.com/lies-damn-lies-statistics-is-mem0-really-sota-in-agent-memory/ —
all peer numbers are end-to-end LLM-judged QA, **not** first-stage recall, and are
not mutually comparable across answerer/judge/version.
