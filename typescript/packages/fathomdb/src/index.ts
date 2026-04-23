import { loadNativeBinding } from "./native.js";

export {
  type QueryEmbedder,
  OpenAIEmbedder,
  JinaEmbedder,
  StellaEmbedder,
  SubprocessEmbedder,
} from "./embedders/index.js";

export { AdminClient, TOKENIZER_PRESETS } from "./admin.js";
export { Engine } from "./engine.js";
export {
  BridgeError,
  BuilderValidationError,
  CapabilityMissingError,
  CompileError,
  DatabaseLockedError,
  DimensionMismatchError,
  FathomError,
  InvalidWriteError,
  IoError,
  KindNotVectorIndexedError,
  RebuildImpactError,
  SchemaError,
  SqliteError,
  WriterRejectedError,
  WriterTimedOutError
} from "./errors.js";
export { FallbackSearchBuilder, Query, SearchBuilder, TextSearchBuilder } from "./query.js";
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
  type EdgeRow,
  type EdgeExpansionPair,
  type EdgeExpansionRootRows,
  type EdgeExpansionSlotRows,
  type ExpansionRootRows,
  type ExpansionSlot,
  type ExpansionSlotRows,
  type GroupedQueryRows,
  type HitAttribution,
  type QueryPlan,
  type QueryRows,
  type RawJson,
  type RetrievalModality,
  type SearchHit,
  type SearchHitSource,
  type SearchMatchMode,
  type SearchRows,
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
  // FTS property schema types
  type FtsPropertyPathMode,
  type FtsPropertyPathSpec,
  type FtsPropertySchemaRecord,
  type RebuildProgress,
  // Projection profile types
  type FtsProfile,
  type VecProfile,
  type ProjectionImpactReport,
  type VecIdentity,
  type VectorRegenerationConfig,
  type VectorRegenerationReport,
  type DrainReport,
  // Pack H: introspection types
  type Capabilities,
  type ConfigureEmbeddingOutcome,
  type ConfigureEmbeddingRequest,
  type ConfigureVecKindsItem,
  type ConfigureVecOutcome,
  type CurrentConfig,
  type EmbedderCapability,
  type EmbeddingProfileSummary,
  type FtsKindConfig,
  type KindDescription,
  type VecKindConfig,
  type WorkQueueSummary,
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
