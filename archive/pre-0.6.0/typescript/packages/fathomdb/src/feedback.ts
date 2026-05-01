import { randomUUID } from "node:crypto";
import { performance } from "node:perf_hooks";
import type { FeedbackConfig, ProgressCallback, ResponseCycleEvent, ResponseCyclePhase } from "./types.js";

/** Surface identifier included in feedback events to distinguish the TypeScript SDK. */
export const SURFACE = "typescript";

const DEFAULT_SLOW_THRESHOLD_MS = 500;

/**
 * Wrap a synchronous operation with response-cycle feedback events.
 *
 * If `progressCallback` is undefined, the operation is called directly.
 * Otherwise emits STARTED before the call, and FINISHED or FAILED after.
 *
 * Note: Since native engine calls are synchronous and block the Node.js
 * event loop, SLOW/HEARTBEAT events cannot fire during the operation.
 * Only STARTED and FINISHED/FAILED are emitted in the current synchronous
 * binding. The timer infrastructure will activate once async bindings
 * are available.
 *
 * @param opts - Feedback wrapper options.
 * @param opts.operationKind - Identifies the operation (e.g. `"engine.open"`, `"query.execute"`).
 * @param opts.metadata - Arbitrary key-value metadata included in every emitted event.
 * @param opts.progressCallback - Optional callback invoked with {@link ResponseCycleEvent} instances.
 * @param opts.feedbackConfig - Timing thresholds controlling slow/heartbeat emission.
 * @param opts.operation - The synchronous operation to execute.
 * @returns The return value of `opts.operation`.
 */
export function runWithFeedback<T>(opts: {
  operationKind: string;
  metadata: Record<string, string>;
  progressCallback: ProgressCallback | undefined;
  feedbackConfig: FeedbackConfig | undefined;
  operation: () => T;
}): T {
  if (!opts.progressCallback) {
    return opts.operation();
  }

  const callback = opts.progressCallback;
  const slowThresholdMs = opts.feedbackConfig?.slowThresholdMs ?? DEFAULT_SLOW_THRESHOLD_MS;
  const operationId = randomUUID().replace(/-/g, "");
  const startedAt = performance.now();
  let callbackDisabled = false;

  function emit(phase: ResponseCyclePhase, errorCode?: string, errorMessage?: string): void {
    if (callbackDisabled) return;
    const event: ResponseCycleEvent = {
      operationId,
      operationKind: opts.operationKind,
      surface: SURFACE,
      phase,
      elapsedMs: Math.round(performance.now() - startedAt),
      slowThresholdMs,
      metadata: opts.metadata,
      errorCode,
      errorMessage,
    };
    try {
      callback(event);
    } catch {
      callbackDisabled = true;
    }
  }

  emit("started");
  try {
    const result = opts.operation();
    emit("finished");
    return result;
  } catch (error) {
    const name = error instanceof Error ? error.constructor.name : "Error";
    const message = error instanceof Error ? error.message : String(error);
    emit("failed", name, message);
    throw error;
  }
}
