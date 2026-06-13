// FathomDB TypeScript SDK public surface.
//
// Five-verb top-level surface (Engine.open, engine.write, engine.search,
// engine.close, admin.configure), engine-attached instrumentation, and
// the FathomDbError leaf-class hierarchy per
// `dev/interfaces/typescript.md` and `dev/design/bindings.md` § 3. The
// runtime is the napi-rs binding in `fathomdb-napi`; this file is a
// thin TS wrapper that funnels every native error through
// `rethrowTyped`.

import { native, type NativeEmbedderEvent, type NativeEngine } from "./binding.js";
import { rethrowTyped } from "./errors.js";
import { validateFfiString, validateFfiTree } from "./validation.js";

export * from "./errors.js";
export { read } from "./read.js";
export type { NodeRecord, OpStoreRow, ReadCollectionOptions } from "./read.js";

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

export type SoftFallbackBranch = "vector" | "text" | "text_edge";

export interface SoftFallback {
  branch: SoftFallbackBranch;
}

export interface SearchHit {
  /** Canonical row `write_cursor` (interim identity carrier). */
  id: number;
  kind: string;
  body: string;
  /**
   * G9 RRF-fused relevance (`Σ 1/(RRF_K + rank)`; higher = more relevant),
   * optionally recency-reweighted. Raw `vec_distance_l2`/`bm25()` are fused on
   * rank, never compared raw.
   */
  score: number;
  branch: SoftFallbackBranch;
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

export interface SearchResult {
  projectionCursor: number;
  softFallback: SoftFallback | null;
  results: SearchHit[];
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
    filter?: SearchFilter,
  ): Promise<SearchResult> {
    validateFfiString(query);
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
    const r = await intercept(() => this.#native.search(query, filter));
    const branch = r.softFallback?.branch;
    return {
      projectionCursor: r.projectionCursor,
      softFallback:
        branch === "vector" || branch === "text" || branch === "text_edge"
          ? { branch: branch as SoftFallbackBranch }
          : null,
      results: r.results.map((h) => ({
        id: h.id,
        kind: h.kind,
        body: h.body,
        score: h.score,
        branch: (h.branch === "vector" || h.branch === "text_edge")
          ? (h.branch as SoftFallbackBranch)
          : "text",
      })),
    };
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
    const nativeDocs = documents.map((d) => ({ sourceDocId: d.sourceDocId, body: d.body }));
    const r = await intercept(() => this.#native.ingestWithExtractor(cmd, nativeDocs));
    return {
      nodesWritten: r.nodesWritten,
      edgesWritten: r.edgesWritten,
      docsProcessed: r.docsProcessed,
    };
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
