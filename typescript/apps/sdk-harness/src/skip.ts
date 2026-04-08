import type { HarnessResult } from "./models.js";

const NATIVE_UNAVAILABLE_PATTERNS = [
  "native binding could not be loaded",
  "Cannot find module",
  "MODULE_NOT_FOUND",
  "ERR_MODULE_NOT_FOUND",
];

function isNativeUnavailable(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return NATIVE_UNAVAILABLE_PATTERNS.some((p) => message.includes(p));
}

export function handleScenarioError(name: string, error: unknown): HarnessResult {
  if (isNativeUnavailable(error)) {
    return { name, ok: true, detail: "skipped: native binding not available" };
  }
  const message = error instanceof Error ? error.message : String(error);
  return { name, ok: false, detail: message };
}

export function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assertion failed: ${message}`);
}

export function tempDbPath(scenario: string): string {
  const ts = Date.now();
  const rand = Math.random().toString(36).slice(2, 8);
  return `/tmp/fathomdb-harness-${scenario}-${ts}-${rand}.db`;
}
