// EU-6 FIX-1 — release-surface introspection (RED).
//
// Builds a release-equivalent fathomdb-napi .node (cargo build --release
// --features default-embedder, NO test-hooks), loads it directly, and
// asserts the *actually shipped* binary surface:
//
// - AC-FIX1-2: writeVectorForTest / configureVectorKindForTest /
//   forcePanicForTest are NOT present on the loaded native Engine.
// - AC-FIX1-4: engineOpen(path, { useDefaultEmbedder: true }) succeeds
//   (network-gated via FATHOMDB_SKIP_NETWORK_TESTS, symmetrical with
//   EU-5c).
//
// Gated on RELEASE_SURFACE_TESTS=1; the cargo build is slow. Default
// `npm test` invocations skip with a clear log line.

import test from "node:test";
import assert from "node:assert/strict";
import { execSync } from "node:child_process";
import { createRequire } from "node:module";
import { mkdtempSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import { freshDbPath } from "./helpers.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
// dist/tests/release-surface.test.js → repo root is four levels up
// (dist/tests → dist → src/ts → src → repo). Resolve dynamically.
const TS_DIR = resolve(__dirname, "..", "..", "..");
const REPO_ROOT = resolve(TS_DIR, "..", "..");
const NAPI_MANIFEST = join(
  REPO_ROOT,
  "src",
  "rust",
  "crates",
  "fathomdb-napi",
  "Cargo.toml",
);

function releaseSurfaceEnabled(): boolean {
  if (process.env.RELEASE_SURFACE_TESTS !== "1") {
    console.log(
      "[skip] RELEASE_SURFACE_TESTS!=1; release-surface tests disabled",
    );
    return false;
  }
  return true;
}

function skipIfNoNetwork(): boolean {
  if (process.env.FATHOMDB_SKIP_NETWORK_TESTS) {
    console.log("[skip] FATHOMDB_SKIP_NETWORK_TESTS set");
    return true;
  }
  return false;
}

interface ReleaseNative {
  loaded: Record<string, unknown>;
  engineCtor: unknown;
}

function buildAndLoadReleaseNative(): ReleaseNative {
  // Build the release-equivalent .node with ONLY default-embedder,
  // mirroring release.yml::build-napi after FIX-1 GREEN.
  execSync(
    `cargo build --release --features default-embedder --manifest-path ${JSON.stringify(NAPI_MANIFEST)}`,
    { stdio: "inherit" },
  );

  // Locate the produced cdylib. Path varies by platform.
  const targetReleaseDir = join(REPO_ROOT, "target", "release");
  const entries = readdirSync(targetReleaseDir);
  const candidate = entries.find(
    (name) =>
      (name.startsWith("libfathomdb_napi") || name.startsWith("fathomdb_napi")) &&
      (name.endsWith(".so") || name.endsWith(".dylib") || name.endsWith(".dll")),
  );
  assert.ok(
    candidate,
    `no fathomdb_napi cdylib found in ${targetReleaseDir}: ${entries.join(", ")}`,
  );

  // Copy/symlink to a temp dir with a .node suffix so require() loads it.
  const stagingDir = mkdtempSync(join(tmpdir(), "fathomdb-rel-surface-"));
  const stagedPath = join(stagingDir, "fathomdb-release.node");
  // Use copyFileSync to avoid symlink quirks on Windows.
  const fs = require("node:fs") as typeof import("node:fs");
  fs.copyFileSync(join(targetReleaseDir, candidate!), stagedPath);

  const requireFromHere = createRequire(import.meta.url);
  const loaded = requireFromHere(stagedPath) as Record<string, unknown>;
  return { loaded, engineCtor: loaded.Engine ?? loaded.NativeEngine };
}

test("release-equivalent .node does not expose test-hooks methods", async () => {
  if (!releaseSurfaceEnabled()) return;

  const { loaded, engineCtor } = buildAndLoadReleaseNative();

  const exportedNames = Object.keys(loaded);
  const leakedExports = exportedNames.filter((name) =>
    /(?:write|configure)VectorForTest|forcePanicForTest/.test(name),
  );
  assert.deepEqual(
    leakedExports,
    [],
    `release .node exports leak dev-only hooks: ${leakedExports.join(", ")}`,
  );

  if (engineCtor && typeof engineCtor === "function") {
    const proto = (engineCtor as { prototype?: Record<string, unknown> }).prototype ?? {};
    const protoLeaked = Object.getOwnPropertyNames(proto).filter((name) =>
      /(?:write|configure)VectorForTest|forcePanicForTest/.test(name),
    );
    assert.deepEqual(
      protoLeaked,
      [],
      `release Engine prototype leaks dev-only hooks: ${protoLeaked.join(", ")}`,
    );
  }
});

test("release-equivalent .node opens engine with useDefaultEmbedder: true", async () => {
  if (!releaseSurfaceEnabled()) return;
  if (skipIfNoNetwork()) return;

  const { loaded } = buildAndLoadReleaseNative();
  const engineOpen = loaded.engineOpen as
    | ((path: string, opts: { useDefaultEmbedder: boolean }) => Promise<unknown>)
    | undefined;
  assert.ok(
    typeof engineOpen === "function",
    `release .node missing engineOpen export; got ${Object.keys(loaded).join(", ")}`,
  );

  const path = freshDbPath();
  const engine = (await engineOpen!(path, { useDefaultEmbedder: true })) as {
    openReport: () => { defaultEmbedder: { name: string; dimension: number } };
    close: () => Promise<void>;
  };
  try {
    const report = engine.openReport();
    assert.equal(report.defaultEmbedder.name, "fathomdb-bge-small-en-v1.5");
    assert.equal(report.defaultEmbedder.dimension, 384);
  } finally {
    await engine.close();
  }
});
