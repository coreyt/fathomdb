// AC-060b — payload-vs-schema validation surfaces as
// `SchemaValidationError` and writes no row.
//
// A registered op-store schema is the contract the engine validates
// against on each op-store write; a body that violates the schema must
// fail BEFORE the row commits. Mirrors
// `src/python/tests/test_save_time_validation.py` from Phase 11a.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine, admin } from "../src/index.js";
import { SchemaValidationError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

test("op-store write violating schema raises SchemaValidationError and writes no row", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    const schemaBody = '{"type":"object","required":["foo"]}';
    await admin.configure(engine, { name: "strict_col", body: schemaBody });
    const before = engine.counters().writeRows;

    await assert.rejects(
      () =>
        engine.write([
          {
            opStore: {
              collection: "strict_col",
              recordKey: "k1",
              schemaId: "strict_col",
              body: "{}",
            },
          },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof SchemaValidationError);
        return true;
      },
    );

    const after = engine.counters().writeRows;
    assert.equal(after, before, "schema-violating write must not commit a row");
  } finally {
    await engine.close();
  }
});
