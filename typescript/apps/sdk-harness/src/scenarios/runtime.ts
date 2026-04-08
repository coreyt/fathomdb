import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, handleScenarioError, tempDbPath } from "../skip.js";

export function runtimeScenario(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("runtime"));

    const builder = new WriteRequestBuilder("runtime-ingest");
    builder.addNode({
      rowId: newRowId(), logicalId: newId(), kind: "Document",
      properties: { title: "Report" }, sourceRef: "runtime-test",
    });
    const run = builder.addRun({
      id: newId(), kind: "ingest", status: "running",
      properties: { startedAt: Date.now() }, sourceRef: "runtime-test",
    });
    const step = builder.addStep({
      id: newId(), run, kind: "parse", status: "completed",
      properties: { duration_ms: 42 }, sourceRef: "runtime-test",
    });
    builder.addAction({
      id: newId(), step, kind: "extract-text", status: "completed",
      properties: { chars: 1500 }, sourceRef: "runtime-test",
    });
    engine.write(builder.build());

    const rows = engine.nodes("Document").execute();
    assert(rows.nodes.length >= 1, "expected ≥1 node");
    assert(rows.runs.length >= 1, "expected ≥1 run");
    assert(rows.steps.length >= 1, "expected ≥1 step");
    assert(rows.actions.length >= 1, "expected ≥1 action");
    assert(rows.runs[0].kind === "ingest", "run kind mismatch");
    assert(rows.steps[0].kind === "parse", "step kind mismatch");
    assert(rows.actions[0].kind === "extract-text", "action kind mismatch");

    const runId = rows.runs[0].id;
    const builder2 = new WriteRequestBuilder("runtime-update");
    builder2.addRun({
      id: newId(), kind: "ingest", status: "completed",
      properties: { finishedAt: Date.now() }, sourceRef: "runtime-test",
      upsert: true, supersedesId: runId,
    });
    engine.write(builder2.build());

    engine.close();
    return { name: "runtime", ok: true };
  } catch (error) {
    return handleScenarioError("runtime", error);
  }
}
