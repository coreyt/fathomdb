// EU-6 — TypeScript binding surface for `useDefaultEmbedder` + EU-5b
// `OpenReport` fields.
//
// Per `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` §EU-6, the
// TS binding exposes the EU-5b binary opt-in selector via
// `Engine.open(path, { useDefaultEmbedder: true })`:
//
// - `true`  → `EmbedderChoice::Default` (engine materialises the pinned
//   bge-small embedder via the EU-3 loader; weights fetched from HF on
//   first use).
// - `false` (default) → `EmbedderChoice::None` (no embedder; vector
//   writes fail with `EmbedderNotConfiguredError`).
//
// Also asserts that the four EU-5a1/5a2/5b `OpenReport` fields round-trip
// through the binding (camelCase per TS convention):
// - `embedderDownloadMs`
// - `embedderEvents`
// - `embedderMeanCenteringRequired`
// - `embedderMeanVecPinned`
//
// Network-hitting tests honour `FATHOMDB_SKIP_NETWORK_TESTS` per EU-5c.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import { native } from "../src/binding.js";
import { freshDbPath } from "./helpers.js";

// Threshold mirrors `fathomdb_engine::MEAN_VEC_PIN_THRESHOLD` (256).
const MEAN_VEC_PIN_THRESHOLD = 256;

function skipIfNoNetwork(): boolean {
  if (process.env.FATHOMDB_SKIP_NETWORK_TESTS) {
    console.log("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; skipping test");
    return true;
  }
  return false;
}

test("engineOpen with useDefaultEmbedder: true succeeds", async () => {
  if (skipIfNoNetwork()) return;
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: true });
  try {
    const report = engine.openReport();
    assert.equal(report.defaultEmbedder.name, "fathomdb-bge-small-en-v1.5");
    assert.equal(report.defaultEmbedder.dimension, 384);
  } finally {
    await engine.close();
  }
});

test("engineOpen with useDefaultEmbedder: false makes no network call", async () => {
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    const report = engine.openReport();
    assert.ok(report.embedderDownloadMs == null);
    for (const ev of report.embedderEvents) {
      assert.notEqual(ev.kind, "DefaultEmbedderDownload");
    }
  } finally {
    await engine.close();
  }
});

test("engineOpen with no useDefaultEmbedder option defaults to no-network", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const report = engine.openReport();
    assert.ok(report.embedderDownloadMs == null);
    for (const ev of report.embedderEvents) {
      assert.notEqual(ev.kind, "DefaultEmbedderDownload");
    }
  } finally {
    await engine.close();
  }
});

test("openReport carries embedder mean-centering booleans", async () => {
  // Workspace identity is bge-small (EU-5b lock-flip), so the static
  // capability flag is true regardless of whether the embedder is
  // materialised. Both the false and true option paths must surface
  // the field — that's the EU-6 binding-coverage point.
  const engineFalse = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    const reportFalse = engineFalse.openReport();
    assert.equal(typeof reportFalse.embedderMeanCenteringRequired, "boolean");
    assert.equal(reportFalse.embedderMeanCenteringRequired, true);
  } finally {
    await engineFalse.close();
  }

  if (skipIfNoNetwork()) return;

  const engineTrue = await Engine.open(freshDbPath(), { useDefaultEmbedder: true });
  try {
    const reportTrue = engineTrue.openReport();
    assert.equal(reportTrue.embedderMeanCenteringRequired, true);
  } finally {
    await engineTrue.close();
  }
});

test("openReport.embedderMeanVecPinned is false on fresh workspace", async () => {
  if (skipIfNoNetwork()) return;
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: true });
  try {
    const report = engine.openReport();
    assert.equal(report.embedderMeanVecPinned, false);
  } finally {
    await engine.close();
  }
});

test("openReport.embedderMeanVecPinned transitions after 256 writes", async () => {
  if (skipIfNoNetwork()) return;

  const path = freshDbPath();
  const engine = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    // Public TS surface does not yet expose typed vector writes; use the
    // `test-hooks`-gated native seam.
    const inner = (engine as unknown as { _native: unknown })._native as {
      configureVectorKindForTest: (kind: string) => Promise<void>;
      writeVectorForTest: (kind: string, text: string) => Promise<void>;
    };
    await inner.configureVectorKindForTest("doc");
    for (let i = 0; i < MEAN_VEC_PIN_THRESHOLD; i += 1) {
      await inner.writeVectorForTest("doc", `doc-${i}`);
    }
  } finally {
    await engine.close();
  }

  const engine2 = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    const report = engine2.openReport();
    assert.equal(report.embedderMeanVecPinned, true);
  } finally {
    await engine2.close();
  }
});

// Silence unused-import lint when network tests skip.
void native;
