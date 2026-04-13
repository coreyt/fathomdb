import {
  Engine,
  WriteRequestBuilder,
  newRowId,
  type SearchRows,
} from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, handleScenarioError, tempDbPath } from "../skip.js";

// Dedicated kinds so these scenarios do not collide with adaptive_search
// or other harness runs that share the tempdir-backed engine.
const GOAL_KIND = "UnifiedSearchGoal";
const KI_KIND = "UnifiedSearchKnowledgeItem";
const TASK_KIND = "UnifiedSearchTask";

function seedSearchGoals(engine: Engine): void {
  const builder = new WriteRequestBuilder("unified-search-seed-goals");
  const budget = builder.addNode({
    rowId: newRowId(),
    logicalId: "unified-search-budget",
    kind: GOAL_KIND,
    properties: { title: "Budget meeting" },
    sourceRef: "unified-search-seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  const quarterly = builder.addNode({
    rowId: newRowId(),
    logicalId: "unified-search-quarterly",
    kind: GOAL_KIND,
    properties: { title: "Quarterly planning" },
    sourceRef: "unified-search-seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  const roadmap = builder.addNode({
    rowId: newRowId(),
    logicalId: "unified-search-roadmap",
    kind: GOAL_KIND,
    properties: { title: "Engineering roadmap" },
    sourceRef: "unified-search-seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  builder.addChunk({
    id: "unified-search-budget-chunk",
    node: budget,
    textContent: "budget meeting notes for finance review",
  });
  builder.addChunk({
    id: "unified-search-quarterly-chunk",
    node: quarterly,
    textContent: "quarterly planning docs and action items",
  });
  builder.addChunk({
    id: "unified-search-roadmap-chunk",
    node: roadmap,
    textContent: "engineering roadmap deliverables",
  });
  engine.write(builder.build());
}

function registerRecursivePayloadSchema(engine: Engine): void {
  engine.admin.registerFtsPropertySchemaWithEntries({
    kind: KI_KIND,
    entries: [{ path: "$.payload", mode: "recursive" }],
    separator: " ",
    excludePaths: [],
  });
}

function seedRecursiveKnowledgeItem(engine: Engine): void {
  const builder = new WriteRequestBuilder("unified-search-seed-ki");
  builder.addNode({
    rowId: newRowId(),
    logicalId: "unified-search-ki-alpha",
    kind: KI_KIND,
    properties: {
      payload: {
        title: "quarterly planning",
        notes: "budget approval",
      },
    },
    sourceRef: "unified-search-seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  engine.write(builder.build());
}

function strictHitPopulatesRows(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("unified-search-strict-hit"));
    try {
      seedSearchGoals(engine);
      const rows: SearchRows = engine
        .query(GOAL_KIND)
        .search("budget meeting", 10)
        .execute();
      assert(rows.hits.length === 1, `expected 1 hit, got ${rows.hits.length}`);
      const hit = rows.hits[0];
      assert(
        hit.node.logicalId === "unified-search-budget",
        `unexpected hit logicalId=${hit.node.logicalId}`,
      );
      assert(hit.matchMode === "strict", `expected matchMode=strict, got ${hit.matchMode}`);
      assert(rows.fallbackUsed === false, "fallback must not fire on strict hit");
      assert(rows.wasDegraded === false, "search must not degrade");
      assert(rows.strictHitCount === 1, `strictHitCount=${rows.strictHitCount}`);
      assert(rows.relaxedHitCount === 0, `relaxedHitCount=${rows.relaxedHitCount}`);
      assert(
        rows.vectorHitCount === 0,
        `v1 vector branch must stay empty; got ${rows.vectorHitCount}`,
      );
    } finally {
      engine.close();
    }
    return { name: "unified_search_strict_hit_populates_rows", ok: true };
  } catch (error) {
    return handleScenarioError("unified_search_strict_hit_populates_rows", error);
  }
}

function strictMissRelaxedRecovery(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("unified-search-relaxed-recovery"));
    try {
      seedSearchGoals(engine);
      const rows = engine
        .query(GOAL_KIND)
        .search("budget nonexistentxyzzy", 10)
        .execute();
      assert(rows.fallbackUsed === true, "strict miss must trigger derived relaxed branch");
      assert(rows.strictHitCount === 0, `strictHitCount=${rows.strictHitCount}`);
      assert(rows.hits.length > 0, "relaxed branch must recover at least one hit");
      assert(
        rows.relaxedHitCount > 0,
        `relaxedHitCount=${rows.relaxedHitCount}`,
      );
      assert(
        rows.hits.some((h) => h.matchMode === "relaxed"),
        "at least one hit must carry matchMode=relaxed",
      );
    } finally {
      engine.close();
    }
    return { name: "unified_search_strict_miss_relaxed_recovery", ok: true };
  } catch (error) {
    return handleScenarioError("unified_search_strict_miss_relaxed_recovery", error);
  }
}

function filterKindEqFuses(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("unified-search-filter-kind"));
    try {
      seedSearchGoals(engine);
      engine.admin.registerFtsPropertySchema(TASK_KIND, ["$.title"]);
      const builder = new WriteRequestBuilder("unified-search-seed-task");
      const task = builder.addNode({
        rowId: newRowId(),
        logicalId: "unified-search-task-budget",
        kind: TASK_KIND,
        properties: { title: "budget review" },
        sourceRef: "unified-search-seed",
        upsert: false,
        chunkPolicy: "preserve",
      });
      builder.addChunk({
        id: "unified-search-task-budget-chunk",
        node: task,
        textContent: "budget alignment task notes",
      });
      engine.write(builder.build());

      const filtered = engine
        .query(GOAL_KIND)
        .search("budget", 10)
        .filterKindEq(GOAL_KIND)
        .execute();
      assert(filtered.hits.length > 0, "strict branch must return at least one Goal");
      assert(
        filtered.hits.every((h) => h.node.kind === GOAL_KIND),
        `filterKindEq must exclude Task rows; got kinds=${JSON.stringify(filtered.hits.map((h) => h.node.kind))}`,
      );
      assert(
        filtered.hits.every((h) => h.node.logicalId !== "unified-search-task-budget"),
        "Task row leaked past filterKindEq",
      );
    } finally {
      engine.close();
    }
    return { name: "unified_search_filter_kind_eq_fuses", ok: true };
  } catch (error) {
    return handleScenarioError("unified_search_filter_kind_eq_fuses", error);
  }
}

function withMatchAttribution(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("unified-search-attribution"));
    try {
      registerRecursivePayloadSchema(engine);
      seedRecursiveKnowledgeItem(engine);

      const rows = engine
        .query(KI_KIND)
        .search("quarterly", 10)
        .withMatchAttribution()
        .execute();
      assert(rows.hits.length === 1, `expected 1 hit, got ${rows.hits.length}`);
      const hit = rows.hits[0];
      assert(
        hit.source === "property",
        `recursive property hit must report source=property; got ${hit.source}`,
      );
      assert(
        hit.matchMode === "strict",
        `expected matchMode=strict, got ${hit.matchMode}`,
      );
      assert(hit.attribution !== null, "attribution must be populated");
      const paths = hit.attribution!.matchedPaths;
      assert(
        paths.includes("$.payload.title"),
        `expected $.payload.title in matchedPaths; got ${JSON.stringify(paths)}`,
      );
    } finally {
      engine.close();
    }
    return { name: "unified_search_with_match_attribution", ok: true };
  } catch (error) {
    return handleScenarioError("unified_search_with_match_attribution", error);
  }
}

export function unifiedSearchScenarios(): HarnessResult[] {
  return [
    strictHitPopulatesRows(),
    strictMissRelaxedRecovery(),
    filterKindEqFuses(),
    withMatchAttribution(),
  ];
}
