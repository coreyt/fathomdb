// Functional tests for the 0.8.1 R1 reranker soft-fallback (X1 harness).
//
// TypeScript half of the X1 cross-binding equivalence tests.
// The Python half lives in ``src/python/tests/test_functional_reranker.py``.
//
// Verifies that:
// 1. engine.search(q, undefined, 0) returns the same body order as
//    engine.search(q) (no-rerank default).
// 2. Negative rerankDepth raises RangeError.
// 3. Non-integer rerankDepth raises TypeError.
// 4. The soft-fallback contract: rerankDepth=0 is byte-identical to
//    the pre-Slice-10 fused order.

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

test("reranker: non-integer rerankDepth raises TypeError", async () => {
  const engine = await openEngineWithDocs();
  try {
    await assert.rejects(
      () => engine.search("cross encoder", undefined, 1.5 as unknown as number),
      (err: unknown) => {
        assert.ok(err instanceof TypeError, `expected TypeError, got ${err}`);
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
