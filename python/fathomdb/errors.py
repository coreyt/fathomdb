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
    pass

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
