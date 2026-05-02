// FathomDB TypeScript SDK public surface.
//
// Five-verb top-level surface, options.engineConfig camelCase knobs, and
// caller-visible result shapes per `dev/interfaces/typescript.md`. napi-rs
// wiring lands in a follow-up slice; the 0.6.0 surface-stub keeps the
// engine in pure TypeScript so parser and error-hierarchy tests run.

import { ClosingError } from "./errors.js";

export * from "./errors.js";

/**
 * Engine-owned runtime knobs.
 *
 * Field set mirrors the Python EngineConfig in camelCase per
 * `dev/interfaces/typescript.md` § Runtime surface.
 */
export interface EngineConfig {
  embedderPoolSize?: number;
  schedulerRuntimeThreads?: number;
  provenanceRowCap?: number;
  embedderCallTimeoutMs?: number;
  slowThresholdMs?: number;
}

/** Options object accepted by `Engine.open`. */
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

export interface CounterSnapshot {}

export interface SubscriberEvent {}

export type SubscriberCallback = (event: SubscriberEvent) => void;

export interface AttachSubscriberOptions {
  heartbeatIntervalMs?: number;
}

export interface AdminConfigureOptions {
  name: string;
  body: string;
}

export class Engine {
  readonly path: string;
  readonly config: EngineConfig;
  #cursor = 0;
  #closed = false;

  private constructor(path: string, config: EngineConfig) {
    this.path = path;
    this.config = config;
  }

  static async open(path: string, options: EngineOpenOptions = {}): Promise<Engine> {
    return new Engine(path, options.engineConfig ?? {});
  }

  async write(batch: unknown[] = []): Promise<WriteReceipt> {
    this.ensureOpen();
    this.#cursor += Math.max(batch.length, 1);
    return { cursor: this.#cursor };
  }

  async search(query: string): Promise<SearchResult> {
    this.ensureOpen();
    const normalized = query.trim();
    if (normalized.length === 0) {
      throw new Error("query must not be empty");
    }
    return {
      projectionCursor: this.#cursor,
      softFallback: null,
      results: [`rewrite scaffold query: ${normalized}`],
    };
  }

  async close(): Promise<void> {
    this.#closed = true;
  }

  async drain(timeoutMs: number): Promise<void> {
    void timeoutMs;
  }

  counters(): CounterSnapshot {
    return {};
  }

  async setProfiling(enabled: boolean): Promise<void> {
    void enabled;
  }

  async setSlowThresholdMs(value: number): Promise<void> {
    void value;
  }

  attachSubscriber(callback: SubscriberCallback, options: AttachSubscriberOptions = {}): void {
    void callback;
    void options;
  }

  /** @internal stub hook used by `admin.configure`. */
  recordAdminConfigure(_options: AdminConfigureOptions): WriteReceipt {
    this.ensureOpen();
    this.#cursor += 1;
    return { cursor: this.#cursor };
  }

  private ensureOpen(): void {
    if (this.#closed) {
      throw new ClosingError("engine is closed");
    }
  }
}

/**
 * Admin namespace exposing the fifth canonical SDK verb.
 *
 * Per `dev/interfaces/typescript.md` § Runtime surface, `admin.configure` is
 * the cross-binding admin entry point. The 0.6.0 stub does not commit
 * anything to a real engine.
 */
export const admin = {
  async configure(engine: Engine, options: AdminConfigureOptions): Promise<WriteReceipt> {
    if (!options.name) {
      throw new Error("admin.configure requires a non-empty name");
    }
    return engine.recordAdminConfigure(options);
  },
};
