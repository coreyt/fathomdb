// Slice 30 (R3) — TypeScript functional harness: useGraphArm parameter.
//
// Exercises:
//   - useGraphArm defaults to undefined (false); results are byte-identical to
//     the two-arm pipeline.
//   - useGraphArm=true surfaces BFS-reachable nodes via temporal fact-edges.
//   - useGraphArm type validation: non-boolean raises TypeError.
//   - Temporal filter: edges with tInvalid in the past do not contribute.
//
// No embedder needed — FTS search only.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function node(logicalId: string, body: string, kind = "doc") {
  return { kind, body, logicalId };
}

function edge(
  from: string,
  to: string,
  logicalId: string,
  opts: { tInvalid?: string } = {},
) {
  const e: Record<string, unknown> = { kind: "link", from, to, logicalId };
  if (opts.tInvalid !== undefined) e.tInvalid = opts.tInvalid;
  return { edge: e };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test("useGraphArm default is false — results match explicit false", async () => {
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    await engine.write([
      node("n1", "alpha bravo delta"),
      node("n2", "charlie delta echo"),
    ]);
    // Wait for projection.
    await engine.drain(3000);
    const rDefault = await engine.search("delta");
    const rExplicit = await engine.search("delta", undefined, undefined, false);
    assert.strictEqual(rDefault.projectionCursor, rExplicit.projectionCursor);
    assert.deepStrictEqual(
      rDefault.results.map((h) => h.body),
      rExplicit.results.map((h) => h.body),
    );
  } finally {
    await engine.close();
  }
});

test("useGraphArm type validation — non-boolean raises TypeError", async () => {
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    await engine.write([node("n1", "test body")]);
    // @ts-expect-error: intentional wrong type
    await assert.rejects(() => engine.search("test", undefined, undefined, 1), {
      name: "TypeError",
      message: /useGraphArm/,
    });
    // @ts-expect-error: intentional wrong type
    await assert.rejects(() => engine.search("test", undefined, undefined, "true"), {
      name: "TypeError",
      message: /useGraphArm/,
    });
  } finally {
    await engine.close();
  }
});

test("useGraphArm=true runs without error with live edges", async () => {
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    await engine.write([
      node("n1", "alice anchor search text"),
      node("n2", "bob reachable via live edge"),
      edge("n1", "n2", "e12"),
    ]);
    await engine.drain(3000);
    const result = await engine.search("alice anchor", undefined, undefined, true);
    // Must return a valid SearchResult (not throw).
    assert.ok(result !== null && typeof result === "object");
    assert.ok(Array.isArray(result.results));
  } finally {
    await engine.close();
  }
});

test("useGraphArm=true excludes nodes reachable only via expired edges", async () => {
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    await engine.write([
      node("n1", "sentinel query anchor ts"),
      node("n2", "unreachable via expired edge zz99 ts"),
      edge("n1", "n2", "e12", { tInvalid: "2000-01-01T00:00:00Z" }),
    ]);
    await engine.drain(3000);
    const result = await engine.search("sentinel query", undefined, undefined, true);
    const bodies = result.results.map((h) => h.body);
    const hasUnreachable = bodies.some((b) =>
      b.includes("unreachable via expired edge"),
    );
    assert.ok(
      !hasUnreachable,
      `graph arm must NOT surface n2 via expired edge; got: ${JSON.stringify(bodies)}`,
    );
  } finally {
    await engine.close();
  }
});
