// Minimal end-to-end scaffold for the TypeScript SDK. Mirrors
// `src/python/tests/test_scaffold.py` from Phase 11a so a green TS run
// proves the napi-rs binding + Promise wrapper can drive the engine
// through a single op-store write.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

test("cursor advances on write", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const receipt = await engine.write([{ kind: "doc", body: "{}" }]);
    assert.equal(receipt.cursor, 1);
  } finally {
    await engine.close();
  }
});
