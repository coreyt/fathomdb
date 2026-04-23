import { AdminClient } from "./admin.js";
import { callNative, FathomError, mapNativeError, parseNativeJson } from "./errors.js";
import { runWithFeedback } from "./feedback.js";
import { loadNativeBinding, type NativeBinding, type NativeEngineCore } from "./native.js";
import { FallbackSearchBuilder, Query } from "./query.js";
import {
  lastAccessTouchReportFromWire,
  telemetrySnapshotFromWire,
  writeReceiptFromWire,
  type EngineOpenOptions,
  type FeedbackConfig,
  type LastAccessTouchReport,
  type LastAccessTouchRequest,
  type ProgressCallback,
  type TelemetrySnapshot,
  type WriteReceipt,
  type WriteRequest,
} from "./types.js";

/**
 * Entry point for interacting with a fathomdb database.
 *
 * Use {@link Engine.open} to create an instance, then call {@link Engine.nodes}
 * to build queries or {@link Engine.write} to submit mutations. Administrative
 * operations are available via the {@link Engine.admin} property.
 */
export class Engine {
  static #binding: NativeBinding | null = null;

  /**
   * Open a fathomdb database at the given path.
   *
   * @param databasePath - Path to the SQLite database file.
   * @param options - Engine configuration options (provenance mode, vector dimension, telemetry level).
   * @param progressCallback - Optional callback invoked with {@link ResponseCycleEvent} instances during long operations.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns A new Engine instance connected to the database.
   * @throws {@link FathomError} If the database cannot be opened or schema bootstrap fails.
   */
  static open(
    databasePath: string,
    options: EngineOpenOptions = {},
    progressCallback?: ProgressCallback,
    feedbackConfig?: FeedbackConfig,
  ): Engine {
    const binding = this.#binding ?? (this.#binding = loadNativeBinding());
    return runWithFeedback({
      operationKind: "engine.open",
      metadata: { database_path: databasePath },
      progressCallback,
      feedbackConfig,
      operation: () => {
        try {
          const core = binding.EngineCore.open(
            databasePath,
            options.provenanceMode ?? "warn",
            options.vectorDimension,
            options.telemetryLevel,
            options.embedder,
            options.autoDrainVector ?? false
          );
          return new Engine(core);
        } catch (error) {
          throw mapNativeError(error);
        }
      },
    });
  }

  /**
   * Override the native binding used by the engine. For testing only.
   *
   * @param binding - The native binding to use, or `null` to reset to auto-detection.
   */
  static setBindingForTests(binding: NativeBinding | null): void {
    this.#binding = binding;
  }

  readonly #core: NativeEngineCore;
  #closed = false;
  readonly admin: AdminClient;

  /**
   * Create an Engine wrapping the given native core. Use {@link Engine.open} instead.
   *
   * @param core - The native engine core handle.
   */
  constructor(core: NativeEngineCore) {
    this.#core = core;
    this.admin = new AdminClient(core);
  }

  /**
   * Close the engine, flushing pending writes and releasing resources.
   *
   * Idempotent -- safe to call multiple times.
   */
  close(): void {
    if (this.#closed) {
      return;
    }
    try {
      this.#core.close();
      this.#closed = true;
    } catch (error) {
      throw mapNativeError(error);
    }
  }

  /**
   * Read all telemetry counters and SQLite cache statistics.
   *
   * Returns a point-in-time snapshot with cumulative counters since engine open.
   * All SQLite cache counters are aggregated across the reader connection pool.
   * This method is safe to call from any context at any time.
   *
   * @returns Cumulative counters and cache statistics.
   */
  telemetrySnapshot(): TelemetrySnapshot {
    this.#assertOpen();
    return telemetrySnapshotFromWire(parseNativeJson(callNative(() => this.#core.telemetrySnapshot())));
  }

  /**
   * Start building a query rooted at nodes of the given kind.
   *
   * @param kind - The node kind to query.
   * @returns A new {@link Query} builder.
   */
  nodes(kind: string): Query {
    this.#assertOpen();
    return new Query(this.#core, kind);
  }

  /**
   * Alias for {@link Engine.nodes}.
   *
   * @param kind - The node kind to query.
   * @returns A new {@link Query} builder.
   */
  query(kind: string): Query {
    return this.nodes(kind);
  }

  /**
   * Start an explicit two-shape fallback search.
   *
   * Unlike the adaptive {@link Query.textSearch} path, the caller supplies
   * both the strict and relaxed branches directly. Pass `null` for
   * `relaxedQuery` to run the strict branch only.
   *
   * @param strictQuery - Strict branch query text.
   * @param relaxedQuery - Optional relaxed branch query text.
   * @param limit - Maximum number of candidate hits to return.
   * @returns A new {@link FallbackSearchBuilder} tethered to the engine core.
   */
  fallbackSearch(
    strictQuery: string,
    relaxedQuery: string | null,
    limit: number,
  ): FallbackSearchBuilder {
    this.#assertOpen();
    // rootKind is intentionally empty: fallback_search is kind-agnostic
    // by design. Callers scope by kind via `.filterKindEq(k)`, which
    // fuses through the filter list at plan-compile time.
    return new FallbackSearchBuilder(this.#core, "", strictQuery, relaxedQuery, limit);
  }

  /**
   * Submit a write request (nodes, edges, chunks, etc.) to the database.
   *
   * @param request - The write request to submit.
   * @param progressCallback - Optional callback invoked with feedback events.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns A {@link WriteReceipt} summarizing the committed changes.
   * @throws {@link FathomError} If the request contains invalid data or the write is rejected.
   */
  write(
    request: WriteRequest,
    progressCallback?: ProgressCallback,
    feedbackConfig?: FeedbackConfig,
  ): WriteReceipt {
    this.#assertOpen();
    return this.#run("write.submit", () =>
      writeReceiptFromWire(parseNativeJson(callNative(() => this.#core.submitWrite(JSON.stringify(request))))),
      progressCallback, feedbackConfig,
    );
  }

  /**
   * Alias for {@link Engine.write}.
   *
   * @param request - The write request to submit.
   * @param progressCallback - Optional callback invoked with feedback events.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns A {@link WriteReceipt} summarizing the committed changes.
   */
  submit(
    request: WriteRequest,
    progressCallback?: ProgressCallback,
    feedbackConfig?: FeedbackConfig,
  ): WriteReceipt {
    return this.write(request, progressCallback, feedbackConfig);
  }

  /**
   * Update the last-accessed timestamp for a set of nodes.
   *
   * @param request - Specifies which logical IDs to touch and the timestamp.
   * @param progressCallback - Optional callback invoked with feedback events.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns A report indicating how many nodes were touched.
   */
  touchLastAccessed(
    request: LastAccessTouchRequest,
    progressCallback?: ProgressCallback,
    feedbackConfig?: FeedbackConfig,
  ): LastAccessTouchReport {
    this.#assertOpen();
    const wire = {
      logical_ids: request.logicalIds,
      touched_at: request.touchedAt,
      source_ref: request.sourceRef ?? null,
    };
    return this.#run("write.touch_last_accessed", () =>
      lastAccessTouchReportFromWire(parseNativeJson(callNative(() => this.#core.touchLastAccessed(JSON.stringify(wire))))),
      progressCallback, feedbackConfig,
    );
  }

  #run<T>(operationKind: string, operation: () => T, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): T {
    return runWithFeedback({ operationKind, metadata: {}, progressCallback, feedbackConfig, operation });
  }

  #assertOpen(): void {
    if (this.#closed) {
      throw new FathomError("engine is closed");
    }
  }
}
