# Design: Automatic And Background Retention For Operational Collections

## Purpose

Define how retention policies in `retention_json` can move from explicit
admin-only enforcement to automatic/background execution without pushing a job
queue or workflow scheduler into the core engine.

The current system already stores retention policy and exposes explicit
compaction and purge commands. This design adds a durable retention execution
model that is automatic in practice while preserving the engine/application
boundary.

## Decision Summary

- Retention policy remains collection metadata in `retention_json`.
- The engine should provide planning and execution primitives, not an internal
  scheduler queue.
- Background scheduling should live outside the engine, in
  `fathom-integrity`, the embedding application, or a host supervisor such as
  `systemd` or `cron`.
- Retention execution must remain idempotent, bounded, provenance-visible, and
  dry-run capable.
- The implemented slice reuses the existing policy shapes and existing
  compact/purge semantics.
- The current operator surface is one-shot planning/execution. Recurring
  scheduling remains intentionally external.

## Goals

- Make retention enforcement operationally reliable without requiring manual
  ad hoc operator runs.
- Preserve recoverability and provenance visibility.
- Keep retention execution bounded so one background cycle cannot monopolize
  the single-writer path.
- Avoid encoding a job scheduler inside the SQLite schema.

## Non-Goals

- An in-engine job queue.
- Domain-specific scheduling semantics.
- Arbitrary cron parsing inside the database engine.
- Multi-node distributed coordination.
- Retroactive deletion of canonical graph data outside documented operational
  collection retention rules.

## Why The Scheduler Should Stay Outside The Engine

The engine already has the right primitives:

- durable collection metadata
- explicit admin operations
- single-writer transactional execution
- provenance and diagnostics

What it does not need is an internal task scheduler table or background thread
contract that every embedding environment must now own forever. That would
reopen exactly the kind of engine/application-boundary sprawl the repository is
trying to avoid.

The correct split is:

- engine: retention planning and execution primitives
- operator/app layer: when and how often those primitives are called

## Existing Policy Shapes

The current policy model already supports:

- `keep_all`
- `purge_before_seconds { max_age_seconds }`
- `keep_last { max_rows }`

The first automatic-retention slice should keep those policy shapes unchanged.
Combined policies can be added later through a new format version if needed.

## Engine Surface

The engine should add two explicit admin operations.

### 1. `plan_operational_retention(now, collections?, max_collections)`

Returns which collections are due for retention action and what action would be
taken:

- no-op
- purge before timestamp
- compact to keep last N rows

This surface exists for dry-run operator visibility and scheduling decisions.

### 2. `run_operational_retention(now, collections?, max_collections, dry_run)`

Executes due retention actions and returns a bounded report:

- collections examined
- collections acted on
- action type per collection
- rows deleted per collection
- rows remaining
- provenance event IDs or action identifiers

`dry_run = true` must be supported.

## Execution Semantics

### Collection Selection

- Only collections with non-`keep_all` policies are eligible.
- Disabled collections remain eligible unless an explicit policy says
  otherwise; retention is storage hygiene, not write admission.

### Action Mapping

- `purge_before_seconds { max_age_seconds }`
  maps to existing purge semantics using `now - max_age_seconds`.
- `keep_last { max_rows }`
  maps to existing compaction semantics.
- `keep_all`
  is a no-op.

### Boundedness

Each retention run must be bounded by:

- maximum collections processed per invocation
- maximum rows removed per collection, or a clear continuation strategy
- one transaction per collection

That keeps retention from monopolizing the writer path and makes retries
predictable.

## Scheduling Layer

Automatic/background behavior should live in operator tooling, not the engine.

Recommended first surface:

```text
fathom-integrity plan-operational-retention --db <path> --bridge <path>
fathom-integrity run-operational-retention --db <path> --bridge <path> --dry-run
fathom-integrity run-operational-retention --db <path> --bridge <path>
```

The long-running loop is intentionally not in the engine or CLI. A production
deployment can drive recurring retention through:

- `cron`
- `systemd timers`
- Kubernetes `CronJob`
- application-owned periodic tasks

## Provenance And Observability

Every non-dry-run retention action should emit provenance-visible records that
identify:

- collection name
- action type (`purge_before_seconds` or `keep_last`)
- effective cutoff or row limit
- rows removed
- execution timestamp

Admin/reporting surfaces should expose:

- last retention run time per collection
- last action result
- whether a collection is overdue for retention

That may require a small engine-owned metadata table such as
`operational_retention_runs`, but only for retention reporting, not scheduling.

## Failure Handling

- Failure in one collection must not corrupt another collection’s retention
  work.
- Partial progress across collections is acceptable; each collection action is
  independently atomic.
- Retry is safe because planning and execution are idempotent with respect to
  current timestamps and current row counts.

## Recovery And Bootstrap Requirements

- Retention policy already lives in `operational_collections`, so export and
  recovery preserve the contract.
- Background retention metadata, if added, must also be exportable and
  rebuild-safe.
- Compaction/purge actions must keep `operational_current`,
  `operational_filter_values`, and any future secondary index tables
  transactionally consistent.

## Verification

Implementation should add requirement-level tests for:

- plan operation returns the expected action for each current policy mode
- run operation correctly maps retention policy to purge/compact behavior
- dry-run reports actions without mutation
- bounded multi-collection runs stop at configured limits
- repeated runs are idempotent
- provenance/admin reporting captures retention execution
- Go operator tooling can invoke plan/run retention against a real bridge

## Risks And Tradeoffs

### Risks Accepted By This Design

- The engine does not self-schedule retention with no operator participation.
- Deployments still need an explicit runtime owner for the background loop.

### Risks Reduced By This Design

- Retention stops depending on ad hoc manual execution.
- The engine avoids growing an internal queue/scheduler subsystem.
- Retention remains auditable, bounded, and recoverable.

## Bottom Line

Automatic/background retention now ships as engine plan/run primitives plus
external scheduling. The engine decides what retention action is due and
executes it safely; the operator layer or host scheduler decides when to call
it.
