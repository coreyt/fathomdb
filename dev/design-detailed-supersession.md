# Design: Supersession in the Typed Write Path

## Purpose

This document designs the full supersession story for fathomdb's Rust engine.

"Supersession" is the mechanism by which canonical rows are updated, replaced, or
retired without destroying history. It is a first-class engine primitive, not a
convenience wrapper around raw SQL.

This document builds directly on the typed write design in
[design-typed-write.md](./design-typed-write.md) and addresses the open item:

> Add append-oriented supersession support for versioned updates.

---

## Current State

The typed write path already has a minimal supersession primitive:

```rust
pub struct NodeInsert {
    pub row_id: String,
    pub logical_id: String,
    ...
    pub upsert: bool,  // supersede active row, then insert new row
}
```

`EdgeInsert` has the same `upsert: bool` field.

When `upsert = true`, `apply_write` executes:

```sql
UPDATE nodes SET superseded_at = unixepoch()
WHERE logical_id = ?1 AND superseded_at IS NULL;
-- followed by INSERT of the new row
```

Both operations are inside the same `BEGIN IMMEDIATE` transaction, so they are atomic.

### What is missing

The current primitive covers only one use case: replace an existing node with a new
version. The following are unresolved:

1. **Chunk lifecycle on node supersession.** When a node is replaced, old chunks and
   their FTS rows remain in the database. FTS queries can return stale text from
   superseded node versions.

2. **Retire without replacement.** There is no typed operation for "remove this row
   without writing a successor." This is needed for soft-delete, bad data excision, and
   completed-run cleanup.

3. **Runtime table updates.** `RunInsert`, `StepInsert`, and `ActionInsert` have no
   `upsert` flag. Status transitions (e.g., `active → completed`) cannot be expressed.

4. **Chunk-only update.** Callers cannot update the text content of a node's chunks
   without re-submitting the node itself.

5. **No explicit cascade policy.** When a node is superseded, its edges are not
   automatically superseded. This is currently undefined behavior, not a deliberate
   policy.

6. **No explicit ID-generation guidance.** Callers must generate `row_id` values for
   new superseding rows, but there is no engine-provided utility for this.

---

## What Supersession Means in fathomdb

### Core invariant

At any point in time, at most one row per `logical_id` may be active:

```
superseded_at IS NULL  ←→  active
superseded_at IS NOT NULL  ←→  historical
```

This invariant is enforced by a partial unique index:

```sql
CREATE UNIQUE INDEX idx_nodes_active_logical_id
    ON nodes(logical_id)
    WHERE superseded_at IS NULL;
```

The same index exists on `edges`.

### Append-oriented history

Supersession is always append-oriented. Old rows are never deleted or mutated (except
to set `superseded_at`). This means:

- Full history is preserved for audit, excision, and repair
- Provenance chains (`source_ref`) remain intact
- `trace_source` and `excise_source` work correctly on historical rows

### What supersession is NOT

- Supersession is **not a cascade**: superseding a node does not automatically
  supersede its edges or chunks.
- Supersession is **not a foreign-key enforcement mechanism**: the engine does not
  validate that superseded endpoints still have an active successor when edges are
  traversed.
- Supersession of nodes does **not guarantee FTS consistency** automatically: stale FTS
  rows from old chunks are not removed unless the write explicitly requests it.

---

## Operation Types

The current `upsert: bool` flag on `NodeInsert` and `EdgeInsert` blends intent and
mechanism. The full supersession surface needs three distinct operations:

| Operation | Description | Current support |
|---|---|---|
| **Insert** | New row; no prior active row for this `logical_id` | ✅ `upsert: false` |
| **Replace** | Supersede active row and insert a new version atomically | ✅ `upsert: true` |
| **Retire** | Supersede active row without inserting a replacement | ❌ missing |

These three operations cover all typed supersession needs:

- **Insert** is used for new knowledge, new runs, new edges.
- **Replace** is used for content updates, property corrections, edge rewires.
- **Retire** is used for soft-delete, completed-run archival, bad-data removal.

### Why not a separate struct per operation

Using a single struct with an operation field keeps the call site uniform:

```rust
pub enum NodeOperation {
    Insert,
    Replace,  // was: upsert: true
    Retire,
}
```

This is cleaner than three separate structs because the field set is mostly shared
and because `WriteRequest` would otherwise need three separate `Vec`s per type.

### Replace vs. Retire distinction

Both operations call the same SQL to supersede the active row:

```sql
UPDATE nodes SET superseded_at = unixepoch()
WHERE logical_id = ?1 AND superseded_at IS NULL
```

The difference is what follows:

- **Replace**: the new row fields are provided; INSERT follows immediately in the same
  transaction.
- **Retire**: no new row is inserted. The `row_id` and other new-row fields in the
  struct are `None` or absent.

To keep the struct symmetric, a `Retire` can be expressed as a `NodeInsert` where
`operation = NodeOperation::Retire` and the new-row fields (`row_id`, `properties`) are
empty. Alternatively, a separate `NodeRetire { logical_id, source_ref }` struct avoids
filling dummy fields.

**Decision**: Use a separate `NodeRetire` (and `EdgeRetire`) struct. It is
unambiguous, its fields are exactly what is needed, and it avoids a partial-init
footgun on `NodeInsert`.

```rust
pub struct NodeRetire {
    pub logical_id: String,
    pub source_ref: Option<String>,
}

pub struct EdgeRetire {
    pub logical_id: String,
    pub source_ref: Option<String>,
}
```

`source_ref` on a retire operation is recorded as a note in the supersession event
(see Retire Provenance below).

---

## Chunk Lifecycle

### The problem

Chunks are attached to nodes via `node_logical_id`. They have no `superseded_at` of
their own. When a node is replaced:

- Old chunks remain in the database with the same `node_logical_id`.
- Old FTS rows referencing those old chunks also remain.
- FTS queries match both old and new text for the same `logical_id`.

This is a silent correctness problem. The FTS index becomes stale immediately after
any node replace that includes new text.

### Design: replace operation carries a chunk replace policy

When a node is replaced, the caller declares one of two chunk policies:

```rust
pub enum ChunkPolicy {
    /// Keep existing chunks untouched.
    /// Use when the replacement changes only node properties, not text content.
    Preserve,

    /// Delete all chunks and FTS rows for the node's logical_id, then insert
    /// the new chunks from this request.
    /// Use when the replacement changes the canonical text content.
    Replace,
}
```

The default is `Preserve` so that the common case (property-only update) does not
incur chunk churn.

When `ChunkPolicy::Replace` is specified, `apply_write` executes, inside the same
`IMMEDIATE` transaction:

```sql
-- Step 1: delete FTS rows for all chunks of this node
DELETE FROM fts_nodes WHERE node_logical_id = ?1;

-- Step 2: delete the old chunks
DELETE FROM chunks WHERE node_logical_id = ?1;

-- Step 3: supersede the active node row
UPDATE nodes SET superseded_at = unixepoch()
WHERE logical_id = ?1 AND superseded_at IS NULL;

-- Step 4: insert the new node row
INSERT INTO nodes ...;

-- Step 5: insert new chunks
INSERT INTO chunks ...;

-- Step 6: insert new FTS rows (derived by resolve_fts_rows as usual)
INSERT INTO fts_nodes ...;
```

Steps 1 and 2 must precede the node supersession so that a partial failure (e.g.,
connection lost after supersession but before chunk delete) can be detected by
`check_semantics` as stale FTS rows against a now-superseded node.

### Why delete old chunks rather than mark them superseded

Chunks have no `superseded_at` column. Adding one would require a schema migration and
would complicate FTS queries (which currently do not need to filter chunks by
supersession state). Deleting old chunks on explicit replace keeps the schema clean and
the FTS index accurate.

This is safe because chunk history is preserved via node history: the old node row
(with its `superseded_at` timestamp) still exists, and `trace_source` can reconstruct
what chunks existed at any point by joining against the old node `row_id`. If forensic
chunk-level history is needed in the future, a `chunk_superseded_at` column can be
added in a migration.

### Retire and chunk lifecycle

When a node is retired, chunks should be deleted by default. Retaining orphaned chunks
(chunks whose node has no active row) is the failure mode detected by `check_semantics`
as `orphaned_chunks`. Retire therefore defaults to `ChunkPolicy::Replace` with an
empty chunk list, effectively deleting all chunks and FTS rows for the node.

---

## FTS Projection Consistency

### Invariant

The FTS index should contain exactly the chunks of currently-active nodes. After any
write that supersedes a node, the FTS index must reflect the new state.

### How this is enforced

The `ChunkPolicy` mechanism above handles this atomically. When `Replace` is specified:

1. Old FTS rows are deleted inside the transaction.
2. New FTS rows are inserted inside the transaction.

When `Preserve` is specified, the old FTS rows remain. This is correct only when the
text content is unchanged. It is the caller's responsibility to choose the right policy.

### FTS rows after a `Retire`

When a node is retired with no replacement, all chunks and FTS rows for that
`node_logical_id` are deleted. After a successful retire, `check_semantics` should
report zero orphaned chunks and zero stale FTS rows for this node.

### Projection rebuild still works

The `rebuild_fts` and `rebuild_missing_projections` operations in `ProjectionService`
do not need to change. They operate on the `chunks`/`nodes` join with
`superseded_at IS NULL`, which naturally excludes chunks from superseded nodes once
those chunks are deleted.

---

## Runtime Table Updates

`runs`, `steps`, and `actions` each have a `superseded_at` column but no `upsert`
support in the current typed write path.

### Use case

Runs transition through status states: `active → completed`, `active → failed`.
Without an update path, callers cannot express these transitions through the engine.
The workaround today would be raw SQL, which the design goal explicitly forbids.

### Design

Add `upsert: bool` to `RunInsert`, `StepInsert`, and `ActionInsert`. This is
consistent with the `NodeInsert`/`EdgeInsert` pattern.

When `upsert = true`, the writer supersedes the active row with the same `id` before
inserting the new one:

```sql
UPDATE runs SET superseded_at = unixepoch()
WHERE id = ?1 AND superseded_at IS NULL;
INSERT INTO runs ...;
```

**No retire operation for runtime tables.** Runs, steps, and actions are event-stream
records. They are retired implicitly by recording a terminal status (`completed`,
`failed`, `cancelled`). There is no need for a `RunRetire` struct: callers use
`upsert = true` with a terminal status to close a run.

**No chunk lifecycle for runtime tables.** Runtime tables do not drive FTS projections.
Upsert for runtime tables is simpler than for nodes: just supersede and re-insert.

### Updated structs

```rust
pub struct RunInsert {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
    pub upsert: bool,  // new field
}

pub struct StepInsert {
    pub id: String,
    pub run_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
    pub upsert: bool,  // new field
}

pub struct ActionInsert {
    pub id: String,
    pub step_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
    pub upsert: bool,  // new field
}
```

---

## Cascade Policy

### Explicit non-cascade

Superseding a node does **not** automatically supersede its edges. This is a
deliberate policy, not an oversight:

- Historical edges are part of the provenance record.
- An edge between a superseded node version and another node is still historically
  true: the relationship existed at that point in time.
- Automatic cascade would silently destroy relationship history.

### Active edges pointing to superseded nodes

After a node retire (or replace without edge update), edges whose
`source_logical_id` or `target_logical_id` pointed to the old logical_id still exist
and remain active. When the traversal query joins `edges` against `nodes` with
`superseded_at IS NULL`, those edges are still traversable if the endpoint node has an
active successor (because the join is on `logical_id`, not `row_id`).

Edge traversal in the query compiler joins on `logical_id`:

```sql
JOIN edges e ON e.source_logical_id = t.logical_id
    AND e.kind = ?
    AND e.superseded_at IS NULL
```

This means existing active edges to a `logical_id` automatically point to the new
active node version after a replace. This is correct behavior: the edge relationship
persists across node versions.

When a node is **retired** (no replacement), edges to that `logical_id` still exist
but their endpoint no longer has an active row. These become dangling edges in the
logical sense. `check_semantics` should eventually detect them (not yet implemented;
see the open items below).

### Caller responsibility on retire

When a node is retired, callers are responsible for deciding what to do with its edges:

- If the edges should also end, retire them explicitly in the same `WriteRequest`.
- If the edges should persist as historical records, leave them.

The engine does not make this decision automatically.

---

## Retire Provenance

When a row is retired, the `superseded_at` timestamp is set on the old row. The retire
operation should carry a `source_ref` so the engine can record *why* the row was
retired. This maps to a note in `provenance_warnings` on the `WriteReceipt` if the
retire has no `source_ref`.

The retire's `source_ref` is not stored in a separate event table (no such table
exists in v1). It is only used for `provenance_warnings`. Future versions can add a
`retire_events` table if audit trail depth is needed.

---

## Updated WriteRequest Shape

```rust
pub struct WriteRequest {
    pub label: String,
    pub nodes: Vec<NodeInsert>,
    pub node_retires: Vec<NodeRetire>,    // new
    pub edges: Vec<EdgeInsert>,
    pub edge_retires: Vec<EdgeRetire>,    // new
    pub chunks: Vec<ChunkInsert>,
    pub runs: Vec<RunInsert>,
    pub steps: Vec<StepInsert>,
    pub actions: Vec<ActionInsert>,
    pub optional_backfills: Vec<OptionalProjectionTask>,
}
```

`NodeInsert` gains `chunk_policy: ChunkPolicy` (only inspected when `upsert = true`).
Runtime insert structs gain `upsert: bool`.

---

## Transaction Discipline

All supersession operations remain inside one `BEGIN IMMEDIATE` transaction per
`WriteRequest`. The execution order inside `apply_write` for a request containing
mixed inserts, replaces, and retires is:

1. Process `node_retires` in order:
   - Delete FTS rows for node's `node_logical_id`
   - Delete chunks for node's `node_logical_id`
   - `UPDATE nodes SET superseded_at = unixepoch() WHERE logical_id = ? AND superseded_at IS NULL`
2. Process `edge_retires` in order:
   - `UPDATE edges SET superseded_at = unixepoch() WHERE logical_id = ? AND superseded_at IS NULL`
3. Process `nodes` (insert and replace) in order:
   - If `upsert = true` and `chunk_policy = Replace`:
     - Delete FTS rows for node's `node_logical_id`
     - Delete chunks for node's `node_logical_id`
   - If `upsert = true`: `UPDATE nodes SET superseded_at = unixepoch() ...`
   - INSERT new node row
4. Process `edges` (insert and replace) in order:
   - If `upsert = true`: `UPDATE edges SET superseded_at = unixepoch() ...`
   - INSERT new edge row
5. Process `runs` in order: upsert if `upsert = true`, then INSERT
6. Process `steps` in order: upsert if `upsert = true`, then INSERT
7. Process `actions` in order: upsert if `upsert = true`, then INSERT
8. Process `chunks` in order: INSERT (unchanged)
9. Process `required_fts_rows` (derived by `resolve_fts_rows`): INSERT (unchanged)
10. `COMMIT`

Retire operations are processed before insert/replace operations so that a request
cannot retire a logical_id and re-insert it in a way that leaves the partial state
ambiguous mid-transaction. The engine always resolves "what was active before this
transaction" cleanly before applying new rows.

---

## ID Generation

The v1 policy is: **callers own IDs**.

Callers provide `row_id` (for nodes and edges) and `logical_id`. This keeps the engine
stateless with respect to ID sequencing and is appropriate for agents that already
have stable identifiers (conversation IDs, document hashes, run UUIDs).

The engine should provide a standalone ID helper as a utility, not as a write-path
requirement:

```rust
/// Generate a new row_id suitable for use as a NodeInsert or EdgeInsert row_id.
/// Format: 26-character lexicographically sortable identifier (e.g., ULID or prefixed UUID).
pub fn new_row_id() -> String { ... }
```

This is a free function in `fathomdb-engine`, not a method on `WriterActor`. Callers
that already have stable IDs are not required to use it.

---

## Design Questions Resolved

| Question | Decision |
|---|---|
| Separate `Retire` struct or flag on existing struct? | Separate `NodeRetire` / `EdgeRetire` structs — no dummy fields, unambiguous |
| Chunk delete vs. chunk `superseded_at`? | Delete on `Replace` policy — keeps schema clean, FTS accurate |
| Default chunk policy on node replace? | `Preserve` — property-only updates are the common case |
| Default chunk policy on node retire? | `Replace` (empty list) — retiring a node must not leave orphaned chunks |
| Runtime table upsert? | Add `upsert: bool` to `RunInsert`, `StepInsert`, `ActionInsert` |
| Runtime table retire? | Not needed — terminal status via upsert is sufficient |
| Cascade on node supersession? | No automatic cascade — explicit at call site |
| ID generation? | Caller-owned in v1; engine provides a utility helper, not a requirement |
| Transaction order when mixing inserts, replaces, retires? | Retires first, then replaces, then inserts |

---

## Open Items (Not In This Design)

- **Edge validation on node retire.** When a node is retired, active edges whose
  endpoint is that `logical_id` become logically dangling. `check_semantics` should
  detect these. This is a Layer 3 addition to the Go integrity tool, not a write-path
  change.
- **Chunk-level history.** If forensic chunk-level versioning is needed (e.g., "what
  text was associated with this node at time T"), a `chunk_superseded_at` migration and
  a `ChunkPolicy::Archive` option can be added. Out of scope here.
- **Bulk retire by source_ref.** `excise_source` in `AdminService` already handles
  bulk removal by provenance. It operates outside the typed write path and is not
  affected by this design.
- **Confidence field.** `nodes` and `edges` have a `confidence REAL` column that no
  typed insert struct exposes yet. This remains out of scope.
- **Vector projection cleanup on chunk delete.** When `ChunkPolicy::Replace` deletes
  old chunks, any vector projection rows for those chunks (in a future `vec_nodes_*`
  table) should also be deleted. This is deferred until the vector capability gate is
  real.

---

## Implementation Plan

All phases use TDD: write the failing test first, then implement.

### Phase 1: NodeRetire and EdgeRetire

**Goal:** Typed retire operations that supersede a row without inserting a replacement.

Tests to write first:
1. `writer_retire_removes_active_node` — retire a node, assert no active row exists,
   assert historical row with `superseded_at IS NOT NULL` exists
2. `writer_retire_deletes_chunks_and_fts_rows` — retire a node that has chunks and FTS
   rows, assert chunks and FTS rows are deleted after retire
3. `writer_retire_node_with_no_source_ref_produces_warning` — retire with
   `source_ref: None`, assert `provenance_warnings` on receipt
4. `writer_retire_edge` — retire an active edge, assert superseded

Files: `crates/fathomdb-engine/src/writer.rs`, `crates/fathomdb-engine/src/lib.rs`,
`crates/fathomdb/src/lib.rs`

### Phase 2: ChunkPolicy on NodeInsert Replace

**Goal:** Node replace with `ChunkPolicy::Replace` atomically cleans up old chunks and
FTS rows before inserting new ones.

Tests to write first:
1. `writer_replace_with_chunk_replace_removes_old_fts_rows` — insert node+chunk,
   replace with new chunk using `ChunkPolicy::Replace`, assert old FTS row gone
2. `writer_replace_with_chunk_replace_removes_old_chunks` — same, assert old chunk row
   gone
3. `writer_replace_with_chunk_preserve_keeps_old_chunks` — replace with
   `ChunkPolicy::Preserve`, assert old chunk row still exists
4. `writer_replace_with_chunk_replace_is_atomic_on_fts` — after replace, FTS search
   returns new text, not old text

Files: `crates/fathomdb-engine/src/writer.rs`

### Phase 3: Runtime Table Upsert

**Goal:** `RunInsert`, `StepInsert`, `ActionInsert` support `upsert: bool` for status
transitions.

Tests to write first:
1. `writer_run_upsert_supersedes_prior_active_run` — insert run as `active`, upsert as
   `completed`, assert only one active row, assert historical row exists
2. `writer_step_upsert_supersedes_prior_active_step` — same for step
3. `writer_action_upsert_supersedes_prior_active_action` — same for action
4. `writer_run_upsert_false_fails_on_duplicate_run_id` — second insert with same `id`
   and `upsert: false` returns a SQLite unique constraint error

Files: `crates/fathomdb-engine/src/writer.rs`

### Phase 4: ID Generation Utility

**Goal:** Engine provides a `new_row_id()` helper for callers that want engine-managed
ID formatting without making it mandatory.

Tests to write first:
1. `new_row_id_returns_nonempty_unique_string` — two calls return different values
2. `new_row_id_is_valid_for_node_insert` — use returned value as `row_id` in a
   `NodeInsert`, assert write succeeds

Files: `crates/fathomdb-engine/src/ids.rs` (new), `crates/fathomdb/src/lib.rs`

---

## Definition of Done

This design is complete when:

- Nodes and edges can be retired without replacement via typed structs
- Retiring a node atomically deletes its chunks and FTS rows
- Replacing a node with `ChunkPolicy::Replace` atomically removes old chunks and FTS
  rows before inserting new ones
- `RunInsert`, `StepInsert`, and `ActionInsert` support typed status transitions via
  `upsert: bool`
- An engine-provided `new_row_id()` utility exists for callers that want it
- `check_semantics` reports zero orphaned chunks and zero stale FTS rows after any
  retire or replace operation
- All paths have TDD coverage before implementation
