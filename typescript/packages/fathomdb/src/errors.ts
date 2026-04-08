const PREFIX = "FATHOMDB_";

/** Base error class for all fathomdb errors. */
export class FathomError extends Error {}

/** Raised when the database file is locked by another process. */
export class DatabaseLockedError extends FathomError {}

/** Raised when a query or expression fails to compile. */
export class CompileError extends FathomError {}

/** Raised when a write request contains invalid data. */
export class InvalidWriteError extends FathomError {}

/** Raised when the write-ahead log writer rejects a transaction. */
export class WriterRejectedError extends FathomError {}

/** Raised when a schema operation fails (e.g. missing table or column). */
export class SchemaError extends FathomError {}

/** Raised when the underlying SQLite engine returns an error. */
export class SqliteError extends FathomError {}

/** Raised on file-system or I/O failures. */
export class IoError extends FathomError {}

/** Raised when the native bridge encounters an internal error. */
export class BridgeError extends FathomError {}

/** Raised when a required engine capability is not available. */
export class CapabilityMissingError extends FathomError {}

/** Raised when a {@link WriteRequestBuilder} detects an invalid handle or reference. */
export class BuilderValidationError extends FathomError {}

/**
 * Parse a JSON string returned by the native engine, mapping parse failures to typed errors.
 *
 * @param payload - Raw JSON string from the native layer.
 * @returns The parsed object.
 * @throws {FathomError} If the payload cannot be parsed (typically wrapping a native error).
 */
export function parseNativeJson(payload: string): Record<string, unknown> {
  try {
    return JSON.parse(payload) as Record<string, unknown>;
  } catch (error) {
    throw mapNativeError(error);
  }
}

/**
 * Map a native error into the appropriate typed {@link FathomError} subclass.
 *
 * Error messages from the native layer are prefixed with `FATHOMDB_<CODE>::`.
 * This function parses that prefix and returns the corresponding error class.
 *
 * @param error - The raw error thrown by the native binding.
 * @returns A typed error instance.
 */
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
