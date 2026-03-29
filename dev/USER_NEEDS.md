# USER_NEEDS.md

## Scope: Engine-Level Needs vs Application-Layer Needs

This document is organized in two parts.

**Part 1** (§1–7) describes what any local agent datastore must provide,
regardless of the application built on top. These needs drive fathomdb's
engine design: the graph backbone, query compiler, write pipeline, provenance
model, operational store, and recovery surface.

**Part 2** (§8) describes the needs of one example application — a
personal AI agent that manages meetings, scheduling, email, and long-running
goals. These are valid and important needs, but they are application-layer
responsibilities. They should be built on top of the engine's primitives, not
baked into the engine schema.

---

# Part 1: Engine-Level User Needs

## 1. Executive Summary

`fathomdb` exists to serve **local AI agents that maintain an ongoing
relationship with a human and their work**. In this domain, the datastore is
not only a place to put documents. It must help the system remember what
matters, connect related information, support action over time, and remain
inspectable and reversible when the agent is wrong.

The key engine-level need is a datastore that supports a durable world model
with multimodal recall: structured facts, relationships, full-text lookup,
semantic similarity, temporal context, provenance, and operational history. It
must also manage ephemeral operational state that does not belong in the
graph, control the lifecycle of its own metadata, stay safe under bursty
write loads, evolve its schema without risking data loss, and keep its wire
types consistent across every language layer. It must be fast enough for
interactive use, safe enough for high-trust personal workflows, and simple
enough to run locally without infrastructure.

## 2. Who This Serves

### 2.1 The Human Principal

The human relies on the agent to manage sensitive, interconnected personal and
professional information. The human needs:

- strong privacy and local control over data
- fast recall and continuity across sessions and interfaces
- a way to inspect why the agent answered, acted, or stored something
- the ability to correct or undo harmful changes
- confidence that the system will not silently lose or distort important context

### 2.2 The Agent Runtime

The agent is the primary operator of the datastore. It works under token
limits, imperfect reasoning, and pressure to act across many kinds of
information. The agent needs:

- deterministic, programmatic access patterns instead of brittle query strings
- a unified way to work with documents, relationships, semantic similarity,
  full-text lookup, and temporal state
- help with ingestion, indexing, and housekeeping
- a durable place to store intent, actions, observations, corrections, and
  learned context over time

### 2.3 The Application Developer And Operator

The surrounding application needs a datastore that is embeddable, observable,
and maintainable. The developer or operator needs:

- a zero-ops local deployment model
- reliable schema evolution and repair paths
- visibility into failures, degraded modes, and behavioral regressions
- replay and auditability for debugging, evaluation, and trust

## 3. Core Engine Needs

### 3.1 Versioned Graph Storage With Flexible Schemas

The engine must store a graph of typed entities (nodes) and relationships
(edges) with flexible JSONB properties. Applications define their own node and
edge `kind` taxonomy. The engine must support:

- create, update (supersede), and retire operations on nodes and edges
- append-oriented history: prior versions are preserved, not overwritten
- logical identity (`logical_id`) that persists across physical versions
- temporal queries: the graph as it was at any point in time

### 3.2 Multi-Modal Recall

The agent must be able to retrieve and combine:

- structured records and typed facts
- document content and attached metadata
- graph-like relationships between entities
- full-text matches (FTS5)
- semantic similarity matches (vector search)
- temporal context: recency, chronology, session continuity
- provenance: where information came from and how trustworthy it is

This is a core engine need because agent questions are rarely only lexical or
only relational. They combine all of these at once.

### 3.3 Deterministic Agent Ergonomics

Agents do not perform well when forced to invent fragile query dialects or
manually orchestrate multiple stores. The engine must support:

- deterministic, code-friendly interaction patterns (fluent SDK, typed writes)
- clear data shapes and predictable access patterns
- safe defaults for reading, writing, updating, and superseding memory
- a query compiler that prevents agents from generating raw SQL

### 3.4 Provenance On Every Write

Every canonical row must carry a `source_ref` that links it to the execution
context that created it. The engine must support:

- `trace_source`: show everything created by a given source reference
- `excise_source`: remove everything created by a source and restore prior state
- provenance warnings or enforcement when `source_ref` is missing

Without provenance, surgical repair is impossible.

### 3.4.1 Provenance Lifecycle

Appending provenance events forever is not viable. Operators need to purge old
events while preserving specific event types that serve as permanent audit
trails (e.g., excision and purge records). The engine must support:

- time-bounded purge of provenance events
- selective retention: certain event types survive purges regardless of age
- configurable defaults so that safety-critical audit records are never
  accidentally deleted

### 3.5 Reversibility Without Losing History

Autonomous agents make mistakes. The engine must support:

- selective undo via supersession rather than destructive overwrite
- correction that preserves lineage instead of erasing prior state
- excision that reverses bad writes while restoring previously active versions
- time-window rollback for broader semantic reversal

### 3.6 Operational State Management

Agents need to track ephemeral operational state that does not belong in the
versioned graph: connector health checks, scheduler cursors, sync bookmarks,
rate-limit counters, and similar high-churn data. This state has different
retention semantics, different query patterns, and different mutation styles
than canonical graph data. The engine must support:

- a dedicated operational store organized by named collections
- mutation semantics appropriate to collection kind (append-only logs vs.
  latest-state key-value tables)
- filtered reads and secondary indexes within operational collections
- retention and compaction policies independent of the graph's history model
- provenance tracking on operational mutations so they participate in
  `excise_source` and traceability

Forcing operational state into the graph model creates schema pollution,
unbounded version history, and misleading provenance chains.

### 3.7 Automated Housekeeping

The agent should not manage low-level synchronization work manually. The engine
must handle:

- FTS and vector projection synchronization on every canonical write
- startup detection and repair of missing optional projections
- deterministic projection rebuild from canonical state at any time

### 3.8 Recovery From All Corruption Classes

The engine must treat recovery as a first-class capability across three
corruption classes:

- **Physical:** disk, filesystem, or crash-related damage
- **Logical:** derived projection drift or broken virtual-table state
- **Semantic:** bad agent reasoning that poisons the world model

Each class has a distinct detection and recovery path. Physical recovery
restores canonical tables and rebuilds projections. Logical recovery rebuilds
projections from canonical state. Semantic recovery uses `excise_source` to
remove bad data surgically.

### 3.9 Local-First, Zero-Ops Operation

The engine must run on a developer's machine or small server without:

- external infrastructure or network dependencies
- heavyweight operational burden
- high resource footprint that conflicts with local inference

A single SQLite file is the deployment unit. Moving it is the backup plan.

### 3.10 Write Safety Under Load

Agents may burst large volumes of writes during ingestion, backfill, or
catch-up synchronization. The engine must remain safe under load:

- bounded write channel depth so callers experience back-pressure rather than
  unbounded memory growth
- per-request size limits on nodes, edges, chunks, vectors, and operational
  mutations so that a single oversized request cannot destabilize the writer
- recovery from internal writer failures (including panics) without losing
  queued data or hanging callers

### 3.11 Schema Evolution Safety

As the engine evolves, databases created by newer versions will contain
migrations that older engines do not understand. Opening such a database must
fail cleanly with a clear version-mismatch error rather than silently operating
on a schema it cannot interpret. The engine must:

- record applied migration versions in the database
- compare the database's highest migration against the engine's known set at
  open time
- reject the database with a descriptive error when the database is ahead of
  the engine

### 3.12 Cross-Layer Type Safety

The engine is accessed from multiple languages — Rust (core), Python
(application harness), and Go (integrity tooling). Wire types that cross these
boundaries (write requests, admin commands, read reports, export manifests) must
stay in sync. The engine must support:

- a single source of truth for wire-format structures, with derived types in
  each language layer
- CI-enforced version consistency checks across Cargo and Python package
  metadata
- integration tests in each language layer that verify request and response
  shapes against the engine's expectations

Drift between language layers produces silent data corruption or opaque runtime
failures that are difficult to diagnose in production.

## 4. Non-Functional Needs

- **Privacy and locality:** data lives on the user's device or private
  infrastructure by default.
- **Interactive speed:** recall and writes must be responsive for conversational
  use.
- **Reliability:** failures are visible and recoverable, not silent or
  corrupting.
- **Portability:** the datastore moves cleanly between common local
  environments.
- **Low resource footprint:** coexists with local inference and normal
  application workloads.
- **Scalable enough for personal knowledge growth:** remains practical as years
  of agent memory accumulate.

## 5. Memory Layers The Engine Must Support

Not all data should be treated the same. The engine must allow applications to
distinguish between:

- ephemeral turn state (not persisted or short-lived)
- session continuity state (persisted for the duration of a session)
- durable semantic memory (long-lived, versioned, provenance-linked)
- correction history (preserved for audit and replay)

This is necessary to avoid both amnesia and unbounded memory pollution. The
engine provides the versioning and temporal primitives; the application defines
its own retention policy.

## 6. Failure Modes The Engine Must Make Visible Or Recoverable

- harmful or hallucinated writes with no practical undo path
- missing or distorted provenance for facts or observations
- silent loss of synchronization between canonical tables and derived projections
- poor retrieval of relevant context across text, relationships, semantics, and
  time
- history being overwritten instead of corrected transparently
- background ingestion failures that disappear without reviewable traces

## 7. Summary

The engine need is a **local, high-trust datastore for persistent AI agents**.
It must provide versioned graph storage, multimodal recall, deterministic agent
ergonomics, provenance on every write with managed lifecycle, reversibility
without history loss, a dedicated operational store for high-churn state,
automated housekeeping, recovery from all corruption classes, write safety
under load, safe schema evolution, and cross-layer type consistency.

If it only stores documents or only accelerates search, it does not solve the
real problem.

---

# Part 2: Application-Layer Needs (Personal Agent Example)

The needs below describe what a personal AI agent application — one that manages
the human's professional and personal life — requires from an application built
on top of fathomdb. These are **not engine requirements**. They should be
implemented using the engine's graph backbone, provenance primitives, and query
compiler.

They are documented here so that fathomdb's design can be validated against a
concrete consumer, not because the engine will provide these directly.

## 8. Personal Agent Application Needs

### 8.1 Meeting Intelligence

The application needs to ingest meeting transcripts, extract participants,
decisions, commitments, and follow-up tasks, and link them to the relevant
people, projects, and goals already in the world model.

*Engine primitives used:* nodes with `kind = "meeting"`, edges for
relationships, FTS for transcript search, vector search for semantic retrieval,
`source_ref` for excision if a meeting is erroneous.

### 8.2 Scheduling, Reminders, And Background Work

The application needs to schedule reminders, track deadlines, and run
background workflows. It needs to review blocked items and escalate when
necessary.

*Engine primitives used:* nodes with `kind = "scheduled_task"`, edges for
dependencies, temporal queries for due-date filtering.

### 8.3 Approval And Review Workflows

The application needs human approval for high-impact actions. Proposals should
be stored, reviewed, and either committed or rejected without losing the
decision record.

*Engine primitives used:* nodes with `kind = "proposal"` and a status property,
`source_ref` for traceability, supersession for committed vs. rejected state.

### 8.4 Prompt Control And Governance

The application needs to record how requests were interpreted, which policies
were applied, and what actions were taken or suppressed, so the human can audit
agent behavior.

*Engine primitives used:* nodes with `kind = "intent_frame"` or
`kind = "control_artifact"`, linked to runs and steps via `source_ref` and
edges.

### 8.5 Evaluation And Behavioral Regression Detection

The application needs to compare agent behavior across versions and label
failures for debugging and rubric-based evaluation.

*Engine primitives used:* nodes with `kind = "eval_record"`, edges linking
evaluations to actions or runs, temporal queries for before/after comparisons.

### 8.6 Multi-Source Ingestion

The application ingests from notes, URLs, files, email, calendars, and system
events. It needs to normalize these into the world model and index them for
retrieval.

*Engine primitives used:* generic nodes with appropriate `kind` values, chunks
and FTS for text, vector projections for semantic search, `source_ref` for
origin tracking.
