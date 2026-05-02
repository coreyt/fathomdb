---
title: Projections Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: FTS and vector projection model, projection status, and freshness wiring
blast_radius: FTS/vector derived state; REQ-008, REQ-013, REQ-014, REQ-015, REQ-029, REQ-055, REQ-059
status: locked
---

# Projections Design

This file owns derived FTS/vector state, projection status semantics, and the
relationship between canonical writes, terminal projection states, and
`projection_cursor`.

## Push model

0.6.0 projections are eager and post-commit:

1. the canonical write commits first
2. the scheduler dispatches projection work after commit
3. vector rows commit later on success
4. terminal failure is recorded durably and the cursor still advances past that
   batch

Canonical and FTS visibility are immediate at write commit. Vector visibility
is governed by terminal projection state.

## Terminal states

Projection status is a three-state runtime view:

- `Pending` — canonical write accepted; terminal projection outcome not yet
  reached
- `Failed` — retries exhausted for the batch; no vector projection was
  materialized for that batch
- `UpToDate` — vector projection for the relevant cursor is committed

`projection_cursor` advances only on terminal outcome (`UpToDate` or `Failed`),
never for in-flight work.

## `projection_failures`

After the fixed retry budget is exhausted, 0.6.0 records one durable failure
event in the op-store `projection_failures` collection.

Storage class:

- collection kind = `append_only_log`
- authoritative table = `operational_mutations`
- rows survive restart and remain available for diagnosis

The failure row is an audit record, not a durable work queue. Restart does not
implicitly clear it or convert it back into pending work once the cursor has
advanced past the failed batch.

## Regenerate workflow

The accepted operator workflow name is **regenerate**, but in 0.6.0 it is
implemented by the existing recovery surface
`fathomdb recover --accept-data-loss --rebuild-projections`; there is no
separate top-level `regenerate` CLI verb.

Workflow contract:

1. failure is recorded durably in `projection_failures`
2. normal reads continue, with vector freshness lag explainable by failure
   status
3. operator explicitly invokes the regenerate workflow
4. regeneration rebuilds projections from canonical rows, not from previously
   failed vector rows

This keeps projection retry policy fixed and automatic behavior narrow, while
still giving operators a concrete repair path.

## Restart semantics

0.6.0 keeps no durable projection queue table.

- Rows whose canonical cursor is ahead of `projection_cursor` are re-enqueued on
  open because they are still pending work.
- Rows already marked terminal-failed are not silently retried on restart,
  because `projection_cursor` has advanced past them.
- The explicit regenerate workflow is therefore the only repair path for
  exhausted failures in 0.6.0.
