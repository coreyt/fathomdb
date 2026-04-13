// Real-engine test helpers.
//
// These helpers are used by the TypeScript SDK test suite to open a real
// fathomdb engine backed by a tempdir-backed SQLite database. Unlike the
// pre-P7.6b test infrastructure, which installed a mocked napi binding via
// `Engine.setBindingForTests`, this module loads the actual `.node` artifact
// built from the worktree's Rust sources. This guarantees that assertions
// exercise real FTS5 queries, real snake→camel wire conversion at the napi
// boundary, and real score/snippet/writtenAt values.
//
// Load order:
//   1. The test runner (vitest.config.ts) points FATHOMDB_NATIVE_BINDING at
//      a freshly copied `.node` file (see test/helpers/native-binding.ts).
//   2. `openTempEngine()` creates a tempdir, opens the engine against
//      `t.db` inside it, and returns an object with `engine` plus a
//      `cleanup()` thunk.
//   3. Every test either uses `openTempEngine` directly in setup/teardown or
//      declares a `beforeEach` that creates a fresh engine per test so that
//      one test's seeded data cannot leak into another.

import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { Engine, WriteRequestBuilder, newRowId } from "../../src/index.js";

export type TempEngine = {
  engine: Engine;
  dir: string;
  cleanup: () => void;
};

export function openTempEngine(): TempEngine {
  const dir = mkdtempSync(join(tmpdir(), "fathomdb-ts-test-"));
  const dbPath = join(dir, "t.db");
  // Reset the cached binding every test — the native binding itself is
  // cached inside src/native.ts, but Engine.setBindingForTests may have
  // been used earlier in the suite to install a scoped mock. Resetting
  // to null forces Engine to fall back to loadNativeBinding(), which
  // resolves the real .node file via FATHOMDB_NATIVE_BINDING.
  Engine.setBindingForTests(null);
  const engine = Engine.open(dbPath);
  return {
    engine,
    dir,
    cleanup: () => {
      try {
        engine.close();
      } catch {
        // swallow — we still want to clean up the tempdir even on errors.
      }
      if (existsSync(dir)) {
        rmSync(dir, { recursive: true, force: true });
      }
    },
  };
}

/**
 * Seed two Goal nodes with chunk text that lets FTS queries find them.
 * Mirrors the Python test_text_search_surface.py seed helper for parity.
 */
export function seedBudgetGoals(engine: Engine): void {
  engine.admin.registerFtsPropertySchema("Goal", ["$.name", "$.description"]);
  const builder = new WriteRequestBuilder("seed-budget");
  const alpha = builder.addNode({
    rowId: newRowId(),
    logicalId: "budget-alpha",
    kind: "Goal",
    properties: {
      name: "budget alpha goal",
      description: "quarterly budget rollup",
    },
    sourceRef: "seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  const bravo = builder.addNode({
    rowId: newRowId(),
    logicalId: "budget-bravo",
    kind: "Goal",
    properties: {
      name: "budget bravo goal",
      description: "annual budget summary",
    },
    sourceRef: "seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  builder.addChunk({
    id: "budget-alpha-chunk",
    node: alpha,
    textContent: "alpha budget quarterly docs review notes",
  });
  builder.addChunk({
    id: "budget-bravo-chunk",
    node: bravo,
    textContent: "bravo budget annual summary notes",
  });
  engine.write(builder.build());
}

/**
 * Seed a single Doc node so queries against `Doc` return exactly one row.
 * Used by tests that were previously asserting on a mocked single-row
 * response (`row_id: "r1"`).
 */
export function seedSingleDoc(
  engine: Engine,
  opts: {
    logicalId?: string;
    contentRef?: string | null;
  } = {},
): { rowId: string; logicalId: string } {
  const rowId = newRowId();
  const logicalId = opts.logicalId ?? "doc-1";
  const builder = new WriteRequestBuilder("seed-doc");
  builder.addNode({
    rowId,
    logicalId,
    kind: "Doc",
    properties: { title: "test" },
    contentRef: opts.contentRef ?? "s3://docs/test.pdf",
  });
  engine.write(builder.build());
  return { rowId, logicalId };
}
