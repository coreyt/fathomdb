# Design: async property-FTS rebuild (shadow-build)

**Release:** 0.4.1 (pulled forward from 0.5.0)
**Scope item:** `dev/notes/scope-0.4.1.md` — async property-FTS
rebuild
**Related:** `dev/notes/roadmap-0.5.0.md` (item 10b's migration
rides on this machinery)

## Problem

`Engine::admin::register_fts_property_schema_with_entries` at
`crates/fathomdb-engine/src/admin.rs:1558-1656` runs the full FTS
rebuild inside one IMMEDIATE transaction:

- `admin.rs:1578` — `TransactionBehavior::Immediate` acquires the
  write lock.
- `admin.rs:1632-1641` — `DELETE` all existing property-FTS rows
  for the kind, then `insert_property_fts_rows_for_kind()` walks
  every node of the kind, computes `text_content` via
  `extract_property_fts` (`crates/fathomdb-engine/src/writer.rs:1176`),
  and inserts into `fts_node_properties`.
- `admin.rs:1655` — commit.

On Memex's 1M-row `WMExecutionRecord` this is 5–10 minutes of
write-lock hold. Memex's `_warn_if_large_rebuild` probe at
`m004_register_fts_property_schemas_v2.py:185-213` exists only
because fathomdb has no async alternative today.

## Current state

- **Single FTS5 virtual table** `fts_node_properties` at
  `crates/fathomdb-schema/src/bootstrap.rs:390-394` with columns
  `(node_logical_id UNINDEXED, kind UNINDEXED, text_content)`.
  One row per (kind, node).
- **Position sidetable** `fts_node_property_positions` records
  which path each token came from — used by fused-filter routing,
  not by ranking.
- **Register call is synchronous and transactional.** If it
  returns `Ok`, the new schema is live and queried. This is the
  contract 0.4.1 shifts.

## Locked semantics (from scope doc)

- Register call is semi-async: schema persisted synchronously,
  rebuild runs in background.
- Reads during rebuild serve the **old schema** (or JSON scan
  fallback for first registration).
- Writes during rebuild double-write (old-schema live + new-schema
  shadow).
- Atomic swap on completion.
- Crash = discard shadow, caller re-invokes.
- `RebuildMode::Eager` escape hatch preserves today's semantics.

## Design

### Storage layout

**Chosen: separate staging table + final bulk-insert swap.**

Reasoning: FTS5 virtual tables don't support efficient filtering
by non-indexed columns at query time, so a `schema_version` column
approach doesn't work. Two parallel FTS5 tables with metadata
pointing reads at one or the other complicates every query with a
per-kind table lookup. The cleanest model is: keep
`fts_node_properties` as the single authoritative FTS5 table, and
hold in-progress rebuild content in a non-FTS5 staging table until
swap time.

**New tables (bootstrap migration):**

```sql
-- Holds precomputed rebuild rows. NOT an FTS5 table — just
-- staging. Rows move into fts_node_properties atomically at
-- swap time.
CREATE TABLE fts_property_rebuild_staging (
    kind             TEXT NOT NULL,
    node_logical_id  TEXT NOT NULL,
    text_content     TEXT NOT NULL,
    positions_blob   BLOB,             -- for sidetable population
    PRIMARY KEY (kind, node_logical_id)
);

-- Per-kind rebuild state. At most one pending rebuild per kind
-- in the first cut (serialized).
CREATE TABLE fts_property_rebuild_state (
    kind               TEXT PRIMARY KEY,
    schema_id          INTEGER NOT NULL,  -- the target schema
    state              TEXT NOT NULL,     -- 'PENDING' | 'BUILDING' | 'SWAPPING' | 'COMPLETE' | 'FAILED'
    rows_total         INTEGER,
    rows_done          INTEGER NOT NULL DEFAULT 0,
    started_at         INTEGER NOT NULL,  -- unix millis
    last_progress_at   INTEGER,
    error_message      TEXT,
    is_first_registration INTEGER NOT NULL DEFAULT 0  -- controls scan fallback
);
```

No changes to `fts_node_properties` itself. No new columns on
existing tables.

### Register call flow (async mode)

```
register_fts_property_schema_with_entries(kind, entries, mode: Async)
  │
  ├─ IMMEDIATE tx #1 (milliseconds):
  │    1. Validate entries (unchanged)
  │    2. Compute schema_id, persist schema row (unchanged)
  │    3. Detect first-registration vs re-registration
  │    4. Upsert fts_property_rebuild_state row:
  │         state='PENDING', schema_id=new, rows_done=0,
  │         is_first_registration=(1 if first else 0)
  │    5. COMMIT
  │
  ├─ Enqueue background rebuild task for (kind, schema_id)
  └─ Return Ok (schema persisted, rebuild pending)
```

The first IMMEDIATE tx is ~milliseconds. It acquires the write
lock only long enough to update metadata. **It does not touch
`fts_node_properties` at all.**

### Background rebuild task

```
rebuild_task(kind, schema_id):
  Update state='BUILDING' (short tx)
  Compute rows_total = count of nodes of this kind (short tx)

  Iterate nodes in batches:
    For each batch:
      short tx:
        For each node in batch:
          Skip if already in staging (idempotent resume)
          Compute (text_content, positions_blob) under new schema
          INSERT INTO fts_property_rebuild_staging
        Update rows_done += batch_size
      Commit batch

  Update state='SWAPPING' (short tx)

  Final swap tx (IMMEDIATE, longer hold — see "Swap cost" below):
    1. DELETE FROM fts_node_properties WHERE kind = X
    2. INSERT INTO fts_node_properties(node_logical_id, kind, text_content)
         SELECT node_logical_id, kind, text_content
         FROM fts_property_rebuild_staging WHERE kind = X
       (FTS5 indexes each row during INSERT — this is the bulk
       of the final-swap cost.)
    3. DELETE FROM fts_node_property_positions WHERE kind = X
    4. Re-populate positions sidetable from staging.positions_blob
    5. DELETE FROM fts_property_rebuild_staging WHERE kind = X
    6. UPDATE fts_property_rebuild_state SET state='COMPLETE'
    7. COMMIT
```

Batch size is chosen dynamically by the engine to keep each
short-tx lock hold under ~1s. Heuristic starting point: batch
size 5000, adjust based on measured per-batch duration.

### Write path during rebuild

Every write to a node of a kind with `fts_property_rebuild_state
!= NULL AND state IN ('PENDING','BUILDING','SWAPPING')`:

1. Extract `text_content` under the **old** schema, write to
   `fts_node_properties` (as today).
2. Extract `text_content` under the **new** schema, upsert into
   `fts_property_rebuild_staging`.

Both happen inside the write's existing transaction — no extra
lock acquisitions, no cross-tx inconsistency window.

The double-extraction cost is 2× the JSON-walking cost for the
property-FTS write path during the rebuild window. Writes outside
the window pay the normal cost.

**Edge case: delete during rebuild.** A node deletion removes
from `fts_node_properties` (as today) and from
`fts_property_rebuild_staging`. Trivial.

### Read path during rebuild

Reads against a kind with a pending rebuild hit
`fts_node_properties` as today. The old schema's rows are still
there until the final swap. **No change to the query path.**

**Exception: first-registration scan fallback.** If the rebuild
state row has `is_first_registration=1`, queries on that kind
during the rebuild cannot use FTS5 (no rows exist yet). They fall
back to a scan over `nodes` for that kind, applying the predicate
via JSON functions. Implementation: check the rebuild state row
before compiling the query; if first-registration rebuild is
pending, route to the scan path. This is a new query execution
path added to the coordinator, scoped to this case only.

The scan path is slow (unindexed JSON scan) but correct. It is
not a general fallback — it exists only to prevent the "register
returns empty for N minutes on first registration" cliff. Once
the swap completes and the state row is `COMPLETE`, the scan
path disables for that kind.

### Swap cost (the one remaining stall)

The final swap transaction does:
- DELETE old rows from `fts_node_properties` (fast — O(rows)
  FTS5 delete).
- INSERT new rows into `fts_node_properties` (FTS5 indexes each
  row — this is the bulk of the cost).

On Memex's 1M-row kind, this is estimated at ~1–2 minutes of
write-lock hold (FTS5 bulk-insert cost for precomputed strings,
without the JSON-walking cost that dominates the current eager
rebuild). A major improvement over 5–10 minutes, but not zero.

The ~45ms/row figure Memex cites is for the full rebuild including
JSON walking; the FTS5-insert-only portion is a fraction of that,
probably 5–10ms/row. At 1M rows × 10ms = 10,000s = 2.8 hours,
which is clearly wrong — must be I/O-bound on bulk insert, not
per-row indexing. SQLite FTS5 bulk insert benchmarks suggest
50k–100k rows/sec for small text fields, so 1M rows is 10–20
seconds. **The real number needs measurement on Memex-scale data
during implementation** — the 1–2 minute estimate is the
conservative upper bound, actual may be much lower.

**Zero-stall swap is a post-0.4.1 design question.** Candidates
to explore after 0.4.1 ships:
- FTS5 segment-level hot-swap (if SQLite internals expose it).
- Shadow FTS5 table with atomic `ALTER TABLE RENAME` swap (needs
  to verify FTS5 rename semantics hold the shadow contents
  correctly).
- Per-kind FTS5 tables (comes naturally with item 10b in 0.5.0 —
  per-kind tables let you swap by drop/rename per kind without
  touching other kinds).

The last option is the cleanest long-term answer: once 10b lands
per-kind tables, the swap becomes a pure rename-based operation
with negligible lock hold. This is another reason item 9 and
item 10b compose well — 10b's migration uses 9's shadow-build
machinery and also makes 9's swap cost go to zero.

### Eager mode escape hatch

```rust
pub enum RebuildMode {
    /// Legacy behavior: full rebuild runs in the register tx.
    /// Preserved for tests, small kinds, and callers that want
    /// strict register-then-query semantics.
    Eager,
    /// 0.4.1+: register returns fast, rebuild runs in background.
    Async,
}

impl Default for RebuildMode {
    fn default() -> Self { RebuildMode::Async }
}
```

**Default mode policy:** `Async`. Existing call sites that rely on
synchronous semantics must be updated to pass `Eager` explicitly.
This is a **source-breaking change** at the Rust level because
`register_fts_property_schema_with_entries` gains a parameter.

Options to minimize churn:

- **Option A:** Overload with two methods —
  `register_fts_property_schema_with_entries(kind, entries)`
  keeps today's eager semantics, new
  `register_fts_property_schema_with_entries_async(kind, entries)`
  provides async. No existing call site breaks.
- **Option B:** Add a `mode: RebuildMode` parameter with
  `Default::default() = Async`. All existing Rust call sites
  break (must add `RebuildMode::Eager` or `RebuildMode::Async`).
  Python/TypeScript bindings get a keyword default of `Async`.
- **Option C:** Keep the existing method signature, change its
  default to async, callers that depend on synchronous semantics
  are silently broken until they hit a test assertion.

**Recommendation: Option B.** Option A bifurcates the API
permanently (two methods to maintain forever). Option C silently
breaks callers, which is the worst outcome. Option B is an
explicit, loud change that forces every caller to make a choice
— which is what a semantics shift this significant warrants.

Python/TS bindings use keyword defaults so binding callers don't
break unless they relied on the synchronous guarantee.

### Crash recovery

On engine open:
1. Scan `fts_property_rebuild_state` for rows in state
   `PENDING` | `BUILDING` | `SWAPPING`.
2. For each:
   - If state is `SWAPPING`: the swap was in-flight at crash.
     Roll back: DELETE partial new rows from `fts_node_properties`
     for the kind (may not be straightforward — FTS5 partial-insert
     rollback needs verification). Mark state `FAILED`.
   - Otherwise: DELETE staging rows for the kind, mark state
     `FAILED`.
3. The `FAILED` state is visible via
   `get_property_fts_rebuild_progress` and the caller must
   re-invoke `register_fts_property_schema_with_entries` to retry.

**First cut does not automatically re-enqueue failed rebuilds.**
The caller is on the hook to observe the failure and retry.
This keeps crash recovery simple at the cost of requiring
caller-side retry logic. Acceptable in 0.4.1; can automate later
if callers ask.

### Observability API

```rust
impl Coordinator {
    pub fn get_property_fts_rebuild_progress(
        &self,
        kind: &str,
    ) -> Result<Option<RebuildProgress>>;
}

pub struct RebuildProgress {
    pub state: RebuildState,     // Pending/Building/Swapping/Complete/Failed
    pub rows_total: Option<u64>, // None until the initial count completes
    pub rows_done: u64,
    pub started_at: SystemTime,
    pub last_progress_at: Option<SystemTime>,
    pub estimated_completion: Option<SystemTime>,
    pub error_message: Option<String>,
}
```

Returns `None` when no rebuild state row exists for the kind (i.e.
no rebuild has ever been requested, or the last one completed
and the state row was cleaned up). Returns `Some(Complete)` for a
brief window between swap completion and state-row cleanup.

Python and TypeScript bindings expose the same shape.

### Concurrent rebuilds

**First cut: serialize.** At most one rebuild runs at a time
across the whole engine. Register calls that land while a rebuild
is in progress enqueue their rebuild task and wait their turn.
The register call itself still returns fast (schema is persisted);
only the background task is serialized.

Relaxation to parallel rebuilds on disjoint kinds is a post-0.4.1
scope item. Serialized is correct, just not maximally fast.

## Acceptance

1. `RebuildMode::Async` is the default; `Eager` escape hatch
   works and preserves today's transactional semantics end-to-end.
2. Register call under async mode returns in <100ms even for
   kinds with 100k+ rows.
3. Reads during the rebuild window return old-schema results
   (or scan-fallback for first registration).
4. Writes during the rebuild window correctly populate both the
   live table and staging, verified by querying the new schema
   immediately post-swap and finding rows written during the
   rebuild window.
5. Crash simulation (kill mid-rebuild, reopen) leaves no
   corrupted state; `fts_property_rebuild_state` reflects
   `FAILED` and staging is cleaned up.
6. `get_property_fts_rebuild_progress` returns monotonic
   `rows_done` during a rebuild.
7. Cross-binding smoke tests for Python + TypeScript.
8. Manual stress measurement on a 100k-row kind: register call
   latency, total rebuild wall clock, final swap lock-hold
   duration. Numbers recorded in release notes.
9. All existing admin.rs / writer.rs tests pass with minimal
   updates (only adding `RebuildMode::Eager` where tests
   require synchronous semantics).

## Out of scope

- Zero-stall swap (the final IMMEDIATE tx still holds the write
  lock for O(rows) FTS5 bulk insert). Post-0.4.1.
- Persistent rebuild resume across engine restart. Post-0.4.1.
- Parallel rebuilds on disjoint kinds. Post-0.4.1.
- Automatic retry of `FAILED` rebuilds. Caller-driven in 0.4.1.
- Cancellation API. Out of scope.
- Per-kind FTS5 tables (item 10b / 0.5.0).

## Open questions

1. **FTS5 bulk-insert measured cost.** Is the final swap actually
   1–2 minutes on 1M rows, or seconds? Needs benchmark on
   Memex-scale data before shipping the release-notes estimate.
2. **FTS5 partial-insert rollback on crash during SWAPPING
   state.** If we crash mid-swap (between DELETE and INSERT
   completing), is the FTS5 table in a consistent state? SQLite
   transaction rollback should handle this, but verify with
   injected-crash tests.
3. **Default mode policy.** Option B recommended (explicit
   parameter, all callers choose). Implementation may lean
   toward Option A (separate method) if Option B's source-break
   hits too many internal call sites during implementation —
   revisit before locking.
4. **Background task execution model.** Tokio task on a shared
   runtime? Dedicated thread? The engine doesn't currently run
   long-lived background work outside the request path — this is
   the first case that needs it. Implementation detail that
   affects the runtime / lifecycle story; resolve early in
   implementation.
5. **Rebuild progress row cleanup.** When does a `COMPLETE` row
   get deleted from `fts_property_rebuild_state`? On next register
   call, on engine open, on timer, or never? Affects whether
   `get_property_fts_rebuild_progress` can return
   `Some(Complete)` after the swap, and for how long.

## Risks

- **Background task model is a new concept for the engine.** The
  runtime (`EngineRuntime`) currently doesn't own long-lived
  tasks. Adding one opens questions about shutdown order, task
  panics, backpressure. Scope to "simplest thing that works" —
  likely a tokio task spawned by the coordinator, cancelled on
  engine close with a best-effort drain.
- **FTS5 rollback semantics under crash during SWAPPING.** If
  this isn't clean, crash recovery complexity balloons. Mitigate
  by running injected-crash tests early in implementation to
  confirm the model holds.
- **Double-write cost during rebuild window.** Roughly 2× the
  per-row property-FTS write cost. Memex's workloads are
  read-dominated so this is probably fine, but if a caller
  registers during a write-heavy burst, writes will slow
  noticeably. Document the expected overhead in the release
  notes.
- **Memex-facing behavior shift.** Memex's tests and any
  benchmark that does "register then query" will see the old
  schema returned during the window. This is semantically
  correct but will surprise test authors. The changelog's
  "Behavior change" callout is load-bearing for catching this
  early in Memex adoption.
