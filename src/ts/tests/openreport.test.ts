// AC-068d — `engine.openReport()` surfaces the native `OpenReport`.
//
// Shape D (locked HITL 2026-05-24): the report is exposed as an
// engine-attached accessor, not a return-shape change on `Engine.open`.
// `engine.openReport()` is sync — the report is a snapshot captured at
// open time and stashed on the napi engine struct; repeat calls return
// identical data.
//
// Spec refs:
// - `dev/design/engine.md` § "`Engine.open` success result" — locked
//   field subset (`schemaVersionBefore`, `schemaVersionAfter`,
//   `migrationSteps`, `embedderWarmupMs`).
// - Native struct: `src/rust/crates/fathomdb-engine/src/lib.rs:541-548`
//   carries two additional fields (`queryBackend`, `defaultEmbedder`).
// - `dev/interfaces/typescript.md` Engine-attached instrumentation list.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

test("openReport returns the spec-locked native fields", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const report = engine.openReport();

    assert.equal(typeof report.schemaVersionBefore, "number");
    assert.equal(typeof report.schemaVersionAfter, "number");
    assert.ok(report.schemaVersionAfter >= report.schemaVersionBefore);
    assert.ok(Array.isArray(report.migrationSteps));
    assert.equal(typeof report.embedderWarmupMs, "number");
    assert.ok(report.embedderWarmupMs >= 0);
    assert.equal(typeof report.queryBackend, "string");
    assert.ok(report.queryBackend.length > 0);

    const id = report.defaultEmbedder;
    assert.equal(typeof id.name, "string");
    assert.ok(id.name.length > 0);
    assert.equal(typeof id.revision, "string");
    assert.ok(id.revision.length > 0);
    assert.equal(typeof id.dimension, "number");
    assert.ok(id.dimension > 0);
  } finally {
    await engine.close();
  }
});

test("openReport is idempotent — repeat calls return identical data", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const first = engine.openReport();
    const second = engine.openReport();

    assert.equal(first.schemaVersionBefore, second.schemaVersionBefore);
    assert.equal(first.schemaVersionAfter, second.schemaVersionAfter);
    assert.equal(first.embedderWarmupMs, second.embedderWarmupMs);
    assert.equal(first.queryBackend, second.queryBackend);
    assert.equal(first.defaultEmbedder.name, second.defaultEmbedder.name);
    assert.equal(first.defaultEmbedder.revision, second.defaultEmbedder.revision);
    assert.equal(first.defaultEmbedder.dimension, second.defaultEmbedder.dimension);

    assert.equal(first.migrationSteps.length, second.migrationSteps.length);
    for (let i = 0; i < first.migrationSteps.length; i += 1) {
      const a = first.migrationSteps[i];
      const b = second.migrationSteps[i];
      assert.equal(a.stepId, b.stepId);
      assert.equal(a.durationMs, b.durationMs);
      assert.equal(a.failed, b.failed);
    }
  } finally {
    await engine.close();
  }
});

test("Engine.open Promise signature unchanged — resolves to just Engine", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    assert.ok(engine instanceof Engine);
  } finally {
    await engine.close();
  }
});
