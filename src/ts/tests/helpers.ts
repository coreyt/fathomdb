// Shared helpers for the TypeScript SDK test suite.

import { randomUUID } from "node:crypto";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

/**
 * Allocate a fresh, never-before-used SQLite path for a single test.
 *
 * The napi-rs binding wires to a real engine that holds an exclusive
 * file lock per database; reusing one path across tests in the same
 * process would surface as `DatabaseLockedError`.
 */
export function freshDbPath(): string {
  const dir = mkdtempSync(join(tmpdir(), "fathomdb-ts-"));
  return join(dir, `${randomUUID()}.sqlite`);
}
