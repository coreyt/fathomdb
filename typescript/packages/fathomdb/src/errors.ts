const PREFIX = "FATHOMDB_";

export class FathomError extends Error {}
export class DatabaseLockedError extends FathomError {}
export class CompileError extends FathomError {}
export class InvalidWriteError extends FathomError {}
export class WriterRejectedError extends FathomError {}
export class SchemaError extends FathomError {}
export class SqliteError extends FathomError {}
export class IoError extends FathomError {}
export class BridgeError extends FathomError {}
export class CapabilityMissingError extends FathomError {}
export class BuilderValidationError extends FathomError {}

export function parseNativeJson(payload: string): Record<string, unknown> {
  try {
    return JSON.parse(payload) as Record<string, unknown>;
  } catch (error) {
    throw mapNativeError(error);
  }
}

export function callNative<T>(fn: () => T): T {
  try {
    return fn();
  } catch (error) {
    throw mapNativeError(error);
  }
}

export function mapNativeError(error: unknown): Error {
  if (!(error instanceof Error)) {
    return new FathomError(String(error));
  }
  const [prefix, message] = error.message.split("::", 2);
  if (!prefix.startsWith(PREFIX)) {
    return error;
  }
  const code = prefix.slice(PREFIX.length);
  switch (code) {
    case "DATABASE_LOCKED":
      return new DatabaseLockedError(message);
    case "COMPILE_ERROR":
      return new CompileError(message);
    case "INVALID_WRITE":
      return new InvalidWriteError(message);
    case "WRITER_REJECTED":
      return new WriterRejectedError(message);
    case "SCHEMA_ERROR":
      return new SchemaError(message);
    case "SQLITE_ERROR":
      return new SqliteError(message);
    case "IO_ERROR":
      return new IoError(message);
    case "BRIDGE_ERROR":
      return new BridgeError(message);
    case "CAPABILITY_MISSING":
      return new CapabilityMissingError(message);
    default:
      return new FathomError(message ?? error.message);
  }
}
