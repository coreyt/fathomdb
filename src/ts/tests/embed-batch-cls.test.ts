// Slice 40 — module-level CLS-pooling embedder parity with Python's
// `fathomdb.embed_batch_cls`. The empty-batch arm stays offline: it proves the
// public TS facade reaches the native export without downloading model weights.

import test from "node:test";
import assert from "node:assert/strict";

import { embedBatchCls } from "../src/index.js";
import { native } from "../src/binding.js";

test("embedBatchCls is a public async facade over the native module export", async () => {
  assert.equal(typeof embedBatchCls, "function");
  assert.equal(typeof native.embedBatchCls, "function");
  assert.deepEqual(await embedBatchCls([]), []);
  assert.deepEqual(await native.embedBatchCls([]), []);
});

test("embedBatchCls returns one L2-normalized BGE-small CLS vector per input", async (t) => {
  if (process.env.FATHOMDB_SKIP_NETWORK_TESTS) {
    t.skip("FATHOMDB_SKIP_NETWORK_TESTS set; skipping default-embedder test");
    return;
  }
  const vectors = await embedBatchCls(["zephyr", "anchor"]);
  assert.equal(vectors.length, 2);
  for (const vector of vectors) {
    assert.equal(vector.length, 384);
    const norm = Math.sqrt(vector.reduce((sum, value) => sum + value * value, 0));
    assert.ok(Math.abs(norm - 1) < 1e-5, `CLS vector must be L2-normalized; got ${norm}`);
  }
});
