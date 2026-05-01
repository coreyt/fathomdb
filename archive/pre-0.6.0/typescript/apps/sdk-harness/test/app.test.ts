import { copyFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { runHarness } from "../src/app.js";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../../..");

const nativeSourceCandidates = [
  resolve(repoRoot, "target/debug/libfathomdb.so"),
  resolve(repoRoot, "target/debug/libfathomdb.dylib"),
  resolve(repoRoot, "target/release/libfathomdb.so"),
  resolve(repoRoot, "target/release/libfathomdb.dylib"),
];

function withNativeBinding<T>(run: () => T): T {
  const tempDir = mkdtempSync(join(tmpdir(), "fathomdb-sdk-harness-"));
  const nativeBindingPath = join(tempDir, "fathomdb.node");
  const nativeSource = nativeSourceCandidates.find((candidate) => existsSync(candidate));
  if (!nativeSource) {
    throw new Error(
      "Missing native binding build output. Run `cargo build -p fathomdb --features node` before the SDK harness tests."
    );
  }
  copyFileSync(nativeSource, nativeBindingPath);
  const previousBinding = process.env.FATHOMDB_NATIVE_BINDING;
  process.env.FATHOMDB_NATIVE_BINDING = nativeBindingPath;
  try {
    return run();
  } finally {
    if (previousBinding === undefined) {
      delete process.env.FATHOMDB_NATIVE_BINDING;
    } else {
      process.env.FATHOMDB_NATIVE_BINDING = previousBinding;
    }
    rmSync(tempDir, { recursive: true, force: true });
  }
}

describe("sdk harness", () => {
  it("runs the baseline scenarios", () => {
    const result = runHarness("baseline");
    console.log(result);
    expect(result).toMatch(/^13\/13 scenarios passed/);
  });

  it("runs the vector scenarios", () => {
    const result = runHarness("vector");
    console.log(result);
    expect(result).toMatch(/^14\/14 scenarios passed/);
  });

  it("runs the observability telemetry scenario", () => {
    const result = withNativeBinding(() => runHarness("observability"));
    console.log(result);
    expect(result).toMatch(/^2\/2 scenarios passed/);
  });

  // Run stress as a truly async subprocess so the vitest worker event loop
  // stays unblocked. spawnSync / direct runHarness() both block for ~2 min,
  // causing vitest's IPC onTaskUpdate call to time out even though tests pass.
  it("runs the stress scenarios", { timeout: 400_000 }, () => {
    const distApp = resolve(here, "../dist/app.js");
    const nativeSource = nativeSourceCandidates.find((c) => existsSync(c));
    if (!nativeSource) {
      throw new Error(
        "Missing native binding. Run `cargo build -p fathomdb --features node` first."
      );
    }
    const tempDir = mkdtempSync(join(tmpdir(), "fathomdb-sdk-harness-stress-"));
    const nativeBindingPath = join(tempDir, "fathomdb.node");
    copyFileSync(nativeSource, nativeBindingPath);

    return new Promise<void>((resolve, reject) => {
      const proc = spawn(process.execPath, [distApp, "stress"], {
        env: { ...process.env, FATHOMDB_NATIVE_BINDING: nativeBindingPath },
      });
      let output = "";
      proc.stdout?.on("data", (chunk: Buffer) => { output += chunk.toString(); });
      proc.stderr?.on("data", (chunk: Buffer) => { output += chunk.toString(); });
      proc.on("close", (code: number | null) => {
        rmSync(tempDir, { recursive: true, force: true });
        if (code !== 0) {
          reject(new Error(`stress harness exited ${String(code)}: ${output}`));
        } else {
          console.log(output);
          try {
            expect(output).toMatch(/^3\/3 scenarios passed/);
            resolve();
          } catch (e) {
            reject(e as Error);
          }
        }
      });
      proc.on("error", (err: Error) => {
        rmSync(tempDir, { recursive: true, force: true });
        reject(err);
      });
    });
  });
});
