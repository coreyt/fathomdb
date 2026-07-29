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

## Node write-item validity window (0.8.20 Slice 15b, TC-34)

`engine.write([...])` takes loose objects, not typed structs. A **node** item
accepts two optional validity keys:

- `validFrom` / `valid_from` — `number | null`, INCLUSIVE lower bound, INTEGER
  epoch **seconds** UTC. Omitted or `null` lands SQL NULL = unbounded below.
- `validUntil` / `valid_until` — `number | null`, EXCLUSIVE upper bound, same
  units. Omitted or `null` lands SQL NULL = unbounded above.

**BOTH spellings are accepted** for each bound. The camelCase spelling is
consulted first and the snake_case spelling is the fallback, mirroring the
existing edge `tValid` / `t_valid` precedent (which TC-33 aligns to the same
INTEGER epoch-seconds units — see below), so a caller porting from the Python
surface keeps working.

```typescript
await engine.write([
  {
    kind: "note",
    body: "…",
    sourceId: "s1",
    validFrom: 1_700_000_000,
    validUntil: 1_700_003_600,
  },
]);
```

The window is **half-open** `[validFrom, validUntil)`: an instant equal to
`validFrom` is IN, an instant equal to `validUntil` is OUT.

**Omitting both keys preserves existing default-view visibility.** The pair
binds NULL/NULL — exactly what every pre-slice row already carries — so an
unchanged caller sees unchanged behaviour.

Refusals (the rule is enforced in the engine's `validate_write`, so it is
identical across Rust / Python / TypeScript and cannot drift):

- Both bounds present with `validFrom >= validUntil` is an UNSATISFIABLE window
  and rejects with **`WriteValidationError`**. Validation runs before any insert,
  so the **whole batch** is rejected.

  > **BREAKING (0.8.20 Slice 22, decision #18).** This rejected with
  > `InvalidArgumentError` (napi code `FDB_INVALID_ARGUMENT`) **carrying both
  > bounds in `message`**, while a non-integral bound from the same call rejected
  > with `WriteValidationError`. Both are now `WriteValidationError` — one family
  > for the whole write-validation boundary (`dev/design/errors.md`, 2026-07-28
  > amendment). The envelope is now `FDB_WRITE_VALIDATION` with the fixed message
  > `"write validation error"` and `data: null`; **the bounds are gone**. A caller
  > that read them out of `message` must validate the pair before calling.
  > `InvalidArgumentError` is unchanged for every other use.
- A **one-sided** window is never refused, however extreme its single bound.
- A non-integral bound rejects with `WriteValidationError`; the value is never
  truncated or coerced.

These are keys on an existing verb, not a new verb: the runtime-verb surface
above is unchanged. The fields-only delta is **PROPOSED, NOT SIGNED**.

## Edge temporal fields (0.8.20 Slice 15c, TC-33)

An **edge** item accepts two optional temporal keys. As of TC-33
(HITL-RATIFIED 2026-07-21) these are **INTEGER epoch seconds (UTC)** — the same
representation as the node validity window and as storage — NOT ISO-8601
strings, which they used to be:

- `tValid` / `t_valid` — `number | null`, event valid-time. `null` = unknown /
  still valid.
- `tInvalid` / `t_invalid` — `number | null`, event invalid-time. `null` =
  **still valid**.

**BOTH spellings are accepted** for each field (camelCase first, snake_case
fallback), exactly as for the node window.

```typescript
await engine.write([
  {
    kind: "works_for",
    from: "bob",
    to: "acme",
    sourceId: "s1",
    tValid: 1_546_300_800, // 2019-01-01T00:00:00Z
    tInvalid: null,        // still valid
  },
]);
```

`null`/omitted is the ONLY way to say "unknown"; it lands SQL NULL, which reads
as **still valid**. A non-integral field rejects with `WriteValidationError` and
is never coerced — the same `json_i64_alt` validator serves the node window and
the edge fields, so the old string-accepting `json_str_alt` no longer applies.

**Layering note.** This is the GOVERNED SDK write surface. ISO-8601 survives
ONLY on the **BYO-LLM extractor wire** (`fathomdb.extract.v1`), where the engine
normalises each timestamp to epoch seconds with a HARD REJECTION of any value it
cannot parse — an unparseable timestamp must never coerce to NULL, because a
NULL `t_invalid` reads as "still valid" and would resurrect an invalidated edge.
Fields-only delta, **PROPOSED, NOT SIGNED**.

## Projection registry (0.8.20 Slice 15d, R-20-PR / C-1)

Two net-new governed verbs declare and inspect projections over interpretive
attributes. **PROPOSED, NOT SIGNED.**

- `engine.configureProjections(specs, drop?)` → `Promise<ProjectionDelta>`.
  Declarative, idempotent apply: the engine diffs `specs` against the durable
  registry and backfills the difference in one transaction. `drop` is EXPLICIT —
  omitting a live projection from `specs` does NOT drop it; removal requires
  naming it in `drop`. A destructive change (a role removal or a
  tokenizer/embedder change) without a drop throws `ProjectionDestructiveError`
  (`name`/`delta` fields, mapped from the `FDB_PROJECTION_DESTRUCTIVE` envelope).
  Re-applying an unchanged spec resolves to `{ unchanged: true }`.
- `read.projections(engine)` → `Promise<ProjectionSpec[]>`, sorted by name — the
  registry introspection (folded into `read.*`).

`ProjectionSpec` is
`{ name, roles: ProjectionRole[], fts, ftsTokenizer?, vector, vectorEmbedder?,
vectorDenseReadiness? }`.
`ProjectionRole` is the string union `"filterable" | "rankable" | "searchable"`;
`searchable→FTS` and `searchable→vector` are tier labels carried by the
`fts`/`vector` sub-object flags, not roles. Cheap roles (`filterable`,
`searchable→FTS`) build same-transaction; `rankable` and the `searchable→vector`
sub-target are persisted-but-deferred (reported in `ProjectionDelta.deferred`).
`ProjectionDelta` is
`{ built, dropped, deferred, unchanged, vectorUnsupportedKinds }`. Field names
are camelCase per this file's casing rule.

### `vectorUnsupportedKinds` (0.8.20 Slice 22, R-20-VC / TC-67)

`ProjectionDelta.vectorUnsupportedKinds` is a `string[]` of node **kinds** — not
attribute names. The first three arrays carry projection attribute names; this one
carries the vector-eligible node kinds present in the corpus that the vector
writer can **never** commit, so no `searchable→vector` declaration will ever
produce an embedding for them.

**What it means for your data.** Rows of a reported kind still get **FTS and
lexical search**; they will simply never participate in dense/vector retrieval —
in this session or any future one. Your options are to use one of the kinds the
engine's locked vocabulary maps, or to accept lexical-only retrieval for those
rows. Waiting is not one of them, which is exactly what the field exists to say:
before it, "no embedder attached this session" (transient) and "this kind will
never be embedded" (permanent) both arrived as the same `deferred` entry.

- **Sorted, de-duplicated, and empty (`[]`) rather than absent** — read it
  unconditionally.
- **A STATE report, not a diff.** It does not feed `unchanged`, so an idempotent
  re-apply resolves to `{ unchanged: true }` with `built`/`dropped`/`deferred`
  empty **and this array populated**.
- **Embedder-independent.** Identical whether or not the engine was opened with
  an embedder — the vocabulary is static, so the fact does not depend on the
  session. Do not read it as the deferral.
- **Output-only.** `configureProjections` accepts specs, never a delta, so this
  field has no inbound direction and cannot affect the `read.projections` →
  `configureProjections` round-trip.
- **Residual — computed at DECLARE time.** A non-committable kind written *after*
  the call is not in a delta you already hold. To refresh, re-apply the same spec:
  an idempotent no-op that returns a current report.
- **Not an error, not a readiness change.** Nothing is rejected and
  `vectorDenseReadiness` still reaches `"ready"` — an un-enrolled kind is not
  outstanding work.

### `vectorDenseReadiness` (0.8.20 Slice 20, R-20-DR)

`ProjectionSpec.vectorDenseReadiness` is **engine-set READ METADATA**, hung off
the `vector` sub-object, typed by the net-new exported string union

```ts
export type DenseReadiness = "ready" | "embedding";
```

It is `null`/omitted on every caller-authored spec and is populated only on the
way OUT of `read.projections(engine)` — and only for a spec that declares
`vector: true`. `filterable` and `searchable→FTS` are same-transaction (non-stale
on commit) so they have no readiness axis at all; `searchable→vector` is async
and rebuild-durable, so it carries one.

- **Exactly two spellings: `"ready"` and `"embedding"`.** `"pending"` is
  DELIBERATELY not one of them — that token is RESERVED for the orthogonal
  **admission** axis (quarantine/trust, an app judgment). Index-readiness and
  admission are different dimensions: a record can be admissible and still read
  `"embedding"`. Do not reuse the word.
- **Derived, never stored.** There is no schema step and no `SCHEMA_VERSION`
  bump; the value is computed per `read.projections` call from outstanding
  projection work (the same predicate `drain` uses), which is what makes
  `{vector-insert ∧ readiness := ready}` atomic by construction — `"ready"` can
  never be observed with the vector row absent.
- **Accept-inert on the way in.** Passing `vectorDenseReadiness` to
  `engine.configureProjections` neither stores nor changes anything: it is not
  part of the declaration and the engine always reports the derived truth. That
  is deliberate, so `read.projections` output stays feedable straight back into
  `configureProjections` as a no-op (`{ unchanged: true }`); an explicit `null`
  is normalized to `undefined` on the way in, exactly as for `ftsTokenizer` /
  `vectorEmbedder`.
- **Two shapes are still hard-rejected**, because they could never round-trip: a
  readiness supplied with `vector: false`, and any spelling outside
  `{"ready", "embedding"}` (including `"pending"`, `""`, and `"Ready"`). Both
  throw the EXISTING `InvalidArgumentError`, mapped from the `FDB_INVALID_ARGUMENT`
  envelope — **no new error type is minted**. `null`/omitted is always accepted.
- **Additive.** A caller who never reads the field sees identical behaviour, and
  the slice adds ZERO net-new governed commands; `DenseReadiness` is the only
  net-new export.

### `engine.drain()` is the flush-to-readiness barrier (0.8.20 Slice 20c, R-20-DR)

There is **no `flushEmbeddings()` verb**. The shipped `engine.drain(timeoutMs)` —
note **MILLISECONDS** here, seconds in Python — carries those semantics, so the
surface gains ZERO net-new governed commands. The pinned invariant, tested in
Rust, Python and TypeScript:

> `await engine.drain(timeoutMs)` resolving ⟹ `vectorDenseReadiness === "ready"`,
> **and every vector-eligible row has its vector row at rest.**

- **`drain` is a BARRIER, not a trigger.** It waits for the engine's projection
  runtime to go quiescent; it never schedules or wakes anything.
  Deferred/backfill work is enqueued on the **enqueue side** instead:
  `configureProjections` enrols the vector kinds and re-opens the stranded rows
  before it resolves, so the very next `drain` flushes them. Turning the dense arm
  on over an existing corpus is therefore just:

  ```ts
  await configureProjections(engine, [
    { name: "summary", roles: ["searchable"], vector: true },
  ]);
  await engine.drain(60_000); // flush the backfill
  // (await readProjections(engine))[0].vectorDenseReadiness === "ready"
  ```

- **Ordering does not matter.** Write-then-declare and declare-then-write behave
  identically. The write path performs the **same** backfill the declaration
  does, so rows of that kind written by an earlier session — for instance one
  opened with `useDefaultEmbedder: false`, where the declaration persisted but
  deferred — are picked up too, rather than being left behind a `"ready"` that is
  not true of them.
- **The dense arm covers only the engine's locked `kind` vocabulary.** A
  `searchable→vector` declaration turns the dense arm on for node kinds in
  `{email, article, paper, meeting, note, todo, doc}`. Rows of ANY other `kind`
  are accepted and stay lexically searchable, but get **no vector** and are not
  counted as outstanding work, so readiness still reaches `"ready"`. This is
  **not** an error condition: `engine.write` does not reject them, nothing
  rejects, and there is no verb to ask about it.
- **Idempotent.** Re-applying an already-satisfied declaration re-embeds nothing
  and resolves `{ unchanged: true }`.
- **Dropping the last `searchable→vector` declaration turns the dense arm back
  off.** `engine.configureProjections([], ["summary"])` un-enrols the node kinds
  that declaration enrolled, so later writes enqueue no embed and `drain` no
  longer waits on them. It **deletes no embedding** — vectors already at rest
  survive the drop, exactly as they always have. Re-declaring re-enrols and
  backfills, so a row written while the arm was off is picked up, not stranded.
  Edge-body vectors are unaffected.
- **The dense arm requires the `searchable` ROLE, not merely `vector: true`**
  (0.8.20 Slice 21c, `TC-71`). A spec such as
  `{ name: "summary", roles: ["filterable"], vector: true }` is accepted and
  round-trips verbatim, but it is **INERT**: it enrols no kind, backfills nothing,
  and makes no later write enqueue an embedding. Previously the engine keyed the
  dense arm off the stored `vector` sub-object alone, so declaring that
  combination against a session with an embedder silently embedded the whole
  corpus. The inverse moves with it: demoting the last `searchable→vector`
  projection to `filterable + vector`, or dropping it while an inert
  `filterable + vector` sibling survives, now un-enrols exactly as a literal drop
  does. The name is still reported in `deferred`.
- **Graceful-absent without a live embedder:** the declaration persists and
  defers, then grafts on when re-applied in a session that has one.
- **…but graceful-absent stops at the enrolment boundary** (fix-4). Once a kind
  IS enrolled — i.e. some earlier session DID have an embedder — writing that
  kind from a session opened with `useDefaultEmbedder: false` leaves real dense
  work outstanding, and this session cannot satisfy it. The write is **accepted**
  and stays lexically searchable, but `vectorDenseReadiness` reads `"embedding"`
  and `drain` rejects with `SchedulerError` for the rest of that session, however
  long you wait. It is **not** lost: no failure is recorded and no terminal is
  written, so the next session opened WITH an embedder embeds it through the
  ordinary scheduler — no re-apply, no operator `rebuild`. Expect the timeout
  there and do not read it as data loss.
- **`drain` stays bounded** and rejects with the existing timeout error rather
  than hanging; size `timeoutMs` for the backfill you just asked for.

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

### `denseDisabled` and the cached equivalence verdict (0.8.20 Slice 22, TC-68)

`OpenReport.denseDisabled` / `engine.denseDisabled()` still mean "the dense arm is
refusing", and the typed query-time `VectorEquivalenceMismatchError` and the
`searchTextOnly` fallback are unchanged. **What changed is when the check behind
them runs.** The 0.8.18 vector-equivalence self-check used to re-embed its 45
probes on *every* open; since 0.8.20 the engine caches that verdict against a
fingerprint of the embedder identity, the pinned mean vector, the probe fixture,
the divergence floors and the stored reference baseline. An open whose fingerprint
is unchanged does **zero** probe embeds — the dominant cost of opening a
vector-indexed workspace with a live embedder — and reuses the previous verdict.

Read `denseDisabled` accordingly: it reports the arm's status **as verified at the
last open whose fingerprint differed**, not a fresh re-verification at this open. A
backend that drifts without changing its declared identity (the same model moved
between CPU and GPU, or rebuilt against a new library) is therefore no longer
caught per-open. An identity *change* is unaffected: it still rejects the open with
`EmbedderIdentityMismatchError`, ahead of any cache. An unreadable or absent cached
verdict runs the probe rather than trusting it. Full rationale and the residual:
`dev/design/0.8.20-tc68-equivalence-probe-fingerprint-cache.md`.

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

## `view` on `search` / `searchTextOnly` (0.8.20 Slice 15b fix-2)

**Status: PROPOSED / NOT SIGNED.**

Both search verbs take the SAME optional `view` argument the five read verbs
take, as a trailing options object.

```ts
engine.search(query, filter?, rerankDepth?, useGraphArm?, alpha?, poolN?,
              explain?, view?): Promise<SearchResult>
engine.searchTextOnly(query, view?): Promise<SearchResult>
```

`view` is the exported `ReadView` interface — the same shape `read.get` /
`read.list` / `graph.neighbors` accept (`camelCase` here, `snake_case` in
Python), with no new type minted.

- Omitted / `undefined` is the STRICT view: active-only, non-superseded, and
  valid AT QUERY TIME.
- `{ validAsOf: t }` evaluates validity at the bound instant `t` (INTEGER epoch
  SECONDS, UTC). Half-open, matching the write side and the read verbs:
  `t === validFrom` is IN, `t === validUntil` is OUT.
- `{ includeOutOfWindow: true }` returns hits whatever their window.

**Default behaviour change.** A node whose window has closed (or has not opened)
is no longer returned by a default `search`. This is a no-op on any corpus that
never authored a window: omitting the write fields lands NULL/NULL, and NULL is
unbounded, so every pre-existing row still matches.

**Axis scope — VALIDITY only.** `{ includeSuperseded: true }` and
`{ includeInactive: true }` reject with `InvalidArgumentError` on the search
path; they are REFUSED rather than silently ignored, because search hydrates
from projection indexes that are not version-complete. Use `read.list` to
enumerate history.

These are ARGUMENTS, not new verbs — the governed command surface
(`src/conformance/governed-surface-allowlist.json`) is unchanged.

## Non-presence

TypeScript does not expose recovery verbs or doctor-only flags. In particular,
there is no SDK equivalent of `recover`, `checkIntegrity`, `quick`, `full`, or
`roundTrip`. See `design/recovery.md`.
