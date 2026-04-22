# DBIM Playbook

**Status:** Current
**Last updated:** 2026-04-22

## Purpose

This file is the operational playbook for database integrity management in
`fathomdb`.

It is derived from historical expert notes now archived at
[dev/archive/db-integrity-management.md](./archive/db-integrity-management.md),
but updated to match the current architecture:

- `logical_id` vs `row_id`
- explicit `chunks` projection
- Rust engine with Go admin tooling
- explicit vector regeneration today and FathomDB-managed vector projection as
  the target design

Use [ARCHITECTURE.md](./ARCHITECTURE.md) for architectural invariants.
Use this file for repair workflows and admin operations.

## 1. Corruption Classes

`fathomdb` treats three corruption classes as first-class operational cases:

1. **Physical corruption**
   - disk, filesystem, or crash-related damage
2. **Logical corruption**
   - FTS or vector projections drift from canonical state
   - missing optional semantic projections after interruption
3. **Semantic corruption**
   - bad agent reasoning poisons the world model

The system should expose deterministic repair flows for all three.

## 2. Recovery Model

The recovery model rests on four invariants:

- canonical state is separate from derived projections
- canonical rows are append-oriented and versioned by `logical_id` / `row_id`
- canonical rows keep direct `source_ref` provenance
- required writes happen through atomic SQLite transactions

These invariants make projection rebuilds, rollback, and surgical excision
possible without restoring a coarse backup.

## 3. Admin Surface

The Rust engine should expose repair primitives. A separate Go admin tool,
`fathom-integrity`, wraps them operationally.

Core admin commands should include:

- `fathom-integrity check`
- `fathom-integrity repair`
- `fathom-integrity rebuild`
- `fathom-integrity recover`
- `fathom-integrity export`
- `fathom-integrity trace`
- `fathom-integrity excise`

## 4. Logical Corruption Repair

### 4.1 Deterministic Projection Rebuild

If FTS or vector projections are broken, stale, or suspected to be out of sync,
rebuild them from canonical state.

Conceptual API:

```python
db.admin.rebuild_projections(target=["vector", "fts"])
```

Conceptual flow:

1. isolate the database from concurrent writers
2. drop the corrupted virtual tables
3. recreate the virtual tables
4. scan active canonical rows
5. deterministically repopulate projections

Active-state rebuild should resolve through the current schema:

- canonical records come from `nodes`, `edges`, `chunks`, `runs`, `steps`,
  `actions`
- active nodes are resolved by `logical_id` and `superseded_at IS NULL`
- property FTS rows rebuild into per-kind `fts_props_<kind>` tables
- vector rows rebuild into per-kind `vec_<kind>` tables from `chunks <- nodes`

### 4.2 Missing Optional Semantic Backfills

Current releases use explicit admin/API regeneration for vector rows. The
target managed-vector design should use durable queue/state for vector
projection work so new writes are not starved behind large backfills.

If the process crashes before backfills complete:

1. pending durable projection work survives restart
2. missing chunk/vector rows are identified from canonical `chunks`
3. vector-enabled kinds are regenerated or drained by the projection worker

This is intentionally different from low-cost FTS projection, which can be
maintained synchronously for normal writes.

## 5. Semantic Corruption Repair

### 5.1 Time-Window Rollback

Use when a recent interval of agent actions is broadly bad.

Conceptual API:

```python
db.admin.rollback_agent_actions(since=t)
```

Conceptual flow:

1. find rows created after `t`
2. supersede those bad physical rows
3. reactivate the most recent prior row for each affected `logical_id`
4. rebuild projections if required

Rollback logic must operate on `row_id` and `logical_id`, not on a simplistic
single-row identity model.

### 5.2 Surgical Excision By `source_ref`

Use when a specific run, step, or action is known to be bad.

Conceptual flow:

1. identify the bad `source_ref`
2. find all canonical rows emitted by that source
3. supersede those bad physical rows
4. reactivate prior rows for the affected `logical_id`s
5. repair projections

This is the preferred repair path when the blast radius can be localized to one
bad inference chain.

## 6. Trace And Explain Workflow

When debugging semantic corruption, the operator needs to answer:

- which run did this come from?
- which step produced it?
- which action emitted it?
- what else was produced by that same source?

Because canonical rows keep direct `source_ref`, the admin tool should support:

```bash
fathom-integrity trace --db local.sqlite --source-ref act_xyz
```

Or time-scoped review:

```bash
fathom-integrity trace --db local.sqlite --last 2h
```

The output should be a causal chain from run -> step -> action -> canonical
rows.

## 7. Physical Recovery Protocol

If the SQLite file is physically damaged, recover canonical tables only.

Do not trust recovered FTS5 or `sqlite-vec` shadow tables as authoritative.

### 7.1 Canonical-Only Recovery

Conceptual flow:

1. isolate the database
2. use SQLite recovery tooling against canonical tables only
3. restore into a fresh SQLite file
4. run projection rebuilds

Canonical tables to preserve in v1:

- `nodes`
- `edges`
- `chunks`
- `runs`
- `steps`
- `actions`

### 7.2 Why Shadow Tables Are Excluded

FTS5 and `sqlite-vec` maintain extension-managed internal state. If a physical
recovery blindly restores shadow tables, the result may be a database that is
more broken than before recovery.

The correct model is:

- canonical tables are the ground truth
- projections are disposable and rebuildable

## 8. Crash Safety And Pre-Flight Writes

Long-running work must happen before the write lock is acquired.

Write discipline:

1. chunk and enrich outside the transaction
2. `BEGIN IMMEDIATE`
3. append canonical rows
4. write required projections
5. `COMMIT`

If the process dies during the transaction, SQLite WAL recovery discards the
partial work. The database returns to the pre-transaction state.

## 9. Export / Patch Workflow

Single-file portability should be treated as part of the recovery story.

### 9.1 Safe Export

```bash
fathom-integrity export --db local.sqlite --out agent_debug.sqlite
```

The export command should:

1. checkpoint WAL safely
2. copy the SQLite file
3. produce a consistent local snapshot for debugging

### 9.2 Patch Generation

If a developer identifies a bad `source_ref`, the admin tool should be able to
generate a small SQL patch that:

1. supersedes bad rows by `row_id`
2. resolves affected `logical_id`s
3. reactivates the correct prior physical version
4. leaves the remaining world model intact

### 9.3 Patch Application

```bash
fathom-integrity repair --db local.sqlite --target all
fathom-integrity rebuild --db local.sqlite --target fts
```

This is much better than shipping a full replacement database file back to the
user.

## 10. Integrity Checks

`fathom-integrity check` should aggregate:

- `PRAGMA integrity_check`
- `PRAGMA foreign_key_check`
- projection existence and shape checks
- missing chunk/vector detection
- active-row uniqueness checks per `logical_id`

The last check is important in an append-only design: for each `logical_id`,
there should be at most one active row at a time.

## 11. V1 Scope

Keep the v1 integrity surface focused:

- deterministic projection rebuild
- missing-projection rebuild on startup
- safe export
- trace by `source_ref`
- excise by `source_ref`
- patch apply

Defer for later if needed:

- richer approval-aware repair logic
- durable background job queues
- more specialized semantic repair tables

## 12. Relation To The Expert Source Note

[db-integrity-management.md](./archive/db-integrity-management.md) remains the
preserved expert design note in `dev/archive/`.

This playbook is the implementation-facing adaptation of that note for the
current `fathomdb` architecture and v1 scope.
