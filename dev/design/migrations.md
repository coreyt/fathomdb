---
title: Migrations Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: Auto-migration behavior at Engine.open and accretion-guard ownership
blast_radius: fathomdb-schema; Engine.open; REQ-042, REQ-045
status: draft
---

# Migrations Design

This file owns the migration loop that runs during `Engine.open`, the per-step
event contract, and the accretion-guard rules cited by REQ-042 / REQ-045.

## Ownership boundary

This file owns:

- when schema migrations run on the open path
- the migration-step event payload schema
- the migration portion of the open-report contract
- the canonical `MigrationError` naming used on open failure
- the semantic rule enforced by the migration accretion guard

This file does not own:

- outer `Engine.open` lifetime / startup ordering (`design/engine.md`)
- lifecycle phase routing (`design/lifecycle.md`)
- binding-specific error spelling or result wrappers (`interfaces/*.md`)
- CI implementation of the accretion-guard linter (`design/release.md`)

Migration step events may be delivered through the lifecycle subscriber route,
but the step payload itself remains migration-owned.

## Open-path contract

0.6.0 runs schema migration automatically during `Engine.open`.

The migration slice of the open path is:

1. read the current `PRAGMA user_version`
2. reject 0.5.x-shaped databases and unsupported legacy layouts as
   `IncompatibleSchemaVersion` before any migration step runs
3. compare supported 0.6.x schema revisions to the engine-supported schema
   version
4. apply every required migration step in ascending order
5. advance `PRAGMA user_version` only as each step commits successfully
6. return the fully-updated schema version on success

`design/engine.md` owns the full open-step ordering around migrations. This
file owns only the migration loop once that stage is reached.

The migration loop is not a 0.5.x upgrade path. AC-047 and
ADR-0.6.0-no-shims-policy remain authoritative: 0.5.x-shaped databases
hard-error before partial read/write and before migration authorship rules are
considered.

## Public step-event schema

REQ-042 / AC-046b / AC-046c make the step-event payload public. The 0.6.0
public payload carries exactly these fields:

- `step_id`
- `duration_ms`
- `failed`

Semantics:

- one event is emitted per applied step
- successful step events carry `failed: false`
- a failed step emits exactly one event for that step with `failed: true`
- `duration_ms` is populated for both success and failure

  0.6.0 may carry additional internal migration metadata in implementation logs,
  but those fields are not part of the public migration-step payload contract
  until accepted elsewhere.

### Routing split with lifecycle

When migrations are observable through the host subscriber:

- lifecycle owns the surrounding route and any phase tag on the outer event
- migrations own the typed step payload listed above

This split is load-bearing and must remain consistent with
`design/lifecycle.md`.

## Open-report contract

REQ-042 requires the open call to report applied version + per-step duration on
completion or failure.

The migration-owned fields are:

- `schema_version_before`
- `schema_version_after`
- `migration_steps`

`migration_steps` is the ordered list of step payloads emitted during the open
attempt, using the same `step_id` / `duration_ms` / `failed` schema above.

Success semantics:

- `schema_version_before` is the observed pre-migration `PRAGMA user_version`
- `schema_version_after` is the post-migration `PRAGMA user_version`
- `migration_steps` includes one successful payload per applied step

Failure semantics:

- `MigrationError` carries `schema_version_before`
- `MigrationError` carries `schema_version_current`, the last committed
  `PRAGMA user_version` observed after the failed attempt
- `MigrationError` carries the ordered `migration_steps`, including the final
  failed step entry
- `schema_version_after` is not reported as a success value on failure

`design/engine.md` owns the outer success wrapper that returns the engine
handle alongside this migration report.

## Failure surface

The canonical open-path error name is `MigrationError`.

`acceptance.md` previously used `MigrationFailed` wording for AC-046c. The
canonical corpus name is now `MigrationError`, and the acceptance text has been
aligned to this file, `design/errors.md`, and `design/bindings.md`.

`MigrationError` is an open-path typed failure, not a lifecycle-owned payload
and not a runtime `EngineError` variant.

## Accretion guard

REQ-045's migration accretion guard is a semantic rule on migration authorship:

- every post-v1 migration that adds a table or column must name one table or
  column it removes, or
- document inline why removal is impossible in that migration

This file owns the meaning of that rule.

The release/CI gate that enforces the rule is owned by `design/release.md`.
Both docs should cross-reference each other rather than duplicate the same
linter description.
