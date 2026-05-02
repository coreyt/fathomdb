---
title: TypeScript Public Interface
date: 2026-04-24
target_release: 0.6.0
desc: Public TypeScript surface for 0.6.0
blast_radius: src/ts/; design/bindings.md; design/errors.md; design/lifecycle.md; design/engine.md
status: draft
---

# TypeScript Interface

This file owns TypeScript-visible symbol spelling and export shape. Cross-
binding parity remains owned by `design/bindings.md`.

## Runtime surface

The canonical runtime verbs available to TypeScript callers are:

- `Engine.open(...)`
- `engine.write(...)`
- `engine.search(...)`
- `engine.close()`
- `admin.configure(...)`

All runtime operations are Promise-returning on the TS surface.

`Engine.open(...)` returns the engine handle plus the structured open report
owned by `design/engine.md`.

`Engine.open(path, options?)` accepts an options object with an `engineConfig`
member carrying the engine-owned knobs from `design/engine.md` in camelCase:

- `embedderPoolSize`
- `schedulerRuntimeThreads`
- `provenanceRowCap`
- `embedderCallTimeoutMs`
- `slowThresholdMs`

If TypeScript exposes ThreadsafeFunction handoff-pool sizing, that option is a
TS binding-runtime option beside `engineConfig`, not a canonical engine config
field and not a Python parity obligation.

## Engine-attached instrumentation / control

These are public instance methods, not extra top-level SDK verbs:

- `engine.drain(timeoutMs)`
- `engine.counters()`
- `engine.setProfiling(enabled)`
- `engine.setSlowThresholdMs(value)`

Subscriber attachment is provided by:

- `engine.attachSubscriber(callback, { heartbeatIntervalMs? })`

`callback` receives the stable `fathomdb` payload described in
`design/bindings.md`.

## Caller-visible data shapes

- `WriteReceipt.cursor`
- `SearchResult.projectionCursor`
- `SearchResult.softFallback.branch`

`softFallback.branch` uses the typed values owned by `design/retrieval.md`.

## Errors

TypeScript exposes one concrete leaf class per canonical row in
`design/errors.md`.

Leaf-class examples:

- `DatabaseLockedError`
- `CorruptionError`
- `MigrationError`
- `IncompatibleSchemaVersionError`
- `EmbedderIdentityMismatchError`
- `EmbedderDimensionMismatchError`
- `SchemaValidationError`
- `OverloadedError`
- `ClosingError`

TypeScript exports one catch-all base class, `FathomDbError`, and every
concrete leaf class in the `design/errors.md` matrix extends it. Open-time and
runtime leaf classes remain distinct, but callers can catch `FathomDbError`
for both.

## Non-presence

TypeScript does not expose recovery verbs or doctor-only flags. In particular,
there is no SDK equivalent of `recover`, `checkIntegrity`, `quick`, `full`, or
`roundTrip`. See `design/recovery.md`.
