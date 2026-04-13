// ── Utility ────────────────────────────────────────────────────────────

export type RawJson =
  | null
  | boolean
  | number
  | string
  | RawJson[]
  | { [key: string]: RawJson };

/**
 * Wrapper for pre-serialized JSON strings that should be passed through
 * to the native layer without additional encoding.
 *
 * Use this when you have already-serialized JSON and want to avoid
 * double-encoding.
 */
export class PreserializedJson {
  constructor(public readonly json: string) {}
}

// ── Engine options ─────────────────────────────────────────────────────

export type EngineOpenOptions = {
  provenanceMode?: "warn" | "require";
  vectorDimension?: number;
  telemetryLevel?: "counters" | "statements" | "profiling";
};

// ── Feedback / progress callbacks ───────────────────────────────────

export type ResponseCyclePhase = "started" | "slow" | "heartbeat" | "finished" | "failed";

export type FeedbackConfig = {
  /** Milliseconds before a SLOW event is emitted (default 500). */
  slowThresholdMs?: number;
  /** Milliseconds between HEARTBEAT events after SLOW (default 2000). */
  heartbeatIntervalMs?: number;
};

export type ResponseCycleEvent = {
  operationId: string;
  operationKind: string;
  surface: string;
  phase: ResponseCyclePhase;
  elapsedMs: number;
  slowThresholdMs: number;
  metadata: Record<string, string>;
  errorCode?: string;
  errorMessage?: string;
};

export type ProgressCallback = (event: ResponseCycleEvent) => void;

// ── Query AST (internal) ───────────────────────────────────────────────

export type QueryAst = {
  root_kind: string;
  steps: Array<Record<string, RawJson>>;
  expansions: Array<Record<string, RawJson>>;
  final_limit: number | null;
};

// ── Telemetry ──────────────────────────────────────────────────────────

export type TelemetrySnapshot = {
  queriesTotal: number;
  writesTotal: number;
  writeRowsTotal: number;
  errorsTotal: number;
  adminOpsTotal: number;
  cacheHits: number;
  cacheMisses: number;
  cacheWrites: number;
  cacheSpills: number;
};

export function telemetrySnapshotFromWire(w: Record<string, unknown>): TelemetrySnapshot {
  return {
    queriesTotal: Number(w.queries_total ?? 0),
    writesTotal: Number(w.writes_total ?? 0),
    writeRowsTotal: Number(w.write_rows_total ?? 0),
    errorsTotal: Number(w.errors_total ?? 0),
    adminOpsTotal: Number(w.admin_ops_total ?? 0),
    cacheHits: Number(w.cache_hits ?? 0),
    cacheMisses: Number(w.cache_misses ?? 0),
    cacheWrites: Number(w.cache_writes ?? 0),
    cacheSpills: Number(w.cache_spills ?? 0),
  };
}

// ── Row types ──────────────────────────────────────────────────────────

export type NodeRow = {
  rowId: string;
  logicalId: string;
  kind: string;
  properties: unknown;
  contentRef: string | null;
  lastAccessedAt: number | null;
};

function nodeRowFromWire(w: Record<string, unknown>): NodeRow {
  return {
    rowId: String(w.row_id ?? ""),
    logicalId: String(w.logical_id ?? ""),
    kind: String(w.kind ?? ""),
    properties: parseJsonField(w.properties),
    contentRef: (w.content_ref as string) ?? null,
    lastAccessedAt: w.last_accessed_at != null ? Number(w.last_accessed_at) : null,
  };
}

export type RunRow = {
  id: string;
  kind: string;
  status: string;
  properties: unknown;
};

function runRowFromWire(w: Record<string, unknown>): RunRow {
  return {
    id: String(w.id ?? ""),
    kind: String(w.kind ?? ""),
    status: String(w.status ?? ""),
    properties: parseJsonField(w.properties),
  };
}

export type StepRow = {
  id: string;
  runId: string;
  kind: string;
  status: string;
  properties: unknown;
};

function stepRowFromWire(w: Record<string, unknown>): StepRow {
  return {
    id: String(w.id ?? ""),
    runId: String(w.run_id ?? ""),
    kind: String(w.kind ?? ""),
    status: String(w.status ?? ""),
    properties: parseJsonField(w.properties),
  };
}

export type ActionRow = {
  id: string;
  stepId: string;
  kind: string;
  status: string;
  properties: unknown;
};

function actionRowFromWire(w: Record<string, unknown>): ActionRow {
  return {
    id: String(w.id ?? ""),
    stepId: String(w.step_id ?? ""),
    kind: String(w.kind ?? ""),
    status: String(w.status ?? ""),
    properties: parseJsonField(w.properties),
  };
}

// ── Query results ──────────────────────────────────────────────────────

export type QueryRows = {
  nodes: NodeRow[];
  runs: RunRow[];
  steps: StepRow[];
  actions: ActionRow[];
  wasDegraded: boolean;
};

export function queryRowsFromWire(w: Record<string, unknown>): QueryRows {
  return {
    nodes: asArray(w.nodes).map(nodeRowFromWire),
    runs: asArray(w.runs).map(runRowFromWire),
    steps: asArray(w.steps).map(stepRowFromWire),
    actions: asArray(w.actions).map(actionRowFromWire),
    wasDegraded: Boolean(w.was_degraded),
  };
}

// ── Search results ─────────────────────────────────────────────────────

export type SearchHitSource = "chunk" | "property" | "vector";

export type SearchMatchMode = "strict" | "relaxed";

export type HitAttribution = {
  matchedPaths: string[];
};

export type SearchHit = {
  node: NodeRow;
  score: number;
  source: SearchHitSource;
  matchMode: SearchMatchMode;
  snippet: string | null;
  /**
   * Seconds since the Unix epoch (1970-01-01 UTC), matching
   * `nodes.created_at` which is populated via SQLite `unixepoch()`.
   */
  writtenAt: number;
  projectionRowId: string | null;
  attribution: HitAttribution | null;
};

export type SearchRows = {
  hits: SearchHit[];
  wasDegraded: boolean;
  fallbackUsed: boolean;
  strictHitCount: number;
  relaxedHitCount: number;
};

function hitAttributionFromWire(w: Record<string, unknown>): HitAttribution {
  return {
    matchedPaths: asStringArray(w.matched_paths),
  };
}

function searchHitFromWire(w: Record<string, unknown>): SearchHit {
  const rawAttribution = w.attribution;
  const attribution =
    rawAttribution != null && typeof rawAttribution === "object"
      ? hitAttributionFromWire(rawAttribution as Record<string, unknown>)
      : null;
  return {
    node: nodeRowFromWire(asObj(w.node)),
    score: Number(w.score ?? 0),
    source: String(w.source ?? "chunk") as SearchHitSource,
    matchMode: String(w.match_mode ?? "strict") as SearchMatchMode,
    snippet: (w.snippet as string) ?? null,
    writtenAt: Number(w.written_at ?? 0),
    projectionRowId: (w.projection_row_id as string) ?? null,
    attribution,
  };
}

export function searchRowsFromWire(w: Record<string, unknown>): SearchRows {
  return {
    hits: asArray(w.hits).map(searchHitFromWire),
    wasDegraded: Boolean(w.was_degraded),
    fallbackUsed: Boolean(w.fallback_used),
    strictHitCount: Number(w.strict_hit_count ?? 0),
    relaxedHitCount: Number(w.relaxed_hit_count ?? 0),
  };
}

export type ExpansionRootRows = {
  rootLogicalId: string;
  nodes: NodeRow[];
};

function expansionRootRowsFromWire(w: Record<string, unknown>): ExpansionRootRows {
  return {
    rootLogicalId: String(w.root_logical_id ?? ""),
    nodes: asArray(w.nodes).map(nodeRowFromWire),
  };
}

export type ExpansionSlotRows = {
  slot: string;
  roots: ExpansionRootRows[];
};

function expansionSlotRowsFromWire(w: Record<string, unknown>): ExpansionSlotRows {
  return {
    slot: String(w.slot ?? ""),
    roots: asArray(w.roots).map(expansionRootRowsFromWire),
  };
}

export type GroupedQueryRows = {
  roots: NodeRow[];
  expansions: ExpansionSlotRows[];
  wasDegraded: boolean;
};

export function groupedQueryRowsFromWire(w: Record<string, unknown>): GroupedQueryRows {
  return {
    roots: asArray(w.roots).map(nodeRowFromWire),
    expansions: asArray(w.expansions).map(expansionSlotRowsFromWire),
    wasDegraded: Boolean(w.was_degraded),
  };
}

// ── Compiled queries ───────────────────────────────────────────────────

export type DrivingTable = "nodes" | "fts_nodes" | "vec_nodes";

export type BindValue =
  | { type: "text"; value: string }
  | { type: "integer"; value: number }
  | { type: "bool"; value: boolean };

export type ExecutionHints = {
  recursionLimit: number;
  hardLimit: number;
};

function executionHintsFromWire(w: Record<string, unknown>): ExecutionHints {
  return {
    recursionLimit: Number(w.recursion_limit ?? 0),
    hardLimit: Number(w.hard_limit ?? 0),
  };
}

export type CompiledQuery = {
  sql: string;
  binds: BindValue[];
  shapeHash: number;
  drivingTable: DrivingTable;
  hints: ExecutionHints;
};

export function compiledQueryFromWire(w: Record<string, unknown>): CompiledQuery {
  return {
    sql: String(w.sql ?? ""),
    binds: asArray(w.binds) as BindValue[],
    shapeHash: Number(w.shape_hash ?? 0),
    drivingTable: String(w.driving_table ?? "nodes") as DrivingTable,
    hints: executionHintsFromWire(asObj(w.hints)),
  };
}

export type ExpansionSlot = {
  slot: string;
  direction: "in" | "out";
  label: string;
  maxDepth: number;
};

function expansionSlotFromWire(w: Record<string, unknown>): ExpansionSlot {
  return {
    slot: String(w.slot ?? ""),
    direction: String(w.direction ?? "out") as "in" | "out",
    label: String(w.label ?? ""),
    maxDepth: Number(w.max_depth ?? 1),
  };
}

export type CompiledGroupedQuery = {
  root: CompiledQuery;
  expansions: ExpansionSlot[];
  shapeHash: number;
  hints: ExecutionHints;
};

export function compiledGroupedQueryFromWire(w: Record<string, unknown>): CompiledGroupedQuery {
  return {
    root: compiledQueryFromWire(asObj(w.root)),
    expansions: asArray(w.expansions).map(expansionSlotFromWire),
    shapeHash: Number(w.shape_hash ?? 0),
    hints: executionHintsFromWire(asObj(w.hints)),
  };
}

export type QueryPlan = {
  sql: string;
  bindCount: number;
  drivingTable: DrivingTable;
  shapeHash: number;
  cacheHit: boolean;
};

export function queryPlanFromWire(w: Record<string, unknown>): QueryPlan {
  return {
    sql: String(w.sql ?? ""),
    bindCount: Number(w.bind_count ?? 0),
    drivingTable: String(w.driving_table ?? "nodes") as DrivingTable,
    shapeHash: Number(w.shape_hash ?? 0),
    cacheHit: Boolean(w.cache_hit),
  };
}

// ── Write results ──────────────────────────────────────────────────────

export type WriteReceipt = {
  label: string;
  optionalBackfillCount: number;
  warnings: string[];
  provenanceWarnings: string[];
};

export function writeReceiptFromWire(w: Record<string, unknown>): WriteReceipt {
  return {
    label: String(w.label ?? ""),
    optionalBackfillCount: Number(w.optional_backfill_count ?? 0),
    warnings: asStringArray(w.warnings),
    provenanceWarnings: asStringArray(w.provenance_warnings),
  };
}

// Input types — accept snake_case wire format from WriteRequestBuilder.build()
// Phase 4 will add typed camelCase input interfaces with automatic conversion.
export type WriteRequest = Record<string, unknown>;

export type LastAccessTouchRequest = {
  logicalIds: string[];
  touchedAt: number;
  sourceRef?: string;
};

export type LastAccessTouchReport = {
  touchedLogicalIds: number;
  touchedAt: number;
};

export function lastAccessTouchReportFromWire(w: Record<string, unknown>): LastAccessTouchReport {
  return {
    touchedLogicalIds: Number(w.touched_logical_ids ?? 0),
    touchedAt: Number(w.touched_at ?? 0),
  };
}

// ── Admin reports ──────────────────────────────────────────────────────

export type IntegrityReport = {
  physicalOk: boolean;
  foreignKeysOk: boolean;
  missingFtsRows: number;
  missingPropertyFtsRows: number;
  duplicateActiveLogicalIds: number;
  operationalMissingCollections: number;
  operationalMissingLastMutations: number;
  warnings: string[];
};

export function integrityReportFromWire(w: Record<string, unknown>): IntegrityReport {
  return {
    physicalOk: Boolean(w.physical_ok),
    foreignKeysOk: Boolean(w.foreign_keys_ok),
    missingFtsRows: Number(w.missing_fts_rows ?? 0),
    missingPropertyFtsRows: Number(w.missing_property_fts_rows ?? 0),
    duplicateActiveLogicalIds: Number(w.duplicate_active_logical_ids ?? 0),
    operationalMissingCollections: Number(w.operational_missing_collections ?? 0),
    operationalMissingLastMutations: Number(w.operational_missing_last_mutations ?? 0),
    warnings: asStringArray(w.warnings),
  };
}

export type SemanticReport = {
  orphanedChunks: number;
  nullSourceRefNodes: number;
  brokenStepFk: number;
  brokenActionFk: number;
  staleFtsRows: number;
  ftsRowsForSupersededNodes: number;
  stalePropertyFtsRows: number;
  orphanedPropertyFtsRows: number;
  mismatchedKindPropertyFtsRows: number;
  duplicatePropertyFtsRows: number;
  driftedPropertyFtsRows: number;
  danglingEdges: number;
  orphanedSupersessionChains: number;
  staleVecRows: number;
  vecRowsForSupersededNodes: number;
  missingOperationalCurrentRows: number;
  staleOperationalCurrentRows: number;
  disabledCollectionMutations: number;
  orphanedLastAccessMetadataRows: number;
  warnings: string[];
};

export function semanticReportFromWire(w: Record<string, unknown>): SemanticReport {
  return {
    orphanedChunks: Number(w.orphaned_chunks ?? 0),
    nullSourceRefNodes: Number(w.null_source_ref_nodes ?? 0),
    brokenStepFk: Number(w.broken_step_fk ?? 0),
    brokenActionFk: Number(w.broken_action_fk ?? 0),
    staleFtsRows: Number(w.stale_fts_rows ?? 0),
    ftsRowsForSupersededNodes: Number(w.fts_rows_for_superseded_nodes ?? 0),
    stalePropertyFtsRows: Number(w.stale_property_fts_rows ?? 0),
    orphanedPropertyFtsRows: Number(w.orphaned_property_fts_rows ?? 0),
    mismatchedKindPropertyFtsRows: Number(w.mismatched_kind_property_fts_rows ?? 0),
    duplicatePropertyFtsRows: Number(w.duplicate_property_fts_rows ?? 0),
    driftedPropertyFtsRows: Number(w.drifted_property_fts_rows ?? 0),
    danglingEdges: Number(w.dangling_edges ?? 0),
    orphanedSupersessionChains: Number(w.orphaned_supersession_chains ?? 0),
    staleVecRows: Number(w.stale_vec_rows ?? 0),
    vecRowsForSupersededNodes: Number(w.vec_rows_for_superseded_nodes ?? 0),
    missingOperationalCurrentRows: Number(w.missing_operational_current_rows ?? 0),
    staleOperationalCurrentRows: Number(w.stale_operational_current_rows ?? 0),
    disabledCollectionMutations: Number(w.disabled_collection_mutations ?? 0),
    orphanedLastAccessMetadataRows: Number(w.orphaned_last_access_metadata_rows ?? 0),
    warnings: asStringArray(w.warnings),
  };
}

export type TraceReport = {
  sourceRef: string;
  nodeRows: number;
  edgeRows: number;
  actionRows: number;
  operationalMutationRows: number;
  nodeLogicalIds: string[];
  actionIds: string[];
  operationalMutationIds: string[];
};

export function traceReportFromWire(w: Record<string, unknown>): TraceReport {
  return {
    sourceRef: String(w.source_ref ?? ""),
    nodeRows: Number(w.node_rows ?? 0),
    edgeRows: Number(w.edge_rows ?? 0),
    actionRows: Number(w.action_rows ?? 0),
    operationalMutationRows: Number(w.operational_mutation_rows ?? 0),
    nodeLogicalIds: asStringArray(w.node_logical_ids),
    actionIds: asStringArray(w.action_ids),
    operationalMutationIds: asStringArray(w.operational_mutation_ids),
  };
}

export type SkippedEdge = {
  edgeLogicalId: string;
  missingEndpoint: string;
};

function skippedEdgeFromWire(w: Record<string, unknown>): SkippedEdge {
  return {
    edgeLogicalId: String(w.edge_logical_id ?? ""),
    missingEndpoint: String(w.missing_endpoint ?? ""),
  };
}

export type LogicalRestoreReport = {
  logicalId: string;
  wasNoop: boolean;
  restoredNodeRows: number;
  restoredEdgeRows: number;
  restoredChunkRows: number;
  restoredFtsRows: number;
  restoredPropertyFtsRows: number;
  restoredVecRows: number;
  skippedEdges: SkippedEdge[];
  notes: string[];
};

export function logicalRestoreReportFromWire(w: Record<string, unknown>): LogicalRestoreReport {
  return {
    logicalId: String(w.logical_id ?? ""),
    wasNoop: Boolean(w.was_noop),
    restoredNodeRows: Number(w.restored_node_rows ?? 0),
    restoredEdgeRows: Number(w.restored_edge_rows ?? 0),
    restoredChunkRows: Number(w.restored_chunk_rows ?? 0),
    restoredFtsRows: Number(w.restored_fts_rows ?? 0),
    restoredPropertyFtsRows: Number(w.restored_property_fts_rows ?? 0),
    restoredVecRows: Number(w.restored_vec_rows ?? 0),
    skippedEdges: asArray(w.skipped_edges).map(skippedEdgeFromWire),
    notes: asStringArray(w.notes),
  };
}

export type LogicalPurgeReport = {
  logicalId: string;
  wasNoop: boolean;
  deletedNodeRows: number;
  deletedEdgeRows: number;
  deletedChunkRows: number;
  deletedFtsRows: number;
  deletedVecRows: number;
  notes: string[];
};

export function logicalPurgeReportFromWire(w: Record<string, unknown>): LogicalPurgeReport {
  return {
    logicalId: String(w.logical_id ?? ""),
    wasNoop: Boolean(w.was_noop),
    deletedNodeRows: Number(w.deleted_node_rows ?? 0),
    deletedEdgeRows: Number(w.deleted_edge_rows ?? 0),
    deletedChunkRows: Number(w.deleted_chunk_rows ?? 0),
    deletedFtsRows: Number(w.deleted_fts_rows ?? 0),
    deletedVecRows: Number(w.deleted_vec_rows ?? 0),
    notes: asStringArray(w.notes),
  };
}

export type ProjectionTarget = "fts" | "vec" | "all";

export type ProjectionRepairReport = {
  targets: ProjectionTarget[];
  rebuiltRows: number;
  notes: string[];
};

export function projectionRepairReportFromWire(w: Record<string, unknown>): ProjectionRepairReport {
  return {
    targets: asStringArray(w.targets) as ProjectionTarget[],
    rebuiltRows: Number(w.rebuilt_rows ?? 0),
    notes: asStringArray(w.notes),
  };
}

export type SafeExportManifest = {
  exportedAt: number;
  sha256: string;
  schemaVersion: number;
  protocolVersion: number;
  pageCount: number;
};

export function safeExportManifestFromWire(w: Record<string, unknown>): SafeExportManifest {
  return {
    exportedAt: Number(w.exported_at ?? 0),
    sha256: String(w.sha256 ?? ""),
    schemaVersion: Number(w.schema_version ?? 0),
    protocolVersion: Number(w.protocol_version ?? 0),
    pageCount: Number(w.page_count ?? 0),
  };
}

export type ProvenancePurgeReport = {
  eventsDeleted: number;
  eventsPreserved: number;
  oldestRemaining: number | null;
};

export function provenancePurgeReportFromWire(w: Record<string, unknown>): ProvenancePurgeReport {
  return {
    eventsDeleted: Number(w.events_deleted ?? 0),
    eventsPreserved: Number(w.events_preserved ?? 0),
    oldestRemaining: w.oldest_remaining != null ? Number(w.oldest_remaining) : null,
  };
}

// ── Operational collection types ───────────────────────────────────────

/**
 * Extraction mode for a single registered FTS property path.
 *
 * `"scalar"` resolves the path and appends the scalar value(s) — matches
 * legacy pre-Phase-4 behaviour. `"recursive"` walks every scalar leaf
 * rooted at the path; each leaf emits one position-map row and is
 * eligible for match-attribution via `withMatchAttribution()`.
 */
export type FtsPropertyPathMode = "scalar" | "recursive";

/** A single registered property-FTS path with its extraction mode. */
export type FtsPropertyPathSpec = {
  /** JSON path to the property (must start with `$.`). */
  path: string;
  /** Whether to treat this path as a scalar or recursively walk it. */
  mode: FtsPropertyPathMode;
};

/** A registered FTS property projection schema for a node kind. */
export type FtsPropertySchemaRecord = {
  /** The node kind this schema applies to. */
  kind: string;
  /** JSON property paths to extract (e.g. `["$.name", "$.title"]`). */
  propertyPaths: string[];
  /** Separator used when concatenating extracted values. */
  separator: string;
  /** Schema format version. */
  formatVersion: number;
};

/** @internal */
export function ftsPropertySchemaRecordFromWire(w: Record<string, unknown>): FtsPropertySchemaRecord {
  return {
    kind: String(w.kind ?? ""),
    propertyPaths: Array.isArray(w.property_paths) ? w.property_paths.map(String) : [],
    separator: String(w.separator ?? " "),
    formatVersion: Number(w.format_version ?? 1),
  };
}

export type OperationalCollectionRecord = {
  name: string;
  kind: string;
  schemaJson: string;
  retentionJson: string;
  validationJson: string;
  secondaryIndexesJson: string;
  formatVersion: number;
  createdAt: number;
  filterFieldsJson: string;
  disabledAt: number | null;
};

export function operationalCollectionRecordFromWire(w: Record<string, unknown>): OperationalCollectionRecord {
  return {
    name: String(w.name ?? ""),
    kind: String(w.kind ?? ""),
    schemaJson: String(w.schema_json ?? ""),
    retentionJson: String(w.retention_json ?? ""),
    validationJson: String(w.validation_json ?? ""),
    secondaryIndexesJson: String(w.secondary_indexes_json ?? "[]"),
    formatVersion: Number(w.format_version ?? 0),
    createdAt: Number(w.created_at ?? 0),
    filterFieldsJson: String(w.filter_fields_json ?? "[]"),
    disabledAt: w.disabled_at != null ? Number(w.disabled_at) : null,
  };
}

export type OperationalMutationRow = {
  id: string;
  collectionName: string;
  recordKey: string;
  opKind: string;
  payloadJson: unknown;
  sourceRef: string | null;
  createdAt: number;
};

function operationalMutationRowFromWire(w: Record<string, unknown>): OperationalMutationRow {
  return {
    id: String(w.id ?? ""),
    collectionName: String(w.collection_name ?? ""),
    recordKey: String(w.record_key ?? ""),
    opKind: String(w.op_kind ?? ""),
    payloadJson: parseJsonField(w.payload_json),
    sourceRef: w.source_ref != null ? String(w.source_ref) : null,
    createdAt: Number(w.created_at ?? 0),
  };
}

export type OperationalCurrentRow = {
  collectionName: string;
  recordKey: string;
  payloadJson: unknown;
  updatedAt: number;
  lastMutationId: string;
};

function operationalCurrentRowFromWire(w: Record<string, unknown>): OperationalCurrentRow {
  return {
    collectionName: String(w.collection_name ?? ""),
    recordKey: String(w.record_key ?? ""),
    payloadJson: parseJsonField(w.payload_json),
    updatedAt: Number(w.updated_at ?? 0),
    lastMutationId: String(w.last_mutation_id ?? ""),
  };
}

export type OperationalTraceReport = {
  collectionName: string;
  recordKey: string | null;
  mutationCount: number;
  currentCount: number;
  mutations: OperationalMutationRow[];
  currentRows: OperationalCurrentRow[];
};

export function operationalTraceReportFromWire(w: Record<string, unknown>): OperationalTraceReport {
  return {
    collectionName: String(w.collection_name ?? ""),
    recordKey: w.record_key != null ? String(w.record_key) : null,
    mutationCount: Number(w.mutation_count ?? 0),
    currentCount: Number(w.current_count ?? 0),
    mutations: asArray(w.mutations).map(operationalMutationRowFromWire),
    currentRows: asArray(w.current_rows).map(operationalCurrentRowFromWire),
  };
}

export type OperationalReadReport = {
  collectionName: string;
  rowCount: number;
  appliedLimit: number;
  wasLimited: boolean;
  rows: OperationalMutationRow[];
};

export function operationalReadReportFromWire(w: Record<string, unknown>): OperationalReadReport {
  return {
    collectionName: String(w.collection_name ?? ""),
    rowCount: Number(w.row_count ?? 0),
    appliedLimit: Number(w.applied_limit ?? 0),
    wasLimited: Boolean(w.was_limited),
    rows: asArray(w.rows).map(operationalMutationRowFromWire),
  };
}

export type OperationalRepairReport = {
  collectionsRebuilt: number;
  currentRowsRebuilt: number;
};

export function operationalRepairReportFromWire(w: Record<string, unknown>): OperationalRepairReport {
  return {
    collectionsRebuilt: Number(w.collections_rebuilt ?? 0),
    currentRowsRebuilt: Number(w.current_rows_rebuilt ?? 0),
  };
}

export type OperationalHistoryValidationIssue = {
  mutationId: string;
  recordKey: string;
  opKind: string;
  message: string;
};

function operationalHistoryValidationIssueFromWire(w: Record<string, unknown>): OperationalHistoryValidationIssue {
  return {
    mutationId: String(w.mutation_id ?? ""),
    recordKey: String(w.record_key ?? ""),
    opKind: String(w.op_kind ?? ""),
    message: String(w.message ?? ""),
  };
}

export type OperationalHistoryValidationReport = {
  collectionName: string;
  checkedRows: number;
  invalidRowCount: number;
  issues: OperationalHistoryValidationIssue[];
};

export function operationalHistoryValidationReportFromWire(w: Record<string, unknown>): OperationalHistoryValidationReport {
  return {
    collectionName: String(w.collection_name ?? ""),
    checkedRows: Number(w.checked_rows ?? 0),
    invalidRowCount: Number(w.invalid_row_count ?? 0),
    issues: asArray(w.issues).map(operationalHistoryValidationIssueFromWire),
  };
}

export type OperationalCompactionReport = {
  collectionName: string;
  deletedMutations: number;
  dryRun: boolean;
  beforeTimestamp: number | null;
};

export function operationalCompactionReportFromWire(w: Record<string, unknown>): OperationalCompactionReport {
  return {
    collectionName: String(w.collection_name ?? ""),
    deletedMutations: Number(w.deleted_mutations ?? 0),
    dryRun: Boolean(w.dry_run),
    beforeTimestamp: w.before_timestamp != null ? Number(w.before_timestamp) : null,
  };
}

export type OperationalPurgeReport = {
  collectionName: string;
  deletedMutations: number;
  beforeTimestamp: number;
};

export function operationalPurgeReportFromWire(w: Record<string, unknown>): OperationalPurgeReport {
  return {
    collectionName: String(w.collection_name ?? ""),
    deletedMutations: Number(w.deleted_mutations ?? 0),
    beforeTimestamp: Number(w.before_timestamp ?? 0),
  };
}

export type OperationalSecondaryIndexRebuildReport = {
  collectionName: string;
  mutationEntriesRebuilt: number;
  currentEntriesRebuilt: number;
};

export function operationalSecondaryIndexRebuildReportFromWire(w: Record<string, unknown>): OperationalSecondaryIndexRebuildReport {
  return {
    collectionName: String(w.collection_name ?? ""),
    mutationEntriesRebuilt: Number(w.mutation_entries_rebuilt ?? 0),
    currentEntriesRebuilt: Number(w.current_entries_rebuilt ?? 0),
  };
}

export type OperationalRetentionPlanItem = {
  collectionName: string;
  actionKind: string;
  candidateDeletions: number;
  beforeTimestamp: number | null;
  maxRows: number | null;
  lastRunAt: number | null;
};

function operationalRetentionPlanItemFromWire(w: Record<string, unknown>): OperationalRetentionPlanItem {
  return {
    collectionName: String(w.collection_name ?? ""),
    actionKind: String(w.action_kind ?? ""),
    candidateDeletions: Number(w.candidate_deletions ?? 0),
    beforeTimestamp: w.before_timestamp != null ? Number(w.before_timestamp) : null,
    maxRows: w.max_rows != null ? Number(w.max_rows) : null,
    lastRunAt: w.last_run_at != null ? Number(w.last_run_at) : null,
  };
}

export type OperationalRetentionPlanReport = {
  plannedAt: number;
  collectionsExamined: number;
  items: OperationalRetentionPlanItem[];
};

export function operationalRetentionPlanReportFromWire(w: Record<string, unknown>): OperationalRetentionPlanReport {
  return {
    plannedAt: Number(w.planned_at ?? 0),
    collectionsExamined: Number(w.collections_examined ?? 0),
    items: asArray(w.items).map(operationalRetentionPlanItemFromWire),
  };
}

export type OperationalRetentionRunItem = {
  collectionName: string;
  actionKind: string;
  deletedMutations: number;
  beforeTimestamp: number | null;
  maxRows: number | null;
  rowsRemaining: number;
};

function operationalRetentionRunItemFromWire(w: Record<string, unknown>): OperationalRetentionRunItem {
  return {
    collectionName: String(w.collection_name ?? ""),
    actionKind: String(w.action_kind ?? ""),
    deletedMutations: Number(w.deleted_mutations ?? 0),
    beforeTimestamp: w.before_timestamp != null ? Number(w.before_timestamp) : null,
    maxRows: w.max_rows != null ? Number(w.max_rows) : null,
    rowsRemaining: Number(w.rows_remaining ?? 0),
  };
}

export type OperationalRetentionRunReport = {
  executedAt: number;
  collectionsExamined: number;
  collectionsActedOn: number;
  dryRun: boolean;
  items: OperationalRetentionRunItem[];
};

export function operationalRetentionRunReportFromWire(w: Record<string, unknown>): OperationalRetentionRunReport {
  return {
    executedAt: Number(w.executed_at ?? 0),
    collectionsExamined: Number(w.collections_examined ?? 0),
    collectionsActedOn: Number(w.collections_acted_on ?? 0),
    dryRun: Boolean(w.dry_run),
    items: asArray(w.items).map(operationalRetentionRunItemFromWire),
  };
}

// ── Enums / constants ──────────────────────────────────────────────────

export type ProvenanceMode = "warn" | "require";
export type TelemetryLevel = "counters" | "statements" | "profiling";
export type TraverseDirection = "in" | "out";
export type OperationalCollectionKind = "append_only_log" | "latest_state";
export type OperationalFilterMode = "exact" | "prefix" | "range";

// ── Admin input types ──────────────────────────────────────────────────

export type OperationalRegisterRequest = {
  name: string;
  kind: OperationalCollectionKind;
  schemaJson: string;
  retentionJson: string;
  formatVersion: number;
  filterFieldsJson?: string;
  validationJson?: string;
  secondaryIndexesJson?: string;
};

export function operationalRegisterRequestToWire(input: OperationalRegisterRequest): Record<string, unknown> {
  return {
    name: input.name,
    kind: input.kind,
    schema_json: input.schemaJson,
    retention_json: input.retentionJson,
    format_version: input.formatVersion,
    filter_fields_json: input.filterFieldsJson ?? "[]",
    validation_json: input.validationJson ?? "",
    secondary_indexes_json: input.secondaryIndexesJson ?? "[]",
  };
}

export type OperationalFilterClause = {
  mode: OperationalFilterMode;
  field: string;
  value?: string | number;
  lower?: number;
  upper?: number;
};

export function operationalFilterClauseToWire(clause: OperationalFilterClause): Record<string, unknown> {
  const wire: Record<string, unknown> = { mode: clause.mode, field: clause.field };
  if (clause.value !== undefined) wire.value = clause.value;
  if (clause.lower !== undefined) wire.lower = clause.lower;
  if (clause.upper !== undefined) wire.upper = clause.upper;
  return wire;
}

export type OperationalReadRequest = {
  collectionName: string;
  filters: OperationalFilterClause[];
  limit?: number;
};

export function operationalReadRequestToWire(input: OperationalReadRequest): Record<string, unknown> {
  return {
    collection_name: input.collectionName,
    filters: input.filters.map(operationalFilterClauseToWire),
    limit: input.limit ?? null,
  };
}

// ── Write input types ──────────────────────────────────────────────────

export type ChunkPolicy = "preserve" | "replace";

export type NodeInsertInput = {
  rowId: string;
  logicalId: string;
  kind: string;
  properties: unknown;
  sourceRef?: string;
  upsert?: boolean;
  chunkPolicy?: ChunkPolicy;
  contentRef?: string;
};

export type EdgeInsertInput = {
  rowId: string;
  logicalId: string;
  kind: string;
  properties: unknown;
  sourceRef?: string;
  upsert?: boolean;
};

export type ChunkInsertInput = {
  id: string;
  textContent: string;
  byteStart?: number;
  byteEnd?: number;
  contentHash?: string;
};

export type RunInsertInput = {
  id: string;
  kind: string;
  status: string;
  properties: unknown;
  sourceRef?: string;
  upsert?: boolean;
  supersedesId?: string;
};

export type StepInsertInput = {
  id: string;
  kind: string;
  status: string;
  properties: unknown;
  sourceRef?: string;
  upsert?: boolean;
  supersedesId?: string;
};

export type ActionInsertInput = {
  id: string;
  kind: string;
  status: string;
  properties: unknown;
  sourceRef?: string;
  upsert?: boolean;
  supersedesId?: string;
};

export type OperationalAppendInput = {
  collection: string;
  recordKey: string;
  payloadJson: unknown;
  sourceRef?: string;
};

export type OperationalPutInput = {
  collection: string;
  recordKey: string;
  payloadJson: unknown;
  sourceRef?: string;
};

export type OperationalDeleteInput = {
  collection: string;
  recordKey: string;
  sourceRef?: string;
};

// ── Internal helpers ───────────────────────────────────────────────────

function asArray(value: unknown): Array<Record<string, unknown>> {
  return Array.isArray(value) ? (value as Array<Record<string, unknown>>) : [];
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.map(String) : [];
}

function asObj(value: unknown): Record<string, unknown> {
  return (value != null && typeof value === "object" && !Array.isArray(value))
    ? (value as Record<string, unknown>)
    : {};
}

function parseJsonField(value: unknown): unknown {
  if (typeof value === "string") {
    try {
      return JSON.parse(value);
    } catch {
      return value;
    }
  }
  return value;
}
