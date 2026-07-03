# Record lifecycle & projection protocol — FathomDB ⇄ Memex

> **Status: PROPOSED (convergence-first).** This is the structural contract FathomDB proposes for the
> record-liveness/lifecycle question (leverage-ledger **OPP-12**). Nothing here is contracted until the
> **HITL ratifies** and it is mirrored to OPP-12; the live convergence thread is
> `memex/dev/fathomdb/enum-discussion-ledger.jsonl` (FATHOM option at `seq 6`). Date: 2026-07-02.
> Refined over **four adversarial review rounds** (see [Provenance](#provenance)). Roadmap placement is
> **TBD** (a later steward/HITL call) — this is design, not a scheduled release.

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
| **CR-060** | `is_latest` enforced by `UNIQUE(logical_id) WHERE is_latest=1` + **composable read-mode relax-flags** whose **default** (`admissible ∧ in-force ∧ deduped-to-current-per-logical_id`) every path uses; the `WorldModel.search` gap closes by construction |
| **Q1** | engine owns the mechanism/state/invariants/evaluation |
| **Q2** | is-latest is a logical, engine-owned query exposed **once** (the partial unique index + default mode) |
| **Q3** | greenfield (migration waived); interpretive content reaches retrieval via the projection registry |
| **Q4** | mechanism/policy (+ the projection contract) |

## Reading order

1. **`structural-lifecycle-contract.md`** — the three axes, read modes, the supersession landmine +
   naming, exclusion-vs-ranking, and how Memex composes its labels.
2. **`projection-registry-and-async-embed.md`** — the projection registry, staleness split by projection
   type, the async-embed execution model + the atomic-flip invariant, GDPR erasure, and the open items.

## Open items (honestly named, not closed)

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
