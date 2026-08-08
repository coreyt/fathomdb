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
  InvalidArgumentError,
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

// TC-47 (keystone terminal codex P2) — a lone UTF-16 high surrogate smuggled
// into a ProjectionSpec / drop string. Unlike a NUL (which survives the napi
// UTF-8 conversion as a real byte and is caught Rust-side), napi-rs silently
// replaces a lone surrogate with U+FFFD BEFORE the Rust guard runs, so the guard
// never sees it. `configureProjections` must therefore run `validateFfiTree`
// JS-side — exactly like `write` — or the mangled U+FFFD is persisted instead of
// raising WriteValidationError. Mirrors the AC-068b surrogate cases in
// `ffi-safety.test.ts`.
const SURROGATE = `a${String.fromCharCode(0xd800)}b`;

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

// 0.8.20 Slice 23 (`R-20-SV`) — write a registry row the way the SHIPPED
// PRE-Slice-23 engine wrote it, back when an `fts`/`vector` sub-object without
// the `searchable` role was ACCEPTED. A raw INSERT is the only way to reach that
// at-rest state now that the verb refuses it, and it is the population a real
// consumer database will be in when it upgrades — so the SDKs must cover it,
// not only the rejection path nobody in the field is on yet. Mirrors the Rust
// `seed_legacy_registry_row` in `src/rust/crates/fathomdb-engine/tests/
// slice23_spec_validation_reject.rs` and the Python `_seed_legacy_registry_row`
// (same columns, same encoding: `fts_tokenizer = ''` is "an `fts` sub-object
// with the DEFAULT tokenizer", NULL is "no `fts` sub-object").
//
// MUST be called on a CLOSED database. A raw RW connection opened while the
// engine is live perturbs the async dispatcher (a duplicate embed, the TC-91
// class — it passes in isolation and fails in a full suite run), so the seed
// goes between a `close()` and the next `Engine.open`.
function seedLegacyRegistryRow(
  path: string,
  name: string,
  rolesCsv: string,
  ftsTokenizer: string | null,
  vectorDeclared: boolean,
): void {
  const db = new DatabaseSync(path);
  try {
    db.prepare(
      "INSERT INTO _fathomdb_projection_registry" +
        " (name, roles, fts_tokenizer, vector_embedder, vector_declared)" +
        " VALUES(?, ?, ?, NULL, ?)" +
        " ON CONFLICT(name) DO UPDATE SET" +
        " roles = excluded.roles," +
        " fts_tokenizer = excluded.fts_tokenizer," +
        " vector_embedder = excluded.vector_embedder," +
        " vector_declared = excluded.vector_declared",
    ).run(name, rolesCsv, ftsTokenizer, vectorDeclared ? 1 : 0);
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

// --- TC-47: lone UTF-16 surrogate must be rejected JS-side (napi → U+FFFD) ---

test("TC-47 surrogate in projection name rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () => engine.configureProjections([spec(SURROGATE, ["filterable"])]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        assert.ok(err instanceof FathomDbError, "must extend FathomDbError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(
    registryNames(path),
    [],
    "no projection may be persisted when a surrogate is rejected",
  );
});

test("TC-47 surrogate in ftsTokenizer rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () =>
        engine.configureProjections([
          { name: "status", roles: ["searchable"], fts: true, ftsTokenizer: SURROGATE, vector: false },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(
    registryNames(path),
    [],
    "no projection may be persisted when a surrogate is rejected",
  );
});

test("TC-47 surrogate in vectorEmbedder rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () =>
        engine.configureProjections([
          { name: "summary", roles: ["searchable"], fts: false, vector: true, vectorEmbedder: SURROGATE },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(
    registryNames(path),
    [],
    "no projection may be persisted when a surrogate is rejected",
  );
});

test("TC-47 surrogate in projection role rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      // A lone surrogate as a role value — validateFfiTree must walk the roles
      // array too. Cast through unknown: an invalid role string is exactly the
      // hostile input the FFI guard exists to reject before native.
      () =>
        engine.configureProjections([
          { name: "status", roles: [SURROGATE as unknown as ProjectionRole], fts: false, vector: false },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(
    registryNames(path),
    [],
    "no projection may be persisted when a surrogate is rejected",
  );
});

test("TC-47 surrogate in drop entry rejected at binding", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    // A live projection exists so the drop path is non-vacuous.
    await engine.write([node("A", "src:a", '{"status":"open"}')]);
    await engine.configureProjections([spec("status", ["filterable"])]);
    await assert.rejects(
      () => engine.configureProjections([], [SURROGATE]),
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

// ---------------------------------------------------------------------------
// 0.8.20 keystone closeout fix-4 — projection-spec binding round-trip
// consistency. A ProjectionSpec the binding ACCEPTS must round-trip through
// `read.projections` IDENTICALLY; a shape that would be silently dropped or
// normalized is refused at the binding boundary with the typed validation error
// (InvalidArgumentError, the same variant the unknown-role rejection uses).
// Mirrors the Python suite one-for-one (Py ≡ TS: both reject the same shapes).
// ---------------------------------------------------------------------------

test("fix-4 orphaned ftsTokenizer (fts:false) rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () =>
        engine.configureProjections([
          { name: "status", roles: ["searchable"], fts: false, ftsTokenizer: "unicode61", vector: false },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof InvalidArgumentError, "must be InvalidArgumentError");
        assert.ok(err instanceof FathomDbError);
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(registryNames(path), [], "a refused spec must persist no registry row");
});

test("fix-4 orphaned vectorEmbedder (vector:false) rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () =>
        engine.configureProjections([
          { name: "summary", roles: ["searchable"], fts: false, vector: false, vectorEmbedder: "bge-small" },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof InvalidArgumentError, "must be InvalidArgumentError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(registryNames(path), [], "a refused spec must persist no registry row");
});

test("fix-4 empty ftsTokenizer (fts:true) rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () =>
        engine.configureProjections([
          { name: "status", roles: ["searchable"], fts: true, ftsTokenizer: "", vector: false },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof InvalidArgumentError, "must be InvalidArgumentError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(registryNames(path), [], "a refused spec must persist no registry row");
});

test("fix-4 empty vectorEmbedder (vector:true) rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () =>
        engine.configureProjections([
          { name: "summary", roles: ["searchable"], fts: false, vector: true, vectorEmbedder: "" },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof InvalidArgumentError, "must be InvalidArgumentError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(registryNames(path), [], "a refused spec must persist no registry row");
});

test("fix-4 duplicate role rejected at binding, not persisted", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await assert.rejects(
      () =>
        engine.configureProjections([
          // The TS `roles` is a plain array (unlike the Python frozenset), so a
          // duplicate spelling is reachable through the public SDK — and cannot
          // round-trip the registry's de-duplicated set.
          { name: "status", roles: ["searchable", "searchable"], fts: false, vector: false },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof InvalidArgumentError, "must be InvalidArgumentError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
  assert.deepEqual(registryNames(path), [], "a refused spec must persist no registry row");
});

test("fix-4 CONTROL — a consistent spec round-trips verbatim via read.projections", async () => {
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    const sent: ProjectionSpec = {
      name: "status",
      roles: ["searchable"],
      fts: true,
      ftsTokenizer: "unicode61",
      vector: true,
      vectorEmbedder: "bge-small",
    };
    const delta = await engine.configureProjections([sent]);
    assert.equal(delta.unchanged, false);
    const back = await read.projections(engine);
    assert.equal(back.length, 1);
    // The full round-trip invariant: read-back equals what was sent, PLUS the
    // one engine-set READ-METADATA field 0.8.20 Slice 20 (R-20-DR) attached to
    // the vector sub-object. `vectorDenseReadiness` is not a declaration, so it
    // is expected to differ from the sent spec (which never authors it); every
    // DECLARED field must still match byte-for-byte.
    assert.equal(
      back[0].vectorDenseReadiness,
      "unavailable",
      "this fixture opens without a runtime embedder, so no-work is not dense-ready",
    );
    assert.deepEqual({ ...back[0], vectorDenseReadiness: undefined }, {
      ...sent,
      vectorDenseReadiness: undefined,
    });
  } finally {
    await engine.close();
  }
});

test("R-20-SV — fts/vector without the searchable role is REJECTED (WriteValidationError)", async () => {
  // 0.8.20 Slice 23 (`R-20-SV`). This test asserted the OPPOSITE until this
  // slice: the shipped 15d fix-4 position was that the shape is accepted and
  // round-trips faithfully. HITL ruling 2026-07-24 (`dev/plans/plan-0.8.20.md`
  // §11 item 4) chose option (b) REJECT — the `fts`/`vector` sub-objects SELECT
  // a sub-target of `searchable` and do not confer it, so without the role the
  // declaration builds, embeds and enrols nothing: a meaningless config.
  //
  // Family: `WriteValidationError` (`FDB_WRITE_VALIDATION`) per decision #18 —
  // a malformed write SHAPE is one family. It is message-less by construction,
  // so the refusal cannot name WHICH spec in the list was invalid
  // (TC-95/TC-98, deferred by the HITL).
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    for (const [fts, vector] of [
      [true, false],
      [false, true],
      [true, true],
    ] as const) {
      await assert.rejects(
        engine.configureProjections([{ name: "status", roles: ["filterable"], fts, vector }]),
        WriteValidationError,
        `fts=${fts} vector=${vector} without the searchable role must be rejected`,
      );
    }
    // `rankable` does not supply the role either — the reject is keyed on the
    // ABSENCE of `searchable`, not on which other roles are present.
    await assert.rejects(
      engine.configureProjections([
        { name: "status", roles: ["rankable"], fts: false, vector: true },
      ]),
      WriteValidationError,
    );
    assert.deepEqual(await read.projections(engine), [], "no refused call persisted anything");

    // CONTROL — with `searchable` present the identical sub-objects are VALID
    // and still round-trip.
    await engine.configureProjections([
      { name: "status", roles: ["filterable", "searchable"], fts: true, vector: true },
    ]);
    const got = (await read.projections(engine)).find((s) => s.name === "status");
    assert.ok(got, "the projection must exist");
    assert.equal(got.fts, true);
    assert.equal(got.vector, true);
    assert.deepEqual([...got.roles].sort(), ["filterable", "searchable"]);
  } finally {
    await engine.close();
  }
});

test("R-20-SV — a LEGACY registry row reads back verbatim but no longer re-applies", async () => {
  // 0.8.20 Slice 23 (`R-20-SV`) — THE UPGRADE PATH, the one a real consumer
  // database is actually on.
  //
  // Databases that declared `fts`/`vector` without the `searchable` role while
  // the engine ACCEPTED it still exist. This slice must not make them
  // unreadable. `read.projections` is a pure read and rejects nothing, so the
  // legacy row is reported VERBATIM — but feeding that output straight back into
  // `configureProjections` (the shipped fix-4 read→configure round-trip) now
  // RAISES `WriteValidationError` (`FDB_WRITE_VALIDATION`).
  //
  // That asymmetry is the honest, documented consequence of the HITL ruling
  // (2026-07-24, `dev/plans/plan-0.8.20.md` §11 item 4, option (b)): for the
  // legacy population the round-trip is broken BY DESIGN, and the remedy is to
  // ADD the `searchable` role (asserted here) or to name the projection in
  // `drop`.
  //
  // X1 parity twin of the Rust
  // `a_legacy_registry_row_reads_back_verbatim_but_no_longer_re_applies`
  // (`slice23_spec_validation_reject.rs`) and the Python
  // `test_a_legacy_registry_row_reads_back_verbatim_but_no_longer_re_applies`.
  // Same seed, same three oracles, same semantics — deliberately NOT a
  // per-binding invention.
  const path = freshDbPath();
  {
    const engine = await Engine.open(path);
    try {
      await engine.write([node("N1", SOURCE, JSON.stringify({ status: "open" }))]);
    } finally {
      await engine.close();
    }
  }

  // The legacy state, written the way the shipped pre-Slice-23 engine wrote it.
  // On a CLOSED database — see the helper's ordering note.
  seedLegacyRegistryRow(path, "status", "filterable", "", true);

  const engine = await Engine.open(path);
  try {
    // (a) Still READABLE, and reported verbatim.
    const back = await read.projections(engine);
    assert.equal(back.length, 1, "a legacy row must still be readable");
    assert.equal(back[0].name, "status");
    assert.deepEqual([...back[0].roles].sort(), ["filterable"]);
    assert.equal(back[0].fts, true, "the legacy `fts` sub-object is reported verbatim");
    assert.equal(back[0].vector, true, "…and the legacy `vector` sub-object too");

    // (b) …but it can no longer be RE-APPLIED. Feeding `read.projections`
    // output straight back in is exactly the shipped fix-4 round-trip.
    await assert.rejects(
      engine.configureProjections(back),
      WriteValidationError,
      "BREAKING, by design: for the legacy fts/vector-without-`searchable` population the " +
        "read.projections -> configureProjections round-trip no longer closes",
    );

    // (c) The remedy closes it again: ADD the `searchable` role. Everything else
    // about the declaration is untouched.
    const fixed: ProjectionSpec = {
      ...back[0],
      roles: [...back[0].roles, "searchable"] as ProjectionRole[],
    };
    await engine.configureProjections([fixed]);
    const after = await read.projections(engine);
    assert.equal(after.length, 1);
    assert.deepEqual([...after[0].roles].sort(), ["filterable", "searchable"]);
    assert.equal(after[0].fts, true);
    assert.equal(after[0].vector, true);
    await engine.drain(5_000);
  } finally {
    await engine.close();
  }
});

test("fix-4 read.projections output round-trips BACK into configureProjections (null≡None, Py≡TS)", async () => {
  // The read→configure round-trip: `read.projections` emits `ftsTokenizer: null`
  // / `vectorEmbedder: null` for a spec with no custom sub-field. Feeding that
  // output straight back into `configureProjections` MUST be accepted as an
  // idempotent no-op — otherwise `read.projections` produces a value its own
  // `configureProjections` cannot consume. pyo3 accepts `None` natively; napi
  // rejected an explicit `null` with an opaque `StringExpected`, diverging from
  // Python and breaking this round-trip. fix-4 normalizes `null → None` at the
  // TS binding boundary so the two bindings behave identically.
  const path = freshDbPath();
  const engine = await Engine.open(path);
  try {
    await engine.configureProjections([
      { name: "status", roles: ["filterable", "searchable"], fts: true, vector: true },
    ]);
    const readBack = await read.projections(engine);
    assert.equal(readBack.length, 1);
    assert.equal(readBack[0].ftsTokenizer, null, "read output carries an explicit null sub-field");
    // Re-applying the read output verbatim is a no-op — proves null round-trips.
    const again = await engine.configureProjections(readBack);
    assert.equal(again.unchanged, true, "read.projections output must re-apply as a no-op");
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
