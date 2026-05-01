// Parity fix tests: G-1 (SearchBuilder.limit), G-3 (tokenizer presets), G-4 (WriterTimedOutError)
// RED phase: all three groups should fail before the implementation changes.

import { describe, expect, it } from "vitest";
import { mapNativeError, WriterTimedOutError } from "../src/errors.js";
import { TOKENIZER_PRESETS } from "../src/admin.js";
import { SearchBuilder } from "../src/query.js";

// ---------------------------------------------------------------------------
// G-4: WriterTimedOutError mapping
// ---------------------------------------------------------------------------
describe("G-4: WriterTimedOutError", () => {
  it("mapNativeError maps FATHOMDB_WRITER_TIMED_OUT:: to WriterTimedOutError", () => {
    const raw = new Error("FATHOMDB_WRITER_TIMED_OUT::msg");
    const mapped = mapNativeError(raw);
    expect(mapped).toBeInstanceOf(WriterTimedOutError);
    expect(mapped.message).toBe("msg");
  });
});

// ---------------------------------------------------------------------------
// G-3: TOKENIZER_PRESETS constant
// ---------------------------------------------------------------------------
describe("G-3: TOKENIZER_PRESETS", () => {
  it("exports a TOKENIZER_PRESETS map", () => {
    expect(TOKENIZER_PRESETS).toBeDefined();
    expect(typeof TOKENIZER_PRESETS).toBe("object");
  });

  it("recall-optimized-english resolves to porter tokenizer string", () => {
    expect(TOKENIZER_PRESETS["recall-optimized-english"]).toBe("porter unicode61 remove_diacritics 2");
  });

  it("precision-optimized resolves correctly", () => {
    expect(TOKENIZER_PRESETS["precision-optimized"]).toBe("unicode61 remove_diacritics 2");
  });

  it("global-cjk resolves correctly", () => {
    expect(TOKENIZER_PRESETS["global-cjk"]).toBe("icu");
  });

  it("substring-trigram resolves correctly", () => {
    expect(TOKENIZER_PRESETS["substring-trigram"]).toBe("trigram");
  });

  it("source-code resolves correctly", () => {
    expect(TOKENIZER_PRESETS["source-code"]).toBe("unicode61 tokenchars '._-$@'");
  });

  it("unknown preset passes through unchanged", () => {
    expect(TOKENIZER_PRESETS["custom-tokenizer"]).toBeUndefined();
    // Passthrough logic: undefined means use the original string as-is
    const resolved = TOKENIZER_PRESETS["custom-tokenizer"] ?? "custom-tokenizer";
    expect(resolved).toBe("custom-tokenizer");
  });
});

// ---------------------------------------------------------------------------
// G-1: SearchBuilder.limit() method
// ---------------------------------------------------------------------------
describe("G-1: SearchBuilder.limit()", () => {
  it("SearchBuilder.limit() returns a SearchBuilder", () => {
    // We need a minimal NativeEngineCore-like object to construct SearchBuilder
    // The easiest approach: use the internal #searchAstJson via executeGrouped spy.
    // But since #searchAstJson is private, we test via toAst-equivalent.
    // Instead, expose via compileGrouped with a mock core that captures the input.
    let capturedAst: Record<string, unknown> | null = null;
    const mockCore = {
      compileGroupedAst: (json: string) => {
        capturedAst = JSON.parse(json) as Record<string, unknown>;
        // Return minimal valid wire response for compiledGroupedQueryFromWire
        return JSON.stringify({
          sql: "SELECT 1",
          bindings: [],
          root_kind: "Test",
          expansion_slots: [],
        });
      },
      describeFtsPropertySchema: () => "null",
      getFtsPropertySchemaForKind: () => "null",
    };

    // Construct builder with our mock core
    // SearchBuilder constructor: (core, rootKind, strictQuery, limit, filters?, attributed?, expansions?, expandLimit?)
    // We'll call compileGrouped which calls #searchAstJson internally
    const builder = new (SearchBuilder as new (
      core: unknown,
      rootKind: string,
      strictQuery: string,
      limit: number,
    ) => SearchBuilder)(mockCore, "Test", "query", 10);

    const limited = builder.limit(5);
    expect(limited).toBeInstanceOf(SearchBuilder);

    // Trigger AST capture by calling compileGrouped
    limited.compileGrouped();
    expect(capturedAst).not.toBeNull();
    expect((capturedAst as unknown as Record<string, unknown>).final_limit).toBe(5);
  });

  it("SearchBuilder without limit() has final_limit null", () => {
    let capturedAst: Record<string, unknown> | null = null;
    const mockCore = {
      compileGroupedAst: (json: string) => {
        capturedAst = JSON.parse(json) as Record<string, unknown>;
        return JSON.stringify({
          sql: "SELECT 1",
          bindings: [],
          root_kind: "Test",
          expansion_slots: [],
        });
      },
      describeFtsPropertySchema: () => "null",
      getFtsPropertySchemaForKind: () => "null",
    };

    const builder = new (SearchBuilder as new (
      core: unknown,
      rootKind: string,
      strictQuery: string,
      limit: number,
    ) => SearchBuilder)(mockCore, "Test", "query", 10);

    builder.compileGrouped();
    expect(capturedAst).not.toBeNull();
    expect((capturedAst as unknown as Record<string, unknown>).final_limit).toBeNull();
  });
});
