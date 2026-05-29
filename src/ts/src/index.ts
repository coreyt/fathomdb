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
 * Not included in the `EmbedderEvent` discriminated union — including
 * an open-`kind: string` member would defeat literal narrowing on the
 * known variants. Instead, `mapEmbedderEvent()` returns a value of this
 * shape under an unsafe cast to `EmbedderEvent` for unknown kinds; user
 * code that does NOT match any known `case` in its switch / if-chain
 * lands in the `default` / final `else` branch at runtime and can
 * pattern-match on `event.kind` further if needed.
 */
export interface UnknownEmbedderEvent {
  readonly kind: string;
  readonly [field: string]: unknown;
}

export type EmbedderEvent =
  | DefaultEmbedderDownloadEvent
  | DefaultEmbedderCacheHitEvent
  | MeanVecPinnedEvent;

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
      // keys the emitter actually populated. Cast through `unknown`
      // because `UnknownEmbedderEvent` is not part of the declared
      // `EmbedderEvent` union (keeping it out preserves literal
      // narrowing on the known variants).
      const out: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(n)) {
        if (v !== null && v !== undefined) out[k] = v;
      }
      return out as unknown as EmbedderEvent;
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
