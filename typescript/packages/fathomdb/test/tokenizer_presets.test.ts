// ARCH-006: TOKENIZER_PRESETS is computed from the Rust constant via FFI.
//
// The TypeScript SDK must NOT hand-maintain a copy of the preset dict; it
// is populated at module load time from
// `native.listTokenizerPresets()`.

import { describe, expect, it } from "vitest";

import { TOKENIZER_PRESETS } from "../src/index.js";

describe("TOKENIZER_PRESETS (ARCH-006)", () => {
  it("exposes the five well-known presets from the Rust constant", () => {
    expect(Object.keys(TOKENIZER_PRESETS).sort()).toEqual(
      [
        "global-cjk",
        "precision-optimized",
        "recall-optimized-english",
        "source-code",
        "substring-trigram",
      ].sort(),
    );
  });

  it("returns a plain string→string record", () => {
    for (const [name, value] of Object.entries(TOKENIZER_PRESETS)) {
      expect(typeof name).toBe("string");
      expect(typeof value).toBe("string");
    }
  });

  it("matches the fixed values currently shipped by the engine", () => {
    expect(TOKENIZER_PRESETS["recall-optimized-english"]).toBe(
      "porter unicode61 remove_diacritics 2",
    );
    expect(TOKENIZER_PRESETS["precision-optimized"]).toBe(
      "unicode61 remove_diacritics 2",
    );
    expect(TOKENIZER_PRESETS["global-cjk"]).toBe("icu");
    expect(TOKENIZER_PRESETS["substring-trigram"]).toBe("trigram");
    expect(TOKENIZER_PRESETS["source-code"]).toBe(
      "unicode61 tokenchars '._-$@'",
    );
  });
});
