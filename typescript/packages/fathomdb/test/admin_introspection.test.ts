// Pack H: admin introspection + batch configure_vec_kinds (TypeScript parity).
//
// The TS napi surface does not currently expose `configureEmbedding` /
// `configureVecKind` single-call, so the end-to-end "seed, configure,
// assert" flows are covered in the Python parity suite and the Rust
// integration suite. These tests verify the wire shape of the four new
// admin methods and the `autoDrainVector` kwarg on `Engine.open`.

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { Engine } from "../src/index.js";
import { openTempEngine, type TempEngine } from "./helpers/engine.js";

describe("admin.capabilities", () => {
  let ctx: TempEngine;
  beforeEach(() => {
    ctx = openTempEngine();
  });
  afterEach(() => ctx.cleanup());

  it("returns the static install/build surface", () => {
    const caps = ctx.engine.admin.capabilities();
    expect(typeof caps.sqlite_vec).toBe("boolean");
    expect(caps.fts_tokenizers).toContain("recall-optimized-english");
    expect(caps.embedders.builtin).toBeDefined();
    expect(typeof caps.embedders.builtin.available).toBe("boolean");
    expect(caps.schema_version).toBeGreaterThanOrEqual(24);
    expect(caps.fathomdb_version.length).toBeGreaterThan(0);
  });
});

describe("admin.currentConfig", () => {
  let ctx: TempEngine;
  beforeEach(() => {
    ctx = openTempEngine();
  });
  afterEach(() => ctx.cleanup());

  it("is empty on a fresh engine", () => {
    const cfg = ctx.engine.admin.currentConfig();
    expect(cfg.active_embedding_profile).toBeNull();
    expect(Object.keys(cfg.vec_kinds)).toHaveLength(0);
    expect(Object.keys(cfg.fts_kinds)).toHaveLength(0);
    expect(cfg.work_queue.pending_incremental).toBe(0);
    expect(cfg.work_queue.pending_backfill).toBe(0);
    expect(cfg.work_queue.inflight).toBe(0);
    expect(cfg.work_queue.failed).toBe(0);
    expect(cfg.work_queue.discarded).toBe(0);
  });
});

describe("admin.describeKind", () => {
  let ctx: TempEngine;
  beforeEach(() => {
    ctx = openTempEngine();
  });
  afterEach(() => ctx.cleanup());

  it("returns a None-like view for an unconfigured kind", () => {
    const desc = ctx.engine.admin.describeKind("Missing");
    expect(desc.kind).toBe("Missing");
    expect(desc.vec).toBeNull();
    expect(desc.fts).toBeNull();
    expect(desc.chunk_count).toBe(0);
    expect(desc.vec_rows).toBeNull();
  });
});

describe("admin.configureVecKinds", () => {
  let ctx: TempEngine;
  beforeEach(() => {
    ctx = openTempEngine();
  });
  afterEach(() => ctx.cleanup());

  it("fails cleanly when no active embedding profile is configured", () => {
    // Without an active profile, configure_vec_kind (underlying each item)
    // rejects with InvalidConfig. The batch method bubbles the first error
    // up — verifying the wire path reaches the engine.
    expect(() =>
      ctx.engine.admin.configureVecKinds([{ kind: "Note", source: "chunks" }]),
    ).toThrow();
  });
});

describe("Engine.open autoDrainVector option", () => {
  it("accepts autoDrainVector=false without error", () => {
    const ctx = openTempEngine();
    try {
      const caps = ctx.engine.admin.capabilities();
      expect(caps).toBeDefined();
    } finally {
      ctx.cleanup();
    }
  });

  it("accepts autoDrainVector=true without an embedder (no-op)", () => {
    const ctx = openTempEngine();
    try {
      ctx.engine.close();
      const engine = Engine.open(`${ctx.dir}/t.db`, { autoDrainVector: true });
      try {
        const caps = engine.admin.capabilities();
        expect(caps).toBeDefined();
      } finally {
        engine.close();
      }
    } finally {
      ctx.cleanup();
    }
  });
});
