// Phase 12.5c: parity tests for the `embedder` option on the
// TypeScript SDK's `Engine.open`. Mirrors python/tests/test_engine_embedder.py.
//
// The SDK surface accepts `undefined` (default), `"none"` (explicit
// opt-out), and `"builtin"` (the Phase 12.5b Candle-based default
// embedder, which silently falls back to no-embedder when the
// `default-embedder` feature is off). Any other string must raise
// `FathomError` at open time.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Engine, FathomError, type SearchRows } from "../src/index.js";

import { seedBudgetGoals } from "./helpers/engine.js";

describe("Engine.open embedder option", () => {
  let dir: string;
  let dbPath: string;
  let engine: Engine | null = null;

  beforeEach(() => {
    // Ensure no cached mock binding leaks from a prior test file.
    Engine.setBindingForTests(null);
    dir = mkdtempSync(join(tmpdir(), "fathomdb-ts-embedder-"));
    dbPath = join(dir, "t.db");
    engine = null;
  });

  afterEach(() => {
    if (engine) {
      try {
        engine.close();
      } catch {
        // swallow
      }
    }
    rmSync(dir, { recursive: true, force: true });
  });

  it("default embedder (undefined) leaves vector-hit-count zero", () => {
    engine = Engine.open(dbPath);
    seedBudgetGoals(engine);
    const rows: SearchRows = engine.query("Goal").search("budget", 10).execute();
    expect(rows.vectorHitCount).toBe(0);
  });

  it("explicit 'none' embedder leaves vector-hit-count zero", () => {
    engine = Engine.open(dbPath, { embedder: "none" });
    seedBudgetGoals(engine);
    const rows: SearchRows = engine.query("Goal").search("budget", 10).execute();
    expect(rows.vectorHitCount).toBe(0);
  });

  it("'builtin' embedder is accepted (falls back to no-embedder without feature)", () => {
    // Without the `default-embedder` feature, the Rust side resolves
    // Builtin to None silently. We assert the engine opens and search()
    // still reports vector_hit_count == 0; Phase 12.5b flips this.
    engine = Engine.open(dbPath, { embedder: "builtin" });
    seedBudgetGoals(engine);
    const rows: SearchRows = engine.query("Goal").search("budget", 10).execute();
    expect(rows.vectorHitCount).toBe(0);
  });

  it("invalid embedder value raises FathomError mentioning valid values", () => {
    expect(() =>
      // @ts-expect-error — intentionally passing an invalid runtime value
      Engine.open(dbPath, { embedder: "bogus" }),
    ).toThrow(FathomError);
    try {
      // @ts-expect-error — intentionally passing an invalid runtime value
      Engine.open(dbPath, { embedder: "bogus" });
    } catch (err) {
      const message = (err as Error).message;
      expect(message).toContain("none");
      expect(message).toContain("builtin");
    }
  });
});
