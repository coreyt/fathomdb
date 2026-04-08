import { parseNativeJson } from "./errors.js";
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

  // ── Integrity & validation ────────────────────────────────────────

  checkIntegrity(): IntegrityReport {
    return integrityReportFromWire(parseNativeJson(this.#core.checkIntegrity()));
  }

  checkSemantics(): SemanticReport {
    return semanticReportFromWire(parseNativeJson(this.#core.checkSemantics()));
  }

  // ── Projection maintenance ────────────────────────────────────────

  rebuild(target: ProjectionTarget = "all"): ProjectionRepairReport {
    return projectionRepairReportFromWire(parseNativeJson(this.#core.rebuildProjections(target)));
  }

  rebuildMissing(): ProjectionRepairReport {
    return projectionRepairReportFromWire(parseNativeJson(this.#core.rebuildMissingProjections()));
  }

  // ── Source tracing & excision ─────────────────────────────────────

  traceSource(sourceRef: string): TraceReport {
    return traceReportFromWire(parseNativeJson(this.#core.traceSource(sourceRef)));
  }

  exciseSource(sourceRef: string): TraceReport {
    return traceReportFromWire(parseNativeJson(this.#core.exciseSource(sourceRef)));
  }

  // ── Logical ID management ─────────────────────────────────────────

  restoreLogicalId(logicalId: string): LogicalRestoreReport {
    return logicalRestoreReportFromWire(parseNativeJson(this.#core.restoreLogicalId(logicalId)));
  }

  purgeLogicalId(logicalId: string): LogicalPurgeReport {
    return logicalPurgeReportFromWire(parseNativeJson(this.#core.purgeLogicalId(logicalId)));
  }

  // ── Safe export ───────────────────────────────────────────────────

  safeExport(destinationPath: string, options: { forceCheckpoint?: boolean } = {}): SafeExportManifest {
    return safeExportManifestFromWire(parseNativeJson(this.#core.safeExport(destinationPath, options.forceCheckpoint ?? true)));
  }

  // ── Operational collection lifecycle ──────────────────────────────

  registerOperationalCollection(request: OperationalRegisterRequest): OperationalCollectionRecord {
    return operationalCollectionRecordFromWire(parseNativeJson(this.#core.registerOperationalCollection(JSON.stringify(operationalRegisterRequestToWire(request)))));
  }

  describeOperationalCollection(name: string): OperationalCollectionRecord | null {
    const json = this.#core.describeOperationalCollection(name);
    const raw = JSON.parse(json);
    if (raw === null || raw.name == null) return null;
    return operationalCollectionRecordFromWire(raw as Record<string, unknown>);
  }

  disableOperationalCollection(name: string): OperationalCollectionRecord {
    return operationalCollectionRecordFromWire(parseNativeJson(this.#core.disableOperationalCollection(name)));
  }

  // ── Operational collection config ─────────────────────────────────

  updateOperationalCollectionFilters(name: string, filterFields: unknown): OperationalCollectionRecord {
    return operationalCollectionRecordFromWire(parseNativeJson(this.#core.updateOperationalCollectionFilters(name, JSON.stringify(filterFields))));
  }

  updateOperationalCollectionValidation(name: string, validation: unknown): OperationalCollectionRecord {
    return operationalCollectionRecordFromWire(parseNativeJson(this.#core.updateOperationalCollectionValidation(name, JSON.stringify(validation))));
  }

  updateOperationalCollectionSecondaryIndexes(name: string, secondaryIndexes: unknown): OperationalCollectionRecord {
    return operationalCollectionRecordFromWire(parseNativeJson(this.#core.updateOperationalCollectionSecondaryIndexes(name, JSON.stringify(secondaryIndexes))));
  }

  // ── Operational collection operations ─────────────────────────────

  traceOperationalCollection(collectionName: string, recordKey?: string): OperationalTraceReport {
    return operationalTraceReportFromWire(parseNativeJson(this.#core.traceOperationalCollection(collectionName, recordKey)));
  }

  readOperationalCollection(request: OperationalReadRequest): OperationalReadReport {
    return operationalReadReportFromWire(parseNativeJson(this.#core.readOperationalCollection(JSON.stringify(operationalReadRequestToWire(request)))));
  }

  rebuildOperationalCurrent(collectionName?: string): OperationalRepairReport {
    return operationalRepairReportFromWire(parseNativeJson(this.#core.rebuildOperationalCurrent(collectionName)));
  }

  validateOperationalCollectionHistory(collectionName: string): OperationalHistoryValidationReport {
    return operationalHistoryValidationReportFromWire(parseNativeJson(this.#core.validateOperationalCollectionHistory(collectionName)));
  }

  rebuildOperationalSecondaryIndexes(collectionName: string): OperationalSecondaryIndexRebuildReport {
    return operationalSecondaryIndexRebuildReportFromWire(parseNativeJson(this.#core.rebuildOperationalSecondaryIndexes(collectionName)));
  }

  // ── Retention & cleanup ───────────────────────────────────────────

  planOperationalRetention(
    nowTimestamp: number,
    options: { collectionNames?: string[]; maxCollections?: number } = {}
  ): OperationalRetentionPlanReport {
    const namesJson = options.collectionNames ? JSON.stringify(options.collectionNames) : undefined;
    return operationalRetentionPlanReportFromWire(parseNativeJson(this.#core.planOperationalRetention(nowTimestamp, namesJson, options.maxCollections)));
  }

  runOperationalRetention(
    nowTimestamp: number,
    options: { collectionNames?: string[]; maxCollections?: number; dryRun?: boolean } = {}
  ): OperationalRetentionRunReport {
    const namesJson = options.collectionNames ? JSON.stringify(options.collectionNames) : undefined;
    return operationalRetentionRunReportFromWire(parseNativeJson(this.#core.runOperationalRetention(nowTimestamp, namesJson, options.maxCollections, options.dryRun)));
  }

  compactOperationalCollection(name: string, dryRun: boolean): OperationalCompactionReport {
    return operationalCompactionReportFromWire(parseNativeJson(this.#core.compactOperationalCollection(name, dryRun)));
  }

  purgeOperationalCollection(name: string, beforeTimestamp: number): OperationalPurgeReport {
    return operationalPurgeReportFromWire(parseNativeJson(this.#core.purgeOperationalCollection(name, beforeTimestamp)));
  }

  purgeProvenanceEvents(beforeTimestamp: number, options: Record<string, unknown> = {}): ProvenancePurgeReport {
    return provenancePurgeReportFromWire(parseNativeJson(this.#core.purgeProvenanceEvents(beforeTimestamp, JSON.stringify(options))));
  }

}
