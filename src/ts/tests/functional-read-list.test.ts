// X1 functional read.list harness (TypeScript SDK) — 0.8.1 Slice 35 / G4.
//
// Opens a REAL engine, writes canonical nodes whose `body` is a JSON object
// carrying fields matched by the G4 allowlist ($.status, $.priority,
// $.created_at), then exercises `read.list` end-to-end:
//
//   * Unfiltered path returns all active nodes of the kind.
//   * Eq predicate filters to matching nodes.
//   * Comparison predicates (gt, gte, lt, lte) work.
//   * Multiple predicates AND-compose.
//   * Non-allowlisted path throws InvalidFilterError.
//   * Cross-binding equivalence anchor matches test_read_list.py.
//
// No mocking — the engine runs against a real (tmpdir) SQLite file.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine, read } from "../src/index.js";
import { InvalidFilterError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

async function seedTaskNodes(engine: Engine): Promise<void> {
  const tasks = [
    { logicalId: "T1", body: { status: "open",   priority: 10, created_at: 1000 } },
    { logicalId: "T2", body: { status: "closed", priority: 20, created_at: 2000 } },
    { logicalId: "T3", body: { status: "open",   priority: 30, created_at: 3000 } },
  ];
  for (const t of tasks) {
    await engine.write([{ kind: "task", body: JSON.stringify(t.body), logicalId: t.logicalId }]);
  }
}

test("functional read.list: unfiltered returns all active nodes of the kind", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTaskNodes(engine);
    // Write a node of a different kind — it must NOT appear.
    await engine.write([{ kind: "note", body: "hello", logicalId: "N1" }]);

    const rows = await read.list(engine, "task");
    assert.equal(rows.length, 3);
    const ids = new Set(rows.map((r) => r.logicalId));
    assert.deepEqual(ids, new Set(["T1", "T2", "T3"]));
    assert.ok(rows.every((r) => r.kind === "task"));
  } finally {
    await engine.close();
  }
});

test("functional read.list: eq predicate filters correctly", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTaskNodes(engine);
    const rows = await read.list(engine, "task", [
      { type: "eq", path: "$.status", value: "open" },
    ]);
    assert.equal(rows.length, 2);
    assert.ok(rows.every((r) => JSON.parse(r.body).status === "open"));
    const ids = new Set(rows.map((r) => r.logicalId));
    assert.deepEqual(ids, new Set(["T1", "T3"]));
  } finally {
    await engine.close();
  }
});

test("functional read.list: gt predicate filters correctly", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTaskNodes(engine);
    const rows = await read.list(engine, "task", [
      { type: "gt", path: "$.priority", value: 10 },
    ]);
    assert.equal(rows.length, 2);
    const ids = new Set(rows.map((r) => r.logicalId));
    assert.deepEqual(ids, new Set(["T2", "T3"]));
  } finally {
    await engine.close();
  }
});

test("functional read.list: AND-composition (eq + gt)", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTaskNodes(engine);
    // open AND priority > 10 → only T3 (priority=30, status=open)
    const rows = await read.list(engine, "task", [
      { type: "eq", path: "$.status",   value: "open" },
      { type: "gt", path: "$.priority", value: 10 },
    ]);
    assert.equal(rows.length, 1);
    assert.equal(rows[0]!.logicalId, "T3");
  } finally {
    await engine.close();
  }
});

test("functional read.list: limit is respected", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTaskNodes(engine);
    const rows = await read.list(engine, "task", undefined, 2);
    assert.equal(rows.length, 2);
  } finally {
    await engine.close();
  }
});

test("functional read.list: non-allowlisted path throws InvalidFilterError", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTaskNodes(engine);
    await assert.rejects(
      () => read.list(engine, "task", [{ type: "eq", path: "$.not_allowed_field", value: "x" }]),
      (err: unknown) => {
        assert.ok(err instanceof InvalidFilterError, `expected InvalidFilterError, got ${err}`);
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});

test("functional read.list: empty predicates returns all", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTaskNodes(engine);
    const rows = await read.list(engine, "task", []);
    assert.equal(rows.length, 3);
  } finally {
    await engine.close();
  }
});

// Cross-binding equivalence anchor: Python harness (test_read_list.py)
// `test_read_list_cross_binding_equivalence_anchor` asserts the SAME result set
// for the same predicates on the same kind, proving Py ≡ TS for read.list.
test("functional read.list: cross-binding equivalence anchor (Py ≡ TS)", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTaskNodes(engine);
    // status == "open" AND priority >= 10 → T1 (priority=10) + T3 (priority=30)
    const rows = await read.list(engine, "task", [
      { type: "eq",  path: "$.status",   value: "open" },
      { type: "gte", path: "$.priority", value: 10 },
    ]);
    const ids = rows.map((r) => r.logicalId).sort();
    assert.deepEqual(ids, ["T1", "T3"]);
  } finally {
    await engine.close();
  }
});
