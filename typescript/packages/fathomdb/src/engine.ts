import { AdminClient } from "./admin.js";
import { FathomError, mapNativeError, parseNativeJson } from "./errors.js";
import { loadNativeBinding, type NativeBinding, type NativeEngineCore } from "./native.js";
import { Query } from "./query.js";
import {
  lastAccessTouchReportFromWire,
  telemetrySnapshotFromWire,
  writeReceiptFromWire,
  type EngineOpenOptions,
  type LastAccessTouchReport,
  type LastAccessTouchRequest,
  type TelemetrySnapshot,
  type WriteReceipt,
  type WriteRequest,
} from "./types.js";

export class Engine {
  static #binding: NativeBinding | null = null;

  static open(databasePath: string, options: EngineOpenOptions = {}): Engine {
    const binding = this.#binding ?? (this.#binding = loadNativeBinding());
    try {
      const core = binding.EngineCore.open(
        databasePath,
        options.provenanceMode ?? "warn",
        options.vectorDimension,
        options.telemetryLevel
      );
      return new Engine(core);
    } catch (error) {
      throw mapNativeError(error);
    }
  }

  static setBindingForTests(binding: NativeBinding | null): void {
    this.#binding = binding;
  }

  readonly #core: NativeEngineCore;
  #closed = false;
  readonly admin: AdminClient;

  constructor(core: NativeEngineCore) {
    this.#core = core;
    this.admin = new AdminClient(core);
  }

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

  telemetrySnapshot(): TelemetrySnapshot {
    this.#assertOpen();
    return telemetrySnapshotFromWire(parseNativeJson(this.#core.telemetrySnapshot()));
  }

  nodes(kind: string): Query {
    this.#assertOpen();
    return new Query(this.#core, kind);
  }

  query(kind: string): Query {
    return this.nodes(kind);
  }

  write(request: WriteRequest): WriteReceipt {
    this.#assertOpen();
    return writeReceiptFromWire(parseNativeJson(this.#core.submitWrite(JSON.stringify(request))));
  }

  submit(request: WriteRequest): WriteReceipt {
    return this.write(request);
  }

  touchLastAccessed(request: LastAccessTouchRequest): LastAccessTouchReport {
    this.#assertOpen();
    const wire = {
      logical_ids: request.logicalIds,
      touched_at: request.touchedAt,
      source_ref: request.sourceRef ?? null,
    };
    return lastAccessTouchReportFromWire(
      parseNativeJson(this.#core.touchLastAccessed(JSON.stringify(wire)))
    );
  }

  #assertOpen(): void {
    if (this.#closed) {
      throw new FathomError("engine is closed");
    }
  }
}
