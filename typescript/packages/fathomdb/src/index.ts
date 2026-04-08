import { loadNativeBinding } from "./native.js";

export { AdminClient } from "./admin.js";
export { Engine } from "./engine.js";
export {
  BridgeError,
  BuilderValidationError,
  CapabilityMissingError,
  CompileError,
  DatabaseLockedError,
  FathomError,
  InvalidWriteError,
  IoError,
  SchemaError,
  SqliteError,
  WriterRejectedError
} from "./errors.js";
export { Query } from "./query.js";
export {
  PreserializedJson,
  // Enums / constants
  type ChunkPolicy,
  type DrivingTable,
  type OperationalCollectionKind,
  type OperationalFilterMode,
  type ProjectionTarget,
  type ProvenanceMode,
  type TelemetryLevel,
  type TraverseDirection,
  // Feedback / progress callbacks
  type FeedbackConfig,
  type ProgressCallback,
  type ResponseCycleEvent,
  type ResponseCyclePhase,
  // Engine options
  type EngineOpenOptions,
  // Row types
  type ActionRow,
  type NodeRow,
  type RunRow,
  type StepRow,
  // Query result types
  type BindValue,
  type CompiledGroupedQuery,
  type CompiledQuery,
  type ExecutionHints,
  type ExpansionRootRows,
  type ExpansionSlot,
  type ExpansionSlotRows,
  type GroupedQueryRows,
  type QueryPlan,
  type QueryRows,
  type RawJson,
  // Write types
  type LastAccessTouchReport,
  type LastAccessTouchRequest,
  type WriteReceipt,
  type WriteRequest,
  // Write input types
  type NodeInsertInput,
  type EdgeInsertInput,
  type ChunkInsertInput,
  type RunInsertInput,
  type StepInsertInput,
  type ActionInsertInput,
  type OperationalAppendInput,
  type OperationalPutInput,
  type OperationalDeleteInput,
  // Admin report types
  type IntegrityReport,
  type LogicalPurgeReport,
  type LogicalRestoreReport,
  type ProjectionRepairReport,
  type ProvenancePurgeReport,
  type SafeExportManifest,
  type SemanticReport,
  type SkippedEdge,
  type TelemetrySnapshot,
  type TraceReport,
  // Operational collection types
  type OperationalCollectionRecord,
  type OperationalCompactionReport,
  type OperationalCurrentRow,
  type OperationalFilterClause,
  type OperationalHistoryValidationIssue,
  type OperationalHistoryValidationReport,
  type OperationalMutationRow,
  type OperationalPurgeReport,
  type OperationalReadReport,
  type OperationalReadRequest,
  type OperationalRegisterRequest,
  type OperationalRepairReport,
  type OperationalRetentionPlanItem,
  type OperationalRetentionPlanReport,
  type OperationalRetentionRunItem,
  type OperationalRetentionRunReport,
  type OperationalSecondaryIndexRebuildReport,
  type OperationalTraceReport,
} from "./types.js";
export {
  type ActionHandle,
  type ChunkHandle,
  type EdgeHandle,
  type NodeHandle,
  type RunHandle,
  type StepHandle,
  WriteRequestBuilder
} from "./write-builder.js";
export { loadNativeBinding } from "./native.js";
export function newId(): string {
  return loadNativeBinding().newId();
}
export function newRowId(): string {
  return loadNativeBinding().newRowId();
}
