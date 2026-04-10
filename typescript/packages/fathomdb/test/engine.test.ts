import { beforeEach, describe, expect, it, vi } from "vitest";
import { BuilderValidationError, FathomError, SqliteError, Engine, PreserializedJson, Query, WriteRequestBuilder, newId, newRowId, type ResponseCycleEvent } from "../src/index.js";
import { runWithFeedback } from "../src/feedback.js";

describe("Engine", () => {
  beforeEach(() => {
    const binding = {
      EngineCore: {
        open: vi.fn(() => ({
          close: vi.fn(),
          telemetrySnapshot: vi.fn(() =>
            JSON.stringify({
              queries_total: 1,
              writes_total: 2,
              write_rows_total: 3,
              errors_total: 4,
              admin_ops_total: 5,
              cache_hits: 6,
              cache_misses: 7,
              cache_writes: 8,
              cache_spills: 9
            })
          ),
          compileAst: vi.fn(() => JSON.stringify({
            sql: "SELECT * FROM nodes", binds: [], shape_hash: 42,
            driving_table: "nodes", hints: { recursion_limit: 100, hard_limit: 1000 }
          })),
          compileGroupedAst: vi.fn(() => JSON.stringify({
            root: { sql: "SELECT * FROM nodes", binds: [], shape_hash: 42, driving_table: "nodes", hints: { recursion_limit: 100, hard_limit: 1000 } },
            expansions: [], shape_hash: 99, hints: { recursion_limit: 100, hard_limit: 1000 }
          })),
          explainAst: vi.fn(() => JSON.stringify({
            sql: "SELECT * FROM nodes", bind_count: 0, driving_table: "nodes", shape_hash: 42, cache_hit: false
          })),
          executeAst: vi.fn(() => JSON.stringify({
            nodes: [{ row_id: "r1", logical_id: "n1", kind: "Doc", properties: "{}", content_ref: "s3://docs/test.pdf", last_accessed_at: null }],
            runs: [], steps: [], actions: [], was_degraded: false
          })),
          executeGroupedAst: vi.fn(() => JSON.stringify({
            roots: [], expansions: [], was_degraded: false
          })),
          submitWrite: vi.fn(() => JSON.stringify({
            label: "test", optional_backfill_count: 0, warnings: [], provenance_warnings: []
          })),
          touchLastAccessed: vi.fn(() => JSON.stringify({
            touched_logical_ids: 2, touched_at: 1000
          })),
          checkIntegrity: vi.fn(() => JSON.stringify({
            physical_ok: true, foreign_keys_ok: true, missing_fts_rows: 0,
            duplicate_active_logical_ids: 0, operational_missing_collections: 0,
            operational_missing_last_mutations: 0, warnings: []
          })),
          checkSemantics: vi.fn(() => JSON.stringify({
            orphaned_chunks: 0, null_source_ref_nodes: 0, broken_step_fk: 0,
            broken_action_fk: 0, stale_fts_rows: 0, fts_rows_for_superseded_nodes: 0,
            dangling_edges: 0, orphaned_supersession_chains: 0, stale_vec_rows: 0,
            vec_rows_for_superseded_nodes: 0, missing_operational_current_rows: 0,
            stale_operational_current_rows: 0, disabled_collection_mutations: 0,
            orphaned_last_access_metadata_rows: 0, warnings: []
          })),
          rebuildProjections: vi.fn(() => JSON.stringify({ targets: ["all"], rebuilt_rows: 5, notes: [] })),
          rebuildMissingProjections: vi.fn(() => JSON.stringify({ targets: ["all"], rebuilt_rows: 0, notes: [] })),
          traceSource: vi.fn(() => JSON.stringify({
            source_ref: "src-1", node_rows: 1, edge_rows: 0, action_rows: 0,
            operational_mutation_rows: 0, node_logical_ids: ["n1"], action_ids: [], operational_mutation_ids: []
          })),
          exciseSource: vi.fn(() => JSON.stringify({
            source_ref: "src-1", node_rows: 1, edge_rows: 0, action_rows: 0,
            operational_mutation_rows: 0, node_logical_ids: ["n1"], action_ids: [], operational_mutation_ids: []
          })),
          restoreLogicalId: vi.fn(() => JSON.stringify({
            logical_id: "n1", was_noop: false, restored_node_rows: 1, restored_edge_rows: 0,
            restored_chunk_rows: 0, restored_fts_rows: 0, restored_vec_rows: 0, skipped_edges: [], notes: []
          })),
          purgeLogicalId: vi.fn(() => JSON.stringify({
            logical_id: "n1", was_noop: false, deleted_node_rows: 1, deleted_edge_rows: 0,
            deleted_chunk_rows: 0, deleted_fts_rows: 0, deleted_vec_rows: 0, notes: []
          })),
          safeExport: vi.fn(() => JSON.stringify({
            exported_at: 1000, sha256: "abc", schema_version: 1, protocol_version: 1, page_count: 10
          })),
          // Operational collection mocks
          registerOperationalCollection: vi.fn(() => JSON.stringify({
            name: "events", kind: "append_only_log", schema_json: "{}", retention_json: "{}",
            validation_json: "", secondary_indexes_json: "[]", format_version: 1,
            created_at: 1000, filter_fields_json: "[]", disabled_at: null
          })),
          describeOperationalCollection: vi.fn(() => JSON.stringify({
            name: "events", kind: "append_only_log", schema_json: "{}", retention_json: "{}",
            validation_json: "", secondary_indexes_json: "[]", format_version: 1,
            created_at: 1000, filter_fields_json: "[]", disabled_at: null
          })),
          updateOperationalCollectionFilters: vi.fn(() => JSON.stringify({
            name: "events", kind: "append_only_log", schema_json: "{}", retention_json: "{}",
            validation_json: "", secondary_indexes_json: "[]", format_version: 1,
            created_at: 1000, filter_fields_json: "[]", disabled_at: null
          })),
          updateOperationalCollectionValidation: vi.fn(() => JSON.stringify({
            name: "events", kind: "append_only_log", schema_json: "{}", retention_json: "{}",
            validation_json: "", secondary_indexes_json: "[]", format_version: 1,
            created_at: 1000, filter_fields_json: "[]", disabled_at: null
          })),
          updateOperationalCollectionSecondaryIndexes: vi.fn(() => JSON.stringify({
            name: "events", kind: "append_only_log", schema_json: "{}", retention_json: "{}",
            validation_json: "", secondary_indexes_json: "[]", format_version: 1,
            created_at: 1000, filter_fields_json: "[]", disabled_at: null
          })),
          traceOperationalCollection: vi.fn(() => JSON.stringify({
            collection_name: "events", record_key: null, mutation_count: 0,
            current_count: 0, mutations: [], current_rows: []
          })),
          readOperationalCollection: vi.fn(() => JSON.stringify({
            collection_name: "events", row_count: 0, applied_limit: 100, was_limited: false, rows: []
          })),
          rebuildOperationalCurrent: vi.fn(() => JSON.stringify({ collections_rebuilt: 1, current_rows_rebuilt: 0 })),
          validateOperationalCollectionHistory: vi.fn(() => JSON.stringify({
            collection_name: "events", checked_rows: 0, invalid_row_count: 0, issues: []
          })),
          rebuildOperationalSecondaryIndexes: vi.fn(() => JSON.stringify({
            collection_name: "events", mutation_entries_rebuilt: 0, current_entries_rebuilt: 0
          })),
          planOperationalRetention: vi.fn(() => JSON.stringify({
            planned_at: 1000, collections_examined: 1, items: []
          })),
          runOperationalRetention: vi.fn(() => JSON.stringify({
            executed_at: 1000, collections_examined: 1, collections_acted_on: 0, dry_run: false, items: []
          })),
          disableOperationalCollection: vi.fn(() => JSON.stringify({
            name: "events", kind: "append_only_log", schema_json: "{}", retention_json: "{}",
            validation_json: "", secondary_indexes_json: "[]", format_version: 1,
            created_at: 1000, filter_fields_json: "[]", disabled_at: 2000
          })),
          compactOperationalCollection: vi.fn(() => JSON.stringify({
            collection_name: "events", deleted_mutations: 0, dry_run: false, before_timestamp: null
          })),
          purgeOperationalCollection: vi.fn(() => JSON.stringify({
            collection_name: "events", deleted_mutations: 0, before_timestamp: 500
          })),
          purgeProvenanceEvents: vi.fn(() => JSON.stringify({
            events_deleted: 0, events_preserved: 10, oldest_remaining: 100
          }))
        }))
      },
      newId: vi.fn(() => "id-1"),
      newRowId: vi.fn(() => "row-1")
    };
    globalThis.__FATHOMDB_NATIVE_MOCK__ = binding as never;
    Engine.setBindingForTests(binding as never);
  });

  it("opens, exposes telemetry, and closes idempotently", () => {
    const engine = Engine.open("/tmp/test.db");
    expect(engine.telemetrySnapshot()).toEqual({
      queriesTotal: 1,
      writesTotal: 2,
      writeRowsTotal: 3,
      errorsTotal: 4,
      adminOpsTotal: 5,
      cacheHits: 6,
      cacheMisses: 7,
      cacheWrites: 8,
      cacheSpills: 9
    });
    engine.close();
    engine.close();
  });

  it("builds immutable queries with python-parity AST shape", () => {
    const engine = Engine.open("/tmp/test.db");
    const base = engine.nodes("Meeting");
    const query = base
      .textSearch("budget", 5)
      .filterJsonTextEq("$.status", "active")
      .expand({ slot: "neighbors", direction: "out", label: "depends_on", maxDepth: 2 })
      .limit(10);

    expect(base).toBeInstanceOf(Query);
    expect(base.toAst()).toEqual({
      root_kind: "Meeting",
      steps: [],
      expansions: [],
      final_limit: null
    });
    expect(query.toAst()).toEqual({
      root_kind: "Meeting",
      steps: [
        { type: "text_search", query: "budget", limit: 5 },
        { type: "filter_json_text_eq", path: "$.status", value: "active" }
      ],
      expansions: [
        { slot: "neighbors", direction: "out", label: "depends_on", max_depth: 2 }
      ],
      final_limit: 10
    });
  });

  it("returns typed query results", () => {
    const engine = Engine.open("/tmp/test.db");
    const rows = engine.nodes("Doc").execute();
    expect(rows.nodes).toHaveLength(1);
    expect(rows.nodes[0].rowId).toBe("r1");
    expect(rows.nodes[0].logicalId).toBe("n1");
    expect(rows.nodes[0].kind).toBe("Doc");
    expect(rows.nodes[0].contentRef).toBe("s3://docs/test.pdf");
    expect(rows.wasDegraded).toBe(false);
  });

  it("returns typed compiled query", () => {
    const engine = Engine.open("/tmp/test.db");
    const compiled = engine.nodes("Doc").compile();
    expect(compiled.sql).toBe("SELECT * FROM nodes");
    expect(compiled.shapeHash).toBe(42);
    expect(compiled.drivingTable).toBe("nodes");
    expect(compiled.hints.recursionLimit).toBe(100);
  });

  it("returns typed query plan", () => {
    const engine = Engine.open("/tmp/test.db");
    const plan = engine.nodes("Doc").explain();
    expect(plan.sql).toBe("SELECT * FROM nodes");
    expect(plan.bindCount).toBe(0);
    expect(plan.cacheHit).toBe(false);
  });

  it("returns typed write receipt", () => {
    const engine = Engine.open("/tmp/test.db");
    const builder = new WriteRequestBuilder("test");
    const receipt = engine.write(builder.build());
    expect(receipt.label).toBe("test");
    expect(receipt.optionalBackfillCount).toBe(0);
    expect(receipt.warnings).toEqual([]);
    expect(receipt.provenanceWarnings).toEqual([]);
  });

  it("returns typed last access touch report", () => {
    const engine = Engine.open("/tmp/test.db");
    const report = engine.touchLastAccessed({ logicalIds: ["n1", "n2"], touchedAt: 1000 });
    expect(report.touchedLogicalIds).toBe(2);
    expect(report.touchedAt).toBe(1000);
  });

  it("returns typed admin reports", () => {
    const engine = Engine.open("/tmp/test.db");
    const integrity = engine.admin.checkIntegrity();
    expect(integrity.physicalOk).toBe(true);
    expect(integrity.foreignKeysOk).toBe(true);

    const semantics = engine.admin.checkSemantics();
    expect(semantics.orphanedChunks).toBe(0);

    const trace = engine.admin.traceSource("src-1");
    expect(trace.sourceRef).toBe("src-1");
    expect(trace.nodeLogicalIds).toEqual(["n1"]);

    const restore = engine.admin.restoreLogicalId("n1");
    expect(restore.logicalId).toBe("n1");
    expect(restore.restoredNodeRows).toBe(1);

    const purge = engine.admin.purgeLogicalId("n1");
    expect(purge.logicalId).toBe("n1");
    expect(purge.deletedNodeRows).toBe(1);

    const rebuild = engine.admin.rebuild("all");
    expect(rebuild.rebuiltRows).toBe(5);

    const manifest = engine.admin.safeExport("/tmp/export.db");
    expect(manifest.sha256).toBe("abc");
    expect(manifest.pageCount).toBe(10);
  });

  it("returns typed operational collection admin results", () => {
    const engine = Engine.open("/tmp/test.db");

    const record = engine.admin.registerOperationalCollection({
      name: "events", kind: "append_only_log", schemaJson: "{}", retentionJson: "{}", formatVersion: 1,
    });
    expect(record.name).toBe("events");
    expect(record.kind).toBe("append_only_log");
    expect(record.disabledAt).toBeNull();

    const described = engine.admin.describeOperationalCollection("events");
    expect(described?.name).toBe("events");

    const traceOp = engine.admin.traceOperationalCollection("events");
    expect(traceOp.collectionName).toBe("events");
    expect(traceOp.mutations).toEqual([]);

    const readOp = engine.admin.readOperationalCollection({ collectionName: "events", filters: [] });
    expect(readOp.collectionName).toBe("events");
    expect(readOp.wasLimited).toBe(false);

    const repairOp = engine.admin.rebuildOperationalCurrent();
    expect(repairOp.collectionsRebuilt).toBe(1);

    const validation = engine.admin.validateOperationalCollectionHistory("events");
    expect(validation.invalidRowCount).toBe(0);

    const indexRebuild = engine.admin.rebuildOperationalSecondaryIndexes("events");
    expect(indexRebuild.collectionName).toBe("events");

    const plan = engine.admin.planOperationalRetention(1000);
    expect(plan.collectionsExamined).toBe(1);

    const run = engine.admin.runOperationalRetention(1000, { dryRun: true });
    expect(run.collectionsExamined).toBe(1);

    const disabled = engine.admin.disableOperationalCollection("events");
    expect(disabled.disabledAt).toBe(2000);

    const compaction = engine.admin.compactOperationalCollection("events", false);
    expect(compaction.collectionName).toBe("events");

    const purgeOp = engine.admin.purgeOperationalCollection("events", 500);
    expect(purgeOp.deletedMutations).toBe(0);

    const provenance = engine.admin.purgeProvenanceEvents(500);
    expect(provenance.eventsPreserved).toBe(10);
    expect(provenance.oldestRemaining).toBe(100);
  });

  it("maps top-level id helpers through the native binding", () => {
    expect(newId()).toBe("id-1");
    expect(newRowId()).toBe("row-1");
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
      chunks: [{ node_logical_id: "doc:1", text_content: "Budget notes" }]
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
      second.addChunk({ id: "chunk:1", node, textContent: "Budget notes" })
    ).toThrow(BuilderValidationError);
  });

  it("throws FathomError when operations are called after close", () => {
    const engine = Engine.open("/tmp/test.db");
    engine.close();
    expect(() => engine.nodes("Doc")).toThrow(FathomError);
    expect(() => engine.telemetrySnapshot()).toThrow(FathomError);
    expect(() => engine.write(new WriteRequestBuilder("x").build())).toThrow(FathomError);
  });

  it("close is idempotent", () => {
    const engine = Engine.open("/tmp/test.db");
    engine.close();
    engine.close();
    engine.close();
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
    // Plain strings must be JSON-encoded (wrapped in quotes)
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
    // PreserializedJson passes through unchanged
    expect(backfills[1].payload).toBe("already-a-string");
    // Plain strings are now JSON-encoded
    expect(backfills[2].payload).toBe('"plain-string"');
  });

  it("correctly converts touchLastAccessed to wire format", () => {
    const engine = Engine.open("/tmp/test.db");
    const report = engine.touchLastAccessed({
      logicalIds: ["a", "b"],
      touchedAt: 12345,
      sourceRef: "test",
    });
    expect(report.touchedLogicalIds).toBe(2);
    expect(report.touchedAt).toBe(1000);
  });

  // ── Feedback / progress callback tests ────────────────────────────

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
      })
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
    expect(callCount).toBe(1); // STARTED fires, throws, then FINISHED is suppressed
  });

  it("maps native FATHOMDB_SQLITE_ERROR to SqliteError via callNative", () => {
    const engine = Engine.open("/tmp/test.db");
    // Make the native method throw with a FATHOMDB_ prefixed error
    const binding = (globalThis as Record<string, unknown>).__FATHOMDB_NATIVE_MOCK__ as Record<string, unknown>;
    const core = (binding.EngineCore as Record<string, unknown>).open as ReturnType<typeof vi.fn>;
    const mockCore = core.mock.results[0].value;
    mockCore.telemetrySnapshot.mockImplementation(() => {
      throw new Error("FATHOMDB_SQLITE_ERROR::disk I/O error");
    });
    expect(() => engine.telemetrySnapshot()).toThrow(SqliteError);
    try {
      engine.telemetrySnapshot();
    } catch (e) {
      expect(e).toBeInstanceOf(SqliteError);
      expect((e as SqliteError).message).toBe("disk I/O error");
    }
  });

  it("maps native error from describeOperationalCollection via callNative", () => {
    const engine = Engine.open("/tmp/test.db");
    const binding = (globalThis as Record<string, unknown>).__FATHOMDB_NATIVE_MOCK__ as Record<string, unknown>;
    const core = (binding.EngineCore as Record<string, unknown>).open as ReturnType<typeof vi.fn>;
    const mockCore = core.mock.results[0].value;
    // Native call itself throws a FATHOMDB_ error
    mockCore.describeOperationalCollection.mockImplementation(() => {
      throw new Error("FATHOMDB_SQLITE_ERROR::table not found");
    });
    expect(() => engine.admin.describeOperationalCollection("events")).toThrow(SqliteError);
  });

  it("uses parseNativeJson in describeOperationalCollection for malformed JSON", () => {
    const engine = Engine.open("/tmp/test.db");
    const binding = (globalThis as Record<string, unknown>).__FATHOMDB_NATIVE_MOCK__ as Record<string, unknown>;
    const core = (binding.EngineCore as Record<string, unknown>).open as ReturnType<typeof vi.fn>;
    const mockCore = core.mock.results[0].value;
    // Native call returns invalid JSON -- parseNativeJson should handle it
    mockCore.describeOperationalCollection.mockImplementation(() => "not valid json{{{");
    expect(() => engine.admin.describeOperationalCollection("events")).toThrow();
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

  it("Engine.write emits feedback events when callback provided", () => {
    const engine = Engine.open("/tmp/test.db");
    const events: ResponseCycleEvent[] = [];
    const builder = new WriteRequestBuilder("feedback-test");
    engine.write(builder.build(), (e) => events.push(e));
    expect(events.length).toBeGreaterThanOrEqual(2);
    expect(events[0].phase).toBe("started");
    expect(events[0].operationKind).toBe("write.submit");
    expect(events[events.length - 1].phase).toBe("finished");
  });

  it("Query.execute emits feedback events when callback provided", () => {
    const engine = Engine.open("/tmp/test.db");
    const events: ResponseCycleEvent[] = [];
    engine.nodes("Doc").execute((e) => events.push(e));
    expect(events.length).toBeGreaterThanOrEqual(2);
    expect(events[0].operationKind).toBe("query.execute");
  });

  it("AdminClient.checkIntegrity emits feedback events", () => {
    const engine = Engine.open("/tmp/test.db");
    const events: ResponseCycleEvent[] = [];
    engine.admin.checkIntegrity((e) => events.push(e));
    expect(events[0].operationKind).toBe("admin.check_integrity");
  });
});
