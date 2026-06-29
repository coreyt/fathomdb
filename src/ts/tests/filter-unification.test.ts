// 0.8.11 Slice 40 (#17) — unified Filter grammar parity (TypeScript SDK).
//
// Mirrors `src/python/tests/test_filter_unification.py` (cross-binding X1
// parity) and the Rust `slice40_filter_unification.rs` engine suite. Exercises
// the unified `Filter` over BOTH backends from a real engine (no mocking):
//
//   * the five closed FilterTerm variants (discriminated union);
//   * SearchFilter <-> Filter sugar round-trip (D4);
//   * vec0 (`engine.search`) dispatch typed-rejects a `json` term (D3);
//   * `read.list(filter=...)` accepts the full set incl. the kind/source_type
//     constant-folds (the engine performs the authoritative dispatch).
//
// `read.list` stays the SAME governed verb (no new surface member); the unified
// grammar rides an additive `filter` argument.

import test from "node:test";
import assert from "node:assert/strict";

import {
  Engine,
  read,
  filterToSearchFilter,
  searchFilterToFilter,
  type Filter,
  type SearchFilter,
} from "../src/index.js";
import { InvalidFilterError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

async function seedTodoNodes(engine: Engine): Promise<void> {
  const rows = [
    { logicalId: "A", body: { status: "open", created_at: 100, priority: 5 } },
    { logicalId: "B", body: { status: "done", created_at: 200, priority: 1 } },
    { logicalId: "C", body: { status: "open", created_at: 300, priority: 9 } },
  ];
  for (const r of rows) {
    await engine.write([{ kind: "todo", body: JSON.stringify(r.body), logicalId: r.logicalId }]);
  }
}

// ----- D4 sugar round-trip --------------------------------------------------

test("SearchFilter round-trips through the unified Filter", () => {
  const sf: SearchFilter = {
    sourceType: "todo",
    kind: "todo",
    createdAfter: 1000,
    status: "open",
  };
  const unified = searchFilterToFilter(sf);
  assert.deepEqual(unified.terms, [
    { term: "source_type", value: "todo" },
    { term: "kind", value: "todo" },
    { term: "created_after", value: 1000 },
    { term: "status", value: "open" },
  ]);
  assert.deepEqual(filterToSearchFilter(unified), sf);
  // empty SearchFilter -> empty terms
  assert.deepEqual(searchFilterToFilter({}).terms, []);
});

// ----- D3 typed rejection (vec0 / search backend) ---------------------------

test("filterToSearchFilter typed-rejects a json term", () => {
  const f: Filter = { terms: [{ term: "json", predicate: { type: "gt", path: "$.priority", value: 3 } }] };
  assert.throws(() => filterToSearchFilter(f), InvalidFilterError);
});

test("engine.search typed-rejects a json Filter", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const f: Filter = {
      terms: [{ term: "json", predicate: { type: "eq", path: "$.status", value: "open" } }],
    };
    await assert.rejects(() => engine.search("anything", f), InvalidFilterError);
  } finally {
    await engine.close();
  }
});

// ----- D3 read.list(filter=...) full set + constant-folds -------------------

test("read.list(filter) accepts the full term set", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTodoNodes(engine);
    const f: Filter = {
      terms: [
        { term: "status", value: "open" },
        { term: "created_after", value: 150 },
        { term: "json", predicate: { type: "gt", path: "$.priority", value: 3 } },
      ],
    };
    const rows = await read.list(engine, "todo", undefined, 100, f);
    const ids = rows.map((r) => r.logicalId).sort();
    assert.deepEqual(ids, ["C"]);
  } finally {
    await engine.close();
  }
});

test("read.list(filter) kind term constant-folds vs the partition", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTodoNodes(engine);
    const all = await read.list(engine, "todo", undefined, 100, { terms: [{ term: "kind", value: "todo" }] });
    assert.equal(all.length, 3);
    const none = await read.list(engine, "todo", undefined, 100, { terms: [{ term: "kind", value: "note" }] });
    assert.equal(none.length, 0);
  } finally {
    await engine.close();
  }
});

test("read.list(filter) source_type term constant-folds (resolve_source_type)", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seedTodoNodes(engine);
    const match = await read.list(engine, "todo", undefined, 100, {
      terms: [{ term: "source_type", value: "todo" }],
    });
    assert.equal(match.length, 3);
    const empty = await read.list(engine, "todo", undefined, 100, {
      terms: [{ term: "source_type", value: "email" }],
    });
    assert.equal(empty.length, 0);
  } finally {
    await engine.close();
  }
});

test("read.list rejects both predicates and filter", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await assert.rejects(
      () =>
        read.list(
          engine,
          "todo",
          [{ type: "eq", path: "$.status", value: "open" }],
          100,
          { terms: [{ term: "status", value: "open" }] },
        ),
      /pass either/,
    );
  } finally {
    await engine.close();
  }
});
