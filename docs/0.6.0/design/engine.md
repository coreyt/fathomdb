---
title: Engine Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: Runtime open/close, writer/read path, engine config, and cursor semantics
blast_radius: fathomdb-engine runtime/writer/reader; interfaces/*.md; acceptance.md REQ-020..REQ-055 intersections
status: draft
---

# Engine Design

This file owns the runtime open path, `Engine` lifetime, writer / reader split,
and the concrete meaning of the cursor values surfaced at the public API.

## Open path

`Engine.open` owns:

1. path canonicalization
2. sidecar lock acquisition
3. SQLite open + PRAGMA application
4. always-on corruption detection
5. migration execution
6. embedder identity check
7. embedder warmup
8. writer / scheduler startup

The detailed step ordering is aligned to HITL `ENG1`; this file is the
authoritative home for the final ordered sequence.

`Engine.open` runs only the frozen always-on detection subset from HITL `R1`.
0.6.0 exposes no env/config integrity knob at open and does not accept any SDK
config that turns quick/full integrity or round-trip verification on during the
open path.

The corruption-producing stages are the four `OpenStage` values owned by
`design/errors.md`:

- `WalReplay` for WAL replay verdicts (`E_CORRUPT_WAL_REPLAY`)
- `HeaderProbe` for page-1/header sanity (`E_CORRUPT_HEADER`)
- `SchemaProbe` for schema / migration-table consistency (`E_CORRUPT_SCHEMA`)
- `EmbedderIdentity` for corrupt stored embedder-profile rows
  (`E_CORRUPT_EMBEDDER_IDENTITY`)

`E_CORRUPT_INTEGRITY_CHECK` is not an open-path code. That code belongs only to
`doctor check-integrity --full` per `design/recovery.md`.

## Writer / reader split

- One dedicated writer thread owns the only write connection.
- Reader connections are pooled and never serialize behind one connection.
- `admin.configure` is writer-thread work. It is not a side path and does not
  bypass the same lock, migration, or error rules as normal writes.

## `PreparedWrite::AdminSchema` provenance

`PreparedWrite::AdminSchema(AdminSchemaWrite)` exists because
`admin.configure` is already accepted public surface in 0.6.0 and its DDL work
must travel through the same writer-thread machinery as every other state
change. The variant is not speculative extension surface and must not be
deleted as "future admin work."

Why it exists:

- `admin.configure` is a top-level SDK verb under REQ-053.
- AC-003c and AC-021 already require observable admin work on the canonical
  engine path.
- ADR-0.6.0-prepared-write-shape commits to one typed carrier enum for writer
  submissions; admin DDL therefore needs an engine-side representation inside
  that carrier.

Status in 0.6.0:

- The existence of the `AdminSchema` variant is locked for 0.6.0.
- The exact field set of `AdminSchemaWrite` remains owned here and by the
  interface docs. It is still an internal engine carrier, not a promise that
  callers directly construct arbitrary DDL payloads.

## Batch submission semantics

`Engine.write(&[PreparedWrite])` is one ordered writer submission in 0.6.0.

The contract is:

1. The slice is validated as one batch before commit-sensitive work begins.
2. If any element fails save-time validation, the batch is rejected and no
   SQLite write transaction commits any member of the batch.
3. If validation succeeds, the writer executes the batch in caller order inside
   one SQLite transaction.
4. Mixed canonical rows and op-store rows in the same slice commit atomically.
5. One write cursor `c_w` is allocated for the committed batch as a whole, not
   per element.
6. Projection work derived from committed writes is enqueued only after that
   transaction commits.

0.6.0 does not regroup the slice by variant, split one slice into multiple
transactions, or expose partial-success semantics. The caller-visible contract
is "all committed at one cursor or none committed."

`admin.configure` uses the same writer machinery and transactional rules, but
it is a separate public verb from `write`; this file does not widen REQ-053
into a promise that arbitrary mixed admin-and-data batches are first-class user
surface.

## Cursor contract

Two distinct cursor concepts exist in 0.6.0:

- **Write cursor (`c_w`).** Returned on write commit. Identifies the accepted
  canonical write transaction.
- **`projection_cursor`.** Returned on read transactions. Identifies the latest
  projection-visible point.

Canonical and FTS visibility are immediate after the write commit. Vector
visibility catches up asynchronously. A caller that needs vector
read-after-write semantics polls until `read_projection_cursor >= c_w`.

This distinction is load-bearing for REQ-055 and AC-059b and must remain
consistent across `architecture.md`, `requirements.md`, `acceptance.md`, and
`interfaces/*.md`.

## EngineConfig ownership

This file owns the canonical engine-config knob set and the rationale for any
publicly exposed tunables. A knob is not considered stable public surface until
it is named and justified here.

Engine-owned 0.6.0 knobs include runtime controls such as
`embedder_pool_size` and `scheduler_runtime_threads`. Binding adapter mechanics
that exist only to bridge a language runtime into the engine are not part of
this canonical knob set even if a binding chooses to surface them near
`Engine.open`.

## Debug-only runtime guard

The embedder-protocol `engine_in_call` guard is a debug-build deadlock
tripwire, not a user-facing contract surface.

- Purpose: catch re-entrant engine calls made from inside `Embedder.embed()`
  during development and CI.
- Behavior: debug builds may panic immediately when the guard detects
  re-entrancy.
- Non-contract: release builds do not promise a stable panic string, stable
  exception class, or a public configuration flag for this guard.

The stable 0.6.0 contract is the embedder invariant itself ("no engine
callbacks from `embed()`"), not the exact mechanics of the debug assertion.
