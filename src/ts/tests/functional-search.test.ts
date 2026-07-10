// X1 functional search harness (TypeScript SDK) — 0.8.0 Slice 5 / G1.
//
// Opens a REAL engine, writes a small corpus, `search()`es, and asserts the
// structured `SearchHit` shape end-to-end across the FFI (id / kind / body /
// score / branch present and correctly typed). Also asserts cross-binding
// equivalence against the SAME `functional_search_fixture.json` the Python
// harness (`src/python/tests/test_functional_search.py`) uses, so both
// bindings are shown to surface equivalent hits for the same DB + query.
//
// This is the seed of the write -> search -> retrieve -> admin harness every
// later slice extends. No mocking of the database.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Engine } from "../src/index.js";
import type { SearchHit } from "../src/index.js";
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
  "functional_search_fixture.json",
);

interface Fixture {
  corpus: { kind: string; body: string }[];
  queries: { query: string; expected_bodies: string[] }[];
}

function loadFixture(): Fixture {
  return JSON.parse(readFileSync(FIXTURE_PATH, "utf-8")) as Fixture;
}

async function searchAfterProjection(
  engine: Engine,
  query: string,
): Promise<SearchHit[]> {
  const deadline = Date.now() + 10_000;
  let last: SearchHit[] = [];
  while (Date.now() < deadline) {
    const result = await engine.search(query);
    last = result.results;
    if (last.length > 0) {
      return last;
    }
    await new Promise((r) => setTimeout(r, 20));
  }
  return last;
}

test("functional search: structured hit shape across the FFI", async () => {
  const fixture = loadFixture();
  const engine = await Engine.open(freshDbPath());
  try {
    for (const doc of fixture.corpus) {
      await engine.write([{ kind: doc.kind, body: doc.body }]);
    }
    await engine.drain(30_000);

    const hits = await searchAfterProjection(engine, "structured");
    assert.ok(hits.length > 0, "expected at least one structured hit");
    for (const hit of hits) {
      // C-2 (0.8.19 / TC-8) — `id` is the typed IdSpace {space, value}, non-null
      // + id-space-total, crossing the FFI. Doc-seeded corpus nodes carry NULL
      // logical_id, so the id is the `content` (`"h:"`) space with a 64-hex
      // content-hash value (never null for a real node hit). Py↔TS parity with
      // the Python `IdSpace`. The pre-0.8.19 int write_cursor id is engine-
      // internal and no longer surfaced.
      assert.equal(hit.id.space, "content");
      assert.equal(typeof hit.id.value, "string");
      assert.equal(hit.id.value.length, 64);
      assert.equal(typeof hit.kind, "string");
      assert.ok(hit.kind.length > 0);
      assert.equal(typeof hit.body, "string");
      assert.ok(hit.body.length > 0);
      assert.equal(typeof hit.score, "number");
      assert.ok(hit.branch === "vector" || hit.branch === "text");
    }
  } finally {
    await engine.close();
  }
});

test("functional search: cross-binding equivalence with the Python harness", async () => {
  const fixture = loadFixture();
  const engine = await Engine.open(freshDbPath());
  try {
    for (const doc of fixture.corpus) {
      await engine.write([{ kind: doc.kind, body: doc.body }]);
    }
    await engine.drain(30_000);

    for (const testCase of fixture.queries) {
      const hits = await searchAfterProjection(engine, testCase.query);
      const got = hits.map((h) => h.body).sort();
      const expected = [...testCase.expected_bodies].sort();
      assert.deepEqual(
        got,
        expected,
        `query ${JSON.stringify(testCase.query)}: TypeScript binding returned ` +
          `${JSON.stringify(got)}, cross-binding fixture expects ${JSON.stringify(expected)}`,
      );
      // FTS-only corpus -> every hit carries the text branch tag.
      assert.ok(hits.every((h) => h.branch === "text"));
    }
  } finally {
    await engine.close();
  }
});

// Slice 10 / X1 — RRF-fused order shared by both bindings. The text branch ranks
// by `write_cursor` (insertion order), so "retrieval" surfaces alpha (written
// first) before delta. The Python harness asserts this same order.
const RRF_ORDER_QUERY = "retrieval";
const RRF_EXPECTED_ORDER = [
  "alpha structured retrieval document",
  "delta retrieval and ranking notes",
];

test("functional search: RRF-fused order matches the Python binding", async () => {
  const fixture = loadFixture();
  const engine = await Engine.open(freshDbPath());
  try {
    for (const doc of fixture.corpus) {
      await engine.write([{ kind: doc.kind, body: doc.body }]);
    }
    await engine.drain(30_000);

    const hits = await searchAfterProjection(engine, RRF_ORDER_QUERY);
    assert.deepEqual(
      hits.map((h) => h.body),
      RRF_EXPECTED_ORDER,
      "RRF-fused order must match the Python binding (rank by write_cursor)",
    );
    const scores = hits.map((h) => h.score);
    assert.deepEqual(scores, [...scores].sort((a, b) => b - a));
  } finally {
    await engine.close();
  }
});

// Slice 15 / X1 / G0 — WriteReceipt.rowCursors is 1:1 with the batch in input
// order and deterministic on a fresh DB. The exact values ([1,2,3] then [4,5])
// are what the Python harness also asserts, proving Py ≡ TS rowCursors for the
// same DB + writes (cross-binding equivalence).
test("functional write: rowCursors are 1:1 with the batch (Py ≡ TS)", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const first = await engine.write([
      { kind: "doc", body: "rc-a" },
      { kind: "doc", body: "rc-b" },
      { kind: "doc", body: "rc-c" },
    ]);
    assert.deepEqual(first.rowCursors, [1, 2, 3]);
    assert.equal(first.cursor, 3);
    assert.equal(first.rowCursors[first.rowCursors.length - 1], first.cursor);

    const second = await engine.write([
      { kind: "doc", body: "rc-d" },
      { kind: "doc", body: "rc-e" },
    ]);
    assert.deepEqual(second.rowCursors, [4, 5]);
    assert.equal(second.cursor, 5);
  } finally {
    await engine.close();
  }
});

// Slice 15 / X1 / G0 — a supersession write (same logicalId) is accepted by the
// SDK and returns its per-row cursor. Active-only read visibility is reserved
// for G2 + shadow reconciliation (Slice 16); the Python harness asserts the
// same values.
test("functional write: a supersession write surfaces its row cursor", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const v1 = await engine.write([{ kind: "doc", body: "fact v1", logicalId: "L1" }]);
    const v2 = await engine.write([{ kind: "doc", body: "fact v2", logicalId: "L1" }]);
    assert.deepEqual(v1.rowCursors, [1]);
    assert.deepEqual(v2.rowCursors, [2]);
    assert.ok(v2.cursor > v1.cursor);
  } finally {
    await engine.close();
  }
});

// Slice 20 / X1 / G8 — a write whose batch contains a dangling edge returns
// danglingEdgeEndpoints > 0; a clean batch returns 0. The Python harness
// (test_functional_dangling_edge_count_across_ffi) asserts the SAME values for
// the SAME batches, proving Py ≡ TS for the dangling count.
test("functional write: dangling-edge count (Py ≡ TS)", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    // Clean batch: the edge's endpoints resolve to live logicalId nodes inserted
    // later in the SAME batch (cross-row) -> 0 dangling.
    const clean = await engine.write([
      { kind: "doc", body: "n1", logicalId: "N1" },
      { kind: "doc", body: "n2", logicalId: "N2" },
      { edge: { kind: "rel", from: "N1", to: "N2" } },
    ]);
    assert.equal(clean.danglingEdgeEndpoints, 0);

    // Dangling batch: both endpoints reference missing logicalIds -> 2
    // (flag-and-count: the write still succeeds).
    const dangling = await engine.write([
      { edge: { kind: "rel", from: "GHOST_A", to: "GHOST_B" } },
    ]);
    assert.equal(dangling.danglingEdgeEndpoints, 2);
  } finally {
    await engine.close();
  }
});

test("functional search: a SearchFilter prunes results", async () => {
  const fixture = loadFixture();
  const engine = await Engine.open(freshDbPath());
  try {
    for (const doc of fixture.corpus) {
      await engine.write([{ kind: doc.kind, body: doc.body }]);
    }
    await engine.drain(30_000);

    const unfiltered = await searchAfterProjection(engine, "retrieval");
    const kinds = new Set(unfiltered.map((h) => h.kind));
    assert.ok(kinds.has("note") && kinds.has("doc"));

    // Filter kind=note drops the doc hit.
    const filtered = await engine.search("retrieval", { kind: "note" });
    assert.deepEqual(
      filtered.results.map((h) => h.body),
      ["alpha structured retrieval document"],
    );
    assert.ok(filtered.results.every((h) => h.kind === "note"));

    // A filter on the NULL-plumbed `status` prunes everything.
    const empty = await engine.search("retrieval", { status: "open" });
    assert.deepEqual(empty.results, []);
  } finally {
    await engine.close();
  }
});
