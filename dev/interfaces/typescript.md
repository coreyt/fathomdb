---
title: TypeScript Public Interface
date: 2026-04-24
target_release: 0.6.0
desc: Public TypeScript surface for 0.6.0
blast_radius: src/ts/; design/bindings.md; design/errors.md; design/lifecycle.md; design/engine.md
status: locked
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

`Engine.open(...)` returns a Promise resolving to the engine handle. The
structured open report owned by `design/engine.md` is accessible after open
via `engine.openReport()` (see Engine-attached instrumentation / control
below).

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
- `engine.openReport()`
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

## Default embedder

`Engine.open(path, { useDefaultEmbedder: true })` opts into the engine's
default embedder (`fathomdb-bge-small-en-v1.5`). On first use, weights
are downloaded from HuggingFace and cached under
`~/.cache/fathomdb/embedders/`; subsequent opens hit the warm cache. See
`dev/adr/ADR-0.7.1-default-embedder-weight-fetch.md` for the network-
surface scope (opt-in only; sha256-verified; visible via
`OpenReport.embedderEvents`). The default (`useDefaultEmbedder: false`
or omitted) opens without an embedder; subsequent vector writes reject
with `EmbedderNotConfiguredError`.

`OpenReport` carries four embedder-related fields surfaced by EU-6
(camelCase per TS convention): `embedderDownloadMs`, `embedderEvents`,
`embedderMeanCenteringRequired`, and `embedderMeanVecPinned`. Each entry
in `embedderEvents` is a discriminated-union object: `kind` is one of
`"DefaultEmbedderDownload"`, `"DefaultEmbedderCacheHit"`,
`"MeanVecPinned"`; the remaining optional fields carry the variant
payload in camelCase.

### Custom embedder implementations (deferred to 0.8.x)

Supplying a custom TypeScript `Embedder` implementation requires a
napi-rs callback bridge subject to ADR-0.6.0-embedder-protocol
Invariant 3 (no host-side log emission during `embed()`). That bridge
is a multi-slice campaign deferred to 0.8.x. In 0.7.1 the binding
surface is binary: `useDefaultEmbedder: true` (engine's bge-small) or
omitted/`false` (no embedder; vector writes reject with
`EmbedderNotConfiguredError`).

## Non-presence

TypeScript does not expose recovery verbs or doctor-only flags. In particular,
there is no SDK equivalent of `recover`, `checkIntegrity`, `quick`, `full`, or
`roundTrip`. See `design/recovery.md`.
