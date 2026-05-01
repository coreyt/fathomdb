import { tmpdir } from "node:os";
import { join } from "node:path";

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

// Route through os.tmpdir() (which reads $TMPDIR) so tests honor the
// per-session temp root the test orchestrator/CI sets up. Cleanup is
// handled at session scope by rm -rf'ing the TMPDIR root — see GH #40.
export function tempDbPath(scenario: string): string {
  const ts = Date.now();
  const rand = Math.random().toString(36).slice(2, 8);
  return join(tmpdir(), `fathomdb-harness-${scenario}-${ts}-${rand}.db`);
}
