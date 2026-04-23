// Pack H.1 follow-up Part B: configureEmbedding / configureVecKind TS napi
// wrappers. Exercises wire-level camelCase → snake_case translation,
// outcome envelope shape, and FFI error propagation.

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { openTempEngine, type TempEngine } from "./helpers/engine.js";

describe("admin.configureEmbedding", () => {
  let ctx: TempEngine;
  beforeEach(() => {
    ctx = openTempEngine();
  });
  afterEach(() => ctx.cleanup());

  it("activates a fresh profile and returns an outcome envelope", () => {
    const outcome = ctx.engine.admin.configureEmbedding({
      modelIdentity: "test-model",
      modelVersion: "1",
      dimensions: 4,
      normalizationPolicy: "none",
      maxTokens: 512,
      acknowledgeRebuildImpact: false,
    });
    expect(["activated", "unchanged", "replaced"]).toContain(outcome.outcome);
  });

  it("rejects an identity change without acknowledgeRebuildImpact when enabled kinds exist", () => {
    // First: activate a profile and enable a kind.
    ctx.engine.admin.configureEmbedding({
      modelIdentity: "test-model",
      modelVersion: "1",
      dimensions: 4,
      normalizationPolicy: "none",
      maxTokens: 512,
      acknowledgeRebuildImpact: false,
    });
    ctx.engine.admin.configureVecKind({ kind: "KnowledgeItem", source: "chunks" });

    // Now swap identity without ack — must throw EmbeddingChangeRequiresAck.
    expect(() =>
      ctx.engine.admin.configureEmbedding({
        modelIdentity: "other-model",
        modelVersion: "2",
        dimensions: 4,
        normalizationPolicy: "none",
        maxTokens: 512,
        acknowledgeRebuildImpact: false,
      }),
    ).toThrow(/acknowledge_rebuild_impact|EMBEDDING_CHANGE_REQUIRES_ACK/i);
  });
});

describe("admin.configureVecKind", () => {
  let ctx: TempEngine;
  beforeEach(() => {
    ctx = openTempEngine();
  });
  afterEach(() => ctx.cleanup());

  it("returns a ConfigureVecOutcome for a chunks source after profile activation", () => {
    ctx.engine.admin.configureEmbedding({
      modelIdentity: "test-model",
      modelVersion: "1",
      dimensions: 4,
      normalizationPolicy: "none",
      maxTokens: 512,
      acknowledgeRebuildImpact: false,
    });
    const outcome = ctx.engine.admin.configureVecKind({
      kind: "KnowledgeItem",
      source: "chunks",
    });
    expect(outcome.kind).toBe("KnowledgeItem");
    expect(typeof outcome.enqueued_backfill_rows).toBe("number");
    expect(typeof outcome.was_already_enabled).toBe("boolean");
  });

  it("rejects an unsupported source", () => {
    // Cast through unknown — `"bogus"` is not in the type's literal union,
    // but we want to verify the FFI path surfaces the error cleanly.
    expect(() =>
      ctx.engine.admin.configureVecKind({
        kind: "K",
        source: "bogus" as unknown as "chunks",
      }),
    ).toThrow();
  });
});
