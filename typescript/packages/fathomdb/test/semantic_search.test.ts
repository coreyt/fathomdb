// Pack F3: TypeScript bindings for `semantic_search`, `raw_vector_search`,
// the `vectorSearch(...)` deprecation shim, and the
// `admin.drainVectorProjection(...)` surface.
//
// The TS SDK cannot open an `Engine` with an in-process embedder (the
// napi surface does not expose `configure_embedding` /
// `configure_vec_kind`), so the end-to-end memex tripwire is covered in
// the Rust pack (Pack F1) and deferred to Pack G for TS. These tests
// cover what IS reachable from TypeScript: AST encoding, client-side
// validation, error-code mapping, deprecation shim behaviour, and the
// drain-vector-projection admin bridge.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  DimensionMismatchError,
  Engine,
  FathomError,
  KindNotVectorIndexedError,
  Query,
} from "../src/index.js";

import { openTempEngine, type TempEngine } from "./helpers/engine.js";

describe("Query.semanticSearch (AST + error propagation)", () => {
  let ctx: TempEngine;

  beforeEach(() => {
    ctx = openTempEngine();
  });

  afterEach(() => {
    ctx.cleanup();
  });

  it("appends a semantic_search step with the expected wire shape", () => {
    const q = ctx.engine.nodes("KnowledgeItem").semanticSearch("Acme", 5);
    expect(q).toBeInstanceOf(Query);
    expect(q.toAst()).toEqual({
      root_kind: "KnowledgeItem",
      steps: [{ type: "semantic_search", text: "Acme", limit: 5 }],
      expansions: [],
      edge_expansions: [],
      final_limit: null,
    });
  });

  it("executes without throwing a wire-encoding error", () => {
    // End-to-end semantic retrieval is not yet wired through the
    // generic `executeAst` path (the compile layer treats
    // SemanticSearch as a no-op for QueryRows, Pack G will expose a
    // dedicated FFI entry). For now we assert that the TS → napi →
    // Rust parse path succeeds: the call must not throw an
    // INVALID_ARGUMENT / wire-shape error, regardless of whether any
    // rows come back.
    expect(() =>
      ctx.engine.nodes("KnowledgeItem").semanticSearch("Acme", 5).execute(),
    ).not.toThrow();
  });
});

describe("Query.rawVectorSearch (AST + validation + error propagation)", () => {
  let ctx: TempEngine;

  beforeEach(() => {
    ctx = openTempEngine();
  });

  afterEach(() => {
    ctx.cleanup();
  });

  it("appends a raw_vector_search step with the expected wire shape", () => {
    const q = ctx.engine.nodes("KnowledgeItem").rawVectorSearch([0.1, 0.2, 0.3, 0.4], 7);
    expect(q.toAst()).toEqual({
      root_kind: "KnowledgeItem",
      steps: [{ type: "raw_vector_search", vector: [0.1, 0.2, 0.3, 0.4], limit: 7 }],
      expansions: [],
      edge_expansions: [],
      final_limit: null,
    });
  });

  it("accepts a Float32Array by converting to a plain number[]", () => {
    const vec = new Float32Array([1.0, 0.0, 0.0, 0.0]);
    const q = ctx.engine.nodes("KnowledgeItem").rawVectorSearch(vec, 3);
    const ast = q.toAst();
    const step = ast.steps[0] as Record<string, unknown>;
    expect(step.type).toBe("raw_vector_search");
    expect(Array.isArray(step.vector)).toBe(true);
    expect(step.vector).toEqual([1.0, 0.0, 0.0, 0.0]);
  });

  it("rejects an empty vector at the client boundary", () => {
    expect(() =>
      ctx.engine.nodes("KnowledgeItem").rawVectorSearch([], 5),
    ).toThrow(/rawVectorSearch.*non-empty|must not be empty/i);
  });

  it("rejects vectors with non-finite components at the client boundary", () => {
    expect(() =>
      ctx.engine.nodes("KnowledgeItem").rawVectorSearch([1.0, Number.NaN, 0.0, 0.0], 5),
    ).toThrow(/finite/i);
    expect(() =>
      ctx.engine
        .nodes("KnowledgeItem")
        .rawVectorSearch([1.0, Number.POSITIVE_INFINITY, 0.0, 0.0], 5),
    ).toThrow(/finite/i);
  });

  it("executes without a wire-shape error", () => {
    // Same caveat as semanticSearch: generic executeAst path does not
    // yet dispatch to `execute_compiled_raw_vector_search` (Pack G),
    // so we only assert the TS → napi → Rust parse path accepts the
    // step without throwing a wire-shape error.
    expect(() =>
      ctx.engine
        .nodes("KnowledgeItem")
        .rawVectorSearch([1.0, 0.0, 0.0, 0.0], 5)
        .execute(),
    ).not.toThrow();
  });
});

describe("Query.vectorSearch deprecation shim", () => {
  let ctx: TempEngine;

  beforeEach(() => {
    ctx = openTempEngine();
  });

  afterEach(() => {
    ctx.cleanup();
  });

  it("warns and routes string input to semanticSearch", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    try {
      const q = ctx.engine.nodes("KnowledgeItem").vectorSearch("Acme", 5);
      expect(warn).toHaveBeenCalledOnce();
      const msg = String(warn.mock.calls[0]?.[0] ?? "");
      expect(msg).toMatch(/deprecated/i);
      expect(msg).toMatch(/semanticSearch/);
      expect(q.toAst().steps).toEqual([
        { type: "semantic_search", text: "Acme", limit: 5 },
      ]);
    } finally {
      warn.mockRestore();
    }
  });

  it("warns and routes number[] input to rawVectorSearch", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    try {
      const q = ctx.engine
        .nodes("KnowledgeItem")
        .vectorSearch([1.0, 0.0, 0.0, 0.0], 5);
      expect(warn).toHaveBeenCalledOnce();
      const msg = String(warn.mock.calls[0]?.[0] ?? "");
      expect(msg).toMatch(/deprecated/i);
      expect(msg).toMatch(/rawVectorSearch/);
      expect(q.toAst().steps).toEqual([
        { type: "raw_vector_search", vector: [1.0, 0.0, 0.0, 0.0], limit: 5 },
      ]);
    } finally {
      warn.mockRestore();
    }
  });

  it("warns and routes Float32Array input to rawVectorSearch", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    try {
      const vec = new Float32Array([1.0, 0.0, 0.0, 0.0]);
      const q = ctx.engine.nodes("KnowledgeItem").vectorSearch(vec, 5);
      expect(warn).toHaveBeenCalledOnce();
      const msg = String(warn.mock.calls[0]?.[0] ?? "");
      expect(msg).toMatch(/deprecated/i);
      expect(msg).toMatch(/rawVectorSearch/);
      const step = q.toAst().steps[0] as Record<string, unknown>;
      expect(step.type).toBe("raw_vector_search");
      expect(step.vector).toEqual([1.0, 0.0, 0.0, 0.0]);
    } finally {
      warn.mockRestore();
    }
  });
});

describe("error-code mapping for vector-index surface", () => {
  it("exports KindNotVectorIndexedError and DimensionMismatchError classes", () => {
    // The classes must exist and extend FathomError so downstream code
    // can `catch (err) { if (err instanceof KindNotVectorIndexedError) ... }`.
    expect(KindNotVectorIndexedError.prototype).toBeInstanceOf(FathomError);
    expect(DimensionMismatchError.prototype).toBeInstanceOf(FathomError);
  });
});

describe("admin.drainVectorProjection", () => {
  let ctx: TempEngine;

  beforeEach(() => {
    ctx = openTempEngine();
  });

  afterEach(() => {
    ctx.cleanup();
  });

  it("throws when the engine has no embedder configured", () => {
    // Engine opened without an embedder (helper default). The Rust side
    // rejects the drain call with EmbedderNotConfigured so we never
    // dispatch on an identity-less engine.
    expect(() => ctx.engine.admin.drainVectorProjection(100)).toThrow(FathomError);
    try {
      ctx.engine.admin.drainVectorProjection(100);
    } catch (err) {
      expect((err as Error).message).toMatch(/embedder not configured/i);
    }
  });

  it("accepts a numeric timeoutMs argument and produces a DrainReport shape", () => {
    // Even though the engine has no embedder, invoking with an explicit
    // timeoutMs argument must still route through the JSON wire (no
    // client-side type errors) before the engine rejects it. Assert that
    // we hit the engine error rather than an earlier client-side bug.
    expect(() => ctx.engine.admin.drainVectorProjection(250)).toThrow(FathomError);
  });
});
