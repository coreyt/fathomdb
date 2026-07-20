// 0.8.20 Slice 10a (TC-31) — `sourceId` is readable on EVERY search hit, so the
// value `eraseSource` consumes is reachable from any hit a caller gets.
//
// The TypeScript arm of the TC-31 contract, mirroring
// `src/python/tests/test_tc31_source_id_every_hit.py` exactly (Py ≡ TS, R-X-1).
//
// The defect this closes: 0.8.20 made provenance structurally mandatory on write
// and shipped `eraseSource` as the SDK erasure verb, but `SearchHit.sourceId`
// was populated by the GRAPH ARM only. Every text/BM25F, vector and edge-FTS hit
// carried `null`, so a consumer holding a text hit and a deletion obligation
// could not name the source to erase. 0.8.19 had also stopped surfacing
// `writeCursor` to the SDKs, so no fallback route from hit → document existed.
//
// Test-design contract (design §3, Rule 1): an erasure witness must NOT be a
// `search()` call — both read paths gate on `canonical_nodes`, so a search
// assertion passes on the broken code. The witnesses here are the returned
// report counts. The RAW-TABLE erasure witnesses live in
// `src/rust/crates/fathomdb-engine/tests/tc31_source_id_on_every_hit.rs`. What
// THIS file proves is the binding-level contract: the value actually arrives in
// JS and is accepted by `eraseSource`.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

function node(body: string, sourceId: string, logicalId?: string) {
  const n: Record<string, unknown> = { kind: "doc", body, sourceId };
  if (logicalId !== undefined) n.logicalId = logicalId;
  return n;
}

function edge(from: string, to: string, logicalId: string, sourceId: string, body: string) {
  return { edge: { kind: "link", from, to, logicalId, sourceId, body } };
}

test("text hit exposes a sourceId that eraseSource accepts", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([
      node("tc31tstext confidential dossier", "tenant-a"),
      node("tc31tstext unrelated retained", "tenant-b"),
    ]);

    const result = await engine.search("tc31tstext");
    const hit = result.results.find((h) => h.body.includes("confidential"));
    assert.ok(hit, "the text arm must surface the document");

    assert.equal(
      hit.sourceId,
      "tenant-a",
      "TC-31: a text hit must carry its own sourceId, not null",
    );

    // The whole point: the value read off the hit is the erasure key.
    const report = await engine.eraseSource(hit.sourceId!);
    assert.equal(report.sourceRef, "tenant-a");
    assert.equal(report.nodesExcised, 1);

    // Non-perturbation, asserted as a SECOND erase (Rule 1: not a search).
    const second = await engine.eraseSource("tenant-b");
    assert.equal(second.nodesExcised, 1, "the first erasure must not have touched tenant-b");
  } finally {
    await engine.close();
  }
});

test("every hit carries its own sourceId, not a shared constant", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([
      node("tc31tsshared marker alpha", "tenant-a"),
      node("tc31tsshared marker beta", "tenant-b"),
    ]);

    const result = await engine.search("tc31tsshared");
    const alpha = result.results.find((h) => h.body.includes("alpha"));
    const beta = result.results.find((h) => h.body.includes("beta"));
    assert.ok(alpha, "alpha must be retrievable");
    assert.ok(beta, "beta must be retrievable");

    assert.equal(alpha.sourceId, "tenant-a", "each hit reports its OWN provenance");
    assert.equal(beta.sourceId, "tenant-b", "each hit reports its OWN provenance");

    assert.equal((await engine.eraseSource("tenant-a")).nodesExcised, 1);
    assert.equal((await engine.eraseSource("tenant-b")).nodesExcised, 1);
  } finally {
    await engine.close();
  }
});

test("edge hit exposes the edge's own sourceId", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([
      node("anna the first entity", "tenant-e", "anna"),
      node("bob the second entity", "tenant-e", "bob"),
      edge("anna", "bob", "edge-ab", "tenant-e", "tc31tsedge anna trusts bob"),
    ]);

    const result = await engine.search("tc31tsedge");
    const hit = result.results.find((h) => h.body.includes("tc31tsedge"));
    assert.ok(hit, "the edge-FTS arm must surface the edge body");
    assert.equal(
      hit.sourceId,
      "tenant-e",
      "TC-31: an edge hit must carry the edge's own sourceId",
    );

    const report = await engine.eraseSource(hit.sourceId!);
    assert.equal(report.sourceRef, "tenant-e");
  } finally {
    await engine.close();
  }
});

test("graph-arm hit sourceId semantics are unchanged by TC-31", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([
      node("carol tc31tsanchor entity", "tenant-g", "carol"),
      node("tc31tsgraph dave neighbor", "tenant-g", "dave"),
      edge("carol", "dave", "edge-cd", "tenant-g", "carol knows dave"),
    ]);

    const result = await engine.search("tc31tsanchor", undefined, undefined, true);
    const reached = result.results.find((h) => h.body.includes("tc31tsgraph"));
    assert.ok(reached, "dave must be graph-reached from the carol seed");
    assert.equal(reached.sourceId, "tenant-g");

    const report = await engine.eraseSource(reached.sourceId!);
    assert.equal(report.sourceRef, "tenant-g");
  } finally {
    await engine.close();
  }
});
