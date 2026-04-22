// Real-engine integration tests for the TypeScript SDK surface.
//
// Converted from a mocked-binding test suite in Pack P7.6b. The file keeps
// every invariant the pre-P7.6b mocked version asserted; the conversion
// adds strength by routing every response through the real napi binding
// and a real on-disk SQLite engine, catching wire-format regressions the
// mocks could not.
//
// Structure:
//   1. Pure-TypeScript tests (WriteRequestBuilder, runWithFeedback,
//      PreserializedJson, toAst) do not need an engine — they test logic
//      that lives entirely in the TypeScript layer.
//   2. Engine-backed tests open a fresh tempdir-backed engine via
//      `openTempEngine` and close it in `afterEach`.
//   3. Three tests still use a scoped mock via `Engine.setBindingForTests`
//      because they exercise error-path code that can only be triggered
//      when the native binding throws a specific error string. That is
//      the only supported way to cover those branches; the mock is
//      installed inside the test body, not in a suite-wide `beforeEach`.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  BuilderValidationError,
  Engine,
  FathomError,
  PreserializedJson,
  Query,
  SqliteError,
  WriteRequestBuilder,
  newId,
  newRowId,
  type ResponseCycleEvent,
} from "../src/index.js";
import { runWithFeedback } from "../src/feedback.js";

import { openTempEngine, seedSingleDoc, type TempEngine } from "./helpers/engine.js";

// ── Pure-TypeScript tests (no engine, no native binding) ─────────────────

describe("WriteRequestBuilder (pure TS)", () => {
  it("builds immutable queries with python-parity AST shape", () => {
    // toAst lives entirely in TypeScript; we can exercise it without an
    // engine by constructing a Query directly against a null core.
    // But the public API requires going through Engine.nodes, so open a
    // real engine here.
    const ctx = openTempEngine();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    try {
      const base = ctx.engine.nodes("Meeting");
      const query = base
        .vectorSearch("budget", 5)
        .filterJsonTextEq("$.status", "active")
        .expand({ slot: "neighbors", direction: "out", label: "depends_on", maxDepth: 2 })
        .limit(10);

      expect(base).toBeInstanceOf(Query);
      expect(base.toAst()).toEqual({
        root_kind: "Meeting",
        steps: [],
        expansions: [],
        edge_expansions: [],
        final_limit: null,
      });
      expect(query.toAst()).toEqual({
        root_kind: "Meeting",
        steps: [
          { type: "semantic_search", text: "budget", limit: 5 },
          { type: "filter_json_text_eq", path: "$.status", value: "active" },
        ],
        expansions: [
          { slot: "neighbors", direction: "out", label: "depends_on", max_depth: 2 },
        ],
        edge_expansions: [],
        final_limit: 10,
      });
    } finally {
      warn.mockRestore();
      ctx.cleanup();
    }
  });

  it("ports the write builder handle-resolution semantics", () => {
    const builder = new WriteRequestBuilder("ingest");
    const node = builder.addNode({
      rowId: "row-1",
      logicalId: "doc:1",
      kind: "Document",
      properties: { title: "Budget" },
    });
    builder.addChunk({
      id: "chunk:1",
      node,
      textContent: "Budget notes",
    });
    expect(builder.build()).toMatchObject({
      label: "ingest",
      nodes: [{ row_id: "row-1", logical_id: "doc:1", kind: "Document" }],
      chunks: [{ node_logical_id: "doc:1", text_content: "Budget notes" }],
    });
  });

  it("builds edges, runs, steps, and actions with typed inputs", () => {
    const builder = new WriteRequestBuilder("workflow");
    const node = builder.addNode({
      rowId: "r1", logicalId: "n1", kind: "Task", properties: {},
    });
    builder.addEdge({
      rowId: "er1", logicalId: "e1", kind: "depends_on",
      properties: {}, source: node, target: "n2",
    });
    const run = builder.addRun({
      id: "run1", kind: "ingest", status: "running", properties: {},
    });
    const step = builder.addStep({
      id: "step1", run, kind: "parse", status: "done", properties: {},
    });
    builder.addAction({
      id: "act1", step, kind: "extract", status: "done", properties: {},
    });
    const built = builder.build();
    expect(built.edges).toMatchObject([{ source_logical_id: "n1", target_logical_id: "n2" }]);
    expect(built.runs).toMatchObject([{ id: "run1" }]);
    expect(built.steps).toMatchObject([{ run_id: "run1" }]);
    expect(built.actions).toMatchObject([{ step_id: "step1" }]);
  });

  it("builds operational writes with typed inputs", () => {
    const builder = new WriteRequestBuilder("ops");
    builder.addOperationalAppend({
      collection: "events", recordKey: "k1", payloadJson: { x: 1 },
    });
    builder.addOperationalPut({
      collection: "state", recordKey: "k2", payloadJson: { y: 2 },
    });
    builder.addOperationalDelete({ collection: "state", recordKey: "k3" });
    const built = builder.build();
    expect(built.operational_writes).toMatchObject([
      { type: "append", collection: "events", record_key: "k1" },
      { type: "put", collection: "state", record_key: "k2" },
      { type: "delete", collection: "state", record_key: "k3" },
    ]);
  });

  it("rejects foreign builder handles", () => {
    const first = new WriteRequestBuilder("first");
    const second = new WriteRequestBuilder("second");
    const node = first.addNode({
      rowId: "row-1",
      logicalId: "doc:1",
      kind: "Document",
      properties: {},
    });
    expect(() =>
      second.addChunk({ id: "chunk:1", node, textContent: "Budget notes" }),
    ).toThrow(BuilderValidationError);
  });

  it("serializes properties as JSON string in wire format", () => {
    const builder = new WriteRequestBuilder("json-test");
    builder.addNode({
      rowId: "r1", logicalId: "n1", kind: "Doc",
      properties: { nested: { key: "value" } },
    });
    const built = builder.build();
    const nodes = built.nodes as Array<Record<string, unknown>>;
    expect(typeof nodes[0].properties).toBe("string");
    expect(JSON.parse(nodes[0].properties as string)).toEqual({ nested: { key: "value" } });
  });

  it("JSON-encodes plain string properties (no longer passes through raw)", () => {
    const builder = new WriteRequestBuilder("string-test");
    builder.addNode({
      rowId: "r1", logicalId: "n1", kind: "Doc",
      properties: "hello",
    });
    const built = builder.build();
    const nodes = built.nodes as Array<Record<string, unknown>>;
    expect(nodes[0].properties).toBe('"hello"');
  });

  it("PreserializedJson bypasses JSON encoding", () => {
    const builder = new WriteRequestBuilder("preserialized-test");
    builder.addNode({
      rowId: "r1", logicalId: "n1", kind: "Doc",
      properties: new PreserializedJson('{"already":"serialized"}'),
    });
    const built = builder.build();
    const nodes = built.nodes as Array<Record<string, unknown>>;
    expect(nodes[0].properties).toBe('{"already":"serialized"}');
  });

  it("handles undefined properties safely via toJsonString", () => {
    const builder = new WriteRequestBuilder("undef-test");
    builder.addNode({
      rowId: "r1", logicalId: "n1", kind: "Doc",
      properties: undefined as unknown,
    });
    const built = builder.build();
    const nodes = built.nodes as Array<Record<string, unknown>>;
    expect(nodes[0].properties).toBe("null");
  });

  it("builds node with all optional fields", () => {
    const builder = new WriteRequestBuilder("full-test");
    builder.addNode({
      rowId: "r1", logicalId: "n1", kind: "Doc",
      properties: {}, sourceRef: "test-src", upsert: true, chunkPolicy: "replace",
    });
    const built = builder.build();
    const nodes = built.nodes as Array<Record<string, unknown>>;
    expect(nodes[0]).toMatchObject({
      source_ref: "test-src",
      upsert: true,
      chunk_policy: "replace",
    });
  });

  it("builds node with contentRef and chunk with contentHash", () => {
    const builder = new WriteRequestBuilder("ext-content-test");
    const node = builder.addNode({
      rowId: "r1", logicalId: "n1", kind: "Document",
      properties: { title: "Report" },
      contentRef: "s3://docs/report.pdf",
    });
    builder.addChunk({
      id: "c1", node, textContent: "page one",
      contentHash: "sha256:abc123",
    });
    const built = builder.build();
    const nodes = built.nodes as Array<Record<string, unknown>>;
    const chunks = built.chunks as Array<Record<string, unknown>>;
    expect(nodes[0].content_ref).toBe("s3://docs/report.pdf");
    expect(chunks[0].content_hash).toBe("sha256:abc123");
  });

  it("builds run with supersedes_id in wire format", () => {
    const builder = new WriteRequestBuilder("supersede-test");
    builder.addRun({
      id: "run2", kind: "ingest", status: "done",
      properties: {}, supersedesId: "run1",
    });
    const built = builder.build();
    const runs = built.runs as Array<Record<string, unknown>>;
    expect(runs[0].supersedes_id).toBe("run1");
  });

  it("serializes optional backfill payload to JSON string", () => {
    const builder = new WriteRequestBuilder("backfill-test");
    builder.addOptionalBackfill("fts", { some: "data" });
    builder.addOptionalBackfill("vec", new PreserializedJson("already-a-string"));
    builder.addOptionalBackfill("all", "plain-string");
    const built = builder.build();
    const backfills = built.optional_backfills as Array<Record<string, unknown>>;
    expect(backfills).toHaveLength(3);
    expect(backfills[0].target).toBe("fts");
    expect(typeof backfills[0].payload).toBe("string");
    expect(JSON.parse(backfills[0].payload as string)).toEqual({ some: "data" });
    expect(backfills[1].payload).toBe("already-a-string");
    expect(backfills[2].payload).toBe('"plain-string"');
  });
});

// ── runWithFeedback (pure TS) ─────────────────────────────────────────────

describe("runWithFeedback (pure TS)", () => {
  it("emits STARTED then FINISHED for successful operation", () => {
    const events: ResponseCycleEvent[] = [];
    const result = runWithFeedback({
      operationKind: "test.op",
      metadata: { key: "value" },
      progressCallback: (e) => events.push(e),
      feedbackConfig: undefined,
      operation: () => 42,
    });
    expect(result).toBe(42);
    expect(events).toHaveLength(2);
    expect(events[0].phase).toBe("started");
    expect(events[0].operationKind).toBe("test.op");
    expect(events[0].surface).toBe("typescript");
    expect(events[0].metadata).toEqual({ key: "value" });
    expect(events[0].elapsedMs).toBeGreaterThanOrEqual(0);
    expect(events[0].operationId).toMatch(/^[0-9a-f]{32}$/);
    expect(events[1].phase).toBe("finished");
    expect(events[1].operationId).toBe(events[0].operationId);
  });

  it("emits STARTED then FAILED on error with error details", () => {
    const events: ResponseCycleEvent[] = [];
    expect(() =>
      runWithFeedback({
        operationKind: "test.fail",
        metadata: {},
        progressCallback: (e) => events.push(e),
        feedbackConfig: undefined,
        operation: () => { throw new TypeError("broken"); },
      }),
    ).toThrow(TypeError);
    expect(events).toHaveLength(2);
    expect(events[0].phase).toBe("started");
    expect(events[1].phase).toBe("failed");
    expect(events[1].errorCode).toBe("TypeError");
    expect(events[1].errorMessage).toBe("broken");
  });

  it("skips feedback when progressCallback is undefined", () => {
    const result = runWithFeedback({
      operationKind: "test.noop",
      metadata: {},
      progressCallback: undefined,
      feedbackConfig: undefined,
      operation: () => "fast",
    });
    expect(result).toBe("fast");
  });

  it("disables callback on exception from callback itself", () => {
    let callCount = 0;
    const result = runWithFeedback({
      operationKind: "test.bad-callback",
      metadata: {},
      progressCallback: () => { callCount++; throw new Error("callback broken"); },
      feedbackConfig: undefined,
      operation: () => "ok",
    });
    expect(result).toBe("ok");
    expect(callCount).toBe(1);
  });

  it("passes feedbackConfig slowThresholdMs into events", () => {
    const events: ResponseCycleEvent[] = [];
    runWithFeedback({
      operationKind: "test.config",
      metadata: {},
      progressCallback: (e) => events.push(e),
      feedbackConfig: { slowThresholdMs: 1234 },
      operation: () => null,
    });
    expect(events[0].slowThresholdMs).toBe(1234);
  });
});

// ── Engine lifecycle, telemetry, query, write ─────────────────────────────

describe("Engine (real engine)", () => {
  let ctx: TempEngine;
  let engine: Engine;

  beforeEach(() => {
    ctx = openTempEngine();
    engine = ctx.engine;
  });

  afterEach(() => {
    ctx.cleanup();
  });

  it("opens, exposes telemetry, and closes idempotently", () => {
    const snapshot = engine.telemetrySnapshot();
    // A freshly opened engine has nonnegative counters across the board.
    // Exact values vary with schema bootstrap, so assert on shape + range.
    expect(typeof snapshot.queriesTotal).toBe("number");
    expect(typeof snapshot.writesTotal).toBe("number");
    expect(typeof snapshot.writeRowsTotal).toBe("number");
    expect(typeof snapshot.errorsTotal).toBe("number");
    expect(typeof snapshot.adminOpsTotal).toBe("number");
    expect(typeof snapshot.cacheHits).toBe("number");
    expect(typeof snapshot.cacheMisses).toBe("number");
    expect(typeof snapshot.cacheWrites).toBe("number");
    expect(typeof snapshot.cacheSpills).toBe("number");
    expect(snapshot.queriesTotal).toBeGreaterThanOrEqual(0);
    expect(snapshot.errorsTotal).toBe(0);
    // Idempotent close
    engine.close();
    engine.close();
  });

  it("returns typed query results", () => {
    seedSingleDoc(engine, { logicalId: "doc-return-test" });
    const rows = engine.nodes("Doc").execute();
    expect(rows.nodes.length).toBe(1);
    // Wire-format: snake→camel conversion at the napi boundary.
    expect(typeof rows.nodes[0].rowId).toBe("string");
    expect(rows.nodes[0].logicalId).toBe("doc-return-test");
    expect(rows.nodes[0].kind).toBe("Doc");
    expect(rows.nodes[0].contentRef).toBe("s3://docs/test.pdf");
    expect(rows.wasDegraded).toBe(false);
  });

  it("returns typed compiled query", () => {
    const compiled = engine.nodes("Doc").compile();
    expect(typeof compiled.sql).toBe("string");
    expect(compiled.sql.length).toBeGreaterThan(0);
    expect(typeof compiled.shapeHash).toBe("number");
    expect(typeof compiled.drivingTable).toBe("string");
    expect(typeof compiled.hints.recursionLimit).toBe("number");
    expect(typeof compiled.hints.hardLimit).toBe("number");
  });

  it("returns typed query plan", () => {
    const plan = engine.nodes("Doc").explain();
    expect(typeof plan.sql).toBe("string");
    expect(plan.sql.length).toBeGreaterThan(0);
    expect(typeof plan.bindCount).toBe("number");
    expect(typeof plan.cacheHit).toBe("boolean");
  });

  it("returns typed write receipt", () => {
    const builder = new WriteRequestBuilder("test");
    builder.addNode({
      rowId: newRowId(),
      logicalId: "receipt-node",
      kind: "Doc",
      properties: {},
    });
    const receipt = engine.write(builder.build());
    expect(receipt.label).toBe("test");
    expect(receipt.optionalBackfillCount).toBe(0);
    expect(Array.isArray(receipt.warnings)).toBe(true);
    expect(Array.isArray(receipt.provenanceWarnings)).toBe(true);
  });

  it("returns typed last access touch report", () => {
    const seeded = seedSingleDoc(engine, { logicalId: "last-access-node" });
    const report = engine.touchLastAccessed({
      logicalIds: [seeded.logicalId],
      touchedAt: 1_700_000_000,
    });
    expect(report.touchedLogicalIds).toBe(1);
    expect(report.touchedAt).toBe(1_700_000_000);
  });

  it("correctly converts touchLastAccessed to wire format", () => {
    const seeded = seedSingleDoc(engine, { logicalId: "touch-wire" });
    const report = engine.touchLastAccessed({
      logicalIds: [seeded.logicalId],
      touchedAt: 12345,
      sourceRef: "test",
    });
    expect(report.touchedLogicalIds).toBe(1);
    expect(report.touchedAt).toBe(12345);
  });

  it("throws FathomError when operations are called after close", () => {
    engine.close();
    expect(() => engine.nodes("Doc")).toThrow(FathomError);
    expect(() => engine.telemetrySnapshot()).toThrow(FathomError);
    expect(() => engine.write(new WriteRequestBuilder("x").build())).toThrow(FathomError);
  });

  it("close is idempotent", () => {
    engine.close();
    engine.close();
    engine.close();
  });

  it("maps top-level id helpers through the native binding", () => {
    // newId / newRowId are module-level functions that call into the
    // native binding. Real binding returns 32-char lowercase hex.
    const id = newId();
    const rowId = newRowId();
    expect(typeof id).toBe("string");
    expect(id.length).toBeGreaterThan(0);
    expect(typeof rowId).toBe("string");
    expect(rowId.length).toBeGreaterThan(0);
    // Successive calls must return distinct values.
    expect(newId()).not.toBe(id);
    expect(newRowId()).not.toBe(rowId);
  });
});

// ── Admin surface (real engine) ───────────────────────────────────────────

describe("Engine admin (real engine)", () => {
  let ctx: TempEngine;
  let engine: Engine;

  beforeEach(() => {
    ctx = openTempEngine();
    engine = ctx.engine;
  });

  afterEach(() => {
    ctx.cleanup();
  });

  it("returns typed admin reports", () => {
    // Seed a single node tagged with source_ref='src-1' so that
    // traceSource / restore / purge have something to act on.
    const builder = new WriteRequestBuilder("seed-admin");
    builder.addNode({
      rowId: newRowId(),
      logicalId: "admin-n1",
      kind: "Doc",
      properties: {},
      sourceRef: "src-1",
    });
    engine.write(builder.build());

    const integrity = engine.admin.checkIntegrity();
    expect(integrity.physicalOk).toBe(true);
    expect(integrity.foreignKeysOk).toBe(true);

    const semantics = engine.admin.checkSemantics();
    expect(semantics.orphanedChunks).toBe(0);

    const trace = engine.admin.traceSource("src-1");
    expect(trace.sourceRef).toBe("src-1");
    expect(trace.nodeLogicalIds).toContain("admin-n1");
    expect(trace.nodeRows).toBeGreaterThanOrEqual(1);

    // To exercise restoreLogicalId we must first retire the node so it
    // is eligible for restore.
    const retireBuilder = new WriteRequestBuilder("retire-admin-n1");
    retireBuilder.retireNode("admin-n1", "src-1");
    engine.write(retireBuilder.build());

    const restore = engine.admin.restoreLogicalId("admin-n1");
    expect(restore.logicalId).toBe("admin-n1");
    expect(restore.restoredNodeRows).toBe(1);
    expect(restore.wasNoop).toBe(false);

    // Retire again so purge has something to purge.
    const retire2 = new WriteRequestBuilder("retire-admin-n1-again");
    retire2.retireNode("admin-n1", "src-1");
    engine.write(retire2.build());

    const purge = engine.admin.purgeLogicalId("admin-n1");
    expect(purge.logicalId).toBe("admin-n1");
    expect(purge.deletedNodeRows).toBeGreaterThanOrEqual(1);

    const rebuild = engine.admin.rebuild("all");
    expect(rebuild.rebuiltRows).toBeGreaterThanOrEqual(0);
    expect(Array.isArray(rebuild.targets)).toBe(true);

    const manifest = engine.admin.safeExport(`${ctx.dir}/export.db`);
    expect(typeof manifest.sha256).toBe("string");
    expect(manifest.sha256.length).toBeGreaterThan(0);
    expect(manifest.pageCount).toBeGreaterThan(0);
  });

  it("returns typed FTS property schema admin results", () => {
    const record = engine.admin.registerFtsPropertySchema("Goal", ["$.name", "$.description"]);
    expect(record.kind).toBe("Goal");
    expect(record.propertyPaths).toEqual(["$.name", "$.description"]);
    expect(record.separator).toBe(" ");
    expect(record.formatVersion).toBe(1);
    // Pack P7.7-fix: scalar-only schemas also expose the per-entry view
    // with every entry marked "scalar", and an empty excludePaths.
    expect(record.entries).toEqual([
      { path: "$.name", mode: "scalar" },
      { path: "$.description", mode: "scalar" },
    ]);
    expect(record.excludePaths).toEqual([]);

    const described = engine.admin.describeFtsPropertySchema("Goal");
    expect(described).not.toBeNull();
    expect(described!.kind).toBe("Goal");
    expect(described!.propertyPaths).toEqual(["$.name", "$.description"]);
    expect(described!.entries).toEqual([
      { path: "$.name", mode: "scalar" },
      { path: "$.description", mode: "scalar" },
    ]);

    const schemas = engine.admin.listFtsPropertySchemas();
    expect(schemas.length).toBeGreaterThanOrEqual(1);
    const goalSchema = schemas.find((s) => s.kind === "Goal");
    expect(goalSchema).toBeDefined();
    expect(goalSchema!.entries).toEqual([
      { path: "$.name", mode: "scalar" },
      { path: "$.description", mode: "scalar" },
    ]);

    engine.admin.removeFtsPropertySchema("Goal");
    const afterRemove = engine.admin.describeFtsPropertySchema("Goal");
    expect(afterRemove).toBeNull();
  });

  it("round-trips recursive FTS property schema entries through describe and list", () => {
    // Pack P7.7-fix regression: before the fix the engine's load path
    // silently returned an empty propertyPaths for recursive-bearing
    // schemas because it tried to deserialize the object-shaped stored
    // JSON as a bare string array. Now describeFtsPropertySchema and
    // listFtsPropertySchemas both surface the per-entry schema with
    // mode information, plus exclude_paths.
    const registered = engine.admin.registerFtsPropertySchemaWithEntries({
      kind: "KnowledgeItem",
      entries: [
        { path: "$.title", mode: "scalar" },
        { path: "$.payload", mode: "recursive" },
      ],
      separator: " ",
      excludePaths: ["$.payload.secret"],
    });
    expect(registered.kind).toBe("KnowledgeItem");
    expect(registered.propertyPaths).toEqual(["$.title", "$.payload"]);
    expect(registered.entries).toEqual([
      { path: "$.title", mode: "scalar" },
      { path: "$.payload", mode: "recursive" },
    ]);
    expect(registered.excludePaths).toEqual(["$.payload.secret"]);

    const described = engine.admin.describeFtsPropertySchema("KnowledgeItem");
    expect(described).not.toBeNull();
    expect(described!.propertyPaths).toEqual(["$.title", "$.payload"]);
    expect(described!.entries).toEqual([
      { path: "$.title", mode: "scalar" },
      { path: "$.payload", mode: "recursive" },
    ]);
    expect(described!.excludePaths).toEqual(["$.payload.secret"]);

    const listed = engine.admin.listFtsPropertySchemas();
    const ki = listed.find((s) => s.kind === "KnowledgeItem");
    expect(ki).toBeDefined();
    expect(ki!.entries).toEqual([
      { path: "$.title", mode: "scalar" },
      { path: "$.payload", mode: "recursive" },
    ]);
    expect(ki!.excludePaths).toEqual(["$.payload.secret"]);

    engine.admin.removeFtsPropertySchema("KnowledgeItem");
  });

  it("returns typed operational collection admin results", () => {
    const record = engine.admin.registerOperationalCollection({
      name: "events",
      kind: "append_only_log",
      schemaJson: "{}",
      retentionJson: JSON.stringify({ mode: "keep_all" }),
      formatVersion: 1,
      // readOperationalCollection requires that every filter referenced
      // at read time was first declared at registration time.
      filterFieldsJson: JSON.stringify([
        { name: "kind", type: "string", modes: ["exact"] },
      ]),
      validationJson: JSON.stringify({
        format_version: 1,
        mode: "report_only",
        additional_properties: true,
        fields: [],
      }),
    });
    expect(record.name).toBe("events");
    expect(record.kind).toBe("append_only_log");
    expect(record.disabledAt).toBeNull();

    const described = engine.admin.describeOperationalCollection("events");
    expect(described?.name).toBe("events");

    const traceOp = engine.admin.traceOperationalCollection("events");
    expect(traceOp.collectionName).toBe("events");
    expect(traceOp.mutations).toEqual([]);

    const readOp = engine.admin.readOperationalCollection({
      collectionName: "events",
      filters: [{ mode: "exact", field: "kind", value: "startup" }],
    });
    expect(readOp.collectionName).toBe("events");
    expect(readOp.wasLimited).toBe(false);

    // rebuildOperationalCurrent with no collection arg only iterates
    // `latest_state` collections. Our test fixture registers only an
    // `append_only_log` collection, which is not iterated by this API,
    // so `collectionsRebuilt` is always 0 on this path — the strongest
    // assertion we can make here is a shape check. Extending this test
    // to also register a `latest_state` collection would require
    // additional validation / secondary-index scaffolding; the focused
    // integration coverage for non-trivial rebuild counts lives in the
    // Rust engine tests.
    const repairOp = engine.admin.rebuildOperationalCurrent();
    expect(typeof repairOp.collectionsRebuilt).toBe("number");
    expect(repairOp.collectionsRebuilt).toBe(0);
    expect(typeof repairOp.currentRowsRebuilt).toBe("number");
    expect(repairOp.currentRowsRebuilt).toBe(0);

    const validation = engine.admin.validateOperationalCollectionHistory("events");
    expect(validation.invalidRowCount).toBe(0);

    const indexRebuild = engine.admin.rebuildOperationalSecondaryIndexes("events");
    expect(indexRebuild.collectionName).toBe("events");

    const plan = engine.admin.planOperationalRetention(1_700_000_000);
    expect(plan.collectionsExamined).toBeGreaterThanOrEqual(1);

    const run = engine.admin.runOperationalRetention(1_700_000_000, { dryRun: true });
    expect(run.collectionsExamined).toBeGreaterThanOrEqual(1);

    const compaction = engine.admin.compactOperationalCollection("events", false);
    expect(compaction.collectionName).toBe("events");

    const disabled = engine.admin.disableOperationalCollection("events");
    expect(disabled.disabledAt).not.toBeNull();
    expect(typeof disabled.disabledAt).toBe("number");

    const purgeOp = engine.admin.purgeOperationalCollection("events", 500);
    expect(purgeOp.deletedMutations).toBeGreaterThanOrEqual(0);

    const provenance = engine.admin.purgeProvenanceEvents(0, { dry_run: true });
    expect(typeof provenance.eventsPreserved).toBe("number");
    // eventsPreserved is nonnegative; oldestRemaining is non-null when
    // events exist.
    expect(provenance.eventsPreserved).toBeGreaterThanOrEqual(0);
  });
});

// ── Feedback event emission against the real engine ─────────────────────

describe("Engine feedback callbacks (real engine)", () => {
  let ctx: TempEngine;
  let engine: Engine;

  beforeEach(() => {
    ctx = openTempEngine();
    engine = ctx.engine;
  });

  afterEach(() => {
    ctx.cleanup();
  });

  it("Engine.write emits feedback events when callback provided", () => {
    const events: ResponseCycleEvent[] = [];
    const builder = new WriteRequestBuilder("feedback-test");
    engine.write(builder.build(), (e) => events.push(e));
    expect(events.length).toBeGreaterThanOrEqual(2);
    expect(events[0].phase).toBe("started");
    expect(events[0].operationKind).toBe("write.submit");
    expect(events[events.length - 1].phase).toBe("finished");
  });

  it("Query.execute emits feedback events when callback provided", () => {
    seedSingleDoc(engine, { logicalId: "feedback-doc" });
    const events: ResponseCycleEvent[] = [];
    engine.nodes("Doc").execute((e) => events.push(e));
    expect(events.length).toBeGreaterThanOrEqual(2);
    expect(events[0].operationKind).toBe("query.execute");
  });

  it("AdminClient.checkIntegrity emits feedback events", () => {
    const events: ResponseCycleEvent[] = [];
    engine.admin.checkIntegrity((e) => events.push(e));
    expect(events[0].operationKind).toBe("admin.check_integrity");
  });
});

// ── Scoped mocks for error-path coverage ─────────────────────────────────
//
// These tests cover code paths that only fire when the native binding
// throws a specific error or returns malformed JSON. Reproducing those
// conditions against a real engine would require corrupting the on-disk
// database or injecting faults inside the Rust layer, which is out of
// scope for this pack. Each test installs a per-test mock, uses it, and
// clears it in afterEach so it cannot leak into other tests.

describe("Engine error mapping (scoped mocks)", () => {
  afterEach(() => {
    delete (globalThis as { __FATHOMDB_NATIVE_MOCK__?: unknown }).__FATHOMDB_NATIVE_MOCK__;
    Engine.setBindingForTests(null);
  });

  function installErrorMock(overrides: Record<string, unknown>): void {
    const core = {
      close: vi.fn(),
      telemetrySnapshot: vi.fn(() => "{}"),
      describeOperationalCollection: vi.fn(() => "{}"),
      ...overrides,
    };
    const binding = {
      EngineCore: { open: vi.fn(() => core) },
      newId: vi.fn(() => "id-1"),
      newRowId: vi.fn(() => "row-1"),
    };
    (globalThis as { __FATHOMDB_NATIVE_MOCK__?: unknown }).__FATHOMDB_NATIVE_MOCK__ = binding;
    Engine.setBindingForTests(binding as never);
  }

  it("maps native FATHOMDB_SQLITE_ERROR to SqliteError via callNative", () => {
    installErrorMock({
      telemetrySnapshot: vi.fn(() => {
        throw new Error("FATHOMDB_SQLITE_ERROR::disk I/O error");
      }),
    });
    const engine = Engine.open("/tmp/test.db");
    expect(() => engine.telemetrySnapshot()).toThrow(SqliteError);
    try {
      engine.telemetrySnapshot();
    } catch (e) {
      expect(e).toBeInstanceOf(SqliteError);
      expect((e as SqliteError).message).toBe("disk I/O error");
    }
  });

  it("maps native error from describeOperationalCollection via callNative", () => {
    installErrorMock({
      describeOperationalCollection: vi.fn(() => {
        throw new Error("FATHOMDB_SQLITE_ERROR::table not found");
      }),
    });
    const engine = Engine.open("/tmp/test.db");
    expect(() => engine.admin.describeOperationalCollection("events")).toThrow(SqliteError);
  });

  it("uses parseNativeJson in describeOperationalCollection for malformed JSON", () => {
    installErrorMock({
      describeOperationalCollection: vi.fn(() => "not valid json{{{"),
    });
    const engine = Engine.open("/tmp/test.db");
    expect(() => engine.admin.describeOperationalCollection("events")).toThrow();
  });
});
