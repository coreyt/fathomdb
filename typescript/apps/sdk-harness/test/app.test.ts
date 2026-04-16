import { copyFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { runHarness } from "../src/app.js";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../../..");
function withNativeBinding<T>(run: () => T): T {
  const tempDir = mkdtempSync(join(tmpdir(), "fathomdb-sdk-harness-"));
  const nativeBindingPath = join(tempDir, "fathomdb.node");
  const nativeSourceCandidates = [
    resolve(repoRoot, "target/debug/libfathomdb.so"),
    resolve(repoRoot, "target/debug/libfathomdb.dylib"),
    resolve(repoRoot, "target/release/libfathomdb.so"),
    resolve(repoRoot, "target/release/libfathomdb.dylib")
  ];
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

  // Run stress scenarios as a subprocess so the vitest worker's event loop
  // stays unblocked during the long synchronous stress run. Calling
  // runHarness("stress") directly in-process blocks the worker for ~2-5
  // minutes, causing vitest's IPC onTaskUpdate call to time out even though
  // the test actually passed. The subprocess inherits FATHOMDB_NATIVE_BINDING
  // set by withNativeBinding.
  it("runs the stress scenarios", { timeout: 400_000 }, () => {
    const distApp = resolve(here, "../dist/app.js");
    withNativeBinding(() => {
      const r = spawnSync(process.execPath, [distApp, "stress"], {
        env: { ...process.env },
        encoding: "utf8",
        timeout: 380_000,
      });
      const output = r.stdout ?? "";
      console.log(output);
      if (r.status !== 0) {
        throw new Error(`stress harness exited ${r.status}: ${r.stderr ?? output}`);
      }
      expect(output).toMatch(/^3\/3 scenarios passed/);
    });
  });
});
