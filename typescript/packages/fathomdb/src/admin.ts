import { parseNativeJson } from "./errors.js";
import { runWithFeedback } from "./feedback.js";
import type { NativeEngineCore } from "./native.js";
import {
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

export class AdminClient {
  readonly #core: NativeEngineCore;

  constructor(core: NativeEngineCore) {
    this.#core = core;
  }

  #run<T>(operationKind: string, operation: () => T, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): T {
    return runWithFeedback({ operationKind, metadata: {}, progressCallback, feedbackConfig, operation });
  }

  // ── Integrity & validation ────────────────────────────────────────

  checkIntegrity(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): IntegrityReport {
    return this.#run("admin.check_integrity", () => integrityReportFromWire(parseNativeJson(this.#core.checkIntegrity())), progressCallback, feedbackConfig);
  }

  checkSemantics(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): SemanticReport {
    return this.#run("admin.check_semantics", () => semanticReportFromWire(parseNativeJson(this.#core.checkSemantics())), progressCallback, feedbackConfig);
  }

  // ── Projection maintenance ────────────────────────────────────────

  rebuild(target: ProjectionTarget = "all", progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): ProjectionRepairReport {
    return this.#run("admin.rebuild", () => projectionRepairReportFromWire(parseNativeJson(this.#core.rebuildProjections(target))), progressCallback, feedbackConfig);
  }

  rebuildMissing(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): ProjectionRepairReport {
    return this.#run("admin.rebuild_missing", () => projectionRepairReportFromWire(parseNativeJson(this.#core.rebuildMissingProjections())), progressCallback, feedbackConfig);
  }

  // ── Source tracing & excision ─────────────────────────────────────

  traceSource(sourceRef: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): TraceReport {
    return this.#run("admin.trace_source", () => traceReportFromWire(parseNativeJson(this.#core.traceSource(sourceRef))), progressCallback, feedbackConfig);
  }

  exciseSource(sourceRef: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): TraceReport {
    return this.#run("admin.excise_source", () => traceReportFromWire(parseNativeJson(this.#core.exciseSource(sourceRef))), progressCallback, feedbackConfig);
  }

  // ── Logical ID management ─────────────────────────────────────────

  restoreLogicalId(logicalId: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): LogicalRestoreReport {
    return this.#run("admin.restore_logical_id", () => logicalRestoreReportFromWire(parseNativeJson(this.#core.restoreLogicalId(logicalId))), progressCallback, feedbackConfig);
  }

  purgeLogicalId(logicalId: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): LogicalPurgeReport {
    return this.#run("admin.purge_logical_id", () => logicalPurgeReportFromWire(parseNativeJson(this.#core.purgeLogicalId(logicalId))), progressCallback, feedbackConfig);
  }

  // ── Safe export ───────────────────────────────────────────────────

  safeExport(destinationPath: string, options: { forceCheckpoint?: boolean } = {}, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): SafeExportManifest {
    return this.#run("admin.safe_export", () => safeExportManifestFromWire(parseNativeJson(this.#core.safeExport(destinationPath, options.forceCheckpoint ?? true))), progressCallback, feedbackConfig);
  }

  // ── Operational collection lifecycle ──────────────────────────────

  registerOperationalCollection(request: OperationalRegisterRequest, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord {
    return this.#run("admin.register_operational_collection", () => operationalCollectionRecordFromWire(parseNativeJson(this.#core.registerOperationalCollection(JSON.stringify(operationalRegisterRequestToWire(request))))), progressCallback, feedbackConfig);
  }

  describeOperationalCollection(name: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord | null {
    return this.#run("admin.describe_operational_collection", () => {
      const json = this.#core.describeOperationalCollection(name);
      const raw = JSON.parse(json);
      if (raw === null || raw.name == null) return null;
      return operationalCollectionRecordFromWire(raw as Record<string, unknown>);
    }, progressCallback, feedbackConfig);
  }

  disableOperationalCollection(name: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord {
    return this.#run("admin.disable_operational_collection", () => operationalCollectionRecordFromWire(parseNativeJson(this.#core.disableOperationalCollection(name))), progressCallback, feedbackConfig);
  }

  // ── Operational collection config ─────────────────────────────────

  updateOperationalCollectionFilters(name: string, filterFields: unknown, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord {
    return this.#run("admin.update_operational_collection_filters", () => operationalCollectionRecordFromWire(parseNativeJson(this.#core.updateOperationalCollectionFilters(name, JSON.stringify(filterFields)))), progressCallback, feedbackConfig);
  }

  updateOperationalCollectionValidation(name: string, validation: unknown, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord {
    return this.#run("admin.update_operational_collection_validation", () => operationalCollectionRecordFromWire(parseNativeJson(this.#core.updateOperationalCollectionValidation(name, JSON.stringify(validation)))), progressCallback, feedbackConfig);
  }

  updateOperationalCollectionSecondaryIndexes(name: string, secondaryIndexes: unknown, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCollectionRecord {
    return this.#run("admin.update_operational_collection_secondary_indexes", () => operationalCollectionRecordFromWire(parseNativeJson(this.#core.updateOperationalCollectionSecondaryIndexes(name, JSON.stringify(secondaryIndexes)))), progressCallback, feedbackConfig);
  }

  // ── Operational collection operations ─────────────────────────────

  traceOperationalCollection(collectionName: string, recordKey?: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalTraceReport {
    return this.#run("admin.trace_operational_collection", () => operationalTraceReportFromWire(parseNativeJson(this.#core.traceOperationalCollection(collectionName, recordKey))), progressCallback, feedbackConfig);
  }

  readOperationalCollection(request: OperationalReadRequest, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalReadReport {
    return this.#run("admin.read_operational_collection", () => operationalReadReportFromWire(parseNativeJson(this.#core.readOperationalCollection(JSON.stringify(operationalReadRequestToWire(request))))), progressCallback, feedbackConfig);
  }

  rebuildOperationalCurrent(collectionName?: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalRepairReport {
    return this.#run("admin.rebuild_operational_current", () => operationalRepairReportFromWire(parseNativeJson(this.#core.rebuildOperationalCurrent(collectionName))), progressCallback, feedbackConfig);
  }

  validateOperationalCollectionHistory(collectionName: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalHistoryValidationReport {
    return this.#run("admin.validate_operational_collection_history", () => operationalHistoryValidationReportFromWire(parseNativeJson(this.#core.validateOperationalCollectionHistory(collectionName))), progressCallback, feedbackConfig);
  }

  rebuildOperationalSecondaryIndexes(collectionName: string, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalSecondaryIndexRebuildReport {
    return this.#run("admin.rebuild_operational_secondary_indexes", () => operationalSecondaryIndexRebuildReportFromWire(parseNativeJson(this.#core.rebuildOperationalSecondaryIndexes(collectionName))), progressCallback, feedbackConfig);
  }

  // ── Retention & cleanup ───────────────────────────────────────────

  planOperationalRetention(nowTimestamp: number, options: { collectionNames?: string[]; maxCollections?: number } = {}, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalRetentionPlanReport {
    const namesJson = options.collectionNames ? JSON.stringify(options.collectionNames) : undefined;
    return this.#run("admin.plan_operational_retention", () => operationalRetentionPlanReportFromWire(parseNativeJson(this.#core.planOperationalRetention(nowTimestamp, namesJson, options.maxCollections))), progressCallback, feedbackConfig);
  }

  runOperationalRetention(nowTimestamp: number, options: { collectionNames?: string[]; maxCollections?: number; dryRun?: boolean } = {}, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalRetentionRunReport {
    const namesJson = options.collectionNames ? JSON.stringify(options.collectionNames) : undefined;
    return this.#run("admin.run_operational_retention", () => operationalRetentionRunReportFromWire(parseNativeJson(this.#core.runOperationalRetention(nowTimestamp, namesJson, options.maxCollections, options.dryRun))), progressCallback, feedbackConfig);
  }

  compactOperationalCollection(name: string, dryRun: boolean, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalCompactionReport {
    return this.#run("admin.compact_operational_collection", () => operationalCompactionReportFromWire(parseNativeJson(this.#core.compactOperationalCollection(name, dryRun))), progressCallback, feedbackConfig);
  }

  purgeOperationalCollection(name: string, beforeTimestamp: number, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): OperationalPurgeReport {
    return this.#run("admin.purge_operational_collection", () => operationalPurgeReportFromWire(parseNativeJson(this.#core.purgeOperationalCollection(name, beforeTimestamp))), progressCallback, feedbackConfig);
  }

  purgeProvenanceEvents(beforeTimestamp: number, options: Record<string, unknown> = {}, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): ProvenancePurgeReport {
    return this.#run("admin.purge_provenance_events", () => provenancePurgeReportFromWire(parseNativeJson(this.#core.purgeProvenanceEvents(beforeTimestamp, JSON.stringify(options)))), progressCallback, feedbackConfig);
  }
}
