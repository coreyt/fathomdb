// X1 SDK parity — 0.8.20 Slice 15b fix-2 (codex §9 [P2]: the validity window
// must govern `search`, not just the five read verbs).
//
// Slice 10b scoped `ReadView` to read.get / read.getMany / read.list /
// read.listFilter / graph.neighbors and left `search` out. That was defensible
// only while NO SDK caller could author a window — the only way to set one was
// raw SQL, so the gap was unreachable. Slice 15b (TC-34) made authoring
// reachable from TypeScript and Python, turning a latent gap into a LIVE
// DEFECT: a caller could write a node whose window had already closed, watch
// `read.get` correctly hide it, and still get it back from `search`.
//
// `dev/design/record-lifecycle-protocol/api-surface.md:50` always specified
// `ReadView` as an optional argument on **`search`** alongside the read verbs.
//
// `node:sqlite` is used ONLY as a data-at-rest oracle on a CLOSED database: a
// search-based assertion alone can pass on broken code, because a node that was
// never indexed is also "not returned". The raw table proves the window landed
// AND that the body reached the FTS projection, so an absence below is a
// filtering decision rather than a missing row.
//
// Windows are chosen far from the real clock (epoch 1000..2000 = 1970;
// 4_000_000_000 = year 2096) so the DEFAULT-view assertions are unambiguous
// without pinning `validAsOf`. Every pinned assertion rides the BOUND `:now`
// seam — no wall clock, no sleep.
//
// Cross-binding equivalence anchor:
// `src/python/tests/test_slice15b_search_validity.py` asserts the SAME
// behaviour for the same inputs (Py ≡ TS, R-X-1), and
// `src/rust/crates/fathomdb-engine/tests/slice15b_search_validity.rs` is the
// engine-level mirror.

import test from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";

import { Engine } from "../src/index.js";
import { InvalidArgumentError } from "../src/errors.js";
import type { SearchResult } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

const SOURCE_ID = "ts-test:slice15b-search-validity";

/** Epoch second comfortably in the FUTURE relative to any real test clock. */
const FAR_FUTURE = 4_000_000_000;
/** Upper bound of a window that closed in 1970 — comfortably in the PAST. */
const FAR_PAST_UNTIL = 2_000;

function windowed(
  logicalId: string,
  body: string,
  validFrom: number | null,
  validUntil: number | null,
): object {
  const item: Record<string, unknown> = { kind: "doc", body, logicalId, sourceId: SOURCE_ID };
  if (validFrom !== null) item.validFrom = validFrom;
  if (validUntil !== null) item.validUntil = validUntil;
  return item;
}

async function openEngine(path: string): Promise<Engine> {
  return Engine.open(path, { useDefaultEmbedder: false });
}

/** Seed a batch on a fresh engine, drain, and CLOSE — freeing the file. */
async function seed(path: string, batch: object[]): Promise<void> {
  const engine = await openEngine(path);
  try {
    await engine.write(batch);
    await engine.drain(30_000);
  } finally {
    await engine.close();
  }
}

/** Read a window back from the raw table — the data-at-rest oracle. */
function rawWindow(path: string, logicalId: string): [number | null, number | null] {
  const db = new DatabaseSync(path);
  try {
    const row = db
      .prepare(
        "SELECT valid_from AS f, valid_until AS u FROM canonical_nodes" +
          " WHERE logical_id = ? AND superseded_at IS NULL",
      )
      .get(logicalId) as { f: number | null; u: number | null } | undefined;
    assert.ok(row !== undefined, `no current row on disk for ${logicalId}`);
    return [row.f === null ? null : Number(row.f), row.u === null ? null : Number(row.u)];
  } finally {
    db.close();
  }
}

/**
 * The bodies present in the FTS projection at rest. Without this the leak tests
 * could pass because nothing was ever searchable in the first place.
 */
function rawIndexedBodies(path: string): string[] {
  const db = new DatabaseSync(path);
  try {
    const rows = db.prepare("SELECT body FROM search_index").all() as { body: string }[];
    return rows.map((r) => String(r.body)).sort();
  } finally {
    db.close();
  }
}

function bodies(result: SearchResult): string[] {
  return result.results.map((h) => h.body).sort();
}

// ---------------------------------------------------------------------------
// (1) The leak, both directions, with its control
// ---------------------------------------------------------------------------

test("fix-2: a node whose validUntil is in the past does not leak through search", async () => {
  const path = freshDbPath();
  await seed(path, [
    windowed("EXPIRED", "quarterly telemetry report", null, FAR_PAST_UNTIL),
    windowed("ALWAYS", "quarterly telemetry summary", null, null),
  ]);

  // Data-at-rest oracle: the window landed, and BOTH bodies reached FTS — so a
  // later absence is a filtering decision, not a missing index row.
  assert.deepStrictEqual(rawWindow(path, "EXPIRED"), [null, FAR_PAST_UNTIL]);
  assert.deepStrictEqual(rawWindow(path, "ALWAYS"), [null, null]);
  const indexed = rawIndexedBodies(path);
  assert.ok(
    indexed.some((b) => b.includes("telemetry report")),
    `expired node must be in search_index (else this test is vacuous): ${JSON.stringify(indexed)}`,
  );
  assert.ok(indexed.some((b) => b.includes("telemetry summary")));

  const engine = await openEngine(path);
  try {
    assert.deepStrictEqual(bodies(await engine.search("telemetry")), [
      "quarterly telemetry summary",
    ]);
  } finally {
    await engine.close();
  }
});

test("fix-2: a node whose validFrom is in the future does not leak through search", async () => {
  const path = freshDbPath();
  await seed(path, [
    windowed("PENDING", "embargoed launch memo", FAR_FUTURE, null),
    windowed("ALWAYS", "published launch note", null, null),
  ]);

  assert.deepStrictEqual(rawWindow(path, "PENDING"), [FAR_FUTURE, null]);
  assert.ok(rawIndexedBodies(path).some((b) => b.includes("embargoed launch memo")));

  const engine = await openEngine(path);
  try {
    assert.deepStrictEqual(bodies(await engine.search("launch")), ["published launch note"]);
  } finally {
    await engine.close();
  }
});

test("fix-2: a node whose window covers now IS still returned by search", async () => {
  const path = freshDbPath();
  await seed(path, [windowed("COVERING", "in force policy text", 1_000, FAR_FUTURE)]);

  const engine = await openEngine(path);
  try {
    assert.deepStrictEqual(bodies(await engine.search("policy")), ["in force policy text"]);
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (2) The no-regression guard
// ---------------------------------------------------------------------------

test("fix-2: default search is unchanged on a corpus with no authored windows", async () => {
  const path = freshDbPath();
  await seed(path, [
    windowed("A", "alpha retrieval corpus", null, null),
    windowed("B", "beta retrieval corpus", null, null),
    windowed("C", "gamma retrieval corpus", null, null),
  ]);

  // The premise the whole no-op argument rests on, asserted rather than assumed:
  // a write that omits the window lands NULL/NULL, and NULL is unbounded.
  for (const id of ["A", "B", "C"]) {
    assert.deepStrictEqual(rawWindow(path, id), [null, null]);
  }

  const engine = await openEngine(path);
  try {
    const expected = ["alpha retrieval corpus", "beta retrieval corpus", "gamma retrieval corpus"];
    assert.deepStrictEqual(bodies(await engine.search("retrieval")), expected);
    // Inert under a pinned instant on both sides, and under the relaxed view —
    // a NULL/NULL row is valid at EVERY instant.
    assert.deepStrictEqual(
      bodies(await engine.search("retrieval", undefined, 0, false, 0.3, 0, false, { validAsOf: 1 })),
      expected,
    );
    assert.deepStrictEqual(
      bodies(
        await engine.search("retrieval", undefined, 0, false, 0.3, 0, false, {
          validAsOf: FAR_FUTURE,
        }),
      ),
      expected,
    );
    assert.deepStrictEqual(
      bodies(
        await engine.search("retrieval", undefined, 0, false, 0.3, 0, false, {
          includeOutOfWindow: true,
        }),
      ),
      expected,
    );
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (3) The escape hatch
// ---------------------------------------------------------------------------

test("fix-2: search accepts a ReadView — validAsOf selects by instant", async () => {
  const path = freshDbPath();
  await seed(path, [
    windowed("EARLY", "epoch alpha record", 1_000, 2_000),
    windowed("LATE", "epoch beta record", 3_000, null),
  ]);

  const engine = await openEngine(path);
  try {
    const at = async (t: number): Promise<string[]> =>
      bodies(await engine.search("epoch", undefined, 0, false, 0.3, 0, false, { validAsOf: t }));

    // Default view: real now is past both bounds, so only LATE is valid.
    assert.deepStrictEqual(bodies(await engine.search("epoch")), ["epoch beta record"]);

    assert.deepStrictEqual(await at(1_500), ["epoch alpha record"]);
    // Half-open: `== validUntil` is OUT, `== validFrom` is IN.
    assert.deepStrictEqual(await at(2_000), []);
    assert.deepStrictEqual(await at(3_000), ["epoch beta record"]);
    // Between the two windows — neither is valid.
    assert.deepStrictEqual(await at(2_500), []);

    // Relaxed — both, whatever their window.
    assert.deepStrictEqual(
      bodies(
        await engine.search("epoch", undefined, 0, false, 0.3, 0, false, {
          includeOutOfWindow: true,
        }),
      ),
      ["epoch alpha record", "epoch beta record"],
    );
  } finally {
    await engine.close();
  }
});

test("fix-2: searchTextOnly takes the same predicate and the same escape hatch", async () => {
  const path = freshDbPath();
  await seed(path, [
    windowed("EXPIRED", "retired runbook entry", null, FAR_PAST_UNTIL),
    windowed("ALWAYS", "current runbook entry", null, null),
  ]);

  const engine = await openEngine(path);
  try {
    assert.deepStrictEqual(bodies(await engine.searchTextOnly("runbook")), [
      "current runbook entry",
    ]);
    assert.deepStrictEqual(
      bodies(await engine.searchTextOnly("runbook", { includeOutOfWindow: true })),
      ["current runbook entry", "retired runbook entry"],
    );
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// (4) Scope guard — the existence axis is REFUSED, never silently ignored
// ---------------------------------------------------------------------------

test("fix-2: search refuses a view that relaxes the existence axis", async () => {
  const path = freshDbPath();
  await seed(path, [windowed("A", "scope guard body", null, null)]);

  const engine = await openEngine(path);
  try {
    for (const view of [{ includeSuperseded: true }, { includeInactive: true }]) {
      await assert.rejects(
        () => engine.search("scope", undefined, 0, false, 0.3, 0, false, view),
        InvalidArgumentError,
        `existence flags on a search view must be a TYPED refusal: ${JSON.stringify(view)}`,
      );
    }
    // The validity axis alone is accepted.
    assert.deepStrictEqual(
      bodies(
        await engine.search("scope", undefined, 0, false, 0.3, 0, false, {
          includeOutOfWindow: true,
        }),
      ),
      ["scope guard body"],
    );
  } finally {
    await engine.close();
  }
});
