// FathomDB TypeScript SDK public surface.
//
// Five-verb top-level surface (Engine.open, engine.write, engine.search,
// engine.close, admin.configure), engine-attached instrumentation, and
// the FathomDbError leaf-class hierarchy per
// `dev/interfaces/typescript.md` and `dev/design/bindings.md` § 3. The
// runtime is the napi-rs binding in `fathomdb-napi`; this file is a
// thin TS wrapper that funnels every native error through
// `rethrowTyped`.

import {
  native,
  type NativeEmbedderEvent,
  type NativeEngine,
  type NativePerHitExplain,
} from "./binding.js";
import { InvalidArgumentError, InvalidFilterError, rethrowTyped } from "./errors.js";
import type { NodeRecord, Predicate } from "./read.js";
import { validateFfiString, validateFfiTree } from "./validation.js";

export * from "./errors.js";
export { read } from "./read.js";
export type { NodeRecord, OpStoreRow, Predicate, ReadCollectionOptions } from "./read.js";

export interface EngineConfig {
  embedderPoolSize?: number;
  schedulerRuntimeThreads?: number;
  provenanceRowCap?: number;
  embedderCallTimeoutMs?: number;
  slowThresholdMs?: number;
}

export interface EngineOpenOptions {
  engineConfig?: EngineConfig;
  /**
   * EU-6: opt-in to the engine's pinned default embedder
   * (`fathomdb-bge-small-en-v1.5`). On first use, weights are downloaded
   * from HuggingFace and cached under `~/.cache/fathomdb/embedders/`.
   * `false` (the default) opens without an embedder; vector writes
   * then fail with `EmbedderNotConfiguredError`. Caller-supplied
   * custom embedders are deferred to a future release per
   * ADR-0.6.0-embedder-protocol Invariant 3.
   */
  useDefaultEmbedder?: boolean;
}

export interface WriteReceipt {
  cursor: number;
  /**
   * G0 (Slice 15) — per-row `write_cursor`s, 1:1 with the input batch order.
   * The `write_cursor`-as-row-id identity carrier; for an N-row batch this is
   * `[cursor-N+1, …, cursor]`.
   */
  rowCursors: number[];
  /**
   * G8 (Slice 20 / F10) — count of edge endpoints in this batch that point at a
   * non-existent or superseded canonical node (an active node carrying that
   * `logical_id`). `from_id`/`to_id` are probed independently, so one edge
   * contributes 0, 1, or 2. Informational only: the batch commits regardless
   * (flag-and-count). `0` when the batch committed no active edges.
   */
  danglingEdgeEndpoints: number;
}

/** G11 (Slice 15) — BYO-LLM ingest receipt. */
export interface IngestWithExtractorReceipt {
  /** Number of `canonical_nodes` rows written (new insertions only). */
  nodesWritten: number;
  /** Number of `canonical_edges` rows written (new fact-edge insertions). */
  edgesWritten: number;
  /** Number of documents processed (including no-facts documents). */
  docsProcessed: number;
}

/** G11 (Slice 15) — a document sent to a BYO-LLM extraction harness. */
export interface ExtractDocument {
  /** Stable opaque identifier for this document. */
  sourceDocId: string;
  /** Full text body to extract entities and relationships from. */
  body: string;
}

/** 0.8.12 Slice 15 (OPP-2) — BYO-LLM consolidation receipt. */
export interface ConsolidateReceipt {
  /** Number of (subject, relation) axes with a non-empty cluster dispatched. */
  clustersProcessed: number;
  /** Number of candidate edges presented across all clusters. */
  edgesExamined: number;
  /** Number of edges the harness ruled `keep`. */
  edgesKept: number;
  /** Number of edges the harness ruled `invalidate` (t_invalid set). */
  edgesInvalidated: number;
  /** Number of edges the harness ruled `supersede`/`merge` (marked superseded). */
  edgesSuperseded: number;
}

/** 0.8.12 Slice 15 (OPP-2) — one (subject, relation) axis to consolidate. */
export interface ConsolidateAxis {
  /** Stable `logicalId` of the subject entity (edge `fromId`). */
  subjectLogicalId: string;
  /** The relation/edge kind whose competing fact-edges form the cluster. */
  relation: string;
}

export type SoftFallbackBranch = "vector" | "text" | "text_edge" | "graph_arm";

export interface SoftFallback {
  branch: SoftFallbackBranch;
}

/**
 * C-2 (0.8.19 / OPP-12 Phase-1, TC-8) — the typed id-space carrier for
 * `SearchHit.id`. `space` is the lowercase discriminant (`"logical"` |
 * `"content"` | `"passage"`), mirroring the engine's `IdSpaceKind` enum (the
 * C-2 binding — a typed carrier, not a magic-prefixed string). `value` is the
 * bare id (id-space prefix stripped). The prefixed form is `${prefix}${value}`
 * (`l:`/`h:`/`p:`) — byte-identical to the pre-0.8.19 `stableId`. Only `logical`
 * ids are lifecycle-addressable.
 */
export interface IdSpace {
  space: string;
  value: string;
}

export interface SearchHit {
  /**
   * C-2 (0.8.19 / TC-8) — the typed, non-null, id-space-total hit id
   * (`{ space, value }`). Governed hits are `logical` (`"l:"`), doc-seeded hits
   * `content` (`"h:"`), synthetic passages `passage` (`"p:"`). Its `value`
   * equals the pre-0.8.19 `stableId` (which this subsumes) so cross-session
   * real-gold keying continues on `id`; it survives re-ingest and never
   * participates in ranking. The pre-C-2 positional `write_cursor` id is
   * engine-internal and no longer surfaced.
   */
  id: IdSpace;
  kind: string;
  body: string;
  /**
   * G9 RRF-fused relevance (`Σ 1/(RRF_K + rank)`; higher = more relevant),
   * optionally recency-reweighted. Raw `vec_distance_l2`/`bm25()` are fused on
   * rank, never compared raw.
   */
  score: number;
  branch: SoftFallbackBranch;
  /**
   * G0 Phase-2 — source-document provenance. The traversed edge's `source_id`
   * for a graph-arm hit; `null` for every two-arm hit.
   */
  sourceId: string | null;
  /**
   * 0.8.5 (EXP-0) — per-candidate CE score (`ce_norm = sigmoid(ce_logit)`) for
   * hits inside the reranked pool; `null` otherwise (out-of-pool, the identity
   * path, or no CE model loaded).
   */
  ceScore: number | null;
}

/**
 * G10 — closed metadata filter for `engine.search(query, filter?)`. All fields
 * optional; an all-`undefined` filter (or omitted) is the unfiltered path. A
 * closed shape, not an open DSL. `createdAfter` is a `created_at >= bound` lower
 * bound in unix seconds. `status` filters the vec0 `status` metadata column,
 * which ships an empty-string sentinel only (no real population source yet), so
 * a `status: "open"`-style filter prunes every row until a population slice
 * lands. Mirrors the Python `SearchFilter` (cross-binding parity).
 */
export interface SearchFilter {
  sourceType?: string;
  kind?: string;
  createdAfter?: number;
  status?: string;
}

/**
 * 0.8.11 Slice 40 (#17) — one term of the unified `Filter` grammar (G4 + G10),
 * a discriminated union mirroring `fathomdb_engine::FilterTerm`
 * (ADR-0.8.11, Option A). Exactly five variants: the four G10 shorthand metadata
 * fields plus the general G4 json-path `Predicate` (`json`). The `json` term is
 * accepted on `read.listFilter` but **typed-rejected** on `search` (D3: an
 * arbitrary json-path predicate is never demoted to a post-KNN `json_extract`).
 */
export type FilterTerm =
  | { term: "source_type"; value: string }
  | { term: "kind"; value: string }
  | { term: "created_after"; value: number }
  | { term: "status"; value: string }
  | { term: "json"; predicate: Predicate };

/**
 * 0.8.11 Slice 40 (#17) — the unified, closed `Filter` contract. ONE typed
 * surface (implicit-AND `terms`) dispatched to two backends: the vec0-metadata
 * indexed pre-KNN `WHERE` for `search`, and `json_extract` over
 * `canonical_nodes.body` for `read.listFilter`. The shipped `SearchFilter` (G10)
 * and `Predicate` lists (G4) re-express as sugar that lowers into this type (D4).
 * Mirrors the Python `fathomdb.filter.Filter` (cross-binding parity, X1).
 */
export interface Filter {
  terms: FilterTerm[];
}

const VEC0_JSON_REJECT =
  "arbitrary json-path predicate not supported on search_filtered; it would " +
  "require a post-KNN json_extract that defeats the indexed pre-KNN filter " +
  "(ADR-0.8.11 D3 no-demotion guarantee)";

function isUnifiedFilter(f: SearchFilter | Filter | undefined): f is Filter {
  return f !== undefined && Array.isArray((f as Filter).terms);
}

/**
 * vec0 (`search`) backend lowering of the unified `Filter` to the shipped
 * `SearchFilter` sugar. Typed-rejects a `json` term with `InvalidFilterError`
 * (D3 no-demotion guarantee); the lowering is canonical-order-independent.
 */
export function filterToSearchFilter(filter: Filter): SearchFilter {
  const sf: SearchFilter = {};
  for (const t of filter.terms) {
    switch (t.term) {
      case "source_type":
        sf.sourceType = t.value;
        break;
      case "kind":
        sf.kind = t.value;
        break;
      case "created_after":
        sf.createdAfter = t.value;
        break;
      case "status":
        sf.status = t.value;
        break;
      case "json":
        throw new InvalidFilterError(VEC0_JSON_REJECT);
    }
  }
  return sf;
}

/** D4 sugar: re-express a shipped `SearchFilter` as the unified `Filter`. */
export function searchFilterToFilter(sf: SearchFilter): Filter {
  const terms: FilterTerm[] = [];
  if (sf.sourceType !== undefined) terms.push({ term: "source_type", value: sf.sourceType });
  if (sf.kind !== undefined) terms.push({ term: "kind", value: sf.kind });
  if (sf.createdAfter !== undefined) terms.push({ term: "created_after", value: sf.createdAfter });
  if (sf.status !== undefined) terms.push({ term: "status", value: sf.status });
  return { terms };
}

/**
 * 0.8.8 EXP-OBS (Slice 10) — query-level retrieval trace (mirror of the Rust
 * `QueryTrace`). Present only on the opt-in `search(..., { explain: true })` path,
 * inside `Explanation.trace`. `queryChars` is the query LENGTH only (never the
 * text); `embedderId` is `"name@rev (dim=N)"` (`""` when none). Field
 * names/order mirror the Python `QueryTrace` (cross-binding parity).
 */
export interface QueryTrace {
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

/**
 * 0.8.8 EXP-OBS (Slice 10) — per-hit provenance + score breakdown (mirror of the
 * Rust `PerHitExplain`); parallel to (and same order as) `SearchResult.results`.
 * `id` mirrors `SearchHit.id` exactly (`perHit[i].id === results[i].id`).
 * `fusedScore` is the RAW post-recency, pre-CE RRF score (not normalized).
 * `importance`/`confidence` (0.8.16 Slice 5 / F9) are the node importance / edge
 * confidence applied to this hit's contribution (`null` = graceful-absent /
 * neutral); they mirror the Python `PerHitExplain` additive fields.
 */
export interface PerHitExplain {
  id: number;
  arm: SoftFallbackBranch;
  vectorRank: number | null;
  textRank: number | null;
  graphRank: number | null;
  fusedScore: number;
  ceScore: number | null;
  blended: number;
  importance: number | null;
  confidence: number | null;
}

/**
 * @internal — map one native per-hit explain object into the public
 * {@link PerHitExplain}. Factored out of `Engine.search` so the mapping is
 * unit-testable against a fake native object without the compiled `.node`
 * (0.8.16 Slice 5 / F9, codex §9 fix-2). `importance`/`confidence` are the
 * additive F9 fields (node importance / edge confidence applied to this hit's
 * contribution; `null` = graceful-absent / neutral), symmetric with the Python
 * `_map_per_hit_explain` wrapper.
 */
export function mapPerHitExplain(p: NativePerHitExplain): PerHitExplain {
  const armOf = (a: string): SoftFallbackBranch =>
    a === "vector" || a === "text" || a === "text_edge" || a === "graph_arm"
      ? (a as SoftFallbackBranch)
      : "text";
  return {
    id: p.id,
    arm: armOf(p.arm),
    vectorRank: p.vectorRank ?? null,
    textRank: p.textRank ?? null,
    graphRank: p.graphRank ?? null,
    fusedScore: p.fusedScore,
    ceScore: p.ceScore ?? null,
    blended: p.blended,
    importance: p.importance ?? null,
    confidence: p.confidence ?? null,
  };
}

/**
 * 0.8.8 EXP-OBS (Slice 10) — opt-in retrieval explanation sidecar (mirror of the
 * Rust `Explanation`): a query-level `trace` + a per-hit breakdown. Returned on
 * `SearchResult.explanation` only when `search(..., { explain: true })`; `null`
 * (default) keeps the result byte-identical to the pre-0.8.8 shape.
 */
export interface Explanation {
  trace: QueryTrace;
  perHit: PerHitExplain[];
}

export interface SearchResult {
  projectionCursor: number;
  softFallback: SoftFallback | null;
  results: SearchHit[];
  /**
   * 0.8.8 EXP-OBS (Slice 10) — opt-in explanation sidecar; `null` unless
   * `search(..., { explain: true })`.
   */
  explanation: Explanation | null;
}

export interface MigrationStepReport {
  readonly stepId: number;
  readonly durationMs: number | null;
  readonly failed: boolean;
}

export interface EmbedderIdentity {
  readonly name: string;
  readonly revision: string;
  readonly dimension: number;
}

/**
 * EU-6 FIX-2 — discriminated-union shape for `OpenReport.embedderEvents`.
 *
 * Each variant interface carries a closed `kind` literal + the
 * variant-specific payload fields (non-optional). Callers pattern-match
 * with `if (event.kind === "...")` and tsc narrows the payload access
 * accordingly. See `dev/design/0.7.1-EU-6-FIX-2-design.md` §6.3.
 */
export interface DefaultEmbedderDownloadEvent {
  readonly kind: "DefaultEmbedderDownload";
  readonly file: string;
  readonly url: string;
  readonly bytes: number;
  readonly sha256: string;
  readonly cachePath: string;
  readonly durationMs: number;
}

export interface DefaultEmbedderCacheHitEvent {
  readonly kind: "DefaultEmbedderCacheHit";
  readonly file: string;
  readonly sha256: string;
  readonly cachePath: string;
}

export interface MeanVecPinnedEvent {
  readonly kind: "MeanVecPinned";
  readonly dim: number;
  readonly docCount: number;
}

/**
 * Forward-compat fallback for `kind` values not known to this build.
 * Part of the public `EmbedderEvent` union for soundness: a future or
 * replaced native extension may emit kinds this build does not know
 * about, and exposing them under a typed fallback member is more honest
 * than pretending the runtime is exhaustive at compile time. Because
 * `kind` here is the open type `string`, tsc cannot exclude this member
 * purely from a literal `event.kind === "..."` check on the bare union
 * — wrap such checks in {@link isKnownEmbedderEvent} first to recover
 * precise narrowing on the three known variants.
 */
export interface UnknownEmbedderEvent {
  readonly kind: string;
  readonly [field: string]: unknown;
}

export type EmbedderEvent =
  | DefaultEmbedderDownloadEvent
  | DefaultEmbedderCacheHitEvent
  | MeanVecPinnedEvent
  | UnknownEmbedderEvent;

/**
 * Type guard that narrows an {@link EmbedderEvent} to the three known
 * variants, excluding {@link UnknownEmbedderEvent}. Use as a gate before
 * discriminating on `event.kind`:
 *
 * ```ts
 * if (isKnownEmbedderEvent(event)) {
 *   if (event.kind === "DefaultEmbedderDownload") {
 *     const bytes: number = event.bytes; // narrowed precisely
 *   }
 * }
 * ```
 *
 * Without this guard, the open `kind: string` on `UnknownEmbedderEvent`
 * prevents tsc from removing it from the union on a literal-equality
 * check, so payload field access widens to `unknown`.
 */
export function isKnownEmbedderEvent(
  event: EmbedderEvent,
): event is
  | DefaultEmbedderDownloadEvent
  | DefaultEmbedderCacheHitEvent
  | MeanVecPinnedEvent {
  return (
    event.kind === "DefaultEmbedderDownload" ||
    event.kind === "DefaultEmbedderCacheHit" ||
    event.kind === "MeanVecPinned"
  );
}

/**
 * @internal — maps the wide napi-rs `NativeEmbedderEvent` into the
 * narrow discriminated `EmbedderEvent` union at the binding → SDK
 * seam. The non-null assertions are sound under the Rust emitter
 * invariant codified by AC-FIX2-6's runtime shape consistency test:
 * for each known `kind`, the emitter populates exactly the variant-
 * appropriate fields. Unknown `kind` values pass through as
 * `UnknownEmbedderEvent` so a forward-compatible variant addition
 * remains a strict refinement, not a breaking change.
 */
export function mapEmbedderEvent(n: NativeEmbedderEvent): EmbedderEvent {
  switch (n.kind) {
    case "DefaultEmbedderDownload":
      return {
        kind: "DefaultEmbedderDownload",
        file: n.file!,
        url: n.url!,
        bytes: n.bytes!,
        sha256: n.sha256!,
        cachePath: n.cachePath!,
        durationMs: n.durationMs!,
      };
    case "DefaultEmbedderCacheHit":
      return {
        kind: "DefaultEmbedderCacheHit",
        file: n.file!,
        sha256: n.sha256!,
        cachePath: n.cachePath!,
      };
    case "MeanVecPinned":
      return {
        kind: "MeanVecPinned",
        dim: n.dim!,
        docCount: n.docCount!,
      };
    default: {
      // Forward-compat: surface unknown kinds verbatim, dropping any
      // nullish wide-shape fields so the resulting object has only the
      // keys the emitter actually populated. `UnknownEmbedderEvent` is
      // part of the declared `EmbedderEvent` union, so no cast through
      // `unknown` is required — callers recover precise narrowing on
      // the known variants via `isKnownEmbedderEvent`.
      const out: Record<string, unknown> = { kind: n.kind };
      for (const [k, v] of Object.entries(n)) {
        if (k !== "kind" && v !== null && v !== undefined) out[k] = v;
      }
      return out as UnknownEmbedderEvent;
    }
  }
}

export interface OpenReport {
  readonly schemaVersionBefore: number;
  readonly schemaVersionAfter: number;
  readonly migrationSteps: ReadonlyArray<MigrationStepReport>;
  readonly embedderWarmupMs: number;
  readonly queryBackend: string;
  readonly defaultEmbedder: EmbedderIdentity;
  /** EU-5b — wall-time ms the loader spent fetching default-embedder
   *  weights, or `null` on full cache hit / caller-supplied embedder. */
  readonly embedderDownloadMs: number | null;
  /** EU-5b — structured loader events (downloads, cache hits,
   *  mean-vec pin). */
  readonly embedderEvents: ReadonlyArray<EmbedderEvent>;
  /** EU-5b — static identity capability (mean-centering required for
   *  bge-small). */
  readonly embedderMeanCenteringRequired: boolean;
  /** EU-5a2 — dynamic workspace state (`mean_vec IS NOT NULL` after the
   *  256-doc threshold crossing). */
  readonly embedderMeanVecPinned: boolean;
  /** 0.8.18 Slice 5 (#5 vector-equivalence probe, R-VEQ-6) — `true` iff the
   *  open-time self-check found a vector-equivalence divergence and every
   *  vector-dependent arm now refuses at query time with
   *  `VectorEquivalenceMismatchError`. The `searchTextOnly` path stays
   *  serviceable. */
  readonly denseDisabled: boolean;
  /** R-VEQ-6 — reason for `denseDisabled`, or `null` when dense is healthy. */
  readonly denseDisabledReason: string | null;
}

export interface CounterSnapshot {
  queries: number;
  writes: number;
  writeRows: number;
  adminOps: number;
  cacheHit: number;
  cacheMiss: number;
}

export interface SubscriberEvent {
  [key: string]: unknown;
}

export type SubscriberCallback = (event: SubscriberEvent) => void;

export interface AttachSubscriberOptions {
  heartbeatIntervalMs?: number;
}

export interface AdminConfigureOptions {
  name: string;
  body: string;
}

async function intercept<T>(fn: () => Promise<T>): Promise<T> {
  try {
    return await fn();
  } catch (err) {
    rethrowTyped(err);
  }
}

function interceptSync<T>(fn: () => T): T {
  try {
    return fn();
  } catch (err) {
    rethrowTyped(err);
  }
}

/**
 * 0.8.8 Slice 15 — validate a relevance-label id array before the native call
 * (mirrors the Python `_validate_id_list` guard for cross-SDK parity). Ids are
 * non-negative integers (the stable `SearchHit.id` identity carrier).
 */
function validateIdArray(name: string, value: number[]): void {
  if (!Array.isArray(value)) {
    throw new TypeError(`${name} must be an array of non-negative integers`);
  }
  for (const item of value) {
    if (!Number.isInteger(item)) {
      throw new RangeError(
        `${name} must contain only integers, got ${typeof item}`,
      );
    }
    if (item < 0) {
      throw new RangeError(
        `${name} must contain only non-negative integers, got ${item}`,
      );
    }
  }
}

export class Engine {
  readonly #native: NativeEngine;
  readonly config: EngineConfig;

  private constructor(inner: NativeEngine, config: EngineConfig) {
    this.#native = inner;
    this.config = config;
  }

  static async open(path: string, options: EngineOpenOptions = {}): Promise<Engine> {
    validateFfiString(path);
    const inner = await intercept(() => native.Engine.open(path, options));
    return new Engine(inner, options.engineConfig ?? {});
  }

  async write(batch: unknown[] = []): Promise<WriteReceipt> {
    validateFfiTree(batch);
    return intercept(() => this.#native.write(batch));
  }

  async search(
    query: string,
    filter?: SearchFilter | Filter,
    rerankDepth?: number,
    useGraphArm?: boolean,
    alpha?: number,
    poolN?: number,
    explain?: boolean,
  ): Promise<SearchResult> {
    validateFfiString(query);
    // 0.8.11 Slice 40 (#17) — accept the unified Filter on the vec0 search path;
    // lower to the SearchFilter sugar (typed-rejects a `json` term, D3).
    if (isUnifiedFilter(filter)) {
      filter = filterToSearchFilter(filter);
    }
    // G10 filter strings cross the FFI like `query` and must clear the same
    // AC-068a/AC-068b guard. napi-rs lossily replaces lone UTF-16 surrogates
    // with U+FFFD before the Rust-side guard runs (see validation.ts), so —
    // exactly like write/configure — the surrogate check must happen JS-side.
    // `createdAfter` is numeric (no string validation).
    if (filter !== undefined) {
      if (filter.sourceType !== undefined) validateFfiString(filter.sourceType);
      if (filter.kind !== undefined) validateFfiString(filter.kind);
      if (filter.status !== undefined) validateFfiString(filter.status);
    }
    // 0.8.1 R1: rerankDepth validation (must be a non-negative integer <= u32::MAX).
    // FIX-5: changed TypeError → RangeError for non-integer (consistency with
    //   validateLimit and graph depth checks).
    // FIX-5: added u32::MAX upper-bound guard (napi_get_value_uint32 wraps mod 2^32).
    // FIX-7: removed `?? undefined` no-op (rerankDepth is already `number | undefined`).
    if (rerankDepth !== undefined) {
      if (!Number.isInteger(rerankDepth)) {
        throw new RangeError(
          `rerankDepth must be an integer, got ${typeof rerankDepth}`,
        );
      }
      if (rerankDepth < 0) {
        throw new RangeError(`rerankDepth must be >= 0, got ${rerankDepth}`);
      }
      if (rerankDepth > 0xFFFFFFFF) {
        throw new RangeError(
          `rerankDepth must be <= 4294967295 (u32 max), got ${rerankDepth}`,
        );
      }
    }
    // 0.8.1 R3 (Slice 30): useGraphArm validation.
    if (useGraphArm !== undefined && typeof useGraphArm !== "boolean") {
      throw new TypeError(
        `useGraphArm must be a boolean, got ${typeof useGraphArm}`,
      );
    }
    // 0.8.5 (EXP-0): alpha is a finite number (clamped to [0,1] in the engine);
    // poolN is a non-negative integer <= u32::MAX (mirrors the rerankDepth guard).
    if (alpha !== undefined && (typeof alpha !== "number" || !Number.isFinite(alpha))) {
      throw new RangeError(`alpha must be a finite number, got ${alpha}`);
    }
    if (poolN !== undefined) {
      if (!Number.isInteger(poolN)) {
        throw new RangeError(`poolN must be an integer, got ${typeof poolN}`);
      }
      if (poolN < 0) {
        throw new RangeError(`poolN must be >= 0, got ${poolN}`);
      }
      if (poolN > 0xFFFFFFFF) {
        throw new RangeError(
          `poolN must be <= 4294967295 (u32 max), got ${poolN}`,
        );
      }
    }
    // 0.8.8 EXP-OBS (Slice 10): explain validation (mirrors useGraphArm + the
    // Python `search` guard, cross-SDK parity).
    if (explain !== undefined && typeof explain !== "boolean") {
      throw new TypeError(`explain must be a boolean, got ${typeof explain}`);
    }
    const r = await intercept(() =>
      this.#native.search(query, filter, rerankDepth, useGraphArm, alpha, poolN, explain),
    );
    const branch = r.softFallback?.branch;
    // 0.8.8 EXP-OBS: map the opt-in explanation sidecar; `null` (default) stays null.
    const e = r.explanation;
    const explanation: Explanation | null = e
      ? {
          trace: {
            queryChars: e.trace.queryChars,
            k: e.trace.k,
            rerankDepth: e.trace.rerankDepth,
            poolN: e.trace.poolN,
            alpha: e.trace.alpha,
            useGraphArm: e.trace.useGraphArm,
            recency: e.trace.recency,
            embedderId: e.trace.embedderId,
            ceActive: e.trace.ceActive,
            vectorHits: e.trace.vectorHits,
            textHits: e.trace.textHits,
            graphHits: e.trace.graphHits,
          },
          perHit: e.perHit.map(mapPerHitExplain),
        }
      : null;
    return {
      projectionCursor: r.projectionCursor,
      softFallback:
        branch === "vector" || branch === "text" || branch === "text_edge" || branch === "graph_arm"
          ? { branch: branch as SoftFallbackBranch }
          : null,
      results: r.results.map((h) => ({
        id: { space: h.id.space, value: h.id.value },
        kind: h.kind,
        body: h.body,
        score: h.score,
        branch: (h.branch === "vector" || h.branch === "text_edge" || h.branch === "graph_arm")
          ? (h.branch as SoftFallbackBranch)
          : "text",
        sourceId: h.sourceId ?? null,
        ceScore: h.ceScore ?? null,
      })),
      explanation,
    };
  }

  /**
   * 0.8.18 Slice 5 (#5 vector-equivalence probe) — the explicit text-only /
   * FTS-only search path. It does NOT embed the query and NEVER throws
   * `VectorEquivalenceMismatchError`, so it stays serviceable when the engine
   * opened in the degraded `denseDisabled` state. Returns node-body FTS hits
   * only (no vector recall, no CE rerank, no graph arm).
   */
  async searchTextOnly(query: string): Promise<SearchResult> {
    validateFfiString(query);
    const r = await intercept(() => this.#native.searchTextOnly(query));
    const branch = r.softFallback?.branch;
    return {
      projectionCursor: r.projectionCursor,
      softFallback:
        branch === "vector" || branch === "text" || branch === "text_edge" || branch === "graph_arm"
          ? { branch: branch as SoftFallbackBranch }
          : null,
      results: r.results.map((h) => ({
        id: { space: h.id.space, value: h.id.value },
        kind: h.kind,
        body: h.body,
        score: h.score,
        branch: (h.branch === "vector" || h.branch === "text_edge" || h.branch === "graph_arm")
          ? (h.branch as SoftFallbackBranch)
          : "text",
        sourceId: h.sourceId ?? null,
        ceScore: h.ceScore ?? null,
      })),
      explanation: null,
    };
  }

  /**
   * 0.8.18 Slice 5 (R-VEQ-6) — `true` iff the engine opened degraded (the #5
   * self-check found a vector-equivalence divergence and every dense arm is
   * refusing). Mirrors `OpenReport.denseDisabled`.
   */
  denseDisabled(): boolean {
    return this.#native.denseDisabled();
  }

  /**
   * 0.8.18 Slice 5 (R-VEQ-6) — the human-readable reason for the degraded state,
   * or `null` when dense is healthy.
   */
  denseDisabledReason(): string | null {
    return this.#native.denseDisabledReason() ?? null;
  }

  /**
   * 0.8.18 Slice 5 (R-VEQ-6) — telemetry counter: query-time dense-arm refusals
   * raised because the engine opened degraded.
   */
  vectorEquivalenceRefusalCount(): number {
    return this.#native.vectorEquivalenceRefusalCount();
  }

  async close(): Promise<void> {
    await intercept(() => this.#native.close());
  }

  async drain(timeoutMs: number): Promise<void> {
    await intercept(() => this.#native.drain(timeoutMs));
  }

  /**
   * G11 (Slice 15) — BYO-LLM ingest. Spawns an external extraction harness
   * speaking the `fathomdb.extract.v1` NDJSON-over-stdio protocol, sends
   * documents for extraction, and writes the resulting entities and fact-edges.
   *
   * @param cmd - argv to spawn (first element = program, rest = args).
   * @param documents - array of `{ sourceDocId, body }` objects to extract from.
   */
  async ingestWithExtractor(
    cmd: string[],
    documents: ExtractDocument[],
  ): Promise<IngestWithExtractorReceipt> {
    // fix-28 [P2]: validate all user-controlled strings at the FFI boundary.
    for (const arg of cmd) validateFfiString(arg);
    for (const doc of documents) {
      validateFfiString(doc.sourceDocId);
      validateFfiString(doc.body);
    }
    const nativeDocs = documents.map((d) => ({ sourceDocId: d.sourceDocId, body: d.body }));
    const r = await intercept(() => this.#native.ingestWithExtractor(cmd, nativeDocs));
    return {
      nodesWritten: r.nodesWritten,
      edgesWritten: r.edgesWritten,
      docsProcessed: r.docsProcessed,
    };
  }

  /**
   * 0.8.12 Slice 15 (OPP-2) — BYO-LLM consolidation / recency. Spawns a
   * caller-supplied harness speaking the `fathomdb.consolidate.v1`
   * NDJSON-over-stdio protocol (the SAME transport as `ingestWithExtractor`).
   * For each `{ subjectLogicalId, relation }` axis FathomDB assembles the
   * competing fact-edge cluster deterministically and applies the harness
   * verdicts as supersession/recency METADATA — edge bodies are never rewritten
   * and no row is ever deleted (ADR-0.8.12 §2.1).
   *
   * @param cmd - argv to spawn (first element = program, rest = args).
   * @param axes - array of `{ subjectLogicalId, relation }` axes to consolidate.
   */
  async consolidateWithProvider(
    cmd: string[],
    axes: ConsolidateAxis[],
  ): Promise<ConsolidateReceipt> {
    // Validate all user-controlled strings at the FFI boundary.
    for (const arg of cmd) validateFfiString(arg);
    for (const axis of axes) {
      validateFfiString(axis.subjectLogicalId);
      validateFfiString(axis.relation);
    }
    const nativeAxes = axes.map((a) => ({
      subjectLogicalId: a.subjectLogicalId,
      relation: a.relation,
    }));
    const r = await intercept(() => this.#native.consolidateWithProvider(cmd, nativeAxes));
    return {
      clustersProcessed: r.clustersProcessed,
      edgesExamined: r.edgesExamined,
      edgesKept: r.edgesKept,
      edgesInvalidated: r.edgesInvalidated,
      edgesSuperseded: r.edgesSuperseded,
    };
  }

  /**
   * Embed `text` with the engine's pinned default embedder
   * (`fathomdb-bge-small-en-v1.5`) and return the raw vector.
   *
   * Read-path primitive (mirror of the Python `Engine.embed`) for callers
   * that need vectors under the engine's own embedder identity (e.g.
   * coverage-index clustering) rather than a parallel, possibly-divergent
   * embedder. Rejects with `FDB_EMBEDDER_NOT_CONFIGURED` if the engine was
   * opened without an embedder (`useDefaultEmbedder: false`).
   */
  async embed(text: string): Promise<number[]> {
    validateFfiString(text);
    return intercept(() => this.#native.embed(text));
  }

  /**
   * 0.8.8 Slice 15 (OPP-9) — enable opt-in local telemetry capture to a JSONL
   * `sinkPath`. Off by default; local file only (no egress). Once enabled, each
   * `search` records a query→result event keyed on the stable id, and
   * `recordFeedback` appends correlated agent labels. The query text and
   * `sourceId` are NEVER written (privacy, ADR §C).
   */
  async enableTelemetry(sinkPath: string): Promise<void> {
    validateFfiString(sinkPath);
    await intercept(() => this.#native.enableTelemetry(sinkPath));
  }

  /**
   * 0.8.8 Slice 15 — the most-recent captured `queryId` (for `recordFeedback`),
   * or `null` when telemetry is off / no query has been captured yet.
   */
  lastTelemetryQueryId(): string | null {
    return interceptSync(() => this.#native.lastTelemetryQueryId());
  }

  /**
   * 0.8.8 Slice 15 — attach agent relevance labels for a previously captured
   * `queryId`. `relevantIds` / `irrelevantIds` are the stable identity carrier
   * (== `SearchHit.id`); `labelSource` is the caller-declared label origin
   * (e.g. `"agent:hermes"`). Rejects when telemetry is off.
   */
  async recordFeedback(
    queryId: string,
    relevantIds: number[],
    irrelevantIds: number[],
    labelSource: string,
  ): Promise<void> {
    validateFfiString(queryId);
    validateFfiString(labelSource);
    validateIdArray("relevantIds", relevantIds);
    validateIdArray("irrelevantIds", irrelevantIds);
    await intercept(() =>
      this.#native.recordFeedback(queryId, relevantIds, irrelevantIds, labelSource),
    );
  }

  counters(): CounterSnapshot {
    return interceptSync(() => this.#native.counters());
  }

  openReport(): OpenReport {
    return interceptSync(() => {
      const r = this.#native.openReport();
      return {
        schemaVersionBefore: r.schemaVersionBefore,
        schemaVersionAfter: r.schemaVersionAfter,
        migrationSteps: r.migrationSteps,
        embedderWarmupMs: r.embedderWarmupMs,
        queryBackend: r.queryBackend,
        defaultEmbedder: r.defaultEmbedder,
        embedderDownloadMs: r.embedderDownloadMs,
        embedderEvents: r.embedderEvents.map(mapEmbedderEvent),
        embedderMeanCenteringRequired: r.embedderMeanCenteringRequired,
        embedderMeanVecPinned: r.embedderMeanVecPinned,
        denseDisabled: r.denseDisabled,
        denseDisabledReason: r.denseDisabledReason ?? null,
      };
    });
  }

  setProfiling(enabled: boolean): void {
    interceptSync(() => this.#native.setProfiling(enabled));
  }

  setSlowThresholdMs(value: number): void {
    interceptSync(() => this.#native.setSlowThresholdMs(value));
  }

  attachSubscriber(callback: SubscriberCallback, options: AttachSubscriberOptions = {}): void {
    interceptSync(() => this.#native.attachSubscriber(callback, options));
  }

  /** @internal — handle to the napi-rs binding, used by `admin.configure`. */
  get _native(): NativeEngine {
    return this.#native;
  }
}

export const admin = {
  async configure(engine: Engine, options: AdminConfigureOptions): Promise<WriteReceipt> {
    validateFfiString(options.name);
    validateFfiString(options.body);
    return intercept(() => native.adminConfigure(engine._native, options));
  },
};

// ===== Slice 20 (G5/G6) — graph traversal ================================

/**
 * Slice 20 (G6) — one node reached by BFS traversal in `graph.searchExpand`.
 *
 * `hopCount` is the BFS distance from the nearest search-hit root. Only nodes
 * NOT already in the search-hit set appear in `SearchExpandResult.expanded`
 * (deduplication: search score takes priority).
 */
export interface ExpandedNode {
  node: NodeRecord;
  hopCount: number;
}

/**
 * Slice 20 (G6) — result of `graph.searchExpand`.
 *
 * `searchHits` — original RRF-scored results from the search step.
 * `expanded`   — nodes reachable from any search hit within `depth` hops
 *                that are NOT in `searchHits`.
 * `allLogicalIds` — deduplicated union of both sets.
 */
export interface SearchExpandResult {
  searchHits: SearchHit[];
  expanded: ExpandedNode[];
  allLogicalIds: string[];
}

/** Direction to follow when traversing `canonical_edges`. */
export type TraversalDirection = "outgoing" | "incoming" | "both";

export const graph = {
  /**
   * G5 — bounded BFS from `logicalId` over `canonical_edges`.
   *
   * `depth` must be 1–3; rejects depth > 3 with `InvalidArgumentError`.
   * `direction` is `"outgoing"`, `"incoming"`, or `"both"`.
   * Returns up to 50 `NodeRecord`s reachable within `depth` hops (root excluded).
   * Edges with `t_invalid` in the past are not traversed (valid-time filter).
   */
  async neighbors(
    engine: Engine,
    logicalId: string,
    depth: number,
    direction: TraversalDirection = "both",
  ): Promise<NodeRecord[]> {
    validateFfiString(logicalId);
    if (!Number.isInteger(depth) || depth < 1 || depth > 3) {
      throw new InvalidArgumentError(
        `graph.neighbors depth must be an integer between 1 and 3; got ${depth}`,
      );
    }
    return intercept(() => native.graphNeighbors(engine._native, logicalId, depth, direction));
  },

  /**
   * G6 — FTS/vector search followed by bounded BFS expansion.
   *
   * Runs `engine.search(query, filter)` (G1), then expands each hit via
   * `graph.neighbors(depth, "both")`. Nodes appearing in both the search hit
   * set and the traversal reach appear only in `searchHits` (deduplication).
   *
   * `depth` must be 0–3; 0 skips expansion. Raises `InvalidArgumentError` for depth > 3.
   */
  async searchExpand(
    engine: Engine,
    query: string,
    depth: number,
    filter?: SearchFilter,
  ): Promise<SearchExpandResult> {
    validateFfiString(query);
    if (!Number.isInteger(depth) || depth < 0 || depth > 3) {
      throw new InvalidArgumentError(
        `graph.searchExpand depth must be an integer between 0 and 3; got ${depth}`,
      );
    }
    if (filter?.sourceType !== undefined) validateFfiString(filter.sourceType);
    if (filter?.kind !== undefined) validateFfiString(filter.kind);
    if (filter?.status !== undefined) validateFfiString(filter.status);
    const r = await intercept(() =>
      native.searchExpand(
        engine._native,
        query,
        depth,
        filter?.sourceType,
        filter?.kind,
        filter?.createdAfter,
        filter?.status,
      ),
    );
    return {
      searchHits: r.searchHits.map((h) => ({
        id: { space: h.id.space, value: h.id.value },
        kind: h.kind,
        body: h.body,
        score: h.score,
        branch: (h.branch === "vector" || h.branch === "text_edge")
          ? (h.branch as SoftFallbackBranch)
          : "text",
        sourceId: h.sourceId ?? null,
        // 0.8.5 — searchExpand never reranks (depth=0) → ceScore is always null.
        ceScore: h.ceScore ?? null,
      })),
      expanded: r.expanded.map((e) => ({
        node: e.node,
        hopCount: e.hopCount,
      })),
      allLogicalIds: r.allLogicalIds,
    };
  },
};
