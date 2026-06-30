// 0.8.8 Slice 15 (OPP-9) telemetry-capture parity (TypeScript SDK).
//
// Mirrors the engine contract in
// `src/rust/crates/fathomdb-engine/tests/telemetry_capture.rs`: telemetry is
// off by default (no captured queryId; the feedback API rejects), an opt-in
// local JSONL sink records a query→result event keyed on the stable id with a
// deterministic sequential queryId (`q0-0`, `q0-1` …), a correlated agent-
// feedback row is appended, and the privacy guarantees hold — the query TEXT
// and `sourceId` are NEVER written to the sink. The Python half is
// `src/python/tests/test_telemetry_parity.py` (same contract, same corpus).

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { Engine } from "../src/index.js";
import type { SearchHit } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

// FTS-only corpus. Both query words ("hybrid", "retrieval") must NOT be
// substrings of any JSONL key the sink emits so the privacy assertions are
// meaningful.
const CORPUS = [
  { kind: "doc", body: "hybrid retrieval alpha" },
  { kind: "doc", body: "hybrid retrieval beta" },
];

async function searchAfterProjection(
  engine: Engine,
  query: string,
): Promise<SearchHit[]> {
  const deadline = Date.now() + 10_000;
  let last: SearchHit[] = [];
  while (Date.now() < deadline) {
    const result = await engine.search(query);
    last = result.results;
    if (last.length > 0) {
      return last;
    }
    await new Promise((r) => setTimeout(r, 20));
  }
  return last;
}

async function seed(engine: Engine): Promise<void> {
  for (const doc of CORPUS) {
    await engine.write([doc]);
  }
  await engine.drain(30_000);
}

test("telemetry: off by default (no captured id; feedback rejects)", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await seed(engine);
    const hits = await searchAfterProjection(engine, "hybrid");
    assert.ok(hits.length > 0, "expected hits");
    // Off by default: no captured query id...
    assert.equal(engine.lastTelemetryQueryId(), null);
    // ...and the feedback API rejects when telemetry is off.
    await assert.rejects(() =>
      engine.recordFeedback("q0-0", [hits[0].id], [], "agent:test"),
    );
  } finally {
    await engine.close();
  }
});

test("telemetry: captures event + correlated feedback deterministically", async () => {
  const dbPath = freshDbPath();
  const sink = join(dirname(dbPath), "telemetry.jsonl");
  const engine = await Engine.open(dbPath);
  try {
    await seed(engine);
    // Warm projection BEFORE enabling telemetry so each post-enable search is a
    // single-shot deterministic capture (a poll loop would capture extras).
    const warm = await searchAfterProjection(engine, "hybrid");
    assert.ok(warm.length > 0, "projection should be ready before enabling");

    await engine.enableTelemetry(sink);

    // First captured query → deterministic id "q0-0".
    const r0 = await engine.search("hybrid");
    assert.ok(r0.results.length > 0, "expected hits to capture");
    assert.equal(engine.lastTelemetryQueryId(), "q0-0");
    // Second query → "q0-1" (deterministic sequential id).
    await engine.search("retrieval");
    assert.equal(engine.lastTelemetryQueryId(), "q0-1");

    // Attach agent feedback correlated to the first query.
    await engine.recordFeedback("q0-0", [r0.results[0].id], [], "agent:test");
  } finally {
    await engine.close();
  }

  const body = readFileSync(sink, "utf-8");
  const lines = body.split("\n").filter((l) => l.length > 0);
  // 2 event rows + 1 feedback row.
  assert.equal(lines.length, 3, `expected 2 events + 1 feedback, got ${lines.length}`);

  const ev0 = JSON.parse(lines[0]);
  assert.equal(ev0.type, "event");
  assert.equal(ev0.query_id, "q0-0");
  assert.equal(ev0.schema_version, 1);
  assert.equal(ev0.query_chars, "hybrid".length);
  assert.ok(Array.isArray(ev0.result_ids) && ev0.result_ids.length > 0);
  assert.equal(typeof ev0.arm_of, "object");
  // Cause-A (0.8.11.2): NEW PARALLEL `result_stable_ids`, RETAINED `result_ids`.
  // Same length/order; doc-corpus hits carry the `"h:"` content-hash stable id.
  assert.ok(Array.isArray(ev0.result_stable_ids));
  assert.equal(ev0.result_stable_ids.length, ev0.result_ids.length);
  assert.ok(
    ev0.result_stable_ids.every((s: unknown) => typeof s === "string" && s.startsWith("h:")),
  );

  const fb = JSON.parse(lines[2]);
  assert.equal(fb.type, "feedback");
  assert.equal(fb.query_id, "q0-0");
  assert.equal(fb.label_source, "agent:test");

  // Privacy (ADR §C): the query TEXT never appears in the sink; only ids/length.
  assert.ok(!body.includes("hybrid"), "query text must NOT be captured");
  assert.ok(!body.includes("retrieval"), "query text must NOT be captured");
  // `source_id` is never a key in the sink (leak vector).
  assert.ok(!body.includes("source_id"), "source_id must NOT be captured");
});
