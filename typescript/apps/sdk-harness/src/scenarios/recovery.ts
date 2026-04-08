import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, handleScenarioError, tempDbPath } from "../skip.js";

export function recoveryScenario(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("recovery"));

    const sourceRef = "recovery-test";
    const builder = new WriteRequestBuilder("recovery-seed");
    const logicalId = newId();
    builder.addNode({
      rowId: newRowId(), logicalId, kind: "Document",
      properties: { title: "Recoverable" }, sourceRef,
    });
    engine.write(builder.build());

    const integrity = engine.admin.checkIntegrity();
    assert(integrity.physicalOk, "physical integrity should be ok");
    assert(integrity.foreignKeysOk, "foreign keys should be ok");

    const semantics = engine.admin.checkSemantics();
    assert(semantics.orphanedChunks === 0, "no orphaned chunks expected");

    const trace = engine.admin.traceSource(sourceRef);
    assert(trace.sourceRef === sourceRef, "trace source_ref mismatch");
    assert(trace.nodeRows >= 1, "should trace ≥1 node");

    const rebuild = engine.admin.rebuild("all");
    assert(Array.isArray(rebuild.notes), "rebuild should have notes array");

    const rebuildMissing = engine.admin.rebuildMissing();
    assert(typeof rebuildMissing.rebuiltRows === "number", "rebuiltRows should be number");

    const excise = engine.admin.exciseSource(sourceRef);
    assert(excise.nodeRows >= 1, "should excise ≥1 node");

    const afterExcise = engine.nodes("Document")
      .filterLogicalIdEq(logicalId)
      .execute();
    assert(afterExcise.nodes.length === 0, "excised node should be gone");

    const builder2 = new WriteRequestBuilder("recovery-reseed");
    const logicalId2 = newId();
    builder2.addNode({
      rowId: newRowId(), logicalId: logicalId2, kind: "Document",
      properties: { title: "Purgeable" }, sourceRef: "recovery-test-2",
    });
    engine.write(builder2.build());

    const builder3 = new WriteRequestBuilder("recovery-retire");
    builder3.retireNode(logicalId2, "recovery-test-2");
    engine.write(builder3.build());

    const restore = engine.admin.restoreLogicalId(logicalId2);
    assert(restore.logicalId === logicalId2, "restore logical_id mismatch");

    const purge = engine.admin.purgeLogicalId(logicalId2);
    assert(purge.logicalId === logicalId2, "purge logical_id mismatch");

    engine.close();
    return { name: "recovery", ok: true };
  } catch (error) {
    return handleScenarioError("recovery", error);
  }
}
