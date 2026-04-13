import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  Engine,
  FallbackSearchBuilder,
  TextSearchBuilder,
  type SearchRows,
} from "../src/index.js";

type WireSearchNode = {
  row_id: string;
  logical_id: string;
  kind: string;
  properties: string;
  content_ref: string | null;
  last_accessed_at: number | null;
};

type WireSearchHit = {
  node: WireSearchNode;
  score: number;
  source: "chunk" | "property" | "vector";
  match_mode: "strict" | "relaxed";
  snippet: string | null;
  written_at: number;
  projection_row_id: string | null;
  attribution: { matched_paths: string[] } | null;
};

type WireSearchRows = {
  hits: WireSearchHit[];
  was_degraded: boolean;
  fallback_used: boolean;
  strict_hit_count: number;
  relaxed_hit_count: number;
};

function makeHit(overrides: Partial<WireSearchHit> = {}): WireSearchHit {
  return {
    node: {
      row_id: "row-budget-alpha",
      logical_id: "budget-alpha",
      kind: "Goal",
      properties: '{"name":"budget alpha goal"}',
      content_ref: null,
      last_accessed_at: null,
    },
    score: 1.5,
    source: "chunk",
    match_mode: "strict",
    snippet: "<b>quarterly</b> budget review",
    written_at: 1_700_000_000,
    projection_row_id: "chunk-budget-alpha",
    attribution: null,
    ...overrides,
  };
}

function makeRows(overrides: Partial<WireSearchRows> = {}): WireSearchRows {
  return {
    hits: [makeHit()],
    was_degraded: false,
    fallback_used: false,
    strict_hit_count: 1,
    relaxed_hit_count: 0,
    ...overrides,
  };
}

function installMock(
  responses: Record<string, WireSearchRows | ((request: unknown) => WireSearchRows)>,
  captured: { lastRequest: unknown | null } = { lastRequest: null },
): { lastRequest: unknown | null; executeSearch: ReturnType<typeof vi.fn> } {
  const executeSearch = vi.fn((requestJson: string) => {
    const parsed = JSON.parse(requestJson) as { mode: string };
    captured.lastRequest = parsed;
    const match = responses[parsed.mode];
    if (!match) {
      throw new Error(`unexpected mode ${parsed.mode}`);
    }
    const rows = typeof match === "function" ? match(parsed) : match;
    return JSON.stringify(rows);
  });

  const binding = {
    EngineCore: {
      open: vi.fn(() => ({
        close: vi.fn(),
        executeSearch,
        executeAst: vi.fn(() =>
          JSON.stringify({
            nodes: [
              {
                row_id: "r1",
                logical_id: "n1",
                kind: "Goal",
                properties: "{}",
                content_ref: null,
                last_accessed_at: null,
              },
            ],
            runs: [],
            steps: [],
            actions: [],
            was_degraded: false,
          }),
        ),
      })),
    },
    newId: vi.fn(() => "id-1"),
    newRowId: vi.fn(() => "row-1"),
  };
  globalThis.__FATHOMDB_NATIVE_MOCK__ = binding as never;
  Engine.setBindingForTests(binding as never);
  return { ...captured, executeSearch };
}

describe("text_search surface", () => {
  beforeEach(() => {
    // Each test installs its own mock via installMock.
  });

  it("text_search returns SearchRows with populated fields", () => {
    installMock({ text_search: makeRows() });
    const engine = Engine.open("/tmp/test.db");
    const rows: SearchRows = engine.query("Goal").textSearch("quarterly", 10).execute();
    expect(rows.hits.length).toBeGreaterThan(0);
    const hit = rows.hits[0];
    expect(typeof hit.score).toBe("number");
    expect(["chunk", "property"]).toContain(hit.source);
    expect(hit.matchMode).toBe("strict");
    expect(hit.snippet).not.toBeNull();
    expect(hit.writtenAt).toBeGreaterThan(0);
    expect(hit.projectionRowId).not.toBeNull();
    expect(hit.attribution).toBeNull();
    expect(rows.strictHitCount).toBe(1);
    expect(rows.relaxedHitCount).toBe(0);
  });

  it("text_search zero hits returns empty SearchRows", () => {
    installMock({
      text_search: makeRows({ hits: [], strict_hit_count: 0, relaxed_hit_count: 0 }),
    });
    const engine = Engine.open("/tmp/test.db");
    const rows = engine.query("Goal").textSearch("nothingmatches", 10).execute();
    expect(rows.hits.length).toBe(0);
    expect(rows.strictHitCount).toBe(0);
    expect(rows.fallbackUsed).toBe(false);
    expect(rows.wasDegraded).toBe(false);
  });

  it("text_search with filter_kind_eq chains into the request payload", () => {
    const captured: { lastRequest: unknown | null } = { lastRequest: null };
    installMock({ text_search: makeRows() }, captured);
    const engine = Engine.open("/tmp/test.db");
    const rows = engine
      .query("Goal")
      .textSearch("quarterly", 10)
      .filterKindEq("Goal")
      .execute();
    for (const hit of rows.hits) {
      expect(hit.node.kind).toBe("Goal");
    }
    const request = captured.lastRequest as {
      mode: string;
      root_kind: string;
      filters: Array<{ type: string; kind?: string }>;
    };
    expect(request.mode).toBe("text_search");
    expect(request.root_kind).toBe("Goal");
    expect(request.filters).toEqual([{ type: "filter_kind_eq", kind: "Goal" }]);
  });

  it("text_search with withMatchAttribution populates leaves", () => {
    const captured: { lastRequest: unknown | null } = { lastRequest: null };
    installMock(
      {
        text_search: makeRows({
          hits: [
            makeHit({
              node: {
                row_id: "row-note-1",
                logical_id: "note-attrib",
                kind: "Note",
                properties: '{"payload":{"body":"shipping quarterly docs"}}',
                content_ref: null,
                last_accessed_at: null,
              },
              source: "property",
              attribution: { matched_paths: ["$.payload.body"] },
            }),
          ],
        }),
      },
      captured,
    );
    const engine = Engine.open("/tmp/test.db");
    const rows = engine
      .query("Note")
      .textSearch("shipping", 10)
      .withMatchAttribution()
      .execute();
    expect(rows.hits.length).toBeGreaterThan(0);
    const hit = rows.hits[0];
    expect(hit.attribution).not.toBeNull();
    expect(hit.attribution?.matchedPaths.length).toBeGreaterThanOrEqual(1);
    expect(hit.attribution?.matchedPaths[0]).toBe("$.payload.body");
    const request = captured.lastRequest as { attribution_requested: boolean };
    expect(request.attribution_requested).toBe(true);
  });

  it("text_search strict miss triggers relaxed fallback", () => {
    installMock({
      text_search: makeRows({
        hits: [
          makeHit({ match_mode: "relaxed", score: 0.4 }),
          makeHit({ match_mode: "relaxed", score: 0.3, projection_row_id: "chunk-bravo" }),
        ],
        fallback_used: true,
        strict_hit_count: 0,
        relaxed_hit_count: 2,
      }),
    });
    const engine = Engine.open("/tmp/test.db");
    const rows = engine
      .query("Goal")
      .textSearch("budget quarterly zzznope", 10)
      .execute();
    expect(rows.fallbackUsed).toBe(true);
    expect(rows.hits.some((h) => h.matchMode === "relaxed")).toBe(true);
    expect(rows.strictHitCount).toBe(0);
    expect(rows.relaxedHitCount).toBe(2);
  });

  it("fallback_search two shape forwards relaxed query and fires relaxed branch", () => {
    const captured: { lastRequest: unknown | null } = { lastRequest: null };
    installMock(
      {
        fallback_search: makeRows({
          hits: [makeHit({ match_mode: "relaxed" })],
          fallback_used: true,
          strict_hit_count: 0,
          relaxed_hit_count: 1,
        }),
      },
      captured,
    );
    const engine = Engine.open("/tmp/test.db");
    const builder = engine.fallbackSearch("nonexistent", "budget", 10);
    expect(builder).toBeInstanceOf(FallbackSearchBuilder);
    const rows = builder.execute();
    expect(rows.fallbackUsed).toBe(true);
    expect(rows.hits.length).toBeGreaterThanOrEqual(1);
    const request = captured.lastRequest as {
      mode: string;
      strict_query: string;
      relaxed_query: string | null;
    };
    expect(request.mode).toBe("fallback_search");
    expect(request.strict_query).toBe("nonexistent");
    expect(request.relaxed_query).toBe("budget");
  });

  it("fallback_search strict only matches text_search result shape", () => {
    const strictRows = makeRows();
    installMock({
      text_search: strictRows,
      fallback_search: strictRows,
    });
    const engine = Engine.open("/tmp/test.db");
    const textRows = engine.query("Goal").textSearch("budget", 10).execute();
    const fallbackRows = engine.fallbackSearch("budget", null, 10).execute();
    expect(fallbackRows).toEqual(textRows);
  });

  it("node query execute still returns QueryRows with nodes shape", () => {
    installMock({ text_search: makeRows() });
    const engine = Engine.open("/tmp/test.db");
    const rows = engine.query("Goal").execute();
    // QueryRows has a `nodes` array, SearchRows has `hits`.
    expect(Array.isArray((rows as unknown as { nodes: unknown[] }).nodes)).toBe(true);
    expect((rows as unknown as { hits?: unknown }).hits).toBeUndefined();
    expect(rows.wasDegraded).toBe(false);
  });

  it("text_search empty query returns empty SearchRows without throwing", () => {
    installMock({
      text_search: makeRows({ hits: [], strict_hit_count: 0, relaxed_hit_count: 0 }),
    });
    const engine = Engine.open("/tmp/test.db");
    expect(() => {
      const rows = engine.query("Goal").textSearch("", 10).execute();
      expect(rows.hits).toEqual([]);
      expect(rows.strictHitCount).toBe(0);
    }).not.toThrow();
  });

  it("textSearch returns a TextSearchBuilder distinct from Query", () => {
    installMock({ text_search: makeRows() });
    const engine = Engine.open("/tmp/test.db");
    const builder = engine.query("Goal").textSearch("quarterly", 10);
    expect(builder).toBeInstanceOf(TextSearchBuilder);
  });
});
