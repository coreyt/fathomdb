// X1 SDK parity — 0.8.20 Slice 15b (TC-34: node-validity WRITE-side authoring).
//
// Slice 10b shipped node validity read-only, so the only way to author a window
// was raw SQL on a closed database — which is exactly what
// `slice10-read-view.test.ts` still does. A window a caller can filter on but
// can never set is dead surface. This suite drives the AUTHORING path through
// the napi-rs binding: every window below is set by `engine.write(...)`, and no
// test here sets `valid_from`/`valid_until` with SQL.
//
// `node:sqlite` IS used, but only as a READ oracle on a CLOSED database: the
// "omitted fields land NULL/NULL" assertion has to read the raw table, because a
// read-verb assertion would pass on broken code (a row wrongly written with
// `valid_from = 0` is also visible under a default view).
//
// Covered, mirroring `src/rust/crates/fathomdb-engine/tests/
// slice15b_node_validity_write.rs`:
//
//   * TC-34 — round trip: authored window visible INSIDE, invisible OUTSIDE, on
//     the deterministic `validAsOf` seam (no wall clock, no sleep).
//   * TC-34 — half-open `[validFrom, validUntil)` survives the write path.
//   * TC-34 — unbounded sides (one bound authored, the other omitted).
//   * TC-34 — omitting BOTH fields lands NULL/NULL (RAW TABLE oracle) and leaves
//     default-view visibility unchanged.
//   * TC-34 — camelCase AND snake_case spellings are both accepted, exactly as
//     the edge translator does for `tValid`/`t_valid`.
//   * TC-34 — an unsatisfiable window (`from >= until`) is a typed refusal, and
//     a non-integer value is a typed refusal rather than a silent coercion.
//   * TC-34 — `read.crossedBoundarySince` works end-to-end on an SDK-authored
//     window.
//
// Cross-binding equivalence anchor:
// `src/python/tests/test_slice15b_node_validity_write.py` asserts the SAME
// behaviour for the same inputs (Py ≡ TS, R-X-1).

import test from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";

import { Engine, read } from "../src/index.js";
import { InvalidArgumentError, WriteValidationError } from "../src/errors.js";
import type { NodeRecord } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

// 0.8.20 (R-20-E3): `sourceId` is mandatory on every canonical write.
const SOURCE_ID = "ts-test:slice15b-node-validity-write";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * A node write item carrying an explicit window in camelCase. A `null` bound is
 * OMITTED from the item rather than sent as JSON `null` — that is the shape a
 * caller who simply does not have that bound produces.
 */
function windowed(
  logicalId: string,
  body: string,
  validFrom: number | null,
  validUntil: number | null,
): object {
  const item: Record<string, unknown> = { kind: "doc", body, logicalId, sourceId: SOURCE_ID };
  if (validFrom !== null) item.validFrom = validFrom;
  if (validUntil !== null) item.validUntil = validUntil;
  return item;
}

/** A node write item that omits the window entirely — the pre-slice shape. */
function plain(logicalId: string, body: string): object {
  return { kind: "doc", body, logicalId, sourceId: SOURCE_ID };
}

async function openEngine(path: string): Promise<Engine> {
  return Engine.open(path, { useDefaultEmbedder: false });
}

/** Seed a batch on a fresh engine, drain, and CLOSE — freeing the file. */
async function seed(path: string, batch: object[]): Promise<void> {
  const engine = await openEngine(path);
  try {
    await engine.write(batch);
    await engine.drain(30_000);
  } finally {
    await engine.close();
  }
}

function ids(rows: NodeRecord[]): string[] {
  return rows.map((r) => r.logicalId).sort();
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
    return [row.f === null ? null : Number(row.f), row.u === null ? null : Number(row.u)];
  } finally {
    db.close();
  }
}

function rawRowCount(path: string, logicalId: string): number {
  const db = new DatabaseSync(path);
  try {
    const row = db
      .prepare("SELECT COUNT(*) AS n FROM canonical_nodes WHERE logical_id = ?")
      .get(logicalId) as { n: number };
    return Number(row.n);
  } finally {
    db.close();
  }
}

// ---------------------------------------------------------------------------
// (1) TC-34 — the round trip
// ---------------------------------------------------------------------------

test("TC-34: an SDK-authored window round-trips through validAsOf", async () => {
  const path = freshDbPath();
  await seed(path, [windowed("WINDOWED", "bounded body", 1000, 2000)]);

  // On disk exactly as authored — no coercion, no clock.
  assert.deepStrictEqual(rawWindow(path, "WINDOWED"), [1000, 2000]);

  const engine = await openEngine(path);
  try {
    // INSIDE.
    assert.ok(
      (await read.get(engine, "WINDOWED", { validAsOf: 1500 })) !== null,
      "visible at an instant inside the authored window",
    );
    assert.deepStrictEqual(
      ids(await read.list(engine, "doc", undefined, 100, undefined, { validAsOf: 1500 })),
      ["WINDOWED"],
    );

    // OUTSIDE, both sides.
    assert.equal(await read.get(engine, "WINDOWED", { validAsOf: 500 }), null);
    assert.equal(await read.get(engine, "WINDOWED", { validAsOf: 2500 }), null);
    assert.deepStrictEqual(
      await read.list(engine, "doc", undefined, 100, undefined, { validAsOf: 2500 }),
      [],
    );

    // The escape hatch still relaxes the conjunct.
    assert.ok(
      (await read.get(engine, "WINDOWED", { validAsOf: 2500, includeOutOfWindow: true })) !== null,
      "includeOutOfWindow must still surface an out-of-window authored row",
    );
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (2) TC-34 — half-open boundaries
// ---------------------------------------------------------------------------

test("TC-34: an SDK-authored window is half-open [validFrom, validUntil)", async () => {
  const path = freshDbPath();
  await seed(path, [windowed("HALFOPEN", "boundary body", 1000, 2000)]);

  const engine = await openEngine(path);
  try {
    assert.equal(
      await read.get(engine, "HALFOPEN", { validAsOf: 999 }),
      null,
      "one second before validFrom is OUT",
    );
    assert.ok(
      (await read.get(engine, "HALFOPEN", { validAsOf: 1000 })) !== null,
      "exactly validFrom is IN (lower bound inclusive)",
    );
    assert.ok(
      (await read.get(engine, "HALFOPEN", { validAsOf: 1999 })) !== null,
      "one second before validUntil is IN",
    );
    assert.equal(
      await read.get(engine, "HALFOPEN", { validAsOf: 2000 }),
      null,
      "exactly validUntil is OUT (upper bound exclusive)",
    );
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (3) TC-34 — unbounded sides
// ---------------------------------------------------------------------------

test("TC-34: one authored bound leaves the other side unbounded", async () => {
  const path = freshDbPath();
  await seed(path, [
    windowed("FROM_ONLY", "from only", 1000, null),
    windowed("UNTIL_ONLY", "until only", null, 2000),
  ]);

  // The omitted side is NULL on disk, not a sentinel.
  assert.deepStrictEqual(rawWindow(path, "FROM_ONLY"), [1000, null]);
  assert.deepStrictEqual(rawWindow(path, "UNTIL_ONLY"), [null, 2000]);

  const engine = await openEngine(path);
  try {
    assert.equal(await read.get(engine, "FROM_ONLY", { validAsOf: 999 }), null);
    assert.ok((await read.get(engine, "FROM_ONLY", { validAsOf: 1000 })) !== null);
    assert.ok((await read.get(engine, "FROM_ONLY", { validAsOf: 4_000_000_000 })) !== null);

    assert.ok((await read.get(engine, "UNTIL_ONLY", { validAsOf: 0 })) !== null);
    assert.ok((await read.get(engine, "UNTIL_ONLY", { validAsOf: 1999 })) !== null);
    assert.equal(await read.get(engine, "UNTIL_ONLY", { validAsOf: 2000 }), null);
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (4) TC-34 — MUST NOT REGRESS: omitting both fields
// ---------------------------------------------------------------------------

test("TC-34: omitting both fields lands NULL/NULL and preserves default visibility", async () => {
  const path = freshDbPath();
  await seed(path, [plain("PLAIN", "no window authored")]);

  // RAW TABLE oracle. A read-verb assertion would pass on broken code.
  assert.deepStrictEqual(
    rawWindow(path, "PLAIN"),
    [null, null],
    "a write omitting the window MUST land NULL/NULL — not 0, not now(), not a sentinel",
  );

  const engine = await openEngine(path);
  try {
    for (const instant of [0, 1, 1000, 2_000_000_000]) {
      assert.ok(
        (await read.get(engine, "PLAIN", { validAsOf: instant })) !== null,
        `NULL/NULL row must be valid at instant ${instant}`,
      );
    }

    // And through the shipped DEFAULT view (no validAsOf — resolves to the wall
    // clock), which is the path every existing caller takes.
    assert.ok((await read.get(engine, "PLAIN")) !== null);
    assert.deepStrictEqual(ids(await read.list(engine, "doc", undefined, 100)), ["PLAIN"]);
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (5) TC-34 — camelCase and snake_case are both accepted
// ---------------------------------------------------------------------------

test("TC-34: validFrom/validUntil and valid_from/valid_until are equivalent", async () => {
  const path = freshDbPath();
  await seed(path, [
    {
      kind: "doc",
      body: "camel",
      logicalId: "CAMEL",
      sourceId: SOURCE_ID,
      validFrom: 1000,
      validUntil: 2000,
    },
    {
      kind: "doc",
      body: "snake",
      logical_id: "SNAKE",
      source_id: SOURCE_ID,
      valid_from: 1000,
      valid_until: 2000,
    },
  ]);

  // Identical on disk — the two spellings are one field, as with tValid/t_valid.
  assert.deepStrictEqual(rawWindow(path, "CAMEL"), [1000, 2000]);
  assert.deepStrictEqual(rawWindow(path, "SNAKE"), [1000, 2000]);

  const engine = await openEngine(path);
  try {
    assert.deepStrictEqual(
      ids(await read.list(engine, "doc", undefined, 100, undefined, { validAsOf: 1500 })),
      ["CAMEL", "SNAKE"],
    );
    assert.deepStrictEqual(
      await read.list(engine, "doc", undefined, 100, undefined, { validAsOf: 2500 }),
      [],
    );
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (6) TC-34 — typed refusals
// ---------------------------------------------------------------------------

test("TC-34: an unsatisfiable window is a typed refusal, not a silent write", async () => {
  const path = freshDbPath();
  const engine = await openEngine(path);
  try {
    await assert.rejects(
      () => engine.write([windowed("BAD", "inverted", 2000, 1000)]),
      InvalidArgumentError,
      "an inverted window must raise InvalidArgumentError",
    );
    await assert.rejects(
      () => engine.write([windowed("BAD", "empty", 1500, 1500)]),
      InvalidArgumentError,
      "an empty half-open window must raise InvalidArgumentError",
    );

    // The refusal rejects the WHOLE batch.
    await assert.rejects(
      () => engine.write([plain("GOOD", "well formed"), windowed("BAD", "inverted", 2000, 1000)]),
      InvalidArgumentError,
    );
  } finally {
    await engine.close();
  }

  assert.equal(rawRowCount(path, "BAD"), 0, "a refused write must not land a row");
  assert.equal(rawRowCount(path, "GOOD"), 0, "batch rejection must not commit the sibling row");
});

test("TC-34: a non-integer validFrom is a typed refusal, not a coercion", async () => {
  const path = freshDbPath();
  const engine = await openEngine(path);
  try {
    for (const bad of ["1000", 10.5, true, {}] as unknown[]) {
      await assert.rejects(
        () =>
          engine.write([
            { kind: "doc", body: "bad", logicalId: "BADTYPE", sourceId: SOURCE_ID, validFrom: bad },
          ]),
        WriteValidationError,
        `validFrom = ${JSON.stringify(bad)} must be refused, never coerced`,
      );
    }
  } finally {
    await engine.close();
  }

  assert.equal(rawRowCount(path, "BADTYPE"), 0);
});

// ---------------------------------------------------------------------------
// (7) TC-34 — the boundary hook, end-to-end on an authored window
// ---------------------------------------------------------------------------

test("TC-34: crossedBoundarySince works on SDK-authored windows", async () => {
  const path = freshDbPath();
  await seed(path, [
    windowed("OPENED", "opened", 1500, null),
    windowed("CLOSED", "closed", 0, 1500),
    windowed("BOTH", "both", 1200, 1800),
    windowed("OUTSIDE", "outside", 5000, 6000),
    plain("UNBOUNDED", "unbounded"),
  ]);

  const engine = await openEngine(path);
  try {
    const crossings = await read.crossedBoundarySince(engine, 1000, { validAsOf: 2000 });
    const got = crossings
      .map((c) => [c.node.logicalId, c.becameValidAt ?? null, c.becameInvalidAt ?? null])
      .sort((a, b) => String(a[0]).localeCompare(String(b[0])));

    assert.deepStrictEqual(got, [
      ["BOTH", 1200, 1800],
      ["CLOSED", null, 1500],
      ["OPENED", 1500, null],
    ]);
  } finally {
    await engine.close();
  }
});
