// Real-engine integration tests for the text-search surface.
//
// This file was converted from a mocked-binding test in Pack P7.6b. Every
// assertion that the mocked version made is preserved; the only thing that
// changed is that the SearchRows / SearchHit payload now comes from a real
// on-disk engine executing real FTS5 queries, not a vi.fn() that returns
// hand-rolled JSON.
//
// The file deliberately mirrors python/tests/test_text_search_surface.py so
// that the Python and TypeScript SDK suites prove the same cross-language
// behaviours.

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  FallbackSearchBuilder,
  TextSearchBuilder,
  type Engine,
  type SearchRows,
} from "../src/index.js";

import { openTempEngine, seedBudgetGoals, type TempEngine } from "./helpers/engine.js";

describe("text_search surface", () => {
  let ctx: TempEngine;
  let engine: Engine;

  beforeEach(() => {
    ctx = openTempEngine();
    engine = ctx.engine;
    seedBudgetGoals(engine);
  });

  afterEach(() => {
    ctx.cleanup();
  });

  it("text_search returns SearchRows with populated fields", () => {
    const rows: SearchRows = engine.query("Goal").textSearch("quarterly", 10).execute();
    expect(rows.hits.length).toBeGreaterThan(0);
    const hit = rows.hits[0];
    expect(typeof hit.score).toBe("number");
    // bm25 sign is flipped at the Rust layer so strong matches produce
    // positive scores — this is the assertion a mock could never catch.
    expect(hit.score).toBeGreaterThan(0);
    expect(["chunk", "property"]).toContain(hit.source);
    expect(hit.matchMode).toBe("strict");
    expect(hit.snippet).not.toBeNull();
    expect(hit.writtenAt).toBeGreaterThan(0);
    expect(hit.projectionRowId).not.toBeNull();
    expect(hit.attribution).toBeNull();
    expect(hit.node.kind).toBe("Goal");
    expect(hit.node.logicalId.startsWith("budget-")).toBe(true);
    expect(rows.strictHitCount).toBeGreaterThanOrEqual(1);
    expect(rows.relaxedHitCount).toBe(0);
  });

  it("text_search zero hits returns empty SearchRows", () => {
    const rows = engine.query("Goal").textSearch("zzznopeterm", 10).execute();
    expect(rows.hits.length).toBe(0);
    expect(rows.strictHitCount).toBe(0);
    expect(rows.fallbackUsed).toBe(false);
    expect(rows.wasDegraded).toBe(false);
  });

  it("text_search with filter_kind_eq chains into the request payload", () => {
    const rows = engine
      .query("Goal")
      .textSearch("quarterly", 10)
      .filterKindEq("Goal")
      .execute();
    expect(rows.hits.length).toBeGreaterThan(0);
    for (const hit of rows.hits) {
      expect(hit.node.kind).toBe("Goal");
    }
  });

  it("text_search with withMatchAttribution populates leaves", () => {
    const rows = engine
      .query("Goal")
      .textSearch("rollup", 10)
      .withMatchAttribution()
      .execute();
    expect(rows.hits.length).toBeGreaterThan(0);
    // With attribution requested, every hit must carry an attribution
    // payload (the matchedPaths list may be empty for chunk-backed hits,
    // but the payload itself proves the FFI flag was honored and the
    // HitAttribution object round-tripped at the napi boundary).
    const attributed = rows.hits.filter((h) => h.attribution !== null);
    expect(attributed.length).toBeGreaterThan(0);
    expect(Array.isArray(attributed[0].attribution?.matchedPaths)).toBe(true);

    // Baseline: without withMatchAttribution(), attribution is always null.
    const plain = engine.query("Goal").textSearch("rollup", 10).execute();
    expect(plain.hits.length).toBeGreaterThan(0);
    for (const hit of plain.hits) {
      expect(hit.attribution).toBeNull();
    }
  });

  it("text_search strict miss triggers relaxed fallback", () => {
    const rows = engine
      .query("Goal")
      .textSearch("budget quarterly zzznopeterm", 10)
      .execute();
    expect(rows.fallbackUsed).toBe(true);
    expect(rows.hits.length).toBeGreaterThanOrEqual(1);
    expect(rows.hits.some((h) => h.matchMode === "relaxed")).toBe(true);
    expect(rows.strictHitCount).toBe(0);
    expect(rows.relaxedHitCount).toBeGreaterThanOrEqual(1);
  });

  it("fallback_search two shape forwards relaxed query and fires relaxed branch", () => {
    const builder = engine.fallbackSearch("zzznope1 zzznope2", "budget OR nothing", 10);
    expect(builder).toBeInstanceOf(FallbackSearchBuilder);
    const rows = builder.filterKindEq("Goal").execute();
    expect(rows.fallbackUsed).toBe(true);
    expect(rows.hits.length).toBeGreaterThanOrEqual(1);
  });

  it("fallback_search strict only matches text_search result shape", () => {
    const textRows = engine.query("Goal").textSearch("budget", 10).filterKindEq("Goal").execute();
    const fallbackRows = engine.fallbackSearch("budget", null, 10).filterKindEq("Goal").execute();
    expect(fallbackRows.hits.length).toBe(textRows.hits.length);
    expect(fallbackRows.strictHitCount).toBe(textRows.strictHitCount);
    expect(fallbackRows.relaxedHitCount).toBe(textRows.relaxedHitCount);
    expect(fallbackRows.fallbackUsed).toBe(textRows.fallbackUsed);
    expect(fallbackRows.wasDegraded).toBe(textRows.wasDegraded);
    // Hit-level comparison: same query should return identical
    // projectionRowId values across the two surfaces (stability).
    const left = textRows.hits.map((h) => h.projectionRowId);
    const right = fallbackRows.hits.map((h) => h.projectionRowId);
    expect(right).toEqual(left);
  });

  it("node query execute still returns QueryRows with nodes shape", () => {
    const rows = engine.query("Goal").execute();
    // QueryRows has a `nodes` array, SearchRows has `hits`.
    expect(Array.isArray((rows as unknown as { nodes: unknown[] }).nodes)).toBe(true);
    expect((rows as unknown as { hits?: unknown }).hits).toBeUndefined();
    expect(rows.wasDegraded).toBe(false);
  });

  it("text_search empty query returns empty SearchRows without throwing", () => {
    expect(() => {
      const rows = engine.query("Goal").textSearch("", 10).execute();
      expect(rows.hits).toEqual([]);
      expect(rows.strictHitCount).toBe(0);
    }).not.toThrow();
  });

  it("textSearch returns a TextSearchBuilder distinct from Query", () => {
    const builder = engine.query("Goal").textSearch("quarterly", 10);
    expect(builder).toBeInstanceOf(TextSearchBuilder);
  });

  // ── Wire-format assertions that mocked tests could not catch ──────────
  //
  // These checks were added in Pack P7.6b. A mocked test could happily
  // return a SearchHit object with camelCase keys already baked in; only
  // a real engine routes data through the snake→camel converter at the
  // napi boundary, so only a real-engine test can prove that every field
  // on SearchHit survives the conversion.

  it("wire format: every SearchHit field is camelCase and populated", () => {
    const rows = engine.query("Goal").textSearch("quarterly", 10).execute();
    expect(rows.hits.length).toBeGreaterThan(0);
    const hit = rows.hits[0];
    // Required SearchHit fields
    expect(hit).toHaveProperty("node");
    expect(hit).toHaveProperty("score");
    expect(hit).toHaveProperty("modality");
    expect(hit).toHaveProperty("source");
    expect(hit).toHaveProperty("matchMode");
    expect(hit).toHaveProperty("snippet");
    expect(hit).toHaveProperty("writtenAt");
    expect(hit).toHaveProperty("projectionRowId");
    expect(hit).toHaveProperty("vectorDistance");
    expect(hit).toHaveProperty("attribution");
    // Required SearchNode fields (snake→camel conversion)
    expect(hit.node).toHaveProperty("rowId");
    expect(hit.node).toHaveProperty("logicalId");
    expect(hit.node).toHaveProperty("kind");
    expect(hit.node).toHaveProperty("properties");
    // Neither the snake_case forms nor stray underscores should leak.
    expect(Object.keys(hit)).not.toContain("match_mode");
    expect(Object.keys(hit)).not.toContain("written_at");
    expect(Object.keys(hit)).not.toContain("projection_row_id");
    expect(Object.keys(hit.node)).not.toContain("row_id");
    expect(Object.keys(hit.node)).not.toContain("logical_id");
    // SearchRows top-level fields
    expect(rows).toHaveProperty("hits");
    expect(rows).toHaveProperty("wasDegraded");
    expect(rows).toHaveProperty("fallbackUsed");
    expect(rows).toHaveProperty("strictHitCount");
    expect(rows).toHaveProperty("relaxedHitCount");
    expect(rows).toHaveProperty("vectorHitCount");
    expect(Object.keys(rows)).not.toContain("was_degraded");
    expect(Object.keys(rows)).not.toContain("fallback_used");
    expect(Object.keys(rows)).not.toContain("strict_hit_count");
    expect(Object.keys(rows)).not.toContain("relaxed_hit_count");
    expect(Object.keys(rows)).not.toContain("vector_hit_count");
    // Stringified negative assertions: the legacy snake_case field names
    // must not survive anywhere in the serialized SearchRows payload, not
    // just at the top-level key set.
    const rowsJson = JSON.stringify(rows);
    expect(rowsJson).not.toContain("strict_hit_count");
    expect(rowsJson).not.toContain("relaxed_hit_count");
    expect(rowsJson).not.toContain("vector_hit_count");
    expect(rowsJson).not.toContain("was_degraded");
    expect(rowsJson).not.toContain("fallback_used");
    expect(rowsJson).not.toContain("vector_distance");
    expect(rowsJson).not.toContain("match_mode");
    const nodeJson = JSON.stringify(hit.node);
    expect(nodeJson).not.toContain("content_ref");
    expect(nodeJson).not.toContain("last_accessed_at");
    expect(nodeJson).not.toContain("row_id");
    expect(nodeJson).not.toContain("logical_id");
  });

  it("wire format: writtenAt is Unix seconds near the write time", () => {
    const nowSeconds = Math.floor(Date.now() / 1000);
    const rows = engine.query("Goal").textSearch("quarterly", 10).execute();
    expect(rows.hits.length).toBeGreaterThan(0);
    for (const hit of rows.hits) {
      // Written by the beforeEach helper within this test run.
      // Allow some slack for slow CI.
      expect(hit.writtenAt).toBeGreaterThan(nowSeconds - 60);
      expect(hit.writtenAt).toBeLessThanOrEqual(nowSeconds + 5);
    }
  });

  it("Phase 10: every text hit is tagged modality=text with null vectorDistance", () => {
    const rows = engine.query("Goal").textSearch("quarterly", 10).execute();
    expect(rows.hits.length).toBeGreaterThan(0);
    for (const hit of rows.hits) {
      expect(hit.modality).toBe("text");
      expect(hit.vectorDistance).toBeNull();
      expect(hit.matchMode).not.toBeNull();
    }
    expect(rows.vectorHitCount).toBe(0);
  });

  it("wire format: projectionRowId is stable across identical queries", () => {
    const a = engine.query("Goal").textSearch("quarterly", 10).execute();
    const b = engine.query("Goal").textSearch("quarterly", 10).execute();
    expect(a.hits.length).toBe(b.hits.length);
    expect(a.hits.length).toBeGreaterThan(0);
    for (let i = 0; i < a.hits.length; i += 1) {
      expect(a.hits[i].projectionRowId).toBe(b.hits[i].projectionRowId);
      expect(typeof a.hits[i].projectionRowId).toBe("string");
      expect(a.hits[i].projectionRowId?.length ?? 0).toBeGreaterThan(0);
    }
  });
});
