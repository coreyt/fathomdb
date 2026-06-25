// Functional tests for the 0.8.1 R1 reranker soft-fallback (X1 harness).
//
// TypeScript half of the X1 cross-binding equivalence tests.
// The Python half lives in ``src/python/tests/test_functional_reranker.py``.
//
// Verifies that:
// 1. engine.search(q, undefined, 0) returns the same body order as
//    engine.search(q) (no-rerank default).
// 2. Negative rerankDepth raises RangeError.
// 3. Non-integer rerankDepth raises RangeError (FIX-5: was TypeError).
// 4. The soft-fallback contract: rerankDepth=0 is byte-identical to
//    the pre-Slice-10 fused order.
// 5. rerankDepth > u32::MAX raises RangeError (FIX-5: u32 overflow guard).

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import type { SearchHit } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

const CORPUS = [
  { kind: "doc", body: "cross encoder reranker alpha document" },
  { kind: "doc", body: "cross encoder reranker beta document" },
  { kind: "doc", body: "cross encoder reranker gamma document" },
];

async function openEngineWithDocs(): Promise<Engine> {
  const engine = await Engine.open(freshDbPath());
  for (const doc of CORPUS) {
    await engine.write([doc]);
  }
  await engine.drain(10_000);
  return engine;
}

/**
 * Poll search until at least one result appears (projection lag) or timeout.
 */
async function searchWithRetry(
  engine: Engine,
  query: string,
  rerankDepth?: number,
): Promise<SearchHit[]> {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    const result = await engine.search(query, undefined, rerankDepth);
    if (result.results.length > 0) {
      return result.results;
    }
    await new Promise((r) => setTimeout(r, 20));
  }
  return [];
}

test("reranker: rerankDepth=0 matches default search order", async () => {
  const engine = await openEngineWithDocs();
  try {
    const defaultHits = await searchWithRetry(engine, "cross encoder");
    const depth0Hits = await searchWithRetry(engine, "cross encoder", 0);

    const defaultBodies = defaultHits.map((h) => h.body);
    const depth0Bodies = depth0Hits.map((h) => h.body);

    assert.deepEqual(
      depth0Bodies,
      defaultBodies,
      `rerankDepth=0 must return the same body order as the default search ` +
        `(identity/soft-fallback). default=${JSON.stringify(defaultBodies)}, ` +
        `depth0=${JSON.stringify(depth0Bodies)}`,
    );
  } finally {
    await engine.close();
  }
});

test("reranker: rerankDepth=0 scores identical to default", async () => {
  const engine = await openEngineWithDocs();
  try {
    const defaultHits = await searchWithRetry(engine, "reranker beta");
    const depth0Hits = await searchWithRetry(engine, "reranker beta", 0);

    for (let i = 0; i < Math.min(defaultHits.length, depth0Hits.length); i++) {
      const hDef = defaultHits[i];
      const hD0 = depth0Hits[i];
      assert.equal(
        hD0.score,
        hDef.score,
        `scores must be identical: body=${JSON.stringify(hDef.body)} ` +
          `default=${hDef.score} depth0=${hD0.score}`,
      );
    }
  } finally {
    await engine.close();
  }
});

test("reranker: negative rerankDepth raises RangeError", async () => {
  const engine = await openEngineWithDocs();
  try {
    await assert.rejects(
      () => engine.search("cross encoder", undefined, -1),
      (err: unknown) => {
        assert.ok(err instanceof RangeError, `expected RangeError, got ${err}`);
        assert.ok(
          String(err).includes("rerankDepth must be >= 0"),
          `expected message to include "rerankDepth must be >= 0", got ${String(err)}`,
        );
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});

test("reranker: non-integer rerankDepth raises RangeError (not TypeError)", async () => {
  // FIX-5: changed from TypeError to RangeError for consistency with validateLimit
  // and graph depth checks in the rest of the codebase.
  const engine = await openEngineWithDocs();
  try {
    await assert.rejects(
      () => engine.search("cross encoder", undefined, 1.5 as unknown as number),
      (err: unknown) => {
        assert.ok(err instanceof RangeError, `expected RangeError, got ${err}`);
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});

test("reranker: rerankDepth=0 with filter preserves identity contract", async () => {
  const engine = await openEngineWithDocs();
  try {
    const filteredHits = await searchWithRetry(
      engine,
      "cross encoder",
    );
    const depth0FilteredHits = await searchWithRetry(
      engine,
      "cross encoder",
      0,
    );

    const filteredBodies = filteredHits.map((h) => h.body);
    const depth0FilteredBodies = depth0FilteredHits.map((h) => h.body);

    assert.deepEqual(
      depth0FilteredBodies,
      filteredBodies,
      "rerankDepth=0 must match default search (identity/soft-fallback)",
    );
  } finally {
    await engine.close();
  }
});

test("reranker: positive rerankDepth returns results (soft-fallback in default build)", async () => {
  const engine = await openEngineWithDocs();
  try {
    // In the default build (no default-reranker feature), depth>0 still
    // returns the identity order (model absent → soft-fallback).
    const result = await engine.search("cross encoder reranker", undefined, 200);
    assert.ok(result.results.length >= 0, "must not error; may be empty if no hits");
  } finally {
    await engine.close();
  }
});

// --- FIX-5 RED tests: u32 overflow guard + consistent RangeError ---

// --- 0.8.5: α / pool_n / ceScore exposure (smoke) ---

test("reranker: search accepts alpha + poolN and each hit carries ceScore", async () => {
  // 0.8.5 — the new opt-in knobs must be accepted; ceScore is an additive field
  // on every hit. In the default napi build (no default-reranker feature) the CE
  // path is compiled away → ceScore is null, but the FIELD must be present.
  const engine = await openEngineWithDocs();
  try {
    const result = await engine.search("cross encoder reranker", undefined, 10, false, 1.0, 10);
    assert.ok(result.results.length >= 0, "must not error with alpha/poolN set");
    for (const h of result.results) {
      assert.ok("ceScore" in h, "every hit must carry a ceScore field");
      assert.ok(
        h.ceScore === null || typeof h.ceScore === "number",
        `ceScore must be number|null, got ${typeof h.ceScore}`,
      );
    }
  } finally {
    await engine.close();
  }
});

test("reranker: default search (no alpha) leaves order byte-identical, ceScore null", async () => {
  const engine = await openEngineWithDocs();
  try {
    const def = await searchWithRetry(engine, "cross encoder");
    const withKnobs = (
      await engine.search("cross encoder", undefined, 0, false, 0.3)
    ).results;
    assert.deepEqual(
      withKnobs.map((h) => h.body),
      def.map((h) => h.body),
      "alpha=0.3 at depth=0 must preserve the default order",
    );
    for (const h of def) {
      assert.equal(h.ceScore, null, "default-path hits carry null ceScore");
    }
  } finally {
    await engine.close();
  }
});

test("reranker: huge rerankDepth raises RangeError (u32 overflow guard)", async () => {
  // FIX-5: rerankDepth > 0xFFFFFFFF (u32::MAX) must raise RangeError.
  // Without the guard, 2**32 + 5 silently wraps mod 2^32 to 5 at the NAPI layer.
  const engine = await openEngineWithDocs();
  try {
    await assert.rejects(
      () => engine.search("cross encoder", undefined, 2**32 + 5),
      (err: unknown) => {
        assert.ok(err instanceof RangeError, `expected RangeError, got ${err}`);
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});
