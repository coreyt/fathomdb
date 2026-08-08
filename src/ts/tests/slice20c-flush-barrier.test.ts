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
// does. The "declaration without a live embedder" test needs no embedder and
// therefore always runs.
//
// ZERO net-new governed commands: this rides the already-governed
// `configureProjections` / `read.projections` verbs plus the shipped
// `engine.drain` INSTRUMENTATION method (TC-55, steward seq-110).

import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { DatabaseSync } from "node:sqlite";

import { Engine, read } from "../src/index.js";
import { SchedulerError } from "../src/errors.js";
import type { ProjectionSpec } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

const SOURCE = "ts-test:slice20c";
const DRAIN_TIMEOUT_MS = 120_000;
// fix-2: the arms whose FAILURE MODE is a wedged projection worker use a shorter
// barrier — an assertion that "`drain` did not hang" should not cost two minutes
// per arm when it is red.
const WEDGE_TIMEOUT_MS = 30_000;
// fix-4: the engine's projection retry ladder is 0 + 1 + 4 + 16 s. The
// no-embedder arm waits it out so its terminal/audit probes are FALSIFYING at
// baseline rather than merely early.
const LADDER_SETTLE_MS = 24_000;

const READ_ONLY_SQL_CHILD = `
import { DatabaseSync } from "node:sqlite";

const [path, sql, parametersJson] = process.argv.slice(1);
const db = new DatabaseSync(path, { readOnly: true });
try {
  const row = db.prepare(sql).get(...JSON.parse(parametersJson));
  console.log(JSON.stringify(row === undefined ? null : Object.values(row)[0]));
} finally {
  db.close();
}
`;

type ReadOnlySqlScalar = number | string | null;

/**
 * Read a single scalar through a separate Node process.
 *
 * The native engine's projection worker can commit while this suite probes the
 * database. `node:sqlite` shares process-global SQLite state with the native
 * addon, so even a `readOnly` handle in this process can crash under that
 * overlap. An exec'd child preserves the mode=ro oracle while isolating that
 * SQLite state from the engine.
 */
function readOnlyScalar(
  path: string,
  sql: string,
  parameters: readonly (number | string)[] = [],
): ReadOnlySqlScalar {
  const result = spawnSync(
    process.execPath,
    ["--input-type=module", "--eval", READ_ONLY_SQL_CHILD, path, sql, JSON.stringify(parameters)],
    { encoding: "utf8" },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`isolated read-only SQLite query failed: ${result.stderr}`);
  }

  const scalar: unknown = JSON.parse(result.stdout);
  if (scalar === null || typeof scalar === "number" || typeof scalar === "string") {
    return scalar;
  }
  throw new TypeError(`unexpected read-only SQLite scalar type: ${typeof scalar}`);
}

function node(logicalId: string, bodyJson: string): object {
  return nodeOfKind("doc", logicalId, bodyJson);
}

function nodeOfKind(kind: string, logicalId: string, bodyJson: string): object {
  return { kind, body: bodyJson, logicalId, sourceId: SOURCE };
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
 * while the engine is open. The query runs in `readOnlyScalar`'s exec'd child:
 * a same-process `node:sqlite` handle can crash while the native projection
 * worker commits. (The engine's exclusive hold is a lock FILE, not a SQLite
 * lock, so a read-only connection still sees committed WAL frames — `mode=ro`,
 * never `immutable=1`.)
 */
function count(path: string, sql: string): number {
  const scalar = readOnlyScalar(path, sql);
  if (typeof scalar !== "number") {
    throw new TypeError(`expected integer raw SQLite result, got ${String(scalar)}`);
  }
  return scalar;
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

/**
 * The same un-joined at-rest probe, narrowed to one `canonical_nodes.kind`.
 * fix-2 needs the narrowed form because its fixture deliberately holds a kind
 * that gets NO dense arm, so the corpus-wide count is legitimately non-zero. It
 * still does not join `_fathomdb_vector_kinds`.
 */
function leafRowsOfKindWithoutVectors(path: string, kind: string): number {
  return count(
    path,
    "SELECT COUNT(*) AS c FROM canonical_nodes n" +
      " LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = n.write_cursor" +
      " WHERE n.row_kind IN ('leaf', 'coverage')" +
      ` AND n.kind = '${kind}' AND v.write_cursor IS NULL`,
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

function activeCursor(path: string, logicalId: string): number {
  return count(
    path,
    "SELECT write_cursor AS c FROM canonical_nodes" +
      ` WHERE logical_id = '${logicalId}' AND superseded_at IS NULL`,
  );
}

/**
 * The raw `_fathomdb_projection_terminal` state for one cursor.
 *
 * `null` is PENDING, and that is the fix-4 property: an ABSENT embedder is an
 * ENVIRONMENT fact, not an embed failure, so it must record NO terminal. A
 * `'failed'` terminal is permanent by design (nothing reopens one, and nothing
 * should — that would loop a genuinely-failing row forever), so recording one
 * here LOSES the write.
 */
function terminalState(path: string, cursor: number): string | null {
  const scalar = readOnlyScalar(
    path,
    "SELECT state FROM _fathomdb_projection_terminal WHERE write_cursor = ?",
    [cursor],
  );
  assert.ok(
    scalar === null || typeof scalar === "string",
    `unexpected terminal state: ${String(scalar)}`,
  );
  return scalar;
}

function ftsRowExists(path: string, cursor: number): boolean {
  return count(path, `SELECT COUNT(*) AS c FROM search_index WHERE write_cursor = ${cursor}`) > 0;
}

function projectionFailureRows(path: string): number {
  return count(
    path,
    "SELECT COUNT(*) AS c FROM operational_mutations" +
      " WHERE collection_name = 'projection_failures'",
  );
}

function skipNetwork(): boolean {
  if (process.env.FATHOMDB_SKIP_NETWORK_TESTS) {
    console.log("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; skipping default-embedder test");
    return true;
  }
  return false;
}

test("the isolated raw SQLite oracle preserves scalar types and parameters", () => {
  const path = freshDbPath();
  const db = new DatabaseSync(path);
  try {
    db.exec("CREATE TABLE oracle (state TEXT, is_ready INTEGER)");
    db.exec("INSERT INTO oracle VALUES ('ready', 1)");
  } finally {
    db.close();
  }

  assert.equal(readOnlyScalar(path, "SELECT state FROM oracle"), "ready");
  assert.equal(readOnlyScalar(path, "SELECT is_ready FROM oracle"), 1);
  assert.equal(
    readOnlyScalar(path, "SELECT state FROM oracle WHERE state = ?", ["missing"]),
    null,
  );
});

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
  // No embedder ⇒ no usable dense runtime. The declaration persists and DEFERS
  // (Q6a graceful-absent, exactly like `rankable`) rather than queueing embeds
  // that could only fail. Needs no network, so this arm always runs.
  const path = freshDbPath();
  const engine = await Engine.open(path, { useDefaultEmbedder: false });
  try {
    await engine.write([node("N1", '{"summary":"a dense meaning"}')]);
    await engine.configureProjections([vectorSpec()]);
    assert.equal(
      await readiness(engine),
      "unavailable",
      "no live embedder makes the declared dense projection unavailable, even with no work",
    );
    await engine.drain(5_000);
    assert.equal(
      await readiness(engine),
      "unavailable",
      "a no-runtime drain cannot establish dense readiness",
    );
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

test("a kind the vector writer cannot commit gets no dense arm", async () => {
  // fix-2 (codex §9 [P1]) — enrolment is restricted to COMMIT-ABLE kinds.
  //
  // The engine maps a node `kind` onto a locked `source_type` partition-key
  // vocabulary before it can commit a vector, and `write` accepts any non-empty
  // `kind`. Enrolling a kind outside that vocabulary made the scheduler pick the
  // row up while the commit could never record a terminal: the row stayed pending
  // forever, `drain` burned its whole timeout, and `vectorDenseReadiness` stuck on
  // `"embedding"` — starving the rows whose kinds ARE commit-able.
  //
  // Both enrolment doors are exercised: `invoice` present in the corpus at
  // DECLARATION time, and a second `invoice` written AFTER the declaration (the
  // write-path late-enrolment door).
  //
  // The un-enrolled kind is not an error: nothing rejects it, nothing throws, and
  // there is no verb to ask about it. It just gets no vector.
  if (skipNetwork()) return;
  const path = freshDbPath();
  const engine = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    await engine.write([
      node("N1", '{"summary":"a dense meaning"}'),
      nodeOfKind("invoice", "I1", '{"summary":"payable in 30 days"}'),
    ]);
    await engine.drain(WEDGE_TIMEOUT_MS);

    // ---- declare-time enrolment ----
    await engine.configureProjections([vectorSpec()]);
    await engine.drain(WEDGE_TIMEOUT_MS); // WEDGES at fix-2 baseline
    assert.equal(
      await readiness(engine),
      "ready",
      'a kind the engine cannot commit a vector for must not hold the whole corpus in "embedding"',
    );

    // ---- late (write-path) enrolment ----
    await engine.write([nodeOfKind("invoice", "I2", '{"summary":"also payable"}')]);
    await engine.drain(WEDGE_TIMEOUT_MS); // WEDGES at fix-2 baseline
    assert.equal(await readiness(engine), "ready");
  } finally {
    await engine.close();
  }

  assert.equal(vectorKindRegistered(path, "doc"), true, "the commit-able kind keeps its dense arm");
  assert.equal(leafRowsOfKindWithoutVectors(path, "doc"), 0, "…and its row is embedded");
  assert.equal(
    vectorKindRegistered(path, "invoice"),
    false,
    "ENROLMENT MUST BE RESTRICTED TO COMMIT-ABLE KINDS",
  );
  assert.equal(vectorRows(path), 1, "exactly the one commit-able row was embedded");
  assert.equal(leafRowsOfKindWithoutVectors(path, "invoice"), 2, "no dense arm for that kind");
  assert.equal(
    count(
      path,
      "SELECT COUNT(*) AS c FROM operational_mutations" +
        " WHERE collection_name = 'projection_failures'",
    ),
    0,
    "a kind with no dense arm is not a FAILURE — it must not pollute the failure audit",
  );
});

test("late enrolment backfills the rows a no-embedder session stranded", async () => {
  // fix-2 (codex §9 [P2]) — a LATE enrolment owes the same backfill.
  //
  // A database persists a `searchable→vector` declaration while opened WITHOUT an
  // embedder (it defers, enrolling nothing), then reopens WITH one and writes the
  // same kind BEFORE re-applying the projection. The write enrols the kind — and
  // used to enqueue only its OWN row, leaving every row from the no-embedder
  // session holding a permanent terminal with no vector. After the flush,
  // readiness reported `"ready"` with pre-existing vector-eligible rows
  // unembedded: a FALSE READY, the exact defect class R-20-DR exists to eliminate.
  if (skipNetwork()) return;
  const path = freshDbPath();

  // ---- session 1: no embedder. The declaration persists and DEFERS. ----
  const cold = await Engine.open(path, { useDefaultEmbedder: false });
  try {
    await cold.configureProjections([vectorSpec()]);
    await cold.write([
      node("N1", '{"summary":"stranded one"}'),
      node("N2", '{"summary":"stranded two"}'),
    ]);
    await cold.drain(5_000);
    assert.equal(vectorKindRegistered(path), false, "fixture: a dead dense arm enrols nothing");
    assert.equal(leafRowsWithoutVectors(path), 2, "fixture: two rows are stranded");
  } finally {
    await cold.close();
  }

  // ---- session 2: SAME database, now WITH an embedder. The projection is NOT
  // re-applied — the WRITE is what turns the dense arm on. ----
  const warm = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    await warm.write([node("N3", '{"summary":"written before re-applying"}')]);
    assert.equal(vectorKindRegistered(path), true, "the write LATE-ENROLLED the kind");
    await warm.drain(DRAIN_TIMEOUT_MS);
    assert.equal(await readiness(warm), "ready");
  } finally {
    await warm.close();
  }

  assert.equal(
    leafRowsWithoutVectors(path),
    0,
    'FALSE-READY: `drain` resolved and readiness reads "ready", but rows written in the ' +
      "no-embedder session still have no vector at rest",
  );
  assert.equal(vectorRows(path), 3, "all three rows were embedded");
});

test("a no-embedder session leaves an enrolled kind's write recoverable", async () => {
  // fix-4 (codex §9 round 3 [P1]) — a write made with NO live embedder, over an
  // ALREADY-ENROLLED kind, must stay RECOVERABLE.
  //
  // fix-2 gated ENROLMENT on a live embedder, but `_fathomdb_vector_kinds` is
  // durable: once an embedder-backed session has enrolled `doc`, every later
  // session sees the enrolment. Reopening with no embedder and writing that kind
  // therefore still enqueues. At baseline the worker exhausted its retry ladder
  // against the absent embedder and recorded an `EmbedderNotConfiguredError`
  // `'failed'` terminal plus a `projection_failures` audit row — and since no
  // path reopens a `'failed'` terminal, reopening WITH an embedder left that
  // write PERMANENTLY unembedded while readiness reported `"ready"`.
  //
  // Three sessions, no re-apply and no second write: the ORDINARY scheduler is
  // the whole recovery path.
  //
  // The CONSUMER-VISIBLE consequence is asserted on purpose, so it cannot change
  // back silently: with no usable dense runtime, readiness reads `"unavailable"`
  // regardless of outstanding work. The pending row remains recoverable and
  // `drain` rejects with `SchedulerError`, rather than recording a failed terminal.
  if (skipNetwork()) return;
  const path = freshDbPath();

  // ---- session 1: WITH an embedder. This durably ENROLS `doc`. ----
  const first = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    await first.configureProjections([vectorSpec()]);
    await first.write([node("N1", '{"summary":"embedded in session one"}')]);
    await first.drain(DRAIN_TIMEOUT_MS);
    assert.equal(vectorKindRegistered(path), true, "fixture: session 1 enrolled `doc`");
    assert.equal(leafRowsWithoutVectors(path), 0, "fixture: session 1's row is embedded");
  } finally {
    await first.close();
  }

  const c1 = activeCursor(path, "N1");

  // ---- session 2: the SAME database, reopened with NO embedder. ----
  const cold = await Engine.open(path, { useDefaultEmbedder: false });
  try {
    // Fixture precondition, asserted rather than assumed: the enrolment
    // PERSISTED, so this session's write reaches the vector pipeline.
    assert.equal(
      vectorKindRegistered(path),
      true,
      "fixture: the kind stays enrolled across the reopen — that is the whole finding",
    );
    await cold.write([node("N2", '{"summary":"written with no dense arm"}')]);

    // THE CONSUMER-VISIBLE CONSEQUENCE. An enrolled row with no vector is
    // outstanding and this session cannot satisfy it, so the barrier must NOT
    // clear. At baseline it cleared by recording a `'failed'` terminal — which
    // is exactly how the write got lost.
    await assert.rejects(() => cold.drain(3_000), SchedulerError);

    // Give the BASELINE its full retry ladder (0 + 1 + 4 + 16 s) before the
    // probes below. Without this wait they all read a merely-unterminated row
    // and pass VACUOUSLY on the broken code — the SDKs expose no equivalent of
    // the Rust suite's `set_projection_retry_delays_for_test` seam, so waiting
    // is the only way to make them falsifying. Under the fix the row is never
    // dispatched, so this is dead time and nothing changes across it.
    await new Promise((resolve) => setTimeout(resolve, LADDER_SETTLE_MS));

    assert.equal(
      await readiness(cold),
      "unavailable",
      "an absent runtime is unavailable even when an enrolled row remains recoverably pending",
    );

    const c2 = activeCursor(path, "N2");
    assert.equal(ftsRowExists(path, c2), true, "the write is accepted and lexically searchable");
    assert.equal(
      projectionFailureRows(path),
      0,
      "an ABSENT embedder is an ENVIRONMENT fact, not an embed failure — it must not pollute " +
        "the `projection_failures` audit",
    );
    assert.equal(
      terminalState(path, c2),
      null,
      "PERMANENTLY LOST WRITE: an absent embedder must record NO terminal. Leaving the row " +
        "PENDING is what lets the next live-embedder session's ORDINARY scheduler pick it up",
    );
    assert.equal(terminalState(path, c1), "up_to_date", "session 1's row is untouched");
    assert.equal(vectorRows(path), 1, "fixture: no dense arm ⇒ no new vector yet");
  } finally {
    await cold.close();
  }

  // ---- session 3: WITH an embedder again. NO re-apply, NO further write. ----
  const warm = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    await warm.drain(DRAIN_TIMEOUT_MS);
    assert.equal(await readiness(warm), "ready");
  } finally {
    await warm.close();
  }

  assert.equal(
    leafRowsWithoutVectors(path),
    0,
    'PERMANENTLY LOST WRITE: `drain` resolved and readiness reads "ready", but the row written ' +
      "in the no-embedder session still has no vector at rest",
  );
  assert.equal(vectorRows(path), 2, "the recovered row was embedded, and only it");
  assert.equal(projectionFailureRows(path), 0, "the recovery leaves no failure audit behind");
});

// ---------------------------------------------------------------------------
// fix-5 (codex §9 round 4) — the scheduler's SCAN WINDOW, and the ATOMICITY of a
// late enrolment
// ---------------------------------------------------------------------------

// `PROJECTION_SCAN_FETCH`, restated: the engine's dispatcher fetches at most
// `PROJECTION_WORKERS (2) * PROJECTION_COMMIT_BATCH (16)` jobs per scan, ordered
// by `write_cursor`. It is the width of the window a post-fetch filter can
// starve. The fixture asserts the window is genuinely exceeded rather than
// trusting this number.
const PROJECTION_SCAN_FETCH = 32;

/**
 * An edge carrying a BODY — the only edge shape that enrols `'edge_fact'` in
 * `_fathomdb_vector_kinds` (engine `project_canonical_edge_row`, G11) and
 * therefore the only one that is schedulable projection work.
 */
function edge(logicalId: string, from: string, to: string, body: string): object {
  return { edge: { kind: "link", from, to, logicalId, sourceId: SOURCE, body } };
}

function activeEdgeCursor(path: string, logicalId: string): number {
  return count(
    path,
    "SELECT write_cursor AS c FROM canonical_edges" +
      ` WHERE logical_id = '${logicalId}' AND superseded_at IS NULL`,
  );
}

function pendingNodeRowsBelow(path: string, cursor: number): number {
  return count(
    path,
    "SELECT COUNT(*) AS c FROM canonical_nodes n" +
      " JOIN _fathomdb_vector_kinds k ON k.kind = n.kind" +
      " LEFT JOIN _fathomdb_projection_terminal t ON t.write_cursor = n.write_cursor" +
      " WHERE n.row_kind IN ('leaf', 'coverage')" +
      "   AND n.superseded_at IS NULL" +
      "   AND t.write_cursor IS NULL" +
      `   AND n.write_cursor < ${cursor}`,
  );
}

async function pollUntil(timeoutMs: number, probe: () => boolean): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    if (probe()) return true;
    if (Date.now() >= deadline) return false;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

// fix-5 [P1] — a pending EDGE body must stay reachable behind a full scan window
// of no-embedder NODE rows.
//
// fix-4 excluded no-embedder node jobs by filtering what the scheduler's scan
// RETURNED — i.e. after its `ORDER BY write_cursor LIMIT PROJECTION_SCAN_FETCH`.
// The `LIMIT` therefore applied to the UNFILTERED set: with more than one window
// of pending node rows ordered before a pending edge body, the scan came back
// entirely full of node jobs, the filter dropped all of them, the dispatcher
// went back to sleep with its wake already consumed — and the edge body was
// NEVER scheduled. Permanently, because those node rows stay pending for the
// life of a no-embedder session, so `drain` could time out indefinitely on that
// edge workload.
//
// Fully OFFLINE: the kind is enrolled through the `test-hooks`-gated
// `configureVectorKindForTest` seam (the same one the shipped
// `slice20-dense-readiness.test.ts` fixture uses), so no embedder is ever
// downloaded. Edges keep their SHIPPED no-embedder behaviour, so "scheduled" is
// observable as the edge body reaching a TERMINAL; this test asserts only that
// the scheduler REACHES it.
test("a pending edge body survives a full scan window of node rows (offline)", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.configureProjections([vectorSpec()]);
    const inner = (engine as unknown as { _native: unknown })._native as {
      configureVectorKindForTest: (kind: string) => Promise<void>;
    };
    await inner.configureVectorKindForTest("doc");

    // MORE than one scan window of node rows, all ordered BEFORE the edge.
    const nodeRows = PROJECTION_SCAN_FETCH + 8;
    await engine.write(
      Array.from({ length: nodeRows }, (_, i) => node(`N${i}`, `{"summary":"row ${i}"}`)),
    );
    await engine.write([edge("E1", "N0", "N1", "the edge body that must not be starved")]);

    const edgeCursor = activeEdgeCursor(path, "E1");
    assert.equal(
      vectorKindRegistered(path, "edge_fact"),
      true,
      "fixture: an edge body auto-registers `edge_fact` (G11), so it IS schedulable work",
    );
    const pendingBefore = pendingNodeRowsBelow(path, edgeCursor);
    assert.ok(
      pendingBefore > PROJECTION_SCAN_FETCH,
      "fixture: the scan window must be over-subscribed by node rows ordered BEFORE the edge " +
        `body. Pending: ${pendingBefore}, window: ${PROJECTION_SCAN_FETCH}`,
    );

    // The shipped edge path with no embedder is the 0 + 1 + 4 + 16 s retry ladder
    // into a `'failed'` terminal; the SDKs expose no equivalent of the Rust
    // `set_projection_retry_delays_for_test` seam, so this waits it out.
    const scheduled = await pollUntil(
      LADDER_SETTLE_MS + 16_000,
      () => terminalState(path, edgeCursor) !== null,
    );
    assert.ok(
      scheduled,
      `SCAN-WINDOW STARVATION: with more than PROJECTION_SCAN_FETCH (${PROJECTION_SCAN_FETCH}) ` +
        `pending node rows ordered before it, the pending edge body at cursor ${edgeCursor} was ` +
        "NEVER scheduled. The no-embedder node exclusion must happen INSIDE the scheduler's SQL " +
        "so the LIMIT applies to the ALREADY-FILTERED set; filtering after the fetch lets one " +
        "full window of node rows hide every later job, and `drain` then times out on that edge " +
        "workload indefinitely",
    );
    assert.equal(
      terminalState(path, edgeCursor),
      "failed",
      "edges keep their shipped no-embedder behaviour: the retry ladder exhausts into a " +
        "`'failed'` terminal. fix-5 changes WHICH rows the scan returns, not what happens to an " +
        "edge once it is dispatched",
    );
    assert.equal(
      pendingNodeRowsBelow(path, edgeCursor),
      pendingBefore,
      "fix-4 stands: an absent embedder records NO terminal for a NODE row, so every one of " +
        "them is still pending and still recoverable by the next live-embedder session",
    );
  } finally {
    await engine.close();
  }
});

// fix-5 [P2] — the boot-graft registry INSERT and the un-stranding it owes must
// commit as ONE transaction.
//
// Safe boot graft registers the kind in `_fathomdb_vector_kinds` AND repairs the
// rows that kind stranded (delete their `'up_to_date'` terminals, rewind the
// watermark). If the INSERT committed ahead of the repair's transaction, a
// failure in between would leave the kind REGISTERED with older rows still
// holding their terminals and no vectors. That state is SELF-SEALING: later
// opens see the kind as registered and skip the graft, while the stranded rows
// remain invisible to recovery.
//
// The failure is injected as a real SQLite `BEFORE DELETE` trigger on
// `_fathomdb_projection_terminal` — the repair genuinely fails, against a real
// engine and a real database, at exactly the point a crash would truncate it.
// The READ-WRITE `DatabaseSync` is opened ONLY while no engine holds the file,
// so the live-WAL hazard documented on `count()` above is not in play. Nothing
// is mocked.
test("a boot graft whose repair fails registers nothing", async () => {
  if (skipNetwork()) return;
  const path = freshDbPath();

  // ---- session A: NO embedder. The declaration persists and DEFERS, so these
  // rows take permanent terminals with no vector: the stranded set.
  {
    const engine = await Engine.open(path);
    try {
      await engine.configureProjections([vectorSpec()]);
      await engine.write([node("N1", '{"summary":"stranded one"}')]);
      await engine.write([node("N2", '{"summary":"stranded two"}')]);
      await engine.drain(5_000);
      assert.equal(vectorKindRegistered(path), false, "fixture: a dead dense arm enrols nothing");
      assert.equal(leafRowsWithoutVectors(path), 2, "fixture: two rows are stranded");
    } finally {
      await engine.close();
    }
  }

  const injector = new DatabaseSync(path);
  try {
    injector.exec(
      "CREATE TRIGGER fix5_repair_fails" +
        " BEFORE DELETE ON _fathomdb_projection_terminal" +
        " BEGIN SELECT RAISE(ABORT, 'fix-5: the un-stranding repair failed'); END",
    );
  } finally {
    injector.close();
  }

  // ---- session B: WITH an embedder. Slice 21's boot graft repairs during
  // `open`, before an Engine exists to close; the injected terminal delete must
  // make that open fail without leaving a registered kind behind.
  await assert.rejects(
    Engine.open(path, { useDefaultEmbedder: true }),
    /could not graft declared vector projection on boot/,
    "fixture: the injected terminal delete must abort the boot graft during open",
  );
  assert.equal(
    vectorKindRegistered(path),
    false,
    "TORN BOOT GRAFT: the registry INSERT committed while the un-stranding it owes did not. " +
      "`_fathomdb_vector_kinds` would then hold `doc` with N1/N2 still carrying " +
      "`'up_to_date'` terminals and no vectors, preventing every later boot graft from " +
      "repairing them. The registry insert and terminal/cursor repair must commit as one " +
      "transaction",
  );

  const remover = new DatabaseSync(path);
  try {
    remover.exec("DROP TRIGGER fix5_repair_fails");
  } finally {
    remover.close();
  }

  // ---- session C: ordinary open/write/drain now recovers through the boot
  // graft, because the failed open left no partial registration behind it. No
  // re-apply or operator rebuild.
  {
    const engine = await Engine.open(path, { useDefaultEmbedder: true });
    try {
      assert.equal(
        vectorKindRegistered(path),
        true,
        "the unblocked boot graft registers the kind before ordinary writes continue",
      );
      await engine.write([node("N4", '{"summary":"the healing write"}')]);
      await engine.drain(DRAIN_TIMEOUT_MS);
      assert.equal(await readiness(engine), "ready");
    } finally {
      await engine.close();
    }
  }

  assert.equal(
    leafRowsWithoutVectors(path),
    0,
    'SELF-SEALED FALSE READY: `drain()` returned and readiness reads "ready", but rows stranded ' +
      "by the torn boot graft still have no vector at rest. The torn state is invisible to " +
      "every later write precisely BECAUSE the kind is already registered — which is why the " +
      "two statements have to be atomic",
  );
  assert.equal(vectorRows(path), 3, "N1, N2 and N4 were all embedded");
});
