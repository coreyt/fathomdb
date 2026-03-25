# Engine vs. Application Boundary

## The Question

How does fathomdb remain a generally-valuable database for local personal agents
without making too many design decisions for agent application developers?

Where should the line fall between "engine primitives" and "application-level
world model"?

---

## Current State Of The Boundary

The existing design draws the boundary inconsistently. Some parts are clearly
engine-level primitives. Others encode a specific agent application's world
model (Memex's) into the engine itself.

### Clearly engine-level

These are storage, query, and lifecycle primitives that any local agent
datastore would need regardless of the application built on top:

- SQLite as canonical store
- `nodes` table with flexible `kind` and JSONB `properties`
- `edges` table with flexible `kind` and JSONB `properties`
- `chunks` table linking text content to nodes
- `logical_id` / `row_id` versioning
- `superseded_at` append-oriented history
- `source_ref` provenance linkage on canonical rows
- Derived FTS5 and sqlite-vec projections
- Single-writer discipline with WAL-backed readers
- Query compiler with inside-out planning
- Projection rebuild, integrity checks, safe export

These primitives don't assume what the agent does, what domain it operates in,
or how its execution is structured. They provide a graph with versioning,
multimodal search, and provenance.

### Currently engine-level but arguably application-level

#### `runs`, `steps`, `actions` as engine-owned typed tables

These three tables encode a specific model of agent execution:

- A **run** is a session-level container
- A **step** is a prompt/control/LLM stage within a run
- An **action** is a concrete tool call or emitted outcome within a step

This is one valid execution model, but not the only one. Different agent
frameworks structure execution differently:

- Some have flat event streams with no nesting
- Some have deeper hierarchies (run → task → subtask → tool call → retry)
- Some have DAG-structured execution, not a linear run → step → action chain
- Some have no concept of "steps" at all

By making these engine-owned tables, fathomdb forces every application to either
adopt this execution model or ignore the typed tables and store execution data
as generic nodes instead.

#### The deferred expansion tables

`ARCHITECTURE-deferred-expansion.md` explicitly plans to promote these into
engine tables:

- `intent_frames`
- `control_artifacts`
- `meeting_artifacts`
- `evaluation_records`
- `approvals`
- `scheduler_runs`

These are Memex domain concepts. A general-purpose agent datastore should not
have engine-level opinions about what a "meeting artifact" or "intent frame" is.

#### `USER_NEEDS.md` scope

The user needs document is written from a specific application's perspective:
meetings, email, calendars, scheduling, approval workflows. These are valid
needs for *one* agent application, but they are not universal needs of all
agent datastores.

A general-purpose engine's user needs are closer to:

- store versioned graph data with flexible schemas
- search across text, vectors, and structure
- track provenance of every write
- support undo/correction without losing history
- run locally with low operational burden
- recover from corruption at all layers

---

## The Core Design Tension

fathomdb's graph backbone is already general-purpose. An application can model
any domain by choosing `kind` values for nodes and edges and putting domain
data in JSONB `properties`. The engine doesn't need to know what a "meeting" or
"task" or "approval" is.

The tension arises when the engine adds **typed tables** for domain concepts.
Each typed table is a bet that all (or most) applications share that concept.
The more typed tables the engine owns, the more opinionated it becomes.

### Why typed tables are attractive

1. **Query performance.** Typed columns are faster to filter than JSONB
   extraction. Expression indexes help, but explicit columns are cheaper.

2. **Provenance anchor.** `source_ref` needs something to point to. If it
   points to a node with `kind = "action"`, the engine can still trace and
   excise. But if `source_ref` semantics depend on the referent having specific
   typed columns (like `status` or `step_id`), then the engine needs to own
   that schema.

3. **Integrity constraints.** Typed tables can enforce referential integrity
   that JSONB properties cannot.

4. **SDK ergonomics.** Typed tables produce cleaner SDK models than generic
   node-with-kind patterns.

### Why typed tables are dangerous for generality

1. **Schema lock-in.** Every engine-owned table is a schema the engine must
   migrate, version, and maintain forever.

2. **Impedance mismatch.** Applications that don't fit the engine's execution
   model must either shoehorn their data or ignore the typed tables entirely.

3. **Feature creep.** The deferred expansion list grows toward a full
   application framework, not a database engine.

4. **Migration burden.** New engine tables require schema migrations that
   every user must run, even if their application doesn't use those tables.

---

## Proposed Boundary

### Principle: the engine owns storage primitives; applications own domain semantics

The engine should provide:

1. **A flexible graph backbone** — nodes, edges, chunks with JSONB properties
   and flexible `kind` values. This is already in place.

2. **Versioning and provenance primitives** — logical_id/row_id, supersession,
   source_ref. This is already in place.

3. **Derived search projections** — FTS, vector, with rebuild and integrity
   checks. This is already in place.

4. **A minimal provenance anchor** — something for `source_ref` to point to
   that doesn't assume a specific execution model.

5. **Query, write, and recovery operations** — compiler, writer, admin surface.
   This is already in place.

The engine should NOT provide:

- Typed tables for domain concepts (meetings, intents, approvals, evaluations)
- A specific agent execution hierarchy (run → step → action)
- Workflow state machines (proposed → approved → rejected)
- Domain-specific promotion rules (observation → knowledge → commitment)

### What happens to `runs`, `steps`, `actions`?

There are two viable paths:

#### Option A: Demote to application-level nodes

`runs`, `steps`, and `actions` become nodes with `kind = "run"`,
`kind = "step"`, `kind = "action"`. The engine treats them the same as any
other node. Applications that want the run → step → action hierarchy model
it with edges. Applications that don't, don't.

`source_ref` points to a `logical_id` of any node. The engine's trace and
excise operations follow `source_ref` to find the referent node and then find
all other nodes/edges with the same `source_ref`.

**Pros:**
- Engine stays maximally general
- No schema migration when applications use different execution models
- Fewer engine-owned tables to maintain

**Cons:**
- No referential integrity between execution records and source_ref
- Query performance for provenance joins depends on JSONB extraction or
  expression indexes
- SDK ergonomics for execution tracking are weaker

#### Option B: Keep one thin provenance-anchor table

Replace `runs`, `steps`, and `actions` with a single `operations` table:

```sql
CREATE TABLE operations (
    row_id TEXT PRIMARY KEY,
    logical_id TEXT NOT NULL,
    kind TEXT NOT NULL,          -- application-defined
    properties BLOB NOT NULL,   -- JSONB, application-defined
    parent_op TEXT,              -- optional, for nesting
    created_at INTEGER NOT NULL,
    superseded_at INTEGER,
    source_ref TEXT
);
```

This gives `source_ref` a typed anchor with referential integrity, allows
applications to define their own execution hierarchy through `kind` and
`parent_op`, and avoids baking in a specific three-level model.

**Pros:**
- Engine provides provenance infrastructure without dictating execution model
- Applications define their own `kind` taxonomy (run, step, action, task,
  retry, sub-agent-call, whatever)
- Nesting depth is application-controlled via `parent_op`
- Referential integrity for `source_ref` is preserved
- One table to migrate, not three

**Cons:**
- Less structured than three dedicated tables
- Applications must enforce their own hierarchy conventions
- Slightly more work for applications that do want the run/step/action model

#### Recommendation: Option B

A single `operations` table is the right trade-off. It preserves the provenance
infrastructure that makes trace/excise/rebuild work, without dictating an
execution model. Applications that want run → step → action can use
`kind = "run"`, `kind = "step"`, `kind = "action"` and link them via
`parent_op`. Applications that want flat event streams just use
`kind = "event"` with no `parent_op`.

### What happens to the deferred expansion tables?

They should be removed from the engine's roadmap entirely.

`intent_frames`, `meeting_artifacts`, `control_artifacts`, `evaluation_records`,
`approvals`, and `scheduler_runs` are application-level domain concepts. They
belong in application code that uses fathomdb's graph backbone to store and
query them.

If an application needs typed columns for performance on hot query paths, the
engine already supports expression indexes on JSONB properties. That is the
right mechanism — it doesn't require engine schema changes to support new
domain concepts.

The deferred expansion doc can be reframed: instead of "tables the engine will
add later," it becomes "domain patterns that applications can implement using
the engine's primitives."

### What happens to `USER_NEEDS.md`?

It should be split into two documents:

1. **Engine-level needs** — what any local agent datastore must provide
   (versioned graph, multimodal search, provenance, recovery, local-first
   operation).

2. **Memex-specific needs** — what Memex as an application needs from the
   engine (meetings, scheduling, email integration, approval workflows).

The engine's design should be driven by the first document. The second document
is a valid use case but should not shape engine primitives.

---

## What The Engine Provides To Applications

Under this boundary, fathomdb's value proposition to application developers is:

**You get a local SQLite-backed datastore with:**

- A versioned graph (nodes + edges) with flexible JSONB properties
- Derived FTS and vector search over your graph data
- An append-oriented history model with provenance tracking
- A query compiler that handles multimodal queries efficiently
- Recovery from physical, logical, and semantic corruption
- A single-writer model with concurrent readers
- One file, zero ops

**You decide:**

- What your node and edge `kind` taxonomy looks like
- How your agent execution model works
- What domain semantics your application has
- What workflow states and transitions you need
- What counts as "knowledge" vs "ephemeral" vs "operational"

**The engine helps you with:**

- Trace: "show me everything created by this operation"
- Excise: "remove everything created by this operation and restore prior state"
- Rebuild: "reconstruct search indexes from canonical data"
- Temporal queries: "show me the graph as it was at time T"

**The engine does NOT impose:**

- A specific execution model (run/step/action vs flat events vs DAGs)
- Domain-specific typed tables (meetings, approvals, evaluations)
- Workflow state machines
- Promotion rules or lifecycle policies

---

## Migration Path From Current Design

1. **Merge `runs`, `steps`, `actions` into a single `operations` table** with
   `kind`, `parent_op`, and JSONB `properties`. Update `source_ref` to point
   to `operations.logical_id`.

2. **Reframe `ARCHITECTURE-deferred-expansion.md`** from "tables the engine
   will add" to "domain patterns applications can build on the engine."

3. **Split `USER_NEEDS.md`** into engine-level needs and application-level
   needs.

4. **Keep the Memex gap map** as a valid consumer analysis, but stop treating
   it as an engine requirements document.

5. **Update the README** to emphasize the engine's general-purpose nature and
   the application developer's freedom to define domain semantics.

---

## What This Does NOT Change

- The graph backbone (nodes, edges, chunks) stays exactly as designed.
- The query compiler stays exactly as designed.
- The write pipeline stays exactly as designed.
- The projection system stays exactly as designed.
- The recovery model stays exactly as designed.
- The single-writer model stays exactly as designed.
- The Rust/Go/Python/TypeScript language split stays exactly as designed.

The change is in where the engine stops and the application begins.
