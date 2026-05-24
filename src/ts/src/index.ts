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

export interface OpenReport {
  readonly schemaVersionBefore: number;
  readonly schemaVersionAfter: number;
  readonly migrationSteps: ReadonlyArray<MigrationStepReport>;
  readonly embedderWarmupMs: number;
  readonly queryBackend: string;
  readonly defaultEmbedder: EmbedderIdentity;
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
