import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { writeFileSync, mkdtempSync, rmSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { SubprocessEmbedder } from "../../src/embedders/subprocess.js";

// Shell script fixture that loops reading lines from stdin and echoes 3 float32 LE
// values per line: 1.0, 2.0, 3.0 in little-endian float32 using octal escapes.
// 1.0 = 0x3f800000 LE = \000\000\200\077
// 2.0 = 0x40000000 LE = \000\000\000\100
// 3.0 = 0x40400000 LE = \000\000\100\100
const FIXTURE_SH = `#!/bin/sh
while read -r line; do
  printf '\\000\\000\\200\\077\\000\\000\\000\\100\\000\\000\\100\\100'
done
`;

let tmpDir: string;
let fixturePath: string;
let embedder: SubprocessEmbedder;

beforeAll(() => {
  tmpDir = mkdtempSync(join(tmpdir(), "fathomdb-subprocess-test-"));
  fixturePath = join(tmpDir, "fixture.sh");
  writeFileSync(fixturePath, FIXTURE_SH, "utf-8");
  chmodSync(fixturePath, 0o755);

  embedder = new SubprocessEmbedder({
    command: ["sh", fixturePath],
    dimensions: 3,
  });
});

afterAll(() => {
  rmSync(tmpDir, { recursive: true, force: true });
});

describe("SubprocessEmbedder", () => {
  it("returns float32 vector from subprocess", async () => {
    const result = await embedder.embed(["any text"]);
    expect(result).toHaveLength(1);
    expect(result[0]).toHaveLength(3);
    // float32 precision: 1.0, 2.0, 3.0
    expect(result[0][0]).toBeCloseTo(1.0, 5);
    expect(result[0][1]).toBeCloseTo(2.0, 5);
    expect(result[0][2]).toBeCloseTo(3.0, 5);
  });

  it("identity() returns command joined by spaces", () => {
    expect(embedder.identity()).toBe(`sh ${fixturePath}`);
  });

  it("maxTokens() returns 512", () => {
    expect(embedder.maxTokens()).toBe(512);
  });

  it("handles multiple texts in sequence", async () => {
    // Each embed call invokes the subprocess once per text
    const r1 = await embedder.embed(["text one"]);
    const r2 = await embedder.embed(["text two"]);
    expect(r1[0][0]).toBeCloseTo(1.0, 5);
    expect(r2[0][0]).toBeCloseTo(1.0, 5);
  });
});
