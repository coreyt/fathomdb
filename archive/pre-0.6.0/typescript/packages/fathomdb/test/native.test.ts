import { describe, expect, it } from "vitest";
import { candidatePaths } from "../src/native.js";

describe("candidatePaths", () => {
  it("includes production paths that do not use relative ../../ patterns", () => {
    const paths = candidatePaths();
    const productionPaths = paths.filter(
      (p) => p.startsWith("/") && !p.includes("../../")
    );
    expect(productionPaths.length).toBeGreaterThanOrEqual(1);
  });

  it("includes a platform-specific path", () => {
    const paths = candidatePaths();
    const platformPath = paths.find(
      (p) => p.includes(`fathomdb.${process.platform}-${process.arch}.node`)
    );
    expect(platformPath).toBeDefined();
  });

  it("still includes development repo-local paths", () => {
    const paths = candidatePaths();
    const devPaths = paths.filter((p) => p.includes("../../"));
    expect(devPaths.length).toBeGreaterThanOrEqual(1);
  });
});
