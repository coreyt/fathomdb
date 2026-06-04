// X1 functional retrieve harness (TypeScript SDK) — 0.8.0 Slice 30 / G2+G3.
//
// Opens a REAL engine, writes canonical nodes (with logicalIds) + op-store rows,
// then exercises the governed `read.*` namespace end-to-end across the FFI:
//
//   * read.get / read.getMany return the written ACTIVE nodes by id; a
//     superseded version is NOT returned; a missing id is null.
//   * read.collection / read.mutations return the op-store rows with the cursor
//     + mandatory limit honored.
//   * admin.configure path is exercised.
//
// Shares ONE fixture (functional_retrieve_fixture.json) with the Python harness
// (src/python/tests/test_functional_retrieve.py); the cross-binding equivalence
// is asserted against the same corpus + ids so both bindings surface equivalent
// rows for the same DB. No mocking of the database.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Engine, admin, read } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

const here = dirname(fileURLToPath(import.meta.url));
// Single source of truth shared with the Python harness. Compiled location is
// `src/ts/dist/tests/`, so three levels up reaches the repo `src/` directory.
const FIXTURE_PATH = join(
  here,
  "..",
  "..",
  "..",
  "python",
  "tests",
  "functional_retrieve_fixture.json",
);

interface Fixture {
  nodes: { kind: string; body: string; logical_id: string }[];
  superseded: { logical_id: string; kind: string; old_body: string; new_body: string };
  collection: string;
  op_rows: { record_key: string; body: string }[];
}

function loadFixture(): Fixture {
  return JSON.parse(readFileSync(FIXTURE_PATH, "utf-8")) as Fixture;
}

async function seed(engine: Engine, fixture: Fixture): Promise<void> {
  const sup = fixture.superseded;
  await engine.write([{ kind: sup.kind, body: sup.old_body, logicalId: sup.logical_id }]);
  for (const node of fixture.nodes) {
    await engine.write([{ kind: node.kind, body: node.body, logicalId: node.logical_id }]);
  }
  await engine.write([
    {
      adminSchema: {
        name: fixture.collection,
        kind: "append_only_log",
        schemaJson: '{"type":"object"}',
        retentionJson: "{}",
      },
    },
  ]);
  for (const row of fixture.op_rows) {
    await engine.write([
      { opStore: { collection: fixture.collection, recordKey: row.record_key, body: row.body } },
    ]);
  }
}

test("functional retrieve: read.get returns the active node by id", async () => {
  const fixture = loadFixture();
  const engine = await Engine.open(freshDbPath());
  try {
    await seed(engine, fixture);

    const got = await read.get(engine, "F2");
    assert.ok(got !== null);
    assert.equal(got.logicalId, "F2");
    assert.equal(got.kind, "fact");
    assert.equal(got.body, "water boils at 100C");
    assert.ok(got.writeCursor > 0);

    // Superseded F1: only the active (new) body is returned.
    const f1 = await read.get(engine, "F1");
    assert.ok(f1 !== null);
    assert.equal(f1.body, "the sky is blue");

    // Missing id → null (normal absence, not a thrown error).
    assert.equal(await read.get(engine, "DOES_NOT_EXIST"), null);
  } finally {
    await engine.close();
  }
});

test("functional retrieve: read.getMany preserves order with null", async () => {
  const fixture = loadFixture();
  const engine = await Engine.open(freshDbPath());
  try {
    await seed(engine, fixture);
    const rows = await read.getMany(engine, ["N1", "MISSING", "F2"]);
    assert.equal(rows.length, 3);
    assert.equal(rows[0]?.body, "remember to hydrate");
    assert.equal(rows[1], null);
    assert.equal(rows[2]?.body, "water boils at 100C");
  } finally {
    await engine.close();
  }
});

test("functional retrieve: read.collection/read.mutations honor cursor + limit", async () => {
  const fixture = loadFixture();
  const engine = await Engine.open(freshDbPath());
  try {
    await seed(engine, fixture);

    const page1 = await read.collection(engine, fixture.collection, { limit: 3 });
    assert.deepEqual(
      page1.map((r) => r.recordKey),
      ["e0", "e1", "e2"],
    );
    assert.ok(page1.every((r) => r.collection === fixture.collection));
    assert.ok(page1.every((r) => r.opKind === "append"));

    const cursor = page1[page1.length - 1]!.id;
    const page2 = await read.collection(engine, fixture.collection, { afterId: cursor, limit: 3 });
    assert.deepEqual(
      page2.map((r) => r.recordKey),
      ["e3", "e4"],
    );
    assert.ok(page2.every((r) => r.id > cursor));

    const muts = await read.mutations(engine, fixture.collection, { limit: 100 });
    assert.deepEqual(
      muts.map((r) => r.id),
      [...page1, ...page2].map((r) => r.id),
    );
  } finally {
    await engine.close();
  }
});

test("functional retrieve: admin.configure path exercised", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const receipt = await admin.configure(engine, { name: "latest_default", body: "{}" });
    assert.equal(typeof receipt.cursor, "number");
  } finally {
    await engine.close();
  }
});

// Cross-binding equivalence: the Python harness asserts these SAME values
// (read.get bodies for F1/F2/N1 + the ordered op-store record_key list) for the
// SAME fixture, proving Py ≡ TS for each read verb on the same DB.
const EXPECTED_BODIES = ["the sky is blue", "water boils at 100C", "remember to hydrate"];
const EXPECTED_KEYS = ["e0", "e1", "e2", "e3", "e4"];

test("functional retrieve: cross-binding equivalence (Py ≡ TS)", async () => {
  const fixture = loadFixture();
  const engine = await Engine.open(freshDbPath());
  try {
    await seed(engine, fixture);

    const bodies: string[] = [];
    for (const lid of ["F1", "F2", "N1"]) {
      const record = await read.get(engine, lid);
      assert.ok(record !== null, `${lid} must be active and present`);
      bodies.push(record.body);
    }
    assert.deepEqual(bodies, EXPECTED_BODIES);

    const keys = (await read.collection(engine, fixture.collection, { limit: 1000 })).map(
      (r) => r.recordKey,
    );
    assert.deepEqual(keys, EXPECTED_KEYS);
  } finally {
    await engine.close();
  }
});
