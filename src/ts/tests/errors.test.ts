// Exception-hierarchy assertions for the TypeScript SDK.
//
// Per `dev/design/errors.md` § Binding-facing class matrix and
// `dev/design/bindings.md` § 3, every leaf class extends FathomDbError so
// callers can `instanceof` narrow per variant or catch the catch-all base.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import {
  ClosingError,
  CorruptionError,
  DatabaseLockedError,
  EmbedderDimensionMismatchError,
  EmbedderError,
  EmbedderIdentityMismatchError,
  EmbedderNotConfiguredError,
  FathomDbError,
  IncompatibleSchemaVersionError,
  InvalidArgumentError,
  KindNotVectorIndexedError,
  MigrationError,
  OpStoreError,
  OverloadedError,
  ProjectionError,
  SchedulerError,
  SchemaValidationError,
  StorageError,
  VectorError,
  WriteValidationError,
} from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

const LEAF_CLASSES = [
  StorageError,
  ProjectionError,
  VectorError,
  EmbedderError,
  SchedulerError,
  OpStoreError,
  WriteValidationError,
  SchemaValidationError,
  OverloadedError,
  ClosingError,
  DatabaseLockedError,
  CorruptionError,
  IncompatibleSchemaVersionError,
  MigrationError,
  EmbedderIdentityMismatchError,
  EmbedderDimensionMismatchError,
  EmbedderNotConfiguredError,
  KindNotVectorIndexedError,
  // 0.8.20 Slice 22 / decision #18 — InvalidArgumentError was absent from both
  // tables in dev/design/errors.md despite being a live SDK class. The
  // settlement adds it to the taxonomy of record; this row pins it here too.
  InvalidArgumentError,
] as const;

test("every leaf class extends FathomDbError", () => {
  for (const Cls of LEAF_CLASSES) {
    const instance = Object.create(Cls.prototype) as object;
    assert.ok(instance instanceof FathomDbError, `${Cls.name} must extend FathomDbError`);
    assert.notEqual(Cls, FathomDbError);
  }
});

test("EmbedderNotConfiguredError sits under EmbedderError under FathomDbError", () => {
  const err = Object.create(EmbedderNotConfiguredError.prototype) as object;
  assert.ok(err instanceof EmbedderNotConfiguredError);
  assert.ok(err instanceof EmbedderError);
  assert.ok(err instanceof FathomDbError);
  assert.notEqual(EmbedderNotConfiguredError, EmbedderError);
});

test("KindNotVectorIndexedError sits under VectorError under FathomDbError", () => {
  const err = Object.create(KindNotVectorIndexedError.prototype) as object;
  assert.ok(err instanceof KindNotVectorIndexedError);
  assert.ok(err instanceof VectorError);
  assert.ok(err instanceof FathomDbError);
  assert.notEqual(KindNotVectorIndexedError, VectorError);
});

test("CorruptionError carries typed recovery hint payload", () => {
  const err = new CorruptionError({
    kind: "HeaderMalformed",
    stage: "HeaderProbe",
    recoveryHintCode: "E_CORRUPT_HEADER",
    docAnchor: "design/recovery.md#header-malformed",
  });
  assert.ok(err instanceof FathomDbError);
  assert.equal(err.kind, "HeaderMalformed");
  assert.equal(err.stage, "HeaderProbe");
  assert.equal(err.recoveryHintCode, "E_CORRUPT_HEADER");
  assert.equal(err.docAnchor, "design/recovery.md#header-malformed");
});

test("DatabaseLockedError carries typed holderPid", () => {
  const err = new DatabaseLockedError({ holderPid: 12345 });
  assert.equal(err.holderPid, 12345);
});

test("EmbedderIdentityMismatchError carries typed identity attrs", () => {
  const err = new EmbedderIdentityMismatchError({
    storedName: "model-a",
    storedRevision: "0",
    suppliedName: "model-b",
    suppliedRevision: "1",
  });
  assert.equal(err.storedName, "model-a");
  assert.equal(err.suppliedName, "model-b");
});

test("EmbedderDimensionMismatchError carries typed dimensions", () => {
  const err = new EmbedderDimensionMismatchError({ stored: 384, supplied: 768 });
  assert.equal(err.stored, 384);
  assert.equal(err.supplied, 768);
});

test("search rejects empty query via WriteValidationError under FathomDbError root", async () => {
  // Per dev/design/errors.md section Binding-facing class matrix, the
  // empty-query rejection must surface as the typed WriteValidationError
  // leaf beneath the single-rooted FathomDbError, not as a bare Error.
  const engine = await Engine.open(freshDbPath());
  try {
    await assert.rejects(
      () => engine.search(""),
      (err: unknown) => {
        assert.ok(err instanceof FathomDbError, "must be a FathomDbError");
        assert.ok(err instanceof WriteValidationError, "must be a WriteValidationError");
        return true;
      },
    );
  } finally {
    await engine.close();
  }
});

// ---------------------------------------------------------------------------
// 0.8.20 Slice 22 (R-20-VC) — decision #18: one family at the write boundary
// ---------------------------------------------------------------------------

const SOURCE_ID = "ts-test:decision-18";

test("decision #18: the write-validation boundary is exactly one error family", async () => {
  // The consumer-visible defect this settles: the SAME engine.write() call used
  // to reject an inverted validity window with InvalidArgumentError but a
  // non-integer bound with WriteValidationError. dev/design/errors.md defines
  // WriteValidationError as "malformed typed write shape", which is exactly what
  // that boundary checks.
  const engine = await Engine.open(freshDbPath(), { useDefaultEmbedder: false });
  try {
    const cases: Array<[string, Record<string, unknown>]> = [
      ["empty body", { kind: "doc", body: "   ", logicalId: "B1", sourceId: SOURCE_ID }],
      ["empty logicalId", { kind: "doc", body: "ok", logicalId: "", sourceId: SOURCE_ID }],
      [
        "inverted window",
        {
          kind: "doc",
          body: "ok",
          logicalId: "W1",
          sourceId: SOURCE_ID,
          validFrom: 2000,
          validUntil: 1000,
        },
      ],
      [
        "empty half-open window",
        {
          kind: "doc",
          body: "ok",
          logicalId: "W2",
          sourceId: SOURCE_ID,
          validFrom: 1500,
          validUntil: 1500,
        },
      ],
      [
        "non-integer bound",
        { kind: "doc", body: "ok", logicalId: "W3", sourceId: SOURCE_ID, validFrom: "1000" },
      ],
    ];
    for (const [label, item] of cases) {
      await assert.rejects(
        () => engine.write([item as never]),
        (err: unknown) => {
          assert.ok(err instanceof WriteValidationError, `${label} must be WriteValidationError`);
          assert.ok(
            !(err instanceof InvalidArgumentError),
            `${label} must not be InvalidArgumentError (decision #18)`,
          );
          return true;
        },
      );
    }
  } finally {
    await engine.close();
  }
});

test("decision #18: InvalidArgumentError survives as a distinct leaf", () => {
  // The settlement narrows WHERE InvalidArgumentError is used; it does not remove
  // it. It stays the message-carrying class for caller-argument rejections
  // OUTSIDE the write-validation boundary (e.g. an out-of-range traversal depth).
  const err = new InvalidArgumentError("depth must be 1-3, got 9");
  assert.ok(err instanceof FathomDbError);
  assert.ok(!(err instanceof WriteValidationError));
  assert.ok(!(new WriteValidationError("x") instanceof InvalidArgumentError));
  assert.match(err.message, /depth must be 1-3/);
});
