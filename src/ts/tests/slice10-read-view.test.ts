// X1 SDK parity — 0.8.20 Slice 10b (R-20-RV read view + R-20-NV node validity).
//
// Opens a REAL engine (tmpdir SQLite, no mocking) and drives the Slice-10b read
// surface THROUGH the napi-rs binding. Symbol-presence assertions are
// deliberately absent: R-20-X1 requires a live functional harness, so every
// test below calls across the FFI and asserts on real returned rows.
//
// Covered, mirroring the engine matrices in
// `src/rust/crates/fathomdb-engine/tests/slice10_read_view.rs` and
// `slice10_node_validity.rs`:
//
//   * R-20-RV — the DEFAULT view is unchanged on all five read verbs
//     (`read.get`, `read.getMany`, `read.list`, `read.list` filter-form,
//     `graph.neighbors`), asserted against a raw-table oracle rather than
//     against a second engine call.
//   * R-20-RV — `includeSuperseded` returns history (the requirement's named
//     acceptance signal) and the point lookup stays deterministic under it.
//   * R-20-RV — `includeInactive` relaxes `state = 'active'`.
//   * R-20-RV — the flags COMPOSE: the four existence views yield the four
//     distinct row sets a truth table predicts.
//   * R-20-RV — `graph.neighbors` honours the view in all THREE directions.
//   * R-20-NV — `validAsOf` selects a world-time instant: a bounded node is
//     visible inside its window and invisible outside it, on every read verb;
//     `includeOutOfWindow` relaxes the conjunct entirely.
//   * R-20-NV — `read.crossedBoundarySince` returns real `BoundaryCrossing`
//     rows naming WHICH boundary was crossed.
//
// Validity windows have NO write-side authoring verb in 0.8.20 (a deliberate,
// escalated gap), so the fixtures below set `valid_from`/`valid_until` with
// direct SQL (`node:sqlite`) on the CLOSED database — exactly as the engine
// suite does with rusqlite. The READ path is what is under test here, and it is
// exercised only through the SDK.
//
// Cross-binding equivalence anchor: `src/python/tests/test_slice10_read_view.py`
// asserts the SAME behaviour for the same inputs (Py ≡ TS, R-X-1).

import test from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";

import { Engine, graph, read } from "../src/index.js";
import type { NodeRecord, ReadView } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

// 0.8.20 (R-20-E3): `sourceId` is mandatory on every canonical write.
const SOURCE_ID = "ts-test:slice10-read-view";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function nodeItem(logicalId: string, body: string, kind = "doc"): object {
  return { kind, body, logicalId, sourceId: SOURCE_ID };
}

function edgeItem(from: string, to: string, logicalId: string): object {
  return { edge: { kind: "link", from, to, logicalId, sourceId: SOURCE_ID } };
}

async function openEngine(path: string): Promise<Engine> {
  return Engine.open(path, { useDefaultEmbedder: false });
}

function ids(rows: NodeRecord[]): string[] {
  return rows.map((r) => r.logicalId).sort();
}

function bodies(rows: NodeRecord[]): string[] {
  return rows.map((r) => r.body).sort();
}

/**
 * Author a validity window directly on the CLOSED database.
 *
 * The engine ships no write verb for this (deliberately out of scope for the
 * slice). Writing at rest also keeps the assertion honest: what the SDK reads
 * back is what is on disk, not what some engine call claimed to store.
 */
function setWindow(
  path: string,
  logicalId: string,
  validFrom: number | null,
  validUntil: number | null,
): void {
  const db = new DatabaseSync(path);
  try {
    db.prepare(
      "UPDATE canonical_nodes SET valid_from = ?, valid_until = ?" +
        " WHERE logical_id = ? AND superseded_at IS NULL",
    ).run(validFrom, validUntil, logicalId);
  } finally {
    db.close();
  }
}

/** Read a window back from the raw table — the data-at-rest oracle. */
function rawWindow(path: string, logicalId: string): [number | null, number | null] {
  const db = new DatabaseSync(path);
  try {
    const row = db
      .prepare(
        "SELECT valid_from AS f, valid_until AS u FROM canonical_nodes" +
          " WHERE logical_id = ? AND superseded_at IS NULL",
      )
      .get(logicalId) as { f: number | null; u: number | null } | undefined;
    assert.ok(row !== undefined, `no current row on disk for ${logicalId}`);
    return [row.f, row.u];
  } finally {
    db.close();
  }
}

/**
 * Bodies that are CURRENT (`superseded_at IS NULL`) AND `state = 'active'` —
 * the oracle the default view must reproduce, taken from data at rest.
 */
function rawCurrentActiveBodies(path: string): string[] {
  const db = new DatabaseSync(path);
  try {
    const rows = db
      .prepare(
        "SELECT body FROM canonical_nodes WHERE superseded_at IS NULL AND state = 'active'",
      )
      .all() as { body: string }[];
    return rows.map((r) => r.body).sort();
  } finally {
    db.close();
  }
}

function rawNodeCount(path: string): number {
  const db = new DatabaseSync(path);
  try {
    const row = db.prepare("SELECT COUNT(*) AS n FROM canonical_nodes").get() as { n: number };
    return Number(row.n);
  } finally {
    db.close();
  }
}

// ---------------------------------------------------------------------------
// (1) R-20-RV — the default view is unchanged on all five read verbs
// ---------------------------------------------------------------------------

test("R-20-RV: the default view is unchanged on all five read verbs", async () => {
  const path = freshDbPath();
  const engine = await openEngine(path);

  let gotGetA: NodeRecord | null;
  let gotGetB: NodeRecord | null;
  let gotMany: (NodeRecord | null)[];
  let gotList: string[];
  let gotListFilter: string[];
  let gotNeighbors: string[];
  try {
    await engine.write([nodeItem("A", "alpha v1"), nodeItem("B", "beta")]);
    // Supersede A, so a historical row exists on disk.
    await engine.write([nodeItem("A", "alpha v2")]);
    // Take B out of the active state.
    await engine.transition("B", "deleted", "test");
    await engine.write([nodeItem("C", "gamma"), edgeItem("A", "C", "E-AC")]);
    await engine.drain(30_000);

    gotGetA = await read.get(engine, "A");
    gotGetB = await read.get(engine, "B");
    gotMany = await read.getMany(engine, ["A", "B", "C"]);
    gotList = bodies(await read.list(engine, "doc"));
    gotListFilter = bodies(await read.list(engine, "doc", undefined, 100, { terms: [] }));
    gotNeighbors = bodies(await graph.neighbors(engine, "A", 1, "outgoing"));

    // Passing `view` EXPLICITLY as `undefined` or as an all-default object must
    // be identical to omitting it — `index.ts` declares `view?: ReadView` with
    // every member optional, so all three call shapes are on the governed
    // surface and all three must hit the strict default.
    assert.deepStrictEqual(
      bodies(await read.list(engine, "doc", undefined, 100, undefined, undefined)),
      gotList,
      "an explicit `undefined` view must equal the omitted view",
    );
    assert.deepStrictEqual(
      bodies(await read.list(engine, "doc", undefined, 100, undefined, {})),
      gotList,
      "an empty ReadView object must equal the omitted view",
    );
  } finally {
    await engine.close();
  }

  // Data-at-rest oracle, read after the engine released its lock.
  const expected = rawCurrentActiveBodies(path);
  assert.deepStrictEqual(
    expected,
    ["alpha v2", "gamma"],
    "fixture precondition: on disk exactly `alpha v2` and `gamma` are current+active " +
      "(`alpha v1` superseded, `beta` deleted)",
  );

  assert.equal(gotGetA?.body, "alpha v2", "default read.get must return the CURRENT version");
  assert.equal(gotGetB, null, "default read.get must not return a deleted node");
  assert.deepStrictEqual(
    gotMany.map((r) => (r === null ? null : r.body)),
    ["alpha v2", null, "gamma"],
    "default read.getMany must be current+active only, in REQUEST order",
  );
  assert.deepStrictEqual(gotList, expected, "default read.list must match the raw oracle");
  assert.deepStrictEqual(
    gotListFilter,
    expected,
    "default read.list filter-form must match the raw oracle too",
  );
  assert.deepStrictEqual(
    gotNeighbors,
    ["gamma"],
    "default graph.neighbors must traverse to the current+active neighbor only",
  );
});

// ---------------------------------------------------------------------------
// (2) R-20-RV — includeSuperseded returns history
// ---------------------------------------------------------------------------

test("R-20-RV: includeSuperseded returns history", async () => {
  const engine = await openEngine(freshDbPath());
  try {
    await engine.write([nodeItem("A", "v1")]);
    await engine.write([nodeItem("A", "v2")]);
    await engine.write([nodeItem("A", "v3")]);
    await engine.drain(30_000);

    assert.deepStrictEqual(
      bodies(await read.list(engine, "doc")),
      ["v3"],
      "the strict view sees only the current version",
    );
    const history = bodies(
      await read.list(engine, "doc", undefined, 100, undefined, { includeSuperseded: true }),
    );
    assert.deepStrictEqual(
      history,
      ["v1", "v2", "v3"],
      "includeSuperseded must return the FULL history, not one extra row",
    );
  } finally {
    await engine.close();
  }
});

test("R-20-RV: the point lookup under includeSuperseded is deterministic", async () => {
  const engine = await openEngine(freshDbPath());
  try {
    await engine.write([nodeItem("A", "v1")]);
    await engine.write([nodeItem("A", "v2")]);
    await engine.write([nodeItem("A", "v3")]);
    await engine.drain(30_000);

    const view: ReadView = { includeSuperseded: true };
    for (let i = 0; i < 5; i += 1) {
      const row = await read.get(engine, "A", view);
      assert.equal(
        row?.body,
        "v3",
        "read.get under includeSuperseded must always resolve to the newest version",
      );
    }
    const many = await read.getMany(engine, ["A"], view);
    assert.equal(many[0]?.body, "v3");
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (3) R-20-RV — the state/active relax flag
// ---------------------------------------------------------------------------

test("R-20-RV: includeInactive returns non-active states", async () => {
  const engine = await openEngine(freshDbPath());
  try {
    await engine.write([nodeItem("A", "kept"), nodeItem("B", "dropped")]);
    await engine.transition("B", "deleted", "test");
    await engine.drain(30_000);

    assert.deepStrictEqual(bodies(await read.list(engine, "doc")), ["kept"]);
    const relaxed = bodies(
      await read.list(engine, "doc", undefined, 100, undefined, { includeInactive: true }),
    );
    assert.deepStrictEqual(
      relaxed,
      ["dropped", "kept"],
      "includeInactive must surface the deleted node",
    );
    const row = await read.get(engine, "B", { includeInactive: true });
    assert.equal(
      row?.body,
      "dropped",
      "includeInactive must apply on the point-lookup verb too",
    );
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (4) R-20-RV — the flags COMPOSE
// ---------------------------------------------------------------------------

test("R-20-RV: the existence flags compose independently (read-mode matrix)", async () => {
  const path = freshDbPath();
  const engine = await openEngine(path);
  let widestLen: number;
  try {
    // A: current + active.
    await engine.write([nodeItem("A", "current-active")]);
    // B: superseded + active (v1 superseded by v2; both rows stay active).
    await engine.write([nodeItem("B", "superseded-active")]);
    await engine.write([nodeItem("B", "current-active-b")]);
    // C: current + inactive.
    await engine.write([nodeItem("C", "current-inactive")]);
    await engine.transition("C", "deleted", "test");
    await engine.drain(30_000);

    const matrix: [ReadView, string[]][] = [
      [{}, ["current-active", "current-active-b"]],
      [
        { includeSuperseded: true },
        ["current-active", "current-active-b", "superseded-active"],
      ],
      [
        { includeInactive: true },
        ["current-active", "current-active-b", "current-inactive"],
      ],
      [
        { includeSuperseded: true, includeInactive: true },
        ["current-active", "current-active-b", "current-inactive", "superseded-active"],
      ],
    ];
    for (const [view, expected] of matrix) {
      const got = bodies(await read.list(engine, "doc", undefined, 100, undefined, view));
      assert.deepStrictEqual(
        got,
        [...expected].sort(),
        `read-mode matrix cell ${JSON.stringify(view)} must yield the predicted row set`,
      );
    }

    // The fully-relaxed view is a FILTER, never a source: it must return
    // exactly the rows that exist in `canonical_nodes` and no more.
    const widest = await read.list(engine, "doc", undefined, 1000, undefined, {
      includeSuperseded: true,
      includeInactive: true,
      includeOutOfWindow: true,
    });
    widestLen = widest.length;
  } finally {
    await engine.close();
  }

  assert.equal(
    widestLen,
    rawNodeCount(path),
    "the fully-relaxed view must return exactly the rows on disk — no more",
  );
});

test("R-20-RV: the read.list filter-form inherits the view", async () => {
  const engine = await openEngine(freshDbPath());
  try {
    await engine.write([nodeItem("A", '{"n":1}')]);
    await engine.write([nodeItem("A", '{"n":2}')]);
    await engine.drain(30_000);

    assert.equal((await read.list(engine, "doc", undefined, 100, { terms: [] })).length, 1);
    const relaxed = await read.list(engine, "doc", undefined, 100, { terms: [] }, {
      includeSuperseded: true,
    });
    assert.equal(
      relaxed.length,
      2,
      "the filter-form must pass the view down, not drop it on the filter-lowering path",
    );
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (5) R-20-RV — graph.neighbors honours the view in every direction
// ---------------------------------------------------------------------------

test("R-20-RV: graph.neighbors honours the view in every direction", async () => {
  const engine = await openEngine(freshDbPath());
  try {
    await engine.write([
      nodeItem("R", "root"),
      nodeItem("OA", "out-active"),
      nodeItem("OI", "out-inactive"),
      nodeItem("IA", "in-active"),
      nodeItem("II", "in-inactive"),
      edgeItem("R", "OA", "E-R-OA"),
      edgeItem("R", "OI", "E-R-OI"),
      edgeItem("IA", "R", "E-IA-R"),
      edgeItem("II", "R", "E-II-R"),
    ]);
    await engine.transition("OI", "deleted", "t");
    await engine.transition("II", "deleted", "t");
    await engine.drain(30_000);

    // There are exactly THREE TraversalDirection variants. "Works on outgoing
    // but silently not on incoming" is the exact failure mode the uniformity
    // requirement exists to prevent, so every cell is asserted.
    const cases: ["outgoing" | "incoming" | "both", ReadView, string[]][] = [
      ["outgoing", {}, ["OA"]],
      ["outgoing", { includeInactive: true }, ["OA", "OI"]],
      ["incoming", {}, ["IA"]],
      ["incoming", { includeInactive: true }, ["IA", "II"]],
      ["both", {}, ["IA", "OA"]],
      ["both", { includeInactive: true }, ["IA", "II", "OA", "OI"]],
    ];
    for (const [direction, view, expected] of cases) {
      const got = ids(await graph.neighbors(engine, "R", 1, direction, view));
      assert.deepStrictEqual(
        got,
        [...expected].sort(),
        `graph.neighbors matrix cell (${direction}, ${JSON.stringify(view)})`,
      );
    }
  } finally {
    await engine.close();
  }
});

test("R-20-RV: the graph.neighbors view reaches the BFS recursive join", async () => {
  const engine = await openEngine(freshDbPath());
  try {
    await engine.write([
      nodeItem("R", "root"),
      nodeItem("D", "middle"),
      nodeItem("E", "far"),
      edgeItem("R", "D", "E-RD"),
      edgeItem("D", "E", "E-DE"),
    ]);
    // Only the INTERMEDIATE node is non-active.
    await engine.transition("D", "deleted", "t");
    await engine.drain(30_000);

    assert.deepStrictEqual(
      await graph.neighbors(engine, "R", 3, "outgoing"),
      [],
      "strict view: a non-active intermediate blocks the frontier",
    );
    const relaxed = bodies(
      await graph.neighbors(engine, "R", 3, "outgoing", { includeInactive: true }),
    );
    assert.deepStrictEqual(
      relaxed,
      ["far", "middle"],
      "includeInactive must apply at the RECURSIVE JOIN: reaching `far` is only " +
        "possible if the frontier expanded THROUGH `middle`",
    );
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (6) R-20-NV — validAsOf selects a world-time instant
// ---------------------------------------------------------------------------

test("R-20-NV: validAsOf window visibility on every read verb", async () => {
  const path = freshDbPath();
  {
    const engine = await openEngine(path);
    try {
      await engine.write([
        nodeItem("R", "root"),
        nodeItem("X", "expiring"),
        edgeItem("R", "X", "E-RX"),
      ]);
      await engine.drain(30_000);
    } finally {
      await engine.close();
    }
  }

  setWindow(path, "X", 1000, 2000);
  // The fixture is real, verified at rest before anything is read back.
  assert.deepStrictEqual(rawWindow(path, "X"), [1000, 2000]);
  assert.deepStrictEqual(rawWindow(path, "R"), [null, null]);

  const engine = await openEngine(path);
  try {
    const inside: ReadView = { validAsOf: 1500 };
    const outside: ReadView = { validAsOf: 5000 };

    // Inside the window: X is visible on every verb.
    assert.notEqual(await read.get(engine, "X", inside), null, "read.get inside window");
    assert.notEqual(
      (await read.getMany(engine, ["X"], inside))[0],
      null,
      "read.getMany inside window",
    );
    assert.deepStrictEqual(
      ids(await read.list(engine, "doc", undefined, 100, undefined, inside)),
      ["R", "X"],
    );
    assert.deepStrictEqual(
      ids(await read.list(engine, "doc", undefined, 100, { terms: [] }, inside)),
      ["R", "X"],
    );
    assert.deepStrictEqual(
      ids(await graph.neighbors(engine, "R", 1, "outgoing", inside)),
      ["X"],
      "graph.neighbors inside window",
    );

    // Outside the window: X vanishes from every verb; R (unbounded) never does.
    assert.equal(await read.get(engine, "X", outside), null, "read.get outside window");
    assert.equal(
      (await read.getMany(engine, ["X"], outside))[0],
      null,
      "read.getMany outside window",
    );
    assert.deepStrictEqual(
      ids(await read.list(engine, "doc", undefined, 100, undefined, outside)),
      ["R"],
    );
    assert.deepStrictEqual(
      ids(await read.list(engine, "doc", undefined, 100, { terms: [] }, outside)),
      ["R"],
    );
    assert.deepStrictEqual(
      await graph.neighbors(engine, "R", 1, "outgoing", outside),
      [],
      "graph.neighbors outside window",
    );

    // The window is HALF-OPEN [validFrom, validUntil): lower bound INCLUSIVE,
    // upper bound EXCLUSIVE.
    const at = async (instant: number): Promise<string[]> =>
      ids(await read.list(engine, "doc", undefined, 100, undefined, { validAsOf: instant }));
    assert.deepStrictEqual(await at(999), ["R"]);
    assert.deepStrictEqual(await at(1000), ["R", "X"]);
    assert.deepStrictEqual(await at(1999), ["R", "X"]);
    assert.deepStrictEqual(await at(2000), ["R"]);

    // `includeOutOfWindow` drops the validity conjunct entirely, and COMPOSES
    // with an instant that would otherwise exclude X.
    assert.deepStrictEqual(
      ids(
        await read.list(engine, "doc", undefined, 100, undefined, {
          includeOutOfWindow: true,
          validAsOf: 5000,
        }),
      ),
      ["R", "X"],
      "includeOutOfWindow must ignore validAsOf entirely",
    );
  } finally {
    await engine.close();
  }
});

test("R-20-NV: the validity flag composes with the existence flag", async () => {
  const path = freshDbPath();
  {
    const engine = await openEngine(path);
    try {
      await engine.write([nodeItem("R", "root"), nodeItem("G", "gone-and-expired")]);
      await engine.transition("G", "deleted", "t");
      await engine.drain(30_000);
    } finally {
      await engine.close();
    }
  }

  setWindow(path, "G", 1000, 2000);
  assert.deepStrictEqual(rawWindow(path, "G"), [1000, 2000]);

  const engine = await openEngine(path);
  try {
    const listWith = async (view: ReadView): Promise<string[]> =>
      ids(await read.list(engine, "doc", undefined, 100, undefined, view));

    assert.deepStrictEqual(await listWith({ validAsOf: 5000 }), ["R"]);
    assert.deepStrictEqual(
      await listWith({ includeInactive: true, validAsOf: 5000 }),
      ["R"],
      "includeInactive alone must not resurrect an out-of-window node",
    );
    assert.deepStrictEqual(
      await listWith({ includeOutOfWindow: true, validAsOf: 5000 }),
      ["R"],
      "includeOutOfWindow alone must not resurrect a deleted node",
    );
    // BOTH flags: the node surfaces. This is the composition claim.
    assert.deepStrictEqual(
      await listWith({ includeInactive: true, includeOutOfWindow: true, validAsOf: 5000 }),
      ["G", "R"],
      "the two relax flags must COMPOSE (each drops exactly one conjunct)",
    );
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (7) R-20-NV — read.crossedBoundarySince
// ---------------------------------------------------------------------------

test("R-20-NV: crossedBoundarySince reports both boundaries", async () => {
  const path = freshDbPath();
  {
    const engine = await openEngine(path);
    try {
      await engine.write([
        nodeItem("OPENED", "opened"),
        nodeItem("CLOSED", "closed"),
        nodeItem("BOTH", "both"),
        nodeItem("OUTSIDE", "outside"),
        nodeItem("UNBOUNDED", "unbounded"),
      ]);
      await engine.drain(30_000);
    } finally {
      await engine.close();
    }
  }

  // The interrogated interval is (1000, 2000].
  setWindow(path, "OPENED", 1500, null); // became valid inside
  setWindow(path, "CLOSED", 0, 1500); // became invalid inside
  setWindow(path, "BOTH", 1200, 1800); // both boundaries inside
  setWindow(path, "OUTSIDE", 5000, 6000); // neither inside
  // UNBOUNDED keeps NULL/NULL — it can never cross a boundary.

  const engine = await openEngine(path);
  try {
    // The delta's ONE net-new command, called live and asserted on real
    // BoundaryCrossing data — not on symbol presence.
    const crossings = await read.crossedBoundarySince(engine, 1000, { validAsOf: 2000 });
    const got = crossings
      .map(
        (c) =>
          [c.node.logicalId, c.becameValidAt ?? null, c.becameInvalidAt ?? null] as const,
      )
      .sort((a, b) => a[0].localeCompare(b[0]));

    assert.deepStrictEqual(
      got.map((r) => [...r]),
      [
        ["BOTH", 1200, 1800],
        ["CLOSED", null, 1500],
        ["OPENED", 1500, null],
      ],
      "the hook must report exactly the nodes crossing a boundary in (1000, 2000], each " +
        "carrying the boundary(ies) it crossed",
    );

    // The carried node is a real, fully-populated NodeRecord.
    const byId = new Map(crossings.map((c) => [c.node.logicalId, c]));
    assert.equal(byId.get("BOTH")?.node.body, "both");
    assert.equal(byId.get("BOTH")?.node.kind, "doc");
    assert.ok((byId.get("BOTH")?.node.writeCursor ?? 0) > 0);

    // Cross-binding shape pin: an UNCROSSED boundary is JS `null` with the
    // property PRESENT — never `undefined` and never absent. napi-rs renders
    // `Option::None` that way, which is why `BoundaryCrossing.becameValidAt` is
    // declared `number | null` and not `?: number`. Python's mirror is `None`.
    const opened = byId.get("OPENED");
    assert.ok(opened !== undefined);
    assert.ok("becameInvalidAt" in opened, "the uncrossed boundary field must be PRESENT");
    assert.strictEqual(
      opened.becameInvalidAt,
      null,
      "an uncrossed boundary must be exactly `null` (napi Option::None), not `undefined`",
    );
    assert.strictEqual(opened.becameValidAt, 1500);

    // A row with no window can never cross a boundary, even over the widest
    // interval — so the hook is silent on every pre-step-22 row.
    // Bounds stay inside Number.MAX_SAFE_INTEGER so the i64 FFI round-trip is
    // exact in BOTH bindings; 1e12 is still astronomically outside every
    // window in this fixture (max 6000).
    const widest = await read.crossedBoundarySince(engine, -1_000_000_000_000, {
      validAsOf: 1_000_000_000_000,
    });
    assert.ok(
      !widest.some((c) => c.node.logicalId === "UNBOUNDED"),
      "unbounded rows cannot cross a boundary",
    );
  } finally {
    await engine.close();
  }
});

test("R-20-NV: crossedBoundarySince honours the view's existence flags", async () => {
  const path = freshDbPath();
  {
    const engine = await openEngine(path);
    try {
      await engine.write([nodeItem("LIVE", "live"), nodeItem("GONE", "gone")]);
      await engine.transition("GONE", "deleted", "t");
      await engine.drain(30_000);
    } finally {
      await engine.close();
    }
  }

  setWindow(path, "LIVE", 1500, null);
  setWindow(path, "GONE", 1500, null);

  const engine = await openEngine(path);
  try {
    const defaultView = await read.crossedBoundarySince(engine, 1000, { validAsOf: 2000 });
    assert.deepStrictEqual(
      defaultView.map((c) => c.node.logicalId),
      ["LIVE"],
      "the default view excludes the deleted node",
    );

    const relaxed = await read.crossedBoundarySince(engine, 1000, {
      includeInactive: true,
      validAsOf: 2000,
    });
    assert.deepStrictEqual(
      relaxed.map((c) => c.node.logicalId).sort(),
      ["GONE", "LIVE"],
      "includeInactive must widen the hook's candidate set too",
    );
  } finally {
    await engine.close();
  }
});
