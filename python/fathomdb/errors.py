"""Error types for the fathomdb Python bindings."""

from ._fathomdb import (
    BridgeError,
    CapabilityMissingError,
    CompileError,
    DatabaseLockedError,
    FathomError,
    InvalidWriteError,
    IoError,
    SchemaError,
    SqliteError,
    WriterRejectedError,
    WriterTimedOutError,
)

class BuilderValidationError(ValueError):
    """Raised when a WriteRequestBuilder detects an invalid handle or reference."""

__all__ = [
    "BuilderValidationError",
    "BridgeError",
    "CapabilityMissingError",
    "CompileError",
    "DatabaseLockedError",
    "FathomError",
    "InvalidWriteError",
    "IoError",
    "SchemaError",
    "SqliteError",
    "WriterRejectedError",
    "WriterTimedOutError",
]
