// Slice 18 — ranked retrieval limits through the public TypeScript SDK.

import test from "node:test";
import assert from "node:assert/strict";

import {
  Engine,
  InvalidArgumentError,
  graph,
} from "../src/index.js";
import { freshDbPath } from "./helpers.js";

async function writeCorpus(engine: Engine): Promise<void> {
  await engine.configureProjections([
    { name: "title", roles: ["searchable"], fts: true, vector: false },
  ]);
  await engine.write(
    Array.from({ length: 101 }, (_, n) => ({
      kind: "doc",
      logicalId: `N${n}`,
      sourceId: "ts-slice18:fixture",
      body: JSON.stringify({ title: `needle result ${n}` }),
    })),
  );
  await engine.drain(10_000);
}

test("ranked retrieval limits are public and truthful", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await writeCorpus(engine);
    assert.equal((await engine.search("needle")).results.length, 10);
    assert.equal((await engine.searchTextOnly("needle")).results.length, 10);
    assert.equal((await engine.searchProjectedText("needle", "title")).results.length, 10);
    assert.equal((await graph.searchExpand(engine, "needle", 0)).searchHits.length, 10);

    for (const limit of [5, 20, 50, 100]) {
      assert.equal((await engine.search("needle", undefined, undefined, undefined, undefined, undefined, undefined, { limit })).results.length, limit);
      assert.equal((await engine.searchTextOnly("needle", { limit })).results.length, limit);
      assert.equal((await engine.searchProjectedText("needle", "title", undefined, { limit })).results.length, limit);
      assert.equal((await graph.searchExpand(engine, "needle", 0, undefined, { searchLimit: limit })).searchHits.length, limit);
    }
  } finally {
    await engine.close();
  }
});

test("ranked retrieval limit rejections are typed", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    for (const limit of [0, -1, 101]) {
      await assert.rejects(
        () => engine.search("needle", undefined, undefined, undefined, undefined, undefined, undefined, { limit }),
        InvalidArgumentError,
      );
      await assert.rejects(() => engine.searchTextOnly("needle", { limit }), InvalidArgumentError);
      await assert.rejects(
        () => engine.searchProjectedText("needle", "title", undefined, { limit }),
        InvalidArgumentError,
      );
      await assert.rejects(
        () => graph.searchExpand(engine, "needle", 0, undefined, { searchLimit: limit }),
        InvalidArgumentError,
      );
    }
  } finally {
    await engine.close();
  }
});
