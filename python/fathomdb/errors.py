"""Error types for the fathomdb Python bindings."""

from ._fathomdb import (
    BridgeError,
    CapabilityMissingError,
    CompileError,
    FathomError,
    InvalidWriteError,
    IoError,
    SchemaError,
    SqliteError,
    WriterRejectedError,
)

class BuilderValidationError(ValueError):
    """Raised when a WriteRequestBuilder detects an invalid handle or reference."""

__all__ = [
    "BuilderValidationError",
    "BridgeError",
    "CapabilityMissingError",
    "CompileError",
    "FathomError",
    "InvalidWriteError",
    "IoError",
    "SchemaError",
    "SqliteError",
    "WriterRejectedError",
]
