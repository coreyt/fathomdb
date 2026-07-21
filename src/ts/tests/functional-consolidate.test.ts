// 0.8.12 Slice B (R-X-1 TS-live) — TypeScript functional harness:
// `Engine.consolidateWithProvider` live end-to-end through the real native
// binding + a real BYO-LLM provider subprocess.
//
// Slice 15 wired the TS/napi `consolidateWithProvider` binding + surface test
// (`slice15-consolidate-surface.test.ts`, structural-only). Slice 40 verified
// the Python live path end-to-end but the live TS run was BUILD-GATED (no
// `node_modules`). This test completes the TS binding functional harness,
// mirroring the Rust reference (`fathomdb-engine/tests/consolidate_provider.rs`,
// `seed_competing_edges` + the inline `supersede` stub) and the Py-live X1 bar
// (Slice 40): a committed test that builds the real native binding, seeds a
// consolidatable cluster through the PUBLIC write API, spawns a real
// `fathomdb.consolidate.v1` provider subprocess, and asserts a verdict was
// actually APPLIED end-to-end (not just a typed-shape round-trip).
//
// $0/local, deterministic, no network at runtime (CALLER-SIDE BYO-LLM /
// OFFLINE-BUILD, ADR-0.8.12 §2.1 / R-CON-3): the "provider" is a local
// `python3 -c <script>` subprocess — no LLM, no randomness, no egress.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine, type ConsolidateAxis, type ConsolidateReceipt } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function openEngine(path: string): Promise<Engine> {
  return Engine.open(path, { useDefaultEmbedder: false });
}

// 0.8.20 (R-20-E3): `sourceId` is mandatory on every canonical write. Test
// fixtures carry a per-suite provenance id; it is inert for consolidation
// (which keys on the fact-edge axis, not on provenance).
const SOURCE_ID = "ts-test:functional-consolidate";

function nodeItem(logicalId: string, body: string, kind = "entity"): object {
  return { kind, body, logicalId, sourceId: SOURCE_ID };
}

function edgeItem(
  from: string,
  to: string,
  logicalId: string,
  body: string,
  tValid: number, // TC-33: INTEGER epoch seconds (UTC), not ISO-8601
): object {
  return { edge: { kind: "works_for", from, to, logicalId, body, tValid, sourceId: SOURCE_ID } };
}

/**
 * Seed one subject node (`bob`) plus two COMPETING `works_for` fact-edges on
 * the same `(subjectLogicalId="bob", relation="works_for")` axis: an older
 * `bob -> acme` (t_valid 2019) and a newer `bob -> globex` (t_valid 2022).
 * Mirrors `seed_competing_edges` in the Rust reference
 * (`fathomdb-engine/tests/consolidate_provider.rs`). Different `to` ids so
 * both stay active pre-consolidation (the G11 invalidate-not-accumulate
 * triple key does not collapse them).
 */
async function seedCompetingEdges(engine: Engine): Promise<void> {
  await engine.write([
    nodeItem("bob", "Bob (subject)"),
    nodeItem("acme", "Acme (org)"),
    nodeItem("globex", "Globex (org)"),
    edgeItem("bob", "acme", "edge-acme", "Bob works for Acme", 1_546_300_800), // 2019-01-01T00:00:00Z
    edgeItem("bob", "globex", "edge-globex", "Bob works for Globex", 1_640_995_200), // 2022-01-01T00:00:00Z
  ]);
}

/**
 * A REAL BYO-LLM provider subprocess speaking `fathomdb.consolidate.v1`
 * (NDJSON over stdio). Deterministic, local, no network: it supersedes the
 * older `edge-acme` (by `edge-globex`) and keeps `edge-globex` — mirroring
 * the Rust reference's inline `supersede_verdict_marks_superseded_row_survives`
 * harness. When `wrongProtocol` is true, the `ready` reply advertises a
 * mismatched protocol string so negotiation fails (used for the RED proof
 * only; never committed as the live path).
 */
function supersedeHarnessCmd(wrongProtocol = false): string[] {
  const protocolLiteral = wrongProtocol
    ? '"fathomdb.consolidate.WRONG"'
    : '"fathomdb.consolidate.v1"';
  const script = `
import json, sys
P = "fathomdb.consolidate.v1"
READY_P = ${protocolLiteral}
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("type") == "hello":
        print(json.dumps({"protocol": READY_P, "type": "ready", "schema_version": 1,
                          "model": "ts-live-stub-v1", "supported_tasks": ["consolidate"],
                          "max_docs_per_request": 8}), flush=True)
    elif msg.get("type") == "consolidate":
        edges = msg.get("cluster", {}).get("edges", [])
        verdicts = []
        for e in edges:
            ref = e.get("edge_ref")
            if ref == "edge-acme":
                verdicts.append({"edge_ref": ref, "verdict": "supersede", "by": "edge-globex"})
            else:
                verdicts.append({"edge_ref": ref, "verdict": "keep"})
        print(json.dumps({"protocol": P, "type": "result",
                          "request_id": msg.get("request_id"), "verdicts": verdicts}), flush=True)
`;
  return ["python3", "-c", script];
}

// ---------------------------------------------------------------------------
// R-X-1 TS-live — live end-to-end consolidate_with_provider round-trip
// ---------------------------------------------------------------------------

test("consolidateWithProvider: live end-to-end supersede round-trip through the real napi binding", async () => {
  const engine = await openEngine(freshDbPath());
  try {
    await seedCompetingEdges(engine);

    const cmd = supersedeHarnessCmd();
    const axes: ConsolidateAxis[] = [{ subjectLogicalId: "bob", relation: "works_for" }];

    const receipt: ConsolidateReceipt = await engine.consolidateWithProvider(cmd, axes);

    // Typed-shape assertions (parity with the surface test, but on a REAL
    // receipt returned by the live native round-trip).
    assert.equal(typeof receipt.clustersProcessed, "number");
    assert.equal(typeof receipt.edgesExamined, "number");
    assert.equal(typeof receipt.edgesKept, "number");
    assert.equal(typeof receipt.edgesInvalidated, "number");
    assert.equal(typeof receipt.edgesSuperseded, "number");

    // Non-vacuous: a verdict was actually APPLIED end-to-end. The stub rules
    // `supersede` on the stale edge and `keep` on the fresh one, so the
    // (subject, relation) cluster must have been dispatched and the
    // supersede verdict must have landed.
    assert.ok(
      receipt.clustersProcessed >= 1,
      `expected clustersProcessed >= 1, got ${receipt.clustersProcessed}`,
    );
    assert.ok(
      receipt.edgesSuperseded >= 1 || receipt.edgesInvalidated >= 1,
      `expected a stale-edge verdict to be applied (edgesSuperseded or edgesInvalidated >= 1); ` +
        `got edgesSuperseded=${receipt.edgesSuperseded}, edgesInvalidated=${receipt.edgesInvalidated}`,
    );
    assert.equal(receipt.edgesKept, 1, "the fresh edge must be kept");
    assert.equal(receipt.edgesExamined, 2, "two competing edges examined");
  } finally {
    await engine.close();
  }
});
