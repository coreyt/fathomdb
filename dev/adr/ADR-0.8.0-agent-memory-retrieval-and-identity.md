---
title: ADR-0.8.0-agent-memory-retrieval-and-identity
date: 2026-05-31
target_release: 0.8.0
desc: Input to 0.8.0 planning. (1) Reclassify hybrid fusion+rerank (G9) and vector metadata columns (G10) from "differentiating" to table-stakes for the named consumer class. (2) Direct the deferred canonical-identity substrate (G0) to be designed bi-temporal-aware (G11) so supersession is not built twice.
blast_radius: dev/roadmap/0.8.0.md (knowledge-store + identity scope); src/rust/crates/fathomdb-engine/src/lib.rs (search fusion path `read_search_in_tx`; vector partition `vector_default`); src/rust/crates/fathomdb-schema/src/lib.rs (canonical-identity substrate migration; vector metadata columns); dev/design/0.8.0-agent-memory-fit.md (source analysis §4/§8); ADR-0.8.0-canonical-identity-substrate (to be drafted — this ADR constrains its shape); AC-057a / dev/design/bindings.md § 1 (five-verb invariant — read-verb question)
status: accepted (HITL 2026-06-02) — see "## HITL decisions (2026-06-02)"
origin: dev/design/0.8.0-agent-memory-fit.md §8 (external validation: shipping peers + agent-memory literature)
---

# ADR-0.8.0 — Agent-memory retrieval quality + bi-temporal-aware identity

**Status:** draft, HITL-required.

This ADR is **input to 0.8.0 planning**, not a self-contained feature spec. It
asks HITL to settle two scoping questions before the 0.8.0 knowledge-store and
canonical-identity work is detailed:

1. **Retrieval-quality reclassification.** Should **hybrid fusion + rerank (G9)**
   and **vector metadata columns for filtered KNN (G10)** be treated as
   **table-stakes** for FathomDB's named consumer class, rather than as
   later-cycle "differentiating" features?
2. **Identity-substrate shape.** Should the canonical-identity substrate already
   scoped for 0.8.0 (`dev/roadmap/0.8.0.md:84-92`, the "G0" item) be designed
   **bi-temporal-aware (G11)** from the start — so that fact/edge supersession and
   point-in-time validity are not re-engineered in a later release?

Gap labels (G0, G9, G10, G11, …) are defined in
[`dev/design/0.8.0-agent-memory-fit.md`](../design/0.8.0-agent-memory-fit.md) §4 and §8c.

## Context

The 0.6.0/0.7.0 rewrite deliberately scoped FathomDB as a **retrieval/index
engine** with a locked five-verb SDK surface (AC-057a; `dev/design/bindings.md`
§ 1). That scoping was correct *sequencing*: get the SQLite + FTS5 + sqlite-vec
substrate, durability, and performance right first. This ADR does not reopen that
decision. It records evidence that two specific retrieval-quality capabilities
sit **below the current floor** for the consumers FathomDB is explicitly being
built for, and that the already-planned identity work has a one-time design
window to avoid a costly rebuild.

### Evidence base

Two independent investigations, recorded in `dev/design/0.8.0-agent-memory-fit.md`:

- **Named consumers are real, public, local-first agent-memory products** —
  Memex, **Hermes Agent** (Nous Research, OSS, Feb 2026), **OpenClaw Agent** —
  not internal projects. **Two of the three run on SQLite + sqlite-vec / FTS5**,
  the exact substrate FathomDB *is* (`0.8.0-agent-memory-fit.md` §8a).
- **A verified deep-research pass** over the agent-memory literature (Zep/Graphiti
  arXiv 2501.13956; Mem0 + arXiv 2504.19413; Microsoft GraphRAG arXiv 2404.16130;
  Zhang et al. agent-memory survey arXiv 2404.13501; Generative Agents; sqlite-vec
  docs; Azure AI Search hybrid-ranking docs). 25/25 extracted claims confirmed
  under 3-vote adversarial verification (`0.8.0-agent-memory-fit.md` §8b/§8d).

### What "below the floor" means concretely

| Capability | Shipping peers today | FathomDB 0.7.2 today |
|---|---|---|
| Hybrid retrieval **scoring** | RRF fusion (`Σ 1/(rank+k)`, k≈60) of vector+text(+graph), then MMR / cross-encoder rerank (Zep, Mem0, OpenClaw, Azure) | **scoreless union, dedup-by-`body`** (`read_search_in_tx`, `lib.rs:3130-3245`) — vector hits concatenated before text hits, no fused score, no rerank |
| **Filtered** vector search | metadata-constrained KNN in one statement (sqlite-vec metadata columns; OpenClaw, Mem0) | none — `vector_default` carries `source_type`/`kind` but `search()` exposes no filter predicate |
| Record identity / by-id | by-id read is a base verb (OpenClaw `memory_get`, Mem0 `get`) | no `logical_id`, no per-row id in the receipt, no `get` (already the 0.8.0 anchor) |

The first two rows are the subject of this ADR's question 1. The third (G0) is
already planned; question 2 is about *how* it is designed.

## Question 1 — reclassify G9 + G10 as table-stakes

`0.8.0-agent-memory-fit.md` §8d currently ranks capabilities table-stakes /
differentiating / world-class. The research moved two items the original §4
analysis had under-weighted:

- **G9 — hybrid fusion + rerank.** Every surveyed shipping peer fuses ranked
  lists (RRF) and reranks (MMR for diversity, optionally a cross-encoder or
  recency/importance reweight per Generative Agents). FathomDB's union-dedup is
  *neither* — it cannot express "this vector hit and this text hit agree, rank it
  higher," which is the entire point of hybrid retrieval. This is an **internal
  upgrade to `search()`** (the `read_search_in_tx` fusion step) and **needs no new
  SDK verb** — it does not touch AC-057a.
- **G10 — vector metadata columns.** sqlite-vec supports metadata/partition
  columns and single-statement filtered KNN *today* (verified: v0.1.6, Nov 2024).
  Filtered retrieval ("semantic search but only `kind:"action_item"` where
  `status="open"`") is table-stakes for an agent world model. This is an **engine
  schema change** on `vector_default` plus a filter argument on the search path;
  it pairs with G4 (`list` + filter) but is independently useful.

### Options

- **Option 1A — Reclassify both G9 and G10 as table-stakes; schedule in 0.8.0
  alongside the knowledge-store work.** (Recommended.)
  - *For:* matches the demonstrated consumer floor; G9 needs no surface change and
    no invariant decision; G10 rides on a capability the embedded vector engine
    already has. Both materially improve retrieval quality for *every* consumer,
    not just graph users.
  - *Against:* adds engine work to a release already anchored on identity +
    knowledge-store; G9 changes observable ranking (a behavior-compat event for
    existing 0.6.x/0.7.x search results).
- **Option 1B — G9 table-stakes, G10 differentiating.** Ship fusion+rerank in
  0.8.0; defer filtered KNN.
  - *For:* fusion is pure-internal and lowest-risk; filtering can wait for the
    `list`/filter-grammar decision (question 3, below) to settle.
  - *Against:* filtered semantic retrieval is arguably the *more* consumer-visible
    of the two; deferring it leaves a named-consumer gap open another cycle.
- **Option 1C — Keep both as later-cycle differentiating (status quo of §4).**
  - *For:* protects 0.8.0 focus; treats retrieval quality as a 0.9.x concern.
  - *Against:* contradicts the evidence that peers ship these now; risks FathomDB
    being below-floor for its own reference consumers at 0.8.0 GA.

### Recommendation (question 1)

**Option 1A.** Reclassify G9 and G10 as table-stakes and fold them into 0.8.0
retrieval scope. G9 is the higher priority (it improves all retrieval, costs no
surface change, and is the difference between "hybrid" meaning a real thing vs a
union). G10 is the natural partner and uses an existing sqlite-vec capability.

Two guardrails if 1A is accepted:

- **Behavior-compat:** the fusion change alters search result *ordering*. Treat it
  as a deliberate, documented ranking change at 0.8.0 (release-note + an
  acceptance test pinning the RRF contract), not a silent drift. Consider a
  config knob only if a consumer needs the old ordering.
- **No DSL creep:** rerank signals (recency/importance) require per-record
  timestamps (G12) and are a *separate* increment; G9's first cut is RRF over the
  two existing branches with the standard `k≈60`, nothing learned.

## Question 2 — design G0 (identity substrate) bi-temporal-aware

`dev/roadmap/0.8.0.md:84-92` already scopes the canonical-identity substrate:
additive `logical_id`, `superseded_at`, partial unique index on `logical_id`
(scoped to `logical_id` ALONE — see substrate ADR **Decision 5**, HITL-SIGNED
2026-06-05; `kind` is payload/classification, not identity) excluding superseded
rows, writer takes `logical_id`, supersession writes `superseded_at` on the prior
row in-txn. A separate `ADR-0.8.0-canonical-identity-substrate` is to be drafted.

The research (`0.8.0-agent-memory-fit.md` §8b, Pillar 3) shows the **world-class
longitudinal-understanding mechanism is a bi-temporal model**: facts/edges carry
**four timestamps** — system *created/expired* (transaction time) and real-world
*valid/invalid* (valid time) — and contradictions are handled by **invalidating,
not deleting** (set the superseded edge's `t_invalid` to the new edge's
`t_valid`). This is the reference design under Zep/Graphiti and is exactly the
"entity resolution across re-ingestion + conflict resolution" requirement.

The risk: a single-timestamp `superseded_at` substrate is a **subset** of the
bi-temporal model. If 0.8.0 ships single-supersession and a later release needs
valid-time, that is a second schema migration over the same load-bearing tables
plus a second writer-path change — the precise "implement supersession twice"
outcome `0.8.0.md:77-79` says the substrate consolidation was meant to avoid.

### Options

- **Option 2A — Design bi-temporal-aware now; ship the minimal subset.**
  (Recommended.) The `ADR-0.8.0-canonical-identity-substrate` reserves the
  bi-temporal column shape (transaction-time + valid-time) and the
  invalidate-not-delete semantics in its data model, even if 0.8.0 *implements*
  only single-supersession (`superseded_at` ≈ transaction-time expiry). The
  schema and writer contract are chosen so adding valid-time later is additive,
  not a reshape.
  - *For:* one design pass; avoids a second migration over `canonical_nodes` /
    `canonical_edges`; aligns the substrate with the proven world-model shape;
    keeps 0.8.0 implementation scope small.
  - *Against:* requires the substrate ADR to reason about valid-time before there
    is a consumer asking for point-in-time queries; modest up-front design cost.
- **Option 2B — Ship single-supersession only; treat bi-temporal as a future,
  separate substrate.**
  - *For:* smallest possible 0.8.0 design surface; defers complexity until a
    consumer demands valid-time.
  - *Against:* high probability of a second schema migration + writer change later
    (the explicit anti-goal); two binding/substrate-coordination events instead of
    one.
- **Option 2C — Implement full bi-temporal in 0.8.0.**
  - *For:* matches world-class end-state immediately.
  - *Against:* over-commits a release already carrying identity + knowledge-store +
    retrieval; valid-time has no validated 0.8.0 consumer yet; violates
    single-load-bearing-change-per-release prudence (cf.
    `ADR-0.7.0-ac020-architectural-lever` § stop-rule).

### Recommendation (question 2)

**Option 2A.** Direct the forthcoming `ADR-0.8.0-canonical-identity-substrate` to
**design for the bi-temporal end-state and implement the minimal subset.**
Concretely, that ADR should settle:

1. column shape that admits both transaction-time (`created`/`expired` ≈
   `superseded_at`) and a later additive valid-time (`t_valid`/`t_invalid`)
   without reshaping existing columns;
2. that supersession is **invalidate-not-delete** (tombstone via timestamp, prior
   row retained) — already implied by `superseded_at`, made explicit and
   bi-temporal-compatible;
3. whether edges (not just nodes) carry identity + temporal columns, since the
   world-model graph (G5/G11) puts the temporal validity on **fact edges**, not
   only nodes;
4. the op-store cascade contract under supersession (already named in
   `0.8.0.md:89-90`), extended to the invalidate-not-delete semantics.

This ADR does **not** ask 0.8.0 to implement valid-time, graph traversal (G5), or
edge invalidation (G11 full) — only to not foreclose them.

> **Cross-ref (Slice 32):** the *intended graph model* that this Option 2A reserves
> — fact-on-edge enrichment (valid-time/body/confidence on edges), edge
> projectability, and edge-inclusive history — is resolved in
> [`ADR-0.8.0-graph-model-and-edge-addressing.md`](./ADR-0.8.0-graph-model-and-edge-addressing.md),
> which concludes a **single ontology-neutral substrate** serves both the corpus
> (GraphRAG) and memory (Graphiti) ontologies and that these columns are
> **reserved-additive**, never a reshape.

## Relationship to the five-verb invariant (AC-057a)

- **G9 (fusion+rerank)** is internal to `search()`; **no surface change**.
- **G10 (vector metadata filter)** can be exposed as an argument to the existing
  `search` verb, or held until the `list`/filter-grammar decision; either way it
  is a parameter question, not a new top-level verb.
- **G0 (identity)** is a write-path + schema change; by-id **read** verbs (G2) are
  a *separate* AC-057a question (already open as `0.8.0-agent-memory-fit.md` §7 Q1) and
  are **out of scope for this ADR**.

This ADR therefore does **not** require relaxing AC-057a. The read-verb question
remains where `dev/design/0.8.0-agent-memory-fit.md` §7 leaves it.

> **Scope reclass (v0.5.x triage, 2026-06-01 — `0.8.0-v05-feature-triage.md`):**
> the read-verb question is now answered by
> `ADR-0.8.0-supersede-five-verb-surface-cap` ("yes, under governance"). Net
> reclass: **G1 / G2 / G3-read / G8 → ADD-0.8.0** under the governed surface;
> **G5 / G6 / G4 / G9-confidence → DEFER-0.8.x**; the v0.5.x "grouped query"
> resolves to the **G6 retrieve+expand** fan-out (not statistical aggregation,
> which stays app-side); valid-time edges (G11 full) remain design-now /
> implement-0.9.x+. Consumer-importance corrections: **OpenClaw is NICE (not
> important) for all graph features** (no graph); **G0 transaction-time identity
> is table-stakes** (keystone), only valid-time is differentiating.

## Open questions for HITL

1. Accept Option 1A (G9 + G10 table-stakes in 0.8.0), 1B (G9 only), or 1C
   (status quo)?
2. Accept Option 2A (bi-temporal-aware design, minimal implementation) as a
   binding constraint on `ADR-0.8.0-canonical-identity-substrate`?
3. Is the 0.8.0 search-ranking change (G9) an acceptable, documented
   behavior-compat event, or must the legacy union ordering remain available
   behind a knob?
4. Do edges carry identity + temporal columns in the 0.8.0 substrate, or nodes
   only (deferring edge-temporal to the graph-traversal cycle)?
5. Should `dev/design/0.8.0-agent-memory-fit.md` §8d's table-stakes/differentiating/
   world-class ranking become the canonical 0.8.0-planning capability ladder, or
   is it advisory input only?

## HITL decisions (2026-06-02)

All five open questions are **signed**. This section is authoritative where it
differs from the Options/Recommendation prose above.

1. **Q1 = Option 1A.** G9 (RRF fusion + rerank hook) **and** G10 (metadata-filtered
   KNN) are reclassified **table-stakes** and both ship in **0.8.0 Slice 10**. G10
   uses a **closed `SearchFilter{source_type, kind, created_after, status}` struct**,
   **not** an open filter DSL — the filter-grammar/G4 ADR stays a **Slice 35** concern.
2. **Q2 = Option 2A.** The canonical-identity substrate (Slice 15) is **designed
   bi-temporal-aware** (column shape + invalidate-not-delete reserved so valid-time is
   additive later) but **implements only single-supersession** (`superseded_at`) in
   0.8.0. No valid-time, no graph traversal (G5), no edge invalidation (G11-full) in 0.8.0.
3. **Q3 = documented-only, NO knob.** RRF is the **unconditional** new ranking; the
   ordering change is a **documented behavior-compat event** (0.8.0 release note) pinned
   by an acceptance test (`pr_g9_rrf_fusion.rs`, determinism). The earlier "consider a
   `fusion_mode` knob" hedge is **rejected** — HITL: *"do not worry about migration; do
   not carry the overhead."* No legacy-union-ordering code path is retained. (The Slice 10
   contract in `dev/plans/0.8.0-implementation.md` has been reconciled to drop the knob.)
4. **Q4 = edges too.** Both `canonical_nodes` **and** `canonical_edges` carry
   `logical_id` + `superseded_at` in the 0.8.0 substrate (matches the substrate ADR's
   authorized delta). 0.8.0 implements **transaction-time supersession on edges as
   well as nodes** (tombstone-then-insert in `commit_batch`, keyed by `logical_id`
   alone — substrate ADR Decisions 3 + 5, pinned by `pr_g0_identity.rs`
   `s15_edge_supersession_*` + `s31_edge_kind_change_reingest_supersedes`). What is
   **reserved** (not done in 0.8.0) is edge *valid-time* invalidation (G11 full) —
   the `t_valid`/`t_invalid` column pair. (Corrected 2026-06-05: the earlier
   "schema-only / node-only behavior" wording understated the code, which DOES
   supersede edges.)
5. **Q5 = advisory.** §8d remains research-backed **advisory input**; the implementation
   plan + ADRs stay the authoritative scope (no second source of truth to keep in lockstep).

> **Gating:** Q1 + Q3 gated **Slice 10** (now cleared → Slice 10 may be prompted). Q2 + Q4
> feed the **substrate gate package** that gates **Slice 15** (the keystone); the remainder
> of that package — op-store cascade-under-supersession contract, forward-migration policy,
> and the FLAGGED `write_cursor`-as-row-id deviation — is **still open** and must be signed
> before Slice 15 spawns.

## Consequences if accepted (1A + 2A)

- 0.8.0 retrieval scope gains G9 (RRF fusion + rerank hook) and G10 (vector
  metadata + filtered KNN). `dev/roadmap/0.8.0.md` "Knowledge-store + retrieval
  anchor" § retrieval-verbs line is amended from "semantic + structured filter +
  rank fusion" to explicitly include **RRF fusion and a rerank hook over the
  vector+text branches** and **metadata-filtered vector search**.
- `ADR-0.8.0-canonical-identity-substrate` inherits a bi-temporal-aware design
  constraint and an explicit invalidate-not-delete semantic.
- `0.8.0-agent-memory-fit.md` §8d is promoted (or re-marked advisory per Q5) and G9/G10
  move to the table-stakes tier in that doc.
- AC-057a is untouched; the by-id read-verb decision stays open and separate.

## Promotion threshold / non-goals

- This ADR settles **classification and design-constraint** questions only. The
  concrete schema, RRF parameters, filter grammar, and acceptance bars are owned
  by 0.8.0 planning + the substrate ADR.
- **Not in scope for *this* ADR** (owned elsewhere, not "blocked"): the read-verb
  surface — by-id (G2), graph traversal (G5), retrieve-then-expand (G6),
  `read.list`/`read.history` (G4/G7) — is **governed under
  `ADR-0.8.0-supersede-five-verb-surface-cap`** (the five-verb cap is superseded;
  reads are additive under governance, not relaxation-of-an-invariant). Per the
  v0.5.x triage, **G2/G3-read are ADD-0.8.0; G5/G6/G4/G7 are DEFER-0.8.x** under
  that governance. Still genuinely out here: community summaries and full
  bi-temporal valid-time (G11 full) remain 0.9.0 candidates.
