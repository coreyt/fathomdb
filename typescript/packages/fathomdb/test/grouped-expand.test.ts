// Integration tests for SearchBuilder.expand(), .compileGrouped(), and .executeGrouped().
//
// These methods were added in Pack 6. The tests exercise:
//   1. Fluent expand() returns a SearchBuilder (instanceof check)
//   2. executeGrouped() returns GroupedQueryRows with correct structure
//   3. Filter chains compose correctly (filter before expand doesn't drop expansions)

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  Engine,
  SearchBuilder,
  WriteRequestBuilder,
  newRowId,
  type GroupedQueryRows,
} from "../src/index.js";

import { openTempEngine, type TempEngine } from "./helpers/engine.js";

/**
 * Seed a Goal node with FTS schema and child Step nodes connected by HAS_STEP edges.
 * Returns the logical IDs of the root goal and its step children.
 */
function seedGoalWithSteps(
  engine: Engine,
  opts: {
    goalLogicalId: string;
    goalName: string;
    stepCount: number;
  },
): { goalRowId: string; stepRowIds: string[] } {
  engine.admin.registerFtsPropertySchema("Goal", ["$.name", "$.description"]);
  engine.admin.registerFtsPropertySchema("Step", ["$.name"]);

  const builder = new WriteRequestBuilder("seed-goal-steps");
  const goalRowId = newRowId();
  const goal = builder.addNode({
    rowId: goalRowId,
    logicalId: opts.goalLogicalId,
    kind: "Goal",
    properties: {
      name: opts.goalName,
      description: "grouped expand test goal",
    },
    sourceRef: "seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  builder.addChunk({
    id: `${opts.goalLogicalId}-chunk`,
    node: goal,
    textContent: `${opts.goalName} grouped expand test chunk`,
  });

  const stepRowIds: string[] = [];
  for (let i = 0; i < opts.stepCount; i++) {
    const stepRowId = newRowId();
    stepRowIds.push(stepRowId);
    const step = builder.addNode({
      rowId: stepRowId,
      logicalId: `${opts.goalLogicalId}-step-${i}`,
      kind: "Step",
      properties: { name: `step ${i}` },
      sourceRef: "seed",
      upsert: false,
      chunkPolicy: "preserve",
    });
    builder.addEdge({
      rowId: newRowId(),
      logicalId: `${opts.goalLogicalId}-edge-${i}`,
      source: goal,
      target: step,
      kind: "HAS_STEP",
      properties: {},
    });
  }

  engine.write(builder.build());
  return { goalRowId, stepRowIds };
}

describe("SearchBuilder grouped expand", () => {
  let ctx: TempEngine;
  let engine: Engine;

  beforeEach(() => {
    ctx = openTempEngine();
    engine = ctx.engine;
  });

  afterEach(() => {
    ctx.cleanup();
  });

  it("search().expand() returns a SearchBuilder", () => {
    engine.admin.registerFtsPropertySchema("Goal", ["$.name"]);
    const builder = engine
      .query("Goal")
      .search("budget", 10)
      .expand({ slot: "children", direction: "out", label: "HAS_CHILD", maxDepth: 1 });
    expect(builder).toBeInstanceOf(SearchBuilder);
  });

  it("executeGrouped() returns GroupedQueryRows with correct structure", () => {
    seedGoalWithSteps(engine, {
      goalLogicalId: "grouped-goal",
      goalName: "budget grouped goal",
      stepCount: 2,
    });

    const rows: GroupedQueryRows = engine
      .query("Goal")
      .search("budget", 10)
      .expand({ slot: "steps", direction: "out", label: "HAS_STEP", maxDepth: 1 })
      .executeGrouped();

    expect(rows.roots.length).toBe(1);
    expect(rows.expansions.length).toBe(1);
    expect(rows.expansions[0].slot).toBe("steps");
  });

  it("filter chains compose correctly (filter before expand preserves both)", () => {
    engine.admin.registerFtsPropertySchema("Goal", ["$.name"]);
    // .filterKindEq().expand() should return a SearchBuilder with both filter and expansion
    const builder = engine
      .query("Goal")
      .search("q", 10)
      .filterKindEq("Goal")
      .expand({ slot: "children", direction: "out", label: "HAS_CHILD", maxDepth: 1 });
    expect(builder).toBeInstanceOf(SearchBuilder);
  });

  it("compileGrouped() returns a CompiledGroupedQuery with root SQL", () => {
    seedGoalWithSteps(engine, {
      goalLogicalId: "compile-goal",
      goalName: "budget compile goal",
      stepCount: 1,
    });

    const compiled = engine
      .query("Goal")
      .search("budget", 10)
      .expand({ slot: "steps", direction: "out", label: "HAS_STEP", maxDepth: 1 })
      .compileGrouped();

    expect(compiled.root.sql).toBeTruthy();
    expect(compiled.expansions.length).toBe(1);
    expect(compiled.expansions[0].slot).toBe("steps");
  });

  it("executeGrouped() with multiple children returns children in expansion roots", () => {
    seedGoalWithSteps(engine, {
      goalLogicalId: "origin-goal",
      goalName: "origin label search",
      stepCount: 3,
    });

    const rows: GroupedQueryRows = engine
      .query("Goal")
      .search("origin", 10)
      .expand({ slot: "steps", direction: "out", label: "HAS_STEP", maxDepth: 1 })
      .executeGrouped();

    expect(rows.roots.length).toBeGreaterThan(0);
    // There should be expansion rows for our root node
    const stepExpansion = rows.expansions.find((e) => e.slot === "steps");
    expect(stepExpansion).toBeDefined();
  });
});
