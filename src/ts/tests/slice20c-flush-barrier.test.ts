// X1 SDK parity — 0.8.20 Slice 20c (R-20-DR remainder): `drain` is the
// flush-to-readiness barrier (`api-surface.md` **C4**).
//
// Drives the barrier through the napi-rs binding by EXECUTION, not symbol
// presence. Mirrors
// `src/rust/crates/fathomdb-engine/tests/slice20c_flush_barrier.rs` and the
// Python suite `src/python/tests/test_slice20c_flush_barrier.py`
// (Py ≡ TS, R-X-1).
//
// The pinned invariant:
//
//     `await engine.drain(timeoutMs)` resolving ⟹ `vectorDenseReadiness ===
//     "ready"` AND every vector-eligible row has its vector row AT REST.
//
// Why the raw-table assertion is load-bearing: a harness that only reads
// readiness back PASSES against the defect. Before this slice
// `configureProjections` never enrolled the kind, so `drain` resolved
// immediately and readiness read `"ready"` with ZERO vectors and nothing that
// would ever create them.
//
// `node:sqlite` is used only as a READ oracle on a CLOSED database.
//
// These tests need a LIVE embedder (`useDefaultEmbedder: true`) because the
// dense arm is what is being flushed; they honour the standing
// `FATHOMDB_SKIP_NETWORK_TESTS` guard, exactly as `embedder-event-narrowing`
// does. The final test needs no embedder and therefore always runs.
//
// ZERO net-new governed commands: this rides the already-governed
// `configureProjections` / `read.projections` verbs plus the shipped
// `engine.drain` INSTRUMENTATION method (TC-55, steward seq-110).

import test from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";

import { Engine, read } from "../src/index.js";
import type { ProjectionSpec } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

const SOURCE = "ts-test:slice20c";
const DRAIN_TIMEOUT_MS = 120_000;

function node(logicalId: string, bodyJson: string): object {
  return { kind: "doc", body: bodyJson, logicalId, sourceId: SOURCE };
}

function vectorSpec(name = "summary"): ProjectionSpec {
  return { name, roles: ["searchable"], fts: false, vector: true };
}

async function readiness(engine: Engine, name = "summary"): Promise<string | null> {
  const specs = await read.projections(engine);
  const found = specs.find((s) => s.name === name);
  return found ? (found.vectorDenseReadiness ?? null) : null;
}

/**
 * READ-ONLY oracle against the LIVE database file.
 *
 * `{ readOnly: true }` is LOAD-BEARING, not tidiness. Unlike the shipped
 * Slice-20 harness — which only probes a CLOSED database — this suite reads
 * while the engine is open, and a third-party READ-WRITE `DatabaseSync` opened
 * and closed against a live WAL database WEDGES the engine's projection
 * pipeline: measured here, the post-declaration `drain` then burns its full
 * 120 s timeout into `SchedulerError` with the backfill never committing,
 * although the engine's own state was correct. Read-only observation reproduces
 * none of it. (The engine's exclusive hold is a lock FILE, not a SQLite lock, so
 * a read-only connection still sees committed WAL frames — `mode=ro`, never
 * `immutable=1`.)
 */
function count(path: string, sql: string): number {
  const db = new DatabaseSync(path, { readOnly: true });
  try {
    return Number((db.prepare(sql).get() as { c: number }).c);
  } finally {
    db.close();
  }
}

function vectorRows(path: string): number {
  return count(path, "SELECT COUNT(*) AS c FROM _fathomdb_vector_rows");
}

// NOTE: the `vector_default` (vec0) partition is NOT probed here. It is a
// VIRTUAL table provided by the engine-linked `vec0` extension, which
// `node:sqlite` cannot load (`no such module: vec0`). The shipped Slice-20
// harness has the same boundary. The Rust suite `slice20c_flush_barrier.rs`
// carries that second at-rest oracle; these bindings assert on
// `_fathomdb_vector_rows` — an ordinary table written in the SAME transaction as
// the vec0 INSERT (`commitProjectionOutcomes`) — plus the un-joined
// `leafRowsWithoutVectors` probe below.

/**
 * Vector-eligible node rows carrying NO vector row. Deliberately does NOT join
 * `_fathomdb_vector_kinds`: the defect IS that the declaration never enrolled
 * the kind, so a joined probe returns a hollow zero on the broken code.
 */
function leafRowsWithoutVectors(path: string): number {
  return count(
    path,
    "SELECT COUNT(*) AS c FROM canonical_nodes n" +
      " LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = n.write_cursor" +
      " WHERE n.row_kind IN ('leaf', 'coverage') AND v.write_cursor IS NULL",
  );
}

function vectorKindRegistered(path: string, kind = "doc"): boolean {
  return (
    count(
      path,
      `SELECT COUNT(*) AS c FROM _fathomdb_vector_kinds WHERE kind = '${kind}'`,
    ) > 0
  );
}

function skipNetwork(): boolean {
  if (process.env.FATHOMDB_SKIP_NETWORK_TESTS) {
    console.log("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; skipping default-embedder test");
    return true;
  }
  return false;
}

test("declaring a vector projection backfills pre-existing rows and drain flushes to ready", async () => {
  if (skipNetwork()) return;
  const path = freshDbPath();
  const engine = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    await engine.write(
      [0, 1, 2, 3].map((i) => node(`N${i}`, `{"summary":"dense meaning ${i}"}`)),
    );
    await engine.drain(DRAIN_TIMEOUT_MS);

    // Fixture preconditions, asserted rather than assumed.
    assert.equal(vectorKindRegistered(path), false, "fixture: `doc` is not yet a vector kind");
    assert.equal(vectorRows(path), 0, "fixture: no vectors exist yet");

    const delta = await engine.configureProjections([vectorSpec()]);
    assert.ok(delta.deferred.includes("summary"), "the vector sub-target is deferred work");

    // `drain` is the flush-to-readiness barrier.
    await engine.drain(DRAIN_TIMEOUT_MS);
    assert.equal(await readiness(engine), "ready", "after drain the dense arm is caught up");
  } finally {
    await engine.close();
  }

  // …and `ready` must be BACKED BY VECTORS AT REST. This is the assertion the
  // defect fails: 0 rows, forever.
  assert.equal(vectorKindRegistered(path), true, "the declaration enrolled the vector kind");
  assert.equal(vectorRows(path), 4, "every pre-existing row was backfilled");
  assert.equal(leafRowsWithoutVectors(path), 0);
});

test("write-after-declare also reaches ready with vectors at rest", async () => {
  if (skipNetwork()) return;
  const path = freshDbPath();
  const engine = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    await engine.configureProjections([vectorSpec()]);
    await engine.drain(DRAIN_TIMEOUT_MS);
    assert.equal(await readiness(engine), "ready", "an empty corpus has nothing outstanding");

    await engine.write([node("N1", '{"summary":"written after declaring"}')]);
    await engine.drain(DRAIN_TIMEOUT_MS);
    assert.equal(await readiness(engine), "ready");
  } finally {
    await engine.close();
  }

  assert.equal(vectorRows(path), 1, "the post-declaration write embedded");
  assert.equal(leafRowsWithoutVectors(path), 0);
});

test("re-applying a satisfied vector declaration is an idempotent no-op", async () => {
  if (skipNetwork()) return;
  const path = freshDbPath();
  const engine = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    await engine.write([node("N1", '{"summary":"a dense meaning"}')]);
    await engine.configureProjections([vectorSpec()]);
    await engine.drain(DRAIN_TIMEOUT_MS);
    assert.equal(await readiness(engine), "ready");
    const before = vectorRows(path);
    assert.equal(before, 1);

    const again = await engine.configureProjections([vectorSpec()]);
    assert.equal(again.unchanged, true, "an identical re-apply diffs to a no-op");
    // Read readiness BEFORE any drain: a spurious re-enqueue would show here.
    assert.equal(
      await readiness(engine),
      "ready",
      "an idempotent re-apply must not re-open the backfill",
    );

    await engine.drain(DRAIN_TIMEOUT_MS);
    assert.equal(vectorRows(path), before, "no row was re-embedded");
  } finally {
    await engine.close();
  }
});

test("a declaration without a live embedder defers and does not enrol the kind", async () => {
  // No embedder ⇒ no dense arm, so the declaration persists and DEFERS (Q6a
  // graceful-absent, exactly like `rankable`) rather than queueing embeds that
  // could only fail. Needs no network, so this arm always runs.
  const path = freshDbPath();
  const engine = await Engine.open(path, { useDefaultEmbedder: false });
  try {
    await engine.write([node("N1", '{"summary":"a dense meaning"}')]);
    await engine.configureProjections([vectorSpec()]);
    assert.equal(
      await readiness(engine),
      "ready",
      "no live embedder ⇒ no dense arm ⇒ nothing outstanding",
    );
    await engine.drain(5_000);
    assert.equal(await readiness(engine), "ready");
  } finally {
    await engine.close();
  }

  assert.equal(vectorKindRegistered(path), false, "a dead dense arm must not enrol the kind");
  assert.equal(
    count(
      path,
      "SELECT COUNT(*) AS c FROM operational_mutations" +
        " WHERE collection_name = 'projection_failures'",
    ),
    0,
    "no doomed embeds may be queued, so no projection_failures audit rows",
  );
});

test("dropping the last vector projection un-enrols the kind and stops embedding", async () => {
  // fix-1 (codex §9 [P2]) — the SYMMETRIC INVERSE, through the binding.
  //
  // Slice 20c gave `_fathomdb_vector_kinds` its first governed-call-reachable
  // enrolment path for a node kind. Without an inverse, dropping the last
  // `searchable→vector` declaration leaves the kind enrolled, so subsequent
  // writes keep embedding for a projection `read.projections` no longer reports.
  //
  // The un-enrolment is NON-DESTRUCTIVE and both halves are pinned here: the
  // kind stops being enrolled, and the vectors already at rest are untouched
  // (the shipped `drop` arm has never deleted an embedding). Re-declaring
  // re-enrols and backfills, so nothing is stranded.
  if (skipNetwork()) return;
  const path = freshDbPath();
  const engine = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    await engine.write([node("N1", '{"summary":"a dense meaning"}')]);
    await engine.configureProjections([vectorSpec()]);
    await engine.drain(DRAIN_TIMEOUT_MS);
    assert.equal(await readiness(engine), "ready");
    assert.equal(vectorKindRegistered(path), true, "fixture: the declaration enrolled `doc`");
    assert.equal(vectorRows(path), 1, "fixture: N1 is embedded");

    // ---- drop the LAST `searchable→vector` declaration ----
    const delta = await engine.configureProjections([], ["summary"]);
    assert.ok(delta.dropped.includes("summary"), "the drop is reported");
    assert.equal(await readiness(engine), null, "the projection is gone from the registry");

    assert.equal(
      vectorKindRegistered(path),
      false,
      "ONE-WAY ENROLMENT: dropping the last `searchable→vector` declaration must un-enrol the " +
        "node kind it enrolled",
    );
    assert.equal(
      vectorRows(path),
      1,
      "un-enrolment must NOT delete embeddings — the shipped `drop` arm leaves vectors at rest",
    );

    // ---- a write of the SAME kind after the drop embeds nothing ----
    await engine.write([node("N2", '{"summary":"written after the drop"}')]);
    await engine.drain(DRAIN_TIMEOUT_MS);
    assert.equal(vectorRows(path), 1, "a write after the drop must not be embedded");
    assert.equal(leafRowsWithoutVectors(path), 1, "N2 is the one un-embedded row");

    // ---- re-declaring re-enrols and backfills: the inverse is reversible ----
    await engine.configureProjections([vectorSpec()]);
    await engine.drain(DRAIN_TIMEOUT_MS);
    assert.equal(await readiness(engine), "ready");
    assert.equal(vectorKindRegistered(path), true, "re-declaring re-enrols the kind");
    assert.equal(vectorRows(path), 2, "the row written while the arm was off is backfilled");
    assert.equal(leafRowsWithoutVectors(path), 0);
  } finally {
    await engine.close();
  }
});
