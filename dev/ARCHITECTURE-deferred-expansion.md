# ARCHITECTURE-deferred-expansion.md

## Purpose

This file documents the engine-application boundary concretely and shows how
domain patterns that applications need can be implemented using fathomdb's
existing graph, versioning, and provenance primitives — without requiring engine
schema changes.

`ARCHITECTURE.md` §2.2 draws a firm line: `runs`, `steps`, and `actions` are
the ceiling of engine-owned typed tables. Everything above that line is
application domain logic. This file shows how to build those patterns on top of
the engine.

---

## 1. Why Domain Concepts Stay Out Of The Engine

Each engine-owned table is a schema the engine must migrate, version, and
support in perpetuity. Every new domain concept adds:

- a migration every user must run, even if their application doesn't use it
- a typed SDK surface the engine must maintain
- an impedance mismatch for applications with a different domain model

Applications using fathomdb will have different node taxonomies, different
execution models, and different domain concepts. A meeting-centric agent, a
code-review agent, and a research agent share the same storage and provenance
primitives. They should not share a schema dictated by one application's domain.

The engine's job is to store versioned graph data with provenance. The
application's job is to define what that data means.

---

## 2. Application Domain Patterns

The following patterns cover domain concepts previously tracked as potential
engine table candidates. Each shows how to implement the concept using nodes,
edges, and JSONB properties.

### 2.1 Meeting And Event Artifacts

Model meetings as nodes with `kind = "meeting"`. Participants, decisions,
commitments, and follow-ups become edges and child nodes.

```
node kind="meeting"    properties: {title, started_at, ended_at, status}
  edge kind="attended_by"  → node kind="person"
  edge kind="produced"     → node kind="commitment"
  edge kind="produced"     → node kind="action_item"
```

FTS and vector projections over meeting chunks work identically to any other
node kind. Temporal queries work the same way. `source_ref` on the meeting node
makes the entire cluster traceable and excisable.

### 2.2 Approval And Proposal Workflows

The engine's visibility model is simple: active, superseded, deleted. Proposal
and review states belong in application JSONB properties or SDK models.

```
node kind="proposal"  properties: {status: "pending|approved|rejected", ...}
```

A `source_ref` pointing to the run or step that created the proposal makes it
traceable and excisable if rejected. The engine does not need to understand
approval states. It stores and retrieves nodes. The application enforces its own
workflow logic.

### 2.3 Intent Frames And Control Artifacts

Prompt interpretation, routing decisions, and response contracts are
semantically important records but not engine concepts. Store them as nodes:

```
node kind="intent_frame"      properties: {input_summary, route, policy, ...}
node kind="control_artifact"  properties: {type, contract, ...}
```

Link them to the run or step they came from via edges or `source_ref`.

### 2.4 Evaluation Records And Comparison Runs

Evaluation labels, rubric scores, and failure annotations are application data:

```
node kind="eval_record"  properties: {score, rubric, label, run_id, ...}
edge kind="evaluates"    → node kind="action" (or run, step, as appropriate)
```

The engine's temporal model lets queries reconstruct behavior at any point in
time. Comparison between two runs is a join through edges, not an engine
feature.

### 2.5 Scheduling And Reminders

Scheduled work, reminders, and deadlines are nodes:

```
node kind="scheduled_task"  properties: {due_at, status, recurrence, ...}
edge kind="depends_on"      → node kind="task"
```

Completion, deferral, and cancellation are modeled as supersession or status
changes in JSONB properties. No engine-level queue table is needed for
application scheduling semantics.

---

## 3. Persistent Queue Table (Deferred, Possible)

A persistent SQLite queue table inside the engine (e.g., `embedding_jobs`) for
optional semantic backfills remains a plausible addition if the product later
needs crash-resilient background job recovery with explicit introspection of
queued and failed work.

It is deferred from v1 because a durable queue inside the same SQLite file
competes with the interactive writer path for the write lock.

Current direction:
- use an in-memory async queue for optional semantic projection work
- repair missing projections via startup rebuild checks (`rebuild_missing_projections`)

---

## 4. Multi-Database Sharding (Future Scale-Out)

fathomdb's v1 model is single-file. If write throughput ever becomes a
bottleneck, the correct scale-out path is **sharding by workspace or agent
identity** (multiple database files), not multi-writer merge semantics.

CRDT-style multi-writer sync requires relaxing `PRAGMA foreign_keys`, partial
unique indexes, and uniqueness constraints beyond primary key. fathomdb's
corruption-class detection and provenance tracing depend on all of these. They
are not optional.

Shard-by-workspace preserves the single-writer model and all integrity
guarantees within each shard. It is the only scale-out path compatible with
fathomdb's integrity model.

---

## 5. What This File Is NOT

This file is not a list of features the engine will add later. The domain
patterns above should be implemented in application code, not in future engine
migrations.

If a pattern becomes so universally needed and so performance-critical that it
genuinely belongs in the engine, that case must be argued explicitly against the
cost framework in §1: migration burden on all users, schema lock-in, and
impedance mismatch with applications that don't use that concept.

Use [ARCHITECTURE.md](./ARCHITECTURE.md) for implementation-driving decisions.
