// X1 SDK parity — 0.8.20 Slice 20 (R-20-DR `dense_readiness`).
//
// Drives the engine-set readiness field through the napi-rs binding by
// EXECUTION (not symbol presence): `read.projections` must surface
// `vectorDenseReadiness` on a `searchable→vector` projection, it must never read
// `"ready"` while an embed is outstanding, and it must be inert on the way back
// in. Mirrors the Rust suite
// `src/rust/crates/fathomdb-engine/tests/slice20_dense_readiness.rs` and the
// Python suite `src/python/tests/test_slice20_dense_readiness.py` (Py ≡ TS,
// R-X-1).
//
// `node:sqlite` is used only as a READ oracle on a CLOSED database — the
// "vector at rest" assertion behind the readiness flip.
//
// ZERO net-new governed commands: this rides the already-governed
// `configureProjections` / `read.projections` verbs.

import test from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";

import { Engine, read } from "../src/index.js";
import type { ProjectionSpec } from "../src/index.js";
import { InvalidArgumentError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

const SOURCE = "ts-test:slice20";

function node(logicalId: string, bodyJson: string): object {
  return { kind: "doc", body: bodyJson, logicalId, sourceId: SOURCE };
}

function vectorSpec(name: string): ProjectionSpec {
  return { name, roles: ["searchable"], fts: false, vector: true };
}

async function readiness(engine: Engine, name: string): Promise<string | null> {
  const specs = await read.projections(engine);
  const found = specs.find((s) => s.name === name);
  return found ? (found.vectorDenseReadiness ?? null) : null;
}

/** Raw at-rest oracle on a CLOSED database: vector rows per write cursor. */
function vectorRowCount(path: string): number {
  const db = new DatabaseSync(path);
  try {
    return Number(
      (db.prepare("SELECT COUNT(*) AS c FROM _fathomdb_vector_rows").get() as { c: number }).c,
    );
  } finally {
    db.close();
  }
}

/**
 * §4.1 invariant 1 as SQL: rows that reached the `up_to_date` projection
 * terminal but carry NO vector row. Must always be zero — the vector INSERT and
 * the terminal are one transaction, so a torn `ready`-without-vector is
 * unreachable.
 */
function tornTerminals(path: string): number {
  const db = new DatabaseSync(path);
  try {
    return Number(
      (
        db
          .prepare(
            "SELECT COUNT(*) AS c FROM _fathomdb_projection_terminal t" +
              " JOIN canonical_nodes n ON n.write_cursor = t.write_cursor" +
              " JOIN _fathomdb_vector_kinds k ON k.kind = n.kind" +
              " LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = t.write_cursor" +
              " WHERE t.state = 'up_to_date' AND v.write_cursor IS NULL",
          )
          .get() as { c: number }
      ).c,
    );
  } finally {
    db.close();
  }
}

test("read.projections surfaces vectorDenseReadiness on a searchable→vector projection", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.configureProjections([vectorSpec("summary")]);
    // No embedder is configured, so nothing is outstanding: `ready`.
    assert.equal(await readiness(engine, "summary"), "ready");
  } finally {
    await engine.close();
  }
});

test("readiness is scoped to the vector sub-object — a non-vector projection carries none", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.configureProjections([
      { name: "status", roles: ["filterable"], fts: false, vector: false },
    ]);
    const got = (await read.projections(engine)).find((s) => s.name === "status");
    assert.ok(got);
    assert.equal(got.vector, false);
    assert.equal(
      got.vectorDenseReadiness,
      null,
      "filterable / searchable→FTS are same-transaction and have NO readiness axis",
    );
  } finally {
    await engine.close();
  }
});

test("readiness never reports ready with pending embeds (offline, deterministic)", async () => {
  // The R-20-DR acceptance signal, driven through the binding with NO network
  // and NO embedder download.
  //
  // How it is made deterministic: register `doc` as a vector kind via the
  // `test-hooks`-gated native seam (the same one `use-default-embedder.test.ts`
  // uses — the public TS surface has no typed vector-write verb), then write a
  // `doc` row on an engine opened WITHOUT an embedder. The row enqueues vector
  // work that cannot complete, and the shipped retry ladder is 1s/4s/16s, so
  // there is a ~21-second window in which the embed is genuinely outstanding.
  // Readiness must read `embedding` throughout it — never `ready`.
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    // Declare the projection FIRST: `configureProjections` drains, and draining
    // with work outstanding would (correctly) time out.
    await engine.configureProjections([vectorSpec("summary")]);
    assert.equal(await readiness(engine, "summary"), "ready", "an empty corpus is ready");

    const inner = (engine as unknown as { _native: unknown })._native as {
      configureVectorKindForTest: (kind: string) => Promise<void>;
    };
    await inner.configureVectorKindForTest("doc");
    await engine.write([node("N1", JSON.stringify({ summary: "a dense meaning" }))]);

    assert.equal(
      await readiness(engine, "summary"),
      "embedding",
      "readiness must NOT report ready while an embed is outstanding",
    );
  } finally {
    await engine.close();
  }
  // At rest: the tolerated torn state (`embedding` with the vector absent) —
  // and NOT the forbidden one. No `up_to_date` terminal exists without its
  // vector row, which is §4.1 invariant 1.
  assert.equal(vectorRowCount(path), 0, "no vector landed — the embed never ran");
  assert.equal(tornTerminals(path), 0, "§4.1: no up_to_date terminal without its vector");
});

test("a caller-supplied vectorDenseReadiness is INERT — the engine reports the derived truth", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    // Declare a LIE. It is accepted (it is not part of the declaration) but the
    // registry never stores it, so the engine still reports what it derives.
    await engine.configureProjections([
      { ...vectorSpec("summary"), vectorDenseReadiness: "embedding" },
    ]);
    assert.equal(
      await readiness(engine, "summary"),
      "ready",
      "the caller's `embedding` must NOT be honoured",
    );
    // And it cannot masquerade as a projection change: re-applying diffs to a
    // no-op even though the readiness field differs from what was first sent.
    const again = await engine.configureProjections([vectorSpec("summary")]);
    assert.equal(again.unchanged, true, "readiness is not part of the declaration");
  } finally {
    await engine.close();
  }
});

test("read.projections output round-trips BACK into configureProjections with readiness attached", async () => {
  // The fix-4 read→configure round-trip, extended: `read.projections` now emits
  // `vectorDenseReadiness` for a vector projection, and feeding that output
  // straight back MUST still re-apply as an idempotent no-op. Otherwise
  // `read.projections` would produce a value its own `configureProjections`
  // cannot consume — the exact defect fix-4 closed.
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.configureProjections([
      { name: "status", roles: ["filterable", "searchable"], fts: true, vector: true },
    ]);
    const readBack = await read.projections(engine);
    assert.equal(readBack.length, 1);
    assert.equal(readBack[0].vectorDenseReadiness, "ready", "read output carries readiness");
    const again = await engine.configureProjections(readBack);
    assert.equal(again.unchanged, true, "read.projections output must re-apply as a no-op");
  } finally {
    await engine.close();
  }
});

test("vectorDenseReadiness with vector:false is REFUSED (cannot round-trip)", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      engine.configureProjections([
        {
          name: "status",
          roles: ["filterable"],
          fts: false,
          vector: false,
          vectorDenseReadiness: "ready",
        },
      ]),
      (err: unknown) => {
        assert.ok(err instanceof InvalidArgumentError);
        assert.match(String((err as Error).message), /vectorDenseReadiness/);
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});

test('an unknown readiness spelling is REFUSED — "pending" is reserved for the admission axis', async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    for (const bad of ["pending", "", "Ready", "embedded"]) {
      await assert.rejects(
        engine.configureProjections([
          // The cast is deliberate: the TS type already forbids these, so the
          // test is proving the RUNTIME gate, not the compile-time one.
          { ...vectorSpec("summary"), vectorDenseReadiness: bad } as ProjectionSpec,
        ]),
        (err: unknown) => {
          assert.ok(
            err instanceof InvalidArgumentError,
            `expected an InvalidArgumentError refusal for readiness spelling ${JSON.stringify(bad)}`,
          );
          return true;
        },
      );
    }
  } finally {
    await engine.close();
  }
});
