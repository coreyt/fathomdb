// X1 EXP-OBS explain parity harness (TypeScript SDK) — 0.8.8 Slice 10.
//
// Opens a REAL engine, writes a small corpus, and exercises
// `search(..., explain)` end-to-end across the FFI. Mirrors the Python harness
// (`src/python/tests/test_exp_obs_explain_parity.py`) so both bindings are shown
// to surface the SAME explanation contract (the only permitted cross-binding
// difference is snake_case Python vs camelCase TS field spelling):
//
//   1. Carrier gating — explain off ⇒ `explanation === null` + byte-identical
//      results; explain on ⇒ present.
//   2. QueryTrace — all 12 fields present + typed; `alpha` exact; `embedderId`
//      is a string.
//   3. per_hit ↔ results alignment (same length/order; correlate by ARRAY
//      POSITION — post-C-2 `perHit[i].id` is the positional write_cursor int,
//      while `results[i].id` is the typed IdSpace, so NOT by id equality).
//   4. The three self-consistency identities (arm===branch, ceScore, blended).
//   5. None/Some rank fidelity (a null rank + an int rank across the pool).
//   6. `"graph_arm"` is assignable to SoftFallbackBranch (the Slice-10 contract).
//   7. F9 (0.8.16 Slice 5) — the additive `importance`/`confidence` per-hit
//      fields survive the real FFI + wrapper. On the default path (F9 reweight
//      OFF, no public SDK seam to enable it) both are `null`; the assertion
//      proves the field crossed the compiled boundary. Mirrored by the Python
//      harness so the two bindings stay symmetric (R-X-1 for F9).

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import type { Explanation, SearchResult, SoftFallbackBranch } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

async function searchAfterProjection(
  engine: Engine,
  query: string,
  explain: boolean,
): Promise<SearchResult> {
  const deadline = Date.now() + 10_000;
  let last = await engine.search(query, undefined, undefined, undefined, undefined, undefined, explain);
  while (Date.now() < deadline) {
    last = await engine.search(query, undefined, undefined, undefined, undefined, undefined, explain);
    if (last.results.length > 0) return last;
    await new Promise((r) => setTimeout(r, 20));
  }
  return last;
}

async function seed(engine: Engine): Promise<void> {
  for (const body of ["hybrid retrieval alpha", "hybrid retrieval beta", "hybrid retrieval gamma"]) {
    await engine.write([{ kind: "doc", body }]);
  }
  await engine.drain(30_000);
}

test("exp-obs carrier gating + byte stability", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seed(engine);
    const plain = await searchAfterProjection(engine, "hybrid", false);
    assert.ok(plain.results.length > 0, "expected hits");
    // (1) default path suppresses the sidecar.
    assert.equal(plain.explanation, null);

    const explained = await engine.search(
      "hybrid", undefined, undefined, undefined, undefined, undefined, true,
    );
    assert.notEqual(explained.explanation, null, "explain populates the sidecar");
    // results byte-identical to the plain call.
    assert.deepEqual(
      explained.results.map((h) => h.id),
      plain.results.map((h) => h.id),
    );
    assert.deepEqual(
      explained.results.map((h) => h.score),
      plain.results.map((h) => h.score),
    );
    assert.equal(explained.projectionCursor, plain.projectionCursor);
  } finally {
    await engine.close();
  }
});

test("exp-obs trace + per-hit fidelity", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seed(engine);
    const result = await searchAfterProjection(engine, "hybrid", true);
    const exp = result.explanation as Explanation;
    assert.notEqual(exp, null);

    // (2) QueryTrace — 12 fields, typed; alpha exact; embedderId is a string.
    const t = exp.trace;
    assert.equal(t.queryChars, "hybrid".length);
    assert.equal(typeof t.k, "number");
    assert.equal(t.rerankDepth, 0);
    assert.equal(typeof t.poolN, "number");
    assert.equal(t.alpha, 0.3);
    assert.equal(t.useGraphArm, false);
    assert.equal(typeof t.recency, "boolean");
    assert.equal(typeof t.embedderId, "string");
    assert.equal(t.ceActive, false);
    assert.equal(typeof t.vectorHits, "number");
    assert.equal(typeof t.textHits, "number");
    assert.equal(typeof t.graphHits, "number");

    // (3) alignment + (4) the three identities.
    assert.equal(exp.perHit.length, result.results.length);
    for (let i = 0; i < exp.perHit.length; i++) {
      const p = exp.perHit[i]!;
      const h = result.results[i]!;
      // C-2 (0.8.19): perHit.id is the hit's engine-internal positional
      // write_cursor (a number, the pre-0.8.19 SearchHit.id space); the caller-
      // facing SearchHit.id is the typed IdSpace. Correlate by position (this
      // loop), not by cross-type id equality.
      assert.equal(typeof p.id, "number");
      assert.ok(["logical", "content", "passage"].includes(h.id.space));
      assert.equal(p.arm, h.branch);
      assert.equal(p.ceScore, h.ceScore);
      assert.equal(p.blended, h.score);
      // fused_score is the RAW RRF value (not normalized to [0,1]).
      assert.ok(p.fusedScore > 0.0 && p.fusedScore < 1.0, "fusedScore is raw RRF");
    }

    // (5) None/Some rank fidelity (no embedder ⇒ vectorRank null, textRank set).
    const ranks = exp.perHit.flatMap((p) => [p.vectorRank, p.textRank, p.graphRank]);
    assert.ok(ranks.some((r) => r === null), "expected at least one null rank");
    assert.ok(ranks.some((r) => typeof r === "number"), "expected at least one int rank");
  } finally {
    await engine.close();
  }
});

test("f9 importance/confidence survive the FFI", async () => {
  // (7) F9 (0.8.16 Slice 5): the additive importance/confidence fields survive
  // the compiled FFI + TS wrapper. This is the R-X-1 gap the 0.8.8 harness
  // predated. There is no public SDK seam to enable the OFF-by-default reweight
  // or to write node importance, so the default path is null for both — which
  // still proves the field crossed the boundary (do NOT invent a seam).
  const engine = await Engine.open(freshDbPath());
  try {
    await seed(engine);
    const result = await searchAfterProjection(engine, "hybrid", true);
    const exp = result.explanation as Explanation;
    assert.notEqual(exp, null);
    assert.ok(exp.perHit.length > 0, "expected at least one per-hit explain");
    for (const p of exp.perHit) {
      // Fields EXIST on the object that came back across the FFI.
      assert.ok("importance" in p, "importance present on per-hit explain");
      assert.ok("confidence" in p, "confidence present on per-hit explain");
      // Default (F9-off) path: graceful-absent / neutral === null.
      assert.equal(p.importance, null);
      assert.equal(p.confidence, null);
      // When present they are numbers (typed contract, symmetric with Python).
      assert.ok(p.importance === null || typeof p.importance === "number");
      assert.ok(p.confidence === null || typeof p.confidence === "number");
    }
  } finally {
    await engine.close();
  }
});

test("graph_arm is assignable to SoftFallbackBranch", () => {
  // (6) The Slice-10 contract: graph_arm is a valid branch on every binding.
  const arm: SoftFallbackBranch = "graph_arm";
  assert.equal(arm, "graph_arm");
});

test("explain must be a boolean (cross-SDK parity guard)", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await assert.rejects(
      // @ts-expect-error — exercising the runtime type guard
      () => engine.search("hybrid", undefined, undefined, undefined, undefined, undefined, "yes"),
      TypeError,
    );
  } finally {
    await engine.close();
  }
});
