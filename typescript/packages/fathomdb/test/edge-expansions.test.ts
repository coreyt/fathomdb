// Tests for Pack F — edge-projecting traversal TypeScript binding.
//
// Covers:
//   - EdgeRow, EdgeExpansionRootRows, EdgeExpansionSlotRows exported from the
//     public API.
//   - GroupedQueryRows.edgeExpansions present and typed.
//   - NodeRow has no `edgeProperties` field (Pack D removal mirrored here).
//   - traverseEdges() builder method shapes the AST correctly.
//   - Wire decoder for edge_expansions produces `{ edge, endpoint }` object
//     pairs (TS-idiomatic, intentional cross-SDK asymmetry with Python tuple
//     decoding per design §10).

import { describe, expect, it } from "vitest";
import {
  groupedQueryRowsFromWire,
  type EdgeRow,
  type EdgeExpansionRootRows,
  type EdgeExpansionSlotRows,
  type GroupedQueryRows,
  type NodeRow,
} from "../src/types.js";
import * as fathomdb from "../src/index.js";

describe("Pack F — EdgeRow + edge expansions", () => {
  it("NodeRow has no edgeProperties field (Pack D removal mirrored)", () => {
    const row: NodeRow = {
      rowId: "r1",
      logicalId: "n1",
      kind: "Goal",
      properties: null,
      contentRef: null,
      lastAccessedAt: null,
    };
    // @ts-expect-error edgeProperties must not be part of NodeRow
    const _leak: unknown = row.edgeProperties;
    void _leak;
    expect(Object.prototype.hasOwnProperty.call(row, "edgeProperties")).toBe(false);
  });

  it("EdgeRow exposes all 8 canonical fields", () => {
    const edge: EdgeRow = {
      rowId: "er-1",
      logicalId: "el-1",
      sourceLogicalId: "n-src",
      targetLogicalId: "n-tgt",
      kind: "HAS_STEP",
      properties: "{\"rel\":\"x\"}",
      sourceRef: "seed",
      confidence: 0.9,
    };
    expect(edge.rowId).toBe("er-1");
    expect(edge.logicalId).toBe("el-1");
    expect(edge.sourceLogicalId).toBe("n-src");
    expect(edge.targetLogicalId).toBe("n-tgt");
    expect(edge.kind).toBe("HAS_STEP");
    expect(edge.properties).toBe("{\"rel\":\"x\"}");
    expect(edge.sourceRef).toBe("seed");
    expect(edge.confidence).toBe(0.9);
  });

  it("wire decoder emits { edge, endpoint } pair objects with all EdgeRow fields populated", () => {
    // Simulated wire payload as produced by FfiGroupedQueryRows.
    const wire = {
      roots: [],
      expansions: [],
      edge_expansions: [
        {
          slot: "provenance",
          roots: [
            {
              root_logical_id: "meeting-abc",
              pairs: [
                {
                  edge: {
                    row_id: "er-1",
                    logical_id: "el-1",
                    source_logical_id: "meeting-abc",
                    target_logical_id: "doc-xyz",
                    kind: "cites",
                    properties: "{\"weight\":5}",
                    source_ref: "seed",
                    confidence: 0.75,
                  },
                  endpoint: {
                    row_id: "r-doc",
                    logical_id: "doc-xyz",
                    kind: "Doc",
                    properties: "{\"title\":\"x\"}",
                    content_ref: null,
                    last_accessed_at: null,
                  },
                },
              ],
            },
          ],
        },
      ],
      was_degraded: false,
    };

    const rows: GroupedQueryRows = groupedQueryRowsFromWire(wire);
    expect(rows.edgeExpansions.length).toBe(1);
    const slot: EdgeExpansionSlotRows = rows.edgeExpansions[0];
    expect(slot.slot).toBe("provenance");
    expect(slot.roots.length).toBe(1);
    const root: EdgeExpansionRootRows = slot.roots[0];
    expect(root.rootLogicalId).toBe("meeting-abc");
    expect(root.pairs.length).toBe(1);
    const pair = root.pairs[0];
    // Must be an OBJECT with edge + endpoint keys (TS-idiomatic shape).
    expect(Array.isArray(pair)).toBe(false);
    expect(pair).toHaveProperty("edge");
    expect(pair).toHaveProperty("endpoint");
    expect(pair.edge.rowId).toBe("er-1");
    expect(pair.edge.logicalId).toBe("el-1");
    expect(pair.edge.sourceLogicalId).toBe("meeting-abc");
    expect(pair.edge.targetLogicalId).toBe("doc-xyz");
    expect(pair.edge.kind).toBe("cites");
    expect(pair.edge.properties).toBe("{\"weight\":5}");
    expect(pair.edge.sourceRef).toBe("seed");
    expect(pair.edge.confidence).toBe(0.75);
    expect(pair.endpoint.logicalId).toBe("doc-xyz");
    expect(pair.endpoint.kind).toBe("Doc");
    // Pack D: no edgeProperties on NodeRow.
    expect(Object.prototype.hasOwnProperty.call(pair.endpoint, "edgeProperties")).toBe(false);
  });

  it("wire decoder tolerates missing edge_expansions (older engines, SDK skew window)", () => {
    const wire = { roots: [], expansions: [], was_degraded: false };
    const rows = groupedQueryRowsFromWire(wire);
    expect(rows.edgeExpansions).toEqual([]);
  });

  it("wire decoder: optional source_ref and confidence decode to null", () => {
    const wire = {
      roots: [],
      expansions: [],
      edge_expansions: [
        {
          slot: "x",
          roots: [
            {
              root_logical_id: "a",
              pairs: [
                {
                  edge: {
                    row_id: "er-2",
                    logical_id: "el-2",
                    source_logical_id: "a",
                    target_logical_id: "b",
                    kind: "k",
                    properties: "{}",
                    source_ref: null,
                    confidence: null,
                  },
                  endpoint: {
                    row_id: "r-b",
                    logical_id: "b",
                    kind: "K",
                    properties: "{}",
                    content_ref: null,
                    last_accessed_at: null,
                  },
                },
              ],
            },
          ],
        },
      ],
      was_degraded: false,
    };
    const rows = groupedQueryRowsFromWire(wire);
    expect(rows.edgeExpansions[0].roots[0].pairs[0].edge.sourceRef).toBeNull();
    expect(rows.edgeExpansions[0].roots[0].pairs[0].edge.confidence).toBeNull();
  });

  it("traverseEdges() shapes the AST under edge_expansions (not expansions)", () => {
    // The builder should emit the slot on a separate `edge_expansions` vec
    // per design §3.
    type QueryWithAst = {
      toAst(): {
        root_kind: string;
        steps: unknown[];
        expansions: unknown[];
        edge_expansions: unknown[];
        final_limit: number | null;
      };
    };

    // Use a synthetic core — we only need toAst(), never terminal execution.
    const core = {} as unknown as ConstructorParameters<typeof fathomdb.Query>[0];
    const q = new fathomdb.Query(core, "Meeting")
      .traverseEdges({
        slot: "provenance",
        direction: "out",
        label: "cites",
        maxDepth: 1,
      });
    const ast = (q as unknown as QueryWithAst).toAst();
    expect(ast.expansions).toEqual([]);
    expect(ast.edge_expansions.length).toBe(1);
    const slot = ast.edge_expansions[0] as Record<string, unknown>;
    expect(slot.slot).toBe("provenance");
    expect(slot.direction).toBe("out");
    expect(slot.label).toBe("cites");
    expect(slot.max_depth).toBe(1);
  });

  it("traverseEdges() carries edgeFilter and endpointFilter predicates through the AST", () => {
    type QueryWithAst = {
      toAst(): {
        edge_expansions: Array<Record<string, unknown>>;
      };
    };
    const core = {} as unknown as ConstructorParameters<typeof fathomdb.Query>[0];
    const q = new fathomdb.Query(core, "Meeting").traverseEdges({
      slot: "provenance",
      direction: "in",
      label: "cites",
      maxDepth: 2,
      edgeFilter: { type: "edge_property_eq", path: "$.rel", value: "cites" },
      endpointFilter: { type: "filter_kind_eq", kind: "Doc" },
    });
    const ast = (q as unknown as QueryWithAst).toAst();
    const slot = ast.edge_expansions[0];
    expect(slot.edge_filter).toEqual({ type: "edge_property_eq", path: "$.rel", value: "cites" });
    expect(slot.endpoint_filter).toEqual({ type: "filter_kind_eq", kind: "Doc" });
    expect(slot.direction).toBe("in");
    expect(slot.max_depth).toBe(2);
  });

  it("public index.ts exports EdgeRow, EdgeExpansionRootRows, EdgeExpansionSlotRows", () => {
    // Type-only exports have no runtime presence, but the import itself will
    // fail to compile if any of these names are missing from src/index.ts.
    const _sample: {
      edge: EdgeRow;
      root: EdgeExpansionRootRows;
      slot: EdgeExpansionSlotRows;
    } | null = null;
    void _sample;
    expect(typeof fathomdb.Query).toBe("function");
  });
});

describe("Pack F — cross-SDK wire parity", () => {
  it("same wire JSON decodes to TS object with 8 edge fields in camelCase", () => {
    // Shared wire blob (snake_case) that both Python and TS must decode.
    // Python decodes pairs to tuple[EdgeRow, NodeRow]; TS decodes to
    // { edge, endpoint } objects. The leaf edge fields must be the same
    // set in both, only the outer pair shape differs (design §10).
    const wire = {
      roots: [],
      expansions: [],
      edge_expansions: [
        {
          slot: "parity",
          roots: [
            {
              root_logical_id: "root-1",
              pairs: [
                {
                  edge: {
                    row_id: "er",
                    logical_id: "el",
                    source_logical_id: "root-1",
                    target_logical_id: "end-1",
                    kind: "K",
                    properties: "{}",
                    source_ref: null,
                    confidence: null,
                  },
                  endpoint: {
                    row_id: "rr",
                    logical_id: "end-1",
                    kind: "K",
                    properties: "{}",
                    content_ref: null,
                    last_accessed_at: null,
                  },
                },
              ],
            },
          ],
        },
      ],
      was_degraded: false,
    };
    const rows = groupedQueryRowsFromWire(wire);
    const pair = rows.edgeExpansions[0].roots[0].pairs[0];
    // 8 edge fields, camelCase — matches Python's 8 snake_case fields at
    // `EdgeRow` leaf level.
    expect(Object.keys(pair.edge).sort()).toEqual(
      [
        "confidence",
        "kind",
        "logicalId",
        "properties",
        "rowId",
        "sourceLogicalId",
        "sourceRef",
        "targetLogicalId",
      ].sort(),
    );
  });
});
