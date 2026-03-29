# Design: Add Operational Store Feature

## Purpose

Define a `fathomdb`-owned operational-store feature that can support
high-churn, table-shaped application state without weakening the current
durability, export, repair, and recovery model.

This feature is meant for operational bookkeeping such as:

- connector health
- scheduler cursors
- queue state
- debounce/heartbeat records
- tool usage counters
- singleton latest-state blobs

This feature is **not** a back door for application domain schema. Goals,
meetings, notifications, plans, knowledge objects, and similar concepts remain
application-defined nodes and edges when they benefit from history, search,
provenance, or graph traversal.

## Non-Goals

The operational store is explicitly **not** the right destination for:

- `scheduled_tasks`
  - durable task definitions with graph relationships, world-model links, or
    task/projection semantics should remain graph-native nodes
- `notifications`
  - user-visible, actionable records benefit from search, provenance, and edges
    and should remain graph-native nodes
- arbitrary application-defined SQL tables
- a second co-resident SQLite file that falls outside `fathomdb` export and
  recovery tooling

## Constraints

The operational store must satisfy the same core protections that already apply
to the rest of `fathomdb`:

1. same-file SQLite durability under WAL
2. safe export through the existing backup-based export path
3. schema bootstrap and migration ownership by `fathomdb-schema`
4. repairability and diagnosability through admin / `fathom-integrity`
5. no silent data-loss mode that is invisible to the engine's integrity surface

The feature must also preserve the current architecture boundary:

- no engine-owned product/domain tables
- no arbitrary application SQL tables
- no separate shadow database

## Problem Statement

`fathomdb` already handles history-bearing and search-bearing operational state
well through:

- generic nodes and edges
- chunks plus FTS/vector projections
- `runs`, `steps`, `actions`
- append-oriented supersession

What it does not handle well today is high-churn current-state bookkeeping
where mutation history has little or no value and full node supersession would
create pure write amplification.

Examples:

- `connector_health` updated every minute
- `last_check` / `last_result` cursors for pollers
- counters incremented on every tool call
- ephemeral "currently running" state

The design challenge is to support those workloads **without** introducing a
second-class storage surface that falls outside export, recover, integrity, and
admin guarantees.

## Design Decision

Support two **user-facing access patterns** on top of one **canonical storage
model**:

- append-only mutation history
- latest-state reads

These are not two unrelated storage modes.

The canonical storage model is:

- append-only `operational_mutations`

The serving / convenience model is:

- rebuildable `operational_current`

This mirrors the existing `fathomdb` philosophy:

- canonical state is durable and auditable
- convenience or query-oriented structures may be rebuilt

## High-Level Model

### Canonical tables

- `operational_collections`
- `operational_mutations`

### Rebuildable / derived table

- `operational_current`

### Optional audit integration

Important admin operations on the operational store emit `provenance_events`.

Normal operational mutations do not have to write a provenance event per row if
the mutation log itself is already the canonical history. Administrative actions
like rebuild, purge, compaction, or collection registration should remain
visible in `provenance_events`.

## Supported Collection Kinds

The initial design supports two logical collection kinds:

1. `append_only_log`
2. `latest_state`

These kinds affect read/write semantics, but both are stored through the same
canonical mutation log.

### `append_only_log`

Intended for:

- append-only audit-like operational events
- heartbeat history
- recent status history where rows never mutate after insert

Semantics:

- every write appends a new mutation
- reads may stream recent mutations directly
- `operational_current` is optional and may be omitted

### `latest_state`

Intended for:

- connector health
- scheduler cursors
- per-key counters or other small mutable records
- singleton current-state blobs
- lifecycle-tracking rows such as `intake_log`

Semantics:

- every write still appends a canonical mutation
- current-state reads come from `operational_current`
- `operational_current` is rebuilt from the mutation log if needed

## Schema Sketch

### 1. `operational_collections`

```sql
CREATE TABLE IF NOT EXISTS operational_collections (
    name TEXT PRIMARY KEY,
    kind TEXT NOT NULL,                 -- append_only_log | latest_state
    schema_json TEXT NOT NULL,          -- declared key/payload contract
    retention_json TEXT NOT NULL,       -- TTL / compaction / max_rows policy
    format_version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    disabled_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_operational_collections_kind
    ON operational_collections(kind, disabled_at);
```

Notes:

- `schema_json` is declarative metadata, not executable SQL.
- in v1, `schema_json` is documentation-only metadata unless validation is
  explicitly enabled later for a collection or a future format version
- the collection is registered through an engine API, not by arbitrary `CREATE TABLE`
- `disabled_at` lets the engine retire a collection definition without dropping
  the mutation history immediately

### 2. `operational_mutations`

```sql
CREATE TABLE IF NOT EXISTS operational_mutations (
    id TEXT PRIMARY KEY,
    collection_name TEXT NOT NULL,
    record_key TEXT NOT NULL,
    op_kind TEXT NOT NULL,              -- append | put | delete | increment
    payload_json TEXT NOT NULL,
    source_ref TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    FOREIGN KEY(collection_name) REFERENCES operational_collections(name)
);

CREATE INDEX IF NOT EXISTS idx_operational_mutations_collection_key_created
    ON operational_mutations(collection_name, record_key, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_operational_mutations_source_ref
    ON operational_mutations(source_ref);
```

Notes:

- this is the canonical operational history
- `record_key` is application-chosen within the collection namespace
- `op_kind` is constrained by collection kind and validated by the writer/admin
- `source_ref` keeps the operational store compatible with existing trace/excise
  patterns

### 3. `operational_current`

```sql
CREATE TABLE IF NOT EXISTS operational_current (
    collection_name TEXT NOT NULL,
    record_key TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    last_mutation_id TEXT NOT NULL,
    PRIMARY KEY(collection_name, record_key),
    FOREIGN KEY(collection_name) REFERENCES operational_collections(name),
    FOREIGN KEY(last_mutation_id) REFERENCES operational_mutations(id)
);

CREATE INDEX IF NOT EXISTS idx_operational_current_collection_updated
    ON operational_current(collection_name, updated_at DESC);
```

Notes:

- this is derived / rebuildable
- only meaningful for `latest_state`
- for `append_only_log`, current rows may be omitted entirely or limited to a
  "latest per key" convenience materialization if later justified

## Mutation Semantics

### Register collection

Applications register a collection with:

- stable collection name
- kind
- declared schema metadata
- retention policy

The engine stores this in `operational_collections`.

### Write to `append_only_log`

On append:

1. validate collection exists and `kind = append_only_log`
2. re-check that the collection is still enabled after the SQLite write
   transaction begins; disable enforcement must be serialized with the write,
   not only preflighted before `BEGIN IMMEDIATE`
3. append a row to `operational_mutations`
4. optionally enforce retention/compaction policy later through admin

No `operational_current` update is required.

### Write to `latest_state`

On put/delete:

1. validate collection exists and `kind = latest_state`
2. re-check that the collection is still enabled after the SQLite write
   transaction begins; a collection disabled concurrently with writer traffic
   must reject the write rather than accept a stale preflight result
3. append canonical row to `operational_mutations`
4. update `operational_current` inside the same SQLite transaction

If `operational_current` is lost or corrupted, it is rebuilt from
`operational_mutations`.

### `Increment` in v1

`Increment` is optional and deferred in v1.

The required v1 write operations are:

- `Append`
- `Put`
- `Delete`

Rationale:

- `Put` is sufficient for the Memex operational tables that currently matter
- mixed `Put` / `Increment` replay semantics add avoidable rebuild complexity
- deferring `Increment` keeps the first implementation deterministic and easier
  to validate

If `Increment` is added later, replay semantics must be specified explicitly.

### Atomicity

Operational writes should be allowed inside the same transaction as graph/runtime
writes.

Recommended extension to `WriteRequest`:

- `operational_writes: Vec<OperationalWrite>`

This lets an application commit, for example:

- a `run` row
- a `step` row
- a `node` update
- an operational current-state cursor update

in one SQLite transaction.

## Writer API Sketch

Rust-side sketch:

```rust
pub enum OperationalWrite {
    Append {
        collection: String,
        record_key: String,
        payload_json: String,
        source_ref: Option<String>,
    },
    Put {
        collection: String,
        record_key: String,
        payload_json: String,
        source_ref: Option<String>,
    },
    Delete {
        collection: String,
        record_key: String,
        source_ref: Option<String>,
    },
    // Optional/deferred in v1.
    Increment {
        collection: String,
        record_key: String,
        field: String,
        by: i64,
        source_ref: Option<String>,
    },
}
```

Notes:

- `Append`, `Put`, and `Delete` are the required v1 operations
- `Increment` is optional/deferred in v1 and should not block the initial
  feature
- all writes remain caller-driven and explicit
- no arbitrary expression language is added

## Read API Sketch

Initial read surface should stay simple and typed:

### Collection metadata

- `list_operational_collections()`
- `get_operational_collection(name)`

### Latest-state reads

- `get_operational_current(collection, record_key)`
- `list_operational_current(collection, limit, prefix_key?)`

### Mutation-history reads

- `list_operational_mutations(collection, record_key?, since?, limit)`

This is enough to support the high-churn use cases without introducing a new
general query language immediately.

## Known v1 Limitations

### Filtered reads on non-key payload fields

The v1 read API is intentionally narrow:

- exact-key current-state lookup
- collection listing
- mutation-history listing by collection, optional key, and optional `since`

This is acceptable for small `latest_state` collections such as:

- `connector_health`
- `user_settings`
- `session_context`
- `auto_ingest_sources`

because applications can list and filter client-side at small row counts.

It is a known limitation for large `append_only_log` collections such as
`audit_log`, where application queries may want to filter by payload fields like:

- `connector`
- `goal_id`
- `session_id`

without scanning all rows since a timestamp cut.

This limitation is acceptable for v1, but should be treated as an explicit
tradeoff, not an accidental omission.

### `schema_json` validation depth

In v1, `schema_json` is documentation-only metadata.

The engine does not need to enforce full payload-shape validation in the first
slice. Applications may validate payloads before submission. If engine-level
validation is added later, it should be:

- opt-in per collection
- format-versioned
- explicit about compatibility on schema evolution

## Future Extension Point: Declared Filterable Fields And Secondary Indexes

If v1 filtered reads prove too weak for large operational collections, the next
extension point should be collection-declared filterable fields rather than raw
SQL.

Possible shape:

- extend `schema_json` with a `filterable_fields` section
- allow the engine to create and own expression indexes for declared fields
- expose typed filtered-read APIs for those declared fields only

Examples:

- `audit_log.connector`
- `audit_log.goal_id`
- `audit_log.session_id`
- `auto_ingest_sources.type`
- `auto_ingest_sources.enabled`

This keeps the operational store within the architecture boundary while giving
large collections a principled performance escape hatch.

## Admin Commands

The operational store needs first-class admin operations, not hidden SQLite
tables.

### Collection management

- `register_operational_collection`
- `disable_operational_collection`
- `describe_operational_collection`

### Integrity and diagnostics

- extend `check_integrity` and `check_semantics` with operational findings
- add `trace_operational_collection --collection <name> [--record-key <key>]`

### Repair and rebuild

- `rebuild_operational_current --collection <name>|--all`
- `compact_operational_collection --collection <name>`
- `purge_operational_collection --collection <name> --before <timestamp>`

### Export and recovery

No new export command is required if the data stays in the main DB file, but the
recover path must treat the operational store explicitly:

- preserve `operational_collections`
- preserve `operational_mutations`
- rebuild `operational_current` after bootstrap if needed

## Integrity Model

The operational store needs explicit invariants comparable to the existing node /
chunk / projection invariants.

### Layer 2 / structural invariants

- every `operational_mutations.collection_name` exists in `operational_collections`
- every `operational_current.collection_name` exists in `operational_collections`
- every `operational_current.last_mutation_id` exists in `operational_mutations`
- every `latest_state` collection has at most one current row per `record_key`

### Layer 3 / semantic invariants

- current row payload matches the latest non-deleted mutation for that key
- disabled collections do not accept new writes, including writes whose initial
  kind/registration lookup happened before the disable committed
- append-only collections do not receive invalid op kinds
- latest-state collections accept `Put` / `Delete` in v1 and reject unsupported
  optional operations
- retention/compaction does not remove canonical rows newer than the policy cut

### Repairability

At minimum, the engine must support deterministic repair of:

- missing `operational_current` rows
- stale `operational_current` rows
- `operational_current` rows pointing at a missing mutation

The repair primitive is:

- rebuild `operational_current` from `operational_mutations`

## Recovery Contract

This feature only meets the required bar if the recovery contract is explicit.

### Safe export

No change in principle:

- `safe_export()` already uses SQLite backup from a live connection
- same-file operational tables are included automatically

### Physical recovery

The `.recover` flow in `fathom-integrity` must be updated so that:

- `operational_collections` and `operational_mutations` are treated as canonical
  tables to preserve
- `operational_current` is treated as rebuildable and may be skipped/rebuilt

This matches current handling of projections and metadata tables.

### Post-recovery bootstrap

After schema bootstrap:

1. restore any required operational schema objects
2. rebuild `operational_current`
3. run integrity and semantic checks
4. report recovered row counts for operational canonical tables

## Compaction And Retention

Retention must be explicit and collection-scoped.

### `append_only_log`

Possible policies:

- keep last `N` rows
- purge older than `T`
- keep all

Compaction should:

- emit a bounded admin audit event
- be previewable / dry-runnable
- never touch rows newer than the declared cut

### `latest_state`

Compaction can purge old mutation history while keeping current state, but only
if the product is willing to discard older operational history.

Recommended initial policy:

- keep all mutation history in v1
- add opt-in history pruning later once audit and recovery expectations are
  clearer

This keeps the first implementation aligned with `fathomdb`'s recoverability
story.

## Interaction With Existing Primitives

### `source_ref`

Operational mutations should accept `source_ref` so they can participate in:

- trace by provenance
- excision by provenance where appropriate
- operator debugging

This does **not** mean every operational collection is subject to automatic
source-based excision in v1. Some state, such as counters or connector health,
may not be meaningfully excisable.

The initial trace surface should work first; excision policy can be collection-
specific and deferred where necessary.

### `provenance_events`

Use `provenance_events` for:

- collection registration
- rebuild
- compaction
- purge
- repair

Do not require a provenance event per ordinary operational mutation if the
mutation log itself is canonical history.

### `runs` / `steps` / `actions`

The operational store complements, not replaces, runtime tables.

- `runs` / `steps` / `actions` remain the typed runtime provenance anchor
- operational collections hold high-churn current-state or append-only
  bookkeeping
- applications may update both in one transaction through `WriteRequest`

## Why Not Direct Mutable Tables?

Direct mutable "latest state only" tables would be simpler, but they fail the
desired parity bar in several ways:

- no canonical mutation trail unless separately added
- harder to rebuild deterministically after corruption
- harder to make purge/repair auditable
- weaker recovery story than current append-oriented engine tables

If `latest_state` is needed, it should be implemented as:

- append-only mutation log
- rebuildable current-state materialization

not as a standalone mutable table with no history.

## Go / Bridge Surface

The admin bridge should add commands for:

- `register_operational_collection`
- `describe_operational_collection`
- `rebuild_operational_current`
- `trace_operational_collection`
- `compact_operational_collection`

`fathom-integrity` should:

- include operational findings in `check`
- include operational canonical row counts in `recover`
- expose repair/rebuild commands for operational current-state

## Acceptance Tests

### Schema and bootstrap

- bootstrap creates `operational_collections`, `operational_mutations`, and
  `operational_current`
- bootstrap is idempotent on fresh and upgraded databases
- collection registration survives re-open and re-bootstrap
- v1 accepts `schema_json` as documentation-only metadata without rejecting
  writes for shape mismatch

### Atomic write behavior

- a `WriteRequest` containing node writes and operational writes commits all or
  none
- a failing operational write rolls back accompanying node/runtime writes
- `latest_state` write appends a mutation and updates `operational_current`
  atomically

### Latest-state rebuild

- deleting all rows from `operational_current` then running
  `rebuild_operational_current` restores correct current state
- corrupting `last_mutation_id` is detected by integrity checks and fixed by
  rebuild
- rebuild ignores deleted keys correctly

### Export and recovery

- `safe_export` includes operational canonical rows and current-state rows
- `.recover`-based recovery preserves `operational_collections` and
  `operational_mutations`
- recovered DB can rebuild `operational_current` and pass integrity checks

### Integrity and repair

- `check_integrity` reports broken operational collection/mutation foreign-key
  conditions
- `check_semantics` reports stale or missing `operational_current`
- repair/rebuild clears those findings deterministically

### Collection-kind enforcement

- `append_only_log` rejects `Put` / `Delete` operations that violate its kind
- `latest_state` accepts `Put` / `Delete` in v1
- `latest_state` rejects unsupported deferred operations like `Increment` when
  they are not enabled
- disabled collections reject new writes

### Known limitation coverage

- `audit_log`-style collections can be listed by collection and `since`, but
  filtered reads on payload fields are documented as degraded in v1
- small `latest_state` collections remain usable with client-side filtering

### Auditability

- collection registration writes a bounded provenance event
- rebuild/compaction/purge write bounded provenance events
- ordinary operational mutations remain queryable through
  `operational_mutations`

### Retention and compaction

- dry-run compaction reports affected row counts without mutation
- compaction deletes only rows older than the requested cut
- compaction never corrupts current-state reconstruction

## Implementation Order

1. Add schema and migration support for the three operational tables.
2. Add collection registration and simple append/put/delete write support.
3. Extend `WriteRequest` and writer transaction flow for operational writes.
4. Add `operational_current` rebuild logic and integrity checks.
5. Extend admin bridge and `fathom-integrity` check/recover reporting.
6. Add compaction and retention policy support.

## Definition Of Done

The operational-store feature is ready when:

- high-churn operational state can live in the main `fathomdb` file without a
  second shadow database
- the canonical operational history is append-only and auditable
- current-state serving rows are rebuildable
- export, recovery, integrity, and repair explicitly cover the feature
- the feature does not introduce arbitrary application SQL tables or product-
  specific engine schema
