import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, handleScenarioError, tempDbPath } from "../skip.js";

export function canonicalScenario(): HarnessResult {
  try {
    const dbPath = tempDbPath("canonical");
    const engine = Engine.open(dbPath);

    const builder = new WriteRequestBuilder("canonical-ingest");
    const node = builder.addNode({
      rowId: newRowId(),
      logicalId: newId(),
      kind: "Document",
      properties: { title: "Budget Report", year: 2026 },
      sourceRef: "canonical-test",
    });
    builder.addChunk({
      id: newId(),
      node,
      textContent: "Full text of the budget report for fiscal year 2026.",
    });
    const receipt = engine.write(builder.build());
    assert(receipt.label === "canonical-ingest", "receipt label mismatch");
    assert(receipt.warnings.length === 0, "unexpected write warnings");

    const rows = engine.nodes("Document").execute();
    assert(rows.nodes.length >= 1, `expected ≥1 node, got ${rows.nodes.length}`);
    assert(rows.nodes[0].kind === "Document", "kind mismatch");
    assert(rows.wasDegraded === false, "query was degraded");

    const snap = engine.telemetrySnapshot();
    assert(snap.writesTotal >= 1, "writes_total should be ≥1");
    assert(snap.queriesTotal >= 1, "queries_total should be ≥1");

    engine.close();
    return { name: "canonical", ok: true };
  } catch (error) {
    return handleScenarioError("canonical", error);
  }
}
