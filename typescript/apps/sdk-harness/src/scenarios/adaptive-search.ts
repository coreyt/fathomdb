import {
  Engine,
  WriteRequestBuilder,
  newRowId,
  type SearchRows,
} from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, handleScenarioError, tempDbPath } from "../skip.js";

// Dedicated kinds so these scenarios do not collide with other
// harness runs that also write to a tempdir-backed engine.
const GOAL_KIND = "AdaptiveGoal";
const KI_KIND = "AdaptiveKnowledgeItem";

function seedAdaptiveGoals(engine: Engine): void {
  const builder = new WriteRequestBuilder("adaptive-seed-goals");
  const budget = builder.addNode({
    rowId: newRowId(),
    logicalId: "adaptive-goal-budget",
    kind: GOAL_KIND,
    properties: { title: "Budget meeting" },
    sourceRef: "adaptive-seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  const quarterly = builder.addNode({
    rowId: newRowId(),
    logicalId: "adaptive-goal-quarterly",
    kind: GOAL_KIND,
    properties: { title: "Quarterly planning" },
    sourceRef: "adaptive-seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  const roadmap = builder.addNode({
    rowId: newRowId(),
    logicalId: "adaptive-goal-roadmap",
    kind: GOAL_KIND,
    properties: { title: "Engineering roadmap" },
    sourceRef: "adaptive-seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  builder.addChunk({
    id: "adaptive-goal-budget-chunk",
    node: budget,
    textContent: "budget meeting notes for finance review",
  });
  builder.addChunk({
    id: "adaptive-goal-quarterly-chunk",
    node: quarterly,
    textContent: "quarterly planning docs and action items",
  });
  builder.addChunk({
    id: "adaptive-goal-roadmap-chunk",
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
  const builder = new WriteRequestBuilder("adaptive-seed-ki");
  builder.addNode({
    rowId: newRowId(),
    logicalId: "adaptive-ki-alpha",
    kind: KI_KIND,
    properties: {
      payload: {
        title: "quarterly planning",
        notes: "budget approval",
      },
    },
    sourceRef: "adaptive-seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  engine.write(builder.build());
}

function strictHitOnly(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("adaptive-strict-hit-only"));
    try {
      seedAdaptiveGoals(engine);
      const rows: SearchRows = engine
        .query(GOAL_KIND)
        .textSearch("budget meeting", 10)
        .execute();
      assert(rows.hits.length === 1, `expected 1 hit, got ${rows.hits.length}`);
      const hit = rows.hits[0];
      assert(
        hit.node.logicalId === "adaptive-goal-budget",
        `unexpected hit logicalId=${hit.node.logicalId}`,
      );
      assert(hit.matchMode === "strict", `expected matchMode=strict, got ${hit.matchMode}`);
      assert(rows.fallbackUsed === false, "fallback must not fire on strict hit");
      assert(rows.wasDegraded === false, "search must not degrade");
      assert(rows.strictHitCount === 1, `strictHitCount=${rows.strictHitCount}`);
      assert(rows.relaxedHitCount === 0, `relaxedHitCount=${rows.relaxedHitCount}`);
    } finally {
      engine.close();
    }
    return { name: "adaptive_search_strict_hit_only", ok: true };
  } catch (error) {
    return handleScenarioError("adaptive_search_strict_hit_only", error);
  }
}

function strictMissRelaxedRecovery(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("adaptive-relaxed-recovery"));
    try {
      seedAdaptiveGoals(engine);
      const rows = engine
        .query(GOAL_KIND)
        .textSearch("budget nonexistentxyzzy", 10)
        .execute();
      assert(rows.fallbackUsed === true, "strict miss must trigger relaxed fallback");
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
    return { name: "adaptive_search_strict_miss_relaxed_recovery", ok: true };
  } catch (error) {
    return handleScenarioError("adaptive_search_strict_miss_relaxed_recovery", error);
  }
}

function mixedChunkAndProperty(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("adaptive-mixed-chunk-property"));
    try {
      engine.admin.registerFtsPropertySchema(GOAL_KIND, ["$.title"]);
      const builder = new WriteRequestBuilder("adaptive-dual-match");
      const dual = builder.addNode({
        rowId: newRowId(),
        logicalId: "adaptive-dual",
        kind: GOAL_KIND,
        properties: { title: "dualmatchneedle target" },
        sourceRef: "adaptive-seed",
        upsert: false,
        chunkPolicy: "preserve",
      });
      builder.addChunk({
        id: "adaptive-dual-chunk",
        node: dual,
        textContent: "dualmatchneedle target appears in chunk body",
      });
      engine.write(builder.build());

      const rows = engine
        .query(GOAL_KIND)
        .textSearch("dualmatchneedle", 10)
        .execute();
      assert(
        rows.hits.length === 1,
        `dedup must collapse chunk+property to one hit; got ${rows.hits.length}`,
      );
      const hit = rows.hits[0];
      assert(
        hit.node.logicalId === "adaptive-dual",
        `unexpected hit logicalId=${hit.node.logicalId}`,
      );
      assert(
        hit.source === "chunk",
        `chunk must win the source tiebreak; got source=${hit.source}`,
      );
    } finally {
      engine.close();
    }
    return { name: "adaptive_search_mixed_chunk_and_property", ok: true };
  } catch (error) {
    return handleScenarioError("adaptive_search_mixed_chunk_and_property", error);
  }
}

function recursiveNestedPayload(): HarnessResult {
  try {
    const engine = Engine.open(tempDbPath("adaptive-recursive-payload"));
    try {
      registerRecursivePayloadSchema(engine);
      seedRecursiveKnowledgeItem(engine);

      const rows = engine
        .query(KI_KIND)
        .textSearch("quarterly", 10)
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

      const multi = engine
        .query(KI_KIND)
        .textSearch("quarterly budget", 10)
        .withMatchAttribution()
        .execute();
      assert(
        multi.hits.length === 1,
        `expected 1 hit on multi-term, got ${multi.hits.length}`,
      );
      const multiHit = multi.hits[0];
      assert(multiHit.attribution !== null, "multi-term attribution must be populated");
      const multiPaths = multiHit.attribution!.matchedPaths;
      assert(
        multiPaths.includes("$.payload.title"),
        `expected $.payload.title; got ${JSON.stringify(multiPaths)}`,
      );
      assert(
        multiPaths.includes("$.payload.notes"),
        `expected $.payload.notes; got ${JSON.stringify(multiPaths)}`,
      );
    } finally {
      engine.close();
    }
    return { name: "adaptive_search_recursive_nested_payload", ok: true };
  } catch (error) {
    return handleScenarioError("adaptive_search_recursive_nested_payload", error);
  }
}

function recursiveRebuildRestore(): HarnessResult {
  try {
    const dbPath = tempDbPath("adaptive-recursive-reopen");
    let pathsBefore: string[] = [];
    let logicalIdBefore = "";
    let sourceBefore = "";

    {
      const engine = Engine.open(dbPath);
      try {
        registerRecursivePayloadSchema(engine);
        seedRecursiveKnowledgeItem(engine);
        const rowsBefore = engine
          .query(KI_KIND)
          .textSearch("quarterly", 10)
          .withMatchAttribution()
          .execute();
        assert(
          rowsBefore.hits.length === 1,
          `pre-reopen expected 1 hit, got ${rowsBefore.hits.length}`,
        );
        const hit = rowsBefore.hits[0];
        assert(hit.attribution !== null, "pre-reopen attribution must be populated");
        pathsBefore = [...hit.attribution!.matchedPaths];
        logicalIdBefore = hit.node.logicalId;
        sourceBefore = hit.source;
      } finally {
        engine.close();
      }
    }

    {
      const engine = Engine.open(dbPath);
      try {
        const rowsAfter = engine
          .query(KI_KIND)
          .textSearch("quarterly", 10)
          .withMatchAttribution()
          .execute();
        assert(
          rowsAfter.hits.length === 1,
          `post-reopen expected 1 hit, got ${rowsAfter.hits.length}`,
        );
        const hit = rowsAfter.hits[0];
        assert(
          hit.node.logicalId === logicalIdBefore,
          `logicalId mismatch across reopen: before=${logicalIdBefore}, after=${hit.node.logicalId}`,
        );
        assert(
          hit.source === sourceBefore,
          `source mismatch across reopen: before=${sourceBefore}, after=${hit.source}`,
        );
        assert(
          hit.attribution !== null,
          "post-reopen attribution must be populated",
        );
        const pathsAfter = hit.attribution!.matchedPaths;
        assert(
          pathsAfter.length === pathsBefore.length &&
            pathsAfter.every((p, i) => p === pathsBefore[i]),
          `matchedPaths mismatch across reopen: before=${JSON.stringify(pathsBefore)}, after=${JSON.stringify(pathsAfter)}`,
        );
      } finally {
        engine.close();
      }
    }
    return { name: "adaptive_search_recursive_rebuild_restore", ok: true };
  } catch (error) {
    return handleScenarioError("adaptive_search_recursive_rebuild_restore", error);
  }
}

export function adaptiveSearchScenarios(): HarnessResult[] {
  return [
    strictHitOnly(),
    strictMissRelaxedRecovery(),
    mixedChunkAndProperty(),
    recursiveNestedPayload(),
    recursiveRebuildRestore(),
  ];
}
