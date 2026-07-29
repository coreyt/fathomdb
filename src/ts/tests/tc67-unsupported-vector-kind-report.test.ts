// X1 SDK parity — 0.8.20 Slice 22 (R-20-VC / **TC-67**): declaring a
// `searchable→vector` projection over a kind the vector writer can never commit
// must REPORT, not fall silent.
//
// Drives `ProjectionDelta.vectorUnsupportedKinds` through the napi-rs binding by
// EXECUTION, not symbol presence. Mirrors
// `src/rust/crates/fathomdb-engine/tests/tc67_unsupported_vector_kind_report.rs`
// and the Python suite
// `src/python/tests/test_tc67_unsupported_vector_kind_report.py`
// (Py ≡ TS, R-X-1).
//
// The silence. The engine maps a node `kind` onto a locked `source_type`
// partition-key vocabulary before it can commit a vector, and `write` accepts any
// non-empty `kind`. Slice 20c restricted enrolment to that vocabulary (enrolling
// anything else wedges the projection worker forever) — but the exclusion was
// silent: the declaration persists, its name lands in `deferred`, and the caller
// could not tell "waiting on the embedder" (transient) from "this kind will NEVER
// be embedded" (permanent). `vectorUnsupportedKinds` is that missing fact.
//
// Consumer meaning: rows of a reported kind still get FTS and lexical search;
// they will simply never get vectors — in this or any future session.
//
// No embedder is needed for most of this suite, and that is the point: the
// report is a static property of the locked vocabulary, so it is IDENTICAL with
// and without a live embedder. Only the readiness arm needs a real embedder and
// it honours the standing `FATHOMDB_SKIP_NETWORK_TESTS` guard.
//
// `node:sqlite` is used only as a READ oracle on a CLOSED database — the "still
// not enrolled" assertion behind the report.
//
// ZERO net-new governed commands: this rides the already-governed
// `configureProjections` / `read.projections` verbs plus the shipped
// `engine.drain` instrumentation method.

import test from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";

import { Engine, read } from "../src/index.js";
import type { ProjectionSpec } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

const SOURCE = "ts-test:tc67";
const WEDGE_TIMEOUT_MS = 30_000;

// `doc` is coerced to the `article` partition key, so it IS commit-able.
// `invoice` is the established non-commit-able fixture kind; `entity` is the
// concrete consumer case (Memex entity kinds sit outside the locked vocabulary).
const SUPPORTED_KIND = "doc";
const UNSUPPORTED_KINDS = ["entity", "invoice"];

function node(kind: string, logicalId: string, bodyJson: string): object {
  return { kind, body: bodyJson, logicalId, sourceId: SOURCE };
}

/** The real dense arm: `searchable` + a `vector` sub-object. Only this shape puts
 *  anything on the dense arm, so only this shape can report. */
function vectorSpec(name = "summary"): ProjectionSpec {
  return { name, roles: ["searchable"], fts: false, vector: true };
}

function filterableSpec(name = "summary"): ProjectionSpec {
  return { name, roles: ["filterable"], fts: false, vector: false };
}

/**
 * One commit-able kind and two that are permanently outside the vocabulary,
 * written out of alphabetical order, each unsupported kind written TWICE — so
 * "sorted and de-duplicated" is falsifiable rather than accidental.
 */
async function writeMixedCorpus(engine: Engine): Promise<void> {
  await engine.write([
    node("invoice", "I1", '{"summary":"payable in 30 days"}'),
    node("doc", "N1", '{"summary":"a dense meaning"}'),
    node("entity", "E1", '{"summary":"Alice, a person"}'),
    node("entity", "E2", '{"summary":"Bob, a person"}'),
    node("invoice", "I2", '{"summary":"paid"}'),
  ]);
}

/** READ oracle on a CLOSED database. */
function vectorKindRegistered(path: string, kind: string): boolean {
  const db = new DatabaseSync(path);
  try {
    const row = db
      .prepare("SELECT COUNT(*) AS c FROM _fathomdb_vector_kinds WHERE kind = ?")
      .get(kind) as { c: number };
    return Number(row.c) > 0;
  } finally {
    db.close();
  }
}

async function readiness(engine: Engine, name = "summary"): Promise<string | null> {
  const specs = await read.projections(engine);
  const found = specs.find((s) => s.name === name);
  return found ? (found.vectorDenseReadiness ?? null) : null;
}

/**
 * THE defect. The excluded kinds are named, sorted and de-duplicated, and the
 * commit-able kind is NOT among them.
 *
 * Also pins the two axes side by side: `deferred` carries the projection
 * ATTRIBUTE NAME (the transient "not built yet" fact), while
 * `vectorUnsupportedKinds` carries node KINDS (the permanent one). Reading a
 * kind out of `deferred` — or an attribute name out of the report — is a
 * category error, so both memberships are asserted explicitly.
 */
test("an uncommittable kind is reported by kind, not silently dropped", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await writeMixedCorpus(engine);
    const delta = await engine.configureProjections([vectorSpec()]);

    assert.deepEqual(
      delta.vectorUnsupportedKinds,
      UNSUPPORTED_KINDS,
      "TC-67: a kind the vector writer can never commit must be REPORTED, by KIND, sorted and " +
        "de-duplicated — not dropped in silence",
    );
    assert.ok(
      !delta.vectorUnsupportedKinds.includes(SUPPORTED_KIND),
      "the report must not name a kind the vector writer CAN commit",
    );
    assert.deepEqual(
      delta.deferred,
      ["summary"],
      "the two axes are separate: `deferred` still carries the projection ATTRIBUTE NAME",
    );
    assert.ok(!delta.vectorUnsupportedKinds.includes("summary"));
  } finally {
    await engine.close();
  }

  // The report does NOT lift the exclusion — enrolling these kinds would wedge
  // the projection worker forever (Slice 20c). TC-67 changes what the engine
  // SAYS, not what it does.
  for (const kind of UNSUPPORTED_KINDS) {
    assert.ok(
      !vectorKindRegistered(path, kind),
      `TC-67 REPORTS the exclusion for \`${kind}\`; it must not lift it`,
    );
  }
});

/** Empty, never absent — the field must be readable unconditionally. */
test("a corpus of only supported kinds reports an empty list, not absent", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.write([
      node("doc", "N1", '{"summary":"a dense meaning"}'),
      node("note", "N2", '{"summary":"a second supported kind"}'),
    ]);
    const delta = await engine.configureProjections([vectorSpec()]);
    assert.deepEqual(
      delta.vectorUnsupportedKinds,
      [],
      "with every kind commit-able the report is EMPTY — present and readable, never absent",
    );
    assert.ok(Array.isArray(delta.vectorUnsupportedKinds));
  } finally {
    await engine.close();
  }
});

/**
 * The other three lists describe what THIS call changed; this one describes the
 * corpus as it stands, so a no-op re-apply still carries it.
 *
 * That is also the RESIDUAL's documented refresh path: the report is computed at
 * DECLARE time, so a kind written LATER is absent from a delta the caller already
 * holds — re-applying the same spec (a no-op) returns a current one.
 */
test("the report is state, not diff, so an idempotent re-apply still carries it", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.write([node("invoice", "I1", '{"summary":"payable in 30 days"}')]);
    const first = await engine.configureProjections([vectorSpec()]);
    assert.deepEqual(first.vectorUnsupportedKinds, ["invoice"]);

    const again = await engine.configureProjections([vectorSpec()]);
    assert.equal(again.unchanged, true, "re-registering the same spec is still a no-op");
    assert.deepEqual([again.built, again.dropped, again.deferred], [[], [], []]);
    assert.deepEqual(
      again.vectorUnsupportedKinds,
      ["invoice"],
      "a STATE report, not a diff: `unchanged: true` must not suppress it",
    );

    // The residual, made concrete, and its refresh.
    await engine.write([node("entity", "E1", '{"summary":"Alice, a person"}')]);
    assert.deepEqual(
      first.vectorUnsupportedKinds,
      ["invoice"],
      "RESIDUAL: the delta already held is a snapshot; it never learns about `entity`",
    );
    const refreshed = await engine.configureProjections([vectorSpec()]);
    assert.equal(refreshed.unchanged, true, "the refresh costs nothing — still a no-op");
    assert.deepEqual(
      refreshed.vectorUnsupportedKinds,
      UNSUPPORTED_KINDS,
      "…and the no-op re-apply reports the corpus as it stands NOW",
    );
  } finally {
    await engine.close();
  }
});

/**
 * Scoped to the dense arm: with no `searchable→vector` declaration there is
 * nothing for a kind to be unsupported FOR, so the report stays empty rather than
 * becoming noise on every non-vector call.
 */
test("no vector declaration means no report", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await writeMixedCorpus(engine);
    const delta = await engine.configureProjections([filterableSpec()]);
    assert.deepEqual(delta.vectorUnsupportedKinds, []);

    const declared = await engine.configureProjections([vectorSpec("meaning")]);
    assert.deepEqual(declared.vectorUnsupportedKinds, UNSUPPORTED_KINDS, "fixture");

    const dropped = await engine.configureProjections([], ["meaning"]);
    assert.ok(dropped.dropped.includes("meaning"), "fixture: the drop is reported");
    assert.deepEqual(
      dropped.vectorUnsupportedKinds,
      [],
      "once the last `searchable→vector` declaration is gone the report goes quiet with it",
    );
  } finally {
    await engine.close();
  }
});

/**
 * `read.projections` output must still feed straight back into
 * `configureProjections` as a no-op. It cannot break, structurally: the new field
 * lives on the DELTA and `configureProjections` accepts specs, never a delta — it
 * is OUTPUT-ONLY. Stated explicitly rather than left to inference.
 */
test("the read → configure round-trip still holds", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await writeMixedCorpus(engine);
    await engine.configureProjections([vectorSpec()]);

    const back = await read.projections(engine);
    assert.equal(back.length, 1);
    const roundTripped = await engine.configureProjections(back);
    assert.equal(
      roundTripped.unchanged,
      true,
      "the shipped read→configure round-trip is still a no-op",
    );
    assert.deepEqual(
      roundTripped.vectorUnsupportedKinds,
      UNSUPPORTED_KINDS,
      "…and the no-op still carries the report",
    );
  } finally {
    await engine.close();
  }
});

/**
 * THE DoD clause most likely to regress. An un-enrolled kind is NOT outstanding
 * work — nothing will ever be embedded for it, so there is nothing to wait for. A
 * corpus made ENTIRELY of unsupported kinds must still reach
 * `vectorDenseReadiness === "ready"` and `drain` must still resolve.
 *
 * This is the arm that needs a LIVE embedder: without one every dense-arm path is
 * short-circuited and the assertion would pass vacuously.
 */
test("readiness semantics are unchanged by the report", { timeout: 300_000 }, async (t) => {
  if (process.env.FATHOMDB_SKIP_NETWORK_TESTS) {
    t.skip("FATHOMDB_SKIP_NETWORK_TESTS set; skipping default-embedder test");
    return;
  }
  const path = freshDbPath();
  const engine = await Engine.open(path, { useDefaultEmbedder: true });
  try {
    await engine.write([
      node("invoice", "I1", '{"summary":"payable in 30 days"}'),
      node("entity", "E1", '{"summary":"Alice, a person"}'),
    ]);
    await engine.drain(WEDGE_TIMEOUT_MS);

    const delta = await engine.configureProjections([vectorSpec()]);
    assert.deepEqual(
      delta.vectorUnsupportedKinds,
      UNSUPPORTED_KINDS,
      "the report is IDENTICAL with a live embedder — it is a static property of the locked " +
        "vocabulary, not a session fact",
    );

    await engine.drain(WEDGE_TIMEOUT_MS);
    assert.equal(
      await readiness(engine),
      "ready",
      "READINESS SEMANTICS ARE UNCHANGED: an un-enrolled kind is not outstanding work, so a " +
        "corpus made entirely of unsupported kinds is `ready`, not `embedding`",
    );

    await engine.write([node("entity", "E2", '{"summary":"Bob, a person"}')]);
    await engine.drain(WEDGE_TIMEOUT_MS);
    assert.equal(
      await readiness(engine),
      "ready",
      "writing MORE rows of an unsupported kind still leaves nothing outstanding",
    );
  } finally {
    await engine.close();
  }
});
