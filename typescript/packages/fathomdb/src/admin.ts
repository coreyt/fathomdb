import { callNative, parseNativeJson, parseNativeJsonArray } from "./errors.js";
import { runWithFeedback } from "./feedback.js";
import type { NativeEngineCore } from "./native.js";
import {
  ftsPropertySchemaRecordFromWire,
  integrityReportFromWire,
  logicalPurgeReportFromWire,
  logicalRestoreReportFromWire,
  operationalCollectionRecordFromWire,
  operationalCompactionReportFromWire,
  operationalHistoryValidationReportFromWire,
  operationalPurgeReportFromWire,
  operationalReadRequestToWire,
  operationalRegisterRequestToWire,
  operationalReadReportFromWire,
  operationalRepairReportFromWire,
  operationalRetentionPlanReportFromWire,
  operationalRetentionRunReportFromWire,
  operationalSecondaryIndexRebuildReportFromWire,
  operationalTraceReportFromWire,
  projectionRepairReportFromWire,
  provenancePurgeReportFromWire,
  safeExportManifestFromWire,
  semanticReportFromWire,
  traceReportFromWire,
  type FeedbackConfig,
  type FtsPropertySchemaRecord,
  type IntegrityReport,
  type LogicalPurgeReport,
  type LogicalRestoreReport,
  type OperationalCollectionRecord,
  type OperationalCompactionReport,
  type OperationalHistoryValidationReport,
  type OperationalPurgeReport,
  type OperationalReadReport,
  type OperationalRepairReport,
  type OperationalRetentionPlanReport,
  type OperationalRetentionRunReport,
  type OperationalSecondaryIndexRebuildReport,
  type OperationalTraceReport,
  type OperationalReadRequest,
  type OperationalRegisterRequest,
  type ProgressCallback,
  type ProjectionRepairReport,
  type ProjectionTarget,
  type ProvenancePurgeReport,
  type SafeExportManifest,
  type SemanticReport,
  type TraceReport,
} from "./types.js";

/**
 * Administrative operations for a fathomdb database.
 *
 * Provides integrity checks, projection rebuilds, source tracing, safe
 * exports, and operational collection management. Accessed via {@link Engine.admin}.
 */
export class AdminClient {
  readonly #core: NativeEngineCore;

  constructor(core: NativeEngineCore) {
    this.#core = core;
  }

  #run<T>(operationKind: string, operation: () => T, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): T {
    return runWithFeedback({ operationKind, metadata: {}, progressCallback, feedbackConfig, operation });
  }

  // ── Integrity & validation ────────────────────────────────────────

  /** Run physical and logical integrity checks on the database. */
  checkIntegrity(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): IntegrityReport {
    return this.#run("admin.check_integrity", () => integrityReportFromWire(parseNativeJson(callNative(() => this.#core.checkIntegrity()))), progressCallback, feedbackConfig);
  }

  /** Run semantic validation (orphan chunks, dangling edges, etc.). */
  checkSemantics(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): SemanticReport {
    return this.#run("admin.check_semantics", () => semanticReportFromWire(parseNativeJson(callNative(() => this.#core.checkSemantics()))), progressCallback, feedbackConfig);
  }

  // ── Projection maintenance ────────────────────────────────────────

  /**
   * Rebuild projection indexes (FTS, vector, or all).
   *
   * @param target - Which projections to rebuild (`"fts"`, `"vec"`, or `"all"`).
   */
  rebuild(target: ProjectionTarget = "all", progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): ProjectionRepairReport {
    return this.#run("admin.rebuild", () => projectionRepairReportFromWire(parseNativeJson(callNative(() => this.#core.rebuildProjections(target)))), progressCallback, feedbackConfig);
  }

  /** Rebuild only missing projection rows without touching existing ones. */
  rebuildMissing(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): ProjectionRepairReport {
    return this.#run("admin.rebuild_missing", () => projectionRepairReportFromWire(parseNativeJson(callNative(() => this.#core.rebuildMissingProjections()))), progressCallback, feedbackConfig);
  }

  // ── Source tracing & excision ─────────────────────────────────────

  /**
   * Trace all nodes, edges, and actions originating from a source reference.
   *
   * @param sourceRef - The provenance source reference to trace.
   */
  traceSource(sourceRef: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): TraceReport {
    return this.#run("admin.trace_source", () => traceReportFromWire(parseNativeJson(callNative(() => this.#core.traceSource(sourceRef)))), progressCallback, feedbackConfig);
  }

  /**
   * Remove all data originating from a source reference.
   *
   * @param sourceRef - The provenance source reference to excise.
   */
  exciseSource(sourceRef: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): TraceReport {
    return this.#run("admin.excise_source", () => traceReportFromWire(parseNativeJson(callNative(() => this.#core.exciseSource(sourceRef)))), progressCallback, feedbackConfig);
  }

  // ── Logical ID management ─────────────────────────────────────────

  /**
   * Restore a previously retired node by its logical ID.
   *
   * @param logicalId - The logical ID of the node to restore.
   */
  restoreLogicalId(logicalId: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): LogicalRestoreReport {
    return this.#run("admin.restore_logical_id", () => logicalRestoreReportFromWire(parseNativeJson(callNative(() => this.#core.restoreLogicalId(logicalId)))), progressCallback, feedbackConfig);
  }

  /**
   * Permanently delete all rows associated with a logical ID.
   *
   * @param logicalId - The logical ID to purge.
   */
  purgeLogicalId(logicalId: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): LogicalPurgeReport {
    return this.#run("admin.purge_logical_id", () => logicalPurgeReportFromWire(parseNativeJson(callNative(() => this.#core.purgeLogicalId(logicalId)))), progressCallback, feedbackConfig);
  }

  // ── Safe export ───────────────────────────────────────────────────

  /**
   * Export a consistent snapshot of the database to a file path.
   *
   * @param destinationPath - Filesystem path for the exported database file.
   * @param options - Export options.
   * @param options.forceCheckpoint - Whether to force a WAL checkpoint before export (default `true`).
   */
  safeExport(destinationPath: string, options: { forceCheckpoint?: boolean } = {}, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): SafeExportManifest {
    return this.#run("admin.safe_export", () => safeExportManifestFromWire(parseNativeJson(callNative(() => this.#core.safeExport(destinationPath, options.forceCheckpoint ?? true)))), progressCallback, feedbackConfig);
  }

  // ── FTS property schema management ───────────────────────────────

  /**
   * Register (or update) an FTS property projection schema for a node kind.
   *
   * After registration, nodes of this kind will have the declared JSON property
   * paths extracted, concatenated with the separator, and indexed for full-text
   * search. `textSearch(...)` transparently covers both chunk-backed and
   * property-backed results.
   *
   * This is an idempotent upsert: calling it again with different paths or
   * separator overwrites the previous schema. Registration does **not** rewrite
   * existing FTS rows; call `rebuild("fts")` to backfill.
   *
   * Paths must use simple `$.`-prefixed dot-notation (e.g. `$.title`,
   * `$.address.city`). Array indexing, wildcards, recursive descent, and
   * duplicate paths are rejected.
   *
   * @param kind - The node kind to register (e.g. `"Goal"`).
   * @param propertyPaths - Ordered list of JSON paths to extract.
   * @param separator - Concatenation separator (default `" "`).
   */
  registerFtsPropertySchema(kind: string, propertyPaths: string[], separator?: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): FtsPropertySchemaRecord {
    return this.#run("admin.register_fts_property_schema", () => ftsPropertySchemaRecordFromWire(parseNativeJson(callNative(() => this.#core.registerFtsPropertySchema(kind, JSON.stringify(propertyPaths), separator)))), progressCallback, feedbackConfig);
  }

  /**
   * Return the FTS property schema for a single node kind, or `null` if not registered.
   */
  describeFtsPropertySchema(kind: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): FtsPropertySchemaRecord | null {
    return this.#run("admin.describe_fts_property_schema", () => {
      const json = callNative(() => this.#core.describeFtsPropertySchema(kind));
      const raw = parseNativeJson(json);
      if (raw === null || raw.kind == null) return null;
      return ftsPropertySchemaRecordFromWire(raw as Record<string, unknown>);
    }, progressCallback, feedbackConfig);
  }

  /**
   * Return all registered FTS property schemas.
   */
  listFtsPropertySchemas(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): FtsPropertySchemaRecord[] {
    return this.#run("admin.list_fts_property_schemas", () => {
      const json = callNative(() => this.#core.listFtsPropertySchemas());
      const arr = parseNativeJsonArray(json);
      return arr.map(ftsPropertySchemaRecordFromWire);
    }, progressCallback, feedbackConfig);
  }

  /**
   * Remove the FTS property schema for a node kind.
   *
   * This deletes the schema row but does **not** delete existing derived
   * `fts_node_properties` rows. An explicit `rebuild("fts")` is required to
   * clean up stale rows after removal.
   *
   * Throws if the kind is not registered.
   */
  removeFtsPropertySchema(kind: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): void {
    this.#run("admin.remove_fts_property_schema", () => {
      callNative(() => this.#core.removeFtsPropertySchema(kind));
    }, progressCallback, feedbackConfig);
  }

  // ── Operational collection lifecycle ──────────────────────────────

  /**
   * Register a new operational collection with the given schema and retention.
   *
   * @param request - Registration request including name, kind, and retention settings.
   */
  registerOperationalCollection(request: OperationalRegisterRequest, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord {
    return this.#run("admin.register_operational_collection", () => operationalCollectionRecordFromWire(parseNativeJson(callNative(() => this.#core.registerOperationalCollection(JSON.stringify(operationalRegisterRequestToWire(request)))))), progressCallback, feedbackConfig);
  }

  /**
   * Return the record for a named operational collection, or `null` if not found.
   *
   * @param name - Name of the operational collection.
   */
  describeOperationalCollection(name: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord | null {
    return this.#run("admin.describe_operational_collection", () => {
      const json = callNative(() => this.#core.describeOperationalCollection(name));
      const raw = parseNativeJson(json);
      if (raw === null || raw.name == null) return null;
      return operationalCollectionRecordFromWire(raw as Record<string, unknown>);
    }, progressCallback, feedbackConfig);
  }

  /**
   * Disable an operational collection, preventing new writes.
   *
   * @param name - Name of the operational collection.
   */
  disableOperationalCollection(name: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord {
    return this.#run("admin.disable_operational_collection", () => operationalCollectionRecordFromWire(parseNativeJson(callNative(() => this.#core.disableOperationalCollection(name)))), progressCallback, feedbackConfig);
  }

  // ── Operational collection config ─────────────────────────────────

  /**
   * Replace the filter field definitions for an operational collection.
   *
   * @param name - Name of the operational collection.
   * @param filterFields - New filter field definitions.
   */
  updateOperationalCollectionFilters(name: string, filterFields: unknown, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord {
    return this.#run("admin.update_operational_collection_filters", () => operationalCollectionRecordFromWire(parseNativeJson(callNative(() => this.#core.updateOperationalCollectionFilters(name, JSON.stringify(filterFields))))), progressCallback, feedbackConfig);
  }

  /**
   * Replace the validation rules for an operational collection.
   *
   * @param name - Name of the operational collection.
   * @param validation - New validation rules.
   */
  updateOperationalCollectionValidation(name: string, validation: unknown, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord {
    return this.#run("admin.update_operational_collection_validation", () => operationalCollectionRecordFromWire(parseNativeJson(callNative(() => this.#core.updateOperationalCollectionValidation(name, JSON.stringify(validation))))), progressCallback, feedbackConfig);
  }

  /**
   * Replace the secondary index definitions for an operational collection.
   *
   * @param name - Name of the operational collection.
   * @param secondaryIndexes - New secondary index definitions.
   */
  updateOperationalCollectionSecondaryIndexes(name: string, secondaryIndexes: unknown, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord {
    return this.#run("admin.update_operational_collection_secondary_indexes", () => operationalCollectionRecordFromWire(parseNativeJson(callNative(() => this.#core.updateOperationalCollectionSecondaryIndexes(name, JSON.stringify(secondaryIndexes))))), progressCallback, feedbackConfig);
  }

  // ── Operational collection operations ─────────────────────────────

  /**
   * Return mutation and current-state rows for an operational collection.
   *
   * @param collectionName - Name of the operational collection to trace.
   * @param recordKey - Optional key to narrow the trace to a single record.
   */
  traceOperationalCollection(collectionName: string, recordKey?: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalTraceReport {
    return this.#run("admin.trace_operational_collection", () => operationalTraceReportFromWire(parseNativeJson(callNative(() => this.#core.traceOperationalCollection(collectionName, recordKey)))), progressCallback, feedbackConfig);
  }

  /**
   * Read filtered mutation rows from an operational collection.
   *
   * @param request - Read request specifying collection, filters, and pagination.
   */
  readOperationalCollection(request: OperationalReadRequest, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalReadReport {
    return this.#run("admin.read_operational_collection", () => operationalReadReportFromWire(parseNativeJson(callNative(() => this.#core.readOperationalCollection(JSON.stringify(operationalReadRequestToWire(request)))))), progressCallback, feedbackConfig);
  }

  /**
   * Rebuild the current-state view for one or all operational collections.
   *
   * @param collectionName - Limit to this collection, or omit for all.
   */
  rebuildOperationalCurrent(collectionName?: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalRepairReport {
    return this.#run("admin.rebuild_operational_current", () => operationalRepairReportFromWire(parseNativeJson(callNative(() => this.#core.rebuildOperationalCurrent(collectionName)))), progressCallback, feedbackConfig);
  }

  /**
   * Validate the mutation history of an operational collection for consistency.
   *
   * @param collectionName - Name of the collection to validate.
   */
  validateOperationalCollectionHistory(collectionName: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalHistoryValidationReport {
    return this.#run("admin.validate_operational_collection_history", () => operationalHistoryValidationReportFromWire(parseNativeJson(callNative(() => this.#core.validateOperationalCollectionHistory(collectionName)))), progressCallback, feedbackConfig);
  }

  /**
   * Rebuild secondary indexes for an operational collection.
   *
   * @param collectionName - Name of the collection whose indexes should be rebuilt.
   */
  rebuildOperationalSecondaryIndexes(collectionName: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalSecondaryIndexRebuildReport {
    return this.#run("admin.rebuild_operational_secondary_indexes", () => operationalSecondaryIndexRebuildReportFromWire(parseNativeJson(callNative(() => this.#core.rebuildOperationalSecondaryIndexes(collectionName)))), progressCallback, feedbackConfig);
  }

  // ── Retention & cleanup ───────────────────────────────────────────

  /**
   * Preview which mutations would be purged by the retention policy.
   *
   * @param nowTimestamp - Reference timestamp (epoch seconds) for retention evaluation.
   * @param options - Optional limits on which collections to evaluate.
   */
  planOperationalRetention(nowTimestamp: number, options: { collectionNames?: string[]; maxCollections?: number } = {}, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalRetentionPlanReport {
    const namesJson = options.collectionNames ? JSON.stringify(options.collectionNames) : undefined;
    return this.#run("admin.plan_operational_retention", () => operationalRetentionPlanReportFromWire(parseNativeJson(callNative(() => this.#core.planOperationalRetention(nowTimestamp, namesJson, options.maxCollections)))), progressCallback, feedbackConfig);
  }

  /**
   * Execute the retention policy, deleting expired mutations.
   *
   * @param nowTimestamp - Reference timestamp (epoch seconds) for retention evaluation.
   * @param options - Optional limits and dry-run flag.
   */
  runOperationalRetention(nowTimestamp: number, options: { collectionNames?: string[]; maxCollections?: number; dryRun?: boolean } = {}, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalRetentionRunReport {
    const namesJson = options.collectionNames ? JSON.stringify(options.collectionNames) : undefined;
    return this.#run("admin.run_operational_retention", () => operationalRetentionRunReportFromWire(parseNativeJson(callNative(() => this.#core.runOperationalRetention(nowTimestamp, namesJson, options.maxCollections, options.dryRun)))), progressCallback, feedbackConfig);
  }

  /**
   * Compact an operational collection by removing superseded mutations.
   *
   * @param name - Name of the operational collection.
   * @param dryRun - If `true`, report what would be compacted without modifying data.
   */
  compactOperationalCollection(name: string, dryRun: boolean, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCompactionReport {
    return this.#run("admin.compact_operational_collection", () => operationalCompactionReportFromWire(parseNativeJson(callNative(() => this.#core.compactOperationalCollection(name, dryRun)))), progressCallback, feedbackConfig);
  }

  /**
   * Delete all mutations older than `beforeTimestamp` from a collection.
   *
   * @param name - Name of the operational collection.
   * @param beforeTimestamp - Epoch-seconds cutoff; mutations before this are deleted.
   */
  purgeOperationalCollection(name: string, beforeTimestamp: number, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalPurgeReport {
    return this.#run("admin.purge_operational_collection", () => operationalPurgeReportFromWire(parseNativeJson(callNative(() => this.#core.purgeOperationalCollection(name, beforeTimestamp)))), progressCallback, feedbackConfig);
  }

  /**
   * Delete provenance events older than `beforeTimestamp`.
   *
   * @param beforeTimestamp - Epoch-seconds cutoff; events before this are deleted.
   * @param options - Optional flags such as `dryRun` and `preserveEventTypes`.
   */
  purgeProvenanceEvents(beforeTimestamp: number, options: Record<string, unknown> = {}, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): ProvenancePurgeReport {
    return this.#run("admin.purge_provenance_events", () => provenancePurgeReportFromWire(parseNativeJson(callNative(() => this.#core.purgeProvenanceEvents(beforeTimestamp, JSON.stringify(options))))), progressCallback, feedbackConfig);
  }
}
