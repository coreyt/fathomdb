// 0.8.19 Slice 15 / C-2 typed `SearchHit.id` swap (TC-8, R-ID-2 / R-X-1) —
// TypeScript SDK X1 parity.
//
// Mirrors the engine contract in
// `src/rust/crates/fathomdb-engine/tests/tc8_idspace_swap.rs` and the Python
// half `src/python/tests/test_idspace_parity.py` (same corpus, same
// assertions): `SearchHit.id` is the typed `IdSpace` (`{ space, value }`),
// non-null and id-space-total — a governed node (carries a `logicalId`)
// surfaces the `logical` (`l:`) space; a doc-seeded node surfaces the `content`
// (`h:`) space. The bare `value` round-trips (prefix + value reconstructs the
// pre-0.8.19 `stableId` string). The pre-C-2 int `write_cursor` id is engine-
// internal and no longer surfaced.

import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";

import { Engine } from "../src/index.js";
import type { IdSpace, SearchHit } from "../src/index.js";
import { freshDbPath } from "./helpers.js";

// The id-space prefixes (mirror the engine `IdSpaceKind::prefix`). No SDK helper
// yields the prefixed form, so a consumer keying real-gold reconstructs it as
// `${prefix}${value}` — byte-identical to the pre-0.8.19 `stableId`.
const PREFIX: Record<string, string> = {
  logical: "l:",
  content: "h:",
  passage: "p:",
};

/** Reconstruct the pre-0.8.19 `stableId` string from the typed id. */
function prefixed(id: IdSpace): string {
  return `${PREFIX[id.space]}${id.value}`;
}

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

test("idspace: governed hit id is the logical space", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([
      { kind: "person", body: "idspace governed entity payload", logicalId: "gov-ts-1" },
    ]);
    await engine.drain(30_000);
    const hits = await searchAfterProjection(engine, "governed");
    assert.ok(hits.length > 0, "expected a governed hit");
    const hit = hits[0]!;
    // Typed carrier (not a magic-prefixed string): a real IdSpace object with
    // the lowercase discriminant + bare value.
    assert.equal(hit.id.space, "logical");
    assert.equal(hit.id.value, "gov-ts-1");
    // eu7 NO-OP (SDK-visible): the reconstructed prefixed/stable form — the key
    // a consumer would key real-gold on — is byte-identical to the pre-0.8.19
    // stableId `"l:" + logicalId`.
    assert.equal(prefixed(hit.id), "l:gov-ts-1");
  } finally {
    await engine.close();
  }
});

test("idspace: doc-seeded hit id is the content space", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const body = "idspace anonymous docseeded xyzzy";
    await engine.write([{ kind: "doc", body }]);
    await engine.drain(30_000);
    const hits = await searchAfterProjection(engine, "docseeded");
    assert.ok(hits.length > 0, "expected a doc-seeded hit");
    const hit = hits[0]!;
    assert.equal(hit.id.space, "content");
    assert.equal(hit.id.value.length, 64);
    assert.ok(/^[0-9a-f]+$/.test(hit.id.value), "content hash is lowercase hex");
    // eu7 NO-OP (SDK-visible): the reconstructed prefixed/stable form is the
    // EXACT pre-0.8.19 stableId `"h:" + sha256(body)` byte-for-byte.
    const expected = `h:${createHash("sha256").update(body).digest("hex")}`;
    assert.equal(prefixed(hit.id), expected);
  } finally {
    await engine.close();
  }
});

test("idspace: every hit id is non-null and space-total", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await engine.write([
      { kind: "person", body: "idspace total governed totalterm", logicalId: "tot-ts-1" },
      { kind: "doc", body: "idspace total anonymous totalterm" },
    ]);
    await engine.drain(30_000);
    const hits = await searchAfterProjection(engine, "totalterm");
    assert.ok(hits.length >= 2);
    for (const hit of hits) {
      assert.ok(["logical", "content", "passage"].includes(hit.id.space));
      assert.ok(hit.id.value.length > 0);
    }
    const spaces = new Set(hits.map((h) => h.id.space));
    assert.ok(spaces.has("logical"));
    assert.ok(spaces.has("content"));
  } finally {
    await engine.close();
  }
});
