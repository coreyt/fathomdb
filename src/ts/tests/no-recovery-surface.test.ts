// AC-041 — recovery tooling unreachable from the TypeScript runtime SDK.
//
// Mirrors the introspection style of `surface.test.ts`. Asserts the public
// module exports, the `Engine` class (static + instance), and the `admin`
// namespace expose none of the canonical recovery-verb names. Recovery is
// CLI-only per `dev/interfaces/cli.md`; the runtime SDK five-verb surface
// (open/write/search/close/admin.configure) does not mirror it.

import test from "node:test";
import assert from "node:assert/strict";

import * as fathomdb from "../src/index.js";
import { Engine, admin } from "../src/index.js";

const FORBIDDEN = new Set(["recover", "restore", "repair", "fix", "rebuild"]);

function publicNames(obj: object): Set<string> {
  return new Set(Object.keys(obj as Record<string, unknown>).filter((n) => !n.startsWith("_")));
}

function intersection<T>(a: Set<T>, b: Set<T>): T[] {
  return [...a].filter((x) => b.has(x));
}

test("module top-level exports include no recovery verbs", () => {
  const names = publicNames(fathomdb);
  assert.deepEqual(intersection(names, FORBIDDEN), []);
});

test("Engine class statics include no recovery methods", () => {
  const names = publicNames(Engine as unknown as object);
  assert.deepEqual(intersection(names, FORBIDDEN), []);
});

test("Engine instance has no recovery methods", async () => {
  const engine = await Engine.open("test.sqlite");
  const proto = Object.getPrototypeOf(engine) as object;
  const protoNames = new Set(
    Object.getOwnPropertyNames(proto).filter((n) => !n.startsWith("_") && n !== "constructor"),
  );
  const ownNames = publicNames(engine);
  const all = new Set<string>([...protoNames, ...ownNames]);
  assert.deepEqual(intersection(all, FORBIDDEN), []);
});

test("admin namespace exports no recovery verbs", () => {
  const names = publicNames(admin);
  assert.deepEqual(intersection(names, FORBIDDEN), []);
});
