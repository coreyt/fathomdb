# Design: Provenance Events Retention

## Purpose

Address the verified finding that `provenance_events` grows without bound
(C-5). Every write, retire, restore, and excise operation appends events.
In production with millions of writes, this table dominates database size
with no cleanup path.

---

## Current State

The `provenance_events` table is created in migration v2:

```sql
CREATE TABLE IF NOT EXISTS provenance_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    subject TEXT NOT NULL,
    source_ref TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
)
```

No `DELETE` or retention mechanism exists anywhere in the codebase.
Operational retention (`plan_operational_retention` / `run_operational_retention`)
covers `operational_mutations` but not provenance.

---

## Design

### Admin primitive: `purge_provenance_events`

Add a new admin method alongside the existing retention primitives:

```rust
pub fn purge_provenance_events(
    &self,
    before_timestamp: i64,
    options: ProvenancePurgeOptions,
) -> Result<ProvenancePurgeReport, EngineError>
```

```rust
pub struct ProvenancePurgeOptions {
    /// If true, report what would be deleted without deleting.
    pub dry_run: bool,
    /// Event types to exclude from purging (e.g. keep "excise" events forever).
    pub preserve_event_types: Vec<String>,
}

pub struct ProvenancePurgeReport {
    pub events_deleted: u64,
    pub events_preserved: u64,
    pub oldest_remaining: Option<i64>,
}
```

### Execution

```sql
DELETE FROM provenance_events
WHERE created_at < ?1
  AND event_type NOT IN (...)
```

Run inside `BEGIN IMMEDIATE`. For large tables, batch the deletes in
chunks of 10,000 rows to avoid holding the write lock for extended
periods:

```sql
DELETE FROM provenance_events
WHERE rowid IN (
    SELECT rowid FROM provenance_events
    WHERE created_at < ?1
      AND event_type NOT IN (...)
    LIMIT 10000
)
```

Loop until zero rows are deleted. Each batch commits and re-acquires the
lock, allowing interleaved writes.

### Default preservation

Certain event types should be preserved by default because they represent
irreversible state transitions:

- `excise` — records data removal; losing the event loses the audit trail
  of what was removed.
- `purge` — records node/edge destruction.

The caller can override `preserve_event_types` to control this.

### Relationship to operational retention

Operational retention and provenance retention are independent concerns:
- Operational retention manages `operational_mutations` per-collection
  based on `retention_json` policy.
- Provenance retention manages the cross-cutting audit log.

They share no data dependencies and can run independently. However, both
should be invocable from the same scheduling surface (operator cron or a
future auto-retention interval).

### Bridge and Python surface

Expose `purge_provenance_events` through:
- The admin bridge protocol as a new command.
- The Python `FathomAdmin` class.
- `fathom-integrity` Go CLI as a subcommand.

---

## Not in scope

- Automatic provenance retention triggered by the engine. This follows the
  same design principle as operational retention: the engine provides
  plan/run primitives; scheduling is external.
- Provenance event archival (export before delete). This can be built
  later using `query_provenance_events` + `purge_provenance_events`.

---

## Test Plan

- Dry-run reports correct counts without deleting.
- Purge deletes events older than the threshold.
- `preserve_event_types` keeps specified events.
- Batched deletion commits between batches (verify via concurrent read
  during purge).
- Purge with `before_timestamp = 0` is a no-op.
