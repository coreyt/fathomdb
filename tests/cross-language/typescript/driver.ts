/**
 * Cross-language test driver for the TypeScript fathomdb SDK.
 *
 * Reads scenarios from scenarios.json, writes data to a database,
 * queries it back, and emits a normalized JSON manifest to stdout.
 */

import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  Engine,
  WriteRequestBuilder,
  type QueryRows,
} from "fathomdb";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SCENARIOS_PATH = resolve(__dirname, "..", "scenarios.json");

interface ScenarioDef {
  name: string;
  writes: WriteDef[];
  queries?: QueryDef[];
  admin?: AdminDef[];
}

interface WriteDef {
  label: string;
  nodes?: NodeDef[];
  node_retires?: NodeRetireDef[];
  edges?: EdgeDef[];
  chunks?: ChunkDef[];
  runs?: RunDef[];
  steps?: StepDef[];
  actions?: ActionDef[];
}

interface NodeDef {
  row_id: string;
  logical_id: string;
  kind: string;
  properties: unknown;
  source_ref?: string;
  upsert?: boolean;
  chunk_policy?: string;
}

interface NodeRetireDef {
  logical_id: string;
  source_ref?: string;
}

interface EdgeDef {
  row_id: string;
  logical_id: string;
  source_logical_id: string;
  target_logical_id: string;
  kind: string;
  properties: unknown;
  source_ref?: string;
  upsert?: boolean;
}

interface ChunkDef { id: string; node_logical_id: string; text_content: string; byte_start?: number; byte_end?: number; }
interface RunDef { id: string; kind: string; status: string; properties: unknown; source_ref?: string; upsert?: boolean; supersedes_id?: string; }
interface StepDef { id: string; run_id: string; kind: string; status: string; properties: unknown; source_ref?: string; upsert?: boolean; supersedes_id?: string; }
interface ActionDef { id: string; step_id: string; kind: string; status: string; properties: unknown; source_ref?: string; upsert?: boolean; supersedes_id?: string; }

type QueryDef = Record<string, unknown>;
type AdminDef = string | Record<string, unknown>;

function loadScenarios(): ScenarioDef[] {
  return JSON.parse(readFileSync(SCENARIOS_PATH, "utf-8")).scenarios;
}

function sortedJson(obj: unknown): string {
  return JSON.stringify(obj, (_key, value) => {
    if (value !== null && typeof value === "object" && !Array.isArray(value)) {
      return Object.keys(value).sort().reduce<Record<string, unknown>>((sorted, k) => {
        sorted[k] = (value as Record<string, unknown>)[k];
        return sorted;
      }, {});
    }
    return value;
  });
}

function normalizeProperties(props: unknown): unknown {
  return JSON.parse(sortedJson(props));
}

function buildWriteRequest(writeDef: WriteDef): Record<string, unknown> {
  const builder = new WriteRequestBuilder(writeDef.label);

  for (const n of writeDef.nodes ?? []) {
    builder.addNode({
      rowId: n.row_id,
      logicalId: n.logical_id,
      kind: n.kind,
      properties: n.properties,
      sourceRef: n.source_ref,
      upsert: n.upsert,
      chunkPolicy: (n.chunk_policy as "preserve" | "replace" | undefined),
      contentRef: n.content_ref,
    });
  }

  for (const nr of writeDef.node_retires ?? []) {
    builder.retireNode(nr.logical_id, nr.source_ref);
  }

  for (const e of writeDef.edges ?? []) {
    builder.addEdge({
      rowId: e.row_id,
      logicalId: e.logical_id,
      kind: e.kind,
      properties: e.properties,
      sourceRef: e.source_ref,
      upsert: e.upsert,
      source: e.source_logical_id,
      target: e.target_logical_id,
    });
  }

  for (const c of writeDef.chunks ?? []) {
    builder.addChunk({
      id: c.id,
      textContent: c.text_content,
      byteStart: c.byte_start,
      byteEnd: c.byte_end,
      contentHash: c.content_hash,
      node: c.node_logical_id,
    });
  }

  for (const r of writeDef.runs ?? []) {
    builder.addRun({
      id: r.id,
      kind: r.kind,
      status: r.status,
      properties: r.properties,
      sourceRef: r.source_ref,
      upsert: r.upsert,
      supersedesId: r.supersedes_id,
    });
  }

  for (const s of writeDef.steps ?? []) {
    builder.addStep({
      id: s.id,
      kind: s.kind,
      status: s.status,
      properties: s.properties,
      sourceRef: s.source_ref,
      upsert: s.upsert,
      supersedesId: s.supersedes_id,
      run: s.run_id,
    });
  }

  for (const a of writeDef.actions ?? []) {
    builder.addAction({
      id: a.id,
      kind: a.kind,
      status: a.status,
      properties: a.properties,
      sourceRef: a.source_ref,
      upsert: a.upsert,
      supersedesId: a.supersedes_id,
      step: a.step_id,
    });
  }

  return builder.build();
}

function executeQuery(engine: Engine, queryDef: QueryDef): Record<string, unknown> {
  const qtype = queryDef.type as string;

  if (qtype === "filter_logical_id") {
    const rows = engine.nodes(queryDef.kind as string)
      .filterLogicalIdEq(queryDef.logical_id as string)
      .execute();
    const nodes = rows.nodes
      .map(n => ({ logical_id: n.logicalId, kind: n.kind, properties: normalizeProperties(n.properties) }))
      .sort((a, b) => a.logical_id.localeCompare(b.logical_id));
    const result: Record<string, unknown> = { type: qtype, count: rows.nodes.length, nodes };
    if (queryDef.expect_runs) result.run_count = rows.runs.length;
    if (queryDef.expect_steps) result.step_count = rows.steps.length;
    if (queryDef.expect_actions) result.action_count = rows.actions.length;
    return result;
  }

  if (qtype === "text_search") {
    const rows = engine.nodes(queryDef.kind as string)
      .textSearch(queryDef.query as string, queryDef.limit as number)
      .execute();
    return { type: qtype, count: rows.nodes.length };
  }

  if (qtype === "filter_content_ref_not_null") {
    const rows = engine.nodes(queryDef.kind as string)
      .filterContentRefNotNull()
      .limit((queryDef.limit as number) ?? 100)
      .execute();
    const foundIds = rows.nodes.map(n => n.logicalId).sort();
    return { type: qtype, count: rows.nodes.length, found_ids: foundIds };
  }

  if (qtype === "traverse") {
    const rows = engine.nodes(queryDef.kind as string)
      .filterLogicalIdEq(queryDef.start_logical_id as string)
      .traverse({
        direction: queryDef.direction as "in" | "out",
        label: queryDef.label as string,
        maxDepth: queryDef.max_depth as number,
      })
      .limit(10)
      .execute();
    const foundIds = rows.nodes.map(n => n.logicalId).sort();
    return { type: qtype, found_ids: foundIds };
  }

  throw new Error(`unknown query type: ${qtype}`);
}

function executeAdmin(engine: Engine, adminDef: AdminDef): Record<string, unknown> {
  const def = typeof adminDef === "string" ? { type: adminDef } : adminDef;
  const atype = def.type as string;

  if (atype === "check_integrity") {
    const report = engine.admin.checkIntegrity();
    return {
      type: "check_integrity",
      physical_ok: report.physicalOk,
      foreign_keys_ok: report.foreignKeysOk,
      missing_fts_rows: report.missingFtsRows,
      duplicate_active_logical_ids: report.duplicateActiveLogicalIds,
    };
  }

  if (atype === "check_semantics") {
    const report = engine.admin.checkSemantics();
    return {
      type: "check_semantics",
      orphaned_chunks: report.orphanedChunks,
      dangling_edges: report.danglingEdges,
      broken_step_fk: report.brokenStepFk,
      broken_action_fk: report.brokenActionFk,
    };
  }

  if (atype === "trace_source") {
    const report = engine.admin.traceSource(def.source_ref as string);
    return {
      type: "trace_source",
      source_ref: def.source_ref as string,
      node_rows: report.nodeRows,
      edge_rows: report.edgeRows,
      action_rows: report.actionRows,
    };
  }

  throw new Error(`unknown admin type: ${atype}`);
}

function runDriver(dbPath: string, mode: string): Record<string, unknown> {
  const scenarios = loadScenarios();
  const engine = Engine.open(dbPath);

  if (mode === "write") {
    for (const scenario of scenarios) {
      for (const writeDef of scenario.writes) {
        engine.write(buildWriteRequest(writeDef));
      }
    }
  }

  const results: Record<string, unknown> = {};
  for (const scenario of scenarios) {
    const queries = (scenario.queries ?? []).map(q => executeQuery(engine, q));
    const admin = (scenario.admin ?? []).map(a => executeAdmin(engine, a));
    results[scenario.name] = { queries, admin };
  }

  engine.close();
  return { results };
}

// CLI entry point
const { values } = parseArgs({
  options: {
    db: { type: "string" },
    mode: { type: "string" },
  },
});

if (!values.db || !values.mode) {
  console.error("Usage: driver.ts --db <path> --mode <write|read>");
  process.exit(1);
}

const manifest = runDriver(values.db, values.mode);
console.log(sortedJson(manifest));
