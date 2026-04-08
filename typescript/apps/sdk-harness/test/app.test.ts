import { describe, expect, it } from "vitest";
import { runHarness } from "../src/app.js";

describe("sdk harness", () => {
  it("runs the baseline scenarios", () => {
    const result = runHarness("baseline");
    expect(result).toMatch(/^4\/4 scenarios passed/);
  });

  it("runs the vector scenarios", () => {
    const result = runHarness("vector");
    expect(result).toMatch(/^5\/5 scenarios passed/);
  });
});
