import { tmpdir } from "node:os";
import { describe, expect, it } from "vitest";
import { tempDbPath } from "../src/skip.js";

describe("tempDbPath", () => {
  it("returns a path under os.tmpdir()", () => {
    const path = tempDbPath("foo");
    expect(path.startsWith(tmpdir())).toBe(true);
    expect(path).toMatch(/fathomdb-harness-foo-/);
    expect(path.endsWith(".db")).toBe(true);
  });
});
