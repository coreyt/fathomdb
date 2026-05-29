// EU-6 FIX-2 — TypeScript compile-time + runtime narrowing test for
// `EmbedderEvent` (AC-FIX2-5, AC-FIX2-6).
//
// Per `dev/design/0.7.1-EU-6-FIX-2-design.md` §5.2:
//
// - Compile-time check is implicit. `npm test` runs `tsc -p
//   tsconfig.json` before `node --test`; if any `// @ts-expect-error`
//   directive is wrong, or any positive narrowing line fails to compile,
//   tsc fails the whole test step.
// - Runtime check asserts the field set actually emitted by the engine
//   matches what the union promises per `kind`.
//
// On current `main` this file fails to COMPILE because:
//   1. `DefaultEmbedderDownloadEvent`, `DefaultEmbedderCacheHitEvent`,
//      `MeanVecPinnedEvent` are not exported from `../src/index.js`.
//   2. The current wide `EmbedderEvent` interface has `bytes?: number |
//      null`, so `const bytes: number = event.bytes` inside the
//      narrowed branch fails (the type is still `number | null |
//      undefined`).
//   3. Conversely, the `// @ts-expect-error` on un-narrowed access does
//      NOT actually trigger an error on the wide shape (the field is
//      optional, accessing it yields `number | null | undefined`, which
//      is assignable nowhere as `number` — but the directive expects an
//      error on the read itself, not the assignment). The unused-
//      directive failure is what trips tsc on RED.

import test from "node:test";
import assert from "node:assert/strict";

import {
  Engine,
  type EmbedderEvent,
  type DefaultEmbedderDownloadEvent,
  type DefaultEmbedderCacheHitEvent,
  type MeanVecPinnedEvent,
} from "../src/index.js";
import { freshDbPath } from "./helpers.js";

// -----------------------------------------------------------------------------
// Compile-time narrowing assertions. These functions are never called;
// they exist purely so tsc type-checks them.
// -----------------------------------------------------------------------------

// Wrapped in an uncalled function so the body is purely a compile-time
// check; tsc still validates the `@ts-expect-error` directives but
// `node --test` does not hit a ReferenceError on module load.
function _unnarrowedAccess(_event: EmbedderEvent): void {
  // Negative: without narrowing, accessing a variant-specific payload
  // field must be a type error. On the discriminated union, `bytes`
  // only exists on `DefaultEmbedderDownloadEvent` — reading it on the
  // bare union is a property-does-not-exist error.
  // @ts-expect-error — bytes is not on every union member
  const _unnarrowedBytes: number = _event.bytes;
  void _unnarrowedBytes;

  // @ts-expect-error — docCount is not on every union member
  const _unnarrowedDocCount: number = _event.docCount;
  void _unnarrowedDocCount;

  // @ts-expect-error — cachePath is not on every union member
  const _unnarrowedCachePath: string = _event.cachePath;
  void _unnarrowedCachePath;
}
void _unnarrowedAccess;

function _exhaustiveNarrowing(event: EmbedderEvent): void {
  if (event.kind === "DefaultEmbedderDownload") {
    const file: string = event.file;
    const url: string = event.url;
    const bytes: number = event.bytes;
    const sha256: string = event.sha256;
    const cachePath: string = event.cachePath;
    const durationMs: number = event.durationMs;
    void file;
    void url;
    void bytes;
    void sha256;
    void cachePath;
    void durationMs;
  } else if (event.kind === "DefaultEmbedderCacheHit") {
    const file: string = event.file;
    const sha256: string = event.sha256;
    const cachePath: string = event.cachePath;
    void file;
    void sha256;
    void cachePath;
  } else if (event.kind === "MeanVecPinned") {
    const dim: number = event.dim;
    const docCount: number = event.docCount;
    void dim;
    void docCount;
  }
}
void _exhaustiveNarrowing;

// The variant-specific interfaces are part of the public surface — they
// must be importable as named exports. Wrapped in an uncalled function
// so the references are compile-time only (the `declare` produces no
// runtime binding).
function _variantInterfacesImportable(
  _download: DefaultEmbedderDownloadEvent,
  _cacheHit: DefaultEmbedderCacheHitEvent,
  _meanVec: MeanVecPinnedEvent,
): void {
  void _download;
  void _cacheHit;
  void _meanVec;
}
void _variantInterfacesImportable;

// -----------------------------------------------------------------------------
// Runtime shape consistency (AC-FIX2-6).
// -----------------------------------------------------------------------------

const VARIANT_KEYS: Record<string, ReadonlyArray<string>> = {
  DefaultEmbedderDownload: [
    "kind",
    "file",
    "url",
    "bytes",
    "sha256",
    "cachePath",
    "durationMs",
  ],
  DefaultEmbedderCacheHit: ["kind", "file", "sha256", "cachePath"],
  MeanVecPinned: ["kind", "dim", "docCount"],
};

const VARIANT_TYPES: Record<string, Record<string, "string" | "number">> = {
  DefaultEmbedderDownload: {
    kind: "string",
    file: "string",
    url: "string",
    bytes: "number",
    sha256: "string",
    cachePath: "string",
    durationMs: "number",
  },
  DefaultEmbedderCacheHit: {
    kind: "string",
    file: "string",
    sha256: "string",
    cachePath: "string",
  },
  MeanVecPinned: {
    kind: "string",
    dim: "number",
    docCount: "number",
  },
};

function skipIfNoNetwork(): boolean {
  if (process.env.FATHOMDB_SKIP_NETWORK_TESTS) {
    console.log("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; skipping test");
    return true;
  }
  return false;
}

test("runtime embedder events match the typed-union shape", async () => {
  if (skipIfNoNetwork()) return;

  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: true });
  let events: ReadonlyArray<EmbedderEvent>;
  try {
    const report = engine.openReport();
    events = report.embedderEvents;
  } finally {
    await engine.close();
  }

  assert.ok(
    events.length > 0,
    "expected at least one embedder event on a fresh default-embedder open",
  );

  for (const event of events) {
    const kind = event.kind;
    const expectedKeys = VARIANT_KEYS[kind];
    assert.ok(
      expectedKeys !== undefined,
      `unknown event kind: ${kind}; expected one of ${Object.keys(VARIANT_KEYS).join(", ")}`,
    );

    const actualKeys = new Set(Object.keys(event as unknown as Record<string, unknown>));
    const expectedSet = new Set(expectedKeys);
    const missing = [...expectedSet].filter((k) => !actualKeys.has(k));
    const extra = [...actualKeys].filter((k) => !expectedSet.has(k));
    assert.deepEqual(missing, [], `${kind}: missing keys ${missing.join(",")}`);
    assert.deepEqual(extra, [], `${kind}: unexpected extra keys ${extra.join(",")}`);

    const typeSpec = VARIANT_TYPES[kind];
    for (const [field, expectedType] of Object.entries(typeSpec)) {
      const value = (event as unknown as Record<string, unknown>)[field];
      assert.equal(
        typeof value,
        expectedType,
        `${kind}.${field}: expected ${expectedType}, got ${typeof value} (${String(value)})`,
      );
    }
  }
});
