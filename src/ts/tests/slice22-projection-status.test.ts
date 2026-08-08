// Slice 22 projection-runtime status parity through the real napi database.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine, read, type ProjectionRuntimeStatus } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

function node(kind: string, logicalId: string): object {
  return {
    kind,
    logicalId,
    body: '{"alpha":"dense meaning","zeta":"plain value"}',
    sourceId: "ts-test:slice22-status",
  };
}

test("read.projectionStatus exposes exact current status wires", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.configureProjections([
      { name: "zeta", roles: ["filterable"], fts: false, vector: false },
      { name: "alpha", roles: ["searchable"], fts: false, vector: true },
    ]);
    await engine.write([
      node("invoice", "I1"),
      node("doc", "D1"),
      node("entity", "E1"),
      node("invoice", "I2"),
    ]);

    const status: ProjectionRuntimeStatus = await read.projectionStatus(engine);
    assert.equal(status.runtimeEmbedderAvailable, false);
    assert.equal(status.runtimeUnavailabilityReason, "no_runtime");
    assert.deepEqual(status.projections, [
      { name: "alpha", denseReadiness: "unavailable" },
      { name: "zeta", denseReadiness: "not_declared" },
    ]);
    assert.deepEqual(status.vectorUnsupportedKinds, ["entity", "invoice"]);
    assert.deepEqual(await read.projectionStatus(engine), status, "repeated reads are pure");
  } finally {
    await engine.close();
  }
});
