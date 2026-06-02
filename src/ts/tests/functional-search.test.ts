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
      assert.equal(typeof hit.id, "number");
      assert.ok(hit.id > 0);
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
