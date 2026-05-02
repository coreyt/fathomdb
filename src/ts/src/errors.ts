// Single-rooted FathomDbError hierarchy for the TypeScript SDK.
//
// Layout owned by `dev/design/errors.md` § Binding-facing class matrix and
// `dev/design/bindings.md` § 3. Per-leaf payloads are typed object
// arguments; callers narrow with `instanceof`.

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
