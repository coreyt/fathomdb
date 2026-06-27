// 0.8.6 Slice 10 — `Engine.embed()`: the read-path embed primitive, brought to
// Py↔TS parity (OPP-5 coupling hygiene). Mirror of the Python `test_embed.py`.
//
// `embed` exposes the engine's pinned default embedder
// (`fathomdb-bge-small-en-v1.5`) as a direct `text -> vector` call, so callers
// (e.g. coverage-index clustering) embed under the engine's OWN identity rather
// than a parallel, possibly-divergent embedder. Network-hitting (weights fetched
// on first use) — honours `FATHOMDB_SKIP_NETWORK_TESTS`.
//
// CROSS-BINDING ANCHOR: `embed_golden_anchor` below and the Python
// `test_embed_cross_binding_golden_anchor` BOTH assert `embed(anchorText)`
// matches the SAME committed golden (`src/conformance/embed-anchor-golden.json`)
// within tolerance — proving Py ≡ TS produce the same vector under the same
// embedder identity.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Engine } from "../src/index.js";
import { EmbedderNotConfiguredError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

const here = dirname(fileURLToPath(import.meta.url));
// Compiled location is `src/ts/dist/tests/`, so three `..` reach repo `src/`.
const GOLDEN_PATH = join(here, "..", "..", "..", "conformance", "embed-anchor-golden.json");

const DIM = 384; // fathomdb-bge-small-en-v1.5

interface Golden {
  embedder: string;
  anchor_text: string;
  dim: number;
  tolerance: number;
  vector: number[];
}

function loadGolden(): Golden {
  return JSON.parse(readFileSync(GOLDEN_PATH, "utf-8")) as Golden;
}

function skipNetwork(): boolean {
  return Boolean(process.env.FATHOMDB_SKIP_NETWORK_TESTS);
}

function cosine(a: number[], b: number[]): number {
  let dot = 0;
  let na = 0;
  let nb = 0;
  for (let i = 0; i < a.length; i++) {
    dot += a[i] * b[i];
    na += a[i] * a[i];
    nb += b[i] * b[i];
  }
  return na && nb ? dot / (Math.sqrt(na) * Math.sqrt(nb)) : 0;
}

test("embed: returns a fixed-dim float vector", async (t) => {
  if (skipNetwork()) return t.skip("FATHOMDB_SKIP_NETWORK_TESTS set");
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: true });
  try {
    const vec = await engine.embed("influenza vaccine clinical trial");
    assert.ok(Array.isArray(vec));
    assert.equal(vec.length, DIM);
    assert.ok(vec.every((x) => typeof x === "number"));
    assert.ok(vec.some((x) => x !== 0)); // not a zero vector
  } finally {
    await engine.close();
  }
});

test("embed: is deterministic", async (t) => {
  if (skipNetwork()) return t.skip("FATHOMDB_SKIP_NETWORK_TESTS set");
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: true });
  try {
    const a = await engine.embed("the central bank raised interest rates");
    const b = await engine.embed("the central bank raised interest rates");
    assert.deepEqual(a, b);
  } finally {
    await engine.close();
  }
});

test("embed: is semantic (paraphrase closer than unrelated)", async (t) => {
  if (skipNetwork()) return t.skip("FATHOMDB_SKIP_NETWORK_TESTS set");
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: true });
  try {
    const fluA = await engine.embed("a new influenza vaccine showed progress in trials");
    const fluB = await engine.embed("researchers report advances in a flu immunization candidate");
    const bank = await engine.embed("the treasury yield curve inverted amid inflation fears");
    assert.ok(cosine(fluA, fluB) > cosine(fluA, bank));
  } finally {
    await engine.close();
  }
});

test("embed: without an embedder rejects EmbedderNotConfiguredError", async () => {
  // The read-path primitive fails closed, same contract as the vector-write path.
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    await assert.rejects(() => engine.embed("anything"), EmbedderNotConfiguredError);
  } finally {
    await engine.close();
  }
});

test("embed: cross-binding golden anchor (Py ≡ TS)", async (t) => {
  if (skipNetwork()) return t.skip("FATHOMDB_SKIP_NETWORK_TESTS set");
  const golden = loadGolden();
  assert.equal(golden.dim, DIM);
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: true });
  try {
    const vec = await engine.embed(golden.anchor_text);
    assert.equal(vec.length, golden.vector.length);
    let maxAbsDiff = 0;
    for (let i = 0; i < vec.length; i++) {
      maxAbsDiff = Math.max(maxAbsDiff, Math.abs(vec[i] - golden.vector[i]));
    }
    assert.ok(
      maxAbsDiff <= golden.tolerance,
      `embed(anchor) must match the committed golden within ${golden.tolerance}; max abs diff was ${maxAbsDiff}`,
    );
  } finally {
    await engine.close();
  }
});
