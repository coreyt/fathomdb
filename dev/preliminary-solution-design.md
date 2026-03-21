# SOLUTION DOCUMENT: FathomDB Preliminary Solution Design

## 1. Executive Summary

This document bridges the current [USER_NEEDS](./USER_NEEDS.md) and
[ARCHITECTURE](./ARCHITECTURE.md).

The user need is not merely "store documents and search them." The system must
support a **local, high-trust datastore for persistent AI agents** that can:

- maintain a durable world model
- retrieve across structure, text, semantics, relationships, and time
- preserve prompt-control and governance context around action
- support replay, evaluation, provenance, approvals, and selective rollback
- remain embeddable, local-first, and operationally lightweight

The architectural answer is to keep **SQLite as the canonical local store** and
place an opinionated **graph/vector/FTS shim** in front of it. The shim does
not replace canonical state. It gives agents deterministic multimodal access,
manages derived projections, and coordinates write-time normalization so the
surrounding application does not have to build and synchronize multiple memory
systems by hand.

## 2. Solution Framing

### 2.1 What Problem This Solves

Personal AI agents are not only retrieval systems. They participate in ongoing
work:

- they carry forward goals, preferences, and commitments
- they ingest mixed sources such as notes, meetings, email, and files
- they operate across interactive and background workflows
- they need records of why they interpreted, acted, stored, escalated, or
  abstained
- they need safe correction and review when they are wrong

The datastore therefore needs to serve as both:

1. a **canonical semantic store** for durable agent state
2. a **multimodal access layer** for graph, document, full-text, and vector
   queries over that state

### 2.2 What This Solution Is

`fathomdb` is designed as:

- a canonical SQLite-backed local datastore
- a graph-centric semantic backbone with typed side tables for key agent state
- a shim-managed multimodal projection layer for FTS and vectors
- an agent-friendly query compiler and SDK surface
- an execution coordinator that manages WAL mode, prepared statements, and serialized writes
- a governed write pipeline that records provenance, correction history, and
  operational artifacts

This preserves the core technical decision already captured in
`ARCHITECTURE.md`: `fathomdb` is intentionally a graph/vector/FTS shim in front
of SQLite.

### 2.3 Implementation Language Split

The agreed implementation split is:

- **Rust for the engine**
  - the AST compiler
  - the SQLite execution layer
  - the single-writer engine core
  - the projection and write-path logic that is tightly coupled to SQLite internals
- **Go for separate surrounding services**
  - a sync daemon that replicates or backs up local `fathomdb` instances
  - a remote ingestion worker for heavy background jobs that are intentionally outside the embedded core
  - a gateway or API server that exposes `fathomdb` over gRPC or HTTP for multi-process or networked use
  - operational tooling such as controllers, job dispatchers, and observability collectors
  - connector services that talk to external systems and feed canonical artifacts into the Rust core
- **Python and TypeScript as SDKs**
  - language-facing SDK layers over the Rust core

This keeps the embedded engine in the language best suited to compiler logic,
SQLite integration, and explicit concurrency control, while allowing Go to be
used where service ergonomics matter more than tight engine coupling.

## 3. Mapping User Needs To Solution Elements

| User Need | Solution Element | How the Need Is Met |
| :--- | :--- | :--- |
| **Privacy, locality, and low operational burden** | SQLite as canonical local store | State remains local, portable, embeddable, and zero-ops relative to heavier server-style databases. |
| **Persistent world model** | Graph-centric backbone plus typed semantic side tables | Shared entities and relationships live in a durable substrate while agent-specific semantic families such as intents, actions, meeting artifacts, approvals, and evaluation records remain first-class canonical data. |
| **Multi-modal recall and reasoning** | Graph relations, JSON documents, FTS, vector projections, temporal metadata | The agent can combine structure, relationships, text, semantic similarity, provenance, and chronology in one query model. |
| **Deterministic agent ergonomics** | Fluent SDK and query compiler | Agents use predictable programmatic builders instead of brittle string-query generation. |
| **Automated housekeeping** | Shim-managed ingestion, projection maintenance, and normalization | Raw content can be ingested once and transformed into canonical records plus derived search surfaces without the caller synchronizing them manually. |
| **Prompt-control and governance visibility** | Canonical control artifacts and proposal states | The system can persist how a request was interpreted, which policy path was chosen, and whether clarification, approval, rejection, or escalation occurred. |
| **Provenance, replay, and evaluation** | Versioned writes, source tracking, action/observation records, evaluation tables | The system can answer why a fact exists, what changed, and which layer failed. |
| **Reversibility and human trust** | Append-oriented state, correction lineage, proposal/approval workflow | Harmful or uncertain actions can be reviewed, corrected, superseded, or undone without erasing history. |
| **Corruption containment and repair** | Canonical-vs-derived split, provenance-linked excision, deterministic rebuilds, admin tooling | Physical corruption, projection drift, and semantic corruption can be repaired with explicit engine/admin operations instead of coarse snapshot restores. |
| **Operational continuity** | Support for scheduler runs, approvals, logs, meeting ingestion, and background work | The datastore supports the full agent runtime, not only ad hoc search. |

## 4. Architectural Synthesis

### 4.1 Canonical Local Store

SQLite remains the canonical persistence layer. This satisfies the need for:

- local-first privacy
- durability
- portability
- embeddable deployment
- manageable operational complexity

The solution explicitly avoids writing a custom storage engine, WAL, or B-tree.
That reduces implementation risk while keeping the product local and reliable.

### 4.2 Graph-Centric Backbone With Typed Semantic Tables

The canonical model is not only a bag of JSON documents. It is split into:

- a graph-friendly backbone for durable entities and relationships
- typed side tables for semantic families that should remain explicit

This lets the system model:

- people, projects, tasks, meetings, claims, and events as connected state
- prompt-control artifacts such as interpretation and policy selections
- action and observation history
- meeting artifacts and promoted commitments
- approvals, review decisions, and evaluation records

This is the main way the solution resolves the gap between "multimodal memory"
and "persistent local agent datastore."

The resolved storage policy is:

- keep human-world entities flexible in JSONB-backed graph records
- keep agent-runtime and evaluation state in explicit typed tables
- avoid aggressive normalization of world-entity properties until the product has much more empirical schema stability

### 4.3 Derived Multi-Modal Projections

FTS and vector indexes remain important, but they are **derived projections**
over canonical state.

This design choice gives the system:

- fast lexical and semantic retrieval
- graph traversal over shared entities and relationships
- rebuildability when projections drift or fail
- the ability to keep canonical truth separate from access optimizations

This is the correct place for the graph/vector/FTS shim concept: it is a core
architectural mechanism, but not the whole product definition.

The expert guidance sharpens this into two projection classes:

- **required projections:** low-cost lexical/search projections that should commit atomically with canonical state
- **optional projections:** expensive semantic enrichments that may be queued for background backfill during bulk ingestion

### 4.4 Agent-Friendly Query Layer

The shim exposes deterministic SDKs and compiles them into optimized SQL.

This is important because the agent needs:

- predictable data access
- multimodal retrieval in one execution model
- fewer opportunities for bespoke syntax hallucination

The compiler must support queries that combine:

- graph traversal
- JSON filtering
- full-text search
- vector similarity
- temporal filtering
- provenance-aware filtering
- joins against typed semantic records such as intents, actions, or evaluations

At the lower level, the query path has four concrete strata:

1. a fluent AST builder in the SDK
2. a compiler that rewrites that AST into one SQLite execution plan
3. an execution coordinator that manages prepared statements, WAL, and connection strategy
4. a write/projection pipeline that keeps canonical rows and search surfaces synchronized

The core compiler rule is **inside-out candidate reduction**:

- start from the narrowest indexed candidate set, usually vector or full-text search
- join through graph structure from that reduced set
- fetch canonical rows only after candidate reduction
- apply JSON and relational filters late over the already narrowed results

This is the operational meaning of top-k pushdown in the current design.

There is one important planner exception: when the request already contains a
highly selective deterministic filter such as a specific entity ID or direct
foreign-key equality, the relational filter should drive the query and semantic
or lexical search should be constrained inside that smaller scope.

### 4.5 Governed Write Path

The write path should not be a thin `INSERT` wrapper. It must be able to:

1. accept raw source artifacts
2. do expensive pre-flight work such as parsing, chunking, or embedding lookup before taking a write lock
3. normalize them into canonical records
4. update entities and relationships
5. project searchable text and embeddings
6. attach provenance, confidence, and correction lineage
7. record control artifacts, actions, observations, and review outcomes

This directly addresses the needs for automated housekeeping, operational
continuity, and replayable behavior.

The lower-level write discipline should be:

1. pre-flight enrichment outside the transaction
2. `BEGIN IMMEDIATE` only when canonical and projection payloads are ready
3. canonical append/update of rows and typed side-table records
4. atomic synchronization of required projections
5. commit and release the writer lock

That gives the system a cleaner consistency story than either application-side
stitching or trigger-heavy synchronization.

For interactive writes, semantic vectors needed immediately by the agent should
be generated before the writer lock is acquired and committed atomically with
the canonical write.

For bulk or background ingestion, canonical rows may be written first and
optional semantic projection jobs queued into a local worker table so the
interactive loop does not stall on long-running embedding work.

## 5. Key System Flows

### 5.1 Retrieval Flow

When the agent needs context, the system should:

1. identify the target semantic families
2. combine graph, structured, lexical, semantic, and temporal filters
3. push narrow candidate generation as deep into SQL as possible
4. return canonical records with enough provenance and context for safe use

This is where the top-k pushdown and query compiler design remain highly
aligned with the user needs.

Operationally, this implies:

- compiled SQL should be parameterized and cached using an AST-shape hash
- read paths should run over WAL-backed read connections
- query plans should prefer indexed driving tables instead of scanning JSON-heavy canonical tables first
- recursive traversals must always be depth-bounded, cycle-aware, and hard-limited

### 5.2 Ingestion And Promotion Flow

When the agent ingests new material, the system should:

1. preserve the raw source
2. create or update canonical semantic records
3. maintain graph relations
4. create or refresh derived FTS/vector projections
5. optionally promote observations into durable knowledge, tasks, decisions, or
   commitments when policy permits

This makes ingestion useful for ongoing agent work, not only future search.

It also means embeddings and other expensive enrichments should be prepared
before the SQLite writer transaction begins.

### 5.3 Control And Review Flow

When the system interprets a request or performs a high-impact action, it should
be able to persist:

- the interpreted request
- uncertainty or ambiguity signals
- selected route or policy bundle
- whether approval or clarification was required
- resulting actions, observations, and outcomes

This is what allows the datastore to support trust, replay, and evaluation.

### 5.4 Correction And Reversal Flow

When the human or system corrects prior state, the system should:

- preserve the earlier state as superseded rather than invisibly erased
- retain source and correction lineage
- record approval, rejection, or revision states where needed
- keep derived projections consistent with the corrected canonical state

This resolves the earlier PSD's overly coarse "undo recent nodes" framing.

### 5.5 Temporal And Explainability Flow

By default, reads should target active state rather than every historical row.
When temporal scoping is requested, the query layer should switch from
"currently active" semantics to "state as of time T" semantics without changing
the external SDK model.

The same canonical metadata should also enable explain-style joins so the system
can answer why a fact, task, or relationship exists by linking it back to the
control artifact, action, observation, or review decision that produced it.

The preferred provenance model is a direct `source_ref` chain on canonical rows
rather than a separate generic lineage graph.

### 5.6 Repair And Recovery Flow

Recovery should be treated as part of the product, not as an external ops
escape hatch. The system should explicitly handle:

- **physical corruption:** recover canonical tables, then rebuild projections
- **logical corruption:** deterministically rebuild derived projections from
  canonical state
- **semantic corruption:** rollback or excise bad outputs by time window or
  `source_ref`

This works because the architecture separates canonical state from derived
projections and preserves append-oriented provenance-linked history.

The practical admin surface should include:

- `rebuild_projections(target=[...])`
- `rebuild_missing_projections()`
- trace by `source_ref`
- export a safe local snapshot
- excise a bad run/step/action and apply a small repair patch

The Go-based admin CLI is the right place for these operational workflows,
because it fits the agreed language split: Rust for the engine, Go for separate
surrounding services and tooling.

## 6. Why The Main Technical Choices Are Acceptable

### 6.1 SQLite Instead Of A Custom Store

**Choice:** use SQLite as the canonical local store.

**Trade-off:** accept single-writer constraints and relational storage
discipline.

**Why acceptable:** the product gains durability, portability, embeddability,
and drastically lower implementation risk.

### 6.2 Shim-Managed Graph, FTS, And Vector Projections

**Choice:** manage multimodal projections in the shim rather than rely on
database triggers or separate systems.

**Trade-off:** the write path becomes more opinionated and complex.

**Why acceptable:** the product gains explicit synchronization, better error
handling, and a single agent-facing API surface.

It also gives the implementation room to:

- precompute embeddings before write lock acquisition
- commit required projections atomically with canonical rows
- rebuild projections deterministically if state must be regenerated

### 6.3 Typed Semantic Tables In Addition To Graph Nodes/Edges

**Choice:** combine a graph backbone with typed semantic side tables.

**Trade-off:** more schema design and migration work.

**Why acceptable:** local agents need explicit semantics for things like
interpretation, actions, approvals, and evaluation records. Hiding all of that
inside generic property blobs weakens auditability and maintainability.

### 6.4 JSONB-First World Entities

**Choice:** keep user-world entity properties primarily in SQLite JSONB.

**Trade-off:** some hot predicates need explicit indexing help.

**Why acceptable:** the product avoids migration churn on user-owned local data
while still allowing expression indexes on frequently queried JSON paths.

### 6.5 Append-Oriented, Versioned State

**Choice:** prefer supersession and correction lineage over destructive
overwrite.

**Trade-off:** more storage use and cleanup complexity.

**Why acceptable:** reversibility, trust, replay, and evaluation depend on
preserving history.

### 6.6 Time-Sortable IDs And Versioned Vector Tables

**Choice:** prefer ULID-style time-sortable IDs and version vector tables by
embedding profile.

**Trade-off:** more explicit migration and backfill mechanics.

**Why acceptable:** sequential-ish IDs are friendlier to SQLite B-trees and
versioned vector tables avoid impossible in-place migrations when model
dimensions change.

### 6.7 Single-File Portability As A Recovery Primitive

**Choice:** keep the world state in a single SQLite file and rebuild projections
from canonical truth rather than spread authority across multiple databases.

**Trade-off:** the embedded engine must own more of the recovery story itself.

**Why acceptable:** portability is not only a deployment convenience. It also
enables lightweight debug, export, patch, and repair workflows for local agent
systems.

## 7. Updated Risk Assessment

### 7.1 Critical Risks

**1. Projection And Canonical State Divergence**

- **Risk:** FTS or vector projections drift from canonical state, causing the
  agent to retrieve stale or misleading context.
- **Mitigation:** keep projections derived and rebuildable, preserve canonical
  linkage on projection rows, precompute required projection payloads before the
  write transaction when needed, and expose repair/rebuild paths.

**2. Opaque Agent Behavior**

- **Risk:** the system stores end-state facts but not enough interpretation,
  action, or provenance data to explain why something happened.
- **Mitigation:** make control artifacts, actions, observations, approvals, and
  evaluation records canonical writes rather than optional logs.

**3. N+1 Or Application-Memory Query Blowups**

- **Risk:** multimodal retrieval falls back to application-side stitching and
  destroys latency.
- **Mitigation:** compile a single cohesive SQL plan where possible and use
  subqueries, indexed driving tables, and pushdown aggressively.

### 7.2 High Risks

**1. SQLite Writer Contention**

- **Risk:** concurrent ingestion and background work cause `SQLITE_BUSY` or
  poor interactive behavior.
- **Mitigation:** use WAL for reader concurrency and serialize writes through a
  coordinated writer path with a single writer connection or equivalent queue.

**2. Embedding And Enrichment Blocking**

- **Risk:** chunking, embedding, or extraction blocks the interactive loop.
- **Mitigation:** decouple enrichment from the locked write path by finishing
  expensive pre-flight work before `BEGIN IMMEDIATE`.

**3. Weak Failure Attribution**

- **Risk:** the product can tell that a turn went badly but not whether the
  problem came from interpretation, retrieval, policy, tool use, or response
  quality.
- **Mitigation:** preserve stage-specific artifacts and outcome metadata for
  replay and evaluation.

**4. Physical Recovery Misuse**

- **Risk:** operators use SQLite recovery tools directly against FTS/vector
  shadow data and make a bad physical-repair event worse.
- **Mitigation:** document and enforce a canonical-only recovery protocol, then
  rebuild projections through engine/admin tooling.

### 7.3 Medium Risks

**1. JSON/Metadata Query Hotspots**

- **Risk:** repeated filtering into JSON-heavy records becomes a latency
  bottleneck.
- **Mitigation:** monitor hot paths and add generated or functional indexes for
  repeated filters, preferring direct expression indexes on common JSON-path predicates.

**2. Storage Growth From History And Projections**

- **Risk:** append-oriented history and multimodal projections increase file
  size significantly over time.
- **Mitigation:** provide pruning, compaction, and projection rebuild paths
  tied to review and approval workflows.

**3. Schema Drift Across Evolving Semantic Families**

- **Risk:** as the product adds new semantic tables and control artifacts,
  migrations become fragile.
- **Mitigation:** keep shim-owned schema metadata and explicit migration logic.

## 8. Alignment Summary

The revised solution remains faithful to the original core thesis:

- use SQLite rather than build a new engine
- present a graph/vector/FTS shim to the agent
- expose deterministic APIs rather than brittle string-query languages
- keep multimodal retrieval fast and explicit

What has changed is the solution boundary. The shim is now properly positioned
inside a larger answer to the actual user needs: a canonical local datastore
for persistent AI agents with durable world modeling, governed action,
provenance, replay, evaluation, approvals, and reversible state.

## 9. MVP Priorities

If the project needs a strict first cut to validate the architecture, the most
important slice is:

1. the AST compiler
2. the JSONB graph backbone
3. atomic SQLite transactions for canonical rows plus required projections
4. the dedicated writer thread / queue

The following can be deferred from an MVP if necessary:

- proposal and approval flows
- dynamic vector-dimension migration machinery
- richer evaluation records beyond core intents and actions

The deferred schema and subsystem ideas are intentionally preserved in
[ARCHITECTURE-deferred-expansion.md](./ARCHITECTURE-deferred-expansion.md)
rather than being dropped from the design record.

The minimal recovery/admin slice that should remain in MVP scope is:

- rebuild projections
- safe export / snapshot
- trace by `source_ref`
- excise bad lineage and re-run projection repair

That keeps the first implementation focused on the hardest architectural truth:
whether the SQLite-backed multimodal engine behaves predictably under real local
agent workloads.
