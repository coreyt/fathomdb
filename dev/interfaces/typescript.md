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

EU-6 FIX-2 refined the `EmbedderEvent` type from a wide
`Option`-collapsed interface to a true discriminated union of
per-variant interfaces (`DefaultEmbedderDownloadEvent`,
`DefaultEmbedderCacheHitEvent`, `MeanVecPinnedEvent`) plus an
`UnknownEmbedderEvent` forward-compat fallback. The unknown member is
part of the published union for soundness: a future or replaced native
extension may emit kinds this build does not know about. Because the
fallback's `kind` field is the open type `string`, tsc cannot exclude
it purely from a literal `event.kind === "..."` check on the bare
union — gate the discriminant chain on `isKnownEmbedderEvent` first to
recover precise narrowing on the three known variants:

```typescript
import { Engine, isKnownEmbedderEvent } from "fathomdb";

const engine = await Engine.open(path, { useDefaultEmbedder: true });
const report = engine.openReport();
for (const event of report.embedderEvents) {
  if (isKnownEmbedderEvent(event)) {
    if (event.kind === "DefaultEmbedderDownload") {
      // tsc narrows: event.bytes is number, event.url is string.
      log(`downloaded ${event.bytes} bytes from ${event.url}`);
    } else if (event.kind === "MeanVecPinned") {
      log(`mean vec pinned at ${event.docCount} docs (dim=${event.dim})`);
    }
  } else {
    // `event` is `UnknownEmbedderEvent` — only `event.kind` is typed;
    // other fields are `unknown` via the index signature.
    log(`unknown embedder event kind: ${event.kind}`);
  }
}
```

The two-step pattern (guard, then discriminate) is required because
TS literal narrowing on a discriminated union cannot remove an open-
typed member from the union when the discriminant is a literal —
`"DefaultEmbedderDownload"` could equal *any* `string`, so the unknown
fallback stays in the narrowed type and widens payload field access to
`unknown`. The exported `isKnownEmbedderEvent` type guard excludes the
unknown member up front, and the inner `if (event.kind === "...")` chain
then narrows precisely to one variant interface.

### Shipped feature axis (EU-6 FIX-1)

Released `.node` binaries published to npm are compiled with the `default-embedder`
Cargo feature ON (see `src/ts/package.json`'s
`build:native` script, consumed by `release.yml`'s build-napi job), so
`useDefaultEmbedder: true` materialises a real bge-small embedder
against the published artifact without any extra install step. The no-
feature build path is preserved as a CI sanity check (informational
wheel-size signal on the minimal-deps tree), not a shipped artifact.

The `test-hooks` Cargo feature is dev-only and never ships: methods
like `writeVectorForTest` and the force-panic probe do not exist on
installed `.node` binaries. They are exposed only when the binding is
built via `npm run build:native:debug` (the script the vitest suite
uses). End-user callers should not rely on these symbols.

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
