// Slice 20 (G5/G6) — TypeScript functional harness: graph.neighbors + graph.searchExpand.
//
// These tests run against the real compiled native binding (napi-rs / fathomdb-engine).
// They use FTS search exclusively (no embedder needed) so that the graph verbs can be
// exercised synchronously in CI without a model download.
//
// All test databases are isolated per-test via freshDbPath().

import test from "node:test";
import assert from "node:assert/strict";

import { Engine, InvalidArgumentError, WriteValidationError, graph } from "../src/index.js";
import type { NodeRecord } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function openEngine(path: string): Promise<Engine> {
  return Engine.open(path, { useDefaultEmbedder: false });
}

function nodeItem(logicalId: string, body: string, kind = "doc"): object {
  return { kind, body, logicalId };
}

function edgeItem(from: string, to: string, logicalId: string): object {
  return { edge: { kind: "link", from, to, logicalId } };
}

async function seedSmallGraph(engine: Engine): Promise<void> {
  // A→B→C, A→D (D is a direct leaf, C is two hops from A)
  await engine.write([
    nodeItem("A", "Root node alpha unique"),
    nodeItem("B", "Neighbor node B beta"),
    nodeItem("C", "Hop-2 node C gamma"),
    nodeItem("D", "Direct leaf D delta"),
    edgeItem("A", "B", "E-AB"),
    edgeItem("B", "C", "E-BC"),
    edgeItem("A", "D", "E-AD"),
  ]);
}

// ---------------------------------------------------------------------------
// G5 — graph.neighbors
// ---------------------------------------------------------------------------

test("graph.neighbors depth=1 outgoing returns B and D (direct children of A)", async () => {
  const engine = await openEngine(freshDbPath());
  await seedSmallGraph(engine);

  const results = await graph.neighbors(engine, "A", 1, "outgoing");
  const ids = results.map((n) => n.logicalId).sort();
  assert.deepStrictEqual(ids, ["B", "D"], `expected [B, D], got ${JSON.stringify(ids)}`);
  // Root A must NOT appear.
  assert.ok(!results.some((n) => n.logicalId === "A"), "root A must not appear in neighbor set");

  await engine.close();
});

test("graph.neighbors depth=2 outgoing from A returns B, C, D", async () => {
  const engine = await openEngine(freshDbPath());
  await seedSmallGraph(engine);

  const results = await graph.neighbors(engine, "A", 2, "outgoing");
  const ids = results.map((n) => n.logicalId).sort();
  assert.ok(ids.includes("B"), `expected B in results; got ${JSON.stringify(ids)}`);
  assert.ok(ids.includes("C"), `expected C in results; got ${JSON.stringify(ids)}`);
  assert.ok(ids.includes("D"), `expected D in results; got ${JSON.stringify(ids)}`);
  assert.ok(!ids.includes("A"), "root A must not appear");

  await engine.close();
});

test("graph.neighbors returns NodeRecord objects with all required fields", async () => {
  const engine = await openEngine(freshDbPath());
  await seedSmallGraph(engine);

  const results = await graph.neighbors(engine, "A", 1, "outgoing");
  assert.ok(results.length > 0, "expected at least one neighbor");
  for (const node of results as NodeRecord[]) {
    assert.ok(typeof node.logicalId === "string", "logicalId must be string");
    assert.ok(typeof node.kind === "string", "kind must be string");
    assert.ok(typeof node.body === "string", "body must be string");
    assert.ok(typeof node.writeCursor === "number", "writeCursor must be number");
  }

  await engine.close();
});

test("graph.neighbors depth=4 raises InvalidArgumentError", async () => {
  const engine = await openEngine(freshDbPath());
  await engine.write([nodeItem("ROOT", "root body")]);

  await assert.rejects(
    () => graph.neighbors(engine, "ROOT", 4, "outgoing"),
    InvalidArgumentError,
    "depth=4 must raise InvalidArgumentError",
  );

  await engine.close();
});

test("graph.neighbors returns empty array for isolated node", async () => {
  const engine = await openEngine(freshDbPath());
  await engine.write([nodeItem("SOLO", "isolated node")]);

  const results = await graph.neighbors(engine, "SOLO", 1, "outgoing");
  assert.deepStrictEqual(results, [], `expected [], got ${JSON.stringify(results)}`);

  await engine.close();
});

test("graph.neighbors unknown direction raises InvalidArgumentError", async () => {
  const engine = await openEngine(freshDbPath());
  await engine.write([nodeItem("ROOT", "root body")]);

  await assert.rejects(
    () => graph.neighbors(engine, "ROOT", 1, "sideways" as "outgoing"),
    InvalidArgumentError,
    "unrecognised direction must raise InvalidArgumentError",
  );

  await engine.close();
});

// ---------------------------------------------------------------------------
// G6 — graph.searchExpand
// ---------------------------------------------------------------------------

test("graph.searchExpand returns SearchExpandResult with expected fields", async () => {
  const engine = await openEngine(freshDbPath());
  await engine.write([
    nodeItem("HIT", "fzunique expand quark harness node alpha"),
    nodeItem("NBR", "neighbor node beta"),
    edgeItem("HIT", "NBR", "E1"),
  ]);

  const result = await graph.searchExpand(engine, "fzunique expand quark", 1);

  assert.ok(Array.isArray(result.searchHits), "searchHits must be an array");
  assert.ok(Array.isArray(result.expanded), "expanded must be an array");
  assert.ok(Array.isArray(result.allLogicalIds), "allLogicalIds must be an array");

  await engine.close();
});

test("graph.searchExpand: neighbor appears in expanded", async () => {
  const engine = await openEngine(freshDbPath());
  await engine.write([
    nodeItem("HIT2", "fzunique expand quark harness node alpha 2"),
    nodeItem("NBR2", "neighbor node gamma"),
    edgeItem("HIT2", "NBR2", "E2"),
  ]);

  const result = await graph.searchExpand(engine, "fzunique expand quark harness", 1);
  const expandedIds = result.expanded.map((e) => e.node.logicalId);

  assert.ok(
    expandedIds.includes("NBR2"),
    `NBR2 must appear in expanded; expanded=${JSON.stringify(expandedIds)}`,
  );

  await engine.close();
});

test("graph.searchExpand: node in both search hits and traversal appears only in searchHits", async () => {
  const engine = await openEngine(freshDbPath());
  await engine.write([
    nodeItem("DA", "dedup shimmer unique probe node alpha zeta"),
    nodeItem("DB", "dedup shimmer unique probe node beta zeta"),
    edgeItem("DA", "DB", "EAB"),
  ]);

  const result = await graph.searchExpand(engine, "dedup shimmer unique probe", 1);
  const expandedIds = result.expanded.map((e) => e.node.logicalId);

  assert.ok(
    !expandedIds.includes("DB"),
    `DB is a search hit and must not appear in expanded; expanded=${JSON.stringify(expandedIds)}`,
  );

  await engine.close();
});

test("graph.searchExpand: each expanded item has node and hopCount", async () => {
  const engine = await openEngine(freshDbPath());
  await engine.write([
    nodeItem("HIT3", "fzunique expand quark harness node alpha 3"),
    nodeItem("CHD3", "child node delta"),
    edgeItem("HIT3", "CHD3", "E3"),
  ]);

  const result = await graph.searchExpand(engine, "fzunique expand quark harness", 1);

  for (const item of result.expanded) {
    assert.ok(typeof item.node.logicalId === "string", "node.logicalId must be string");
    assert.ok(typeof item.hopCount === "number", "hopCount must be number");
    assert.ok(item.hopCount >= 1, "hopCount must be >= 1");
  }

  await engine.close();
});
