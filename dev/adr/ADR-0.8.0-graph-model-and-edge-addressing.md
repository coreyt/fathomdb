# ADR-0.8.0 — Graph model & edge addressing: one neutral substrate for corpus + memory

> **Status:** PROPOSED (design-ADR; Slice 32 deliverable). Read-only evaluation —
> no engine/schema change is proposed for 0.8.0. Resolves the *intended graph
> model* (model shape + edge addressing) so the deferred G4–G7 graph verbs build
> on a decided foundation. Does **not** re-open or re-edit the HITL-signed Slice 31
> identity decision (`logical_id`-alone); any recommended change is a future
> HITL-gated follow-up.
>
> **Decides:** the intended graph model is a **single ontology-neutral binary
> property-graph substrate** that first-classes **both** a corpus ontology
> (GraphRAG-shaped) and a memory ontology (Graphiti-shaped, fact-on-edge);
> addressing stays opaque-`logical_id` for 0.8.0 with hybrid natural-key upsert as
> a future ADR; and the fact-on-edge *enrichment* (edge text/embedding/valid-time/
> confidence + edge-projectability + edge-inclusive history) is **reserved-additive
> now** so the world-class memory ontology is never a reshape of the load-bearing
> tables.

---

## 1. Context — a local agent must store two different kinds of knowledge

A local-first agent (Memex, Hermes, OpenClaw and the broader class FathomDB
targets) accumulates two structurally different kinds of knowledge, and it must
hold both:

- **Corpus** — durable reference material the agent reads *from*: docs, policies,
  wiki pages, code, research papers, transcripts. It is mostly **static**, it is
  large, and its value is **retrieval and whole-corpus sense-making** ("summarize
  the research corpus", "what is our PTO policy", multi-hop "what connects A to
  C"). This is the **GraphRAG** problem (Microsoft GraphRAG, arXiv 2404.16130):
  entity/relationship extraction + hierarchical **community summaries** over the
  entity graph.

- **Memory** — lived, changing state the agent writes *about the world*: user
  preferences, project status, account facts, conversation-derived facts, tasks.
  It **changes**, it must be **time-aware** ("what did we know last week", "where
  did Corey work in March 2025"), it must **invalidate-not-delete** on
  contradiction, and every fact must trace to the conversation/episode that
  established it. This is the **Graphiti** problem (Zep/Graphiti, arXiv
  2501.13956): an entity graph whose **facts live on edges** carrying
  `valid_at`/`invalid_at`/`created_at`/`expired_at`, with raw **episodes** as the
  provenance ground-truth tier.

These two are **complementary, not competing** (a common production design runs
GraphRAG for published org knowledge *and* Graphiti for evolving agent memory,
routing ingestion and queries between them). An agent that does both needs a
store that can serve both. The question this ADR answers is: **what must FathomDB
*be* so a local agent can build that dual-graph router on top of it — and does
serving both halves force FathomDB to natively implement two graph engines?**

### 1.1 The reification taxonomy (stated precisely, to avoid a common conflation)

A "fact"/relationship can be reified three ways. Getting these distinct is
load-bearing for everything below:

| Shape | A "fact" IS… | Canonical system | n-ary? | Provenance |
|---|---|---|---|---|
| **(1) Binary edges** (FathomDB status quo) | a typed directed edge row; no body/valid-time | Neo4j default; Mem0ᵍ | binary | scalar/property |
| **(2) Temporal fact-EDGES** | **the edge itself**, enriched: `fact` text + four timestamps | **Graphiti/Zep** | binary triplet | **node-based, traversable** (episode tier) |
| **(3) Reified fact-NODES** | a **node** carrying text/valid-time/confidence/embedding + **n-ary roles**, with SUBJECT/OBJECT/ROLE edges | **HyperGraphRAG** (arXiv 2503.21322); research-note tradition; GraphRAG *claims/covariates* (optional, off by default) | **yes** | node + provenance edges |

> **The conflation to avoid.** Graphiti is **fact-on-EDGE**, not fact-on-node. Its
> *fact* is an edge property (binary triplet, two entity nodes); what it reifies
> as **nodes** is its **provenance/episode tier**, reached by traversable edges.
> Mainline GraphRAG's *relationships* are likewise **edges**; only its optional
> *claims/covariates* are node-like reified statements. The genuine fact-as-NODE
> tradition is **HyperGraphRAG** + research-notes, chosen specifically for **n-ary
> facts and per-fact embeddings** — capabilities binary edges genuinely struggle
> with. The shorthand "GraphRAG = fact on node, Graphiti = fact on edge" is a
> useful intuition pump but mis-assigns GraphRAG's relationship model; this ADR
> uses the precise three-way taxonomy.

---

## 2. The constraint — serving a *local* agent, *locally*

FathomDB is not a graph database in the Neo4j/distributed sense. It is the
**embedded substrate the named consumers otherwise hand-roll on raw SQLite**
(`dev/design/0.8.0-agent-memory-fit.md:362-365`: two of three named consumers run
on SQLite + sqlite-vec/FTS5 — the exact stack FathomDB *is*). That single fact is
the dominant architectural constraint, and it has hard consequences:

- **Single-writer, single-file, in-process.** One writer thread; op-store, FTS,
  and vector projections all derive from one canonical log
  (`ADR-0.6.0-single-writer-thread`, `ADR-0.6.0-projection-model`). Any "second
  engine" would have to honor the same single-writer/replay/projection
  invariants or forfeit them.
- **Brute-force vector + FTS, no ANN, no distribution.** `vector_default` is
  sqlite-vec with **filtered KNN** over partition columns
  (`source_type`/`kind`/`created_at`/`status` — `schema:lib.rs:209`,
  `engine:lib.rs:2715,3641-3700`); `search_index` is FTS5 (`schema` step-11).
- **A bounded record envelope.** The latency budget is **tiered**: ≤10k binding
  for 0.x/1.x, 100k/1M as post-1.0 ANN work
  (`memory/pr3-tiered-latency-budget.md`; AC-013/AC-019). Real-embedder N=1M is
  infeasible locally. The consumers are single-machine personal/agent stores, not
  warehouse graphs.
- **No deep-traversal / billion-edge / distributed workload exists in the
  consumer set.** Graph is **not a named 0.8.0 blocker**: OpenClaw graph =
  "none/markdown", and the docs explicitly warn *"do NOT let OpenClaw create
  sequencing pressure for graph features"*; only Mem0ᵍ/Zep-graph want graph at
  all, and shallowly (`agent-memory-fit:357-360,493-496`).

**Constraint, stated as a rule:** *FathomDB must serve both corpus and memory from
one embedded, single-writer SQLite substrate within a ≤100k–1M-record envelope.
Any design that requires a second physical engine, deep large-graph traversal, or
distribution is out of envelope by construction.*

---

## 3. Requirements

For a local agent to build a corpus+memory dual-graph router on FathomDB, the
substrate must provide — **as mechanisms, not policy**:

- **R1 — Store both ontologies on one substrate.** Entity nodes, typed
  relationship edges, community-summary nodes (corpus), episode nodes, and
  fact-edges (memory) must all be representable in `canonical_nodes`/
  `canonical_edges`.
- **R2 — Discriminate corpus vs memory cheaply.** The router must scope reads to
  one ontology in a single statement (e.g. `kind`-partitioned filtered KNN /
  list / traversal), with no cross-contamination.
- **R3 — Stable identity + idempotent re-ingestion.** `logical_id`-alone identity,
  one active row per id (signed Slice 31). *Settled — not re-opened here.*
- **R4 — Invalidate-not-delete supersession.** Contradiction handled by
  tombstoning the prior version, retaining history (`superseded_at` today;
  bi-temporal end-state per retrieval-ADR Option 2A `:135-159,177-196`).
- **R5 — Temporal validity on facts.** `valid_at`/`invalid_at` so "what did we
  know last week" / point-in-time is expressible. *(memory half)*
- **R6 — Provenance, both cheap and traversable.** Scalar `source_id` for the
  common case; optional graph-traversable `SUPPORTS`/`OBSERVED_IN`/`MENTIONS`
  edges + an episode tier for lineage queries.
- **R7 — Hybrid retrieval + bounded traversal read verbs.** G4 `list`, G5
  `neighbors(id, edge_type?, depth)`, G6 `search(expand=)`, G7 `history(id)` —
  the shared retrieve-and-expand-and-compare surface both ontologies use.
- **R8 — Per-fact semantic search.** A fact (corpus relationship or memory
  fact-edge) should be embeddable/FTS-able, not only traversable.
- **R9 — Ontology-neutrality.** The engine must commit to neither shape; it
  exposes mechanisms and lets the consumer's router own routing/precedence/
  reconciliation **policy**. (Baking policy in fits one consumer and breaks the
  rest — the same reason FathomDB does not bake in OpenClaw's MMR rerank.)

---

## 4. Reasoning

### 4.1 Decompose each half to physical operations

The only way to know whether two ontologies force two engines is to walk each
down to the physical operations FathomDB executes and check for collision.

**Corpus (GraphRAG) half:**

| Operation | FathomDB physical op | Native today? |
|---|---|---|
| Entity node + description + embedding | `canonical_node(kind="entity")` + vector row | ✅ |
| Relationship edge | `canonical_edge(kind="works_at")` | ✅ |
| Community-summary node (hierarchical) | `canonical_node(kind="community_summary")` + member edges | ✅ representable (`agent-memory-fit:395-396`) |
| Local search (entity→neighbors→text) | filtered-KNN + G5 CTE expand + FTS = **G6** | ✅ (deferred verb) |
| Global search (map-reduce over summaries) | `list(kind="community_summary")` + **app-side** LLM reduce | ✅ retrieval-side; reduce is app |
| Claims/covariates (optional) | `canonical_node(kind="claim")` | ✅ representable |

**Memory (Graphiti) half:**

| Operation | FathomDB physical op | Native today? |
|---|---|---|
| Episodic node (raw event, ground truth) | `canonical_node(kind="episode")` | ✅ |
| Entity node | `canonical_node(kind="entity")` | ✅ |
| Fact-edge (binary triplet + `fact` text) | `canonical_edge` + **text on edge** | ⚠️ edge has **no body column** |
| Valid-time on fact-edge | edge `valid_at`/`invalid_at` | ⚠️ **not present** (reserved-additive) |
| Confidence on fact-edge | edge `confidence` | ⚠️ not present (v0.5.6 *had* it) |
| Invalidate-not-delete | supersession via `superseded_at` | ✅ (transaction-time half) |
| Provenance: fact→episode (traversable) | `MENTIONS`/`SUPPORTS` edge **or** scalar `source_id` | ✅ both representable |
| Point-in-time recall | filter on edge valid-time | ⚠️ needs valid-time columns |
| Per-fact semantic search | vector/FTS keyed to the fact | ⚠️ **vectors/FTS project nodes, not edges** |

### 4.2 Where "one substrate" holds, and where it cracks

**Structure and traversal reduce to one substrate.** Both halves are
entity-nodes + typed-edges + (community / episode) nodes, walked by the **same**
recursive CTE over `from_id`/`to_id`
(`dev/design/agent-memory-impl-strategy.md:326-329`), retrieved by the **same**
filtered KNN + FTS, superseded by the **same** `logical_id`/`superseded_at`
mechanism. The two graphs differ in **ontology and policy**, not in physical
access pattern.

**It cracks in exactly three narrow places — all on fact-on-EDGE enrichment:**

1. **Edges have no `body`** → a fact-edge's `fact` text and embedding have nowhere
   to live (current edge = `{kind, from_id, to_id, source_id, logical_id,
   superseded_at}`, `schema:lib.rs:111`+step-8/12).
2. **Edges have no valid-time/confidence** → point-in-time and confidence-weighted
   recall aren't expressible.
3. **Per-fact semantic search** → only *node* `body` is projected to vector/FTS;
   an edge cannot be embedded today.

Note v0.5.6 edges already carried `properties BLOB` + `confidence REAL` + dual
kind-scoped traversal indexes under `logical_id`-alone identity
(`git show v0.5.6:crates/fathomdb-schema/src/bootstrap.rs:30-51`;
`dev/profiling/v05-lineage.md:14-48`) — so fact-on-edge enrichment is **proven
portable in this codebase**, and re-adding it is additive, not a reshape
(retrieval-ADR Option 2A explicitly certifies the bi-temporal column shape as
additive, `:155-159`).

### 4.3 Does *performance* force native support for both graph structures?

A separate native engine wins only when its **physical storage layout** is
structurally better for a workload the shared layout handles badly. Test each
router workload against FathomDB's envelope (§2):

- **W1 — Corpus global sense-making (community summaries).** Bulk `kind`-indexed
  node scan + **app-side** LLM reduce. Even native GraphRAG reduces in the
  application, not the graph engine. **No native engine helps. No divergence.**
- **W2 — Memory point-in-time ("what did we know last week").** Indexed valid-time
  range scan over fact-edges — *the same access pattern a native temporal KG
  uses* (Graphiti-on-Neo4j is indexed valid-time property filters; there is no
  special temporal storage engine). **No divergence — once valid-time columns
  exist (the reserved-additive item).**
- **W3 — Multi-hop traversal (both halves).** Clamped depth-≤3 recursive CTE over
  indexed endpoint columns. SQLite CTEs are competitive with native graph engines
  until **deep (>4-hop) traversal over millions of edges** — which **no named
  consumer has** and which is **out of envelope** (§2). A native property-graph
  engine earns its keep precisely in that out-of-envelope regime. **No divergence
  in-envelope.**
- **W4 — Per-fact semantic search (the genuinely hard one).** To make a fact-edge
  semantically searchable you either (a) **project edge text** into the same
  `vector_default`/FTS with a `source_type="edge_fact"` partition — *one filtered
  index, same engine* — or (b) **reify the fact as a node** (`kind="fact"`) so it
  projects through the existing node path for free. Option (b) is the
  fact-on-NODE escape hatch, and this is exactly where it earns its keep (n-ary +
  per-fact embedding, the HyperGraphRAG rationale). **Both are single-engine. No
  divergence; where fact-embedding pressure is highest, the answer is the already-
  documented escape hatch — still on one substrate.**

**The asymptotic check.** The regimes where two native engines beat one
well-indexed SQLite substrate are: deep large-graph traversal (native adjacency),
billion-edge analytics, and distributed graph. **All three are out of FathomDB's
embedded, single-writer, ≤100k–1M envelope by construction.** None of the dual-
graph router's workloads fall into them. The router's two "graphs" differ in
**ontology and policy**, not **physical access pattern** — and physical access
pattern is the only thing that would justify native divergence.

**A second native engine would buy nothing the consumer can measure, while
costing FathomDB its single-substrate determinism, its single-writer/replay/
projection invariants, and a second engine to keep world-class. That trade is
strictly negative.**

### 4.4 Identity vs addressing (the open crux, kept separate from identity)

Edge **identity** is settled: `logical_id`-alone, both tables (Decision 5,
HITL-SIGNED; `ADR-0.8.0-canonical-identity-substrate.md:188-231`). Multigraph is
**already representable** — distinct typed edges between a `(from,to)` pair each
carry their own `logical_id`; `kind` is never identity. Decision 5's "edge `kind`
buys no real capability" is **correct about identity** and **silent about
traversal/addressing**, where `kind` *does* carry weight (it is the G5
`edge_type` filter and the natural relationship-type key a hybrid upsert would
dedup on). This ADR does not touch identity.

What is **open** is **addressing**: to supersede "*the `friend` edge from A to
B*," must a consumer mint+track the opaque `logical_id` (status quo), address by
the natural `(from,to,kind)` tuple, or a **hybrid** (engine derives/dedups on the
tuple for upsert ergonomics but still stores a `logical_id` handle)? The
Neo4j norm is pattern-`MERGE` on `(start,type,end)` with relationships still
carrying their own identity — i.e. **hybrid**. For 0.8.0, opaque-id is sufficient
and blocks nothing; hybrid is the right *future* write-ergonomics layer and must
**never** become identity (tuple-as-identity would fork/collapse distinct facts
sharing a tuple).

---

## 5. Decision / Recommendation

**The intended graph model is a single ontology-neutral binary property-graph
substrate that first-classes BOTH a corpus ontology (GraphRAG-shaped) and a
memory ontology (Graphiti-shaped, fact-on-edge). FathomDB does NOT natively
implement two graph engines.** The two graphs of a dual-graph router are an
**ontology + policy** distinction that lives in the **consumer**; they dissolve to
one physical substrate within FathomDB's embedded single-writer envelope.

Concretely:

- **Model shape.** The recommended/documented end-state ontology for the **memory**
  half is **temporal fact-EDGES** (Graphiti-shaped: `fact` text + valid-time +
  confidence on the edge, episodes as provenance) — matching the docs' own Option
  2A and v0.5.6 precedent. The **corpus** half is the GraphRAG-shaped
  entity/relationship + community-summary ontology — *equally supported* on the
  same neutral substrate. **Reified fact-NODES** stay a **documented escape
  hatch** for the n-ary / heavy-per-fact-embedding case (representable today, no
  schema change). The **engine commits to none of these** — neutrality (R9) is the
  load-bearing property that lets one engine serve a dual-graph router.

- **Addressing.** Keep **opaque-`logical_id`-only for 0.8.0** (sufficient, signed,
  blocks nothing). Document **hybrid `(from,to,kind)` upsert as the intended
  future write-ergonomics ADR**, never as identity.

### What to RESERVE NOW (the only substrate-now action — additive, ~zero migration)

Because re-migrating the load-bearing `canonical_nodes`/`canonical_edges` post-
release is the expensive, hard-to-reverse mistake, the world-class move is to
**reserve the fact-on-edge enrichment now so it is forever additive**:

1. **Reserve edge-enrichment columns** (`body`/`text`, `valid_at`/`invalid_at`,
   `confidence`) as additive-now in the substrate ADR's data model — a **prose
   reservation**, not a column. Option 2A already certifies additivity; v0.5.6
   proves portability.
2. **First-class edge-projectability as an intended capability** — fact-edges must
   be embeddable/FTS-able for per-fact semantic recall (R8). Design the projection
   seam to admit an edge source even if 0.8.0 ships node-only projection.
3. **Resolve G7 `history` to include edges** — the memory half's temporal-
   comparison workload requires it. (This flips the earlier "nodes-first" lean:
   edges are in-scope for the memory ontology.)
4. **Keep fact-on-node as the documented n-ary escape hatch** — representable
   today, no schema change; the W4 answer where reification genuinely helps.
5. **State ontology-neutrality (R9) as the load-bearing substrate property.**

### What to DEFER to 0.8.x

Edge enrichment columns (ship with valid-time/G11); hybrid `(from,to,kind)` upsert
(future write-API ADR); traversable provenance edges + episode tier (ontology
recommendation); reified fact-nodes (escape hatch, on n-ary demand); and the
G4/G5/G6/G7 verbs themselves (already deferred, `roadmap/0.8.0.md:143-146`).

### What the deferred G4–G7 verbs should assume

`logical_id`-alone identity; `kind` as the `edge_type` traversal filter (the
landed `canonical_edges(from_id)/(to_id)` indexes already serve it, exactly as
v0.5.6's `(source_logical_id, kind, superseded_at)` did); opaque-id addressing
(hybrid may add MERGE ergonomics later without changing these verbs); and G7
covers edges for the memory ontology.

### HITL product-decision points (the human's call; recommended defaults in bold)

| # | Decision | Options | Default |
|---|---|---|---|
| H1 | Intended model | binary / **ontology-neutral substrate first-classing BOTH corpus + memory, fact-on-edge recommended for memory** / fact-node | **neutral-both** |
| H2 | Addressing for 0.8.0 | **opaque-id** / natural-key / hybrid-later | **opaque-id now; hybrid future ADR** |
| H3 | Reserve edge-enrichment now? | **prose-reserve** / say nothing | **prose-reserve** (zero-cost, prevents reshape) |
| H4 | Provenance | **scalar now; traversable later** / traversable now | **scalar now** |
| H5 | G7 history scope | nodes-only / **nodes + edges** | **nodes + edges** (dual-graph needs it) |
| H6 | Adopt fact-nodes now? | adopt / **escape hatch** | **escape hatch** |

None of H1–H6 changes signed Slice 31; only H3 has a (prose-only) substrate-now
footprint.

---

## 6. Consequences

- **For 0.8.0:** the binary substrate ships **unchanged**; the entire richer end-
  state is additive-later; the only action is a zero-cost prose reservation +
  documenting the dual-ontology intent and ontology-neutrality.
- **For the consumer:** FathomDB can serve a corpus+memory dual-graph router as the
  single substrate beneath it — supplying `kind`-scoping, filtered KNN, valid-time,
  supersession, provenance, and G4–G7 — while the router (precedence,
  reconciliation, "store as 'user believes X'", never-overwrite-static) stays
  consumer policy.
- **Avoided:** a second graph engine (forfeits invariants for unmeasurable gain),
  and a second migration over the load-bearing tables (reserved-additive instead).

---

## 7. Sources

Repo: `ADR-0.8.0-agent-memory-retrieval-and-identity.md:126-196` (Option 2A,
bi-temporal fact-edges); `ADR-0.8.0-canonical-identity-substrate.md:188-231`
(Decision 5); `dev/design/0.8.0-agent-memory-fit.md:60-115,218-228,340-503`;
`dev/design/agent-memory-impl-strategy.md:318-333`;
`dev/profiling/v05-lineage.md:14-48`; `git show v0.5.6:.../bootstrap.rs` &
`.../writer/mod.rs`; current `src/rust/crates/fathomdb-schema/src/lib.rs:111,
155-314`, `src/rust/crates/fathomdb-engine/src/lib.rs:2715,3641-3700,6036`;
`dev/roadmap/0.8.0.md:128-146`; `memory/pr3-tiered-latency-budget.md`.

External: Microsoft GraphRAG (arXiv 2404.16130; entity/relationship + community
summaries; optional claims/covariates) ·
<https://microsoft.github.io/graphrag/index/default_dataflow/>. Zep/Graphiti
(arXiv 2501.13956; fact-on-edge, four timestamps, episodes as provenance) ·
<https://help.getzep.com/graphiti/getting-started/overview>. HyperGraphRAG
(arXiv 2503.21322; n-ary hyperedges, why binary edges struggle). Mem0
(arXiv 2504.19413; entities=nodes, typed relationships=edges). Neo4j MERGE
(relationship identity+type; pattern-MERGE) ·
<https://neo4j.com/docs/cypher-manual/current/clauses/merge/>.
