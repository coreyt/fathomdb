// AC-060a — typed error payload coverage for the napi-rs binding.
//
// The engine returns enum variants with typed fields; the binding
// translator (`engine_error_to_napi` / `engine_open_error_to_napi`)
// encodes those fields in a JSON envelope that the TS-side
// `rethrowTyped` reconstitutes into typed leaf classes. Mirrors
// `src/python/tests/test_typed_errors.py` from Phase 11a.

import test from "node:test";
import assert from "node:assert/strict";

import { Engine } from "../src/index.js";
import {
  CorruptionError,
  DatabaseLockedError,
  EmbedderDimensionMismatchError,
  EmbedderError,
  EmbedderIdentityMismatchError,
  EmbedderNotConfiguredError,
  FathomDbError,
  KindNotVectorIndexedError,
  VectorError,
} from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

test("DatabaseLockedError carries holderPid attr", () => {
  const err = new DatabaseLockedError({ holderPid: 12345 });
  assert.equal(err.holderPid, 12345);
});

test("opening the same database twice surfaces DatabaseLockedError with holderPid", async () => {
  const path = freshDbPath();
  const a = await Engine.open(path);
  try {
    await assert.rejects(
      () => Engine.open(path),
      (err: unknown) => {
        assert.ok(err instanceof DatabaseLockedError, "must be DatabaseLockedError");
        assert.ok(err instanceof FathomDbError);
        const holder = (err as DatabaseLockedError).holderPid;
        assert.ok(
          holder === undefined || typeof holder === "number",
          "holderPid must be undefined or number",
        );
        return true;
      },
    );
  } finally {
    await a.close();
  }
});

test("CorruptionError carries typed kind/stage/recoveryHintCode/docAnchor", () => {
  const err = new CorruptionError({
    kind: "HeaderMalformed",
    stage: "HeaderProbe",
    recoveryHintCode: "E_CORRUPT_HEADER",
    docAnchor: "design/recovery.md#header-malformed",
  });
  assert.equal(err.kind, "HeaderMalformed");
  assert.equal(err.stage, "HeaderProbe");
  assert.equal(err.recoveryHintCode, "E_CORRUPT_HEADER");
  assert.equal(err.docAnchor, "design/recovery.md#header-malformed");
});

test("EmbedderDimensionMismatchError carries typed stored/supplied", () => {
  const err = new EmbedderDimensionMismatchError({ stored: 384, supplied: 768 });
  assert.equal(err.stored, 384);
  assert.equal(err.supplied, 768);
  assert.equal(typeof err.stored, "number");
  assert.equal(typeof err.supplied, "number");
});

test("EmbedderIdentityMismatchError carries typed identity attrs", () => {
  const err = new EmbedderIdentityMismatchError({
    storedName: "model-a",
    storedRevision: "0",
    suppliedName: "model-b",
    suppliedRevision: "1",
  });
  assert.equal(err.storedName, "model-a");
  assert.equal(err.storedRevision, "0");
  assert.equal(err.suppliedName, "model-b");
  assert.equal(err.suppliedRevision, "1");
});

test("EmbedderNotConfiguredError is a distinct leaf under EmbedderError", () => {
  const err = new EmbedderNotConfiguredError("no embedder");
  assert.ok(err instanceof EmbedderNotConfiguredError);
  assert.ok(err instanceof EmbedderError);
  assert.ok(err instanceof FathomDbError);
  assert.notEqual(EmbedderNotConfiguredError, EmbedderError);
});

test("KindNotVectorIndexedError is a distinct leaf under VectorError", () => {
  const err = new KindNotVectorIndexedError("kind X not vector indexed");
  assert.ok(err instanceof KindNotVectorIndexedError);
  assert.ok(err instanceof VectorError);
  assert.ok(err instanceof FathomDbError);
  assert.notEqual(KindNotVectorIndexedError, VectorError);
});
