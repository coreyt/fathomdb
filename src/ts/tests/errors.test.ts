// Exception-hierarchy assertions for the TypeScript SDK.
//
// Per `dev/design/errors.md` § Binding-facing class matrix and
// `dev/design/bindings.md` § 3, every leaf class extends FathomDbError so
// callers can `instanceof` narrow per variant or catch the catch-all base.

import test from "node:test";
import assert from "node:assert/strict";

import {
  ClosingError,
  CorruptionError,
  DatabaseLockedError,
  EmbedderDimensionMismatchError,
  EmbedderError,
  EmbedderIdentityMismatchError,
  FathomDbError,
  IncompatibleSchemaVersionError,
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
] as const;

test("every leaf class extends FathomDbError", () => {
  for (const Cls of LEAF_CLASSES) {
    const instance = Object.create(Cls.prototype) as object;
    assert.ok(instance instanceof FathomDbError, `${Cls.name} must extend FathomDbError`);
    assert.notEqual(Cls, FathomDbError);
  }
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
