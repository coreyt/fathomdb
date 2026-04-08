import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, handleScenarioError, tempDbPath } from "../skip.js";

const VECTOR_DIM = 4;

export function vectorScenario(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("vector"), { vectorDimension: VECTOR_DIM });

    const builder = new WriteRequestBuilder("vector-ingest");
    const node = builder.addNode({
      rowId: newRowId(), logicalId: newId(), kind: "Embedding",
      properties: { topic: "machine learning" }, sourceRef: "vector-test",
    });
    const chunk = builder.addChunk({
      id: newId(), node, textContent: "Neural networks for classification.",
    });
    builder.addVecInsert({
      chunk,
      embedding: [0.1, 0.2, 0.3, 0.4],
    });
    const receipt = engine.write(builder.build());
    assert(receipt.warnings.length === 0, "unexpected write warnings");

    const rows = engine.nodes("Embedding")
      .vectorSearch("classification", 5)
      .execute();
    assert(rows.nodes.length >= 1, `expected ≥1 vector result, got ${rows.nodes.length}`);
    assert(rows.nodes[0].kind === "Embedding", "kind mismatch");

    const standard = engine.nodes("Embedding").execute();
    assert(standard.nodes.length >= 1, "standard query should return results");

    engine.close();
    return { name: "vector", ok: true };
  } catch (error) {
    return handleScenarioError("vector", error);
  }
}
