import { describe, it, expect } from "vitest";
import type { QueryEmbedder } from "../../src/embedders/index.js";

describe("QueryEmbedder interface", () => {
  it("can be satisfied by a minimal object", () => {
    const minimal: QueryEmbedder = {
      embed: async (_texts: string[]) => [[0.1, 0.2]],
      identity: () => "test/model/2",
      maxTokens: () => 512,
    };

    expect(minimal.identity()).toBe("test/model/2");
    expect(minimal.maxTokens()).toBe(512);
  });
});
