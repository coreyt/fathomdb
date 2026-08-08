// Slice 23 — direct text-only result-prefix stability through TypeScript.

import assert from "node:assert/strict";
import test from "node:test";

import { Engine, type SearchResult } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

const QUERY = "s23prefix";

function writes(): object[] {
  const nodes = Array.from({ length: 50 }, (_, rank) => ({
    kind: "doc",
    body: `${QUERY} matching document ${rank.toString().padStart(2, "0")}`,
    logicalId: `N${rank.toString().padStart(2, "0")}`,
    sourceId: "ts-test:slice23",
  }));
  return [
    ...nodes,
    {
      edge: {
        kind: "link",
        from: "N00",
        to: "N01",
        logicalId: "E10",
        sourceId: "ts-test:slice23",
        // Fusion deduplicates matching node and edge candidates by body.
        body: `${QUERY} matching document 10`,
      },
    },
  ];
}

function assertOrderedPrefix(small: SearchResult, large: SearchResult): void {
  assert.equal(small.results.length, 10);
  assert.equal(large.results.length, 50);
  const actual = small.results.map((hit) => [hit.id, hit.score]);
  const expected = large.results.slice(0, small.results.length).map((hit) => [hit.id, hit.score]);
  assert.deepStrictEqual(actual, expected);
}

test("direct text limits are prefix-stable through TypeScript", async () => {
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    await engine.write(writes());
    assertOrderedPrefix(
      await engine.searchTextOnly(QUERY, { limit: 10 }),
      await engine.searchTextOnly(QUERY, { limit: 50 }),
    );
  } finally {
    await engine.close();
  }
});

test("direct text view limits are prefix-stable through TypeScript", async () => {
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    await engine.write(writes());
    assertOrderedPrefix(
      await engine.searchTextOnly(QUERY, { validAsOf: 1_700_000_000, limit: 10 }),
      await engine.searchTextOnly(QUERY, { validAsOf: 1_700_000_000, limit: 50 }),
    );
  } finally {
    await engine.close();
  }
});
