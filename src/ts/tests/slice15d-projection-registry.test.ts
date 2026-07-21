// X1 SDK parity — 0.8.20 Slice 15d (R-20-PR / R-20-EAV projection registry).
//
// Drives the two net-new governed verbs through the napi-rs binding by
// EXECUTION: `Engine.configureProjections` and `read.projections`. Mirrors the
// Rust suite `src/rust/crates/fathomdb-engine/tests/
// slice15d_projection_registry.rs` and the Python suite
// `src/python/tests/test_slice15d_projection_registry.py` (Py ≡ TS, R-X-1).
//
// `node:sqlite` is used only as a READ oracle on a CLOSED database — the "value
// at rest" assertion for the EAV store / property-FTS.

import test from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";

import { Engine, read } from "../src/index.js";
import type { ProjectionSpec, ProjectionRole } from "../src/index.js";
import {
  FathomDbError,
  ProjectionDestructiveError,
  WriteValidationError,
} from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

const SOURCE = "ts-test:slice15d";

// Slice 15d fix-5 (AC-068a) — an embedded NUL smuggled into a ProjectionSpec /
// drop string. JS strings are UTF-16; a NUL codepoint is representable and the
// napi conversion accepts it, so it must be rejected at the BINDING before the
// writer transaction opens — never persisted in `_fathomdb_projection_registry`.
const NUL = `a${String.fromCharCode(0)}b`;

function node(logicalId: string, source: string, bodyJson: string): object {
  return { kind: "doc", body: bodyJson, logicalId, sourceId: source };
}

function spec(
  name: string,
  roles: ProjectionRole[],
  opts: { fts?: boolean; vector?: boolean } = {},
): ProjectionSpec {
  return { name, roles, fts: opts.fts ?? false, vector: opts.vector ?? false };
}

function eavValues(path: string, attrName: string): string[] {
  const db = new DatabaseSync(path);
  try {
    return (
      db
        .prepare(
          "SELECT attr_value AS v FROM canonical_attributes" +
            " WHERE attr_name = ? ORDER BY attr_value",
        )
        .all(attrName) as { v: string }[]
    ).map((r) => r.v);
  } finally {
    db.close();
  }
}

function registryNames(path: string): string[] {
  const db = new DatabaseSync(path);
  try {
    return (
      db
        .prepare("SELECT name AS n FROM _fathomdb_projection_registry ORDER BY name")
        .all() as { n: string }[]
    ).map((r) => r.n);
  } finally {
    db.close();
  }
}

function pftsMatch(path: string, attrName: string, query: string): number[] {
  const db = new DatabaseSync(path);
  try {
    return (
      db
        .prepare(
          "SELECT write_cursor AS c FROM property_search_index" +
            " WHERE attr_name = ? AND property_search_index MATCH ? ORDER BY write_cursor",
        )
        .all(attrName, query) as { c: number }[]
    ).map((r) => Number(r.c));
  } finally {
    db.close();
  }
}

test("configure + read.projections round-trips a spec verbatim", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.configureProjections([
      spec("status", ["filterable", "searchable"], { fts: true }),
    ]);
    const back = await read.projections(engine);
    assert.equal(back.length, 1);
    assert.equal(back[0].name, "status");
    assert.deepEqual([...back[0].roles].sort(), ["filterable", "searchable"]);
    assert.equal(back[0].fts, true);
    assert.equal(back[0].vector, false);
  } finally {
    await engine.close();
  }
});

test("idempotent re-registration is a no-op", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.write([node("N1", "src:1", '{"status":"open"}')]);
    const s = spec("status", ["filterable"]);
    const first = await engine.configureProjections([s]);
    assert.equal(first.unchanged, false);
    assert.deepEqual(first.built, ["status"]);

    const second = await engine.configureProjections([s]);
    assert.equal(second.unchanged, true);
    assert.deepEqual(second.built, []);
    assert.deepEqual(second.dropped, []);
    assert.deepEqual(second.deferred, []);
  } finally {
    await engine.close();
  }
});

test("property filter + property-FTS return correct rows at rest", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.write([node("A", "src:a", '{"title":"the quick brown fox"}')]);
    await engine.write([node("B", "src:b", '{"title":"lazy dogs sleeping"}')]);
    await engine.configureProjections([
      spec("title", ["filterable", "searchable"], { fts: true }),
    ]);
    await engine.write([node("C", "src:c", '{"title":"a brown bear"}')]);
    await engine.drain(30_000);
  } finally {
    await engine.close();
  }

  assert.deepEqual(eavValues(path, "title"), [
    "a brown bear",
    "lazy dogs sleeping",
    "the quick brown fox",
  ]);
  assert.deepEqual(pftsMatch(path, "title", "brown"), [1, 3]);
  assert.deepEqual(pftsMatch(path, "title", "fox"), [1]);
});

test("explicit drop drops exactly one; omission does not drop", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.write([node("A", "src:a", '{"status":"open","title":"hello"}')]);
    await engine.configureProjections([spec("status", ["filterable"])]);
    await engine.configureProjections([spec("title", ["searchable"], { fts: true })]);

    const omit = await engine.configureProjections([
      spec("title", ["searchable"], { fts: true }),
    ]);
    assert.deepEqual(omit.dropped, []);
    assert.deepEqual(
      (await read.projections(engine)).map((s) => s.name).sort(),
      ["status", "title"],
    );

    const d = await engine.configureProjections([], ["status"]);
    assert.deepEqual(d.dropped, ["status"]);
    assert.deepEqual(
      (await read.projections(engine)).map((s) => s.name),
      ["title"],
    );
    await engine.drain(30_000);
  } finally {
    await engine.close();
  }

  assert.deepEqual(eavValues(path, "status"), []);
});

test("destructive change requires an explicit drop", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.write([node("A", "src:a", '{"status":"open"}')]);
    await engine.configureProjections([
      spec("status", ["filterable", "searchable"], { fts: true }),
    ]);

    await assert.rejects(
      () => engine.configureProjections([spec("status", ["filterable"])]),
      (err: unknown) => {
        assert.ok(err instanceof ProjectionDestructiveError);
        assert.equal((err as ProjectionDestructiveError).name, "status");
        return true;
      },
    );

    const ok = await engine.configureProjections([spec("status", ["filterable"])], [
      "status",
    ]);
    assert.deepEqual(ok.dropped, ["status"]);
    assert.deepEqual([...(await read.projections(engine))[0].roles], ["filterable"]);
  } finally {
    await engine.close();
  }
});

test("fix-5 NUL in projection name rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () => engine.configureProjections([spec(NUL, ["filterable"])]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        assert.ok(err instanceof FathomDbError, "must extend FathomDbError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(registryNames(path), [], "no projection may be persisted when a NUL is rejected");
});

test("fix-5 NUL in ftsTokenizer rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () =>
        engine.configureProjections([
          { name: "status", roles: ["searchable"], fts: true, ftsTokenizer: NUL, vector: false },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(registryNames(path), [], "no projection may be persisted when a NUL is rejected");
});

test("fix-5 NUL in vectorEmbedder rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () =>
        engine.configureProjections([
          { name: "summary", roles: ["searchable"], fts: false, vector: true, vectorEmbedder: NUL },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(registryNames(path), [], "no projection may be persisted when a NUL is rejected");
});

test("fix-5 NUL in drop entry rejected at binding", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    // A live projection exists so the drop path is non-vacuous.
    await engine.write([node("A", "src:a", '{"status":"open"}')]);
    await engine.configureProjections([spec("status", ["filterable"])]);
    await assert.rejects(
      () => engine.configureProjections([], [NUL]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        return true;
      },
    );
    assert.deepEqual(
      (await read.projections(engine)).map((s) => s.name),
      ["status"],
      "the refused drop must not touch the live projection",
    );
  } finally {
    await engine.close();
  }
});

test("rankable and vector sub-target are deferred, not built", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.write([node("A", "src:a", '{"importance":"high","summary":"a meaning"}')]);
    const d1 = await engine.configureProjections([spec("importance", ["rankable"])]);
    assert.deepEqual(d1.built, []);
    assert.deepEqual(d1.deferred, ["importance"]);

    const d2 = await engine.configureProjections([
      spec("summary", ["searchable"], { vector: true }),
    ]);
    assert.deepEqual(d2.deferred, ["summary"]);
    const summary = (await read.projections(engine)).find((s) => s.name === "summary");
    assert.ok(summary && summary.vector === true);
  } finally {
    await engine.close();
  }
});
