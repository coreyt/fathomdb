import { afterEach, describe, expect, it } from "vitest";
import { type RebuildProgress } from "../src/index.js";
import { openTempEngine, type TempEngine } from "./helpers/engine.js";

describe("async FTS rebuild", () => {
  let ctx: TempEngine;

  afterEach(() => {
    ctx?.cleanup();
  });

  it("registerFtsPropertySchemaAsync returns an FtsPropertySchemaRecord", () => {
    ctx = openTempEngine();
    const record = ctx.engine.admin.registerFtsPropertySchemaAsync("Kind", ["$.name"]);
    expect(record).toBeDefined();
    expect(record.kind).toBe("Kind");
    expect(record.propertyPaths).toContain("$.name");
  });

  it("getRebuildProgress returns a progress object after async registration", () => {
    ctx = openTempEngine();
    ctx.engine.admin.registerFtsPropertySchemaAsync("Thing", ["$.title"]);
    const progress = ctx.engine.admin.getRebuildProgress("Thing");
    expect(progress).not.toBeNull();
    expect(progress).toBeDefined();
    const p = progress as RebuildProgress;
    expect(["BUILDING", "COMPLETE", "FAILED"]).toContain(p.state);
  });

  it("rebuild eventually reaches COMPLETE state", async () => {
    ctx = openTempEngine();
    ctx.engine.admin.registerFtsPropertySchemaAsync("Item", ["$.body"]);

    const deadline = Date.now() + 10_000;
    let progress: RebuildProgress | null = null;
    while (Date.now() < deadline) {
      progress = ctx.engine.admin.getRebuildProgress("Item");
      if (progress?.state === "COMPLETE") break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    expect(progress).not.toBeNull();
    expect(progress?.state).toBe("COMPLETE");
  });
});
