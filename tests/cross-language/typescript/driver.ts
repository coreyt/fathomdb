/**
 * Cross-language test driver for the TypeScript fathomdb SDK.
 *
 * Reads scenarios from scenarios.json, writes data to a database,
 * queries it back, and emits a normalized JSON manifest to stdout.
 *
 * Design, scenario format, and instructions for adding new scenarios
 * are documented in tests/cross-language/README.md.
 */

import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  Engine,
  WriteRequestBuilder,
  type FtsPropertyPathMode,
  type FtsPropertyPathSpec,
  type SearchHit,
  type SearchRows,
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
  content_ref?: string;
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

interface ChunkDef { id: string; node_logical_id: string; text_content: string; byte_start?: number; byte_end?: number; content_hash?: string; }
interface RunDef { id: string; kind: string; status: string; properties: unknown; source_ref?: string; upsert?: boolean; supersedes_id?: string; }
interface StepDef { id: string; run_id: string; kind: string; status: string; properties: unknown; source_ref?: string; upsert?: boolean; supersedes_id?: string; }
interface ActionDef { id: string; step_id: string; kind: string; status: string; properties: unknown; source_ref?: string; upsert?: boolean; supersedes_id?: string; }

type QueryDef = Record<string, unknown>;
type AdminDef = string | Record<string, unknown>;

interface ScenariosFile {
  scenarios: ScenarioDef[];
  setup_admin?: AdminDef[];
}

function loadScenariosFile(): ScenariosFile {
  return JSON.parse(readFileSync(SCENARIOS_PATH, "utf-8"));
}

function loadScenarios(): ScenarioDef[] {
  return loadScenariosFile().scenarios;
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

const WRITTEN_AT_RECENT_WINDOW_SECONDS = 300;

function actualHit(hit: SearchHit, withAttribution: boolean): Record<string, unknown> {
  const nowSeconds = Math.floor(Date.now() / 1000);
  const writtenAtRecent =
    hit.writtenAt > 0 &&
    hit.writtenAt <= nowSeconds &&
    hit.writtenAt >= nowSeconds - WRITTEN_AT_RECENT_WINDOW_SECONDS;
  const entry: Record<string, unknown> = {
    logical_id: hit.node.logicalId,
    kind: hit.node.kind,
    source: hit.source,
    // Phase 10: matchMode is nullable on the wire — future vector hits
    // will carry `null`. Preserve nulls rather than coercing to a string
    // so cross-language parity covers both cases.
    match_mode: hit.matchMode ?? null,
    snippet_non_empty: Boolean(hit.snippet && hit.snippet.trim().length > 0),
    written_at_recent: writtenAtRecent,
    projection_row_id_present: hit.projectionRowId != null,
  };
  if (withAttribution) {
    entry.attribution_matched_paths = hit.attribution ? [...hit.attribution.matchedPaths] : [];
  }
  return entry;
}

function searchRowsToActual(rows: SearchRows, withAttribution: boolean): Record<string, unknown> {
  const rawProjectionIds = rows.hits.map(h => h.projectionRowId);
  const allPresent = rawProjectionIds.every(pid => pid != null);
  const presentIds = rawProjectionIds.filter((pid): pid is string => pid != null);
  const projectionRowIdsUnique =
    allPresent && new Set(presentIds).size === presentIds.length;
  return {
    hit_count: rows.hits.length,
    strict_hit_count: rows.strictHitCount,
    relaxed_hit_count: rows.relaxedHitCount,
    fallback_used: rows.fallbackUsed,
    was_degraded: rows.wasDegraded,
    projection_row_ids_unique: projectionRowIdsUnique,
    hits: rows.hits.map(h => actualHit(h, withAttribution)),
  };
}

function evaluateSearchExpectations(queryDef: QueryDef, actual: Record<string, unknown>): string[] {
  const failures: string[] = [];
  const hits = actual.hits as Array<Record<string, unknown>>;

  const arrayEquals = (a: unknown[], b: unknown[]): boolean =>
    a.length === b.length && a.every((v, i) => v === b[i]);

  if (Array.isArray(queryDef.expect_hit_logical_ids)) {
    const want = (queryDef.expect_hit_logical_ids as string[]).slice();
    const got = hits.map(h => h.logical_id as string);
    if (!arrayEquals(want, got)) {
      failures.push(`expect_hit_logical_ids: want ${JSON.stringify(want)}, got ${JSON.stringify(got)}`);
    }
  }
  if (Array.isArray(queryDef.expect_hit_sources)) {
    const want = (queryDef.expect_hit_sources as string[]).slice();
    const got = hits.map(h => h.source as string);
    if (!arrayEquals(want, got)) {
      failures.push(`expect_hit_sources: want ${JSON.stringify(want)}, got ${JSON.stringify(got)}`);
    }
  }
  if (Array.isArray(queryDef.expect_match_modes)) {
    const want = (queryDef.expect_match_modes as string[]).slice();
    const got = hits.map(h => h.match_mode as string);
    if (!arrayEquals(want, got)) {
      failures.push(`expect_match_modes: want ${JSON.stringify(want)}, got ${JSON.stringify(got)}`);
    }
  }
  if (queryDef.expect_snippets_non_empty) {
    if (!hits.every(h => h.snippet_non_empty)) {
      failures.push("expect_snippets_non_empty: some hit had empty snippet");
    }
  }
  if (queryDef.expect_written_at_seconds_recent) {
    if (!hits.every(h => h.written_at_recent)) {
      failures.push("expect_written_at_seconds_recent: some hit written_at out of window");
    }
  }
  if (queryDef.expect_projection_row_ids_unique) {
    if (!hits.every(h => h.projection_row_id_present)) {
      failures.push("expect_projection_row_ids_unique: some hit missing projection_row_id");
    } else if (actual.projection_row_ids_unique !== true) {
      failures.push("expect_projection_row_ids_unique: duplicate projection_row_ids across hits");
    }
  }
  if (typeof queryDef.expect_strict_hit_count === "number") {
    if (actual.strict_hit_count !== queryDef.expect_strict_hit_count) {
      failures.push(`expect_strict_hit_count: want ${queryDef.expect_strict_hit_count}, got ${actual.strict_hit_count}`);
    }
  }
  if (typeof queryDef.expect_strict_hit_count_min === "number") {
    if ((actual.strict_hit_count as number) < (queryDef.expect_strict_hit_count_min as number)) {
      failures.push(`expect_strict_hit_count_min: want >= ${queryDef.expect_strict_hit_count_min}, got ${actual.strict_hit_count}`);
    }
  }
  if (typeof queryDef.expect_relaxed_hit_count === "number") {
    if (actual.relaxed_hit_count !== queryDef.expect_relaxed_hit_count) {
      failures.push(`expect_relaxed_hit_count: want ${queryDef.expect_relaxed_hit_count}, got ${actual.relaxed_hit_count}`);
    }
  }
  if (typeof queryDef.expect_relaxed_hit_count_min === "number") {
    if ((actual.relaxed_hit_count as number) < (queryDef.expect_relaxed_hit_count_min as number)) {
      failures.push(`expect_relaxed_hit_count_min: want >= ${queryDef.expect_relaxed_hit_count_min}, got ${actual.relaxed_hit_count}`);
    }
  }
  if (typeof queryDef.expect_fallback_used === "boolean") {
    if (actual.fallback_used !== queryDef.expect_fallback_used) {
      failures.push(`expect_fallback_used: want ${queryDef.expect_fallback_used}, got ${actual.fallback_used}`);
    }
  }
  if (typeof queryDef.expect_was_degraded === "boolean") {
    if (actual.was_degraded !== queryDef.expect_was_degraded) {
      failures.push(`expect_was_degraded: want ${queryDef.expect_was_degraded}, got ${actual.was_degraded}`);
    }
  }
  if (typeof queryDef.expect_min_count === "number") {
    if ((actual.hit_count as number) < (queryDef.expect_min_count as number)) {
      failures.push(`expect_min_count: want >= ${queryDef.expect_min_count}, got ${actual.hit_count}`);
    }
  }
  if (Array.isArray(queryDef.expect_matched_paths)) {
    for (const item of queryDef.expect_matched_paths as Array<Record<string, unknown>>) {
      const idx = item.hit_index as number;
      const wantPaths = ((item.paths as string[]) ?? []).slice().sort();
      if (idx >= hits.length) {
        failures.push(`expect_matched_paths: hit_index ${idx} out of range`);
        continue;
      }
      const gotPaths = ((hits[idx].attribution_matched_paths as string[]) ?? []).slice().sort();
      if (!arrayEquals(wantPaths, gotPaths)) {
        failures.push(
          `expect_matched_paths[${idx}]: want ${JSON.stringify(wantPaths)}, got ${JSON.stringify(gotPaths)}`,
        );
      }
    }
  }
  return failures;
}

function executeTextSearch(engine: Engine, queryDef: QueryDef): Record<string, unknown> {
  const withAttribution = Boolean(queryDef.with_match_attribution);
  let builder = engine
    .nodes(queryDef.kind as string)
    .textSearch(queryDef.query as string, queryDef.limit as number);
  if (withAttribution) {
    builder = builder.withMatchAttribution();
  }

  const repeatRuns = Math.max(1, Number(queryDef.repeat_runs ?? 1));
  const runsActual: Record<string, unknown>[] = [];
  for (let i = 0; i < repeatRuns; i++) {
    runsActual.push(searchRowsToActual(builder.execute(), withAttribution));
  }
  const actual = runsActual[0];

  const failures: string[] = [];
  if (queryDef.expect_deterministic_across_runs) {
    const firstJson = sortedJson(runsActual[0]);
    for (let i = 1; i < runsActual.length; i++) {
      if (sortedJson(runsActual[i]) !== firstJson) {
        failures.push(`expect_deterministic_across_runs: run ${i + 1} differs from run 1`);
      }
    }
  }
  failures.push(...evaluateSearchExpectations(queryDef, actual));

  const result: Record<string, unknown> = {
    type: "text_search",
    name: queryDef.name ?? null,
    actual,
    pass: failures.length === 0,
    failures,
  };
  if (repeatRuns > 1) {
    result.repeat_runs = repeatRuns;
  }
  return result;
}

function executeFallbackSearch(engine: Engine, queryDef: QueryDef): Record<string, unknown> {
  const withAttribution = Boolean(queryDef.with_match_attribution);
  let builder = engine.fallbackSearch(
    queryDef.strict_query as string,
    (queryDef.relaxed_query as string | null) ?? null,
    Number(queryDef.limit ?? 10),
  );
  if (typeof queryDef.kind_filter === "string") {
    builder = builder.filterKindEq(queryDef.kind_filter);
  }
  if (withAttribution) {
    builder = builder.withMatchAttribution();
  }
  const actual = searchRowsToActual(builder.execute(), withAttribution);
  const failures = evaluateSearchExpectations(queryDef, actual);
  return {
    type: "fallback_search",
    name: queryDef.name ?? null,
    actual,
    pass: failures.length === 0,
    failures,
  };
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
    return executeTextSearch(engine, queryDef);
  }

  if (qtype === "fallback_search") {
    return executeFallbackSearch(engine, queryDef);
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

  if (atype === "register_fts_property_schema") {
    const record = engine.admin.registerFtsPropertySchema(
      def.kind as string, def.property_paths as string[], def.separator as string | undefined);
    return {
      type: "register_fts_property_schema",
      kind: record.kind,
      property_paths: record.propertyPaths,
      separator: record.separator,
    };
  }

  if (atype === "register_fts_property_schema_with_entries") {
    const rawEntries = (def.entries as Array<Record<string, unknown>>) ?? [];
    const entries: FtsPropertyPathSpec[] = rawEntries.map(e => ({
      path: String(e.path ?? ""),
      mode: (String(e.mode ?? "scalar") === "recursive" ? "recursive" : "scalar") as FtsPropertyPathMode,
    }));
    const record = engine.admin.registerFtsPropertySchemaWithEntries({
      kind: def.kind as string,
      entries,
      separator: (def.separator as string | undefined) ?? " ",
      excludePaths: (def.exclude_paths as string[] | undefined) ?? [],
    });
    return {
      type: "register_fts_property_schema_with_entries",
      kind: record.kind,
      entries: record.entries.map(e => ({ path: e.path, mode: e.mode })),
      separator: record.separator,
      exclude_paths: record.excludePaths,
    };
  }

  if (atype === "describe_fts_property_schema") {
    const record = engine.admin.describeFtsPropertySchema(def.kind as string);
    if (record === null) {
      return { type: "describe_fts_property_schema", kind: def.kind as string, found: false };
    }
    return {
      type: "describe_fts_property_schema",
      kind: record.kind,
      property_paths: record.propertyPaths,
      separator: record.separator,
      found: true,
    };
  }

  if (atype === "list_fts_property_schemas") {
    const schemas = engine.admin.listFtsPropertySchemas();
    return {
      type: "list_fts_property_schemas",
      count: schemas.length,
      kinds: schemas.map(s => s.kind).sort(),
    };
  }

  throw new Error(`unknown admin type: ${atype}`);
}

function runDriver(dbPath: string, mode: string): Record<string, unknown> {
  const raw = loadScenariosFile();
  const scenarios = raw.scenarios;
  const engine = Engine.open(dbPath);

  if (mode === "write") {
    // Run global setup_admin before any writes so schemas are in place.
    for (const adminDef of raw.setup_admin ?? []) {
      executeAdmin(engine, adminDef);
    }
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
