// X1 SDK parity — OPP-12 record-lifecycle Phase-1 existence axis (0.8.19 Slice 5).
//
// Opens a REAL engine (tmpdir SQLite, no mocking) and exercises the create-time
// existence surface through the N-API/TypeScript binding:
//
//   * R-EX-1 — a write item gains create-time `state` ({"pending","active"}) +
//     advisory `reason`; both round-trip.
//   * R-EX-1 — `state: "deleted"`/`"purged"` (or any out-of-subset value) throws a
//     typed WriteValidationError — you cannot CREATE a deleted/purged node.
//   * R-EX-2 — a `pending` node is absent from default `search` / `read.get` /
//     `read.list`; an `active` node is present. NO-OP on an all-active corpus.
//
// Cross-binding equivalence anchor: src/python/tests/test_opp12_existence_axis.py
// asserts the SAME behavior for the same inputs (Py ≡ TS, R-X-1).

import test from "node:test";
import assert from "node:assert/strict";

import { Engine, read } from "../src/index.js";
import { WriteValidationError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

test("existence axis: state/reason round-trip; pending excluded from default reads", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([
      { kind: "doc", body: "zephyrunique active payload", logicalId: "act1" },
    ]);
    await engine.write([
      {
        kind: "doc",
        body: "zephyrunique pending payload",
        logicalId: "pen1",
        state: "pending",
        reason: "awaiting-review",
      },
    ]);
    // Explicit state: "active" is value-identical to the default.
    await engine.write([
      { kind: "doc", body: "zephyrunique second active", logicalId: "act2", state: "active" },
    ]);

    // R-EX-2 default search excludes the pending node.
    const hits = (await engine.search("zephyrunique")).results;
    const bodies = hits.map((h) => h.body);
    assert.ok(bodies.some((b) => b.includes("active payload")), JSON.stringify(bodies));
    assert.ok(bodies.some((b) => b.includes("second active")), JSON.stringify(bodies));
    assert.ok(!bodies.some((b) => b.includes("pending payload")), JSON.stringify(bodies));

    // R-EX-2 read.get: pending -> null, active -> present.
    assert.notEqual(await read.get(engine, "act1"), null);
    assert.equal(await read.get(engine, "pen1"), null);

    // R-EX-2 read.list: pending excluded from the kind listing.
    const listed = new Set((await read.list(engine, "doc")).map((r) => r.logicalId));
    assert.ok(listed.has("act1") && listed.has("act2"));
    assert.ok(!listed.has("pen1"));
  } finally {
    await engine.close();
  }
});

for (const badState of ["deleted", "purged", "bogus"]) {
  test(`existence axis: state="${badState}" is not creatable (WriteValidationError)`, async () => {
    const engine = await Engine.open(freshDbPath());
    try {
      await assert.rejects(
        () => engine.write([{ kind: "doc", body: "x", logicalId: "n1", state: badState }]),
        (err: unknown) => {
          assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
          return true;
        },
      );
    } finally {
      await engine.close();
    }
  });
}

test("existence axis: state='active' is a no-op on an all-active corpus", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    for (let i = 0; i < 5; i++) {
      await engine.write([
        { kind: "doc", body: `commonterm doc number ${i}`, logicalId: `id${i}` },
      ]);
    }
    const hits = (await engine.search("commonterm")).results.filter((h) =>
      h.body.includes("commonterm"),
    );
    assert.equal(hits.length, 5);
  } finally {
    await engine.close();
  }
});
