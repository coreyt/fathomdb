// R-REL-4f — npm per-platform split contract. Verifies the host-resolution +
// loader logic in `../src/platform.ts` WITHOUT a built native binding (the
// module is side-effect-free), so this test runs even when no `.node` is
// present. See dev/design/0.8.18-slice-20-publish-pipeline.md §U2.
//
// Contract under test: a mac/win install of the linux-x64-only 0.8.18 release
// resolves NO linux `.node` (the platform optionalDependency was skipped by
// npm's os/cpu match) and the loader throws a clear "unsupported platform"
// error at REQUIRE time — never a silent runtime segfault.

import test from "node:test";
import assert from "node:assert/strict";

import {
  UnsupportedPlatformError,
  loadPlatformBinding,
  platformPackageName,
  resolveTriple,
  tripleFor,
} from "../src/platform.js";

test("npm_platform_split_contract: triple resolution for supported hosts", () => {
  assert.equal(resolveTriple("linux", "x64", false), "linux-x64-gnu");
  assert.equal(resolveTriple("linux", "x64", true), "linux-x64-musl");
  assert.equal(resolveTriple("linux", "arm64", false), "linux-arm64-gnu");
  assert.equal(resolveTriple("darwin", "arm64", false), "darwin-arm64");
  assert.equal(resolveTriple("darwin", "x64", false), "darwin-x64");
  assert.equal(resolveTriple("win32", "x64", false), "win32-x64-msvc");
});

test("npm_platform_split_contract: platform package name is scoped @fathomdb", () => {
  assert.equal(
    platformPackageName("linux-x64-gnu"),
    "@fathomdb/fathomdb-linux-x64-gnu",
  );
});

test("npm_platform_split_contract: unmapped host throws unsupported platform", () => {
  assert.equal(tripleFor("sunos" as NodeJS.Platform, "mips", false), null);
  assert.throws(
    () => resolveTriple("sunos" as NodeJS.Platform, "mips", false),
    (err: unknown) => {
      assert.ok(err instanceof UnsupportedPlatformError);
      assert.match((err as Error).message, /unsupported platform/i);
      return true;
    },
  );
});

test("npm_platform_split_contract: mac install with skipped linux dep throws at require-time (no segfault)", () => {
  // Simulate a darwin/arm64 host where 0.8.18 published ONLY the linux-x64-gnu
  // platform package: no local dev `.node`, and `require('@fathomdb/fathomdb-
  // darwin-arm64')` fails MODULE_NOT_FOUND because npm skipped that optionalDep.
  const moduleNotFound = () => {
    const e = new Error(
      "Cannot find module '@fathomdb/fathomdb-darwin-arm64'",
    ) as Error & { code?: string };
    e.code = "MODULE_NOT_FOUND";
    throw e;
  };

  assert.throws(
    () =>
      loadPlatformBinding({
        platform: "darwin",
        arch: "arm64",
        isMusl: false,
        loadLocal: () => null,
        requirePackage: moduleNotFound,
      }),
    (err: unknown) => {
      assert.ok(
        err instanceof UnsupportedPlatformError,
        "must be UnsupportedPlatformError, not a raw MODULE_NOT_FOUND / segfault",
      );
      const msg = (err as Error).message;
      assert.match(msg, /unsupported platform/i);
      assert.match(msg, /@fathomdb\/fathomdb-darwin-arm64/);
      return true;
    },
  );
});

test("npm_platform_split_contract: loads local dev binary when present", () => {
  const sentinel = { __fathomdb_native: true };
  const mod = loadPlatformBinding({
    platform: "linux",
    arch: "x64",
    isMusl: false,
    loadLocal: (triple) => (triple === "linux-x64-gnu" ? sentinel : null),
    requirePackage: () => {
      throw new Error("should not reach the platform package when local exists");
    },
  });
  assert.equal(mod, sentinel);
});

test("npm_platform_split_contract: loads published platform package when installed", () => {
  const sentinel = { __fathomdb_native: "pkg" };
  const mod = loadPlatformBinding({
    platform: "linux",
    arch: "x64",
    isMusl: false,
    loadLocal: () => null,
    requirePackage: (pkg) => {
      assert.equal(pkg, "@fathomdb/fathomdb-linux-x64-gnu");
      return sentinel;
    },
  });
  assert.equal(mod, sentinel);
});
