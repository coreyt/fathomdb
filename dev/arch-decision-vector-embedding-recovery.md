# Architecture Decision Whitepaper: Vector Embedding Recovery

## Purpose

This paper defines the three credible architectural choices for vector
embedding recovery in `fathomdb`, and compares their pros, cons, and practical
implications.

It is written for both:

- humans making product and architecture decisions
- LLMs/agents that need a compact, unambiguous statement of the trade-offs

## Scope

This paper is specifically about what happens to embeddings written through
`VecInsert` when a database must be physically recovered or logically rebuilt.

It is not about whether `sqlite-vec` is useful, whether vector search should
exist, or whether embeddings are valuable to agent applications. Those are
already assumed.

## Current Repo Position

The current documented v0.1 contract is:

- canonical tables are recovered first
- vector profile metadata is preserved
- vector table capability is restored when supported
- embedding rows written through `VecInsert` are not yet guaranteed to survive
  physical recovery

See:

- [ARCHITECTURE.md](./ARCHITECTURE.md)
- [repair-support-contract.md](./repair-support-contract.md)

## The Three Choices

### Choice A: Canonical Embeddings

Embeddings are treated as canonical application data.

Implications:

- embeddings become part of the authoritative stored state
- physical recovery must preserve them exactly
- export, backup, repair, and replication must treat them like primary data

### Choice B: Projection-Preserved Embeddings

Embeddings remain non-canonical projection material, but recovery explicitly
preserves and restores the embedding rows themselves.

Implications:

- canonical authority still lives outside vector rows
- recovery must dump/replay vec rows in addition to canonical tables
- vector rows are operationally important even though they are not canonical

### Choice C: Derived / Regenerable Embeddings

Embeddings are treated as derived data that may be regenerated after recovery
from durable source material plus embedding-generation metadata.

Implications:

- physical recovery restores canonical state and vector capability
- embedding rows may be absent immediately after recovery
- a deterministic or controlled regeneration workflow is required

## Quick Comparison

| Dimension | Choice A: Canonical | Choice B: Projection-Preserved | Choice C: Derived / Regenerable |
|---|---|---|---|
| Recovery fidelity | Highest | High if replay works | Variable; depends on regeneration |
| Architectural fit with current `fathomdb` canonical/projection split | Weakest | Medium | Strongest |
| Storage cost | Highest | Medium to high | Lowest in canonical layer |
| Recovery speed | Medium | Medium | Potentially slowest |
| Dependence on external models/services | Lowest | Lowest | Highest unless local/deterministic |
| Determinism of post-recovery search behavior | Highest | High | Lowest unless tightly controlled |
| Product-boundary complexity | Medium | Medium | Highest |
| Operational simplicity at query time | High | High | Medium after recovery |
| Operational simplicity at recovery time | High | Medium | Lowest |

## Detailed Analysis

### Choice A: Canonical Embeddings

#### Pros

- Strongest recovery claim.
  - If the database recovers, embeddings recover too.
- Simplest operator story.
  - No separate regeneration step is needed.
- Best post-recovery fidelity.
  - Vector search behavior can match pre-failure behavior most closely.
- Lowest dependency on external model/runtime availability during recovery.

#### Cons

- Weak fit with the current architecture.
  - `fathomdb` is intentionally built around canonical relational state plus
    derived FTS/vector projections.
- Higher storage and export cost.
  - Canonical backups and snapshots become much larger.
- Embedding model artifacts become part of long-term authority.
  - model version
  - dimension
  - normalization policy
  - chunking assumptions
- Harder future migrations.
  - model upgrades become canonical-data migration problems, not projection
    rebuild problems.
- Higher blast radius for corruption.
  - corrupted embeddings now affect canonical state, not only an optional
    search surface.

#### Best Fit

Choose this when exact vector-search continuity after recovery is more
important than preserving the current canonical/projection architecture.

### Choice B: Projection-Preserved Embeddings

#### Pros

- Better recovery fidelity than the current v0.1 contract without fully making
  embeddings canonical.
- Preserves current query behavior more closely after recovery.
- Lower external dependency during recovery than regeneration-based designs.
- Keeps the application-facing story simple.
  - recovered DB opens with vectors already usable

#### Cons

- Architecturally awkward.
  - rows are called non-canonical, but recovery now depends on preserving them
    almost like canonical data
- Recovery implementation gets more complex.
  - physical recovery must safely replay vec rows and vec capability together
- The boundary between canonical and projection data becomes less clean.
- Rebuild semantics remain underspecified.
  - if vec rows are lost outside physical recovery, are they rebuildable or not?
- Testing burden increases.
  - recovery must prove not only table recreation, but vec-row preservation and
    query correctness

#### Best Fit

Choose this when the product wants near-immediate vector usability after
recovery, but does not want to fully promote embeddings to canonical status.

### Choice C: Derived / Regenerable Embeddings

#### Pros

- Strongest fit with the current architecture.
  - canonical state stays small and authoritative
  - vector state remains derived
- Lower canonical storage burden.
- Cleaner model for rebuildable projections.
- Easier to change embedding models over time, if the regeneration contract is
  explicit.
- Keeps physical recovery centered on canonical tables.

#### Cons

- Recovery is not self-contained unless regeneration inputs are fully durable.
- Post-recovery vector search may be unavailable until regeneration completes.
- Determinism is harder.
  - model version drift
  - tokenizer drift
  - chunking drift
  - normalization drift
- Operational dependency is higher.
  - local model artifacts or external embedding providers may be needed
- Regeneration can be slow and expensive on large datasets.
- Product-boundary questions become unavoidable.
  - Does `fathomdb` own regeneration?
  - Does the application own regeneration?
  - What metadata must be persisted to make regeneration valid?

#### Best Fit

Choose this when architectural cleanliness and canonical/projection separation
matter more than immediate recovery-time vector continuity.

## What Each Choice Requires

### If `fathomdb` Chooses A

Required implementation work:

- add canonical storage for embedding payloads
- define migration/version policy for stored embeddings
- update export, recovery, and integrity tooling to treat embeddings as
  authoritative state
- add e2e tests proving exact vector search continuity after recovery

New contract:

- recovered databases preserve embedding rows exactly

### If `fathomdb` Chooses B

Required implementation work:

- extend physical recovery to preserve and replay vec rows safely
- prove recovered databases can execute vector search immediately
- define how vec-row preservation interacts with projection rebuild operations
- document whether lost vec rows outside physical recovery are repairable

New contract:

- recovered databases preserve operational vector rows, even though those rows
  are not the canonical source of truth

### If `fathomdb` Chooses C

Required implementation work:

- define the regeneration owner:
  - engine
  - admin tool
  - application
- persist enough metadata to make regeneration valid:
  - model identity
  - model version
  - embedding dimension
  - normalization policy
  - chunking/preprocessing policy
- add a regeneration workflow and admin command
- add e2e tests for recover -> regenerate -> vector search

New contract:

- recovered databases restore vector capability and can regain embeddings
  through a defined regeneration workflow

## Decision Criteria

Use these questions to choose among A, B, and C:

1. Must post-recovery vector search behave as closely as possible to the
   pre-failure database?
2. Is `fathomdb` willing to treat model-specific artifacts as long-term
   authoritative data?
3. Must recovery remain self-contained with no external model dependency?
4. Is preserving the current canonical/projection split a hard architectural
   requirement?
5. Is delayed vector availability after recovery acceptable?
6. Who owns embedding generation in the product boundary: the engine or the
   application?

## Agent Summary

This section is intentionally compact for machine readers.

```yaml
topic: vector_embedding_recovery
choices:
  A:
    name: canonical_embeddings
    summary: embeddings are authoritative stored data and must recover exactly
    strengths:
      - strongest recovery fidelity
      - self-contained recovery
      - simplest operator story
    weaknesses:
      - weakest fit with canonical/projection split
      - highest storage and migration cost
      - model artifacts become long-term authority
  B:
    name: projection_preserved_embeddings
    summary: embeddings remain non-canonical, but recovery preserves vec rows directly
    strengths:
      - strong post-recovery usability
      - lower external dependency than regeneration
    weaknesses:
      - awkward architectural boundary
      - more complex recovery semantics
      - rebuild contract becomes blurry
  C:
    name: derived_regenerable_embeddings
    summary: embeddings are rebuilt from durable source material and metadata
    strengths:
      - cleanest fit with current architecture
      - smallest canonical burden
      - easiest long-term model evolution
    weaknesses:
      - weakest self-contained recovery story
      - higher operational dependency
      - deterministic recovery is harder
current_v0_1_position:
  closest_to: C
  note: current docs preserve vector profile capability, but not VecInsert rows
recommended_next_step:
  decide whether recovery fidelity or architectural purity is the governing priority
```

## Bottom Line

There is no free option.

- Choice A maximizes recovery fidelity by making embeddings authoritative.
- Choice B maximizes short-term usability after recovery while keeping an
  awkward mixed model.
- Choice C best matches `fathomdb`'s current architecture, but only works well
  if regeneration is made explicit, durable, and testable.

The real decision is not just "should embeddings survive recovery?" It is:

**Are embeddings part of the database's authority, part of its operational
working set, or part of a rebuildable derived layer?**
