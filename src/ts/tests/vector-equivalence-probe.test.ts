// 0.8.18 Slice 5 (#5 vector-equivalence probe) — TypeScript SDK surface parity (X1).
//
// Mirror of `src/python/tests/test_vector_equivalence_probe.py`. Verifies the TS
// binding surfaces the #5 vector-equivalence probe contract at Py↔TS parity:
//   - `VectorEquivalenceMismatchError` is an exported leaf of `FathomDbError`.
//   - `OpenReport` surfaces `denseDisabled` (+ `denseDisabledReason`); a healthy
//     open reports `denseDisabled === false` (R-VEQ-6).
//   - the `Engine.searchTextOnly` FTS-only path exists and serves.
//   - the degraded-observability accessors exist and report the healthy default.
//
// The DIVERGENCE behaviour is proven at the engine layer in Rust
// (`fathomdb-engine/tests/vector_equivalence_probe.rs`); this suite pins the SDK
// SURFACE parity.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import { FathomDbError, VectorEquivalenceMismatchError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

test("VectorEquivalenceMismatchError is a FathomDbError leaf carrying a reason", () => {
  const err = new VectorEquivalenceMismatchError("boom", "P1 flips=3");
  assert.ok(err instanceof FathomDbError);
  assert.equal(err.reason, "P1 flips=3");
});

test("openReport surfaces denseDisabled (default false) + accessors", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const report = engine.openReport();
    assert.equal(report.denseDisabled, false);
    assert.equal(report.denseDisabledReason, null);
    assert.equal(engine.denseDisabled(), false);
    assert.equal(engine.denseDisabledReason(), null);
    assert.equal(engine.vectorEquivalenceRefusalCount(), 0);
  } finally {
    await engine.close();
  }
});

test("searchTextOnly serves the FTS-only path", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([{ kind: "note", body: "alpha bravo charlie" }]);
    await engine.drain(30_000);
    const result = await engine.searchTextOnly("alpha");
    assert.ok(Array.isArray(result.results));
  } finally {
    await engine.close();
  }
});
