// Surface assertions for the TypeScript SDK.
//
// Pins the five-verb top-level surface, the engine-attached instrumentation
// methods, the options.engineConfig camelCase knobs, the soft-fallback
// record shape, and the FathomDbError single-rooted hierarchy per
// `dev/interfaces/typescript.md` and `dev/design/bindings.md` § 3.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine, admin, type EngineConfig, type SoftFallback } from "../src/index.js";

test("Engine exposes five-verb shape plus instrumentation methods", async () => {
  assert.equal(typeof Engine.open, "function", "Engine.open must be a static method");

  const engine = await Engine.open("test.sqlite");
  for (const v of ["write", "search", "close"] as const) {
    assert.equal(
      typeof (engine as unknown as Record<string, unknown>)[v],
      "function",
      `engine must expose ${v}`,
    );
  }

  for (const m of [
    "drain",
    "counters",
    "setProfiling",
    "setSlowThresholdMs",
    "attachSubscriber",
  ] as const) {
    assert.equal(
      typeof (engine as unknown as Record<string, unknown>)[m],
      "function",
      `engine must expose ${m}`,
    );
  }
});

test("admin.configure is exported beside Engine", async () => {
  assert.equal(typeof admin.configure, "function");
  const engine = await Engine.open("test.sqlite");
  const receipt = await admin.configure(engine, { name: "default", body: "{}" });
  assert.equal(typeof receipt.cursor, "number");
});

test("Engine.open accepts engineConfig with camelCase knobs", async () => {
  const cfg: EngineConfig = {
    embedderPoolSize: 2,
    schedulerRuntimeThreads: 4,
    provenanceRowCap: 1024,
    embedderCallTimeoutMs: 30_000,
    slowThresholdMs: 250,
  };
  const engine = await Engine.open("test.sqlite", { engineConfig: cfg });
  assert.equal(engine.config.embedderPoolSize, 2);
  assert.equal(engine.config.schedulerRuntimeThreads, 4);
  assert.equal(engine.config.provenanceRowCap, 1024);
  assert.equal(engine.config.embedderCallTimeoutMs, 30_000);
  assert.equal(engine.config.slowThresholdMs, 250);
});

test("write returns a typed receipt with cursor", async () => {
  const engine = await Engine.open("test.sqlite");
  const receipt = await engine.write([{ kind: "doc" }]);
  assert.equal(typeof receipt.cursor, "number");
});

test("search returns soft-fallback null by default", async () => {
  const engine = await Engine.open("test.sqlite");
  const result = await engine.search("hello");
  assert.equal(result.softFallback, null);
  assert.equal(typeof result.projectionCursor, "number");
});

test("SoftFallback branch is the typed two-member union", () => {
  const v: SoftFallback = { branch: "vector" };
  const t: SoftFallback = { branch: "text" };
  assert.equal(v.branch, "vector");
  assert.equal(t.branch, "text");
});

test("instrumentation stubs return canonical types", async () => {
  const engine = await Engine.open("test.sqlite");
  await engine.drain(0);
  const snap = engine.counters();
  assert.ok(snap !== undefined);
  await engine.setProfiling(true);
  await engine.setSlowThresholdMs(100);
  engine.attachSubscriber(() => undefined, {});
});
