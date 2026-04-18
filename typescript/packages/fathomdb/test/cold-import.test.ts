/**
 * Regression guard: importing fathomdb must not require the native binding
 * to already be resolvable. Consumers like the sdk-harness (and anyone using
 * FATHOMDB_NATIVE_BINDING set by a test harness at runtime rather than at
 * import time) expect module load to be lazy. ARCH-006 briefly broke this
 * contract by calling `loadNativeBinding().listTokenizerPresets()` at
 * top-level module initialization; the fix is a Proxy-backed lazy accessor.
 *
 * This test imports the package with the native binding hidden and asserts
 * that the import itself succeeds. Touching TOKENIZER_PRESETS (a Proxy get)
 * is what should trigger binding resolution, not the import.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const BINDING_KEY = "FATHOMDB_NATIVE_BINDING";
const INVALID_PATH = "/nonexistent/fathomdb-cold-import-guard.node";

describe("cold import (no native binding resolvable)", () => {
  let savedBinding: string | undefined;
  let savedMock: unknown;

  beforeEach(() => {
    savedBinding = process.env[BINDING_KEY];
    // Point the loader at a path that will fail to require(), simulating
    // a consumer that imports fathomdb before arranging a real binding.
    process.env[BINDING_KEY] = INVALID_PATH;
    // Also clear the global mock hook that vitest.config.ts may install.
    savedMock = (globalThis as { __FATHOMDB_NATIVE_MOCK__?: unknown }).__FATHOMDB_NATIVE_MOCK__;
    delete (globalThis as { __FATHOMDB_NATIVE_MOCK__?: unknown }).__FATHOMDB_NATIVE_MOCK__;
    // vitest caches ES modules across tests; resetModules forces a fresh
    // module graph so the top-level init of admin.ts actually re-runs
    // under the overridden env.
    vi.resetModules();
  });

  afterEach(() => {
    if (savedBinding === undefined) {
      delete process.env[BINDING_KEY];
    } else {
      process.env[BINDING_KEY] = savedBinding;
    }
    if (savedMock !== undefined) {
      (globalThis as { __FATHOMDB_NATIVE_MOCK__?: unknown }).__FATHOMDB_NATIVE_MOCK__ = savedMock;
    }
    vi.resetModules();
  });

  it("import of fathomdb root does not resolve the native binding", async () => {
    // If ARCH-006-style eager FFI init ever regresses, this import itself
    // throws (because require(INVALID_PATH) fails). A successful import is
    // the signal that module load is lazy.
    const mod = await import("../src/index.js");
    expect(mod).toBeDefined();
    expect(typeof mod.Engine).toBe("function");
  });

  it("accessing TOKENIZER_PRESETS is what triggers binding resolution", async () => {
    const mod = await import("../src/admin.js");
    // Touching the proxy should now try to load the (invalid) binding and
    // throw — proving the accessor, not the import, drives the FFI call.
    expect(() => Object.keys(mod.TOKENIZER_PRESETS)).toThrow();
  });
});
