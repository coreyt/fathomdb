import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, handleScenarioError, tempDbPath } from "../skip.js";

export function graphScenario(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("graph"));

    const builder = new WriteRequestBuilder("graph-ingest");
    const parent = builder.addNode({
      rowId: newRowId(), logicalId: newId(), kind: "Project",
      properties: { name: "Alpha" }, sourceRef: "graph-test",
    });
    const child = builder.addNode({
      rowId: newRowId(), logicalId: newId(), kind: "Task",
      properties: { name: "Design" }, sourceRef: "graph-test",
    });
    builder.addEdge({
      rowId: newRowId(), logicalId: newId(), kind: "owns",
      properties: {}, source: parent, target: child, sourceRef: "graph-test",
    });
    engine.write(builder.build());

    const projects = engine.nodes("Project").execute();
    assert(projects.nodes.length === 1, `expected 1 project, got ${projects.nodes.length}`);

    const tasks = engine.nodes("Task").execute();
    assert(tasks.nodes.length === 1, `expected 1 task, got ${tasks.nodes.length}`);

    const expanded = engine.nodes("Project")
      .expand({ slot: "children", direction: "out", label: "owns", maxDepth: 1 })
      .executeGrouped();
    assert(expanded.roots.length >= 1, "expected ≥1 root");

    const filtered = engine.nodes("Project").filterKindEq("Project").execute();
    assert(filtered.nodes.length >= 1, "kind filter should match");

    const byId = engine.nodes("Project")
      .filterLogicalIdEq(projects.nodes[0].logicalId)
      .execute();
    assert(byId.nodes.length === 1, "logical id filter should match exactly 1");

    engine.close();
    return { name: "graph", ok: true };
  } catch (error) {
    return handleScenarioError("graph", error);
  }
}
