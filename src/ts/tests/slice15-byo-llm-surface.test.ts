// Slice 15 (G11) — TypeScript parity test: BYO-LLM surface assertions.
//
// Pins that:
//   1. `ExtractorError` is exported and is a `FathomDbError` subclass.
//   2. `Engine.prototype.ingestWithExtractor` exists and is callable.
//   3. `IngestWithExtractorReceipt` and `ExtractDocument` types are correct.
//
// These are surface-only assertions (not functional end-to-end tests).
// Functional conformance is in the Rust test suite
// (`tests/slice15_byo_llm_ingest.rs`).

import test from "node:test";
import assert from "node:assert/strict";

import {
  Engine,
  type ExtractDocument,
  type IngestWithExtractorReceipt,
} from "../src/index.js";
import { ExtractorError, FathomDbError } from "../src/errors.js";
import { freshDbPath } from "./helpers.js";

test("ExtractorError is a FathomDbError subclass", () => {
  const err = new ExtractorError("extractor error");
  assert.ok(err instanceof FathomDbError, "ExtractorError must be a FathomDbError");
  assert.ok(err instanceof Error, "ExtractorError must be an Error");
  assert.equal(err.message, "extractor error");
});

test("Engine.ingestWithExtractor is a callable method", async () => {
  const engine = await Engine.open(freshDbPath());
  try {
    assert.equal(
      typeof engine.ingestWithExtractor,
      "function",
      "Engine.ingestWithExtractor must be a function",
    );
  } finally {
    await engine.close();
  }
});

test("IngestWithExtractorReceipt shape is correct (structural type check)", async () => {
  // Verify the expected shape by constructing an object that satisfies
  // the type (compile-time check), and asserting the fields exist.
  const mockReceipt: IngestWithExtractorReceipt = {
    nodesWritten: 0,
    edgesWritten: 0,
    docsProcessed: 0,
  };
  assert.equal(typeof mockReceipt.nodesWritten, "number");
  assert.equal(typeof mockReceipt.edgesWritten, "number");
  assert.equal(typeof mockReceipt.docsProcessed, "number");
});

test("ExtractDocument shape is correct (structural type check)", () => {
  const doc: ExtractDocument = {
    sourceDocId: "doc-001",
    body: "Alice owns Project X",
  };
  assert.equal(typeof doc.sourceDocId, "string");
  assert.equal(typeof doc.body, "string");
});
