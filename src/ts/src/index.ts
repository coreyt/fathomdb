// FathomDB TypeScript SDK public surface.
//
// Five-verb top-level surface (Engine.open, engine.write, engine.search,
// engine.close, admin.configure), engine-attached instrumentation, and
// the FathomDbError leaf-class hierarchy per
// `dev/interfaces/typescript.md` and `dev/design/bindings.md` § 3. The
// runtime is the napi-rs binding in `fathomdb-napi`; this file is a
// thin TS wrapper that funnels every native error through
// `rethrowTyped`.

import { native, type NativeEngine } from "./binding.js";
import { rethrowTyped } from "./errors.js";
import { validateFfiString, validateFfiTree } from "./validation.js";

export * from "./errors.js";

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
}

export type SoftFallbackBranch = "vector" | "text";

export interface SoftFallback {
  branch: SoftFallbackBranch;
}

export interface SearchResult {
  projectionCursor: number;
  softFallback: SoftFallback | null;
  results: string[];
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
 * EU-6 — discriminated-union shape for `OpenReport.embedderEvents`.
 *
 * `kind` carries the variant name; the remaining optional fields carry
 * the variant payload. Callers pattern-match on `event.kind`.
 */
export interface EmbedderEvent {
  readonly kind:
    | "DefaultEmbedderDownload"
    | "DefaultEmbedderCacheHit"
    | "MeanVecPinned"
    | string;
  readonly file?: string | null;
  readonly url?: string | null;
  readonly bytes?: number | null;
  readonly sha256?: string | null;
  readonly cachePath?: string | null;
  readonly durationMs?: number | null;
  readonly dim?: number | null;
  readonly docCount?: number | null;
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

  async search(query: string): Promise<SearchResult> {
    validateFfiString(query);
    const r = await intercept(() => this.#native.search(query));
    const branch = r.softFallback?.branch;
    return {
      projectionCursor: r.projectionCursor,
      softFallback:
        branch === "vector" || branch === "text" ? { branch } : null,
      results: r.results,
    };
  }

  async close(): Promise<void> {
    await intercept(() => this.#native.close());
  }

  async drain(timeoutMs: number): Promise<void> {
    await intercept(() => this.#native.drain(timeoutMs));
  }

  counters(): CounterSnapshot {
    return interceptSync(() => this.#native.counters());
  }

  openReport(): OpenReport {
    return interceptSync(() => this.#native.openReport());
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
