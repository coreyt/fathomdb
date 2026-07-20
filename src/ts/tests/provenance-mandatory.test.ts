// 0.8.20 Slice 5c (R-20-E3) — provenance is mandatory on every canonical write.
//
// The TypeScript arm of "an un-provenanced public write does not compile /
// raises", mirroring `src/python/tests/test_provenance_mandatory.py` exactly.
//
// Rust makes the absence of provenance INEXPRESSIBLE: `PreparedWrite` carries a
// `SourceId` newtype, so `source_id: None` is a compile error (see
// `src/rust/crates/fathomdb/tests/ui/unprovenanced_public_write.rs`). TypeScript
// has no equivalent guarantee at the N-API boundary — `write` takes
// `unknown[]` — so the binding throws `WriteValidationError` instead, which is
// the closest available enforcement.
//
// Why this matters at all: `excise_source` addresses rows BY `sourceId`. A row
// written without one is reachable by no erasure call, i.e. permanently
// un-erasable. That is the defect R-20-E3 closes, not a style rule.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import { WriteValidationError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

test("node write without sourceId rejects and commits no row", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const before = engine.counters().writeRows;

    await assert.rejects(
      () => engine.write([{ kind: "doc", body: "un-provenanced body" }]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError);
        return true;
      },
    );

    assert.equal(
      engine.counters().writeRows,
      before,
      "an un-provenanced write must commit no row",
    );
  } finally {
    await engine.close();
  }
});

test("edge write without sourceId rejects", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await assert.rejects(
      () => engine.write([{ edge: { kind: "rel", from: "a", to: "b" } }]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError);
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});

// The engine's reserved namespace. A caller able to mint `_legacy:pre-0.8.20`
// could hide rows among the ones schema migration step 21 back-filled;
// `_engine:` rows read as engine substrate.
const REJECTED_SOURCE_IDS = ["", "   ", "_engine:coverage", "_legacy:pre-0.8.20"];

for (const sourceId of REJECTED_SOURCE_IDS) {
  test(`empty or reserved sourceId ${JSON.stringify(sourceId)} rejects`, async () => {
    const engine = await Engine.open(freshDbPath());
    try {
      await assert.rejects(
        () => engine.write([{ kind: "doc", body: "x", sourceId }]),
        (err: unknown) => {
          assert.ok(err instanceof WriteValidationError);
          return true;
        },
      );
    } finally {
      await engine.close();
    }
  });
}

test("provenanced write succeeds (positive control)", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const receipt = await engine.write([
      { kind: "doc", body: "provenanced body", sourceId: "doc-1" },
    ]);
    assert.ok(receipt.cursor >= 1);
  } finally {
    await engine.close();
  }
});
