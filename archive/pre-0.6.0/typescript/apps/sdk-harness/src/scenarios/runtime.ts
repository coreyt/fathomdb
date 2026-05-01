import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, handleScenarioError, tempDbPath } from "../skip.js";

export function runtimeScenario(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("runtime"));

    const sourceRef = "runtime-test";
    const anchorLogicalId = newId();
    const runId = newId();
    const stepId = newId();
    const actionId = newId();

    const builder = new WriteRequestBuilder("runtime-ingest");
    builder.addNode({
      rowId: newRowId(), logicalId: anchorLogicalId, kind: "Document",
      properties: { title: "Report" }, sourceRef,
    });
    const run = builder.addRun({
      id: runId, kind: "ingest", status: "running",
      properties: { startedAt: Date.now() }, sourceRef,
    });
    const step = builder.addStep({
      id: stepId, run, kind: "parse", status: "completed",
      properties: { duration_ms: 42 }, sourceRef,
    });
    builder.addAction({
      id: actionId, step, kind: "extract-text", status: "completed",
      properties: { chars: 1500 }, sourceRef,
    });
    engine.write(builder.build());

    const trace = engine.admin.traceSource(sourceRef);
    assert(trace.nodeRows === 1, `expected 1 traced node, got ${trace.nodeRows}`);
    assert(trace.actionRows === 1, `expected 1 traced action, got ${trace.actionRows}`);
    assert(trace.nodeLogicalIds.includes(anchorLogicalId), "trace missing anchor node");
    assert(trace.actionIds.includes(actionId), "trace missing action id");

    const nodeRows = engine.nodes("Document").filterLogicalIdEq(anchorLogicalId).execute();
    assert(nodeRows.nodes.length === 1, `expected 1 node, got ${nodeRows.nodes.length}`);

    const semantics = engine.admin.checkSemantics();
    assert(semantics.brokenStepFk === 0, `broken_step_fk=${semantics.brokenStepFk}`);
    assert(semantics.brokenActionFk === 0, `broken_action_fk=${semantics.brokenActionFk}`);

    engine.close();
    return { name: "runtime", ok: true };
  } catch (error) {
    return handleScenarioError("runtime", error);
  }
}
