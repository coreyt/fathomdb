---
title: ADR-0.8.0-graph-traversal-scope
date: 2026-06-06
target_release: 0.8.x
desc: Settle the SCOPE of the deferred graph-traversal verbs (F1 / G5 neighbors / G6 search-with-expand) so the 0.8.x graph slices build on a decided foundation. Pins the SDK depth ceiling (≤3) and the engine hard cap (50, ported from v0.5.6 MAX_TRAVERSAL_DEPTH); fixes the 0.8.x traversal filter at superseded_at IS NULL (edge valid-time G11 deferred); confirms the canonical_edges(from_id)/(to_id) indexes are already folded into G0 (no new migration); defines G6 = G1 + G4 + G5 + G9 and recommends building G6 before standalone G5; records the impl-time depth/perf profiling as part of the 0.8.x acceptance criterion (not run here). Zero 0.8.0 code/schema change. Inherits — does not re-open — the G0 substrate (Slice 15/31), the read.* namespace (Slice 25), and the graph model (Slice 32).
blast_radius: dev/plans/0.8.0-implementation.md (Slice H / G5+G6 contracts); dev/design/agent-memory-impl-strategy.md (G5/G6 seam); future 0.8.x graph slices (read.neighbors / search expand= compilation); NO 0.8.0 production code or schema change
status: proposed (awaiting HITL sign-off; the orchestrator routes the decision and flips proposed→accepted at close)
origin: dev/design/0.8.0-v05-feature-triage.md F1 (DEFER 0.8.x; v05-ready + design-ADR + profiling); dev/plans/0.8.0-implementation.md Slice 35 (HITL-split 2026-06-06: graph-traversal-scope decided now, F9/F5 deferred to Slice 46); dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md (Slice 32 foundation)
inherits: ADR-0.8.0-canonical-identity-substrate (G0 — logical_id-alone, folded edge indexes), ADR-0.8.0-supersede-five-verb-surface-cap (read.* namespace), ADR-0.8.0-graph-model-and-edge-addressing (Slice 32 — neutral substrate, opaque-id addressing, reserved-additive edge enrichment)
---

# ADR-0.8.0 — Graph-traversal scope (F1 / G5 / G6)

**Status:** 🟡 **proposed** (Slice 35 deliverable, HITL-split 2026-06-06). Awaiting
HITL sign-off; the orchestrator runs the codex adversarial pass, routes this
decision to sign-off, and flips `proposed → accepted` at close. **No 0.8.0 code or
schema change follows from this ADR** — it scopes the *deferred* 0.8.x graph verbs
so they build on a decided foundation.

> **Decides:** the *scope* of the deferred graph-traversal surface — F1's G5
> `read.neighbors(id, edge_type?, depth=1)` and the G6 `expand=` parameter on
> `search()`. The scope decision is **data-independent** (proven v0.5.6 prior art
> over the shipped G0 substrate), so every question below is settled with a
> concrete decision + a falsifiable 0.8.x acceptance criterion — **no "TBD"**. The
> impl-time depth/perf *profiling* the triage flags for F1 is **tuning for the
> eventual 0.8.x build, not a blocker on this scope decision**; it is recorded as
> part of the acceptance criterion, not run here.

---

## 1. Context — what gap, what is inherited

F1 (`dev/design/0.8.0-v05-feature-triage.md` §F1) is a **DEFER-0.8.x** feature
dispositioned **v05-ready + design-ADR + profiling**: v0.5.x already shipped a
production-grade depth-bounded `WITH RECURSIVE` BFS, so no web research is needed —
the reference is directly portable once re-targeted to the G0-folded endpoint
columns. The verbs it unlocks:

- **G5 `read.neighbors(id, edge_type?, depth=1)`** — bounded graph walk from a seed
  node over `canonical_edges`, returning neighbor `body`/`kind`/`logical_id`.
- **G6 `search(..., expand=N)`** — retrieve seed roots, then 1..N-hop expand in one
  call (the higher-leverage capstone primitive).

Both are **non-destructive reads**, governance-eligible under the `read.*`
namespace, Py+TS lockstep, with typed args + a small fixed server-side-compiled
grammar (**no raw SQL**).

**Inherited (fixed inputs — cited, never re-litigated):**

- **G0 substrate** (`ADR-0.8.0-canonical-identity-substrate.md`): active identity =
  `logical_id` **alone** on both canonical tables (Decision 5, HITL-SIGNED
  2026-06-05); invalidate-not-delete via `superseded_at`; and — load-bearing for
  this ADR — the **folded traversal indexes** `canonical_edges(from_id)` and
  `canonical_edges(to_id)` landed in the **verbatim step-12 delta** (Slice 15,
  `SCHEMA_VERSION 12`).
- **`read.*` namespace** (`ADR-0.8.0-supersede-five-verb-surface-cap.md`, Q1=A1 /
  Q2=B1): G5/G6 hang off the governed `read.*` surface. Fixed input, not a question.
- **Graph model** (`ADR-0.8.0-graph-model-and-edge-addressing.md`, Slice 32): one
  ontology-neutral binary property-graph substrate; **opaque-`logical_id` edge
  addressing** for 0.8.0; edge-enrichment (`body`/`confidence`, valid-time
  `t_valid`/`t_invalid`), edge-projectability, and edge-inclusive G7 are
  **reserved-additive (H2/H4/H5/H6 deferred "decided when built")**. This ADR builds
  on that foundation; it does **not** re-decide edge addressing or edge-enrichment.

---

## 2. Open questions (the "RED" — each must be settled below, no "TBD")

- **Q-G1 — Depth ceiling.** What SDK-facing depth limit, and what engine hard cap?
- **Q-G2 — Traversal filter at 0.8.x.** Which active-row predicate does the walk
  apply — only `superseded_at IS NULL`, or also edge valid-time?
- **Q-G3 — Index sufficiency.** Are the chosen indexes already present, or is a new
  migration required?
- **Q-G4 — G6 definition + sequencing.** What does G6 compose, and is G6 built
  before or after standalone G5?
- **Q-G5 — Prior-art fidelity.** What exactly is ported from v0.5.6, and how is it
  re-targeted to the G0 substrate?
- **Q-G6 — Profiling.** Is depth/perf profiling a blocker on this scope decision, or
  impl-time tuning?

---

## 3. Decision (one concrete answer per question)

### D-G1 — Depth ceiling: SDK default ceiling **≤ 3**, engine hard cap **50**

The **SDK** accepts a `depth` argument **defaulting to 1** and **clamps/rejects
above 3** (`read.neighbors(id, edge_type?, depth=1)`; `search(expand=)` likewise
caps at 3). The **engine** carries an independent **hard cap of 50** ported
verbatim from v0.5.6 `MAX_TRAVERSAL_DEPTH` (`compile.rs:253`) as defense-in-depth:
a compile-time `TraversalTooDeep` for any request above 50, regardless of how the
caller is constructed. The two limits are layered, not redundant — the SDK ceiling
is the ergonomic/abuse boundary for the named consumers (none of whom need deep
traversal; `agent-memory-fit` warns OpenClaw must not pressure graph depth); the
engine cap is the structural backstop that closed the v0.5.x "unbounded
`usize::MAX` → effectively-infinite CTE" defect (`compile.rs:249` FIX(review)).

> **Falsifiable 0.8.x criterion.** A `read.neighbors(id, depth=4)` (and
> `search(expand=4)`) call is **rejected at the SDK boundary** with a typed
> argument error (not silently clamped to a surprising value), in Py **and** TS
> lockstep. A unit test asserts the engine raises `TraversalTooDeep(51)` for a
> depth-51 request constructed below the SDK. The compiled CTE for any accepted
> depth `N (≤3)` contains the literal depth-bound guard `WHERE t.depth < N`.

### D-G2 — Traversal filter at 0.8.x: **`superseded_at IS NULL` only**

The 0.8.x walk filters active rows by **`superseded_at IS NULL` on both the edge
and the joined node** — the transaction-time predicate G0 landed. **Edge
valid-time (G11) is deferred**: the graph-model ADR (Slice 32) keeps the valid-time
pair **`t_valid`/`t_invalid`** (Graphiti name `valid_at`/`invalid_at` — identical
reserved columns; the substrate ADR is the canonical naming authority)
**reserved-additive**, so a later release adds a valid-time `AND` to the walk as a
pure read-path addition with **no reshape** of the traversal CTE landed at 0.8.x.

> **Falsifiable 0.8.x criterion.** The compiled neighbors/expand SQL contains
> `superseded_at IS NULL` on **both** the edge join and the node join, and contains
> **no** reference to `t_valid`/`t_invalid` (valid-time is out of 0.8.x scope). A
> test ingests a node, supersedes it, and asserts the superseded version never
> appears in a neighbor set.

### D-G3 — Index sufficiency: **already folded into G0 — no new migration**

`canonical_edges(from_id)` and `canonical_edges(to_id)` were **folded into the
verbatim G0 step-12 delta** (Slice 15;
`ADR-0.8.0-canonical-identity-substrate.md` "Authorized Slice-15 schema delta",
elements 3 + the `canonical_edges_from_id_idx` / `canonical_edges_to_id_idx`
`CREATE INDEX`es). These are exactly the indexes the directional walk joins on. The
chosen depth ceiling (≤3 SDK / 50 engine) requires **no additional index** beyond
these and `canonical_nodes(logical_id)` (the active-row resolution from the seed).
**Therefore no new migration is required for graph traversal**, and
**reserved-gap 36** (the "migration-fix slice if `from_id`/`to_id` coverage is
insufficient", `0.8.0-implementation.md:1319`) **is NOT triggered** by this scope.

> **Falsifiable 0.8.x criterion.** `EXPLAIN QUERY PLAN` of the depth-1 neighbors
> SELECT (both the `from_id` out-direction and `to_id` in-direction probes) shows
> an **index-driven** lookup using `canonical_edges_from_id_idx` /
> `canonical_edges_to_id_idx` — **no full-table `SCAN canonical_edges`**. The G5/G6
> slice adds **no migration step** (`SCHEMA_VERSION` unchanged by the graph verbs).

### D-G4 — G6 = **G1 + G4 + G5 + G9**; build **G6 before standalone G5**

**G6 `search(..., expand=N)` = the composition `G1 + G4 + G5 + G9`:** G1 (hybrid
vector/FTS retrieval) + G4 (the typed list-filter grammar — see the companion
[`ADR-0.8.0-filter-grammar.md`](./ADR-0.8.0-filter-grammar.md)) + G9 (RRF
hybrid-rank fusion, Slice 10) **seed the roots**, then G5 (the bounded walk)
**expands** them, in one call. G6 is **not independently buildable** — it
orchestrates the others; its only own surface is the `expand=` argument.

**Recommendation: build G6 before standalone G5.** G6 (search → seed → expand) is
the higher-leverage primitive the consumers actually want (`triage` F1: "G6
retrieve+expand is the higher-leverage primitive … composes G1+G4+G5+G9");
standalone `read.neighbors` is the lower-level building block G6 already exercises.
Building G6 first delivers consumer value first and exercises the G5 walk on the
way; standalone G5 then falls out as the already-built expand step promoted to a
public verb.

> **Falsifiable 0.8.x criterion.** When the G6 slice lands, `search(expand=1)`
> returns, for each hit, its 1-hop neighbors (body/kind/logical_id) sourced from
> the **same** compiled G5 walk that backs `read.neighbors` — verified by a test
> asserting the two surfaces return the identical neighbor set for the same seed +
> depth (one walk implementation, two entry points). The G4 grammar G6 filters on
> is the **same** closed enum the filter-grammar ADR pins (cross-ADR §5).

### D-G5 — Prior art: port the v0.5.6 BFS, re-targeted to `from_id`/`to_id`

The v0.5.6 depth-bounded `WITH RECURSIVE` BFS is **directly portable**:

- **`MAX_TRAVERSAL_DEPTH = 50`** hard cap (`compile.rs:253`) → engine hard cap
  (D-G1).
- the **`traversed(logical_id, depth, visited)` recursive CTE** (`compile.rs:664`)
  with the **`instr()`-over-comma-joined-path visited-set cycle guard**
  (`compile.rs:674`: `instr(t.visited, printf(',%s,', {next})) = 0`).
- **directional joins** (`compile.rs:657-658`) — and **this is the genuine porting
  work**: v0.5.6 joins on `e.source_logical_id = t.logical_id` (Out) /
  `e.target_logical_id = t.logical_id` (In), but the 0.8.0 `canonical_edges`
  endpoint columns are **`from_id` / `to_id`**. The CTE is re-targeted
  `source_logical_id → from_id`, `target_logical_id → to_id` (the triage's
  "genuine porting work", §F1).
- **depth-cap enforcement** (`compile.rs:673` `WHERE t.depth < {max_depth}`;
  `:322-331` `TraversalTooDeep`) → the SDK + engine ceilings (D-G1).
- **`superseded_at IS NULL` filtering** on the recursive joins (`compile.rs:672,
  687`) → D-G2.

For the **depth-1** case, the engine already has the shape: `trace_source_ref`
(`src/rust/crates/fathomdb-engine/src/lib.rs:3006`) probes `from_id`/`to_id`
independently — the model for a single-SELECT depth-1 walk (impl-strategy
`:326-327`); depth>1 escalates to the bounded recursive CTE.

> **Falsifiable 0.8.x criterion.** The ported CTE test mirrors v0.5.6's
> `traversal_query_is_depth_bounded` (`compile.rs:1223`): the compiled SQL contains
> `WITH RECURSIVE`, the visited-set `instr(...)` cycle guard, and `WHERE t.depth <
> N`; a cyclic-graph fixture (A→B→A) terminates and visits each node once. The
> directional joins reference `from_id`/`to_id` (not `source_logical_id`/
> `target_logical_id`).

### D-G6 — Profiling is **impl-time tuning, not a blocker** on this scope

The triage flags F1 as design-ADR **+ profiling**. The profiling — measuring walk
latency at the depth ceiling against the read-latency budget (impl-strategy
`:413`: "read-latency ceiling for G5's recursive-CTE walk at `MAX_WALK_DEPTH`") —
is **0.8.x build-time tuning of the chosen scope**, not an input to the scope
decision. The scope is settled here; the profiling validates the implementation
meets the tiered latency budget once built.

> **Falsifiable 0.8.x acceptance criterion (records the profiling).** The G5/G6
> slice ships a profiling result demonstrating the depth-≤3 walk over the binding
> ≤10k-record envelope (`memory/pr3-tiered-latency-budget.md`) meets the
> retrieval-side read-latency gate; the result is recorded (not merely asserted) so
> the depth ceiling can be re-tuned against data without re-opening this scope ADR.

---

## 4. EXCLUDE list (explicitly out of scope — named so the 0.8.x slice does not drift)

- **Unbounded / `usize::MAX` depth** — the v0.5.x pre-fix defect; the hard cap 50 +
  SDK ceiling 3 exist to make this unrepresentable.
- **Deep (>4-hop) traversal over millions of edges** — out of FathomDB's embedded
  single-writer ≤100k–1M envelope by construction (graph-model ADR §2/§4.3 W3); no
  named consumer has it. The cap is deliberately well below where a native graph
  engine would earn its keep.
- **Edge valid-time traversal (G11)** — `t_valid`/`t_invalid` stay reserved-additive
  (graph-model ADR); 0.8.x filters `superseded_at IS NULL` only (D-G2).
- **Hybrid `(from,to,kind)` MERGE addressing** — opaque-`logical_id` addressing is
  the inherited 0.8.0 decision (graph-model ADR H2 deferred); the walk addresses
  endpoints by `from_id`/`to_id` only.
- **Traversable provenance edges / episode tier (G7 lineage scope)** — deferred
  (graph-model ADR H4/H5); not a traversal-scope concern here.
- **Raw SQL / string-interpolated `edge_type`** — `edge_type` is a typed,
  parameterized filter on the compiled CTE; never interpolated.
- **A new migration / any schema change** — D-G3: indexes already folded into G0.

---

## 5. Cross-ADR consistency (required self-check)

- **G6 references G4.** `G6 = G1 + G4 + G5 + G9` (D-G4) consumes the G4 grammar
  defined in [`ADR-0.8.0-filter-grammar.md`](./ADR-0.8.0-filter-grammar.md). Both
  ADRs agree G4 = `read.list(kind, filter?, limit)` with a single **closed typed
  `Predicate` enum**; G6's `search(expand=)` filters with that same enum.
- **Valid-time naming.** Where referenced (D-G2, EXCLUDE), the pair is
  **`t_valid`/`t_invalid`** — the graph-model/substrate ADR authority — never
  `valid_at`/`invalid_at`.
- **No re-opening.** This ADR does not touch G0 identity (`logical_id`-alone), the
  `read.*` namespace, or the Slice-32 graph model / edge addressing — all cited as
  fixed inputs (§1).

---

## 6. Inheritance (upstream decisions built on, not re-opened)

| Inherited decision | Source | How this ADR uses it |
|---|---|---|
| Active identity = `logical_id` alone; invalidate-not-delete | `ADR-0.8.0-canonical-identity-substrate` D5/D2 (Slice 15/31) | Seed resolution + `superseded_at IS NULL` walk filter |
| Folded `canonical_edges(from_id)/(to_id)` indexes | same, step-12 delta (Slice 15) | D-G3 — no new migration |
| `read.*` governed namespace | `ADR-0.8.0-supersede-five-verb-surface-cap` Q1/Q2 | G5/G6 surface home |
| Neutral substrate; opaque-id addressing; reserved-additive edge enrichment | `ADR-0.8.0-graph-model-and-edge-addressing` (Slice 32) | Walk addresses `from_id`/`to_id`; valid-time stays deferred |

---

## 7. Consequences / reserved follow-on

- **For 0.8.0:** nothing ships — this ADR scopes deferred 0.8.x work. Zero code,
  zero schema, zero `acceptance.md` change.
- **For 0.8.x:** the G5/G6 slice (impl-strategy "Slice H") builds the ported BFS
  against the already-present indexes, behind the governed `read.*` surface, with no
  migration; G6 first, standalone G5 promoted from G6's expand step.
- **Reserved-additive (decided when built):** edge valid-time traversal (G11) adds a
  `t_valid`/`t_invalid` `AND` to the walk; the hybrid-addressing write ergonomics
  (graph-model H2) add MERGE without changing these read verbs.
- **Reserved-gap 36 NOT triggered** — index coverage is sufficient (D-G3); the
  reserved migration-fix slice stays unused unless a future deeper ceiling needs it.

---

## 8. Sources

Repo: `dev/design/0.8.0-v05-feature-triage.md` §F1;
`dev/adr/ADR-0.8.0-canonical-identity-substrate.md` (Decision 5; step-12 delta);
`dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md` (Slice 32; §2/§4.3 envelope,
reserved-additive enrichment); `dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md`
(`read.*`); `dev/design/agent-memory-impl-strategy.md:326-329,348-362,413` (G5/G6
seam); `dev/plans/0.8.0-implementation.md:1207,1319` (Slice 35 split; reserved-gap
36). v0.5.6 prior art: `git show v0.5.6:crates/fathomdb-query/src/compile.rs`
(`MAX_TRAVERSAL_DEPTH:253`, recursive CTE `:664`, visited-set guard `:674`,
directional joins `:657-658`, depth-cap `:673`, `superseded_at IS NULL` `:672,687`,
`traversal_query_is_depth_bounded:1223`); current engine `trace_source_ref`
(`src/rust/crates/fathomdb-engine/src/lib.rs:3006`).
