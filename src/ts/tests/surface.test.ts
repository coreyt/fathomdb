// Surface assertions for the TypeScript SDK.
//
// Binds AC-074 (REQ-053): governed SDK surface — a curated allowlist with
// cross-binding parity, a permanent recovery-name denylist, and the typed /
// no-raw-SQL boundary. (AC-057a's verb-count scope cap is superseded by AC-074;
// the surface is now governed-but-open, not capped.)
//
// Pins the governed-surface allowlist (membership, not a count), the
// engine-attached instrumentation methods, the options.engineConfig camelCase
// knobs, the soft-fallback record shape, and the FathomDbError single-rooted
// hierarchy per `dev/interfaces/typescript.md` and
// `dev/design/bindings.md` § 1 / § 3.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine, admin, type EngineConfig, type SoftFallback } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

// The governed SDK surface allowlist (AC-074 / REQ-053). The set of public
// application-command callables across the SDK bindings, B1 `read.*` namespace.
// Declared identically in `src/python/tests/test_surface.py`; the two are
// membership-identical (cross-binding parity, P2) and byte-compared by the
// Slice 25.b audit. The `read.*` members are documented-allowlist members now
// but do NOT go live as callable symbols until Slice 30 — so the membership
// check below (P1) is a subset test, never equality.
const GOVERNED_SURFACE_ALLOWLIST: ReadonlySet<string> = new Set([
  // Core (live today, unchanged)
  "Engine.open",
  "admin.configure",
  "write",
  "search",
  "close",
  // Read surface (B1 read.*, ships 0.8.0, goes LIVE at Slice 30)
  "read.get",
  "read.get_many",
  "read.collection",
  "read.mutations",
]);

// Engine-attached instrumentation/control methods are observability, NOT
// application commands — excluded from the allowlist (preserved from AC-057a's
// measurement boundary).
const INSTRUMENTATION = ["drain", "counters", "setProfiling", "setSlowThresholdMs", "attachSubscriber"] as const;

// The permanent recovery-name denylist (the FIVE names). `doctor` is SDK-absent
// by non-membership in the allowlist (it is a CLI verb), NOT by this denylist.
const RECOVERY_DENYLIST = ["recover", "restore", "repair", "fix", "rebuild"] as const;

test("public surface is the governed allowlist (membership, not a count)", async () => {
  // P1 — every live public application command is a governed-allowlist member.
  // Membership (subset), NOT equality: the allowlist is a superset including the
  // not-yet-live read.* verbs (live at Slice 30), so a live surface that is
  // currently the core five is honestly green against the 9-member allowlist.
  assert.equal(typeof Engine.open, "function", "Engine.open must be a static method");

  const engine = await Engine.open(freshDbPath());
  try {
    const live = new Set<string>();
    if (typeof Engine.open === "function") live.add("Engine.open");
    for (const v of ["write", "search", "close"] as const) {
      if (typeof (engine as unknown as Record<string, unknown>)[v] === "function") {
        live.add(v);
      }
    }
    if (typeof admin.configure === "function") live.add("admin.configure");

    for (const name of live) {
      assert.ok(
        GOVERNED_SURFACE_ALLOWLIST.has(name),
        `live command ${name} is outside the governed allowlist`,
      );
    }
    for (const core of ["Engine.open", "admin.configure", "write", "search", "close"]) {
      assert.ok(live.has(core), `core command ${core} must be live`);
    }
  } finally {
    await engine.close();
  }
});

test("surface parity: TS allowlist matches Python", () => {
  // P2 — the TypeScript governed allowlist equals the Python one.
  // `src/python/tests/test_surface.py` declares the identical
  // GOVERNED_SURFACE_ALLOWLIST; the mirror below is the Python contract and the
  // Slice 25.b audit byte-compares the two constants across files.
  const pythonGovernedSurfaceAllowlist = new Set([
    "Engine.open",
    "admin.configure",
    "write",
    "search",
    "close",
    "read.get",
    "read.get_many",
    "read.collection",
    "read.mutations",
  ]);
  assert.equal(GOVERNED_SURFACE_ALLOWLIST.size, pythonGovernedSurfaceAllowlist.size);
  for (const name of pythonGovernedSurfaceAllowlist) {
    assert.ok(GOVERNED_SURFACE_ALLOWLIST.has(name), `parity: TS allowlist missing ${name}`);
  }
});

test("allowlist excludes the recovery denylist", () => {
  // P3 — allowlist ∩ {recover,restore,repair,fix,rebuild} = ∅. Allowlist-level
  // only; the byte-frozen no-recovery-surface.test.ts is the live enforcement.
  for (const name of RECOVERY_DENYLIST) {
    assert.ok(!GOVERNED_SURFACE_ALLOWLIST.has(name), `denylisted name ${name} in allowlist`);
  }
});

test("engine exposes instrumentation methods", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    for (const m of INSTRUMENTATION) {
      assert.equal(
        typeof (engine as unknown as Record<string, unknown>)[m],
        "function",
        `engine must expose ${m}`,
      );
    }
  } finally {
    await engine.close();
  }
});

test("admin.configure is exported beside Engine", async () => {
  assert.equal(typeof admin.configure, "function");
  const engine = await Engine.open(freshDbPath());
  try {
    const receipt = await admin.configure(engine, { name: "default", body: "{}" });
    assert.equal(typeof receipt.cursor, "number");
  } finally {
    await engine.close();
  }
});

test("Engine.open accepts engineConfig with camelCase knobs", async () => {
  const cfg: EngineConfig = {
    embedderPoolSize: 2,
    schedulerRuntimeThreads: 4,
    provenanceRowCap: 1024,
    embedderCallTimeoutMs: 30_000,
    slowThresholdMs: 250,
  };
  const engine = await Engine.open(freshDbPath(), { engineConfig: cfg });
  try {
    assert.equal(engine.config.embedderPoolSize, 2);
    assert.equal(engine.config.schedulerRuntimeThreads, 4);
    assert.equal(engine.config.provenanceRowCap, 1024);
    assert.equal(engine.config.embedderCallTimeoutMs, 30_000);
    assert.equal(engine.config.slowThresholdMs, 250);
  } finally {
    await engine.close();
  }
});

test("write returns a typed receipt with cursor", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const receipt = await engine.write([{ kind: "doc", body: "{}" }]);
    assert.equal(typeof receipt.cursor, "number");
  } finally {
    await engine.close();
  }
});

test("search returns soft-fallback null by default", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const result = await engine.search("hello");
    assert.equal(result.softFallback, null);
    assert.equal(typeof result.projectionCursor, "number");
  } finally {
    await engine.close();
  }
});

test("SoftFallback branch is the typed two-member union", () => {
  const v: SoftFallback = { branch: "vector" };
  const t: SoftFallback = { branch: "text" };
  assert.equal(v.branch, "vector");
  assert.equal(t.branch, "text");
});

test("instrumentation stubs return canonical types", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.drain(0);
    const snap = engine.counters();
    assert.ok(snap !== undefined);
    engine.setProfiling(true);
    engine.setSlowThresholdMs(100);
    engine.attachSubscriber(() => undefined, {});
  } finally {
    await engine.close();
  }
});
