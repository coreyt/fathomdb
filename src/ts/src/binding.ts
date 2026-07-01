// Native-binding loader. Locates the platform-tagged `.node` artifact
// produced by `napi build` and re-exports it as a typed module.

import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));

// `napi build` writes `<name>.<triple>.node` next to the package.json
// (cwd of the build script). Walk up from this file's compiled location
// to find candidates.
const SEARCH_ROOTS = [
  join(here, ".."), // dist/src/ -> dist/
  join(here, "..", ".."), // dist/src/ -> dist/.. (pkg root)
  here, // co-located fallback
];

const TRIPLES = [
  "linux-x64-gnu",
  "linux-x64-musl",
  "linux-arm64-gnu",
  "linux-arm64-musl",
  "linux-arm-gnueabihf",
  "darwin-x64",
  "darwin-arm64",
  "darwin-universal",
  "win32-x64-msvc",
  "win32-ia32-msvc",
  "win32-arm64-msvc",
  "freebsd-x64",
  "android-arm64",
  "android-arm-eabi",
];

function loadNative(): unknown {
  for (const root of SEARCH_ROOTS) {
    for (const triple of TRIPLES) {
      const candidate = join(root, `fathomdb.${triple}.node`);
      if (existsSync(candidate)) {
        return require(candidate);
      }
    }
  }
  throw new Error(
    "fathomdb native binding not found. Run `npm run build` to compile it.",
  );
}

interface NativeWriteReceipt {
  cursor: number;
  rowCursors: number[];
  danglingEdgeEndpoints: number;
}

interface NativeSoftFallback {
  branch: string;
}

interface NativeSearchHit {
  id: number;
  kind: string;
  body: string;
  score: number;
  branch: string;
  /** G0 Phase-2 — source-document provenance; set only for graph-arm hits. */
  sourceId?: string | null;
  /** 0.8.5 (EXP-0) — CE score (sigmoid of the cross-encoder logit) for in-pool
   * reranked hits; null otherwise. */
  ceScore?: number | null;
  /** Cause-A (0.8.11.2) — additive cross-session-stable hit id (`"l:"`-tagged
   * `logical_id`, or `"h:"` content-hash for doc nodes); null for synthetic
   * passages. */
  stableId?: string | null;
}

interface NativeQueryTrace {
  queryChars: number;
  k: number;
  rerankDepth: number;
  poolN: number;
  alpha: number;
  useGraphArm: boolean;
  recency: boolean;
  embedderId: string;
  ceActive: boolean;
  vectorHits: number;
  textHits: number;
  graphHits: number;
}

interface NativePerHitExplain {
  id: number;
  arm: string;
  vectorRank?: number | null;
  textRank?: number | null;
  graphRank?: number | null;
  fusedScore: number;
  ceScore?: number | null;
  blended: number;
}

interface NativeExplanation {
  trace: NativeQueryTrace;
  perHit: NativePerHitExplain[];
}

interface NativeSearchResult {
  projectionCursor: number;
  softFallback: NativeSoftFallback | null;
  results: NativeSearchHit[];
  /** 0.8.8 EXP-OBS (Slice 10) — opt-in explanation sidecar; null by default. */
  explanation?: NativeExplanation | null;
}

interface NativeSearchFilter {
  sourceType?: string;
  kind?: string;
  createdAfter?: number;
  status?: string;
}

interface NativeMigrationStepReport {
  stepId: number;
  durationMs: number | null;
  failed: boolean;
}

interface NativeEmbedderIdentity {
  name: string;
  revision: string;
  dimension: number;
}

// Slice 30 (G2/G3) — native row shapes for the governed read.* namespace.
export interface NativeNodeRecord {
  logicalId: string;
  kind: string;
  body: string;
  writeCursor: number;
}

export interface NativeOpStoreRow {
  id: number;
  collection: string;
  recordKey: string;
  opKind: string;
  payload: string;
  schemaId: string | null;
  writeCursor: number;
}

export interface NativeReadCollectionOptions {
  afterId?: number;
  limit: number;
}

// EU-6 FIX-2: Wide native shape emitted by the napi-rs binding. napi-rs
// has no first-class tagged-union support, so every variant payload
// field is modelled here as `Option<T>` (collapsed to `T | null |
// undefined`). The canonical narrow public surface — the discriminated
// `EmbedderEvent` union — lives in `index.ts` and is built by
// `mapEmbedderEvent()` at the napi → SDK seam.
export interface NativeEmbedderEvent {
  kind: string;
  file?: string | null;
  url?: string | null;
  bytes?: number | null;
  sha256?: string | null;
  cachePath?: string | null;
  durationMs?: number | null;
  dim?: number | null;
  docCount?: number | null;
}

interface NativeOpenReport {
  schemaVersionBefore: number;
  schemaVersionAfter: number;
  migrationSteps: NativeMigrationStepReport[];
  embedderWarmupMs: number;
  queryBackend: string;
  defaultEmbedder: NativeEmbedderIdentity;
  embedderDownloadMs: number | null;
  embedderEvents: NativeEmbedderEvent[];
  embedderMeanCenteringRequired: boolean;
  embedderMeanVecPinned: boolean;
}

interface NativeCounterSnapshot {
  queries: number;
  writes: number;
  writeRows: number;
  adminOps: number;
  cacheHit: number;
  cacheMiss: number;
}

interface NativeAttachSubscriberOptions {
  heartbeatIntervalMs?: number;
}

interface NativeEngineConfig {
  embedderPoolSize?: number;
  schedulerRuntimeThreads?: number;
  provenanceRowCap?: number;
  embedderCallTimeoutMs?: number;
  slowThresholdMs?: number;
}

// Slice 20 (G5/G6) — graph traversal result shapes.

export interface NativeExpandedNode {
  node: NativeNodeRecord;
  hopCount: number;
}

export interface NativeSearchExpandResult {
  searchHits: NativeSearchHit[];
  expanded: NativeExpandedNode[];
  allLogicalIds: string[];
}

// G11 (Slice 15) — BYO-LLM ingest receipt.
export interface NativeIngestWithExtractorReceipt {
  nodesWritten: number;
  edgesWritten: number;
  docsProcessed: number;
}

// G11 (Slice 15) — a document sent to the BYO-LLM extraction harness.
export interface NativeExtractDocument {
  sourceDocId: string;
  body: string;
}

// 0.8.12 Slice 15 (OPP-2) — BYO-LLM consolidation receipt.
export interface NativeConsolidateReceipt {
  clustersProcessed: number;
  edgesExamined: number;
  edgesKept: number;
  edgesInvalidated: number;
  edgesSuperseded: number;
}

// 0.8.12 Slice 15 (OPP-2) — one (subject, relation) axis to consolidate.
export interface NativeConsolidateAxis {
  subjectLogicalId: string;
  relation: string;
}

interface NativeEngineOpenOptions {
  engineConfig?: NativeEngineConfig;
  useDefaultEmbedder?: boolean;
}

interface NativeAdminConfigureOptions {
  name: string;
  body: string;
}

export interface NativeEngine {
  write(batch: unknown[]): Promise<NativeWriteReceipt>;
  search(
    query: string,
    filter?: NativeSearchFilter,
    rerankDepth?: number,
    useGraphArm?: boolean,
    alpha?: number,
    poolN?: number,
    explain?: boolean,
  ): Promise<NativeSearchResult>;
  close(): Promise<void>;
  drain(timeoutMs: number): Promise<void>;
  counters(): NativeCounterSnapshot;
  openReport(): NativeOpenReport;
  setProfiling(enabled: boolean): void;
  setSlowThresholdMs(value: number): void;
  attachSubscriber(callback: unknown, options?: NativeAttachSubscriberOptions): void;
  // G11 (Slice 15) — BYO-LLM ingest.
  ingestWithExtractor(
    cmd: string[],
    documents: NativeExtractDocument[],
  ): Promise<NativeIngestWithExtractorReceipt>;
  // 0.8.12 Slice 15 (OPP-2) — BYO-LLM consolidation over the same transport.
  consolidateWithProvider(
    cmd: string[],
    axes: NativeConsolidateAxis[],
  ): Promise<NativeConsolidateReceipt>;
  // 0.8.6 Slice 10 — read-path embed primitive (Py↔TS parity for Engine.embed).
  embed(text: string): Promise<number[]>;
  // 0.8.8 Slice 15 (OPP-9) — opt-in local telemetry capture. enable/record are
  // async (napi `async fn`); lastTelemetryQueryId is a sync getter.
  enableTelemetry(sinkPath: string): Promise<void>;
  lastTelemetryQueryId(): string | null;
  recordFeedback(
    queryId: string,
    relevantIds: number[],
    irrelevantIds: number[],
    labelSource: string,
  ): Promise<void>;
  // EU-6 test-hooks-gated seam. Present only when the napi binding is
  // built with `--features test-hooks`; the TS surface forwards calls
  // unconditionally and the runtime fails fast if absent.
  configureVectorKindForTest?(kind: string): Promise<void>;
  writeVectorForTest?(kind: string, text: string): Promise<void>;
}

export interface NativeModule {
  Engine: {
    open(path: string, options?: NativeEngineOpenOptions): Promise<NativeEngine>;
  };
  adminConfigure(
    engine: NativeEngine,
    options: NativeAdminConfigureOptions,
  ): Promise<NativeWriteReceipt>;
  // Slice 30 — governed read.* native fns (G2/G3).
  readGet(engine: NativeEngine, logicalId: string): Promise<NativeNodeRecord | null>;
  readGetMany(
    engine: NativeEngine,
    logicalIds: string[],
  ): Promise<(NativeNodeRecord | null)[]>;
  readCollection(
    engine: NativeEngine,
    collection: string,
    options: NativeReadCollectionOptions,
  ): Promise<NativeOpStoreRow[]>;
  readMutations(
    engine: NativeEngine,
    collection: string,
    options: NativeReadCollectionOptions,
  ): Promise<NativeOpStoreRow[]>;
  // Slice 35 (G4) — read.list with Predicate filter.
  readList(
    engine: NativeEngine,
    kind: string,
    predicates?: NativePredicateInput[],
    limit?: number,
  ): Promise<NativeNodeRecord[]>;
  // 0.8.11 Slice 40 (#17) — unified Filter → read.list backend.
  readListFilter(
    engine: NativeEngine,
    kind: string,
    terms?: NativeFilterTermInput[],
    limit?: number,
  ): Promise<NativeNodeRecord[]>;
  // Slice 20 — G5/G6 graph traversal fns.
  graphNeighbors(
    engine: NativeEngine,
    logicalId: string,
    depth: number,
    direction: string,
  ): Promise<NativeNodeRecord[]>;
  searchExpand(
    engine: NativeEngine,
    query: string,
    depth: number,
    sourceType?: string,
    kind?: string,
    createdAfter?: number,
    status?: string,
  ): Promise<NativeSearchExpandResult>;
  forcePanicForTest?: () => void;
  forcePanicInAccessorForTest?: () => void;
}

/// G4 (Slice 35) — predicate input for `readList`. Mirrors `PredicateInput`
/// on the Rust side. The value is split into three optional fields to match
/// the napi `#[napi(object)]` struct with optional fields.
export interface NativePredicateInput {
  type: string;
  path: string;
  valueStr?: string | null;
  valueInt?: number | null;
  valueBool?: boolean | null;
}

/// 0.8.11 Slice 40 (#17) — one term of the unified `Filter` grammar. Mirrors the
/// napi `FilterTermInput` struct. `term` ∈ `{"source_type","kind",
/// "created_after","status","json"}`; the four shorthand terms set
/// `valueStr`/`valueInt`, the `json` term sets `predicate`.
export interface NativeFilterTermInput {
  term: string;
  valueStr?: string | null;
  valueInt?: number | null;
  predicate?: NativePredicateInput | null;
}

export const native = loadNative() as NativeModule;
