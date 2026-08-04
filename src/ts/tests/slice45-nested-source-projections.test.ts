// 0.8.21 Slice 45 — nested projections through the public TypeScript binding.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

test("nested source type-collapsed equality and projected search use a real database", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.configureProjections([
      {
        name: "value",
        roles: ["filterable", "searchable"],
        fts: true,
        vector: false,
        source: ["attributes", "core:value", "value"],
      },
    ]);
    await engine.write([
      {
        kind: "doc",
        logicalId: "text",
        sourceId: "ts-slice45:text",
        body: JSON.stringify({ attributes: { "core:value": { value: "1" } } }),
      },
      {
        kind: "doc",
        logicalId: "number",
        sourceId: "ts-slice45:number",
        body: JSON.stringify({ attributes: { "core:value": { value: 1 } } }),
      },
      {
        kind: "doc",
        logicalId: "different",
        sourceId: "ts-slice45:different",
        body: JSON.stringify({ note: "1", attributes: { "core:value": { value: "2" } } }),
      },
    ]);
    const result = await engine.searchProjectedText("1", "value", {
      attributes: [["value", "1"]],
    });
    assert.deepEqual(new Set(result.results.map((hit) => hit.id.value)), new Set(["text", "number"]));
    assert.ok((await engine.search("1")).results.some((hit) => hit.id.value === "different"));
    const hybrid = await engine.search("1", { attributes: [["value", "1"]] });
    assert.ok(hybrid.results.every((hit) => hit.id.value !== "different"));
  } finally {
    await engine.close();
  }
});
