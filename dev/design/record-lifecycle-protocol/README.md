# Record lifecycle & projection protocol — FathomDB ⇄ Memex

> **Status: RATIFIED (both sides, 2026-07-03).** The record-liveness/lifecycle contract (leverage-ledger
> **OPP-12**) is ratified: converged over the enum-discussion-ledger (`seq 1→12`) — shape + all 10 seq-8
> conditions + seq-9 residuals (C-1..C-5) resolved; **FathomDB HITL approved (seq-11)** and **MEMEX agreed
> (seq-12, "OPP-12 RATIFIED"),** having applied the OPP-12 mirror to its leverage ledger. **Ratification
> schedules nothing / authorizes no build (build ≠ adopt).** **C-1** (projection ↔
> `EntityTypeSpec` unification — *hard*) and **C-2** (`SearchHit.id` typed `IdSpace` newtype — *binding*) are
> folded into the surface below. Live thread: `memex/dev/fathomdb/enum-discussion-ledger.jsonl` (FATHOM option
> at `seq 6` → RATIFIED at `seq 12`). Refined over **four adversarial review rounds** + a code-grounded audit + two
> consolidation rounds (see [Provenance](#provenance)). Roadmap placement is **TBD** (breaking ⇒ likely ≥0.9.x;
> a later steward/HITL scheduling call) — this is design, not a scheduled release.

## The problem (what Memex brought)

Memex's fathom facade expresses "not live" in **three orthogonal ways plus a fourth unstored notion**,
and there is no coherent, non-stale, queryable home for lifecycle. Findings:

- **CR-056** — three incompatible liveness encodings (governed retired tombstone, op-store deleted
  tombstone, knowledge soft-delete `deleted_at`).
- **CR-057** — provenance links denormalize mutable status/title → goes stale.
- **CR-060** — "superseded" (is-this-the-latest-version-for-a-`logical_id`?) is not stored; each search
  path reconciles it differently, and `WorldModel.search` never reconciles to current → a stale/superseded
  row can leak. (Origin: a Phase-D **code-review** finding — a *logical* is-latest correctness bug, not a
  semantic-liveness issue.)

The four questions (Q1–Q4): engine concept vs app convention? where does supersededness live + can one
is-latest determination be exposed? migration/compat + EAV interaction? where is the structure/semantics
seam? **HITL waived** migration/compat and permits breaking changes on both sides.

## The answer in one paragraph

The **engine owns the record lifecycle as three orthogonal MECHANISM axes** — existence/admission,
version-currency, temporal-validity — each an indexed column, materialized so liveness exclusion happens
**at the index, not derived per query**. The **app owns the transition DECISIONS, the reasons, the
edge/attribute vocabulary + values, and the view-policy.** Interpretive content that must be *retrievable*
flows through an **engine-owned projection registry** (the sanctioned, non-stale denormalization channel
that fixes CR-057). The seam is **mechanism vs policy**, not "facts vs judgments."

## The seam: mechanism / policy

- **FathomDB (structural / mechanism)** defines each axis's **type, storage, indexing, invariants, and
  evaluation**, plus the edge substrate and the projection machinery.
- **Memex (interpretive / policy)** supplies the **values** and makes the **transition decisions**, owns
  the edge-type vocabulary and the graded-attribute meanings, and sets the per-view include-policy.

This cut is clean even where "facts vs judgments" fails: temporal-validity *values* are Memex assertions
while the window *mechanism* is FathomDB's; `pending` is a FathomDB state that holds a Memex quarantine
judgment. ("Structural/interpretive" are kept as informal names, **defined as** mechanism/policy.)

## Mapping Memex's A / B / C

Memex decomposed its side into (A) lifecycle STATES, (B) semantic RELATIONS (edges), (C) graded
ATTRIBUTES (dials). Under the seam:

| Memex item | Really is | Owner | FathomDB mechanism |
|---|---|---|---|
| A: live/active | existence `active` | structural | existence enum |
| A: draft/quarantined | existence `pending` | structural state, app reason | existence enum + reason attr |
| A: retired / deleted | `deleted` + **reason** | state=structural, reason=interpretive | existence enum; reason is an attr |
| A: expired ("on vacation till Friday") | **temporal validity** (still true, not in-force) | structural | `valid_from`/`valid_until` |
| A: archived | **view-policy** | interpretive | retrieval scope over `active` |
| B: obsoleted-by / contradicts / refines / duplicate-of | typed **edges** | mechanism=structural, vocab=interpretive | graph substrate; Memex owns types |
| C: confidence / salience / decay / relevance | graded **dials** → *ranking* | mechanism=structural, values=interpretive | attribute storage + F9/recency signal |

Memex's flat `live | superseded | retired | deleted` enum was a **conflation** of these; the clean cut
puts states on the structural axes, reasons/vocab/dials/view-policy on the interpretive side, and lets
Memex **compose** its labels as predicates.

## How it resolves the findings & questions

| | Resolved by |
|---|---|
| **CR-056** | one engine existence-state replaces the three hand-rolled tombstones; retired/deleted become reason attributes |
| **CR-057** | the **projection registry** (same-transaction for FTS/filter, async+rebuild-durable for vector) is the sanctioned denormalization channel; "never denormalize" → "never *hand*-denormalize" |
| **CR-060** | the **shipped** `UNIQUE(logical_id) WHERE superseded_at IS NULL` G0 index (not a new `is_latest` col) + **composable read-mode relax-flags** (net-new) whose **default** (`current ∧ in-force ∧ deduped-per-logical_id`) every path uses; the `WorldModel.search` gap closes by construction |
| **Q1** | engine owns the mechanism/state/invariants/evaluation |
| **Q2** | is-latest is a logical, engine-owned query exposed **once** (the partial unique index + default mode) |
| **Q3** | greenfield (migration waived); interpretive content reaches retrieval via the projection registry |
| **Q4** | mechanism/policy (+ the projection contract) |

## What exists today vs net-new (code-grounded)

This contract is a **design proposal**: most of it is **net-new engine work** and must not read as shipped.
Code-grounded delta (`fathomdb-schema` / `fathomdb-engine`, `SCHEMA_VERSION=15`; full audit in
[`code-grounded-audit.md`](code-grounded-audit.md)):

**Exists today (the contract builds on these):**

- **Version-currency** via `superseded_at` — the HITL-signed G0 `UNIQUE(logical_id) WHERE superseded_at IS NULL`
  index (ADR-0.8.0). This *is* the single is-latest authority.
- **Edge-level temporal validity** — `canonical_edges.t_valid`/`t_invalid` (ISO-8601), inline-`datetime`-evaluated.
- **An engine-owned async projection worker** (dispatcher + thread pool), rebuild-durable projections,
  `_fathomdb_vector_rows.kind` coverage tracking + a `verify_embed_db` gate.
- **`SearchHit`** (`id`=`write_cursor` + additive `stable_id`), `body`-FTS + vector + RRF + CE-rerank +
  recency-reweight seam, typed edges + a `confidence` dial, `logical_id` identity for governed writes.

**Net-new (must be built; do not imply it exists):**

- The **existence axis** (`pending/active/deleted/purged`) + transition verbs + **physical purge**/erasure.
- **Node-level validity** (`valid_from`/`valid_until`), integer windows, the bound-`:now` seam, `valid_as_of`,
  the `crossed_boundary_since` hook.
- A materialized **`admissible`** column; `is_latest` as a stored column (it is a derived predicate today).
- **Composable read-mode relax-flags** + read-mode uniformity on `get`/`list`/`neighbors`.
- The **projection registry** (`filterable`/`rankable`/`searchable`) + the **EAV/attribute store + property-FTS**
  it projects from (only `body`-FTS exists today).
- `dense_readiness` + `flush_embeddings()` + the atomic readiness-flip (additions to the existing worker).
- An engine-minted **opaque surrogate `logical_id`** for anonymous nodes.

## SDK surface (Python + TS)

FathomDB ships **thin** SDKs — `fathomdb-py` (pyo3) and `fathomdb-napi` (napi) — that **marshal** the governed
engine surface (types, errors, counters) and forward calls; they add **no** client-side liveness / dedupe /
filtering logic (verified). Implications for this contract: (1) every net-new surface (read-modes,
`SearchHit.logical_id`, lifecycle verbs, the projection-declaration API, `valid_as_of`/`:now`) must be threaded
through **both** SDKs with **parity** (the X1 cross-binding requirement) — exposure work, not logic; (2) the
single authority stays in the **engine** — the SDKs must remain pass-through, and the app (e.g. Memex's
`fathom_store.py` facade) must **stop** re-deriving liveness client-side, or CR-060 simply reappears one layer
up. Lifecycle is **engine mechanism, exposed 1:1 by the SDKs, with policy in the app** — no per-binding logic.

## Reading order

1. **`structural-lifecycle-contract.md`** — the three axes, read modes, the supersession landmine +
   naming, exclusion-vs-ranking, and how Memex composes its labels.
2. **`projection-registry-and-async-embed.md`** — the projection registry, staleness split by projection
   type, the async-embed execution model + the atomic-flip invariant, GDPR erasure, and the open items.
3. **`api-surface.md`** — the consolidated verb + signature delta (net +3 verbs; `ReadView`, `PreparedWrite`,
   `SearchHit`-shrink, `LifecycleState`), with the four consolidation-review fixes folded in.

## Co-requisites & open items (honestly named, not closed)

- **`SearchHit` `logical_id` co-requisite (GATING; = F-8a / ADR-0.8.0 swap).** Dedupe-to-current requires
  hits to carry the `logical_id`, not the interim `write_cursor` (`SearchHit.id` today). Complete the
  `SearchHit.id: write_cursor → logical_id` swap (building on Cause-A's additive `stable_id`) and resolve the
  **doc-seeded nodes have no `logical_id`** gap (they are `h:`-content-hashed and fall outside version-currency).
  Lands-together with the Cause-A id-contract. See `structural-lifecycle-contract.md` §2.

- **F9 signal algebra** — the graded-attribute ranking contract (range / monotonicity / missing-value
  default / combination law with BM25·vector·RRF·recency) is named but unfilled; specced when F9 lands (~0.8.16).
- **`history_as_of`** (transaction-time travel) — deferred out of scope; the append-only version history +
  write-time stamps do not preclude it. Only `valid_as_of` (world-time) ships.
- **GDPR file-erasure preconditions** — `PRAGMA secure_delete=ON` / post-purge `VACUUM`, and `logical_id`
  must be an opaque surrogate.
- **Roadmap placement** — unscheduled; a steward/HITL decision (breaking changes ⇒ likely a ≥0.9.x line).

## Provenance

- Convergence thread: `memex/dev/fathomdb/enum-discussion-ledger.jsonl` — MEMEX `seq 1` (problem),
  HITL `seq 2` (waiver), MEMEX `seq 3` (the logical/semantic split of "superseded"), MEMEX `seq 4`
  (CR-060 origin = a logical is-latest bug), FATHOM `seq 5` (working note), FATHOM `seq 6` (this option).
- **Four adversarial review rounds** (Fable 5): R1 dismantled the original single materialized
  `retrievable` bit + the "never denormalize" rule + missing read-modes/transition-table; R2 confirmed the
  bones and pinned the vector-projection staleness split; R3 closed `valid_as_of`, purge, read-mode
  composability; R4 confirmed the host-driven async-embed model and pinned the **atomic `{vector-write ∧
  dense_readiness:=ready}`** invariant as the last sharp edge. What survived every round: the three
  orthogonal axes, the mechanism/policy seam, exclusion-vs-ranking separation, version-vs-meaning
  supersession, the projection registry, and named composable read modes.
