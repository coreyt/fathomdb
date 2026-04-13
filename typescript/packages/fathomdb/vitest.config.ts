import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

// Locate the freshly-built fathomdb native binding for the real-engine
// integration tests. The test suite expects `cargo build -p fathomdb
// --features node` to have already been run from the repo root, producing
// `target/debug/libfathomdb.so` (or the platform equivalent).
//
// We copy the raw cdylib into a stable location under this package and
// expose it to the test workers via the FATHOMDB_NATIVE_BINDING env var.
// `src/native.ts` consults that env var first when resolving the binding.
const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../..");
const candidateSources = [
  resolve(repoRoot, "target/debug/libfathomdb.so"),
  resolve(repoRoot, "target/debug/libfathomdb.dylib"),
  resolve(repoRoot, "target/release/libfathomdb.so"),
  resolve(repoRoot, "target/release/libfathomdb.dylib"),
];
const source = candidateSources.find((candidate) => existsSync(candidate));
if (!source) {
  throw new Error(
    "Missing native binding build output. Run `cargo build -p fathomdb --features node` from the repo root before running the TypeScript test suite.",
  );
}
const targetDir = resolve(here, "test/.native");
mkdirSync(targetDir, { recursive: true });
const targetPath = resolve(targetDir, "fathomdb.node");
copyFileSync(source, targetPath);

export default defineConfig({
  test: {
    env: {
      FATHOMDB_NATIVE_BINDING: targetPath,
    },
    // Real-engine setup (engine open + FTS schema registration + seed data)
    // adds a few hundred ms per test vs the old mocked path; bump the
    // default timeout to give tests headroom on slow CI machines.
    testTimeout: 30_000,
    // Real engines are tempdir-backed and isolated per test, but napi-rs
    // initializes a single shared tracing subscriber inside the .node file
    // and multi-process sharding doesn't play well with that. Keep the
    // test runner single-threaded to avoid flakiness.
    pool: "forks",
    poolOptions: {
      forks: {
        singleFork: true,
      },
    },
  },
});
