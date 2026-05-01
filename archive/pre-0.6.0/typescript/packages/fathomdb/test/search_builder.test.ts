// Real-engine integration tests for the unified Phase 12/13a `search()`
// surface exposed by the TypeScript SDK. Mirrors the Rust roundtrip tests
// in `crates/fathomdb/tests/python_search_ffi.rs` (`search_*` block) so
// the TS SDK proves the same wire behaviour as the Python / Rust paths.
//
// Every test runs against a real on-disk engine; no binding-level mocks.

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  BuilderValidationError,
  Engine,
  SearchBuilder,
  WriteRequestBuilder,
  newRowId,
  type SearchRows,
} from "../src/index.js";

import { openTempEngine, seedBudgetGoals, type TempEngine } from "./helpers/engine.js";

function seedBudgetTask(engine: Engine): void {
  engine.admin.registerFtsPropertySchema("Task", ["$.name", "$.description"]);
  const builder = new WriteRequestBuilder("seed-budget-task");
  const task = builder.addNode({
    rowId: newRowId(),
    logicalId: "budget-task",
    kind: "Task",
    properties: {
      name: "budget task",
      description: "reconcile quarterly budget figures",
    },
    sourceRef: "seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  builder.addChunk({
    id: "budget-task-chunk",
    node: task,
    textContent: "task budget reconciliation notes",
  });
  engine.write(builder.build());
}

function seedRecursiveNote(engine: Engine, logicalId: string, payloadJson: object): void {
  engine.admin.registerFtsPropertySchemaWithEntries({
    kind: "Note",
    entries: [{ path: "$.payload", mode: "recursive" }],
  });
  const builder = new WriteRequestBuilder("seed-note");
  builder.addNode({
    rowId: newRowId(),
    logicalId,
    kind: "Note",
    properties: payloadJson as Record<string, unknown>,
    sourceRef: "seed",
    upsert: false,
    chunkPolicy: "preserve",
  });
  engine.write(builder.build());
}

describe("SearchBuilder (unified search surface)", () => {
  let ctx: TempEngine;
  let engine: Engine;

  beforeEach(() => {
    ctx = openTempEngine();
    engine = ctx.engine;
  });

  afterEach(() => {
    ctx.cleanup();
  });

  it("search() returns a SearchBuilder distinct from Query", () => {
    seedBudgetGoals(engine);
    const builder = engine.query("Goal").search("budget", 10);
    expect(builder).toBeInstanceOf(SearchBuilder);
  });

  it("basic search populates SearchRows with positive score and strict match mode", () => {
    seedBudgetGoals(engine);
    const rows: SearchRows = engine.query("Goal").search("budget", 10).execute();

    expect(rows.hits.length).toBeGreaterThan(0);
    expect(rows.strictHitCount).toBe(rows.hits.length);
    expect(rows.relaxedHitCount).toBe(0);
    expect(rows.vectorHitCount).toBe(0);
    expect(rows.fallbackUsed).toBe(false);
    expect(rows.wasDegraded).toBe(false);

    const hit = rows.hits[0];
    expect(hit.score).toBeGreaterThan(0);
    expect(hit.matchMode).toBe("strict");
    expect(hit.node.kind).toBe("Goal");
    expect(hit.attribution).toBeNull();
  });

  it("search with filter_kind_eq is fused (control vs filtered)", () => {
    seedBudgetGoals(engine);
    seedBudgetTask(engine);

    // Control: kind-agnostic root so both Goal and Task rows come back.
    const control = engine.query("").search("budget", 10).execute();
    expect(control.hits.some((h) => h.node.kind === "Task")).toBe(true);

    const filtered = engine
      .query("")
      .search("budget", 10)
      .filterKindEq("Goal")
      .execute();
    expect(filtered.hits.length).toBeGreaterThan(0);
    for (const hit of filtered.hits) {
      expect(hit.node.kind).toBe("Goal");
    }
    expect(filtered.hits.length).toBeLessThan(control.hits.length);
  });

  it("search with filter_json_text_eq post-filter narrows to one logical id", () => {
    seedBudgetGoals(engine);
    const rows = engine
      .query("Goal")
      .search("budget", 10)
      .filterJsonTextEq("$.name", "budget alpha goal")
      .execute();
    expect(rows.hits.length).toBeGreaterThan(0);
    for (const hit of rows.hits) {
      expect(hit.node.logicalId).toBe("budget-alpha");
    }
  });

  it("search with withMatchAttribution on recursive Note schema populates $.payload.body", () => {
    seedRecursiveNote(engine, "note-search-attrib", {
      payload: { body: "shipping quarterly docs" },
    });

    const rows = engine
      .query("Note")
      .search("shipping", 10)
      .withMatchAttribution()
      .execute();
    expect(rows.hits.length).toBeGreaterThan(0);
    const hit = rows.hits[0];
    expect(hit.attribution).not.toBeNull();
    expect(hit.attribution?.matchedPaths).toEqual(["$.payload.body"]);
  });

  it("empty-query search returns empty SearchRows without throwing", () => {
    seedBudgetGoals(engine);
    const rows = engine.query("Goal").search("", 10).execute();
    expect(rows.hits).toEqual([]);
    expect(rows.strictHitCount).toBe(0);
    expect(rows.relaxedHitCount).toBe(0);
    expect(rows.vectorHitCount).toBe(0);
    expect(rows.fallbackUsed).toBe(false);
  });

  it("filterJsonFusedTextEq throws BuilderValidationError without registered schema", () => {
    expect(() =>
      engine.query("Note").search("x", 5).filterJsonFusedTextEq("$.title", "hello"),
    ).toThrow(BuilderValidationError);
  });

  it("filterJsonFusedTextEq rejects path not in registered schema", () => {
    engine.admin.registerFtsPropertySchema("Note", ["$.title"]);
    expect(() =>
      engine
        .query("Note")
        .search("x", 5)
        .filterJsonFusedTextEq("$.not_indexed", "hello"),
    ).toThrow(BuilderValidationError);
  });

  it("filterJsonFusedTextEq succeeds with a matching registered schema", () => {
    engine.admin.registerFtsPropertySchema("Note", ["$.title"]);
    const builder = engine
      .query("Note")
      .search("x", 5)
      .filterJsonFusedTextEq("$.title", "hello");
    expect(builder).toBeInstanceOf(SearchBuilder);
  });

  it("filterJsonFusedTimestampGt validates on text_search path", () => {
    engine.admin.registerFtsPropertySchema("Note", ["$.written_at"]);
    const builder = engine
      .query("Note")
      .textSearch("x", 5)
      .filterJsonFusedTimestampGt("$.written_at", 1_700_000_000);
    expect(builder).toBeDefined();
  });

  it("post-filter filterJsonTextEq still works without schema (regression)", () => {
    const builder = engine
      .query("Note")
      .search("x", 5)
      .filterJsonTextEq("$.status", "active");
    expect(builder).toBeInstanceOf(SearchBuilder);
  });
});
