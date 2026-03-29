# Operator Guide: Operational Retention Scheduling

## Retention Is Not Automatic

Declaring a `retention_json` policy on an operational collection does **not**
enable automatic retention enforcement. The engine stores the policy and
provides planning and execution primitives, but the operator must schedule
periodic invocation. Without external scheduling, operational mutation data
grows unbounded regardless of the declared policy.

This is an intentional design decision. The engine avoids embedding an internal
job scheduler or background-thread contract. Instead it provides idempotent,
bounded, provenance-visible retention primitives that any scheduling layer can
drive safely.

See [`dev/design-automatic-background-retention.md`](design-automatic-background-retention.md)
for the full design rationale.

## Retention Policy Modes

When registering an operational collection, `retention_json` accepts one of
three modes:

| Mode | JSON | Behavior |
|------|------|----------|
| Keep all | `{"mode":"keep_all"}` | No-op; mutations are never pruned |
| Purge by age | `{"mode":"purge_before_seconds","max_age_seconds":86400}` | Deletes mutations older than `now - max_age_seconds` |
| Keep last N | `{"mode":"keep_last","max_rows":1000}` | Deletes the oldest mutations beyond the row limit |

## Retention API Reference

### Rust

```rust
// Plan: inspect which collections are due for retention and what actions
// would be taken, without mutating anything.
let plan: OperationalRetentionPlanReport = engine.plan_operational_retention(
    now_timestamp,              // i64 — Unix epoch seconds
    collection_names,           // Option<&[String]> — filter to specific collections
    max_collections,            // Option<usize> — bound the number of collections examined
)?;

// Execute: run retention actions. Each collection is processed in its own
// transaction so a failure in one collection does not affect others.
let report: OperationalRetentionRunReport = engine.run_operational_retention(
    now_timestamp,              // i64 — Unix epoch seconds
    collection_names,           // Option<&[String]> — filter to specific collections
    max_collections,            // Option<usize> — bound the number of collections examined
    dry_run,                    // bool — if true, report actions without mutation
)?;
```

### Python

```python
import time
from fathomdb import Engine

engine = Engine(db_path="/path/to/db")

now = int(time.time())

# Plan
plan = engine.plan_operational_retention(
    now,
    collection_names=["audit_log"],   # optional
    max_collections=10,               # optional
)

# Execute
report = engine.run_operational_retention(
    now,
    collection_names=["audit_log"],   # optional
    max_collections=10,               # optional
    dry_run=False,                    # set True for a side-effect-free report
)
```

### Bridge (JSON over stdio)

Plan request:

```json
{
  "protocol_version": 1,
  "database_path": "/path/to/db",
  "command": "plan_operational_retention",
  "now_timestamp": 1711699200,
  "collection_names": ["audit_log"],
  "max_collections": 10
}
```

Run request:

```json
{
  "protocol_version": 1,
  "database_path": "/path/to/db",
  "command": "run_operational_retention",
  "now_timestamp": 1711699200,
  "collection_names": ["audit_log"],
  "max_collections": 10,
  "dry_run": false
}
```

### Go CLI (`fathom-integrity`)

```bash
# Plan — inspect pending retention actions
fathom-integrity plan-operational-retention \
  --db /path/to/db \
  --bridge /path/to/fathomdb-admin-bridge

# Execute — run retention with a dry-run first
fathom-integrity run-operational-retention \
  --db /path/to/db \
  --bridge /path/to/fathomdb-admin-bridge \
  --dry-run

# Execute — run retention for real
fathom-integrity run-operational-retention \
  --db /path/to/db \
  --bridge /path/to/fathomdb-admin-bridge

# Optional flags:
#   --collections-json '["audit_log"]'   filter to specific collections
#   --now <unix-timestamp>               override the current time
#   --max-collections <n>                bound the number of collections examined
```

## Scheduling Examples

### Cron

```cron
# Run operational retention every hour
0 * * * * /usr/local/bin/fathom-integrity run-operational-retention --db /data/fathom.db --bridge /usr/local/bin/fathomdb-admin-bridge

# Dry-run daily at 06:00 for monitoring/alerting
0 6 * * * /usr/local/bin/fathom-integrity run-operational-retention --db /data/fathom.db --bridge /usr/local/bin/fathomdb-admin-bridge --dry-run >> /var/log/fathom-retention-dryrun.log 2>&1
```

### systemd Timer

```ini
# /etc/systemd/system/fathom-retention.service
[Unit]
Description=FathomDB operational retention

[Service]
Type=oneshot
ExecStart=/usr/local/bin/fathom-integrity run-operational-retention --db /data/fathom.db --bridge /usr/local/bin/fathomdb-admin-bridge
```

```ini
# /etc/systemd/system/fathom-retention.timer
[Unit]
Description=Run FathomDB operational retention hourly

[Timer]
OnCalendar=hourly
Persistent=true

[Install]
WantedBy=timers.target
```

### Python (periodic loop)

```python
import time
from fathomdb import Engine

engine = Engine(db_path="/data/fathom.db")
INTERVAL_SECONDS = 3600  # every hour

while True:
    now = int(time.time())
    try:
        report = engine.run_operational_retention(now, dry_run=False)
        print(f"Retention: examined={report.collections_examined}, "
              f"acted_on={report.collections_acted_on}")
    except Exception as e:
        print(f"Retention failed: {e}")
    time.sleep(INTERVAL_SECONDS)
```

## Execution Semantics

- **Idempotent.** Running retention multiple times with the same `now` timestamp
  is safe. After the first pass deletes eligible rows, subsequent passes find
  nothing to delete.

- **Bounded.** Use `max_collections` to limit how many collections are processed
  in a single invocation, preventing retention from monopolizing the
  single-writer path.

- **Per-collection atomic.** Each collection's retention action runs in its own
  transaction. A failure in one collection does not roll back work already
  committed for other collections.

- **Provenance-visible.** Non-dry-run retention actions emit provenance events
  recording the collection name, action kind, effective cutoff or row limit,
  rows deleted, and execution timestamp.

- **Dry-run capable.** Pass `dry_run=true` to see what retention would do
  without mutating any data.

## Provenance Retention (Planned)

The `provenance_events` table also grows unbounded. A future
`purge_provenance_events` API is planned (tracked as C-5, Wave 2). When that
API ships, operators should schedule provenance retention alongside operational
retention to keep the database size bounded.

Until then, operators managing high-write-volume deployments should monitor
`provenance_events` row counts and plan for the additional scheduling
requirement.
