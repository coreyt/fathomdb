// 0.8.12 Slice 15 (OPP-2) — TypeScript parity test: consolidation surface.
//
// Pins that:
//   1. `ConsolidatorError` is exported and is a `FathomDbError` subclass.
//   2. `Engine.prototype.consolidateWithProvider` exists and is callable.
//   3. `ConsolidateReceipt` and `ConsolidateAxis` types are correct.
//
// Surface-only assertions (not functional end-to-end tests). Functional
// conformance is in the Rust test suite (`tests/consolidate_provider.rs`);
// the live cross-binding functional run is deferred to Slice 40.

import test from "node:test";
import assert from "node:assert/strict";

import {
  Engine,
  type ConsolidateAxis,
  type ConsolidateReceipt,
} from "../src/index.js";
import { ConsolidatorError, FathomDbError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

test("ConsolidatorError is a FathomDbError subclass", () => {
  const err = new ConsolidatorError("consolidator error");
  assert.ok(err instanceof FathomDbError, "ConsolidatorError must be a FathomDbError");
  assert.ok(err instanceof Error, "ConsolidatorError must be an Error");
  assert.equal(err.message, "consolidator error");
});

test("Engine.consolidateWithProvider is a callable method", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    assert.equal(
      typeof engine.consolidateWithProvider,
      "function",
      "Engine.consolidateWithProvider must be a function",
    );
  } finally {
    await engine.close();
  }
});

test("ConsolidateReceipt shape is correct (structural type check)", () => {
  const mockReceipt: ConsolidateReceipt = {
    clustersProcessed: 0,
    edgesExamined: 0,
    edgesKept: 0,
    edgesInvalidated: 0,
    edgesSuperseded: 0,
  };
  assert.equal(typeof mockReceipt.clustersProcessed, "number");
  assert.equal(typeof mockReceipt.edgesExamined, "number");
  assert.equal(typeof mockReceipt.edgesKept, "number");
  assert.equal(typeof mockReceipt.edgesInvalidated, "number");
  assert.equal(typeof mockReceipt.edgesSuperseded, "number");
});

test("ConsolidateAxis shape is correct (structural type check)", () => {
  const axis: ConsolidateAxis = {
    subjectLogicalId: "bob",
    relation: "works_for",
  };
  assert.equal(typeof axis.subjectLogicalId, "string");
  assert.equal(typeof axis.relation, "string");
});
