// X1 SDK parity — OPP-12 record-lifecycle Phase-1 lifecycle verbs (0.8.19 Slice 10).
//
// Opens a REAL engine (tmpdir SQLite, no mocking) and exercises the
// `transition`/`purge` lifecycle verbs through the N-API/TypeScript binding:
//
//   * R-TR-1 — each legal `transition` move succeeds; each illegal move throws a
//     typed IllegalTransitionError carrying fromState/toState/legal (parity-safe
//     field names — S7).
//   * gap-6 — promote/undelete CLEAR reason (the node re-appears in reads);
//     reject/soft-delete SET it and the node leaves default reads.
//   * §3 — a lifecycle verb on a non-`l:` (`h:`/`p:`) id throws
//     NotLifecycleAddressableError carrying idSpace.
//   * R-PG-1/2 — `purge` requires deleted-first, is idempotent, and erases the
//     node from every read path.
//
// Cross-binding equivalence anchor: src/python/tests/test_opp12_lifecycle_verbs.py
// asserts the SAME behavior for the same inputs (Py ≡ TS, R-X-1).

import test from "node:test";
import assert from "node:assert/strict";

import { Engine, read } from "../src/index.js";
import {
  IllegalTransitionError,
  InvalidArgumentError,
  NotLifecycleAddressableError,
} from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

test("legal transitions + reason clear-on-admit / set-on-exclude", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([
      {
        kind: "doc",
        body: "quarantined",
        logicalId: "p1",
        state: "pending",
        reason: "awaiting-review",
      },
    ]);
    assert.equal(await read.get(engine, "p1"), null);
    await engine.transition("p1", "active");
    assert.notEqual(await read.get(engine, "p1"), null);

    await engine.write([{ kind: "doc", body: "live", logicalId: "a1" }]);
    await engine.transition("a1", "deleted", "user-deleted");
    assert.equal(await read.get(engine, "a1"), null);
    await engine.transition("a1", "active");
    assert.notEqual(await read.get(engine, "a1"), null);
  } finally {
    await engine.close();
  }
});

test("illegal transition is a typed error with fromState/toState/legal", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([{ kind: "doc", body: "x", logicalId: "a1" }]);
    await assert.rejects(
      () => engine.transition("a1", "purged"),
      (err: unknown) => {
        assert.ok(err instanceof IllegalTransitionError);
        assert.equal(err.fromState, "active");
        assert.equal(err.toState, "purged");
        assert.deepEqual(err.legal, ["deleted"]);
        return true;
      },
    );
    // A self-loop is also illegal.
    await assert.rejects(
      () => engine.transition("a1", "active"),
      IllegalTransitionError,
    );
    // An unknown lifecycle string is a boundary argument error.
    await assert.rejects(
      () => engine.transition("a1", "bogus" as never),
      InvalidArgumentError,
    );
  } finally {
    await engine.close();
  }
});

for (const badId of ["h:deadbeef", "p:7"]) {
  test(`non-logical id ${badId} is refused by lifecycle verbs`, async () => {
    const engine = await Engine.open(freshDbPath());
    try {
      await assert.rejects(
        () => engine.transition(badId, "deleted"),
        (err: unknown) => {
          assert.ok(err instanceof NotLifecycleAddressableError);
          assert.ok(["content", "passage"].includes(err.idSpace));
          return true;
        },
      );
      await assert.rejects(
        () => engine.purge(badId),
        NotLifecycleAddressableError,
      );
    } finally {
      await engine.close();
    }
  });
}

test("purge requires deleted-first and is idempotent", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([{ kind: "doc", body: "x", logicalId: "a1" }]);
    await assert.rejects(
      () => engine.purge("a1"),
      (err: unknown) => {
        assert.ok(err instanceof IllegalTransitionError);
        assert.equal(err.fromState, "active");
        assert.equal(err.toState, "purged");
        return true;
      },
    );

    await engine.transition("a1", "deleted");
    await engine.purge("a1");
    assert.equal(await read.get(engine, "a1"), null);
    // Idempotent: purging an absent id is a no-op success.
    await engine.purge("a1");
    await engine.purge("never-existed");
  } finally {
    await engine.close();
  }
});
