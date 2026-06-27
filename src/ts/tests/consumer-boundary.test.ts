// 0.8.6 Slice 10 (OPP-5) — the governed READ surface is the COMPLETE consumer
// boundary (R-CH-1), TS mirror of `src/python/tests/test_consumer_boundary.py`.
//
// A consumer (e.g. Memex) migrating onto FathomDB's governed verbs must satisfy
// every read-path need through the PUBLIC namespaces — `read.*`, `graph.*`,
// `engine.embed`, plus core `search`/`write` — WITHOUT reaching into the engine's
// private surface. This test enumerates the verbs a consumer needs and asserts
// each is (a) a public callable (no internal reach), and (b) a member of the
// governed-surface allowlist (a sanctioned boundary, not an accidental leak).

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Engine, read, graph } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

const here = dirname(fileURLToPath(import.meta.url));
const CONTRACT_PATH = join(
  here,
  "..",
  "..",
  "..",
  "conformance",
  "governed-surface-allowlist.json",
);
const allowlist: Set<string> = new Set(
  (JSON.parse(readFileSync(CONTRACT_PATH, "utf-8")) as { allowlist: string[] }).allowlist,
);

// camelCase consumer read verbs (the TS surface idiom).
const CONSUMER_READ_VERBS: Record<string, unknown> = {
  "read.get": read.get,
  "read.getMany": read.getMany,
  "read.collection": read.collection,
  "read.mutations": read.mutations,
  "read.list": read.list,
  "graph.neighbors": graph.neighbors,
  "graph.searchExpand": graph.searchExpand,
};

test("consumer boundary: every read verb is a public callable (no internal reach)", () => {
  for (const [name, fn] of Object.entries(CONSUMER_READ_VERBS)) {
    assert.equal(typeof fn, "function", `${name} must be a public callable on the governed surface`);
  }
});

test("consumer boundary: engine.embed is a public method", async () => {
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    assert.equal(typeof engine.embed, "function", "engine.embed must be a public method");
  } finally {
    await engine.close();
  }
});

test("consumer boundary: the needed verbs are all governed (allowlisted)", () => {
  // Allowlist members are language-canonicalized; assert the sanctioned read/graph/
  // embed contract a consumer relies on is present (snake_case canonical names).
  const needed = [
    "read.get",
    "read.get_many",
    "read.collection",
    "read.mutations",
    "read.list",
    "graph.neighbors",
    "graph.search_expand",
    "embed",
    "search",
    "write",
    "Engine.open",
    "close",
  ];
  const missing = needed.filter((n) => !allowlist.has(n));
  assert.deepEqual(missing, [], `consumer-boundary verbs missing from the governed allowlist: ${missing}`);
});
