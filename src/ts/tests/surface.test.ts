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
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Engine, admin, read, type EngineConfig, type SoftFallback } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

// The governed SDK surface allowlist (AC-074 / REQ-053) is declared exactly
// ONCE, in `src/conformance/governed-surface-allowlist.json`, and read by BOTH
// this suite and the Python suite (`src/python/tests/test_surface.py`). There
// is no per-binding duplicate literal, so TypeScript and Python cannot drift
// apart (cross-binding parity, P2). As of Slice 30 the four `read.*` members are
// LIVE: they are introspected off the `read` namespace object and enter the live
// set, so the membership check below (P1) is still a subset test (never
// equality) but `read.*` is now actually asserted-live, not documented-only.
interface GovernedSurfaceContract {
  allowlist: string[];
  core: string[];
  recovery_denylist: string[];
}

function loadGovernedSurfaceContract(): GovernedSurfaceContract {
  // This file compiles to `src/ts/dist/tests/surface.test.js` and runs from
  // there; the shared contract lives at `src/conformance/...`.
  //   dist/tests -> dist -> src/ts -> src -> src/conformance
  const here = dirname(fileURLToPath(import.meta.url));
  const contractPath = join(
    here,
    "..",
    "..",
    "..",
    "conformance",
    "governed-surface-allowlist.json",
  );
  return JSON.parse(readFileSync(contractPath, "utf8")) as GovernedSurfaceContract;
}

const CONTRACT = loadGovernedSurfaceContract();
const GOVERNED_SURFACE_ALLOWLIST: ReadonlySet<string> = new Set(CONTRACT.allowlist);
const CORE_LIVE_SURFACE: readonly string[] = CONTRACT.core;

// Engine-attached instrumentation/control methods are observability, NOT
// application commands — excluded from the allowlist (preserved from AC-057a's
// measurement boundary).
const INSTRUMENTATION = [
  "drain",
  "counters",
  "setProfiling",
  "setSlowThresholdMs",
  "attachSubscriber",
  // 0.8.8 Slice 15 (OPP-9) — opt-in telemetry capture is observability, NOT an
  // application command (mirrors setProfiling/attachSubscriber).
  "enableTelemetry",
  "lastTelemetryQueryId",
  "recordFeedback",
  // 0.8.18 Slice 5 (#5 vector-equivalence probe, R-VEQ-6) — degraded-open
  // observability accessors, NOT application commands (mirror counters /
  // openReport / lastTelemetryQueryId).
  "denseDisabled",
  "denseDisabledReason",
  "vectorEquivalenceRefusalCount",
] as const;

// Other public `Engine` members that are NOT application commands: the
// open-time report accessor (Shape D) and the `config` data accessor.
// Subtracted from the introspected surface alongside INSTRUMENTATION so the
// live-command set is exactly the command verbs. A NEW public command (e.g.
// `Engine.delete`) is NOT in this exclusion set, so it would enter `live` and
// fail the subset check (P1).
const ENGINE_NON_COMMAND = new Set<string>([...INSTRUMENTATION, "openReport", "config"]);

// The permanent recovery-name denylist (the FIVE names). `doctor` is SDK-absent
// by non-membership in the allowlist (it is a CLI verb), NOT by this denylist.
const RECOVERY_DENYLIST = CONTRACT.recovery_denylist;

// Slice 30 — the four governed read verbs that go LIVE under `read.*`. Asserted
// present-in-the-introspected-surface so a future REMOVAL of any `read.*` verb
// fails this suite (parity with the Python `_NOW_LIVE_READ_VERBS` check).
const NOW_LIVE_READ_VERBS = [
  "read.get",
  "read.get_many",
  "read.collection",
  "read.mutations",
] as const;

// camelCase → snake_case so the introspected TS `read` verb identity maps to the
// single shared allowlist's dotted snake_case names (`getMany` → `get_many`).
// This is the ONLY place TS verb identity is normalized to the canonical
// allowlist name, so a one-sided extra/missing `read.*` verb still fails parity.
function toSnakeCase(name: string): string {
  return name.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
}

// Introspect the REAL live public command surface of the TypeScript SDK,
// mirroring the enumeration style of `no-recovery-surface.test.ts`: static
// names off `Engine`, plus prototype + own instance names off a live engine,
// minus dunder/private/constructor and the known non-command members. Any
// public name the binding actually exposes that is not a known non-command verb
// enters the live set — so a hypothetical `Engine.delete` would surface here
// and fail the subset check, rather than being silently ignored.
function liveTsCommandSurface(engine: Engine): Set<string> {
  const live = new Set<string>();
  const proto = Object.getPrototypeOf(engine) as object;
  const instanceNames = new Set<string>([
    ...Object.getOwnPropertyNames(proto),
    ...Object.keys(engine as unknown as Record<string, unknown>),
  ]);
  for (const name of instanceNames) {
    if (name.startsWith("_") || name === "constructor") continue;
    if (ENGINE_NON_COMMAND.has(name)) continue;
    const value = (engine as unknown as Record<string, unknown>)[name];
    if (typeof value !== "function") continue; // skip data accessors like `config`
    live.add(name);
  }
  // Static command verbs off the `Engine` class itself.
  for (const name of Object.getOwnPropertyNames(Engine)) {
    if (name.startsWith("_")) continue;
    if (["length", "name", "prototype"].includes(name)) continue;
    if (typeof (Engine as unknown as Record<string, unknown>)[name] !== "function") continue;
    live.add(name === "open" ? "Engine.open" : name);
  }
  if (typeof admin.configure === "function") live.add("admin.configure");
  // Slice 30 — the governed `read.*` namespace. Introspect the `read` object's
  // own function-valued keys (mirroring the `admin` introspection); normalize
  // camelCase → snake_case so the emitted name matches the dotted allowlist. A
  // stray non-allowlisted `read` verb (e.g. `read.delete`) enters `live` and
  // fails the P1 subset check.
  for (const key of Object.keys(read as unknown as Record<string, unknown>)) {
    if (key.startsWith("_")) continue;
    if (typeof (read as unknown as Record<string, unknown>)[key] !== "function") continue;
    live.add(`read.${toSnakeCase(key)}`);
  }
  return live;
}

test("public surface is the governed allowlist (membership, not a count)", async () => {
  // P1 — every live public application command is a governed-allowlist member.
  // Membership (subset), NOT equality: the allowlist is a superset; the live
  // surface as of Slice 30 is the core five PLUS the four read.* verbs, all
  // members of the 9-name allowlist.
  assert.equal(typeof Engine.open, "function", "Engine.open must be a static method");

  const engine = await Engine.open(freshDbPath());
  try {
    const live = liveTsCommandSurface(engine);

    for (const name of live) {
      assert.ok(
        GOVERNED_SURFACE_ALLOWLIST.has(name),
        `live command ${name} is outside the governed allowlist`,
      );
    }
    for (const core of CORE_LIVE_SURFACE) {
      assert.ok(live.has(core), `core command ${core} must be live`);
    }
  } finally {
    await engine.close();
  }
});

test("read.* namespace verbs are live (Slice 30, introspected not documented-only)", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const live = liveTsCommandSurface(engine);
    for (const verb of NOW_LIVE_READ_VERBS) {
      assert.ok(live.has(verb), `read verb ${verb} must be live (introspected)`);
      assert.ok(GOVERNED_SURFACE_ALLOWLIST.has(verb), `${verb} must be an allowlist member`);
    }
  } finally {
    await engine.close();
  }
});

test("searchTextOnly verb is live (0.8.18 Slice 5 #5, CONCERN #7, introspected)", async () => {
  // The P1 subset check passes even if the verb VANISHES (fewer-live is allowed)
  // and `searchTextOnly` is not in CORE_LIVE_SURFACE, so guard its PRESENCE +
  // callability directly — otherwise the FTS-only degraded-mode surface (R-VEQ-4)
  // could disappear vacuously-green while the allowlist still lists it.
  assert.ok(GOVERNED_SURFACE_ALLOWLIST.has("searchTextOnly"));
  const engine = await Engine.open(freshDbPath());
  try {
    assert.equal(
      typeof (engine as unknown as Record<string, unknown>).searchTextOnly,
      "function",
      "Engine.searchTextOnly must be a live callable (governed FTS-only path)",
    );
    assert.ok(
      liveTsCommandSurface(engine).has("searchTextOnly"),
      "searchTextOnly must be introspected-live, not documented-only",
    );
  } finally {
    await engine.close();
  }
});

test("surface parity: TS and Python read one shared allowlist", async () => {
  // P2 — the governed allowlist is declared exactly ONCE, in
  // `src/conformance/governed-surface-allowlist.json`. This suite loads it via
  // `loadGovernedSurfaceContract()`; `src/python/tests/test_surface.py` loads
  // the same file. Because there is a single declaration, TypeScript and Python
  // can no longer carry divergent copies — parity is structural, not a
  // byte-compared duplicate. This test pins that the suite genuinely consumes
  // the shared contract (the introspected live surface is a subset of it).
  const contract = loadGovernedSurfaceContract();
  assert.deepEqual([...GOVERNED_SURFACE_ALLOWLIST].sort(), [...new Set(contract.allowlist)].sort());

  const engine = await Engine.open(freshDbPath());
  try {
    const live = liveTsCommandSurface(engine);
    for (const name of live) {
      assert.ok(
        GOVERNED_SURFACE_ALLOWLIST.has(name),
        `live command ${name} is outside the shared governed allowlist`,
      );
    }
  } finally {
    await engine.close();
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
