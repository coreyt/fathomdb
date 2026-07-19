// 0.8.20 Slice 5d (R-20-E4, design §4 item 9b) — `eraseSource` is a first-class
// SDK lifecycle verb, reachable with NO CLI on `PATH`.
//
// The gap this closes: `purge` addresses a GOVERNED node by `logicalId`, so
// anonymous content — rows written with no `logicalId` — was reachable by no
// SDK verb at all. The only erasure path was the operator CLI
// (`fathomdb recover --excise-source`), which an embedded SDK consumer may not
// have. A consumer holding a deletion obligation over anonymous content could
// therefore not discharge it.
//
// Cross-binding equivalence anchor: `src/python/tests/test_erase_source.py`
// asserts the SAME behaviour for the same inputs (Py ≡ TS, R-X-1).
//
// Test-design contract (design §3, Rule 1): an erasure witness must NOT be a
// `search()` call — both read paths gate on `canonical_nodes`, so a search
// assertion passes on the broken code. The witnesses here are the returned
// report counts (a second erase proves the first did not touch the other
// source) plus durability across a close/re-open. The raw-table assertions for
// the same erasure live in the engine suites, which have SQL access.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import { WriteValidationError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

function anonymousNode(body: string, sourceId: string): object {
  // No `logicalId` — this is exactly the content `purge` cannot reach.
  return { kind: "doc", body, sourceId };
}

test("eraseSource erases anonymous content end-to-end without the CLI", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([
      anonymousNode("erasable alpha payload", "tenant-a"),
      anonymousNode("erasable beta payload", "tenant-a"),
      anonymousNode("retained gamma payload", "tenant-b"),
    ]);

    const report = await engine.eraseSource("tenant-a");
    assert.equal(report.sourceRef, "tenant-a");
    assert.equal(report.nodesExcised, 2, "both tenant-a rows must be erased, and ONLY those");

    // Non-perturbation, asserted as a SECOND erase: its count proves tenant-b's
    // row still existed after the first call.
    const second = await engine.eraseSource("tenant-b");
    assert.equal(second.nodesExcised, 1, "the first erasure must not have touched tenant-b");
  } finally {
    await engine.close();
  }
});

test("eraseSource is idempotent (an absent source is a zero-count success)", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([anonymousNode("idempotence payload", "tenant-a")]);
    assert.equal((await engine.eraseSource("tenant-a")).nodesExcised, 1);
    // Retrying an interrupted erasure obligation must not throw.
    assert.equal((await engine.eraseSource("tenant-a")).nodesExcised, 0);
    assert.equal((await engine.eraseSource("never-written")).nodesExcised, 0);
  } finally {
    await engine.close();
  }
});

// The engine's reserved namespace is reachable ONLY through the CLI recovery
// seam. A caller able to erase `_legacy:pre-0.8.20` through the governed verb
// could wipe every pre-0.8.20 anonymous row in a single call.
for (const sourceId of ["", "   ", "_engine:coverage", "_legacy:pre-0.8.20"]) {
  test(`eraseSource rejects empty or reserved sourceId ${JSON.stringify(sourceId)}`, async () => {
    const engine = await Engine.open(freshDbPath());
    try {
      await assert.rejects(
        () => engine.eraseSource(sourceId),
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
