// FFI safety-contract tests for the napi-rs binding.
//
// Covers AC-067 (panic catch), AC-068a (embedded NUL rejected), and
// AC-068b (unpaired UTF-16 surrogate rejected). Also covers fix-28 [P2]:
// ingestWithExtractor validates cmd/sourceDocId/body strings before the
// native call. Mirrors `src/python/tests/test_ffi_safety.py` from Phase 11a.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine, admin } from "../src/index.js";
import {
  FathomDbError,
  FathomDbPanicError,
  WriteValidationError,
  rethrowTyped,
} from "../src/errors.js";
import { native } from "../src/binding.js";
import { freshDbPath } from "./helpers.js";

const NUL = String.fromCharCode(0);
const SURROGATE = String.fromCharCode(0xd800);

test("AC-067 panic surfaces as FathomDbPanicError, process unchanged", async () => {
  const pidBefore = process.pid;

  assert.equal(typeof native.forcePanicForTest, "function", "force-panic hook must be exposed");

  assert.throws(
    () => {
      try {
        native.forcePanicForTest!();
      } catch (err) {
        rethrowTyped(err);
      }
    },
    (err: unknown) => {
      assert.ok(err instanceof FathomDbPanicError, "must be FathomDbPanicError");
      assert.ok(!(err instanceof FathomDbError), "panic must NOT subclass FathomDbError");
      return true;
    },
  );

  assert.equal(process.pid, pidBefore, "host process must not be aborted by engine panic");

  const engine = await Engine.open(freshDbPath());
  try {
    const snap = engine.counters();
    assert.ok(snap !== undefined);
  } finally {
    await engine.close();
  }
});

test("AC-067 panic on sync accessor surfaces as FathomDbPanicError", () => {
  assert.equal(
    typeof native.forcePanicInAccessorForTest,
    "function",
    "sync force-panic hook must be exposed",
  );

  assert.throws(
    () => {
      try {
        native.forcePanicInAccessorForTest!();
      } catch (err) {
        rethrowTyped(err);
      }
    },
    (err: unknown) => {
      assert.ok(err instanceof FathomDbPanicError, "must be FathomDbPanicError");
      assert.ok(!(err instanceof FathomDbError), "panic must NOT subclass FathomDbError");
      assert.match((err as Error).message, /engine panic/);
      return true;
    },
  );
});

test("AC-068a embedded NUL in op-store body rejected as WriteValidationError", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await admin.configure(engine, { name: "nul_col", body: "{}" });
    const before = engine.counters().writeRows;

    await assert.rejects(
      () =>
        engine.write([
          {
            opStore: {
              collection: "nul_col",
              recordKey: "k1",
              body: `{"x":"a${NUL}b"}`,
            },
          },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        assert.ok(err instanceof FathomDbError, "must extend FathomDbError");
        return true;
      },
    );

    const after = engine.counters().writeRows;
    assert.equal(after, before, "no row may be written when a NUL is rejected");
  } finally {
    await engine.close();
  }
});

test("AC-068b unpaired UTF-16 surrogate in op-store body rejected as WriteValidationError", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await admin.configure(engine, { name: "sur_col", body: "{}" });
    const before = engine.counters().writeRows;

    await assert.rejects(
      () =>
        engine.write([
          {
            opStore: {
              collection: "sur_col",
              recordKey: "k1",
              body: `{"x":"a${SURROGATE}b"}`,
            },
          },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        assert.ok(err instanceof FathomDbError, "must extend FathomDbError");
        return true;
      },
    );

    const after = engine.counters().writeRows;
    assert.equal(after, before, "no row may be written when a surrogate is rejected");
  } finally {
    await engine.close();
  }
});

test("AC-068a embedded NUL in node kind also rejected before write", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await assert.rejects(
      () =>
        engine.write([
          // A VALID `sourceId` (0.8.20 R-20-E3) so the rejection is attributable to
          // the embedded NUL in `kind`, not to missing provenance.
          { kind: `do${NUL}c`, body: "{}", sourceId: "ts-test:ffi-safety" },
        ]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError);
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});

// --- Slice 10 fix-1: G10 SearchFilter string fields cross the FFI too ---
// The new filter fields (sourceType, kind, status) are FFI strings and must be
// routed through the same validate_ffi_string_napi gate as `query` and the
// write fields. createdAfter is a number — no string validation. These pin the
// *binding wiring* (that search's filter args reject NUL / lone surrogate
// before the query reaches the engine).

const FILTER_STRING_FIELDS = ["sourceType", "kind", "status"] as const;

for (const field of FILTER_STRING_FIELDS) {
  test(`AC-068a embedded NUL in search filter ${field} rejected as WriteValidationError`, async () => {
    const engine = await Engine.open(freshDbPath());
    try {
      await assert.rejects(
        () => engine.search("q", { [field]: `a${NUL}b` }),
        (err: unknown) => {
          assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
          assert.ok(err instanceof FathomDbError, "must extend FathomDbError");
          return true;
        },
      );
    } finally {
      await engine.close();
    }
  });

  test(`AC-068b unpaired UTF-16 surrogate in search filter ${field} rejected as WriteValidationError`, async () => {
    const engine = await Engine.open(freshDbPath());
    try {
      await assert.rejects(
        () => engine.search("q", { [field]: `a${SURROGATE}b` }),
        (err: unknown) => {
          assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
          assert.ok(err instanceof FathomDbError, "must extend FathomDbError");
          return true;
        },
      );
    } finally {
      await engine.close();
    }
  });
}

// fix-28 [P2]: ingestWithExtractor validates cmd/sourceDocId/body at the FFI boundary.
test("fix-28 NUL in ingestWithExtractor sourceDocId rejected", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await assert.rejects(
      () => engine.ingestWithExtractor(["echo"], [{ sourceDocId: `id${NUL}x`, body: "text" }]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});

test("fix-28 NUL in ingestWithExtractor body rejected", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await assert.rejects(
      () => engine.ingestWithExtractor(["echo"], [{ sourceDocId: "id", body: `te${NUL}xt` }]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});

test("fix-28 NUL in ingestWithExtractor cmd arg rejected", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    await assert.rejects(
      () => engine.ingestWithExtractor([`ec${NUL}ho`], [{ sourceDocId: "id", body: "text" }]),
      (err: unknown) => {
        assert.ok(err instanceof WriteValidationError, "must be WriteValidationError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});
