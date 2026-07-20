// Single-rooted FathomDbError hierarchy for the TypeScript SDK.
//
// Layout owned by `dev/design/errors.md` § Binding-facing class matrix
// and `dev/design/bindings.md` § 3. Per-leaf payloads are typed object
// arguments; callers narrow with `instanceof`.
//
// The napi-rs binding cannot throw a Rust-defined JS class with a TS
// `instanceof` chain. Instead, it throws `Error` whose `message` is a
// JSON envelope `{ code, message, payload }`. `rethrowTyped` parses
// that envelope and constructs the matching leaf class so `instanceof
// FathomDbError` narrowing works on the TS side.

export class FathomDbError extends Error {
  constructor(message: string) {
    super(message);
    this.name = new.target.name;
  }
}

export class StorageError extends FathomDbError {}
export class ProjectionError extends FathomDbError {}
export class VectorError extends FathomDbError {}
export class EmbedderError extends FathomDbError {}
export class SchedulerError extends FathomDbError {}
export class OpStoreError extends FathomDbError {}
export class WriteValidationError extends FathomDbError {}
export class SchemaValidationError extends FathomDbError {}
export class OverloadedError extends FathomDbError {}
export class ClosingError extends FathomDbError {}

export class EmbedderNotConfiguredError extends EmbedderError {}
export class KindNotVectorIndexedError extends VectorError {}

export interface DatabaseLockedErrorPayload {
  holderPid?: number;
}

export class DatabaseLockedError extends FathomDbError {
  readonly holderPid?: number;
  constructor(payload: DatabaseLockedErrorPayload = {}) {
    super(`database locked (holderPid=${payload.holderPid ?? "unknown"})`);
    this.holderPid = payload.holderPid;
  }
}

export interface CorruptionErrorPayload {
  kind: string;
  stage: string;
  recoveryHintCode: string;
  docAnchor: string;
}

export class CorruptionError extends FathomDbError {
  readonly kind: string;
  readonly stage: string;
  readonly recoveryHintCode: string;
  readonly docAnchor: string;
  constructor(payload: CorruptionErrorPayload) {
    super(`corruption ${payload.kind} at stage ${payload.stage} (${payload.recoveryHintCode})`);
    this.kind = payload.kind;
    this.stage = payload.stage;
    this.recoveryHintCode = payload.recoveryHintCode;
    this.docAnchor = payload.docAnchor;
  }
}

export class IncompatibleSchemaVersionError extends FathomDbError {}
export class MigrationError extends FathomDbError {}

export interface EmbedderIdentityMismatchPayload {
  storedName: string;
  storedRevision: string;
  suppliedName: string;
  suppliedRevision: string;
}

export class EmbedderIdentityMismatchError extends FathomDbError {
  readonly storedName: string;
  readonly storedRevision: string;
  readonly suppliedName: string;
  readonly suppliedRevision: string;
  constructor(payload: EmbedderIdentityMismatchPayload) {
    super(
      `embedder identity mismatch: stored ${payload.storedName}@${payload.storedRevision}, ` +
        `supplied ${payload.suppliedName}@${payload.suppliedRevision}`,
    );
    this.storedName = payload.storedName;
    this.storedRevision = payload.storedRevision;
    this.suppliedName = payload.suppliedName;
    this.suppliedRevision = payload.suppliedRevision;
  }
}

export interface EmbedderDimensionMismatchPayload {
  stored: number;
  supplied: number;
}

export class EmbedderDimensionMismatchError extends FathomDbError {
  readonly stored: number;
  readonly supplied: number;
  constructor(payload: EmbedderDimensionMismatchPayload) {
    super(
      `embedder vector dimension mismatch: stored ${payload.stored}, supplied ${payload.supplied}`,
    );
    this.stored = payload.stored;
    this.supplied = payload.supplied;
  }
}

// G11 (Slice 15) — BYO-LLM extraction harness protocol error.
export class ExtractorError extends FathomDbError {}

// 0.8.12 Slice 15 (OPP-2) — BYO-LLM consolidation harness protocol error.
export class ConsolidatorError extends FathomDbError {}

// G4 (Slice 35) — filter predicate construction error (non-allowlisted path).
export class InvalidFilterError extends FathomDbError {}

// Slice 20 — depth > 3 or other invalid argument (G5/G6).
export class InvalidArgumentError extends FathomDbError {}

// OPP-12 Phase-1 (0.8.19 Slice 10) — an illegal lifecycle `transition`/`purge`
// move. Parity-safe field names (S7): `fromState`/`toState` (never `from`, a
// reserved word in the Python peer), plus the legal target enumeration.
export interface IllegalTransitionPayload {
  fromState: string;
  toState: string;
  legal: string[];
}

export class IllegalTransitionError extends FathomDbError {
  readonly fromState: string;
  readonly toState: string;
  readonly legal: string[];
  constructor(message: string, payload: IllegalTransitionPayload) {
    super(message);
    this.fromState = payload.fromState;
    this.toState = payload.toState;
    this.legal = payload.legal;
  }
}

// OPP-12 Phase-1 (0.8.19 Slice 10) — a lifecycle verb addressed with a non-`l:`
// (`h:`/`p:`) id. Only the logical (`l:`) space is lifecycle-addressable.
export class NotLifecycleAddressableError extends FathomDbError {
  readonly idSpace: string;
  constructor(message: string, idSpace: string) {
    super(message);
    this.idSpace = idSpace;
  }
}

// 0.8.20 Slice 5b (R-20-E5) — an erasure verb (`purge` / `excise_source`)
// deleted its rows but could NOT complete the erasure AT REST: typically
// `wal_checkpoint(TRUNCATE)` stayed busy because a concurrent reader is pinning
// a WAL snapshot, so the erased bytes are still readable in the `-wal` file.
// Retryable — re-run the verb once the reader has finished. `stage` names the
// uncompleted step (`"wal_checkpoint"` / `"telemetry_redaction"`).
export class ErasureIncompleteError extends FathomDbError {
  readonly stage: string;
  readonly detail: string;
  constructor(message: string, stage: string, detail: string) {
    super(message);
    this.stage = stage;
    this.detail = detail;
  }
}

// 0.8.18 Slice 5 (#5 vector-equivalence probe) — the open-time self-check found a
// vector-equivalence divergence beyond the D4 floor, so every vector-dependent arm
// refuses at query time. The text-only/FTS-only path (`searchTextOnly`) stays
// serviceable. Leaf parity with `EmbedderIdentityMismatchError`.
export class VectorEquivalenceMismatchError extends FathomDbError {
  readonly reason: string;
  constructor(message: string, reason: string) {
    super(message);
    this.reason = reason;
  }
}

// Panic is a contract bug, not a typed engine outcome — intentionally
// NOT a FathomDbError subclass so callers that catch FathomDbError do
// not silently swallow it. Mirrors PyO3 PanicException in 11a.
export class FathomDbPanicError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "FathomDbPanicError";
  }
}

// ===== Typed-error rethrow ============================================

type ErrorCode =
  | "FDB_STORAGE"
  | "FDB_PROJECTION"
  | "FDB_VECTOR"
  | "FDB_EMBEDDER"
  | "FDB_EMBEDDER_NOT_CONFIGURED"
  | "FDB_KIND_NOT_VECTOR_INDEXED"
  | "FDB_EMBEDDER_DIMENSION_MISMATCH"
  | "FDB_SCHEDULER"
  | "FDB_OP_STORE"
  | "FDB_WRITE_VALIDATION"
  | "FDB_SCHEMA_VALIDATION"
  | "FDB_OVERLOADED"
  | "FDB_CLOSING"
  | "FDB_DATABASE_LOCKED"
  | "FDB_CORRUPTION"
  | "FDB_INCOMPATIBLE_SCHEMA_VERSION"
  | "FDB_MIGRATION"
  | "FDB_EMBEDDER_IDENTITY_MISMATCH"
  // G11 (Slice 15) — BYO-LLM extraction harness protocol error.
  | "FDB_EXTRACTOR"
  // 0.8.12 Slice 15 (OPP-2) — BYO-LLM consolidation harness protocol error.
  | "FDB_CONSOLIDATOR"
  // G4 (Slice 35) — filter predicate construction error.
  | "FDB_INVALID_FILTER"
  // Slice 20 — depth > 3 or invalid argument (G5/G6).
  | "FDB_INVALID_ARGUMENT"
  // 0.8.18 Slice 5 (#5 vector-equivalence probe) — query-time dense refusal.
  | "FDB_VECTOR_EQUIVALENCE_MISMATCH"
  // OPP-12 Phase-1 (0.8.19 Slice 10) — lifecycle-verb typed errors.
  | "FDB_ILLEGAL_TRANSITION"
  | "FDB_NOT_LIFECYCLE_ADDRESSABLE"
  // 0.8.20 Slice 5b (R-20-E5) — erasure verb could not finish at rest.
  | "FDB_ERASURE_INCOMPLETE"
  | "FDB_PANIC";

interface Envelope {
  code: ErrorCode;
  message: string;
  payload: Record<string, unknown> | null;
}

function parseEnvelope(raw: unknown): Envelope | null {
  if (typeof raw !== "string" || raw.length === 0 || raw[0] !== "{") {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (parsed === null || typeof parsed !== "object") {
    return null;
  }
  const obj = parsed as Record<string, unknown>;
  if (typeof obj.code !== "string" || typeof obj.message !== "string") {
    return null;
  }
  return {
    code: obj.code as ErrorCode,
    message: obj.message,
    payload: (obj.payload as Record<string, unknown> | null) ?? null,
  };
}

function build(envelope: Envelope): Error {
  const p = envelope.payload ?? {};
  switch (envelope.code) {
    case "FDB_STORAGE":
      return new StorageError(envelope.message);
    case "FDB_PROJECTION":
      return new ProjectionError(envelope.message);
    case "FDB_VECTOR":
      return new VectorError(envelope.message);
    case "FDB_EMBEDDER":
      return new EmbedderError(envelope.message);
    case "FDB_EMBEDDER_NOT_CONFIGURED":
      return new EmbedderNotConfiguredError(envelope.message);
    case "FDB_KIND_NOT_VECTOR_INDEXED":
      return new KindNotVectorIndexedError(envelope.message);
    case "FDB_EMBEDDER_DIMENSION_MISMATCH":
      return new EmbedderDimensionMismatchError({
        stored: Number(p.stored),
        supplied: Number(p.supplied),
      });
    case "FDB_SCHEDULER":
      return new SchedulerError(envelope.message);
    case "FDB_OP_STORE":
      return new OpStoreError(envelope.message);
    case "FDB_WRITE_VALIDATION":
      return new WriteValidationError(envelope.message);
    case "FDB_SCHEMA_VALIDATION":
      return new SchemaValidationError(envelope.message);
    case "FDB_OVERLOADED":
      return new OverloadedError(envelope.message);
    case "FDB_CLOSING":
      return new ClosingError(envelope.message);
    case "FDB_DATABASE_LOCKED":
      return new DatabaseLockedError({
        holderPid: typeof p.holderPid === "number" ? p.holderPid : undefined,
      });
    case "FDB_CORRUPTION":
      return new CorruptionError({
        kind: String(p.kind ?? ""),
        stage: String(p.stage ?? ""),
        recoveryHintCode: String(p.recoveryHintCode ?? ""),
        docAnchor: String(p.docAnchor ?? ""),
      });
    case "FDB_INCOMPATIBLE_SCHEMA_VERSION":
      return new IncompatibleSchemaVersionError(envelope.message);
    case "FDB_MIGRATION":
      return new MigrationError(envelope.message);
    case "FDB_EMBEDDER_IDENTITY_MISMATCH":
      return new EmbedderIdentityMismatchError({
        storedName: String(p.storedName ?? ""),
        storedRevision: String(p.storedRevision ?? ""),
        suppliedName: String(p.suppliedName ?? ""),
        suppliedRevision: String(p.suppliedRevision ?? ""),
      });
    case "FDB_EXTRACTOR":
      return new ExtractorError(envelope.message);
    case "FDB_CONSOLIDATOR":
      return new ConsolidatorError(envelope.message);
    case "FDB_INVALID_FILTER":
      return new InvalidFilterError(envelope.message);
    case "FDB_INVALID_ARGUMENT":
      return new InvalidArgumentError(envelope.message);
    case "FDB_VECTOR_EQUIVALENCE_MISMATCH":
      return new VectorEquivalenceMismatchError(
        envelope.message,
        String(p.reason ?? ""),
      );
    case "FDB_ILLEGAL_TRANSITION":
      return new IllegalTransitionError(envelope.message, {
        fromState: String(p.fromState ?? ""),
        toState: String(p.toState ?? ""),
        legal: Array.isArray(p.legal) ? p.legal.map((s) => String(s)) : [],
      });
    case "FDB_NOT_LIFECYCLE_ADDRESSABLE":
      return new NotLifecycleAddressableError(
        envelope.message,
        String(p.idSpace ?? ""),
      );
    case "FDB_ERASURE_INCOMPLETE":
      return new ErasureIncompleteError(
        envelope.message,
        String(p.stage ?? ""),
        String(p.detail ?? ""),
      );
    case "FDB_PANIC":
      return new FathomDbPanicError(envelope.message);
    default: {
      // Exhaustiveness — if ErrorCode union grows and switch arms
      // don't cover it, this branch becomes reachable and TS narrows
      // `envelope.code` to `never`, surfacing as a compile error.
      const _exhaustive: never = envelope.code;
      return new Error(`unrecognised error code: ${String(_exhaustive)}`);
    }
  }
}

/**
 * Re-raise a binding-thrown error as the corresponding TS leaf class.
 *
 * Errors not carrying the JSON envelope are rethrown unchanged so
 * non-binding errors (e.g. native panics from outside the catch
 * surface) reach the caller untouched.
 */
export function rethrowTyped(err: unknown): never {
  if (err instanceof Error) {
    const env = parseEnvelope(err.message);
    if (env !== null) {
      throw build(env);
    }
  }
  throw err;
}
