// 0.8.16 Slice 5 / F9 (codex §9 fix-2) — public-wrapper propagation of the
// per-hit explain `importance`/`confidence` fields (TypeScript SDK).
//
// The native N-API `PerHitExplain` already carries `importance`/`confidence`
// (fix-1). This test guards the *public TS SDK* seam: `mapPerHitExplain`
// (factored out of `Engine.search`) must copy the two F9 fields through into the
// public `PerHitExplain`, symmetric with the Python `_map_per_hit_explain`
// wrapper (`src/python/fathomdb/tests` counterpart
// `test_f9_explain_wrapper_propagation.py`).
//
// Uses a fake native per-hit object (no real `.node` needed for the mapping
// itself). NOTE: the `npm test` script builds the native module before running
// (`build:native:debug`), so this file executes at the Slice-40 MAIN-tree /
// CI gate — it is not run from the release worktree (HARD BUILD RULE).

import test from "node:test";
import assert from "node:assert/strict";

import { mapPerHitExplain } from "../src/index.js";

// A fake native per-hit object mirroring the N-API `PerHitExplain` shape.
function nativePerHit(
  importance: number | null,
  confidence: number | null,
): {
  id: number;
  arm: string;
  vectorRank: number | null;
  textRank: number | null;
  graphRank: number | null;
  fusedScore: number;
  ceScore: number | null;
  blended: number;
  importance: number | null;
  confidence: number | null;
} {
  return {
    id: 7,
    arm: "graph_arm",
    vectorRank: null,
    textRank: 1,
    graphRank: 0,
    fusedScore: 0.42,
    ceScore: null,
    blended: 0.42,
    importance,
    confidence,
  };
}

test("F9: importance + confidence propagate through mapPerHitExplain", () => {
  const mapped = mapPerHitExplain(nativePerHit(0.75, 0.9));
  // The F9 fields must reach the public object (the fix-2 regression).
  assert.equal(mapped.importance, 0.75);
  assert.equal(mapped.confidence, 0.9);
  // Pre-existing fields still map through unchanged (no regression).
  assert.equal(mapped.id, 7);
  assert.equal(mapped.arm, "graph_arm");
  assert.equal(mapped.vectorRank, null);
  assert.equal(mapped.textRank, 1);
  assert.equal(mapped.graphRank, 0);
  assert.equal(mapped.fusedScore, 0.42);
  assert.equal(mapped.ceScore, null);
  assert.equal(mapped.blended, 0.42);
});

test("F9: null/undefined importance + confidence normalize to null", () => {
  // Graceful-absent / neutral: null stays null.
  const fromNull = mapPerHitExplain(nativePerHit(null, null));
  assert.equal(fromNull.importance, null);
  assert.equal(fromNull.confidence, null);

  // A native object omitting the fields (undefined) also normalizes to null
  // via the `?? null` coalesce — parity with the Python `None` default.
  const bare = {
    id: 1,
    arm: "text",
    vectorRank: null,
    textRank: 0,
    graphRank: null,
    fusedScore: 0.1,
    ceScore: null,
    blended: 0.1,
  };
  const fromUndefined = mapPerHitExplain(bare);
  assert.equal(fromUndefined.importance, null);
  assert.equal(fromUndefined.confidence, null);
});
